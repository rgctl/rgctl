//! Dockerfile extraction plugin (FROM, COPY, RUN).

use rbuilder_plugin_api::Result;
use rbuilder_plugin_api::*;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

static FROM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^FROM\s+([^\s]+)(?:\s+AS\s+(\w+))?").unwrap());
static COPY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:COPY|ADD)\s+(--from=\S+\s+)?(\S+)\s+(\S+)").unwrap());
static RUN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^RUN\s+(.+)").unwrap());

/// Dockerfile language plugin.
pub struct DockerfilePlugin;

impl DockerfilePlugin {
    /// Create a new Dockerfile plugin instance.
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

impl LanguagePlugin for DockerfilePlugin {
    fn language_id(&self) -> &str {
        "dockerfile"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["dockerfile"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let file = file_path.to_string_lossy();
        let text = std::str::from_utf8(source).unwrap_or("");
        let mut symbols = Vec::new();

        for (line_no, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if let Some(cap) = FROM_RE.captures(trimmed) {
                let image = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                symbols.push(Symbol {
                    name: image,
                    symbol_type: SymbolType::Dependency,
                    qualified_name: cap.get(2).map(|m| m.as_str().to_string()),
                    location: Self::loc(&file, line_no + 1),
                    signature: Some(trimmed.to_string()),
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "kind": "from" }),
                });
            } else if let Some(cap) = COPY_RE.captures(trimmed) {
                let src = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
                symbols.push(Symbol {
                    name: src.clone(),
                    symbol_type: SymbolType::Import,
                    qualified_name: None,
                    location: Self::loc(&file, line_no + 1),
                    signature: Some(trimmed.to_string()),
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "kind": "copy", "dest": cap.get(3).map(|m| m.as_str()) }),
                });
            } else if let Some(cap) = RUN_RE.captures(trimmed) {
                let cmd = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                symbols.push(Symbol {
                    name: format!("run_{}", line_no + 1),
                    symbol_type: SymbolType::BuildStep,
                    qualified_name: None,
                    location: Self::loc(&file, line_no + 1),
                    signature: Some(cmd),
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "kind": "run" }),
                });
            }
        }
        Ok(symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let file = file_path.to_string_lossy();
        let text = std::str::from_utf8(source).unwrap_or("");
        let mut relations = Vec::new();
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Dockerfile");

        for sym in symbols {
            if matches!(sym.symbol_type, SymbolType::Dependency | SymbolType::Import) {
                relations.push(Relation {
                    from: file_name.to_string(),
                    to: sym.name.clone(),
                    relation_type: RelationType::Uses,
                    location: sym.location.clone(),
                    metadata: serde_json::json!({}),
                });
            }
        }
        let _ = (file, text, symbols);
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
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        name.eq_ignore_ascii_case("dockerfile") || name.ends_with(".dockerfile")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dockerfile_base_image_extraction() {
        let plugin = DockerfilePlugin::new().unwrap();
        let source = b"FROM rust:1.75\nRUN cargo build";
        let symbols = plugin
            .extract_symbols(Path::new("Dockerfile"), source)
            .unwrap();
        let deps: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Dependency)
            .collect();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "rust:1.75");
    }
}
