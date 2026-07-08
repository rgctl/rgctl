//! Parallel processing pipeline
//!
//! Task 1.6.2: Parallel file parsing with rayon

use crate::parallel::par_filter_map;
use indicatif::{ProgressBar, ProgressStyle};
use rbuilder_error::Result;
use rbuilder_extraction::discovery::{DiscoveryConfig, FileDiscoverer};
use rbuilder_extraction::{Extractor, GraphBuilder};
use rbuilder_graph::code_graph::CodeGraph;
use rbuilder_graph::schema::{Edge, Node};
use rbuilder_registry::LanguageRegistry;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Options for the processing pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// File discovery configuration
    pub discovery: DiscoveryConfig,
    /// Show progress bar during processing
    pub show_progress: bool,
    /// Optional thread count for parallel extraction (defaults to rayon pool size)
    pub thread_count: Option<usize>,
    /// Batch size for parallel file processing
    pub batch_size: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            discovery: DiscoveryConfig::default(),
            show_progress: true,
            thread_count: None,
            batch_size: 64,
        }
    }
}

/// Statistics from a pipeline run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PipelineStats {
    /// Files discovered
    pub files_discovered: usize,
    /// Files successfully processed
    pub files_processed: usize,
    /// Files that failed extraction
    pub files_failed: usize,
    /// Nodes created in the graph
    pub nodes_created: usize,
    /// Edges created in the graph
    pub edges_created: usize,
    /// Total processing duration
    pub duration: Duration,
}

/// End-to-end repository processing pipeline.
pub struct ProcessingPipeline {
    registry: Arc<LanguageRegistry>,
    config: PipelineConfig,
}

impl ProcessingPipeline {
    /// Create a pipeline with default configuration.
    pub fn new(registry: Arc<LanguageRegistry>) -> Self {
        Self {
            registry,
            config: PipelineConfig::default(),
        }
    }

    /// Create a pipeline with custom configuration.
    pub fn with_config(registry: Arc<LanguageRegistry>, config: PipelineConfig) -> Self {
        Self { registry, config }
    }

    /// Discover, extract, and build a graph for a repository.
    pub fn process_repository(&self, root: &Path) -> Result<(CodeGraph, PipelineStats)> {
        let start = Instant::now();
        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let files = discoverer.discover(root)?;
        let files_discovered = files.len();

        let progress = if self.config.show_progress && files_discovered > 0 {
            let pb = ProgressBar::new(files_discovered as u64);
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("#>-"),
            );
            pb.set_message("extracting");
            Some(pb)
        } else {
            None
        };

        let extractor = Extractor::new(Arc::clone(&self.registry));
        let progress_ref = progress.as_ref();
        let extractions = par_filter_map(self.config.thread_count, &files, |path| {
            let result = extractor.extract_file(path);
            if let Some(pb) = progress_ref {
                pb.inc(1);
            }
            result.ok()
        });

        if let Some(pb) = progress {
            pb.finish_with_message("done");
        }

        let files_processed = extractions.len();
        let files_failed = files_discovered.saturating_sub(files_processed);

        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&extractions, &mut builder)?;
        let (nodes, edges): (Vec<Node>, Vec<Edge>) = builder.into_graph();

        let nodes_created = nodes.len();
        let edges_created = edges.len();
        let mut graph = CodeGraph::new();
        graph.load(nodes, edges)?;

        Ok((
            graph,
            PipelineStats {
                files_discovered,
                files_processed,
                files_failed,
                nodes_created,
                edges_created,
                duration: start.elapsed(),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_parallel_parsing() {
        let temp = TempDir::new().unwrap();
        for i in 0..10 {
            fs::write(
                temp.path().join(format!("file{i}.rs")),
                format!("fn func{i}() {{}}\n"),
            )
            .unwrap();
        }

        let config = PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        };
        let pipeline = ProcessingPipeline::with_config(
            Arc::new(rbuilder_languages::default_registry()),
            config,
        );
        let (graph, stats) = pipeline.process_repository(temp.path()).unwrap();

        assert_eq!(stats.files_processed, 10);
        assert!(graph.node_count() > 10);
    }
}
