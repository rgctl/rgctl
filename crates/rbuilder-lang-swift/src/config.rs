//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "swift",
    extensions: &["swift"],
    function_kinds: &["function_declaration", "init_declaration", "function"],
    class_kinds: &[
        "class_declaration",
        "protocol_declaration",
        "enum_declaration",
        "struct_declaration",
    ],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
