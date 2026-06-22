//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;
use rbuilder_lang_runtime::RegexPatternConfig;
use rbuilder_plugin_api::SymbolType;

static REGEX_PATTERNS: &[RegexPatternConfig] = &[RegexPatternConfig {
    pattern: "(?m)^\\s*macro\\s+([A-Za-z_][\\w!']*)",
    symbol_type: SymbolType::Macro,
}];

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "julia",
    extensions: &["jl"],
    function_kinds: &["function_definition", "short_function_definition"],
    class_kinds: &[
        "struct_definition",
        "module_definition",
        "abstract_definition",
    ],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: Some(REGEX_PATTERNS),
};
