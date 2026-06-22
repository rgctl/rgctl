//! TOML configuration format plugin

use rbuilder_plugin_api::Result;
use rbuilder_plugin_api::*;
use std::path::Path;

/// TOML config format plugin
pub struct TomlPlugin;

impl TomlPlugin {
    /// Create a new TOML plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn flatten_toml_value(
        &self,
        value: &toml::Value,
        prefix: &str,
        file: &str,
        results: &mut Vec<ConfigKey>,
    ) {
        match value {
            toml::Value::Table(map) => {
                for (k, v) in map {
                    let full_key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    self.flatten_toml_value(v, &full_key, file, results);
                }
            }
            toml::Value::Array(arr) => {
                results.push(ConfigKey {
                    key_path: prefix.to_string(),
                    value: format!("[array with {} items]", arr.len()),
                    value_type: ConfigValueType::Array,
                    location: SourceLocation {
                        file: file.to_string(),
                        start_line: 0,
                        end_line: 0,
                        start_column: 0,
                        end_column: 0,
                    },
                });
            }
            toml::Value::String(s) => {
                results.push(ConfigKey {
                    key_path: prefix.to_string(),
                    value: s.clone(),
                    value_type: ConfigValueType::String,
                    location: SourceLocation {
                        file: file.to_string(),
                        start_line: 0,
                        end_line: 0,
                        start_column: 0,
                        end_column: 0,
                    },
                });
            }
            toml::Value::Integer(n) => {
                results.push(ConfigKey {
                    key_path: prefix.to_string(),
                    value: n.to_string(),
                    value_type: ConfigValueType::Number,
                    location: SourceLocation {
                        file: file.to_string(),
                        start_line: 0,
                        end_line: 0,
                        start_column: 0,
                        end_column: 0,
                    },
                });
            }
            toml::Value::Float(n) => {
                results.push(ConfigKey {
                    key_path: prefix.to_string(),
                    value: n.to_string(),
                    value_type: ConfigValueType::Number,
                    location: SourceLocation {
                        file: file.to_string(),
                        start_line: 0,
                        end_line: 0,
                        start_column: 0,
                        end_column: 0,
                    },
                });
            }
            toml::Value::Boolean(b) => {
                results.push(ConfigKey {
                    key_path: prefix.to_string(),
                    value: b.to_string(),
                    value_type: ConfigValueType::Boolean,
                    location: SourceLocation {
                        file: file.to_string(),
                        start_line: 0,
                        end_line: 0,
                        start_column: 0,
                        end_column: 0,
                    },
                });
            }
            toml::Value::Datetime(dt) => {
                results.push(ConfigKey {
                    key_path: prefix.to_string(),
                    value: dt.to_string(),
                    value_type: ConfigValueType::String,
                    location: SourceLocation {
                        file: file.to_string(),
                        start_line: 0,
                        end_line: 0,
                        start_column: 0,
                        end_column: 0,
                    },
                });
            }
        }
    }
}

impl Default for TomlPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create TomlPlugin")
    }
}

impl ConfigFormatPlugin for TomlPlugin {
    fn format_id(&self) -> &str {
        "toml"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["toml"]
    }

    fn extract_config_keys(&self, file_path: &Path, source: &[u8]) -> Result<Vec<ConfigKey>> {
        let content = std::str::from_utf8(source)?;
        let value: toml::Value = toml::from_str(content)?;

        let mut results = Vec::new();
        self.flatten_toml_value(&value, "", &file_path.to_string_lossy(), &mut results);

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toml_plugin_format_id() {
        let plugin = TomlPlugin::new().unwrap();
        assert_eq!(plugin.format_id(), "toml");
    }

    #[test]
    fn test_toml_plugin_file_extensions() {
        let plugin = TomlPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["toml"]);
    }

    #[test]
    fn test_extract_simple_toml() {
        let plugin = TomlPlugin::new().unwrap();
        let source = b"name = \"test\"\nport = 8080\nenabled = true";
        let keys = plugin
            .extract_config_keys(Path::new("config.toml"), source)
            .unwrap();

        assert!(keys.len() >= 3);
        assert!(keys
            .iter()
            .any(|k| k.key_path == "name" && k.value == "test"));
        assert!(keys
            .iter()
            .any(|k| k.key_path == "port" && k.value_type == ConfigValueType::Number));
        assert!(keys
            .iter()
            .any(|k| k.key_path == "enabled" && k.value_type == ConfigValueType::Boolean));
    }

    #[test]
    fn test_extract_nested_toml() {
        let plugin = TomlPlugin::new().unwrap();
        let source = b"[server]\nhost = \"localhost\"\nport = 8080";
        let keys = plugin
            .extract_config_keys(Path::new("config.toml"), source)
            .unwrap();

        assert!(keys.iter().any(|k| k.key_path == "server.host"));
        assert!(keys.iter().any(|k| k.key_path == "server.port"));
    }
}
