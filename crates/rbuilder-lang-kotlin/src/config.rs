//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;
use rbuilder_lang_runtime::RegexPatternConfig;
use rbuilder_plugin_api::SymbolType;

static REGEX_PATTERNS: &[RegexPatternConfig] = &[
    RegexPatternConfig {
        pattern: "(?m)^\\s*fun\\s+([A-Za-z_][A-Za-z0-9_]*)",
        symbol_type: SymbolType::Function,
    },
    RegexPatternConfig {
        pattern: "(?m)^\\s*class\\s+([A-Za-z_][A-Za-z0-9_]*)",
        symbol_type: SymbolType::Class,
    },
    RegexPatternConfig {
        pattern: "(?m)^\\s*object\\s+([A-Za-z_][A-Za-z0-9_]*)",
        symbol_type: SymbolType::Class,
    },
    RegexPatternConfig {
        pattern: "(?m)^\\s*interface\\s+([A-Za-z_][A-Za-z0-9_]*)",
        symbol_type: SymbolType::Interface,
    },
    RegexPatternConfig {
        pattern: "(?m)^\\s*data\\s+class\\s+([A-Za-z_][A-Za-z0-9_]*)",
        symbol_type: SymbolType::Class,
    },
];

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "kotlin",
    extensions: &["kt", "kts"],
    function_kinds: &["fun"],
    class_kinds: &["class", "object", "interface", "data class"],
    enable_complexity: false,
    enable_type_inference: false,
    regex_patterns: Some(REGEX_PATTERNS),
};
