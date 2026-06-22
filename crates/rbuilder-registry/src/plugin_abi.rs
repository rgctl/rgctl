//! Plugin metadata for external plugin registry entries.

/// Current plugin registry schema version.
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
    /// ABI / registry schema version recorded at install time.
    pub abi_version: u32,
    /// Path the plugin was loaded from
    pub path: String,
}

impl PluginMetadata {
    /// Validate registry schema version.
    pub fn is_compatible(&self) -> bool {
        self.abi_version == PLUGIN_ABI_VERSION
    }
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
