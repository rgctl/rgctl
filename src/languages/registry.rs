//! Language plugin registry
//!
//! Manages all available language plugins and routes files to the appropriate plugin.

use crate::error::{Error, Result};
use crate::languages::config::*;
use crate::languages::plugin_trait::{ConfigFormatPlugin, LanguagePlugin};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

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
    /// Create a new registry with default built-in plugins
    pub fn new() -> Self {
        let mut registry = Self {
            language_plugins: HashMap::new(),
            config_plugins: HashMap::new(),
            extension_map: HashMap::new(),
            config_extension_map: HashMap::new(),
        };

        // Register language plugins (generated from languages.toml)
        register_all_language_plugins(&mut registry);

        // Register built-in config format plugins
        registry.register_config_plugin(Arc::new(YamlPlugin::new().unwrap()));
        registry.register_config_plugin(Arc::new(JsonPlugin::new().unwrap()));
        registry.register_config_plugin(Arc::new(TomlPlugin::new().unwrap()));
        registry.register_config_plugin(Arc::new(PropertiesPlugin::new().unwrap()));

        registry
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

        // Register by language ID
        self.language_plugins.insert(id.clone(), Arc::clone(&plugin));

        // Register file extensions
        for ext in plugin.file_extensions() {
            self.extension_map.insert(ext.to_string(), Arc::clone(&plugin));
        }
    }

    /// Register a config format plugin
    pub fn register_config_plugin(&mut self, plugin: Arc<dyn ConfigFormatPlugin>) {
        let id = plugin.format_id().to_string();

        // Register by format ID
        self.config_plugins.insert(id.clone(), Arc::clone(&plugin));

        // Register file extensions
        for ext in plugin.file_extensions() {
            self.config_extension_map.insert(ext.to_string(), Arc::clone(&plugin));
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

        // Path-sensitive CI plugins (share .yml/.yaml with config handlers).
        if path_str.contains(".github/workflows/") {
            if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                if ext == "yml" || ext == "yaml" {
                    if let Some(plugin) = self.language_plugins.get("github_actions") {
                        return Ok(Arc::clone(plugin));
                    }
                }
            }
        }
        if file_path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == ".gitlab-ci.yml" || n.ends_with(".gitlab-ci.yml"))
        {
            if let Some(plugin) = self.language_plugins.get("gitlab_ci") {
                return Ok(Arc::clone(plugin));
            }
        }
        if crate::languages::multimodal::chef::parser::ChefParser::is_chef_path(&path_str) {
            if let Some(plugin) = self.language_plugins.get("chef") {
                return Ok(Arc::clone(plugin));
            }
        }
        if crate::languages::multimodal::ansible::parser::AnsibleParser::is_ansible_path(&path_str) {
            if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                if ext == "yml" || ext == "yaml" || ext == "j2" {
                    if let Some(plugin) = self.language_plugins.get("ansible") {
                        return Ok(Arc::clone(plugin));
                    }
                }
            } else if path_str.ends_with(".j2") {
                if let Some(plugin) = self.language_plugins.get("ansible") {
                    return Ok(Arc::clone(plugin));
                }
            }
        }

        // Dockerfile has no extension.
        if file_path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("dockerfile"))
        {
            if let Some(plugin) = self.language_plugins.get("dockerfile") {
                return Ok(Arc::clone(plugin));
            }
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
    pub fn get_config_plugin_for_file(&self, file_path: &Path) -> Result<Arc<dyn ConfigFormatPlugin>> {
        let path_str = file_path.to_string_lossy().replace('\\', "/");
        if path_str.contains(".github/workflows/")
            || file_path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains("gitlab-ci"))
            || crate::languages::multimodal::chef::parser::ChefParser::is_chef_path(&path_str)
            || crate::languages::multimodal::ansible::parser::AnsibleParser::is_ansible_path(&path_str)
        {
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

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
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

include!(concat!(env!("OUT_DIR"), "/generated_register.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = LanguageRegistry::new();
        let stats = registry.stats();
        assert!(stats.language_plugins >= 1);
        assert_eq!(stats.config_plugins, 4);
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn test_get_rust_plugin() {
        let registry = LanguageRegistry::new();
        let rust_plugin = registry.get_language_plugin("rust");
        assert!(rust_plugin.is_some());
        assert_eq!(rust_plugin.unwrap().language_id(), "rust");
    }

    #[cfg(feature = "lang-python")]
    #[test]
    fn test_get_python_plugin() {
        let registry = LanguageRegistry::new();
        let python_plugin = registry.get_language_plugin("python");
        assert!(python_plugin.is_some());
        assert_eq!(python_plugin.unwrap().language_id(), "python");
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn test_get_plugin_for_rust_file() {
        let registry = LanguageRegistry::new();
        let plugin = registry.get_plugin_for_file(Path::new("test.rs")).unwrap();
        assert_eq!(plugin.language_id(), "rust");
    }

    #[cfg(feature = "lang-python")]
    #[test]
    fn test_get_plugin_for_python_file() {
        let registry = LanguageRegistry::new();
        let plugin = registry.get_plugin_for_file(Path::new("test.py")).unwrap();
        assert_eq!(plugin.language_id(), "python");
    }

    #[cfg(feature = "lang-typescript")]
    #[test]
    fn test_get_plugin_for_typescript_file() {
        let registry = LanguageRegistry::new();
        let plugin = registry.get_plugin_for_file(Path::new("test.ts")).unwrap();
        assert_eq!(plugin.language_id(), "typescript");
    }

    #[cfg(feature = "lang-javascript")]
    #[test]
    fn test_get_plugin_for_javascript_file() {
        let registry = LanguageRegistry::new();
        let plugin = registry.get_plugin_for_file(Path::new("test.js")).unwrap();
        assert_eq!(plugin.language_id(), "javascript");
    }

    #[cfg(feature = "lang-go")]
    #[test]
    fn test_get_plugin_for_go_file() {
        let registry = LanguageRegistry::new();
        let plugin = registry.get_plugin_for_file(Path::new("test.go")).unwrap();
        assert_eq!(plugin.language_id(), "go");
    }

    #[test]
    fn test_unsupported_file() {
        let registry = LanguageRegistry::new();
        let result = registry.get_plugin_for_file(Path::new("test.xyz"));
        assert!(result.is_err());
        if let Err(Error::UnsupportedLanguage(ext)) = result {
            assert_eq!(ext, "xyz");
        } else {
            panic!("Expected UnsupportedLanguage error");
        }
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn test_can_process_rust_file() {
        let registry = LanguageRegistry::new();
        assert!(registry.can_process_file(Path::new("test.rs")));
    }

    #[cfg(all(feature = "bundle-minimal", not(feature = "bundle-extended")))]
    #[test]
    fn test_minimal_bundle_language_count() {
        let registry = LanguageRegistry::new();
        assert_eq!(registry.stats().language_plugins, 5);
    }

    #[cfg(all(feature = "bundle-extended", not(feature = "bundle-full")))]
    #[test]
    fn test_extended_bundle_language_count() {
        let registry = LanguageRegistry::new();
        assert_eq!(registry.stats().language_plugins, 20);
    }

    #[cfg(all(feature = "bundle-full", not(feature = "bundle-extra")))]
    #[test]
    fn test_full_bundle_language_count() {
        let registry = LanguageRegistry::new();
        assert_eq!(registry.stats().language_plugins, 30);
    }

    #[cfg(feature = "bundle-extra")]
    #[test]
    fn test_extra_bundle_language_count() {
        let registry = LanguageRegistry::new();
        assert_eq!(registry.stats().language_plugins, 43);
    }

    #[cfg(feature = "lang-javascript")]
    #[test]
    fn test_javascript_multiple_extensions() {
        let registry = LanguageRegistry::new();
        for ext in ["test.js", "test.jsx", "test.mjs"] {
            let plugin = registry.get_plugin_for_file(Path::new(ext)).unwrap();
            assert_eq!(plugin.language_id(), "javascript");
        }
    }

    #[cfg(feature = "lang-typescript")]
    #[test]
    fn test_typescript_multiple_extensions() {
        let registry = LanguageRegistry::new();
        for ext in ["test.ts", "test.tsx"] {
            let plugin = registry.get_plugin_for_file(Path::new(ext)).unwrap();
            assert_eq!(plugin.language_id(), "typescript");
        }
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
