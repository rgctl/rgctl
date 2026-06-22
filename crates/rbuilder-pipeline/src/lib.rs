//! Parallel processing pipeline

pub mod parallel;
mod pipeline;

pub use parallel::par_filter_map;
pub use pipeline::{PipelineConfig, PipelineStats, ProcessingPipeline};

use rbuilder_error::Result;
use rbuilder_graph::CodeGraph;
use std::path::Path;
use std::sync::Arc;

/// Build a code graph from a repository path using the default registry and pipeline.
pub fn code_graph_from_repository(root: &Path) -> Result<CodeGraph> {
    let pipeline = ProcessingPipeline::new(Arc::new(rbuilder_registry::full_registry()));
    let (graph, _) = pipeline.process_repository(root)?;
    Ok(graph)
}
