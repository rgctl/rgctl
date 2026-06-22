//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "nim",
    extensions: &["nim", "nims"],
    function_kinds: &[
        "proc_declaration",
        "func_declaration",
        "method_declaration",
        "iterator_declaration",
        "converter_declaration",
    ],
    class_kinds: &["type_declaration", "enum_declaration", "object_declaration"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
