//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;
use rbuilder_lang_runtime::RegexPatternConfig;
use rbuilder_plugin_api::SymbolType;

static REGEX_PATTERNS: &[RegexPatternConfig] = &[
    RegexPatternConfig { pattern: "(?m)\\bclass\\s+([A-Za-z_][A-Za-z0-9_]*)", symbol_type: SymbolType::Class },
    RegexPatternConfig { pattern: "(?m)\\binterface\\s+([A-Za-z_][A-Za-z0-9_]*)", symbol_type: SymbolType::Interface },
    RegexPatternConfig { pattern: "(?m)\\bstruct\\s+([A-Za-z_][A-Za-z0-9_]*)", symbol_type: SymbolType::Struct },
    RegexPatternConfig { pattern: "(?m)\\benum\\s+([A-Za-z_][A-Za-z0-9_]*)", symbol_type: SymbolType::Enum },
    RegexPatternConfig { pattern: "(?m)\\b(?:public|private|protected|internal|static|async|\\s)+[\\w<>\\[\\]?]+\\s+([A-Za-z_][A-Za-z0-9_]*)\\s*\\(", symbol_type: SymbolType::Function },
];

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "csharp",
    extensions: &["cs"],
    function_kinds: &["method"],
    class_kinds: &["class", "interface", "struct", "enum"],
    enable_complexity: false,
    enable_type_inference: false,
    regex_patterns: Some(REGEX_PATTERNS),
};
