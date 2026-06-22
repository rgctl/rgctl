//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "vhdl",
    extensions: &["vhd", "vhdl"],
    function_kinds: &["function_declaration", "procedure_declaration"],
    class_kinds: &["entity_declaration", "package_declaration"],
    enable_complexity: false,
    enable_type_inference: false,
    regex_patterns: None,
};
