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
//! ```no_run
//! use rbuilder::Graph;
//!
//! // Initialize graph for a repository
//! let graph = Graph::from_repository("./my-project")?;
//!
//! // Query using natural language
//! let result = graph.query_nl("How many React components?")?;
//! println!("{}", result);
//! # Ok::<(), Box<dyn std::error::Error>>(())
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

// Analysis layer
pub mod analysis;
pub mod config;

// NLP & Query layer
pub mod nlp;

// Integration layer
#[cfg(feature = "mcp-server")]
pub mod mcp;

pub mod api;

// Utility modules
pub mod incremental;
pub mod output;
pub mod pipeline;
pub mod rules;
pub mod semantic;

// Re-exports for convenience
pub use error::{Error, Result};

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
