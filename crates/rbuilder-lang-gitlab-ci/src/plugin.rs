//! GitLab CI pipeline extraction plugin.

use rbuilder_plugin_api::Result;
use rbuilder_plugin_api::*;
use serde_yaml::Value;
use std::path::Path;

/// GitLab CI plugin.
pub struct GitlabCiPlugin;

impl GitlabCiPlugin {
    /// Create a new GitLab CI plugin instance.
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn loc(file: &str, line: usize) -> SourceLocation {
        SourceLocation {
            file: file.to_string(),
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: 0,
        }
    }
}

impl LanguagePlugin for GitlabCiPlugin {
    fn language_id(&self) -> &str {
        "gitlab_ci"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec![]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        if !Self::is_gitlab_ci_file(file_path) {
            return Ok(vec![]);
        }
        let file = file_path.to_string_lossy();
        let text = std::str::from_utf8(source).unwrap_or("");
        let value: Value = serde_yaml::from_str(text).unwrap_or(Value::Null);
        let mut symbols = Vec::new();

        if let Some(mapping) = value.as_mapping() {
            for (key, job_val) in mapping {
                let Some(name) = key.as_str() else {
                    continue;
                };
                if matches!(
                    name,
                    "stages" | "variables" | "include" | "default" | "workflow"
                ) {
                    continue;
                }
                if !job_val.is_mapping() {
                    continue;
                }
                symbols.push(Symbol {
                    name: name.to_string(),
                    symbol_type: SymbolType::Job,
                    qualified_name: None,
                    location: Self::loc(&file, 1),
                    signature: job_val
                        .get("script")
                        .and_then(|s| s.as_sequence())
                        .map(|seq| {
                            seq.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join("; ")
                        }),
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "ci": "gitlab" }),
                });

                if let Some(script) = job_val.get("script").and_then(|s| s.as_sequence()) {
                    for (i, step) in script.iter().enumerate() {
                        if let Some(cmd) = step.as_str() {
                            symbols.push(Symbol {
                                name: format!("{name}::step_{i}"),
                                symbol_type: SymbolType::BuildStep,
                                qualified_name: Some(name.to_string()),
                                location: Self::loc(&file, 1),
                                signature: Some(cmd.to_string()),
                                return_type: None,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata: serde_json::json!({}),
                            });
                        }
                    }
                }
            }
        }
        Ok(symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        if !Self::is_gitlab_ci_file(file_path) {
            return Ok(vec![]);
        }
        let file = file_path.to_string_lossy();
        let text = std::str::from_utf8(source).unwrap_or("");
        let value: Value = serde_yaml::from_str(text).unwrap_or(Value::Null);
        let mut relations = Vec::new();

        if let Some(mapping) = value.as_mapping() {
            for (key, job_val) in mapping {
                let Some(name) = key.as_str() else {
                    continue;
                };
                if !job_val.is_mapping() {
                    continue;
                }
                if let Some(needs) = job_val.get("needs") {
                    let deps: Vec<String> = match needs {
                        Value::String(s) => vec![s.clone()],
                        Value::Sequence(seq) => seq
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect(),
                        _ => vec![],
                    };
                    for dep in deps {
                        relations.push(Relation {
                            from: name.to_string(),
                            to: dep,
                            relation_type: RelationType::DependsOn,
                            location: Self::loc(&file, 1),
                            metadata: serde_json::json!({}),
                        });
                    }
                }
            }
        }
        Ok(relations)
    }

    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
    }

    fn matches_path(&self, path: &str) -> bool {
        Self::is_gitlab_ci_file(Path::new(path))
    }
}

impl GitlabCiPlugin {
    fn is_gitlab_ci_file(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == ".gitlab-ci.yml" || n.ends_with(".gitlab-ci.yml"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gitlab_ci_job_extraction() {
        let plugin = GitlabCiPlugin::new().unwrap();
        let source = br#"stages: [test, build]
test_job:
  stage: test
  script: [cargo test]
build_job:
  stage: build
  needs: [test_job]
  script: [cargo build]
"#;
        let symbols = plugin
            .extract_symbols(Path::new(".gitlab-ci.yml"), source)
            .unwrap();
        let jobs: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Job)
            .collect();
        assert_eq!(jobs.len(), 2);
    }
}
