//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "ruby",
    extensions: &["rb", "rake"],
    function_kinds: &["method", "singleton_method"],
    class_kinds: &["class", "module"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
