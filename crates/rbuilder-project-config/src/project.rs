//! Project configuration (`rbuilder.toml`).

use rbuilder_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Risk level for pre-commit hook blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    /// Block only critical risk.
    #[default]
    Critical,
    /// Block high and critical risk.
    High,
    /// Block medium, high, and critical risk.
    Medium,
    /// Block all non-low risk.
    Low,
}

impl RiskLevel {
    /// Whether `actual` meets or exceeds this threshold.
    pub fn blocks(&self, actual: RiskLevel) -> bool {
        actual.severity() >= self.severity()
    }

    fn severity(self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }
}

/// Git hook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Install and enable the pre-commit hook.
    #[serde(default = "default_true")]
    pub pre_commit: bool,
    /// Install and enable the post-commit hook.
    #[serde(default = "default_true")]
    pub post_commit: bool,
    /// Install and enable the post-checkout hook.
    #[serde(default = "default_true")]
    pub post_checkout: bool,
    /// Minimum risk level that blocks commits.
    #[serde(default)]
    pub block_on_risk: RiskLevel,
    /// Blast-radius impact zone size that escalates risk.
    #[serde(default = "default_blast_threshold")]
    pub blast_radius_threshold: usize,
}

fn default_true() -> bool {
    true
}

fn default_blast_threshold() -> usize {
    50
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            pre_commit: true,
            post_commit: true,
            post_checkout: true,
            block_on_risk: RiskLevel::Critical,
            blast_radius_threshold: default_blast_threshold(),
        }
    }
}

/// Watch mode configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    /// Debounce window for rapid file changes (milliseconds).
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

fn default_debounce_ms() -> u64 {
    500
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce_ms: default_debounce_ms(),
        }
    }
}

/// Root project configuration file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RbuilderConfig {
    /// Git hook settings.
    #[serde(default)]
    pub hooks: HooksConfig,
    /// Watch mode settings.
    #[serde(default)]
    pub watch: WatchConfig,
}

impl RbuilderConfig {
    /// Load `rbuilder.toml` from the repository root, or return defaults.
    pub fn load(repo_root: &Path) -> Result<Self> {
        let path = Self::config_path(repo_root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| Error::Other(format!("Failed to read {}: {e}", path.display())))?;
        toml::from_str(&raw).map_err(|e| Error::Other(format!("Invalid rbuilder.toml: {e}")))
    }

    /// Write configuration to `rbuilder.toml`.
    pub fn save(&self, repo_root: &Path) -> Result<PathBuf> {
        let path = Self::config_path(repo_root);
        let raw = toml::to_string_pretty(self)
            .map_err(|e| Error::Other(format!("Failed to serialize config: {e}")))?;
        std::fs::write(&path, raw)
            .map_err(|e| Error::Other(format!("Failed to write {}: {e}", path.display())))?;
        Ok(path)
    }

    /// Path to the project config file.
    pub fn config_path(repo_root: &Path) -> PathBuf {
        repo_root.join("rbuilder.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_roundtrip() {
        let temp = TempDir::new().unwrap();
        let mut cfg = RbuilderConfig::default();
        cfg.hooks.block_on_risk = RiskLevel::High;
        cfg.watch.debounce_ms = 250;
        let path = cfg.save(temp.path()).unwrap();
        assert!(path.exists());
        let loaded = RbuilderConfig::load(temp.path()).unwrap();
        assert_eq!(loaded.hooks.block_on_risk, RiskLevel::High);
        assert_eq!(loaded.watch.debounce_ms, 250);
    }

    #[test]
    fn test_risk_level_blocks() {
        assert!(RiskLevel::Critical.blocks(RiskLevel::Critical));
        assert!(RiskLevel::High.blocks(RiskLevel::Critical));
        assert!(!RiskLevel::Critical.blocks(RiskLevel::High));
    }
}
