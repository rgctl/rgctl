//! Plugin ABI for external language plugins
//!
//! Task 3.2.1: Stable ABI version constants and metadata types.

/// Current plugin ABI version.
pub const PLUGIN_ABI_VERSION: u32 = 1;

/// Plugin metadata returned by external plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMetadata {
    /// Plugin language ID
    pub language_id: String,
    /// Plugin version
    pub version: String,
    /// Supported file extensions
    pub extensions: Vec<String>,
    /// ABI version implemented by the plugin
    pub abi_version: u32,
    /// Path the plugin was loaded from
    pub path: String,
}

impl PluginMetadata {
    /// Validate that plugin ABI is compatible.
    pub fn is_compatible(&self) -> bool {
        self.abi_version == PLUGIN_ABI_VERSION
    }
}

/// Required exported symbols from external plugins.
pub mod exports {
    /// Plugin info export name
    pub const PLUGIN_INFO: &str = "rbuilder_plugin_info";
    /// Plugin ABI version export name
    pub const PLUGIN_ABI: &str = "rbuilder_plugin_abi_version";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_compatibility() {
        let meta = PluginMetadata {
            language_id: "custom".to_string(),
            version: "0.1.0".to_string(),
            extensions: vec!["cst".to_string()],
            abi_version: PLUGIN_ABI_VERSION,
            path: "/tmp/libcustom.so".to_string(),
        };
        assert!(meta.is_compatible());
    }
}
