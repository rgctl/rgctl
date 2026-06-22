//! Project configuration analysis

pub mod analyzer;
pub mod drift;
pub mod project;
pub mod secret_detector;

pub use analyzer::{ConfigAnalyzer, MissingEnvVar, UnusedConfigKey};
pub use drift::{
    compare_configs, format_drift_report, ConfigDiffEntry, ConfigDiffKind, ConfigDriftReport,
};
pub use project::{HooksConfig, RbuilderConfig, RiskLevel, WatchConfig};
pub use rbuilder_extraction::usage_detector::{ConfigConfidence, ConfigUsage, ConfigUsageDetector};
pub use secret_detector::{DetectedSecret, SecretDetector, Severity as SecretSeverity};
