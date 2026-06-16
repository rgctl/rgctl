//! YAML configuration format plugin

use crate::error::Result;
use crate::languages::plugin_trait::*;
use std::path::Path;

/// YAML config format plugin
pub struct YamlPlugin;

impl YamlPlugin {
    /// Create a new YAML plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn flatten_yaml_value(
        &self,
        value: &serde_yaml::Value,
        prefix: &str,
        file: &str,
        results: &mut Vec<ConfigKey>,
    ) {
        match value {
            serde_yaml::Value::Mapping(map) => {
                for (k, v) in map {
                    if let serde_yaml::Value::String(key) = k {
                        let full_key = if prefix.is_empty() {
                            key.clone()
                        } else {
                            format!("{}.{}", prefix, key)
                        };
                        self.flatten_yaml_value(v, &full_key, file, results);
                    }
                }
            }
            serde_yaml::Value::Sequence(arr) => {
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
            serde_yaml::Value::String(s) => {
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
            serde_yaml::Value::Number(n) => {
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
            serde_yaml::Value::Bool(b) => {
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
            serde_yaml::Value::Null => {
                results.push(ConfigKey {
                    key_path: prefix.to_string(),
                    value: "null".to_string(),
                    value_type: ConfigValueType::Null,
                    location: SourceLocation {
                        file: file.to_string(),
                        start_line: 0,
                        end_line: 0,
                        start_column: 0,
                        end_column: 0,
                    },
                });
            }
            _ => {}
        }
    }
}

impl Default for YamlPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create YamlPlugin")
    }
}

impl ConfigFormatPlugin for YamlPlugin {
    fn format_id(&self) -> &str {
        "yaml"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["yaml", "yml"]
    }

    fn extract_config_keys(&self, file_path: &Path, source: &[u8]) -> Result<Vec<ConfigKey>> {
        let content = std::str::from_utf8(source)?;
        let value: serde_yaml::Value = serde_yaml::from_str(content)?;

        let mut results = Vec::new();
        self.flatten_yaml_value(&value, "", &file_path.to_string_lossy(), &mut results);

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_plugin_format_id() {
        let plugin = YamlPlugin::new().unwrap();
        assert_eq!(plugin.format_id(), "yaml");
    }

    #[test]
    fn test_yaml_plugin_file_extensions() {
        let plugin = YamlPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["yaml", "yml"]);
    }

    #[test]
    fn test_extract_simple_yaml() {
        let plugin = YamlPlugin::new().unwrap();
        let source = b"name: test\nport: 8080\nenabled: true";
        let keys = plugin.extract_config_keys(Path::new("config.yaml"), source).unwrap();

        assert!(keys.len() >= 3);
        assert!(keys.iter().any(|k| k.key_path == "name" && k.value == "test"));
        assert!(keys.iter().any(|k| k.key_path == "port" && k.value_type == ConfigValueType::Number));
        assert!(keys.iter().any(|k| k.key_path == "enabled" && k.value_type == ConfigValueType::Boolean));
    }

    #[test]
    fn test_extract_nested_yaml() {
        let plugin = YamlPlugin::new().unwrap();
        let source = b"server:\n  host: localhost\n  port: 8080";
        let keys = plugin.extract_config_keys(Path::new("config.yaml"), source).unwrap();

        assert!(keys.iter().any(|k| k.key_path == "server.host"));
        assert!(keys.iter().any(|k| k.key_path == "server.port"));
    }
}
