//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "scala",
    extensions: &["scala", "sc"],
    function_kinds: &[
        "function_definition",
        "function_declaration",
        "val_definition",
        "object_definition",
    ],
    class_kinds: &[
        "class_definition",
        "trait_definition",
        "enum_definition",
        "type_definition",
    ],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
