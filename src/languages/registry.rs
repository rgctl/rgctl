//! Language plugin registry
//!
//! Manages all available language plugins and routes files to the appropriate plugin.

use crate::error::{Error, Result};
use crate::languages::builtin::*;
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

        // Register built-in language plugins
        registry.register_language_plugin(Arc::new(RustPlugin::new().unwrap()));
        registry.register_language_plugin(Arc::new(PythonPlugin::new().unwrap()));
        registry.register_language_plugin(Arc::new(TypeScriptPlugin::new().unwrap()));
        registry.register_language_plugin(Arc::new(JavaScriptPlugin::new().unwrap()));
        registry.register_language_plugin(Arc::new(GoPlugin::new().unwrap()));
        registry.register_language_plugin(Arc::new(MarkdownPlugin::new().unwrap()));

        // Register built-in config format plugins
        registry.register_config_plugin(Arc::new(YamlPlugin::new().unwrap()));
        registry.register_config_plugin(Arc::new(JsonPlugin::new().unwrap()));
        registry.register_config_plugin(Arc::new(TomlPlugin::new().unwrap()));
        registry.register_config_plugin(Arc::new(PropertiesPlugin::new().unwrap()));

        registry
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
        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            self.extension_map.contains_key(ext) || self.config_extension_map.contains_key(ext)
        } else {
            false
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = LanguageRegistry::new();
        let stats = registry.stats();

        // Should have 6 built-in language plugins
        assert_eq!(stats.language_plugins, 6);
    }

    #[test]
    fn test_get_plugin_by_language_id() {
        let registry = LanguageRegistry::new();

        let rust_plugin = registry.get_language_plugin("rust");
        assert!(rust_plugin.is_some());
        assert_eq!(rust_plugin.unwrap().language_id(), "rust");

        let python_plugin = registry.get_language_plugin("python");
        assert!(python_plugin.is_some());
        assert_eq!(python_plugin.unwrap().language_id(), "python");
    }

    #[test]
    fn test_get_plugin_for_file() {
        let registry = LanguageRegistry::new();

        let plugin = registry.get_plugin_for_file(Path::new("test.rs")).unwrap();
        assert_eq!(plugin.language_id(), "rust");

        let plugin = registry.get_plugin_for_file(Path::new("test.py")).unwrap();
        assert_eq!(plugin.language_id(), "python");

        let plugin = registry.get_plugin_for_file(Path::new("test.ts")).unwrap();
        assert_eq!(plugin.language_id(), "typescript");

        let plugin = registry.get_plugin_for_file(Path::new("test.js")).unwrap();
        assert_eq!(plugin.language_id(), "javascript");

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

    #[test]
    fn test_can_process_file() {
        let registry = LanguageRegistry::new();

        assert!(registry.can_process_file(Path::new("test.rs")));
        assert!(registry.can_process_file(Path::new("test.py")));
        assert!(registry.can_process_file(Path::new("test.ts")));
        assert!(registry.can_process_file(Path::new("test.js")));
        assert!(registry.can_process_file(Path::new("test.go")));
        assert!(!registry.can_process_file(Path::new("test.xyz")));
    }

    #[test]
    fn test_supported_languages() {
        let registry = LanguageRegistry::new();
        let languages = registry.supported_languages();

        assert_eq!(languages.len(), 6);
        assert!(languages.contains(&"rust".to_string()));
        assert!(languages.contains(&"python".to_string()));
        assert!(languages.contains(&"typescript".to_string()));
        assert!(languages.contains(&"javascript".to_string()));
        assert!(languages.contains(&"go".to_string()));
    }

    #[test]
    fn test_supported_extensions() {
        let registry = LanguageRegistry::new();
        let extensions = registry.supported_extensions();

        assert!(extensions.contains(&"rs".to_string()));
        assert!(extensions.contains(&"py".to_string()));
        assert!(extensions.contains(&"ts".to_string()));
        assert!(extensions.contains(&"js".to_string()));
        assert!(extensions.contains(&"go".to_string()));
    }

    #[test]
    fn test_multiple_extensions() {
        let registry = LanguageRegistry::new();

        // JavaScript plugin handles multiple extensions
        let plugin1 = registry.get_plugin_for_file(Path::new("test.js")).unwrap();
        let plugin2 = registry.get_plugin_for_file(Path::new("test.jsx")).unwrap();
        let plugin3 = registry.get_plugin_for_file(Path::new("test.mjs")).unwrap();

        assert_eq!(plugin1.language_id(), "javascript");
        assert_eq!(plugin2.language_id(), "javascript");
        assert_eq!(plugin3.language_id(), "javascript");

        // TypeScript plugin handles .ts and .tsx
        let plugin4 = registry.get_plugin_for_file(Path::new("test.ts")).unwrap();
        let plugin5 = registry.get_plugin_for_file(Path::new("test.tsx")).unwrap();

        assert_eq!(plugin4.language_id(), "typescript");
        assert_eq!(plugin5.language_id(), "typescript");
    }

    #[test]
    fn test_registry_stats() {
        let registry = LanguageRegistry::new();
        let stats = registry.stats();

        assert_eq!(stats.language_plugins, 6);
        assert_eq!(stats.config_plugins, 4); // YAML, JSON, TOML, Properties
        assert!(stats.total_extensions > 0);
    }

    #[test]
    fn test_config_plugins() {
        let registry = LanguageRegistry::new();

        // YAML plugin
        let yaml_plugin = registry.get_config_plugin("yaml");
        assert!(yaml_plugin.is_some());
        assert_eq!(yaml_plugin.unwrap().format_id(), "yaml");

        // JSON plugin
        let json_plugin = registry.get_config_plugin("json");
        assert!(json_plugin.is_some());
        assert_eq!(json_plugin.unwrap().format_id(), "json");

        // TOML plugin
        let toml_plugin = registry.get_config_plugin("toml");
        assert!(toml_plugin.is_some());
        assert_eq!(toml_plugin.unwrap().format_id(), "toml");
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
