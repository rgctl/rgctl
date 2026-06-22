//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "crystal",
    extensions: &["cr"],
    function_kinds: &["def", "fun_def", "method_def"],
    class_kinds: &["class_def", "module_def", "struct_def"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
