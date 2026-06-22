//! Generic language plugins with per-crate static configuration.

pub mod config;
pub mod regex_extract;
pub mod regex_plugin;
pub mod tree_sitter_plugin;

pub use config::{LanguageConfig, RegexPatternConfig};
pub use regex_plugin::RegexLanguagePlugin;
pub use tree_sitter_plugin::TreeSitterLanguagePlugin;

/// Create a tree-sitter plugin from a static config and grammar loader.
pub fn tree_sitter_plugin(
    config: &'static LanguageConfig,
    grammar: fn() -> tree_sitter::Language,
) -> TreeSitterLanguagePlugin {
    TreeSitterLanguagePlugin::from_config(config, grammar)
}

/// Create a regex plugin from a static config.
pub fn regex_plugin(config: &'static LanguageConfig) -> RegexLanguagePlugin {
    RegexLanguagePlugin::from_config(config)
}
