//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;
use rbuilder_lang_runtime::RegexPatternConfig;
use rbuilder_plugin_api::SymbolType;

static REGEX_PATTERNS: &[RegexPatternConfig] = &[RegexPatternConfig {
    pattern: "(?m)^([A-Za-z_][\\w]*):",
    symbol_type: SymbolType::Function,
}];

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "assembly",
    extensions: &["asm", "s", "nasm"],
    function_kinds: &["label"],
    class_kinds: &[],
    enable_complexity: false,
    enable_type_inference: false,
    regex_patterns: Some(REGEX_PATTERNS),
};
