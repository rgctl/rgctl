//! rBuilder - AI-Powered Code Knowledge Graph

#![warn(missing_docs)]
#![warn(clippy::all)]

pub use rbuilder_core::*;

pub mod analysis;
#[allow(missing_docs)]
pub mod cli;
pub mod graph;
pub mod languages;
pub mod security;

pub use rbuilder_error::{Error, Result};
pub use rbuilder_graph::CodeGraph;

/// Build information
pub const BUILD_INFO: &str = concat!(
    "rBuilder v",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);

/// Initialize workspace hooks (language registry builder).
pub fn init() {
    languages::ensure_registry_initialized();
}

/// Build a code graph from a repository using all built-in language plugins.
pub fn code_graph_from_repository(root: &std::path::Path) -> Result<CodeGraph> {
    use rbuilder_pipeline::ProcessingPipeline;
    use std::sync::Arc;

    languages::ensure_registry_initialized();
    let pipeline =
        ProcessingPipeline::new(Arc::new(languages::LanguageRegistry::new().into_inner()));
    let (graph, _) = pipeline.process_repository(root)?;
    Ok(graph)
}

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
