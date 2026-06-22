//! Monolith registry wrapper — delegates to bundle crates.

pub use rbuilder_registry::{plugin_abi, plugin_loader, RegistryStats};

use rbuilder_registry::LanguageRegistry as InnerRegistry;
use std::sync::Once;

static INIT: Once = Once::new();

/// Wire the bundle registry builder for `rbuilder_registry::full_registry()`.
pub fn ensure_initialized() {
    INIT.call_once(|| {
        rbuilder_registry::set_full_registry_builder(build_registry_inner);
    });
}

fn build_registry_inner() -> InnerRegistry {
    #[cfg(feature = "bundle-extra")]
    {
        rbuilder_bundle_extra::default_registry()
    }
    #[cfg(all(feature = "bundle-full", not(feature = "bundle-extra")))]
    {
        rbuilder_bundle_full::default_registry()
    }
    #[cfg(all(
        feature = "bundle-extended",
        not(any(feature = "bundle-full", feature = "bundle-extra"))
    ))]
    {
        rbuilder_bundle_extended::default_registry()
    }
    #[cfg(all(
        feature = "bundle-minimal",
        not(any(
            feature = "bundle-extended",
            feature = "bundle-full",
            feature = "bundle-extra"
        ))
    ))]
    {
        rbuilder_bundle_minimal::default_registry()
    }
    #[cfg(not(any(
        feature = "bundle-minimal",
        feature = "bundle-extended",
        feature = "bundle-full",
        feature = "bundle-extra"
    )))]
    {
        InnerRegistry::with_config_formats()
    }
}

/// Build a registry using the active bundle feature.
pub fn build_registry() -> LanguageRegistry {
    ensure_initialized();
    LanguageRegistry(build_registry_inner())
}

/// Registry with bundle-selected language plugins.
pub struct LanguageRegistry(InnerRegistry);

impl LanguageRegistry {
    /// Create a registry with config formats and bundle language plugins.
    pub fn new() -> Self {
        build_registry()
    }

    /// Create an empty registry.
    pub fn empty() -> Self {
        Self(InnerRegistry::empty())
    }

    /// Create a registry with config format plugins only.
    pub fn with_config_formats() -> Self {
        Self(InnerRegistry::with_config_formats())
    }

    /// Consume the wrapper and return the inner registry.
    pub fn into_inner(self) -> InnerRegistry {
        self.0
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for LanguageRegistry {
    type Target = InnerRegistry;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for LanguageRegistry {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<LanguageRegistry> for std::sync::Arc<InnerRegistry> {
    fn from(value: LanguageRegistry) -> Self {
        std::sync::Arc::new(value.0)
    }
}

impl From<LanguageRegistry> for InnerRegistry {
    fn from(value: LanguageRegistry) -> Self {
        value.0
    }
}

/// No-op alias; wiring happens in [`ensure_initialized`].
pub fn ensure_registry_initialized() {
    ensure_initialized();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_registry_creation() {
        let registry = LanguageRegistry::new();
        let stats = registry.stats();
        assert!(stats.language_plugins >= 1);
        assert_eq!(stats.config_plugins, 4);
    }

    #[test]
    fn test_get_rust_plugin() {
        let registry = LanguageRegistry::new();
        let rust_plugin = registry.get_language_plugin("rust");
        assert!(rust_plugin.is_some());
        assert_eq!(rust_plugin.unwrap().language_id(), "rust");
    }

    #[test]
    fn test_config_plugins() {
        let registry = LanguageRegistry::new();
        assert!(registry.get_config_plugin("yaml").is_some());
        assert!(registry.get_config_plugin("json").is_some());
        assert!(registry.get_config_plugin("toml").is_some());
    }

    #[test]
    fn test_can_process_config_files() {
        let registry = LanguageRegistry::new();
        assert!(registry.can_process_file(Path::new("config.yaml")));
        assert!(registry.can_process_file(Path::new("config.yml")));
        assert!(registry.can_process_file(Path::new("config.json")));
        assert!(registry.can_process_file(Path::new("config.toml")));
    }
}
