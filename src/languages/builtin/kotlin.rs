//! Kotlin language plugin
//!
//! Task 3.2.4: Extract functions, classes, and objects from Kotlin source.

use crate::error::Result;
use crate::languages::plugin_trait::*;
use regex::Regex;
use std::path::Path;

/// Kotlin language plugin (regex-based extraction).
pub struct KotlinPlugin;

impl KotlinPlugin {
    /// Create a new Kotlin plugin.
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl LanguagePlugin for KotlinPlugin {
    fn language_id(&self) -> &str {
        "kotlin"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["kt", "kts"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let file = file_path.to_string_lossy().to_string();
        let text = String::from_utf8_lossy(source);
        let mut symbols = Vec::new();

        let patterns = [
            (Regex::new(r"(?m)^\s*fun\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap(), SymbolType::Function),
            (Regex::new(r"(?m)^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap(), SymbolType::Class),
            (Regex::new(r"(?m)^\s*object\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap(), SymbolType::Class),
            (Regex::new(r"(?m)^\s*interface\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap(), SymbolType::Interface),
            (Regex::new(r"(?m)^\s*data\s+class\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap(), SymbolType::Class),
        ];

        for (line_no, line) in text.lines().enumerate() {
            for (re, symbol_type) in &patterns {
                if let Some(cap) = re.captures(line) {
                    symbols.push(Symbol {
                        name: cap[1].to_string(),
                        symbol_type: *symbol_type,
                        qualified_name: None,
                        location: SourceLocation {
                            file: file.clone(),
                            start_line: line_no + 1,
                            end_line: line_no + 1,
                            start_column: 0,
                            end_column: 0,
                        },
                        signature: Some(line.trim().to_string()),
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "kotlin" }),
                    });
                }
            }
        }
        Ok(symbols)
    }

    fn extract_relations(
        &self,
        _file_path: &Path,
        _source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        Ok(vec![])
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
    use std::path::Path;

    #[test]
    fn test_extract_kotlin_symbols() {
        let source = br#"
class UserService {
    fun authenticate(token: String): String = token
}
object Config
"#;
        let plugin = KotlinPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("App.kt"), source).unwrap();
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "authenticate"));
        assert!(symbols.iter().any(|s| s.name == "Config"));
    }
}
