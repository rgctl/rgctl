//! Regex-based language plugin driven by a static [`LanguageConfig`].

use crate::config::LanguageConfig;
use crate::regex_extract::extract_regex_symbols;
use rbuilder_plugin_api::Result;
use rbuilder_plugin_api::*;
use std::path::Path;

/// Generic regex language plugin configured via a static [`LanguageConfig`].
pub struct RegexLanguagePlugin {
    config: &'static LanguageConfig,
}

impl RegexLanguagePlugin {
    /// Create a plugin from a static config.
    pub fn from_config(config: &'static LanguageConfig) -> Self {
        Self { config }
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

    #[test]
    fn test_regex_kotlin_plugin() {
        use crate::config::test_configs::KOTLIN;

        let plugin = RegexLanguagePlugin::from_config(&KOTLIN);
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

    #[test]
    fn test_regex_csharp_plugin() {
        use crate::config::test_configs::CSHARP;

        let plugin = RegexLanguagePlugin::from_config(&CSHARP);
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
