//! Language plugin crate for rBuilder

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

mod plugin;
pub use plugin::GitlabCiPlugin;

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(
        GitlabCiPlugin::new().expect("init GitlabCiPlugin"),
    ));
}
