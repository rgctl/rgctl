//! Language plugin crate for rBuilder

mod config;

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

use rbuilder_lang_runtime::RegexLanguagePlugin;

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(RegexLanguagePlugin::from_config(&config::CONFIG)));
}
