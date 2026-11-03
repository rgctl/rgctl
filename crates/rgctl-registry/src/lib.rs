//! Language plugin registry and dynamic plugin loading

pub mod ingest_route;
pub mod m_disambiguate;
pub mod plugin_abi;
pub mod plugin_loader;

mod registry;

pub use ingest_route::{IngestRoute, classify_ingest_path};
pub use m_disambiguate::sniff_objc_vs_matlab;
pub use registry::{
    LanguageRegistry, RegistryStats, full_registry, set_full_registry_builder,
    set_registry_pre_init,
};

pub mod template_options;
pub use template_options::{AnsibleOptions, JinjaOptions, TemplateOptions};

/// Canonical structural language aliases (generic YAML remains a config format).
pub fn canonical_language_id(id: &str) -> &str {
    match id {
        "j2" | "jinja2" => "jinja",
        _ => id,
    }
}
