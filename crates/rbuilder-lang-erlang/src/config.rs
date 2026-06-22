//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "erlang",
    extensions: &["erl", "hrl"],
    function_kinds: &["function_clause", "fun_decl", "fun_expr"],
    class_kinds: &["module", "record_declaration"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
