//! Built-in Tier 1 language plugin registration.

use rbuilder_registry::LanguageRegistry;

/// Register all Tier 1 language plugins.
pub fn register_languages(registry: &mut LanguageRegistry) {
    rbuilder_lang_rust::register(registry);
    rbuilder_lang_python::register(registry);
    rbuilder_lang_javascript::register(registry);
    rbuilder_lang_typescript::register(registry);
    rbuilder_lang_go::register(registry);
    rbuilder_lang_java::register(registry);
    rbuilder_lang_csharp::register(registry);
}

/// Default registry with config formats and all built-in languages.
pub fn default_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::with_config_formats();
    register_languages(&mut registry);
    registry
}
