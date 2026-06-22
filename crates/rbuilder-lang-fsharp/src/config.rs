//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "fsharp",
    extensions: &["fs", "fsx"],
    function_kinds: &["function_or_value_defn", "member_definition"],
    class_kinds: &["class", "enum_type_defn"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
