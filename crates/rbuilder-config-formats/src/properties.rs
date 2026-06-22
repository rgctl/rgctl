//! Java properties file plugin

use rbuilder_plugin_api::*;
use rbuilder_plugin_api::{Error, Result};
use std::path::Path;

/// Properties file config format plugin
pub struct PropertiesPlugin;

impl PropertiesPlugin {
    /// Create a new properties plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl ConfigFormatPlugin for PropertiesPlugin {
    fn format_id(&self) -> &str {
        "properties"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["properties", "ini"]
    }

    fn extract_config_keys(&self, file_path: &Path, source: &[u8]) -> Result<Vec<ConfigKey>> {
        let file = file_path.to_string_lossy().to_string();
        let text = std::str::from_utf8(source).map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e.to_string(),
        })?;

        let mut keys = Vec::new();

        for (line_idx, line) in text.lines().enumerate() {
            let line_no = line_idx + 1;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                continue;
            }

            let Some((key, value)) = trimmed.split_once('=') else {
                continue;
            };

            keys.push(ConfigKey {
                key_path: key.trim().to_string(),
                value: value.trim().to_string(),
                value_type: ConfigValueType::String,
                location: SourceLocation {
                    file: file.clone(),
                    start_line: line_no,
                    end_line: line_no,
                    start_column: 0,
                    end_column: 0,
                },
            });
        }

        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_properties_parsing() {
        let source = b"# comment\nserver.port=8080\ndb.host=localhost\n";
        let plugin = PropertiesPlugin::new().unwrap();
        let keys = plugin
            .extract_config_keys(Path::new("app.properties"), source)
            .unwrap();

        assert_eq!(keys.len(), 2);
        assert!(keys.iter().any(|k| k.key_path == "server.port"));
    }
}
