//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "haskell",
    extensions: &["hs", "lhs"],
    function_kinds: &["function", "bind", "value_definition", "signature"],
    class_kinds: &[
        "data_type",
        "class_decl",
        "newtype",
        "type_synomym",
        "instance",
    ],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
