//! Regex-based language plugin driven by `languages.toml`

use crate::error::Result;
use crate::languages::generic::config::{get_language_config, LanguageConfig};
use crate::languages::generic::regex_extract::extract_regex_symbols;
use crate::languages::plugin_trait::*;
use std::path::Path;

/// Generic regex language plugin configured via `languages.toml`.
pub struct RegexLanguagePlugin {
    config: &'static LanguageConfig,
}

impl RegexLanguagePlugin {
    /// Create a plugin for the given language ID.
    pub fn new(language_id: &str) -> Result<Self> {
        let config = get_language_config(language_id).ok_or_else(|| {
            crate::error::Error::PluginError(format!("Unknown regex language: {language_id}"))
        })?;
        if config.regex_patterns.is_none() {
            return Err(crate::error::Error::PluginError(format!(
                "No regex patterns for: {language_id}"
            )));
        }
        Ok(Self { config })
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
        extract_regex_symbols(
            file_path,
            source,
            self.config.regex_patterns.expect("validated at init"),
            "regex",
        )
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
