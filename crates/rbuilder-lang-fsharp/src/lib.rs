//! Language plugin crate for rBuilder

mod config;

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

use rbuilder_lang_runtime::TreeSitterLanguagePlugin;

fn load_fsharp_grammar() -> tree_sitter::Language {
    tree_sitter_fsharp::LANGUAGE_FSHARP.into()
}

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(TreeSitterLanguagePlugin::from_config(
        &config::CONFIG,
        load_fsharp_grammar,
    )));
}
