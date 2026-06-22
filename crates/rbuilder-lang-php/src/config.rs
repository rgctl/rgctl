//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "php",
    extensions: &["php"],
    function_kinds: &["function_definition", "method_declaration"],
    class_kinds: &[
        "class_declaration",
        "interface_declaration",
        "trait_declaration",
    ],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
