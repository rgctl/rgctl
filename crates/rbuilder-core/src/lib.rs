//! rBuilder core library facade

pub mod memory;

pub use rbuilder_analysis as analysis;
pub use rbuilder_error::{Error, Result};
pub use rbuilder_export as export;
pub use rbuilder_extraction as extraction;
pub use rbuilder_gql as gql;
pub use rbuilder_graph as graph;
pub use rbuilder_incremental as incremental;
pub use rbuilder_pipeline as pipeline;
pub use rbuilder_plugin_api as plugin;
pub use rbuilder_project_config as config;
pub use rbuilder_registry as registry;
pub use rbuilder_rules as rules;
pub use rbuilder_security as security;
pub use rbuilder_semantic as semantic;

pub use rbuilder_extraction::discovery;
pub use rbuilder_graph::CodeGraph;
pub use rbuilder_incremental::changes;
pub use rbuilder_incremental::{
    ChangeDetail, ChangeDetectionResult, ChangeDetector, ChangeSet, ChangeSummary, FileTracker,
    IncrementalUpdater, UpdateOptions, UpdateResult,
};
pub use rbuilder_pipeline::parallel;
pub use rbuilder_pipeline::{par_filter_map, PipelineConfig, PipelineStats, ProcessingPipeline};
pub use rbuilder_project_config::analyzer::{ConfigAnalyzer, MissingEnvVar, UnusedConfigKey};
pub use rbuilder_project_config::drift::{
    compare_configs, format_drift_report, ConfigDiffEntry, ConfigDiffKind, ConfigDriftReport,
};
pub use rbuilder_project_config::project::{HooksConfig, RbuilderConfig, RiskLevel, WatchConfig};
pub use rbuilder_project_config::secret_detector::{
    DetectedSecret, SecretDetector, Severity as SecretSeverity,
};
pub use rbuilder_registry::LanguageRegistry;
pub use rbuilder_rules::{RuleApplicationReport, RuleEngine, Ruleset};
pub use rbuilder_semantic::{
    FunctionSignature, IdlFormat, IdlGenerator, SignatureExtractor, TypeInferencer,
};

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
