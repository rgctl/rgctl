//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "fortran",
    extensions: &["f", "f90", "f03", "f08"],
    function_kinds: &["function_statement", "subroutine_statement"],
    class_kinds: &["derived_type_definition", "module_statement"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
