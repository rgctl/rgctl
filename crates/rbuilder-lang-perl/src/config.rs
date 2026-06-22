//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "perl",
    extensions: &["pl", "pm"],
    function_kinds: &[
        "subroutine_declaration_statement",
        "method_declaration_statement",
    ],
    class_kinds: &["package_statement"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
