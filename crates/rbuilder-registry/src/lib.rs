//! Language plugin registry and dynamic plugin loading

pub mod plugin_abi;
pub mod plugin_loader;

mod registry;

pub use registry::{
    full_registry, set_full_registry_builder, set_registry_pre_init, LanguageRegistry,
    RegistryStats,
};
