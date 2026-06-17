//! GitHub Actions workflow extraction plugin.

use crate::error::Result;
use crate::languages::plugin_trait::*;
use serde_yaml::Value;
use std::path::Path;

/// GitHub Actions CI plugin.
pub struct GithubActionsPlugin;

impl GithubActionsPlugin {
/// Create a new GitHub Actions plugin instance.
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

    fn parse_jobs(value: &Value, file: &str) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let Some(jobs) = value.get("jobs").and_then(|j| j.as_mapping()) else {
            return (symbols, relations);
        };

        for (job_key, job_val) in jobs {
            let Some(name) = job_key.as_str() else {
                continue;
            };
            symbols.push(Symbol {
                name: name.to_string(),
                symbol_type: SymbolType::Job,
                qualified_name: None,
                location: Self::loc(file, 1),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: serde_json::json!({ "ci": "github-actions" }),
            });

            if let Some(steps) = job_val.get("steps").and_then(|s| s.as_sequence()) {
                for (i, step) in steps.iter().enumerate() {
                    let step_name = step
                        .get("name")
                        .and_then(|n| n.as_str())
                        .or_else(|| step.get("run").and_then(|r| r.as_str()))
                        .or_else(|| step.get("uses").and_then(|u| u.as_str()))
                        .unwrap_or("step");
                    symbols.push(Symbol {
                        name: format!("{name}::step_{i}"),
                        symbol_type: SymbolType::BuildStep,
                        qualified_name: Some(name.to_string()),
                        location: Self::loc(file, 1),
                        signature: Some(step_name.to_string()),
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "job": name }),
                    });
                }
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
                        location: Self::loc(file, 1),
                        metadata: serde_json::json!({}),
                    });
                }
            }
        }
        (symbols, relations)
    }
}

impl LanguagePlugin for GithubActionsPlugin {
    fn language_id(&self) -> &str {
        "github_actions"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec![]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        if !Self::is_github_actions_path(file_path) {
            return Ok(vec![]);
        }
        let file = file_path.to_string_lossy();
        let text = std::str::from_utf8(source).unwrap_or("");
        let value: Value = serde_yaml::from_str(text).unwrap_or(Value::Null);
        Ok(Self::parse_jobs(&value, &file).0)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        if !Self::is_github_actions_path(file_path) {
            return Ok(vec![]);
        }
        let file = file_path.to_string_lossy();
        let text = std::str::from_utf8(source).unwrap_or("");
        let value: Value = serde_yaml::from_str(text).unwrap_or(Value::Null);
        Ok(Self::parse_jobs(&value, &file).1)
    }

    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
    }
}

impl GithubActionsPlugin {
    fn is_github_actions_path(path: &Path) -> bool {
        path.to_string_lossy()
            .replace('\\', "/")
            .contains(".github/workflows/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_actions_job_extraction() {
        let plugin = GithubActionsPlugin::new().unwrap();
        let source = br#"name: CI
on: [push]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test
  build:
    needs: test
    runs-on: ubuntu-latest
    steps:
      - run: cargo build --release
"#;
        let path = Path::new(".github/workflows/ci.yml");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let jobs: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Job)
            .collect();
        assert_eq!(jobs.len(), 2);
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(relations.iter().any(|r| r.relation_type == RelationType::DependsOn));
    }
}
