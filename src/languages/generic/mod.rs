//! Generic language plugins driven by `languages.toml` (Phase 7)

pub mod config;
pub mod regex_plugin;
pub mod tree_sitter_plugin;

pub use regex_plugin::RegexLanguagePlugin;
pub use tree_sitter_plugin::TreeSitterLanguagePlugin;
