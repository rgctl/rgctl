//! Language plugin crate for rBuilder

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

pub mod analysis;
pub mod cli;
pub mod parser;
pub mod plugin;
pub mod security;

pub use analysis::*;
pub use plugin::role_dependencies_from_meta;
pub use plugin::AnsiblePlugin;
pub use security::*;

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(AnsiblePlugin::new().expect("init AnsiblePlugin")));
}
