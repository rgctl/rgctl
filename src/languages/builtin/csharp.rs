//! C# language plugin
//!
//! Task 3.2.5: Extract classes, interfaces, and methods from C# source.

use crate::error::Result;
use crate::languages::plugin_trait::*;
use regex::Regex;
use std::path::Path;

/// C# language plugin (regex-based extraction).
pub struct CSharpPlugin;

impl CSharpPlugin {
    /// Create a new C# plugin.
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl LanguagePlugin for CSharpPlugin {
    fn language_id(&self) -> &str {
        "csharp"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["cs"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let file = file_path.to_string_lossy().to_string();
        let text = String::from_utf8_lossy(source);
        let mut symbols = Vec::new();

        let patterns = [
            (Regex::new(r"(?m)\bclass\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap(), SymbolType::Class),
            (Regex::new(r"(?m)\binterface\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap(), SymbolType::Interface),
            (Regex::new(r"(?m)\bstruct\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap(), SymbolType::Struct),
            (Regex::new(r"(?m)\benum\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap(), SymbolType::Enum),
            (
                Regex::new(r"(?m)\b(?:public|private|protected|internal|static|async|\s)+[\w<>\[\]?]+\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap(),
                SymbolType::Function,
            ),
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
                        metadata: serde_json::json!({ "language": "csharp" }),
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
    fn test_extract_csharp_symbols() {
        let source = br#"
namespace Demo {
    public class UserService {
        public async Task<string> AuthenticateAsync(string token) {
            return token;
        }
    }
}
"#;
        let plugin = CSharpPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("UserService.cs"), source).unwrap();
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "AuthenticateAsync"));
    }
}
