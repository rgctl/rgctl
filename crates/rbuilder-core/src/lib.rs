//! rBuilder core library facade — one dependency for graph, analysis, pipeline, and plugins.
#![warn(missing_docs)]

/// Process memory monitoring utilities.
pub mod memory;

/// Graph and program analysis algorithms (`rbuilder-analysis`).
pub use rbuilder_analysis as analysis;
/// Shared error types.
pub use rbuilder_error::{Error, Result};
/// Export helpers for analysis artifacts.
pub use rbuilder_export as export;
/// Language extraction and discovery.
pub use rbuilder_extraction as extraction;
/// Graph query language (GQL).
pub use rbuilder_gql as gql;
/// Graph storage and query layer.
pub use rbuilder_graph as graph;
/// Incremental update pipeline.
pub use rbuilder_incremental as incremental;
/// Multi-stage processing pipeline.
pub use rbuilder_pipeline as pipeline;
/// Language plugin API types.
pub use rbuilder_plugin_api as plugin;
/// Project configuration parsing.
pub use rbuilder_project_config as config;
/// Language registry.
pub use rbuilder_registry as registry;
/// Rule engine.
pub use rbuilder_rules as rules;
/// Security scanning helpers.
pub use rbuilder_security as security;
/// Semantic analysis (signatures, IDL).
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

/// Crate version string (matches `CARGO_PKG_VERSION`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
