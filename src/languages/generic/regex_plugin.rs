//! Regex-based language plugin driven by `languages.toml`

use crate::error::Result;
use crate::languages::generic::config::{get_language_config, LanguageConfig};
use crate::languages::plugin_trait::*;
use regex::Regex;
use std::path::Path;

/// Generic regex language plugin configured via `languages.toml`.
pub struct RegexLanguagePlugin {
    config: &'static LanguageConfig,
    patterns: Vec<(Regex, SymbolType)>,
}

impl RegexLanguagePlugin {
    /// Create a plugin for the given language ID.
    pub fn new(language_id: &str) -> Result<Self> {
        let config = get_language_config(language_id).ok_or_else(|| {
            crate::error::Error::PluginError(format!("Unknown regex language: {language_id}"))
        })?;
        let regex_patterns = config.regex_patterns.ok_or_else(|| {
            crate::error::Error::PluginError(format!("No regex patterns for: {language_id}"))
        })?;
        let patterns = regex_patterns
            .iter()
            .map(|p| {
                Ok((
                    Regex::new(p.pattern).map_err(|e| {
                        crate::error::Error::PluginError(format!(
                            "Invalid regex for {}: {e}",
                            language_id
                        ))
                    })?,
                    p.symbol_type,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { config, patterns })
    }
}

impl LanguagePlugin for RegexLanguagePlugin {
    fn language_id(&self) -> &str {
        self.config.id
    }

    fn file_extensions(&self) -> Vec<&str> {
        self.config.extensions.to_vec()
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let file = file_path.to_string_lossy().to_string();
        let text = String::from_utf8_lossy(source);
        let mut symbols = Vec::new();

        for (line_no, line) in text.lines().enumerate() {
            for (re, symbol_type) in &self.patterns {
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
                        metadata: serde_json::json!({
                            "language": self.config.id,
                            "extractor": "regex"
                        }),
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

    #[cfg(feature = "lang-kotlin")]
    #[test]
    fn test_regex_kotlin_plugin() {
        let plugin = RegexLanguagePlugin::new("kotlin").unwrap();
        let source = br#"
class UserService {
    fun authenticate(token: String): String = token
}
object Config
"#;
        let symbols = plugin.extract_symbols(Path::new("App.kt"), source).unwrap();
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "authenticate"));
        assert!(symbols.iter().any(|s| s.name == "Config"));
    }

    #[cfg(feature = "lang-csharp")]
    #[test]
    fn test_regex_csharp_plugin() {
        let plugin = RegexLanguagePlugin::new("csharp").unwrap();
        let source = br#"
public class UserService {
    public async Task<string> AuthenticateAsync(string token) { return token; }
}
"#;
        let symbols = plugin
            .extract_symbols(Path::new("UserService.cs"), source)
            .unwrap();
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "AuthenticateAsync"));
    }
}
