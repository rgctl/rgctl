//! Static language configuration for this plugin crate.

use rbuilder_lang_runtime::LanguageConfig;

/// Language configuration embedded at compile time.
pub static CONFIG: LanguageConfig = LanguageConfig {
    id: "zig",
    extensions: &["zig", "zon"],
    function_kinds: &["function_declaration", "fn"],
    class_kinds: &["struct", "enum"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};
