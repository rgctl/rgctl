//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "pascal",
    extensions: &["pas", "pp", "dpr"],
    function_kinds: &["declProc"],
    class_kinds: &["declClass", "declEnum"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
