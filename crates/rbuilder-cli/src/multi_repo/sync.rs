//! Workspace sync: index multiple repos into one graph

use crate::multi_repo::linker::{link_cross_repo, CrossRepoLinkReport};
use crate::multi_repo::workspace::{stamp_repo_namespace, WorkspaceManifest};
use rbuilder_error::Result;
use rbuilder_extraction::discovery::DiscoveryConfig;
use rbuilder_graph::CodeGraph;
use rbuilder_incremental::FileTracker;
use rbuilder_pipeline::{PipelineConfig, ProcessingPipeline};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// Result of syncing a multi-repo workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceSyncReport {
    /// Repos indexed
    pub repos_indexed: usize,
    /// Total nodes in merged graph
    pub nodes: usize,
    /// Total edges in merged graph
    pub edges: usize,
    /// Cross-repo linking report
    pub cross_repo: CrossRepoLinkReport,
    /// Duration in seconds
    pub duration_secs: f64,
}

/// Index all repos in a workspace and merge into a single graph.
pub fn sync_workspace(
    workspace_root: &Path,
    show_progress: bool,
) -> Result<(CodeGraph, WorkspaceSyncReport)> {
    let start = Instant::now();
    let manifest = WorkspaceManifest::load(workspace_root)?;

    if manifest.repos.is_empty() {
        return Err(rbuilder_error::Error::InvalidQuery(
            "Workspace has no repos. Use `rbuilder workspace add` first.".into(),
        ));
    }

    let registry = Arc::new(rbuilder_registry::full_registry());
    let mut merged = CodeGraph::new();

    for entry in &manifest.repos {
        let repo_path = manifest.resolve_path(entry, workspace_root);
        if !repo_path.exists() {
            tracing::warn!("Skipping missing repo path: {}", repo_path.display());
            continue;
        }

        let pipeline = ProcessingPipeline::with_config(
            Arc::clone(&registry),
            PipelineConfig {
                discovery: DiscoveryConfig::default(),
                show_progress,
                ..PipelineConfig::default()
            },
        );

        let (mut graph, _) = pipeline.process_repository(&repo_path)?;
        stamp_repo_namespace(&mut graph, &entry.namespace);

        let nodes = graph.backend().all_nodes()?;
        let edges = graph.backend().all_edges()?;
        merged.load(nodes, edges)?;

        // Per-repo file tracker (optional, for incremental updates within repo)
        let mut tracker = FileTracker::new(&repo_path);
        let discoverer = rbuilder_extraction::discovery::FileDiscoverer::new(Arc::clone(&registry));
        if let Ok(files) = discoverer.discover(&repo_path) {
            let _ = tracker.index_files(&files, &graph);
            let _ = tracker.save();
        }

        graph.save_to_repo(&repo_path)?;
    }

    let cross_repo = link_cross_repo(merged.backend_mut())?;
    merged.save_to_repo(workspace_root)?;

    let report = WorkspaceSyncReport {
        repos_indexed: manifest.repos.len(),
        nodes: merged.node_count(),
        edges: merged.edge_count(),
        cross_repo,
        duration_secs: start.elapsed().as_secs_f64(),
    };

    Ok((merged, report))
}
