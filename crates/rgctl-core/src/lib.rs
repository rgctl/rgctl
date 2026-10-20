//! rgctl core library facade — one dependency for graph, analysis, pipeline, and plugins.
#![warn(missing_docs)]

/// Process memory monitoring utilities.
pub mod memory;

/// Graph and program analysis algorithms (`rgctl-analysis`).
pub use rgctl_analysis as analysis;
/// Shared error types.
pub use rgctl_error::{Error, Result};
/// Export helpers for analysis artifacts.
pub use rgctl_export as export;
/// Language extraction and discovery.
pub use rgctl_extraction as extraction;
/// Graph query language (GQL).
pub use rgctl_gql as gql;
/// Graph storage and query layer.
pub use rgctl_graph as graph;
/// Incremental update pipeline.
pub use rgctl_incremental as incremental;
/// Multi-stage processing pipeline.
pub use rgctl_pipeline as pipeline;
/// Language plugin API types.
pub use rgctl_plugin_api as plugin;
/// Project configuration parsing.
pub use rgctl_project_config as config;
/// Language registry.
pub use rgctl_registry as registry;
/// Rule engine.
pub use rgctl_rules as rules;
/// Security scanning helpers.
pub use rgctl_security as security;
/// Semantic analysis (signatures, IDL).
pub use rgctl_semantic as semantic;

pub use rgctl_extraction::discovery;
pub use rgctl_graph::CodeGraph;
pub use rgctl_incremental::changes;
pub use rgctl_incremental::{
    ChangeDetail, ChangeDetectionResult, ChangeDetector, ChangeSet, ChangeSummary,
    incoming_callers_files, incoming_callers_files_depth,
    DEFAULT_BASE_ARTIFACT_SUBDIR, EntityScope, FileTracker, HunkIndex, IncrementalUpdater,
    LineRange, PrCheckArtifactPaths, RGCTL_BASE_ARTIFACT_ENV, ScopedPaths, SymbolScopeOptions,
    UpdateOptions, UpdateResult, changed_function_symbols, function_symbols_in_paths,
    HeadSynthesisOptions, default_synth_head_dir, git_diff_name_only, git_diff_name_status,
    git_unified_diff, merge_change_sets, parse_name_status_z, resolve_base_artifact_root,
    resolve_base_snapshot, resolve_pr_check_artifacts, resolve_snapshot_path,
    synthesize_head_snapshot, synthesize_worktree_head_snapshot,
};
pub use rgctl_pipeline::parallel;
pub use rgctl_pipeline::{PipelineConfig, PipelineStats, ProcessingPipeline, par_filter_map};
pub use rgctl_project_config::analyzer::{ConfigAnalyzer, MissingEnvVar, UnusedConfigKey};
pub use rgctl_project_config::drift::{
    ConfigDiffEntry, ConfigDiffKind, ConfigDriftReport, compare_configs, format_drift_report,
};
pub use rgctl_project_config::project::{HooksConfig, RgctlConfig, RiskLevel, WatchConfig};
pub use rgctl_project_config::secret_detector::{
    DetectedSecret, SecretDetector, Severity as SecretSeverity,
};
pub use rgctl_registry::LanguageRegistry;
pub use rgctl_rules::{RuleApplicationReport, RuleEngine, Ruleset};
pub use rgctl_semantic::{
    FunctionSignature, IdlFormat, IdlGenerator, SignatureExtractor, TypeInferencer,
};

/// Crate version string (matches `CARGO_PKG_VERSION`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
