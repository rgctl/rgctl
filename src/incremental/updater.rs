//! Incremental graph updater
//!
//! Task 5.1.2: Update graph for changed files only (CRITICAL)

use crate::discovery::{DiscoveryConfig, FileDiscoverer};
use crate::error::Result;
use crate::extraction::extractor::Extractor;
use crate::extraction::graph_builder::GraphBuilder;
use crate::graph::code_graph::CodeGraph;
use crate::graph::schema::EdgeType;
use crate::incremental::file_tracker::{
    git_changed_files, relative_path, resolve_path, ChangeSet, FileTracker,
};
use crate::languages::registry::LanguageRegistry;
use crate::parallel::{par_filter_map, par_map};
use crate::pipeline::{PipelineConfig, ProcessingPipeline};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Options for an incremental update run.
#[derive(Debug, Clone, Default)]
pub struct UpdateOptions {
    /// Update files changed since this git commit ref
    pub since: Option<String>,
    /// Force a full rebuild instead of incremental update
    pub force: bool,
    /// File discovery configuration
    pub discovery: DiscoveryConfig,
    /// Show progress during update
    pub show_progress: bool,
    /// Optional thread count for parallel extraction
    pub thread_count: Option<usize>,
}

/// Summary of an incremental update operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateResult {
    /// Files added since last index
    pub files_added: usize,
    /// Files modified since last index
    pub files_changed: usize,
    /// Files deleted since last index
    pub files_deleted: usize,
    /// Nodes removed from the graph
    pub nodes_removed: usize,
    /// Nodes added to the graph
    pub nodes_added: usize,
    /// Edges removed from the graph
    pub edges_removed: usize,
    /// Edges added to the graph
    pub edges_added: usize,
    /// Total update duration
    pub duration: Duration,
}

impl UpdateResult {
    /// Total number of files touched by the update.
    pub fn files_affected(&self) -> usize {
        self.files_added + self.files_changed + self.files_deleted
    }
}

/// Performs incremental graph updates for changed files.
pub struct IncrementalUpdater {
    registry: Arc<LanguageRegistry>,
    config: UpdateOptions,
}

impl IncrementalUpdater {
    /// Create an updater with default options.
    pub fn new(registry: Arc<LanguageRegistry>) -> Self {
        Self {
            registry,
            config: UpdateOptions::default(),
        }
    }

    /// Create an updater with custom options.
    pub fn with_options(registry: Arc<LanguageRegistry>, config: UpdateOptions) -> Self {
        Self { registry, config }
    }

    /// Update the graph for a repository, returning statistics.
    pub fn update(&self, graph: &mut CodeGraph, repo_root: &Path) -> Result<UpdateResult> {
        let start = Instant::now();

        if self.config.force {
            return self.full_rebuild(graph, repo_root, start);
        }

        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let all_files = discoverer.discover(repo_root)?;

        let changes = if let Some(ref since) = self.config.since {
            self.changes_from_git(repo_root, since, &all_files)?
        } else {
            let tracker = FileTracker::load(repo_root)?;
            tracker.detect_changes(&all_files)?
        };

        if changes.is_empty() {
            return Ok(UpdateResult {
                duration: start.elapsed(),
                ..Default::default()
            });
        }

        self.apply_changes(graph, repo_root, &all_files, changes, start)
    }

    fn full_rebuild(
        &self,
        graph: &mut CodeGraph,
        repo_root: &Path,
        start: Instant,
    ) -> Result<UpdateResult> {
        let pipeline = ProcessingPipeline::with_config(
            Arc::clone(&self.registry),
            PipelineConfig {
                discovery: self.config.discovery.clone(),
                show_progress: self.config.show_progress,
                thread_count: self.config.thread_count,
                ..PipelineConfig::default()
            },
        );

        let (new_graph, stats) = pipeline.process_repository(repo_root)?;
        let nodes_removed = graph.node_count();
        let edges_removed = graph.edge_count();

        *graph = new_graph;

        let mut tracker = FileTracker::new(repo_root);
        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let files = discoverer.discover(repo_root)?;
        tracker.index_files(&files, graph)?;
        tracker.save()?;
        graph.save_to_repo(repo_root)?;

        Ok(UpdateResult {
            files_changed: stats.files_processed,
            nodes_removed,
            nodes_added: graph.node_count(),
            edges_removed,
            edges_added: graph.edge_count(),
            duration: start.elapsed(),
            ..Default::default()
        })
    }

    fn changes_from_git(
        &self,
        repo_root: &Path,
        since: &str,
        all_files: &[PathBuf],
    ) -> Result<ChangeSet> {
        let git_files = git_changed_files(repo_root, since)?;
        let git_set: std::collections::HashSet<String> = git_files
            .iter()
            .filter_map(|p| relative_path(repo_root, p).ok())
            .collect();

        let tracker = FileTracker::load(repo_root)?;
        let hash_changes = tracker.detect_changes(all_files)?;

        let mut added = hash_changes.added;
        let mut changed = hash_changes.changed;
        let deleted = hash_changes.deleted;

        for rel in git_set {
            if !added.contains(&rel) && !changed.contains(&rel) && !deleted.contains(&rel) {
                if tracker.file_hashes().contains_key(&rel) {
                    if !changed.contains(&rel) {
                        changed.push(rel);
                    }
                } else {
                    added.push(rel);
                }
            }
        }

        Ok(ChangeSet {
            added,
            changed,
            deleted,
        })
    }

    fn apply_changes(
        &self,
        graph: &mut CodeGraph,
        repo_root: &Path,
        all_files: &[PathBuf],
        changes: ChangeSet,
        start: Instant,
    ) -> Result<UpdateResult> {
        let mut result = UpdateResult {
            files_added: changes.added.len(),
            files_changed: changes.changed.len(),
            files_deleted: changes.deleted.len(),
            ..Default::default()
        };

        let progress = if self.config.show_progress && !changes.is_empty() {
            let pb = ProgressBar::new(changes.len() as u64);
            pb.set_style(
                ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}")
                    .unwrap(),
            );
            pb.set_message("updating");
            Some(pb)
        } else {
            None
        };

        let edges_before = graph.edge_count();

        for rel in &changes.deleted {
            let removed = graph.backend_mut().remove_nodes_for_file(rel)?;
            result.nodes_removed += removed;
            if let Some(pb) = &progress {
                pb.inc(1);
            }
        }

        let mut paths_to_update: Vec<PathBuf> = changes
            .added
            .iter()
            .chain(changes.changed.iter())
            .map(|rel| resolve_path(repo_root, rel))
            .collect();

        paths_to_update.sort();
        paths_to_update.dedup();

        for path in &paths_to_update {
            if let Ok(rel) = relative_path(repo_root, path) {
                let removed = graph.backend_mut().remove_nodes_for_file(&rel)?;
                result.nodes_removed += removed;
            }
            if let Some(pb) = &progress {
                pb.inc(1);
            }
        }

        if let Some(pb) = progress {
            pb.finish_with_message("rebuilding relations");
        }

        let extractor = Extractor::new(Arc::clone(&self.registry));
        let extractions = par_filter_map(self.config.thread_count, &paths_to_update, |path| {
            extractor.extract_file(path).ok()
        });

        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&extractions, &mut builder)?;
        let (new_nodes, new_edges) = builder.into_graph();
        let nodes_added = new_nodes.len();
        let edges_added = new_edges.len();

        {
            let backend = graph.backend_mut();
            backend.insert_nodes_batch(new_nodes)?;
            backend.insert_edges_batch(new_edges)?;
        }

        result.nodes_added = nodes_added;
        let relation_edges = self.rebuild_relations(graph, repo_root, &paths_to_update)?;
        result.edges_added = edges_added + relation_edges;
        result.edges_removed = edges_before.saturating_sub(graph.edge_count());

        let mut tracker = FileTracker::new(repo_root);
        tracker.index_files(all_files, graph)?;
        tracker.save()?;
        graph.save_to_repo(repo_root)?;

        result.duration = start.elapsed();
        Ok(result)
    }

    /// Re-extract relations for updated files and add missing edges.
    fn rebuild_relations(
        &self,
        graph: &mut CodeGraph,
        repo_root: &Path,
        changed_files: &[PathBuf],
    ) -> Result<usize> {
        if changed_files.is_empty() {
            return Ok(0);
        }

        let extractor = Extractor::new(Arc::clone(&self.registry));
        let symbol_index = graph.backend().build_symbol_index();
        let repo_root = repo_root.to_path_buf();

        let pending_edges = par_map(self.config.thread_count, changed_files, |path| {
            let Ok(extraction) = extractor.extract_file(path) else {
                return Vec::new();
            };

            let file = relative_path(&repo_root, path)
                .unwrap_or_else(|_| extraction.path.to_string_lossy().to_string());

            let mut batch = Vec::new();
            for relation in extraction.relations {
                let from_id = resolve_symbol(&symbol_index, &relation.from, &file);
                let to_id = resolve_symbol(&symbol_index, &relation.to, &file);

                if let (Some(from), Some(to)) = (from_id, to_id) {
                    batch.push((from, to, relation_type_to_edge(relation.relation_type)));
                }
            }
            batch
        });

        let mut edges_to_add = Vec::new();
        for batch in pending_edges {
            edges_to_add.extend(batch);
        }

        let mut added = 0usize;
        if !edges_to_add.is_empty() {
            let backend = graph.backend_mut();
            let mut new_edges = Vec::new();
            for (from, to, edge_type) in edges_to_add {
                if !backend.has_edge(from, to, edge_type) {
                    new_edges.push(crate::graph::schema::Edge::new(from, to, edge_type));
                    added += 1;
                }
            }
            backend.insert_edges_batch(new_edges)?;
            graph.backend_mut().prune_orphan_edges();
        }

        Ok(added)
    }
}

fn resolve_symbol(index: &HashMap<String, Uuid>, name: &str, file: &str) -> Option<Uuid> {
    let qualified = format!("{file}::{name}");
    if let Some(id) = index.get(&qualified) {
        return Some(*id);
    }
    index
        .iter()
        .find(|(k, _)| k.ends_with(&format!("::{name}")))
        .map(|(_, id)| *id)
}

fn relation_type_to_edge(
    relation_type: crate::languages::plugin_trait::RelationType,
) -> EdgeType {
    use crate::languages::plugin_trait::RelationType;
    match relation_type {
        RelationType::Calls => EdgeType::Calls,
        RelationType::Uses => EdgeType::Uses,
        RelationType::Implements => EdgeType::Implements,
        RelationType::Extends => EdgeType::Extends,
        RelationType::Defines => EdgeType::Contains,
        RelationType::References => EdgeType::References,
        RelationType::Instantiates => EdgeType::Instantiates,
        RelationType::Modifies => EdgeType::Modifies,
        RelationType::DependsOn => EdgeType::DependsOn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::schema::NodeType;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_incremental_update() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let main = root.join("src/main.rs");
        fs::create_dir_all(main.parent().unwrap()).unwrap();
        fs::write(&main, "fn main() {}\n").unwrap();

        let pipeline = ProcessingPipeline::new(Arc::new(LanguageRegistry::new()));
        let (mut graph, _) = pipeline.process_repository(root).unwrap();
        let initial_count = graph.node_count();

        let mut tracker = FileTracker::new(root);
        let files = vec![main.clone()];
        tracker.index_files(&files, &graph).unwrap();
        tracker.save().unwrap();
        graph.save_to_repo(root).unwrap();

        fs::write(&main, "fn main() { helper(); }\nfn helper() {}\n").unwrap();

        let updater = IncrementalUpdater::new(Arc::new(LanguageRegistry::new()));
        let result = updater.update(&mut graph, root).unwrap();

        assert!(result.files_changed >= 1 || result.nodes_added > 0);
        let delta = (graph.node_count() as i32 - initial_count as i32).unsigned_abs();
        assert!(delta < 10, "node count delta too large: {delta}");
    }

    #[test]
    fn test_force_rebuild() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(root.join("lib.rs"), "fn alpha() {}\n").unwrap();

        let registry = Arc::new(LanguageRegistry::new());
        let pipeline = ProcessingPipeline::new(Arc::clone(&registry));
        let (mut graph, _) = pipeline.process_repository(root).unwrap();

        fs::write(root.join("lib.rs"), "fn beta() {}\n").unwrap();

        let updater = IncrementalUpdater::with_options(
            registry,
            UpdateOptions {
                force: true,
                show_progress: false,
                ..Default::default()
            },
        );
        let result = updater.update(&mut graph, root).unwrap();
        assert!(result.nodes_added > 0);

        let functions = graph.find_by_type(NodeType::Function).unwrap();
        assert!(functions.iter().any(|n| n.name == "beta"));
    }
}
