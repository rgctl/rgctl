//! rBuilder - AI-Powered Code Knowledge Graph
//!
//! rBuilder transforms code repositories into queryable knowledge graphs that AI agents
//! can interrogate via natural language, enabling accurate impact analysis, architecture
//! review, and refactoring.
//!
//! # Architecture
//!
//! - **Extraction**: Tree-sitter based AST parsing for 36+ languages
//! - **Graph**: IndraDB-backed graph storage with pluggable backends
//! - **Analysis**: Community detection, complexity metrics, centrality analysis
//! - **NLP**: Hybrid query system (pattern matching + optional LLM)
//! - **MCP**: Model Context Protocol server for AI agent integration
//!
//! # Example
//!
//! ```ignore
//! use rbuilder::CodeGraph;
//!
//! let graph = CodeGraph::from_repository("./my-project")?;
//! let functions = graph.query("functions")?;
//! println!("Found {} functions", functions.len());
//! # Ok::<(), rbuilder::Error>(())
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

// Core modules
pub mod error;

// Extraction layer
pub mod discovery;
pub mod extraction;
pub mod languages;

// Graph layer
pub mod graph;

// Graph query language (Phase 12.4)
pub mod gql;

// Security analysis (Phase 13.5)
pub mod security;

// Analysis layer
pub mod analysis;
pub mod config;

// NLP & Query layer
pub mod nlp;

// Integration layer
#[cfg(feature = "mcp-server")]
pub mod api;

#[cfg(feature = "mcp-server")]
pub mod mcp;

// Utility modules
pub mod cli;
pub mod incremental;
pub mod output;
pub mod pipeline;
pub mod parallel;
pub mod rules;
pub mod semantic;
pub mod multi_repo;

// Re-exports for convenience
pub use error::{Error, Result};
pub use graph::CodeGraph;
pub use pipeline::{PipelineConfig, PipelineStats, ProcessingPipeline};
pub use config::analyzer::{ConfigAnalyzer, MissingEnvVar, UnusedConfigKey};
pub use config::secret_detector::{DetectedSecret, SecretDetector, Severity as SecretSeverity};
pub use nlp::conversation::ConversationContext;
pub use nlp::{DomainContext, PatternDetector, PatternMatcher, QueryResult, TranslatedQuery};
pub use rules::{RuleApplicationReport, RuleEngine, Ruleset};
pub use semantic::{FunctionSignature, IdlFormat, IdlGenerator, SignatureExtractor, TypeInferencer};
pub use incremental::{ChangeSet, FileTracker, IncrementalUpdater, UpdateOptions, UpdateResult};
pub use config::drift::{compare_configs, format_drift_report, ConfigDiffEntry, ConfigDiffKind, ConfigDriftReport};
pub use multi_repo::{
    link_cross_repo, load_workspace_graph, stamp_repo_namespace, sync_workspace, CrossRepoLinkReport,
    RepoEntry, WorkspaceManifest, WorkspaceSyncReport, WORKSPACE_FILE,
};

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build information
pub const BUILD_INFO: &str = concat!(
    "rBuilder v",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_build_info() {
        assert!(BUILD_INFO.contains("rBuilder"));
    }
}
