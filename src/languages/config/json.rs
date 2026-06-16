//! JSON configuration format plugin

use crate::error::Result;
use crate::languages::plugin_trait::*;
use std::path::Path;

/// JSON config format plugin
pub struct JsonPlugin;

impl JsonPlugin {
    /// Create a new JSON plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn flatten_json_value(
        &self,
        value: &serde_json::Value,
        prefix: &str,
        file: &str,
        results: &mut Vec<ConfigKey>,
    ) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let full_key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    self.flatten_json_value(v, &full_key, file, results);
                }
            }
            serde_json::Value::Array(arr) => {
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
            serde_json::Value::String(s) => {
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
            serde_json::Value::Number(n) => {
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
            serde_json::Value::Bool(b) => {
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
            serde_json::Value::Null => {
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
        }
    }
}

impl Default for JsonPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create JsonPlugin")
    }
}

impl ConfigFormatPlugin for JsonPlugin {
    fn format_id(&self) -> &str {
        "json"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["json"]
    }

    fn extract_config_keys(&self, file_path: &Path, source: &[u8]) -> Result<Vec<ConfigKey>> {
        let content = std::str::from_utf8(source)?;
        let value: serde_json::Value = serde_json::from_str(content)?;

        let mut results = Vec::new();
        self.flatten_json_value(&value, "", &file_path.to_string_lossy(), &mut results);

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_plugin_format_id() {
        let plugin = JsonPlugin::new().unwrap();
        assert_eq!(plugin.format_id(), "json");
    }

    #[test]
    fn test_json_plugin_file_extensions() {
        let plugin = JsonPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["json"]);
    }

    #[test]
    fn test_extract_simple_json() {
        let plugin = JsonPlugin::new().unwrap();
        let source = br#"{"name": "test", "port": 8080, "enabled": true}"#;
        let keys = plugin.extract_config_keys(Path::new("config.json"), source).unwrap();

        assert!(keys.len() >= 3);
        assert!(keys.iter().any(|k| k.key_path == "name" && k.value == "test"));
        assert!(keys.iter().any(|k| k.key_path == "port" && k.value_type == ConfigValueType::Number));
        assert!(keys.iter().any(|k| k.key_path == "enabled" && k.value_type == ConfigValueType::Boolean));
    }

    #[test]
    fn test_extract_nested_json() {
        let plugin = JsonPlugin::new().unwrap();
        let source = br#"{"server": {"host": "localhost", "port": 8080}}"#;
        let keys = plugin.extract_config_keys(Path::new("config.json"), source).unwrap();

        assert!(keys.iter().any(|k| k.key_path == "server.host"));
        assert!(keys.iter().any(|k| k.key_path == "server.port"));
    }
}
