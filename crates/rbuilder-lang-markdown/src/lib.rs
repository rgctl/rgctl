//! Language plugin crate for rBuilder

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

use rbuilder_config_formats::MarkdownPlugin;

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(MarkdownPlugin::new().expect("init markdown")));
}
