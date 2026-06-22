//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "r",
    extensions: &["r", "R"],
    function_kinds: &[
        "binary_operator",
        "function_definition",
        "function",
        "assignment",
    ],
    class_kinds: &["namespace_definition"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
