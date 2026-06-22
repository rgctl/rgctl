//! Runtime language configuration types for generic plugins.

use rbuilder_plugin_api::SymbolType;

/// A regex pattern for symbol extraction.
#[derive(Debug, Clone, Copy)]
pub struct RegexPatternConfig {
    /// Regex pattern string
    pub pattern: &'static str,
    /// Symbol type to assign on match
    pub symbol_type: SymbolType,
}

/// Language configuration embedded in each `rbuilder-lang-*` crate.
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
    /// Optional supplemental regex patterns (regex handler, or hybrid tree-sitter)
    pub regex_patterns: Option<&'static [RegexPatternConfig]>,
}

#[cfg(test)]
pub(crate) mod test_configs {
    use super::*;

    pub static C: LanguageConfig = LanguageConfig {
        id: "c",
        extensions: &["c", "h"],
        function_kinds: &["function_definition", "function_declarator"],
        class_kinds: &["struct_specifier", "enum_specifier", "type_definition"],
        enable_complexity: true,
        enable_type_inference: false,
        regex_patterns: None,
    };

    static KOTLIN_PATTERNS: &[RegexPatternConfig] = &[
        RegexPatternConfig {
            pattern: r"(?m)^\s*fun\s+([A-Za-z_][A-Za-z0-9_]*)",
            symbol_type: SymbolType::Function,
        },
        RegexPatternConfig {
            pattern: r"(?m)^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)",
            symbol_type: SymbolType::Class,
        },
        RegexPatternConfig {
            pattern: r"(?m)^\s*object\s+([A-Za-z_][A-Za-z0-9_]*)",
            symbol_type: SymbolType::Class,
        },
    ];

    pub static KOTLIN: LanguageConfig = LanguageConfig {
        id: "kotlin",
        extensions: &["kt", "kts"],
        function_kinds: &["fun"],
        class_kinds: &["class", "object", "interface", "data class"],
        enable_complexity: false,
        enable_type_inference: false,
        regex_patterns: Some(KOTLIN_PATTERNS),
    };

    static CSHARP_PATTERNS: &[RegexPatternConfig] = &[
        RegexPatternConfig {
            pattern: r"(?m)\bclass\s+([A-Za-z_][A-Za-z0-9_]*)",
            symbol_type: SymbolType::Class,
        },
        RegexPatternConfig {
            pattern: r"(?m)\b(?:public|private|protected|internal|static|async|\s)+[\w<>\[\]?]+\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(",
            symbol_type: SymbolType::Function,
        },
    ];

    pub static CSHARP: LanguageConfig = LanguageConfig {
        id: "csharp",
        extensions: &["cs"],
        function_kinds: &["method"],
        class_kinds: &["class", "interface", "struct", "enum"],
        enable_complexity: false,
        enable_type_inference: false,
        regex_patterns: Some(CSHARP_PATTERNS),
    };
}
