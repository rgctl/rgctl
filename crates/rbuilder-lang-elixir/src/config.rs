//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;
use rbuilder_lang_runtime::RegexPatternConfig;
use rbuilder_plugin_api::SymbolType;

static REGEX_PATTERNS: &[RegexPatternConfig] = &[
    RegexPatternConfig {
        pattern: "(?m)^\\s*defmodule\\s+([A-Za-z_.][\\w.]*)",
        symbol_type: SymbolType::Class,
    },
    RegexPatternConfig {
        pattern: "(?m)^\\s*def(?:p|macro|guard|delegate)?\\s+([A-Za-z_][\\w?!]*)",
        symbol_type: SymbolType::Function,
    },
];

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "elixir",
    extensions: &["ex", "exs"],
    function_kinds: &["anonymous_function", "stab_clause"],
    class_kinds: &["struct"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: Some(REGEX_PATTERNS),
};
