//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "dart",
    extensions: &["dart"],
    function_kinds: &[
        "function_signature",
        "method_signature",
        "method_declaration",
        "getter_signature",
        "setter_signature",
        "constructor_signature",
    ],
    class_kinds: &[
        "class_definition",
        "enum_declaration",
        "extension_declaration",
        "mixin_declaration",
        "typedef",
    ],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
