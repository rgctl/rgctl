//! Incremental graph updater
//!
//! Task 5.1.2: Update graph for changed files only (CRITICAL)

use crate::cascade::incoming_callers_files_depth;
use crate::file_tracker::{ChangeSet, FileTracker, merge_change_sets, relative_path, resolve_path};
use crate::pr_scope::git_diff_name_status;
use indicatif::{ProgressBar, ProgressStyle};
use rgctl_analysis::{
    BlastEngineSnapshot, CfgPdgArchive, MacroCallIndex, MacroCallLookupDb, SemanticIndex,
};
use rgctl_error::Result;
use rgctl_extraction::discovery::{DiscoveryConfig, FileDiscoverer};
use rgctl_extraction::{Extractor, GraphBuilder};
use rgctl_graph::code_graph::CodeGraph;
use rgctl_graph::schema::EdgeType;
use rgctl_graph::{
    DeltaSegment, MmappedGraphSnapshot, compact_repo_snapshot, compact_snapshot_file,
    write_columnar_from_backend,
};
use rgctl_pipeline::parallel::par_map;
use rgctl_pipeline::stream::{DEFAULT_STREAM_CHANNEL_CAPACITY, stream_into_graph};
use rgctl_pipeline::{PipelineConfig, ProcessingPipeline};
use rgctl_registry::LanguageRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Options for an incremental update run.
#[derive(Debug, Clone)]
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
    /// Reverse call-dependency hops to re-extract when callees change (0 = disabled).
    pub cascade_depth: usize,
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

impl Default for UpdateOptions {
    fn default() -> Self {
        Self {
            since: None,
            force: false,
            discovery: DiscoveryConfig::default(),
            show_progress: false,
            thread_count: None,
            cascade_depth: 1,
        }
    }
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

    /// Apply a pre-built git or tracker [`ChangeSet`] to the repo snapshot.
    pub fn apply_change_set(
        &self,
        graph: &mut CodeGraph,
        repo_root: &Path,
        changes: ChangeSet,
    ) -> Result<UpdateResult> {
        let start = Instant::now();
        if changes.is_empty() {
            return Ok(UpdateResult {
                duration: start.elapsed(),
                ..Default::default()
            });
        }
        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let all_files = discoverer.discover(repo_root)?;
        self.apply_changes(graph, repo_root, &all_files, changes, start)
    }

    /// Compact `base_snapshot` + `changes` into `output_snapshot` without mutating the live
    /// `{repo}/.rgctl/` structural tree (no tracker/JSON/sidecar invalidation).
    ///
    /// Spill and compact scratch live under `work_root`. Source files are still read from
    /// `repo_root` (working tree). Used by review/pr-check delta-head synthesis (#124).
    pub fn apply_change_set_to_snapshot(
        &self,
        base_snapshot: &Path,
        output_snapshot: &Path,
        work_root: &Path,
        repo_root: &Path,
        changes: ChangeSet,
    ) -> Result<UpdateResult> {
        let start = Instant::now();
        if !base_snapshot.is_file() {
            return Err(rgctl_error::Error::NotFound(format!(
                "base snapshot not found at {}",
                base_snapshot.display()
            )));
        }
        if let Some(parent) = output_snapshot.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir_all(work_root)?;

        if changes.is_empty() {
            std::fs::copy(base_snapshot, output_snapshot)?;
            return Ok(UpdateResult {
                duration: start.elapsed(),
                ..Default::default()
            });
        }

        let mut result = UpdateResult {
            files_added: changes.added.len(),
            files_changed: changes.changed.len(),
            files_deleted: changes.deleted.len(),
            ..Default::default()
        };

        let extract_paths = self.expand_extract_paths(repo_root, base_snapshot, &changes)?;
        let paths_to_update: Vec<PathBuf> = extract_paths
            .iter()
            .map(|rel| resolve_path(repo_root, rel))
            .collect();

        let extractor = Extractor::new(Arc::clone(&self.registry));
        let spill_dir = work_root.join("spill_delta");
        if spill_dir.exists() {
            std::fs::remove_dir_all(&spill_dir)?;
        }
        std::fs::create_dir_all(&spill_dir)?;
        let mut builder = GraphBuilder::with_spill(&spill_dir)?;
        let (_stream_stats, tails) = stream_into_graph(
            self.config.thread_count,
            &extractor,
            Arc::clone(&self.registry),
            &paths_to_update,
            DEFAULT_STREAM_CHANNEL_CAPACITY,
            &mut builder,
            || {},
        )?;
        builder.build_resolution_indexes();
        extractor.populate_pass2(&tails, &mut builder)?;
        let finished = builder.finish_spill()?;
        let (new_nodes, new_edges) =
            rgctl_graph::segmented_spill::materialize_sorted_graph(&finished)?;
        finished.cleanup()?;
        result.nodes_added = new_nodes.len();
        result.edges_added = new_edges.len();

        let mut delta = DeltaSegment::new();
        for rel in extract_paths.iter().chain(changes.deleted.iter()) {
            delta.invalidate_file(rel);
        }
        delta.new_nodes = new_nodes;
        delta.new_edges = new_edges;

        let scratch = work_root.join("compact-scratch");
        let _stats = compact_snapshot_file(base_snapshot, delta, output_snapshot, &scratch)?;

        let mut graph = CodeGraph::open_snapshot(output_snapshot)?;
        let relation_edges = self.rebuild_relations(&mut graph, repo_root, &paths_to_update)?;
        result.edges_added += relation_edges;
        if relation_edges > 0 {
            let _ = write_columnar_from_backend(graph.backend(), output_snapshot)?;
        }

        result.duration = start.elapsed();
        Ok(result)
    }

    /// Update only the given repo-relative file paths.
    pub fn update_files(
        &self,
        graph: &mut CodeGraph,
        repo_root: &Path,
        relative_paths: &[String],
    ) -> Result<UpdateResult> {
        let start = Instant::now();
        if relative_paths.is_empty() {
            return Ok(UpdateResult {
                duration: start.elapsed(),
                ..Default::default()
            });
        }

        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let all_files = discoverer.discover(repo_root)?;
        let changes = crate::file_tracker::changes_for_paths(repo_root, relative_paths)?;

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

        let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);
        std::fs::create_dir_all(repo_root.join(".rgctl"))?;
        let nodes_removed = graph.node_count();
        let edges_removed = graph.edge_count();

        let (stats, _digest) =
            pipeline.process_repository_to_snapshot(repo_root, &snapshot_path, None)?;
        *graph = CodeGraph::open_snapshot(&snapshot_path)?;

        let mut tracker = FileTracker::new(repo_root);
        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let files = discoverer.discover(repo_root)?;
        tracker.index_files(&files, graph)?;
        tracker.save()?;
        // Keep legacy JSON export for callers that still read it.
        let _ = graph.save_to_repo(repo_root);

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
        let git_changes = git_diff_name_status(repo_root, since, "HEAD")?;
        let tracker = FileTracker::load(repo_root)?;
        let hash_changes = tracker.detect_changes(all_files)?;
        Ok(merge_change_sets(git_changes, hash_changes))
    }

    fn apply_changes(
        &self,
        graph: &mut CodeGraph,
        repo_root: &Path,
        all_files: &[PathBuf],
        changes: ChangeSet,
        start: Instant,
    ) -> Result<UpdateResult> {
        let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);
        if snapshot_path.is_file() {
            return self.apply_changes_via_compact(
                graph,
                repo_root,
                all_files,
                changes,
                start,
                &snapshot_path,
            );
        }
        self.apply_changes_in_memory(graph, repo_root, all_files, changes, start)
    }

    /// Low-memory path: extract delta only, stream-compact into a new columnar snapshot.
    fn apply_changes_via_compact(
        &self,
        graph: &mut CodeGraph,
        repo_root: &Path,
        all_files: &[PathBuf],
        changes: ChangeSet,
        start: Instant,
        snapshot_path: &Path,
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
                ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}").unwrap(),
            );
            pb.set_message("compacting");
            Some(pb)
        } else {
            None
        };

        let nodes_before = graph.node_count();
        let edges_before = graph.edge_count();

        let extract_paths = self.expand_extract_paths(repo_root, snapshot_path, &changes)?;
        let paths_to_update: Vec<PathBuf> = extract_paths
            .iter()
            .map(|rel| resolve_path(repo_root, rel))
            .collect();

        let extractor = Extractor::new(Arc::clone(&self.registry));
        let spill_dir = repo_root.join(".rgctl").join("spill_delta");
        if spill_dir.exists() {
            std::fs::remove_dir_all(&spill_dir)?;
        }
        std::fs::create_dir_all(&spill_dir)?;
        let mut builder = GraphBuilder::with_spill(&spill_dir)?;
        let (_stream_stats, tails) = stream_into_graph(
            self.config.thread_count,
            &extractor,
            Arc::clone(&self.registry),
            &paths_to_update,
            DEFAULT_STREAM_CHANNEL_CAPACITY,
            &mut builder,
            || {},
        )?;
        builder.build_resolution_indexes();
        extractor.populate_pass2(&tails, &mut builder)?;
        let finished = builder.finish_spill()?;
        let (new_nodes, new_edges) =
            rgctl_graph::segmented_spill::materialize_sorted_graph(&finished)?;
        finished.cleanup()?;
        result.nodes_added = new_nodes.len();
        result.edges_added = new_edges.len();

        let mut delta = DeltaSegment::new();
        for rel in extract_paths.iter().chain(changes.deleted.iter()) {
            delta.invalidate_file(rel);
        }
        delta.new_nodes = new_nodes;
        delta.new_edges = new_edges;

        if let Some(pb) = &progress {
            pb.set_message("streaming compact");
        }
        let stats = compact_repo_snapshot(repo_root, delta)?;
        let _ = stats.content_digest;

        *graph = CodeGraph::open_snapshot(snapshot_path)?;

        // Cross-file edges from changed and cascaded caller files into the rest of the graph.
        let relation_edges = self.rebuild_relations(graph, repo_root, &paths_to_update)?;
        result.edges_added += relation_edges;
        if relation_edges > 0 {
            // Keep columnar digest rules (topology-only edges), not PreparedGraphSnapshot hash.
            let _ = write_columnar_from_backend(graph.backend(), snapshot_path)?;
            *graph = CodeGraph::open_snapshot(snapshot_path)?;
        }

        // Sidecars key off graph_digest — drop them so next discover/blast rebuilds cleanly.
        invalidate_digest_sidecars(repo_root);

        result.nodes_removed =
            nodes_before.saturating_sub(graph.node_count().saturating_sub(result.nodes_added));
        result.edges_removed =
            edges_before.saturating_sub(graph.edge_count().saturating_sub(result.edges_added));

        if let Some(pb) = progress {
            pb.finish_with_message("done");
        }

        let mut tracker = FileTracker::new(repo_root);
        tracker.index_files(all_files, graph)?;
        tracker.save()?;
        let _ = graph.save_to_repo(repo_root);

        result.duration = start.elapsed();
        Ok(result)
    }

    /// Legacy path when no columnar snapshot exists yet (in-memory backend edits).
    fn apply_changes_in_memory(
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
                ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}").unwrap(),
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

        let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);
        let extract_paths = if snapshot_path.is_file() {
            self.expand_extract_paths(repo_root, &snapshot_path, &changes)?
        } else {
            changes.extract_paths()
        };
        let paths_to_update: Vec<PathBuf> = extract_paths
            .iter()
            .map(|rel| resolve_path(repo_root, rel))
            .collect();

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
        let mut builder = GraphBuilder::new();
        let (stream_stats, tails) = stream_into_graph(
            self.config.thread_count,
            &extractor,
            Arc::clone(&self.registry),
            &paths_to_update,
            DEFAULT_STREAM_CHANNEL_CAPACITY,
            &mut builder,
            || {},
        )?;
        let _ = stream_stats;
        builder.build_resolution_indexes();
        extractor.populate_pass2(&tails, &mut builder)?;
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
        // Persist columnar snapshot so subsequent updates can use the compact path.
        let _ = graph.save_snapshot(repo_root);

        result.duration = start.elapsed();
        Ok(result)
    }

    fn expand_extract_paths(
        &self,
        _repo_root: &Path,
        snapshot_path: &Path,
        changes: &ChangeSet,
    ) -> Result<Vec<String>> {
        let mut paths = changes.extract_paths();
        if self.config.cascade_depth == 0 {
            return Ok(paths);
        }

        let mmap = MmappedGraphSnapshot::open(snapshot_path)?;
        let col = mmap
            .columnar()
            .ok_or_else(|| rgctl_error::Error::Other("cascade requires columnar snapshot".into()))?;
        let seeds = changes.invalidation_paths();
        let cascaded =
            incoming_callers_files_depth(col, &seeds, self.config.cascade_depth)?;
        paths.extend(cascaded);
        paths.sort();
        paths.dedup();
        Ok(paths)
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
                    new_edges.push(rgctl_graph::schema::Edge::new(from, to, edge_type));
                    added += 1;
                }
            }
            backend.insert_edges_batch(new_edges)?;
            graph.backend_mut().prune_orphan_edges()?;
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

/// Delete `.rgctl` artifacts that embed `graph_digest` so they cannot outlive a compact.
fn invalidate_digest_sidecars(repo_root: &Path) {
    let targets = [
        BlastEngineSnapshot::default_path(repo_root),
        MacroCallIndex::default_path(repo_root),
        MacroCallLookupDb::default_path(repo_root),
        repo_root.join(".rgctl").join("analysis_results.bin"),
        SemanticIndex::default_path(repo_root),
        CfgPdgArchive::default_path(repo_root),
    ];
    for path in targets {
        let _ = std::fs::remove_file(&path);
    }
}

fn relation_type_to_edge(relation_type: rgctl_plugin_api::RelationType) -> EdgeType {
    use rgctl_plugin_api::RelationType;
    match relation_type {
        RelationType::Calls => EdgeType::Calls,
        RelationType::Uses => EdgeType::Uses,
        RelationType::Implements => EdgeType::Implements,
        RelationType::Extends => EdgeType::Extends,
        RelationType::AnnotatedWith => EdgeType::AnnotatedWith,
        RelationType::Permits => EdgeType::Permits,
        RelationType::Defines => EdgeType::Contains,
        RelationType::References => EdgeType::References,
        RelationType::Instantiates => EdgeType::Instantiates,
        RelationType::Modifies => EdgeType::Modifies,
        RelationType::DependsOn => EdgeType::DependsOn,
        RelationType::IncludesRole => EdgeType::IncludesRole,
        RelationType::DependsOnRole => EdgeType::DependsOnRole,
        RelationType::ExecutesTask => EdgeType::ExecutesTask,
        RelationType::NotifiesHandler => EdgeType::NotifiesHandler,
        RelationType::IncludesPlaybook => EdgeType::IncludesPlaybook,
        RelationType::UsesVariable => EdgeType::Uses,
        RelationType::RendersTemplate => EdgeType::RendersTemplate,
        RelationType::DependsOnCookbook => EdgeType::DependsOnCookbook,
        RelationType::IncludesRecipe => EdgeType::IncludesRecipe,
        RelationType::DeclaresResource => EdgeType::DeclaresResource,
        RelationType::UsesTemplate => EdgeType::UsesTemplate,
        RelationType::DefinesAttribute => EdgeType::DefinesAttribute,
        RelationType::NotifiesResource => EdgeType::NotifiesResource,
        RelationType::DependsOnModule => EdgeType::DependsOnModule,
        RelationType::IncludesClass => EdgeType::IncludesClass,
        RelationType::InheritsClass => EdgeType::InheritsClass,
        RelationType::RequiresResource => EdgeType::RequiresResource,
        RelationType::UsesFact => EdgeType::UsesFact,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::schema::NodeType;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_incremental_update() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let main = root.join("src/main.rs");
        fs::create_dir_all(main.parent().unwrap()).unwrap();
        fs::write(&main, "fn main() {}\n").unwrap();

        let pipeline = ProcessingPipeline::new(Arc::new(rgctl_languages::default_registry()));
        let (mut graph, _) = pipeline.process_repository(root).unwrap();
        let initial_count = graph.node_count();

        let mut tracker = FileTracker::new(root);
        let files = vec![main.clone()];
        tracker.index_files(&files, &graph).unwrap();
        tracker.save().unwrap();
        graph.save_to_repo(root).unwrap();
        graph.save_snapshot(root).unwrap();

        fs::write(&main, "fn main() { helper(); }\nfn helper() {}\n").unwrap();

        let updater = IncrementalUpdater::new(Arc::new(rgctl_languages::default_registry()));
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

        let registry = Arc::new(rgctl_languages::default_registry());
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

    #[test]
    fn callee_rename_cascades_to_caller_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            "mod auth;\nfn main() { auth::login(); }\n",
        )
        .unwrap();
        fs::write(root.join("src/auth.rs"), "pub fn login() {}\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let pipeline = ProcessingPipeline::new(Arc::clone(&registry));
        let (mut graph, _) = pipeline.process_repository(root).unwrap();

        let mut tracker = FileTracker::new(root);
        tracker
            .index_files(
                &[
                    root.join("src/main.rs"),
                    root.join("src/auth.rs"),
                ],
                &graph,
            )
            .unwrap();
        tracker.save().unwrap();
        graph.save_to_repo(root).unwrap();
        graph.save_snapshot(root).unwrap();

        fs::write(root.join("src/auth.rs"), "pub fn authenticate() {}\n").unwrap();
        fs::write(
            root.join("src/main.rs"),
            "mod auth;\nfn main() { auth::authenticate(); }\n",
        )
        .unwrap();

        let updater = IncrementalUpdater::with_options(
            registry,
            UpdateOptions {
                cascade_depth: 1,
                show_progress: false,
                ..Default::default()
            },
        );
        updater
            .update_files(&mut graph, root, &["src/auth.rs".into()])
            .unwrap();

        let functions = graph.find_by_type(NodeType::Function).unwrap();
        assert!(
            functions.iter().any(|n| &*n.name == "main"),
            "expected main after cascade; got {:?}",
            functions
                .iter()
                .map(|n| (&*n.name, n.file_path.clone()))
                .collect::<Vec<_>>()
        );
        let authenticate_id = functions
            .iter()
            .find(|n| &*n.name == "authenticate")
            .map(|n| n.id)
            .expect("authenticate");

        let main_id = functions
            .iter()
            .find(|n| &*n.name == "main")
            .map(|n| n.id)
            .expect("main");

        assert!(
            graph
                .backend()
                .has_edge(main_id, authenticate_id, EdgeType::Calls),
            "cascade should re-extract main.rs and rebuild main→authenticate"
        );
    }

    #[test]
    fn compact_update_invalidates_digest_sidecars() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let main = root.join("src/main.rs");
        fs::create_dir_all(main.parent().unwrap()).unwrap();
        fs::write(&main, "fn main() {}\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let pipeline = ProcessingPipeline::new(Arc::clone(&registry));
        let (mut graph, _) = pipeline.process_repository(root).unwrap();

        let mut tracker = FileTracker::new(root);
        tracker.index_files(&[main.clone()], &graph).unwrap();
        tracker.save().unwrap();
        graph.save_to_repo(root).unwrap();
        graph.save_snapshot(root).unwrap();

        let blast = BlastEngineSnapshot::default_path(root);
        let macro_idx = MacroCallIndex::default_path(root);
        let analysis = root.join(".rgctl").join("analysis_results.bin");
        fs::create_dir_all(blast.parent().unwrap()).unwrap();
        fs::write(&blast, b"stale").unwrap();
        fs::write(&macro_idx, b"stale").unwrap();
        fs::write(&analysis, b"stale").unwrap();

        fs::write(&main, "fn main() { helper(); }\nfn helper() {}\n").unwrap();
        let updater = IncrementalUpdater::new(registry);
        updater.update(&mut graph, root).unwrap();

        assert!(
            !blast.exists(),
            "blast sidecar should be removed after compact"
        );
        assert!(
            !macro_idx.exists(),
            "macro index should be removed after compact"
        );
        assert!(
            !analysis.exists(),
            "analysis_results should be removed after compact"
        );
    }
}
