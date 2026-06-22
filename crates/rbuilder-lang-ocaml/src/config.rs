//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "ocaml",
    extensions: &["ml"],
    function_kinds: &["value_definition", "method_definition", "let_binding"],
    class_kinds: &["module_definition", "class_definition", "type_definition"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
