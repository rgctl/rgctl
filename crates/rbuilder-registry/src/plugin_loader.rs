//! External plugin registry (`.rbuilder/plugins.json`).
//!
//! Records paths to third-party plugin artifacts. Parsing is not loaded from
//! shared libraries; built-in languages use `rbuilder-lang-*` crates instead.

use crate::plugin_abi::{PluginMetadata, PLUGIN_ABI_VERSION};
use rbuilder_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persisted plugin registry file name.
pub const PLUGIN_REGISTRY_FILE: &str = "plugins.json";

/// Installed plugin record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPlugin {
    /// Language ID
    pub language_id: String,
    /// Plugin version
    pub version: String,
    /// Path to plugin library
    pub path: String,
    /// File extensions
    pub extensions: Vec<String>,
}

/// Plugin registry persisted under `.rbuilder/`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRegistry {
    /// Installed external plugins
    pub plugins: Vec<InstalledPlugin>,
}

impl PluginRegistry {
    /// Load registry from a repository root.
    pub fn load(repo_root: &Path) -> Result<Self> {
        let path = repo_root
            .join(rbuilder_graph::code_graph::GRAPH_DIR)
            .join(PLUGIN_REGISTRY_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(|e| Error::SerdeError(e.to_string()))
    }

    /// Save registry to a repository root.
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let dir = repo_root.join(rbuilder_graph::code_graph::GRAPH_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(PLUGIN_REGISTRY_FILE);
        let json =
            serde_json::to_string_pretty(self).map_err(|e| Error::SerdeError(e.to_string()))?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Install a plugin path into the registry.
    pub fn install(&mut self, metadata: PluginMetadata) -> Result<()> {
        if !metadata.is_compatible() {
            return Err(Error::PluginError(format!(
                "Plugin ABI version {} incompatible with {}",
                metadata.abi_version, PLUGIN_ABI_VERSION
            )));
        }
        self.plugins
            .retain(|p| p.language_id != metadata.language_id);
        self.plugins.push(InstalledPlugin {
            language_id: metadata.language_id,
            version: metadata.version,
            path: metadata.path,
            extensions: metadata.extensions,
        });
        Ok(())
    }

    /// Uninstall a plugin by language ID.
    pub fn uninstall(&mut self, language_id: &str) -> bool {
        let before = self.plugins.len();
        self.plugins.retain(|p| p.language_id != language_id);
        self.plugins.len() < before
    }

    /// Get plugin by language ID.
    pub fn get(&self, language_id: &str) -> Option<&InstalledPlugin> {
        self.plugins.iter().find(|p| p.language_id == language_id)
    }
}

/// Registry helpers for external plugin paths (metadata only).
pub struct PluginLoader;

impl PluginLoader {
    /// Inspect a plugin path and return metadata from the file name.
    pub fn inspect(path: &Path) -> Result<PluginMetadata> {
        if !path.exists() {
            return Err(Error::NotFound(format!(
                "Plugin not found: {}",
                path.display()
            )));
        }

        Ok(PluginMetadata {
            language_id: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("external")
                .to_string(),
            version: "0.1.0".to_string(),
            extensions: vec![],
            abi_version: PLUGIN_ABI_VERSION,
            path: path.display().to_string(),
        })
    }

    /// Install plugin into registry by copying path reference.
    pub fn install(repo_root: &Path, plugin_path: &Path) -> Result<PluginMetadata> {
        let metadata = Self::inspect(plugin_path)?;
        let mut registry = PluginRegistry::load(repo_root)?;
        registry.install(metadata.clone())?;
        registry.save(repo_root)?;
        Ok(metadata)
    }

    /// Copy plugin into `.rbuilder/plugins/` directory.
    pub fn copy_to_plugins_dir(repo_root: &Path, source: &Path) -> Result<PathBuf> {
        let dir = repo_root
            .join(rbuilder_graph::code_graph::GRAPH_DIR)
            .join("plugins");
        std::fs::create_dir_all(&dir)?;
        let file_name = source
            .file_name()
            .ok_or_else(|| Error::PluginError("Invalid plugin path".to_string()))?;
        let dest = dir.join(file_name);
        std::fs::copy(source, &dest)?;
        Ok(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_plugin_registry_roundtrip() {
        let temp = TempDir::new().unwrap();
        let mut registry = PluginRegistry::default();
        registry
            .install(PluginMetadata {
                language_id: "custom".to_string(),
                version: "0.1.0".to_string(),
                extensions: vec!["cst".to_string()],
                abi_version: PLUGIN_ABI_VERSION,
                path: "/tmp/libcustom.so".to_string(),
            })
            .unwrap();
        registry.save(temp.path()).unwrap();
        let loaded = PluginRegistry::load(temp.path()).unwrap();
        assert_eq!(loaded.plugins.len(), 1);
    }
}
