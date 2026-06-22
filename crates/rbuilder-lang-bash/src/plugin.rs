//! Bash shell script extraction plugin (tree-sitter).

use rbuilder_plugin_api::Result;
use rbuilder_plugin_api::*;
use rbuilder_plugin_helpers::tree_sitter::{extract_symbols_by_kinds, parse_source};
use std::path::Path;

/// Bash language plugin using tree-sitter-bash.
pub struct BashPlugin;

impl BashPlugin {
    /// Create a new bash plugin instance.
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn extract_symbols_inner(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree = parse_source(source, file_path, tree_sitter_bash::LANGUAGE.into())?;
        extract_symbols_by_kinds(&tree, source, file_path, &["function_definition"], &[])
    }
}

impl LanguagePlugin for BashPlugin {
    fn language_id(&self) -> &str {
        "bash"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["sh", "bash"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        {
            return Some(tree_sitter_bash::LANGUAGE.into());
        }
        #[allow(unreachable_code)]
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        self.extract_symbols_inner(file_path, source)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let file = file_path.to_string_lossy();
        let text = std::str::from_utf8(source).unwrap_or("");
        let mut relations = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("source ") {
                let target = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches('"');
                if !target.is_empty() {
                    relations.push(Relation {
                        from: file_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("script")
                            .to_string(),
                        to: target.to_string(),
                        relation_type: RelationType::Uses,
                        location: SourceLocation {
                            file: file.to_string(),
                            start_line: 1,
                            end_line: 1,
                            start_column: 0,
                            end_column: 0,
                        },
                        metadata: serde_json::json!({ "kind": "source" }),
                    });
                }
            }
        }
        let _ = source;
        Ok(relations)
    }

    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_function_extraction() {
        let plugin = BashPlugin::new().unwrap();
        let source = b"deploy() {\n  echo 'Deploying...'\n}";
        let symbols = plugin
            .extract_symbols(Path::new("deploy.sh"), source)
            .unwrap();
        assert!(!symbols.is_empty());
        assert_eq!(symbols[0].name, "deploy");
    }
}
