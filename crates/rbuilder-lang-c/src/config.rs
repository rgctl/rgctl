//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "c",
    extensions: &["c", "h"],
    function_kinds: &["function_definition", "function_declarator"],
    class_kinds: &["struct_specifier", "enum_specifier", "type_definition"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
