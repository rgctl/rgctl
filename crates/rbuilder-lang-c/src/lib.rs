//! Language plugin crate for rBuilder

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

mod plugin;
pub use plugin::CPlugin;

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(CPlugin::new().expect("init CPlugin")));
}
