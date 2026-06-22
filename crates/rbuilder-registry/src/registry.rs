//! Language plugin registry
//!
//! Manages all available language plugins and routes files to the appropriate plugin.

use rbuilder_error::{Error, Result};
use rbuilder_plugin_api::{ConfigFormatPlugin, ConfigFormatRegistrar, LanguagePlugin};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

/// Optional hook for the binary crate to register feature-gated language plugins.
static FULL_REGISTRY_BUILDER: OnceLock<fn() -> LanguageRegistry> = OnceLock::new();

/// Optional hook run before building a full registry (e.g. register the builder).
static REGISTRY_PRE_INIT: OnceLock<fn()> = OnceLock::new();

/// Register a builder that returns a registry with language plugins enabled by Cargo features.
pub fn set_full_registry_builder(builder: fn() -> LanguageRegistry) {
    let _ = FULL_REGISTRY_BUILDER.set(builder);
}

/// Register a one-time initializer (monolith sets this to wire `build.rs` registration).
pub fn set_registry_pre_init(init: fn()) {
    let _ = REGISTRY_PRE_INIT.set(init);
}

/// Create a registry using the registered builder, or config formats only if unset.
pub fn full_registry() -> LanguageRegistry {
    if let Some(init) = REGISTRY_PRE_INIT.get() {
        init();
    }
    FULL_REGISTRY_BUILDER
        .get()
        .copied()
        .unwrap_or(LanguageRegistry::with_config_formats)()
}

/// Registry for all language and config format plugins
pub struct LanguageRegistry {
    /// Language plugins by language ID
    language_plugins: HashMap<String, Arc<dyn LanguagePlugin>>,

    /// Config format plugins by format ID
    config_plugins: HashMap<String, Arc<dyn ConfigFormatPlugin>>,

    /// File extension to language plugin mapping
    extension_map: HashMap<String, Arc<dyn LanguagePlugin>>,

    /// File extension to config plugin mapping
    config_extension_map: HashMap<String, Arc<dyn ConfigFormatPlugin>>,
}

impl LanguageRegistry {
    /// Create an empty registry with no plugins registered.
    pub fn empty() -> Self {
        Self {
            language_plugins: HashMap::new(),
            config_plugins: HashMap::new(),
            extension_map: HashMap::new(),
            config_extension_map: HashMap::new(),
        }
    }

    /// Create a registry with built-in config format plugins only.
    pub fn with_config_formats() -> Self {
        let mut registry = Self::empty();
        rbuilder_config_formats::register_all(&mut registry);
        registry
    }

    /// Create a registry with config formats and any registered language plugins.
    pub fn new() -> Self {
        full_registry()
    }

    /// Check if a language plugin is registered.
    pub fn has_plugin(&self, language_id: &str) -> bool {
        self.language_plugins.contains_key(language_id)
    }

    /// List all language plugin IDs.
    pub fn language_plugin_ids(&self) -> Vec<String> {
        self.language_plugins.keys().cloned().collect()
    }

    /// Register a language plugin
    pub fn register_language_plugin(&mut self, plugin: Arc<dyn LanguagePlugin>) {
        let id = plugin.language_id().to_string();

        self.language_plugins
            .insert(id.clone(), Arc::clone(&plugin));

        for ext in plugin.file_extensions() {
            self.extension_map
                .insert(ext.to_string(), Arc::clone(&plugin));
        }
    }

    /// Register a config format plugin
    pub fn register_config_plugin(&mut self, plugin: Arc<dyn ConfigFormatPlugin>) {
        let id = plugin.format_id().to_string();

        self.config_plugins.insert(id.clone(), Arc::clone(&plugin));

        for ext in plugin.file_extensions() {
            self.config_extension_map
                .insert(ext.to_string(), Arc::clone(&plugin));
        }
    }

    /// Get a language plugin by ID
    pub fn get_language_plugin(&self, language_id: &str) -> Option<Arc<dyn LanguagePlugin>> {
        self.language_plugins.get(language_id).cloned()
    }

    /// Get a config format plugin by ID
    pub fn get_config_plugin(&self, format_id: &str) -> Option<Arc<dyn ConfigFormatPlugin>> {
        self.config_plugins.get(format_id).cloned()
    }

    /// Get a language plugin for a file path
    pub fn get_plugin_for_file(&self, file_path: &Path) -> Result<Arc<dyn LanguagePlugin>> {
        let path_str = file_path.to_string_lossy().replace('\\', "/");

        if let Some(plugin) = self.language_plugin_for_path(&path_str) {
            return Ok(plugin);
        }

        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            self.extension_map
                .get(ext)
                .cloned()
                .ok_or_else(|| Error::UnsupportedLanguage(ext.to_string()))
        } else {
            Err(Error::UnsupportedLanguage(
                file_path.to_string_lossy().to_string(),
            ))
        }
    }

    /// Get a config plugin for a file path
    pub fn get_config_plugin_for_file(
        &self,
        file_path: &Path,
    ) -> Result<Arc<dyn ConfigFormatPlugin>> {
        let path_str = file_path.to_string_lossy().replace('\\', "/");
        if self.language_plugin_for_path(&path_str).is_some() {
            return Err(Error::UnsupportedLanguage(
                file_path.to_string_lossy().to_string(),
            ));
        }

        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            self.config_extension_map
                .get(ext)
                .cloned()
                .ok_or_else(|| Error::UnsupportedLanguage(ext.to_string()))
        } else {
            Err(Error::UnsupportedLanguage(
                file_path.to_string_lossy().to_string(),
            ))
        }
    }

    /// Find a registered language plugin that claims `path` via [`LanguagePlugin::matches_path`].
    ///
    /// Path-heuristic plugins (empty [`LanguagePlugin::file_extensions`]) are checked first so
    /// IaC/CI routing wins over generic extension handlers (e.g. chef vs ruby on `.rb`).
    fn language_plugin_for_path(&self, path: &str) -> Option<Arc<dyn LanguagePlugin>> {
        if let Some(plugin) = self
            .language_plugins
            .values()
            .filter(|plugin| plugin.file_extensions().is_empty())
            .find(|plugin| plugin.matches_path(path))
        {
            return Some(Arc::clone(plugin));
        }
        self.language_plugins
            .values()
            .find(|plugin| plugin.matches_path(path))
            .cloned()
    }

    /// Check if a file can be processed (either as code or config)
    pub fn can_process_file(&self, file_path: &Path) -> bool {
        if self.get_plugin_for_file(file_path).is_ok() {
            return true;
        }
        self.get_config_plugin_for_file(file_path).is_ok()
    }

    /// List all supported language IDs
    pub fn supported_languages(&self) -> Vec<String> {
        self.language_plugins.keys().cloned().collect()
    }

    /// List all supported config formats
    pub fn supported_config_formats(&self) -> Vec<String> {
        self.config_plugins.keys().cloned().collect()
    }

    /// List all supported file extensions
    pub fn supported_extensions(&self) -> Vec<String> {
        let mut extensions: Vec<String> = self
            .extension_map
            .keys()
            .chain(self.config_extension_map.keys())
            .cloned()
            .collect();
        extensions.sort();
        extensions.dedup();
        extensions
    }

    /// Get statistics about registered plugins
    pub fn stats(&self) -> RegistryStats {
        RegistryStats {
            language_plugins: self.language_plugins.len(),
            config_plugins: self.config_plugins.len(),
            total_extensions: self.supported_extensions().len(),
        }
    }
}

impl ConfigFormatRegistrar for LanguageRegistry {
    fn register_config_plugin(&mut self, plugin: Arc<dyn ConfigFormatPlugin>) {
        LanguageRegistry::register_config_plugin(self, plugin);
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::with_config_formats()
    }
}

/// Statistics about the registry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryStats {
    /// Number of language plugins
    pub language_plugins: usize,

    /// Number of config format plugins
    pub config_plugins: usize,

    /// Total number of supported file extensions
    pub total_extensions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry() {
        let registry = LanguageRegistry::empty();
        let stats = registry.stats();
        assert_eq!(stats.language_plugins, 0);
        assert_eq!(stats.config_plugins, 0);
    }

    #[test]
    fn test_with_config_formats() {
        let registry = LanguageRegistry::with_config_formats();
        let stats = registry.stats();
        assert_eq!(stats.language_plugins, 0);
        assert_eq!(stats.config_plugins, 4);
    }

    #[test]
    fn test_config_plugins() {
        let registry = LanguageRegistry::with_config_formats();
        assert!(registry.get_config_plugin("yaml").is_some());
        assert!(registry.get_config_plugin("json").is_some());
        assert!(registry.get_config_plugin("toml").is_some());
    }

    #[test]
    fn test_can_process_config_files() {
        let registry = LanguageRegistry::with_config_formats();
        assert!(registry.can_process_file(Path::new("config.yaml")));
        assert!(registry.can_process_file(Path::new("config.yml")));
        assert!(registry.can_process_file(Path::new("config.json")));
        assert!(registry.can_process_file(Path::new("config.toml")));
    }
}
