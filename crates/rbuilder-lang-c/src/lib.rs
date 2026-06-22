//! Language plugin crate for rBuilder

mod config;

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

use rbuilder_lang_runtime::TreeSitterLanguagePlugin;

fn load_c_grammar() -> tree_sitter::Language {
    tree_sitter_c::LANGUAGE.into()
}

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(TreeSitterLanguagePlugin::from_config(
        &config::CONFIG,
        load_c_grammar,
    )));
}
