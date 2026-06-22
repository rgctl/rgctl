//! Language plugin registry wrapper (languages live in `rbuilder-lang-*` crates).

pub use rbuilder_config_formats as config;
pub use rbuilder_lang_runtime as generic;
pub use rbuilder_plugin_api as plugin_trait;
pub use rbuilder_plugin_helpers as extraction;
pub use rbuilder_registry::{plugin_abi, plugin_loader};

pub mod registry;

pub use registry::LanguageRegistry;

/// No-op alias; wiring happens in [`registry::ensure_initialized`].
pub fn ensure_registry_initialized() {
    registry::ensure_initialized();
}
