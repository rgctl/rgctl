//! Runtime language configuration types (populated from build-time generated configs)

use crate::languages::plugin_trait::SymbolType;

/// A regex pattern for symbol extraction.
#[derive(Debug, Clone, Copy)]
pub struct RegexPatternConfig {
    /// Regex pattern string
    pub pattern: &'static str,
    /// Symbol type to assign on match
    pub symbol_type: SymbolType,
}

/// Language configuration embedded at build time from `languages.toml`.
#[derive(Debug, Clone, Copy)]
pub struct LanguageConfig {
    /// Language identifier (e.g. `"rust"`)
    pub id: &'static str,
    /// Supported file extensions
    pub extensions: &'static [&'static str],
    /// Tree-sitter node kinds treated as functions
    pub function_kinds: &'static [&'static str],
    /// Tree-sitter node kinds treated as classes/types
    pub class_kinds: &'static [&'static str],
    /// Whether to calculate complexity metrics
    pub enable_complexity: bool,
    /// Whether type inference is enabled
    pub enable_type_inference: bool,
    /// Regex patterns (regex handler only)
    pub regex_patterns: Option<&'static [RegexPatternConfig]>,
}

/// Build-time generated language configs.
pub mod configs {
    #![allow(missing_docs)]
    include!(concat!(env!("OUT_DIR"), "/generated_lang_configs.rs"));
}

pub use configs::get_language_config;
