//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;
use rbuilder_lang_runtime::RegexPatternConfig;
use rbuilder_plugin_api::SymbolType;

static REGEX_PATTERNS: &[RegexPatternConfig] = &[RegexPatternConfig {
    pattern: "(?m)^\\s*([A-Z0-9][A-Z0-9-]*)\\s*\\.",
    symbol_type: SymbolType::Function,
}];

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "cobol",
    extensions: &["cob", "cbl", "cpy"],
    function_kinds: &["PARAGRAPH", "PROCEDURE"],
    class_kinds: &[],
    enable_complexity: false,
    enable_type_inference: false,
    regex_patterns: Some(REGEX_PATTERNS),
};
