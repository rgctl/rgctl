//! Language bundle: minimal

use rbuilder_registry::LanguageRegistry;

/// Register config format plugins (yaml, json, toml, properties).
pub fn register_config_formats(registry: &mut LanguageRegistry) {
    rbuilder_config_formats::register_all(registry);
}

/// Register all language plugins in this bundle.
pub fn register_languages(registry: &mut LanguageRegistry) {
    rbuilder_lang_rust::register(registry);
    rbuilder_lang_python::register(registry);
    rbuilder_lang_javascript::register(registry);
    rbuilder_lang_typescript::register(registry);
    rbuilder_lang_go::register(registry);
}

/// Default registry with config formats and bundle languages.
pub fn default_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::with_config_formats();
    register_languages(&mut registry);
    registry
}
