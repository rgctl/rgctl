//! Language bundle: extended

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
    rbuilder_lang_java::register(registry);
    rbuilder_lang_kotlin::register(registry);
    rbuilder_lang_csharp::register(registry);
    rbuilder_lang_markdown::register(registry);
    rbuilder_lang_c::register(registry);
    rbuilder_lang_cpp::register(registry);
    rbuilder_lang_ruby::register(registry);
    rbuilder_lang_php::register(registry);
    rbuilder_lang_sql::register(registry);
    rbuilder_lang_bash::register(registry);
    rbuilder_lang_dockerfile::register(registry);
    rbuilder_lang_github_actions::register(registry);
    rbuilder_lang_gitlab_ci::register(registry);
    rbuilder_lang_ansible::register(registry);
    rbuilder_lang_chef::register(registry);
    rbuilder_lang_puppet::register(registry);
}

/// Default registry with config formats and bundle languages.
pub fn default_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::with_config_formats();
    register_languages(&mut registry);
    registry
}
