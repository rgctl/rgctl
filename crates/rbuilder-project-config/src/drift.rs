//! Configuration drift detection across environments
//!
//! Task 7.3.1: Compare flattened config key paths between files.

use rbuilder_error::{Error, Result};
use rbuilder_plugin_api::ConfigKey;
use rbuilder_registry::LanguageRegistry;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A single config key difference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDiffEntry {
    /// Dot-separated key path
    pub key: String,
    /// Value in the left (baseline) file, if present
    pub left_value: Option<String>,
    /// Value in the right (comparison) file, if present
    pub right_value: Option<String>,
    /// Kind of difference
    pub kind: ConfigDiffKind,
}

/// Type of configuration drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigDiffKind {
    /// Key exists only in the left file
    Removed,
    /// Key exists only in the right file
    Added,
    /// Key exists in both but values differ
    Changed,
}

/// Full drift report between two config files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDriftReport {
    /// Left/baseline file path
    pub left: PathBuf,
    /// Right/comparison file path
    pub right: PathBuf,
    /// Keys only in left
    pub removed: Vec<ConfigDiffEntry>,
    /// Keys only in right
    pub added: Vec<ConfigDiffEntry>,
    /// Keys with different values
    pub changed: Vec<ConfigDiffEntry>,
}

impl ConfigDriftReport {
    /// Total number of differences.
    pub fn total_differences(&self) -> usize {
        self.removed.len() + self.added.len() + self.changed.len()
    }

    /// Whether configs are identical (no drift).
    pub fn is_clean(&self) -> bool {
        self.total_differences() == 0
    }
}

/// Compare two configuration files and produce a drift report.
pub fn compare_configs(left: &Path, right: &Path) -> Result<ConfigDriftReport> {
    let registry = rbuilder_registry::full_registry();
    let left_keys = load_config_keys(&registry, left)?;
    let right_keys = load_config_keys(&registry, right)?;

    let left_map = keys_to_map(&left_keys);
    let right_map = keys_to_map(&right_keys);

    let left_keys_set: BTreeSet<_> = left_map.keys().cloned().collect();
    let right_keys_set: BTreeSet<_> = right_map.keys().cloned().collect();

    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut changed = Vec::new();

    for key in left_keys_set.difference(&right_keys_set) {
        removed.push(ConfigDiffEntry {
            key: key.clone(),
            left_value: left_map.get(key).cloned(),
            right_value: None,
            kind: ConfigDiffKind::Removed,
        });
    }

    for key in right_keys_set.difference(&left_keys_set) {
        added.push(ConfigDiffEntry {
            key: key.clone(),
            left_value: None,
            right_value: right_map.get(key).cloned(),
            kind: ConfigDiffKind::Added,
        });
    }

    for key in left_keys_set.intersection(&right_keys_set) {
        let lv = left_map.get(key).map(String::as_str).unwrap_or("");
        let rv = right_map.get(key).map(String::as_str).unwrap_or("");
        if lv != rv {
            changed.push(ConfigDiffEntry {
                key: key.clone(),
                left_value: left_map.get(key).cloned(),
                right_value: right_map.get(key).cloned(),
                kind: ConfigDiffKind::Changed,
            });
        }
    }

    Ok(ConfigDriftReport {
        left: left.to_path_buf(),
        right: right.to_path_buf(),
        removed,
        added,
        changed,
    })
}

fn load_config_keys(registry: &LanguageRegistry, path: &Path) -> Result<Vec<ConfigKey>> {
    let plugin = registry.get_config_plugin_for_file(path).map_err(|_| {
        Error::UnsupportedLanguage(format!(
            "No config parser for {}. Supported: yaml, yml, json, toml, properties",
            path.display()
        ))
    })?;

    let source = std::fs::read(path)?;
    Ok(plugin.extract_config_keys(path, &source)?)
}

fn keys_to_map(keys: &[ConfigKey]) -> BTreeMap<String, String> {
    keys.iter()
        .map(|k| (k.key_path.clone(), k.value.clone()))
        .collect()
}

/// Format a drift report for CLI display.
pub fn format_drift_report(report: &ConfigDriftReport) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Config drift: {} vs {}",
        report.left.display(),
        report.right.display()
    );

    if report.is_clean() {
        let _ = writeln!(out, "\n✅ No drift detected — configs match.");
        return out;
    }

    let _ = writeln!(
        out,
        "\n⚠️  {} difference(s): {} removed, {} added, {} changed\n",
        report.total_differences(),
        report.removed.len(),
        report.added.len(),
        report.changed.len()
    );

    if !report.removed.is_empty() {
        let _ = writeln!(out, "🔴 REMOVED (in baseline only):");
        for entry in &report.removed {
            let _ = writeln!(
                out,
                "   - {} = {}",
                entry.key,
                entry.left_value.as_deref().unwrap_or("?")
            );
        }
        let _ = writeln!(out);
    }

    if !report.added.is_empty() {
        let _ = writeln!(out, "🟢 ADDED (in comparison only):");
        for entry in &report.added {
            let _ = writeln!(
                out,
                "   + {} = {}",
                entry.key,
                entry.right_value.as_deref().unwrap_or("?")
            );
        }
        let _ = writeln!(out);
    }

    if !report.changed.is_empty() {
        let _ = writeln!(out, "⚠️  CHANGED:");
        for entry in &report.changed {
            let _ = writeln!(
                out,
                "   ~ {} : {:?} → {:?}",
                entry.key,
                entry.left_value.as_deref().unwrap_or("?"),
                entry.right_value.as_deref().unwrap_or("?")
            );
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_config_drift_detection() {
        let temp = TempDir::new().unwrap();
        let prod = temp.path().join("prod.yaml");
        let dev = temp.path().join("dev.yaml");

        std::fs::File::create(&prod)
            .unwrap()
            .write_all(b"server:\n  port: 8080\n  host: prod.example.com\ndatabase:\n  pool: 10\n")
            .unwrap();
        std::fs::File::create(&dev)
            .unwrap()
            .write_all(
                b"server:\n  port: 3000\n  host: localhost\ndatabase:\n  pool: 10\n  debug: true\n",
            )
            .unwrap();

        let report = compare_configs(&prod, &dev).unwrap();
        assert!(!report.is_clean());
        assert!(report.changed.iter().any(|e| e.key == "server.port"));
        assert!(report.changed.iter().any(|e| e.key == "server.host"));
        assert!(report.added.iter().any(|e| e.key == "database.debug"));
        assert_eq!(report.removed.len(), 0);
    }

    #[test]
    fn test_identical_configs() {
        let temp = TempDir::new().unwrap();
        let a = temp.path().join("a.json");
        let b = temp.path().join("b.json");
        std::fs::write(&a, r#"{"server":{"port":8080}}"#).unwrap();
        std::fs::write(&b, r#"{"server":{"port":8080}}"#).unwrap();

        let report = compare_configs(&a, &b).unwrap();
        assert!(report.is_clean());
    }
}
