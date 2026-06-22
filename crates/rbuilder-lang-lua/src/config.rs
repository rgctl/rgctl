//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "lua",
    extensions: &["lua"],
    function_kinds: &[
        "function_declaration",
        "function_definition",
        "function",
        "local_function",
    ],
    class_kinds: &[],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
