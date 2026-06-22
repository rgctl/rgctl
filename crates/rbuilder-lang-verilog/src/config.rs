//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "verilog",
    extensions: &["v", "sv"],
    function_kinds: &["function_declaration", "task_declaration"],
    class_kinds: &["module_declaration", "class_declaration"],
    enable_complexity: false,
    enable_type_inference: false,
    regex_patterns: None,
};
