//! Language plugin crate for rBuilder

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

pub mod analysis;
pub mod cli;
pub mod parser;
pub mod plugin;
pub mod security;

pub use analysis::*;
pub use plugin::module_dependencies_from_metadata;
pub use plugin::parse_content;
pub use plugin::PuppetPlugin;
pub use security::*;

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(PuppetPlugin::new().expect("init PuppetPlugin")));
}
