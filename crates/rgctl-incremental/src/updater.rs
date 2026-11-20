//! Incremental graph updater
//!
//! Task 5.1.2: Update graph for changed files only (CRITICAL)

use crate::cascade::incoming_callers_files_depth_in_repo;
use crate::file_tracker::{
    ChangeSet, FileTracker, changes_for_paths, merge_change_sets, normalize_path_str,
    relative_path, resolve_path,
};
use crate::pr_scope::git_diff_name_status;
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::Mmap;
use rgctl_analysis::{
    BlastEngineSnapshot, CfgPdgArchive, MacroCallIndex, MacroCallLookupDb, SemanticIndex,
};
use rgctl_error::Result;
use rgctl_extraction::discovery::{DiscoveryConfig, FileDiscoverer};
use rgctl_extraction::{ExtractionTail, Extractor, FactsCache, GraphBuilder};
use rgctl_graph::code_graph::CodeGraph;
use rgctl_graph::columnar_snapshot::ColumnarGraphMmap;
use rgctl_graph::csr::edge_type_to_u8;
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType, SharedStr};
use rgctl_graph::{DeltaSegment, GraphCompactor, MmappedGraphSnapshot};
use rgctl_pipeline::parallel::par_map;
use rgctl_pipeline::stream::{DEFAULT_STREAM_CHANNEL_CAPACITY, stream_into_graph};
use rgctl_pipeline::{PipelineConfig, ProcessingPipeline};
use rgctl_registry::LanguageRegistry;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Minimum repository size before considering a full streaming rebuild over delta compaction.
const LARGE_DELTA_MIN_REPO_FILES: usize = 500;
/// Absolute extracted-file count above which streaming snapshot rebuild beats delta compaction.
const LARGE_DELTA_ABS_THRESHOLD: usize = 5_000;
/// Fraction numerator (`/ 10` = 30%) of repo files above which streaming rebuild is used.
const LARGE_DELTA_RATIO_TENTHS: usize = 3;

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
    // Preserve the last discover mode when changed files are re-extracted.
    fn fields_enabled(repo_root: &Path) -> bool {
        rgctl_graph::paths::artifact_path(repo_root, "materialized_fields.digest").is_file()
    }

    fn refresh_fields_marker(repo_root: &Path) {
        if Self::fields_enabled(repo_root)
            && let Ok(store) = rgctl_graph::SnapshotNodeStore::open_from_repo(repo_root)
            && let Ok(digest) = store.content_digest()
        {
            let _ = std::fs::write(
                rgctl_graph::paths::artifact_path(repo_root, "materialized_fields.digest"),
                digest,
            );
        }
    }

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

    /// Update `.rgctl/graph.snapshot.bin` in place without hydrating [`CodeGraph`] into RAM.
    pub fn update_repo(&self, repo_root: &Path) -> Result<UpdateResult> {
        let start = Instant::now();
        let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);

        if self.config.force {
            let discoverer = FileDiscoverer::with_config(
                Arc::clone(&self.registry),
                self.config.discovery.clone(),
            );
            let all_files = discoverer.discover(repo_root)?;
            return self.full_rebuild_to_snapshot(repo_root, &all_files, None, start);
        }

        if !snapshot_path.is_file() {
            return Err(rgctl_error::Error::NotFound(format!(
                "snapshot not found at {}",
                snapshot_path.display()
            )));
        }

        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let all_files = discoverer.discover(repo_root)?;
        let mut tracker = FileTracker::load(repo_root)?;

        let (changes, dirty_hashes) = if let Some(ref since) = self.config.since {
            self.changes_from_git(repo_root, since, &all_files, &tracker)?
        } else {
            tracker.detect_changes_with_hashes(&all_files)?
        };

        if changes.is_empty() {
            return Ok(UpdateResult {
                duration: start.elapsed(),
                ..Default::default()
            });
        }

        self.apply_changes_to_repo_snapshot(
            repo_root,
            &snapshot_path,
            Some(&all_files),
            &mut tracker,
            changes,
            dirty_hashes,
            start,
        )
    }

    /// Update only the given repo-relative file paths directly on `.rgctl/graph.snapshot.bin`
    /// without hydrating [`CodeGraph`] or walking the repository directory tree.
    pub fn update_repo_files(
        &self,
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

        let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);
        if !snapshot_path.is_file() {
            return Err(rgctl_error::Error::NotFound(format!(
                "snapshot not found at {}",
                snapshot_path.display()
            )));
        }

        let changes = changes_for_paths(repo_root, relative_paths)?;
        if changes.is_empty() {
            return Ok(UpdateResult {
                duration: start.elapsed(),
                ..Default::default()
            });
        }

        let mut tracker =
            FileTracker::load(repo_root).unwrap_or_else(|_| FileTracker::new(repo_root));
        self.apply_changes_to_repo_snapshot(
            repo_root,
            &snapshot_path,
            None,
            &mut tracker,
            changes,
            FxHashMap::default(),
            start,
        )
    }

    /// Update the graph for a repository, returning statistics.
    pub fn update(&self, graph: &mut CodeGraph, repo_root: &Path) -> Result<UpdateResult> {
        let start = Instant::now();
        let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);

        if self.config.force {
            let discoverer = FileDiscoverer::with_config(
                Arc::clone(&self.registry),
                self.config.discovery.clone(),
            );
            let all_files = discoverer.discover(repo_root)?;
            let prev_counts =
                (!snapshot_path.is_file()).then(|| (graph.node_count(), graph.edge_count()));
            let mut result = self.full_rebuild_to_snapshot(repo_root, &all_files, None, start)?;
            if let Some((prev_nodes, prev_edges)) = prev_counts {
                result.nodes_removed = prev_nodes;
                result.edges_removed = prev_edges;
            }
            *graph = CodeGraph::open_snapshot(&snapshot_path)?;
            result.duration = start.elapsed();
            return Ok(result);
        }

        if snapshot_path.is_file() {
            let result = self.update_repo(repo_root)?;
            if result.files_affected() > 0 {
                *graph = CodeGraph::open_snapshot(&snapshot_path)?;
            }
            return Ok(result);
        }

        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let all_files = discoverer.discover(repo_root)?;
        let tracker = FileTracker::load(repo_root)?;
        let (changes, _) = if let Some(ref since) = self.config.since {
            self.changes_from_git(repo_root, since, &all_files, &tracker)?
        } else {
            tracker.detect_changes_with_hashes(&all_files)?
        };

        if changes.is_empty() {
            return Ok(UpdateResult {
                duration: start.elapsed(),
                ..Default::default()
            });
        }

        self.apply_changes_in_memory(graph, repo_root, &all_files, changes, start)
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
        let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);
        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let all_files = discoverer.discover(repo_root)?;

        if snapshot_path.is_file() {
            let mut tracker =
                FileTracker::load(repo_root).unwrap_or_else(|_| FileTracker::new(repo_root));
            let result = self.apply_changes_to_repo_snapshot(
                repo_root,
                &snapshot_path,
                Some(&all_files),
                &mut tracker,
                changes,
                FxHashMap::default(),
                start,
            )?;
            *graph = CodeGraph::open_snapshot(&snapshot_path)?;
            return Ok(result);
        }

        self.apply_changes_in_memory(graph, repo_root, &all_files, changes, start)
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

        let base = open_columnar_mmap(base_snapshot)?;
        let extract_paths = self.expand_extract_paths_with_col(repo_root, &base, &changes)?;
        let paths_to_update: Vec<PathBuf> = extract_paths
            .iter()
            .map(|rel| resolve_path(repo_root, rel))
            .collect();

        let facts = Arc::new(FactsCache::load(repo_root));
        let extractor =
            Extractor::new(Arc::clone(&self.registry)).with_facts_cache(Arc::clone(&facts));
        let spill_dir = work_root.join("spill_delta");
        if spill_dir.exists() {
            std::fs::remove_dir_all(&spill_dir)?;
        }
        std::fs::create_dir_all(&spill_dir)?;
        let mut builder = GraphBuilder::with_spill(&spill_dir)?;
        builder.set_materialize_fields(Self::fields_enabled(repo_root));
        let (_stream_stats, tails, _lossless_sidecars) = stream_into_graph(
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

        let mut delta = build_delta_segment(
            repo_root,
            &base,
            &extract_paths,
            &changes.deleted,
            new_nodes,
            new_edges,
        );
        resolve_cross_file_edges_from_tails(
            &base,
            &mut delta,
            &tails,
            repo_root,
            self.config.thread_count,
        )?;
        result.nodes_added = delta.new_nodes.len();
        result.edges_added = delta.new_edges.len();

        let scratch = work_root.join("compact-scratch");
        let stats = GraphCompactor::new(&base, delta).compact_to_path(output_snapshot, &scratch)?;
        result.nodes_removed = stats.nodes_dropped;
        result.edges_removed = stats.edges_dropped;

        let _ = facts.save(repo_root);

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

        let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);
        if snapshot_path.is_file() {
            let result = self.update_repo_files(repo_root, relative_paths)?;
            if result.files_affected() > 0 {
                *graph = CodeGraph::open_snapshot(&snapshot_path)?;
            }
            return Ok(result);
        }

        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let all_files = discoverer.discover(repo_root)?;
        let changes = changes_for_paths(repo_root, relative_paths)?;

        if changes.is_empty() {
            return Ok(UpdateResult {
                duration: start.elapsed(),
                ..Default::default()
            });
        }

        self.apply_changes_in_memory(graph, repo_root, &all_files, changes, start)
    }

    fn full_rebuild_to_snapshot(
        &self,
        repo_root: &Path,
        all_files: &[PathBuf],
        trigger_changes: Option<&ChangeSet>,
        start: Instant,
    ) -> Result<UpdateResult> {
        let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);
        let (nodes_removed, edges_removed) = if snapshot_path.is_file()
            && let Ok(prev) = open_columnar_mmap(&snapshot_path)
        {
            (prev.node_count(), prev.edge_count())
        } else {
            (0, 0)
        };

        let pipeline = ProcessingPipeline::with_config(
            Arc::clone(&self.registry),
            PipelineConfig {
                discovery: self.config.discovery.clone(),
                show_progress: self.config.show_progress,
                thread_count: self.config.thread_count,
                materialize_fields: Self::fields_enabled(repo_root),
                ..PipelineConfig::default()
            },
        );

        std::fs::create_dir_all(repo_root.join(".rgctl"))?;
        rgctl_graph::paths::ensure_rgctl_git_exclude(repo_root);

        let (stats, _digest) =
            pipeline.process_files_to_snapshot(repo_root, all_files, &snapshot_path, None)?;

        invalidate_digest_sidecars(repo_root);

        let mut tracker = FileTracker::new(repo_root);
        tracker.index_files_with_mapping(
            all_files,
            stats.node_path_mapping,
            Some(&stats.file_hashes),
        )?;
        tracker.save()?;

        Self::refresh_fields_marker(repo_root);
        let (files_added, files_changed, files_deleted) = match trigger_changes {
            Some(cs) => (cs.added.len(), cs.changed.len(), cs.deleted.len()),
            None => (0, stats.files_processed, 0),
        };
        Ok(UpdateResult {
            files_added,
            files_changed,
            files_deleted,
            nodes_removed,
            nodes_added: stats.nodes_created,
            edges_removed,
            edges_added: stats.edges_created,
            duration: start.elapsed(),
        })
    }

    fn changes_from_git(
        &self,
        repo_root: &Path,
        since: &str,
        all_files: &[PathBuf],
        tracker: &FileTracker,
    ) -> Result<(ChangeSet, FxHashMap<String, String>)> {
        let git_changes = git_diff_name_status(repo_root, since, "HEAD")?;
        let (hash_changes, dirty_hashes) = tracker.detect_changes_with_hashes(all_files)?;
        Ok((merge_change_sets(git_changes, hash_changes), dirty_hashes))
    }

    /// Zero-hydration snapshot compaction path:
    /// 1. Open base mmap once for reverse-CALLS cascade + pre-compaction cross-file resolution.
    /// 2. Extract `paths_to_update` once via `stream_into_graph`.
    /// 3. Resolve cross-file edges from `tails` directly against hot base columns + `delta.new_nodes`.
    /// 4. Write `graph.snapshot.bin` once via `GraphCompactor` (zero `MemoryBackend` hydrations).
    /// 5. Patch `FileTracker` in `O(|delta|)`.
    #[allow(clippy::too_many_arguments)]
    fn apply_changes_to_repo_snapshot(
        &self,
        repo_root: &Path,
        snapshot_path: &Path,
        all_files: Option<&[PathBuf]>,
        tracker: &mut FileTracker,
        changes: ChangeSet,
        dirty_hashes: FxHashMap<String, String>,
        start: Instant,
    ) -> Result<UpdateResult> {
        let base = open_columnar_mmap(snapshot_path)?;
        let extract_paths = self.expand_extract_paths_with_col(repo_root, &base, &changes)?;

        if let Some(files) = all_files
            && should_fallback_to_full_rebuild(extract_paths.len(), files.len())
        {
            drop(base);
            return self.full_rebuild_to_snapshot(repo_root, files, Some(&changes), start);
        }

        let mut result = UpdateResult {
            files_added: changes.added.len(),
            files_changed: changes.changed.len(),
            files_deleted: changes.deleted.len(),
            ..Default::default()
        };

        let progress = if self.config.show_progress && !changes.is_empty() {
            let pb = ProgressBar::new(changes.len() as u64);
            if let Ok(style) =
                ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}")
            {
                pb.set_style(style);
            }
            pb.set_message("extracting delta");
            Some(pb)
        } else {
            None
        };

        let paths_to_update: Vec<PathBuf> = extract_paths
            .iter()
            .map(|rel| resolve_path(repo_root, rel))
            .collect();

        let facts = Arc::new(FactsCache::load(repo_root));
        let extractor =
            Extractor::new(Arc::clone(&self.registry)).with_facts_cache(Arc::clone(&facts));
        let spill_dir = repo_root.join(".rgctl").join("spill_delta");
        if spill_dir.exists() {
            std::fs::remove_dir_all(&spill_dir)?;
        }
        std::fs::create_dir_all(&spill_dir)?;
        let mut builder = GraphBuilder::with_spill(&spill_dir)?;
        builder.set_materialize_fields(Self::fields_enabled(repo_root));
        let (stream_stats, tails, _lossless_sidecars) = stream_into_graph(
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

        let mut delta = build_delta_segment(
            repo_root,
            &base,
            &extract_paths,
            &changes.deleted,
            new_nodes,
            new_edges,
        );

        if let Some(pb) = &progress {
            pb.set_message("resolving cross-file edges");
        }
        resolve_cross_file_edges_from_tails(
            &base,
            &mut delta,
            &tails,
            repo_root,
            self.config.thread_count,
        )?;
        drop(tails);

        // Patch tracker before `delta.new_nodes` is moved into `GraphCompactor`.
        let mut deleted_and_renamed_old = changes.deleted.clone();
        for (old, _) in &changes.renamed {
            if !deleted_and_renamed_old.contains(old) {
                deleted_and_renamed_old.push(old.clone());
            }
        }
        tracker.apply_delta_update(
            &deleted_and_renamed_old,
            &extract_paths,
            &delta.new_nodes,
            &stream_stats.file_hashes,
            &dirty_hashes,
        )?;

        result.nodes_added = delta.new_nodes.len();
        result.edges_added = delta.new_edges.len();

        if let Some(pb) = &progress {
            pb.set_message("streaming compact");
        }
        let scratch = repo_root.join(".rgctl").join("compact-scratch");
        let stats = GraphCompactor::new(&base, delta).compact_to_path(snapshot_path, &scratch)?;
        drop(base);

        result.nodes_removed = stats.nodes_dropped;
        result.edges_removed = stats.edges_dropped;

        if let Err(err) = facts.save(repo_root) {
            eprintln!("warning: failed to persist facts.bin after update: {err}");
        }

        // Sidecars key off graph_digest — drop them so next discover/blast rebuilds cleanly.
        invalidate_digest_sidecars(repo_root);

        if let Some(pb) = progress {
            pb.finish_with_message("done");
        }

        tracker.save()?;
        rgctl_graph::paths::ensure_rgctl_git_exclude(repo_root);

        result.duration = start.elapsed();
        Self::refresh_fields_marker(repo_root);
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
            if let Ok(style) =
                ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}")
            {
                pb.set_style(style);
            }
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

        let extract_paths = changes.extract_paths();
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

        let facts = Arc::new(FactsCache::load(repo_root));
        let extractor =
            Extractor::new(Arc::clone(&self.registry)).with_facts_cache(Arc::clone(&facts));
        let mut builder = GraphBuilder::new();
        builder.set_materialize_fields(Self::fields_enabled(repo_root));
        let (stream_stats, tails, _lossless_sidecars) = stream_into_graph(
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
        let delta_ids: FxHashSet<Uuid> = new_nodes.iter().map(|n| n.id).collect();

        {
            let backend = graph.backend_mut();
            backend.insert_nodes_batch(new_nodes)?;
            backend.insert_edges_batch(new_edges)?;
        }

        result.nodes_added = nodes_added;
        let relation_edges =
            self.rebuild_relations_in_memory(graph, repo_root, &tails, &delta_ids)?;
        result.edges_added = edges_added + relation_edges;
        result.edges_removed = edges_before.saturating_sub(graph.edge_count());

        let mut tracker = FileTracker::new(repo_root);
        tracker.index_files(all_files, graph)?;
        tracker.save()?;
        rgctl_graph::paths::ensure_rgctl_git_exclude(repo_root);
        // Persist columnar snapshot so subsequent updates can use the compact path.
        // Do not write legacy `.rgctl/graph.json` (matches `rgctl discover`).
        let _ = graph.save_snapshot(repo_root);
        let _ = facts.save(repo_root);

        result.duration = start.elapsed();
        Self::refresh_fields_marker(repo_root);
        Ok(result)
    }

    fn expand_extract_paths_with_col(
        &self,
        repo_root: &Path,
        col: &ColumnarGraphMmap,
        changes: &ChangeSet,
    ) -> Result<Vec<String>> {
        let mut paths = changes.extract_paths();
        if self.config.cascade_depth == 0 {
            return Ok(paths);
        }

        let seeds = changes.invalidation_paths();
        let cascaded = incoming_callers_files_depth_in_repo(
            col,
            Some(repo_root),
            &seeds,
            self.config.cascade_depth,
        )?;
        for c in cascaded {
            let abs = resolve_path(repo_root, &c);
            if abs.is_file() {
                if let Ok(rel) = relative_path(repo_root, &abs) {
                    paths.push(rel);
                } else {
                    paths.push(c);
                }
            }
        }
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// Resolve cross-file relations for the legacy in-memory backend using already-extracted `tails`.
    fn rebuild_relations_in_memory(
        &self,
        graph: &mut CodeGraph,
        repo_root: &Path,
        tails: &[ExtractionTail],
        delta_ids: &FxHashSet<Uuid>,
    ) -> Result<usize> {
        if tails.is_empty() {
            return Ok(0);
        }

        let requirements = TailResolutionRequirements::from_tails(tails);
        if requirements.tokens.is_empty() && !requirements.has_empty_method_probe {
            return Ok(0);
        }

        let nodes = graph.backend().all_nodes()?;
        if nodes.iter().all(|n| delta_ids.contains(&n.id)) {
            return Ok(0);
        }
        let mut builder = CompactRelationIndexBuilder::new(repo_root, &requirements);
        for node in &nodes {
            builder.observe_node(node, delta_ids.contains(&node.id));
        }
        let mut index = builder.finish();
        if requirements.need_inheritance {
            graph.backend().for_each_edge(|edge| {
                index.record_inheritance_edge(edge.from, edge.to, edge.edge_type);
            })?;
        }

        let edges_to_add =
            resolve_tails_against_index(&index, tails, repo_root, self.config.thread_count);
        let mut added = 0usize;
        if !edges_to_add.is_empty() {
            let backend = graph.backend_mut();
            let mut new_edges = Vec::new();
            for (from, to, edge_type) in edges_to_add {
                if delta_ids.contains(&from) && delta_ids.contains(&to) {
                    continue;
                }
                if !backend.has_edge(from, to, edge_type) {
                    new_edges.push(Edge::new(from, to, edge_type));
                    added += 1;
                }
            }
            backend.insert_edges_batch(new_edges)?;
            graph.backend_mut().prune_orphan_edges()?;
        }
        Ok(added)
    }
}

#[inline]
fn should_fallback_to_full_rebuild(extract_count: usize, total_files: usize) -> bool {
    total_files >= LARGE_DELTA_MIN_REPO_FILES
        && (extract_count >= LARGE_DELTA_ABS_THRESHOLD
            || extract_count.saturating_mul(10)
                >= total_files.saturating_mul(LARGE_DELTA_RATIO_TENTHS))
}

fn open_columnar_mmap(path: &Path) -> Result<ColumnarGraphMmap> {
    let file = File::open(path)?;
    // SAFETY: snapshot file is read-only for the duration of inspection/compaction;
    // replacement writes go to a sibling `.bin.tmp` and rename atomically.
    let mmap = Arc::new(unsafe { Mmap::map(&file)? });
    ColumnarGraphMmap::open(mmap)
}

fn base_uses_foreign_or_relative_paths(base: &ColumnarGraphMmap, repo_prefix: &str) -> bool {
    let limit = base.node_count().min(64);
    for idx in 0..limit {
        if let Ok(Some(fp)) = base.node_file_path_str(idx)
            && !fp.is_empty()
            && fp != "<external>"
        {
            let norm = rgctl_graph::normalize_path_str(fp);
            let s = norm.as_ref();
            if !s.starts_with('/') {
                return true;
            }
            if !repo_prefix.is_empty()
                && !s
                    .strip_prefix(repo_prefix)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            {
                return true;
            }
            return false;
        }
    }
    true
}

fn build_delta_segment(
    repo_root: &Path,
    base: &ColumnarGraphMmap,
    extract_paths: &[String],
    deleted_paths: &[String],
    new_nodes: Vec<Node>,
    new_edges: Vec<Edge>,
) -> DeltaSegment {
    let repo_prefix = normalize_path_str(&repo_root.to_string_lossy());
    let repo_prefix = repo_prefix.trim_end_matches('/');
    let include_root_basenames = base_uses_foreign_or_relative_paths(base, repo_prefix);

    let mut delta = DeltaSegment::new();
    for rel in extract_paths.iter().chain(deleted_paths.iter()) {
        let norm_rel = normalize_path_str(rel);
        let abs = resolve_path(repo_root, &norm_rel);
        delta.invalidate_file(abs.to_string_lossy());
        if norm_rel.contains('/') || include_root_basenames {
            delta.invalidate_file(&norm_rel);
        }
    }
    delta.new_nodes = new_nodes;
    delta.new_edges = new_edges;
    delta
}

/// Demand-driven query requirements collected from `ExtractionTail` relations.
struct TailResolutionRequirements<'a> {
    tokens: FxHashSet<&'a str>,
    has_empty_method_probe: bool,
    need_inheritance: bool,
    need_member_methods: bool,
    need_param_identity: bool,
}

impl<'a> TailResolutionRequirements<'a> {
    fn from_tails(tails: &'a [ExtractionTail]) -> Self {
        let mut tokens = FxHashSet::default();
        let mut has_empty_method_probe = false;
        let mut need_inheritance = false;
        let mut need_member_methods = false;
        let mut need_param_identity = false;

        for tail in tails {
            for rel in &tail.relations {
                if !rel.from.is_empty() {
                    tokens.insert(rel.from.as_str());
                }
                if !rel.to.is_empty() {
                    tokens.insert(rel.to.as_str());
                }
                if let Some(hint) = rel.to_qualified_hint.as_deref()
                    && !hint.is_empty()
                {
                    tokens.insert(hint);
                }
                if rel.relation_type == rgctl_plugin_api::RelationType::Calls {
                    let target = rel.to_qualified_hint.as_deref().unwrap_or(&rel.to);
                    if let Some((_, method)) = split_callable_owner(target) {
                        need_inheritance = true;
                        if method.is_empty() {
                            has_empty_method_probe = true;
                        } else {
                            tokens.insert(method);
                        }
                    }
                    if rel.is_inferred_callable_owner()
                        && rel.to_identity_discriminator_str().is_none()
                    {
                        need_member_methods = true;
                    }
                    if rel.argument_identity_discriminator_str().is_some()
                        && rel.to_identity_discriminator_str().is_none()
                    {
                        need_param_identity = true;
                    }
                }
            }
        }

        Self {
            tokens,
            has_empty_method_probe,
            need_inheritance,
            need_member_methods,
            need_param_identity,
        }
    }

    #[inline]
    fn matches_node_names(&self, name: &str, qname: Option<&str>, node_type: NodeType) -> bool {
        if self.has_empty_method_probe && node_type == NodeType::Function {
            return true;
        }
        let primary = qname.unwrap_or(name);
        if self.tokens.contains(primary) || (name != primary && self.tokens.contains(name)) {
            return true;
        }
        let mut matched = false;
        for_each_symbol_suffix(primary, |suffix| {
            if self.tokens.contains(suffix) {
                matched = true;
            }
        });
        matched
    }
}

#[inline]
fn for_each_symbol_suffix<'a>(name: &'a str, mut f: impl FnMut(&'a str)) {
    for (offset, ch) in name.char_indices() {
        let width = match ch {
            '.' | '#' => 1,
            ':' if name[offset..].starts_with("::") => 2,
            _ => continue,
        };
        let suffix = &name[offset + width..];
        if !suffix.is_empty() {
            f(suffix);
        }
    }
}

#[derive(Clone, Debug)]
struct CandidateSym {
    id: Uuid,
    node_type: NodeType,
    is_member_method: bool,
    identity: Option<Box<str>>,
    parameters: Option<Box<str>>,
    /// True when >= 2 distinct node UUIDs share identical `(node_type, is_member_method, identity, parameters)`.
    ambiguous: bool,
}

struct PendingMatchedNode {
    id: Uuid,
    node_type: NodeType,
    name: SharedStr,
    primary_name: SharedStr,
    local_rel_file: Option<SharedStr>,
    is_field: bool,
    identity: Option<Box<str>>,
    parameters: Option<Box<str>>,
}

/// Compact `O(1)` local + global symbol resolver without unbounded `100k`-entry alias `Vec`s.
struct CompactRelationIndex {
    local_by_file: FxHashMap<SharedStr, FxHashMap<SharedStr, SmallVec<[CandidateSym; 2]>>>,
    global: FxHashMap<SharedStr, SmallVec<[CandidateSym; 2]>>,
    type_names: FxHashMap<Uuid, SharedStr>,
    bases: FxHashMap<SharedStr, Vec<(SharedStr, bool)>>,
}

struct CompactRelationIndexBuilder<'a> {
    repo_prefix: String,
    req: &'a TailResolutionRequirements<'a>,
    matched: Vec<PendingMatchedNode>,
    type_names: FxHashMap<Uuid, SharedStr>,
    type_owner_aliases: FxHashSet<SharedStr>,
}

impl<'a> CompactRelationIndexBuilder<'a> {
    fn new(repo_root: &Path, req: &'a TailResolutionRequirements<'a>) -> Self {
        let repo_prefix = normalize_path_str(&repo_root.to_string_lossy())
            .trim_end_matches('/')
            .to_owned();
        Self {
            repo_prefix,
            req,
            matched: Vec::new(),
            type_names: FxHashMap::default(),
            type_owner_aliases: FxHashSet::default(),
        }
    }

    fn canonical_rel_path(&self, raw: &str) -> SharedStr {
        let norm = rgctl_graph::normalize_path_str(raw);
        let s = norm.as_ref();
        if !self.repo_prefix.is_empty()
            && let Some(rest) = s.strip_prefix(self.repo_prefix.as_str())
        {
            let trimmed = rest.trim_start_matches('/');
            if !trimmed.is_empty() {
                return SharedStr::from(trimmed);
            }
        }
        SharedStr::from(s.trim_start_matches("./"))
    }

    fn record_type_metadata(
        &mut self,
        id: Uuid,
        node_type: NodeType,
        name: &str,
        qname: Option<&str>,
    ) {
        let primary = qname.unwrap_or(name);
        if self.req.need_inheritance
            && matches!(
                node_type,
                NodeType::Class | NodeType::Struct | NodeType::Interface | NodeType::Module
            )
        {
            self.type_names.insert(id, SharedStr::from(primary));
        }
        if self.req.need_member_methods
            && matches!(
                node_type,
                NodeType::Class
                    | NodeType::Struct
                    | NodeType::Interface
                    | NodeType::Enum
                    | NodeType::Annotation
            )
        {
            self.type_owner_aliases.insert(SharedStr::from(primary));
            if name != primary {
                self.type_owner_aliases.insert(SharedStr::from(name));
            }
            for_each_symbol_suffix(primary, |suffix| {
                self.type_owner_aliases.insert(SharedStr::from(suffix));
            });
        }
    }

    fn observe_node(&mut self, node: &Node, include_in_local: bool) {
        let name = node.name.as_str();
        let qname = node.qualified_name.as_deref();
        self.record_type_metadata(node.id, node.node_type, name, qname);

        if !self.req.matches_node_names(name, qname, node.node_type) {
            return;
        }

        let primary_name = node
            .qualified_name
            .clone()
            .unwrap_or_else(|| node.name.clone());
        let local_rel_file = if include_in_local {
            node.file_path
                .as_deref()
                .map(|fp| self.canonical_rel_path(fp))
        } else {
            None
        };
        let identity = node.identity_discriminator().map(Box::<str>::from);
        let parameters = if self.req.need_param_identity
            && node.node_type == NodeType::Function
            && identity.is_some()
        {
            node.get_property("parameter_identity")
                .map(Box::<str>::from)
        } else {
            None
        };

        self.matched.push(PendingMatchedNode {
            id: node.id,
            node_type: node.node_type,
            name: node.name.clone(),
            primary_name,
            local_rel_file,
            is_field: node.has_label("field"),
            identity,
            parameters,
        });
    }

    fn observe_kept_base_row(&mut self, base: &ColumnarGraphMmap, idx: usize) -> Result<()> {
        let node_type = base.node_type_at(idx)?;
        let need_type = (self.req.need_inheritance
            && matches!(
                node_type,
                NodeType::Class | NodeType::Struct | NodeType::Interface | NodeType::Module
            ))
            || (self.req.need_member_methods
                && matches!(
                    node_type,
                    NodeType::Class
                        | NodeType::Struct
                        | NodeType::Interface
                        | NodeType::Enum
                        | NodeType::Annotation
                ));

        let name = base.node_name_str(idx)?;
        let qname = base.node_qualified_name_str(idx)?;

        if need_type {
            let id = base.node_id_at(idx)?;
            self.record_type_metadata(id, node_type, name, qname);
        }

        if !self.req.matches_node_names(name, qname, node_type) {
            return Ok(());
        }

        let id = base.node_id_at(idx)?;
        let identity = if node_type == NodeType::Function {
            base.node_identity_discriminator_str(idx)?
                .map(Box::<str>::from)
        } else {
            None
        };

        // Decode cold `NodeExtension` ONLY on the rare matched `Variable` (to check `"field"` label)
        // or matched overloaded `Function` when `argument_identity_discriminator` is queried.
        let (is_field, parameters) = if node_type == NodeType::Variable {
            let materialized = base.materialize_node_at(idx)?;
            (materialized.has_label("field"), None)
        } else if self.req.need_param_identity
            && node_type == NodeType::Function
            && identity.is_some()
        {
            let materialized = base.materialize_node_at(idx)?;
            (
                false,
                materialized
                    .get_property("parameter_identity")
                    .map(Box::<str>::from),
            )
        } else {
            (false, None)
        };

        let name_shared = SharedStr::from(name);
        let primary_name = qname
            .map(SharedStr::from)
            .unwrap_or_else(|| name_shared.clone());

        self.matched.push(PendingMatchedNode {
            id,
            node_type,
            name: name_shared,
            primary_name,
            local_rel_file: None,
            is_field,
            identity,
            parameters,
        });
        Ok(())
    }

    fn is_member_method(&self, item: &PendingMatchedNode) -> bool {
        if !self.req.need_member_methods || item.node_type != NodeType::Function {
            return false;
        }
        let check_alias = |alias: &str| {
            split_callable_owner(alias)
                .is_some_and(|(owner, _)| self.type_owner_aliases.contains(owner))
        };
        if check_alias(&item.primary_name) {
            return true;
        }
        if !item.is_field {
            if item.name != item.primary_name && check_alias(&item.name) {
                return true;
            }
            let mut found = false;
            for_each_symbol_suffix(&item.primary_name, |suffix| {
                if check_alias(suffix) {
                    found = true;
                }
            });
            if found {
                return true;
            }
        }
        false
    }

    fn finish(self) -> CompactRelationIndex {
        let mut local_by_file: FxHashMap<
            SharedStr,
            FxHashMap<SharedStr, SmallVec<[CandidateSym; 2]>>,
        > = FxHashMap::default();
        let mut global: FxHashMap<SharedStr, SmallVec<[CandidateSym; 2]>> = FxHashMap::default();

        for item in &self.matched {
            let is_member_method = self.is_member_method(item);
            let cand = CandidateSym {
                id: item.id,
                node_type: item.node_type,
                is_member_method,
                identity: item.identity.clone(),
                parameters: item.parameters.clone(),
                ambiguous: false,
            };

            let mut push_alias = |alias: SharedStr| {
                push_candidate_deduped(global.entry(alias.clone()).or_default(), &cand);
                if let Some(ref file_key) = item.local_rel_file {
                    let file_map = local_by_file.entry(file_key.clone()).or_default();
                    push_candidate_deduped(file_map.entry(alias).or_default(), &cand);
                }
            };

            if !item.is_field {
                if item.name != item.primary_name {
                    push_alias(item.name.clone());
                }
                for_each_symbol_suffix(&item.primary_name, |suffix| {
                    push_alias(SharedStr::from(suffix));
                });
            }
            push_alias(item.primary_name.clone());
        }

        CompactRelationIndex {
            local_by_file,
            global,
            type_names: self.type_names,
            bases: FxHashMap::default(),
        }
    }
}

#[inline]
fn push_candidate_deduped(bucket: &mut SmallVec<[CandidateSym; 2]>, cand: &CandidateSym) {
    if let Some(existing) = bucket.iter_mut().find(|c| {
        c.node_type == cand.node_type
            && c.is_member_method == cand.is_member_method
            && c.identity == cand.identity
            && c.parameters == cand.parameters
    }) {
        if existing.id != cand.id {
            existing.ambiguous = true;
        }
        return;
    }
    bucket.push(cand.clone());
}

impl CompactRelationIndex {
    #[cfg(test)]
    fn from_nodes_for_test(nodes: &[Node]) -> Self {
        let mut tokens = FxHashSet::default();
        for n in nodes {
            tokens.insert(n.name.as_str());
            if let Some(qn) = n.qualified_name.as_deref() {
                tokens.insert(qn);
            }
        }
        let req = TailResolutionRequirements {
            tokens,
            has_empty_method_probe: true,
            need_inheritance: true,
            need_member_methods: true,
            need_param_identity: true,
        };
        let mut builder = CompactRelationIndexBuilder::new(Path::new(""), &req);
        for n in nodes {
            builder.observe_node(n, true);
        }
        builder.finish()
    }

    #[inline]
    fn record_inheritance_edge(&mut self, from: Uuid, to: Uuid, edge_type: EdgeType) {
        if matches!(
            edge_type,
            EdgeType::Extends
                | EdgeType::Implements
                | EdgeType::IncludesClass
                | EdgeType::InheritsClass
        ) && let (Some(child), Some(parent)) =
            (self.type_names.get(&from), self.type_names.get(&to))
        {
            self.bases.entry(child.clone()).or_default().push((
                parent.clone(),
                matches!(edge_type, EdgeType::Implements | EdgeType::IncludesClass),
            ));
        }
    }

    fn resolve_symbol(
        &self,
        name: &str,
        file: &str,
        callable: bool,
        identity: Option<&str>,
    ) -> Option<Uuid> {
        let eligible = |node: &&CandidateSym| {
            (!callable || node.node_type == NodeType::Function)
                && identity.is_none_or(|value| node.identity.as_deref().unwrap_or("") == value)
        };
        if let Some(local_bucket) = self.local_by_file.get(file).and_then(|m| m.get(name)) {
            let mut local = local_bucket.iter().filter(eligible);
            if let Some(first) = local.next() {
                return (!first.ambiguous && local.all(|n| !n.ambiguous && n.id == first.id))
                    .then_some(first.id);
            }
        }
        let global_bucket = self.global.get(name)?;
        let mut global = global_bucket.iter().filter(eligible);
        let first = global.next()?;
        (!first.ambiguous && global.all(|n| !n.ambiguous && n.id == first.id)).then_some(first.id)
    }

    // Incremental edge repair obeys the same identity/argument evidence as cold
    // discovery; a rejected overload must not be resurrected by a bare-name lookup.
    fn resolve_call_target(
        &self,
        relation: &rgctl_plugin_api::Relation,
        file: &str,
    ) -> Option<Uuid> {
        let identity = relation.to_identity_discriminator_str();
        let arguments = relation.argument_identity_discriminator_str();
        if relation.explicit_generic_arity_val().is_some() && identity.is_none() {
            return None;
        }
        let direct = |name: &str, free_only: bool| -> Option<Option<Uuid>> {
            let global_bucket = self.global.get(name)?;
            if !global_bucket
                .iter()
                .any(|n| n.node_type == NodeType::Function)
            {
                return None;
            }
            let eligible = |n: &&CandidateSym| {
                n.node_type == NodeType::Function
                    && (!free_only || !n.is_member_method)
                    && identity.is_none_or(|value| n.identity.as_deref().unwrap_or("") == value)
                    && (identity.is_some()
                        || arguments.is_none_or(|args| {
                            n.identity.is_none()
                                || n.parameters.as_deref().or(n.identity.as_deref()) == Some(args)
                        }))
            };
            if let Some(local_bucket) = self.local_by_file.get(file).and_then(|m| m.get(name)) {
                let mut local = local_bucket.iter().filter(eligible);
                if let Some(first) = local.next() {
                    return Some(
                        (!first.ambiguous && local.all(|n| !n.ambiguous && n.id == first.id))
                            .then_some(first.id),
                    );
                }
            }
            let mut global = global_bucket.iter().filter(eligible);
            Some(
                global
                    .next()
                    .filter(|first| {
                        !first.ambiguous && global.all(|n| !n.ambiguous && n.id == first.id)
                    })
                    .map(|first| first.id),
            )
        };

        let target = relation
            .to_qualified_hint
            .as_deref()
            .unwrap_or(&relation.to);
        let inherited_overloads =
            relation.language_str() == Some("java") && arguments.is_some() && identity.is_none();
        let local = direct(target, false);
        if let Some(found) = local
            && (found.is_some() || !inherited_overloads)
        {
            return found;
        }
        if let Some((owner, method)) = split_callable_owner(target)
            && !self.bases.is_empty()
        {
            let mut seen = FxHashSet::default();
            let mut pending: Vec<_> = self
                .bases
                .get(owner)
                .into_iter()
                .flatten()
                .map(|(name, iface)| (name, *iface))
                .collect();
            let mut class_target = None;
            let mut interface_target = None;
            let mut class_ambiguous = false;
            let mut interface_ambiguous = false;
            let mut probe_buf = String::new();
            while let Some((ancestor, interface_path)) = pending.pop() {
                if !seen.insert((ancestor, interface_path)) {
                    continue;
                }
                let mut family = false;
                for separator in [".", "::", "#"] {
                    probe_buf.clear();
                    probe_buf.reserve(ancestor.len() + separator.len() + method.len());
                    probe_buf.push_str(ancestor);
                    probe_buf.push_str(separator);
                    probe_buf.push_str(method);
                    if let Some(candidate) = direct(&probe_buf, false) {
                        family = true;
                        let Some(id) = candidate else {
                            if inherited_overloads {
                                family = false;
                                break;
                            }
                            return None;
                        };
                        let (slot, ambiguous) = if interface_path {
                            (&mut interface_target, &mut interface_ambiguous)
                        } else {
                            (&mut class_target, &mut class_ambiguous)
                        };
                        if slot.is_some_and(|previous| previous != id) {
                            *ambiguous = true;
                        }
                        *slot = Some(id);
                        break;
                    }
                }
                if !family {
                    pending.extend(
                        self.bases
                            .get(ancestor)
                            .into_iter()
                            .flatten()
                            .map(|(name, iface)| (name, interface_path || *iface)),
                    );
                }
            }
            if class_target.is_some() || class_ambiguous {
                return class_target.filter(|_| !class_ambiguous);
            }
            if interface_target.is_some() || interface_ambiguous {
                return interface_target.filter(|_| !interface_ambiguous);
            }
        }
        if local.is_none() && relation.is_inferred_callable_owner() && identity.is_none() {
            return direct(&relation.to, true).flatten();
        }
        None
    }
}

fn resolve_tails_against_index(
    index: &CompactRelationIndex,
    tails: &[ExtractionTail],
    repo_root: &Path,
    thread_count: Option<usize>,
) -> Vec<(Uuid, Uuid, EdgeType)> {
    let per_file = par_map(thread_count, tails, |tail| {
        if tail.relations.is_empty() {
            return Vec::new();
        }
        let file = relative_path(repo_root, &tail.path)
            .unwrap_or_else(|_| normalize_path_str(&tail.path.to_string_lossy()));
        let mut batch = Vec::with_capacity(tail.relations.len());
        for relation in &tail.relations {
            let callable = relation.relation_type == rgctl_plugin_api::RelationType::Calls;
            let from_id = index.resolve_symbol(
                &relation.from,
                &file,
                callable,
                relation.from_identity_discriminator_str(),
            );
            let to_id = if callable {
                index.resolve_call_target(relation, &file)
            } else {
                index.resolve_symbol(
                    relation
                        .to_qualified_hint
                        .as_deref()
                        .unwrap_or(&relation.to),
                    &file,
                    false,
                    None,
                )
            };
            if let (Some(from), Some(to)) = (from_id, to_id) {
                batch.push((from, to, relation_type_to_edge(relation.relation_type)));
            }
        }
        batch
    });

    let total: usize = per_file.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(total);
    for batch in per_file {
        out.extend(batch);
    }
    out
}

/// Resolve cross-file edges from already-extracted `tails` directly against kept base mmap
/// columns + `delta.new_nodes`, appending unique edges into `delta.new_edges` before compaction.
fn resolve_cross_file_edges_from_tails(
    base: &ColumnarGraphMmap,
    delta: &mut DeltaSegment,
    tails: &[ExtractionTail],
    repo_root: &Path,
    thread_count: Option<usize>,
) -> Result<()> {
    if tails.is_empty() {
        return Ok(());
    }
    let requirements = TailResolutionRequirements::from_tails(tails);
    if requirements.tokens.is_empty() && !requirements.has_empty_method_probe {
        return Ok(());
    }

    let mut builder = CompactRelationIndexBuilder::new(repo_root, &requirements);
    let base_count = base.node_count();
    let mut kept_base_count = 0usize;
    let mut kept_base_ids: FxHashSet<Uuid> = FxHashSet::default();
    for idx in 0..base_count {
        if base.is_node_invalidated_at(idx, &delta.invalidated_files)? {
            continue;
        }
        kept_base_count += 1;
        if requirements.need_inheritance {
            kept_base_ids.insert(base.node_id_at(idx)?);
        }
        builder.observe_kept_base_row(base, idx)?;
    }
    // When every base node was invalidated, `GraphBuilder::populate_pass2` already had
    // full visibility over the entire repository and resolved all edges authoritatively.
    if kept_base_count == 0 {
        return Ok(());
    }
    let delta_ids: FxHashSet<Uuid> = delta.new_nodes.iter().map(|n| n.id).collect();
    for node in &delta.new_nodes {
        builder.observe_node(node, true);
    }

    let mut index = builder.finish();
    if requirements.need_inheritance {
        base.for_each_edge(|from, to, edge_type| {
            if matches!(
                edge_type,
                EdgeType::Extends
                    | EdgeType::Implements
                    | EdgeType::IncludesClass
                    | EdgeType::InheritsClass
            ) && kept_base_ids.contains(&from)
                && !delta_ids.contains(&from)
                && (kept_base_ids.contains(&to) || delta_ids.contains(&to))
            {
                index.record_inheritance_edge(from, to, edge_type);
            }
            Ok(())
        })?;
        for edge in &delta.new_edges {
            index.record_inheritance_edge(edge.from, edge.to, edge.edge_type);
        }
    }

    let resolved = resolve_tails_against_index(&index, tails, repo_root, thread_count);
    if resolved.is_empty() {
        return Ok(());
    }

    let mut existing: FxHashSet<(Uuid, Uuid, u8)> = FxHashSet::with_capacity_and_hasher(
        delta.new_edges.len() + resolved.len(),
        Default::default(),
    );
    for edge in &delta.new_edges {
        existing.insert((edge.from, edge.to, edge_type_to_u8(edge.edge_type)));
    }
    for (from, to, edge_type) in resolved {
        // Intra-delta edges (`from` and `to` both extracted in this delta) were already
        // evaluated by `GraphBuilder::populate_pass2`; only stitch edges reaching kept base nodes.
        if delta_ids.contains(&from) && delta_ids.contains(&to) {
            continue;
        }
        if existing.insert((from, to, edge_type_to_u8(edge_type))) {
            delta.new_edges.push(Edge::new(from, to, edge_type));
        }
    }
    Ok(())
}

fn split_callable_owner(name: &str) -> Option<(&str, &str)> {
    name.rsplit_once("::")
        .or_else(|| name.rsplit_once('#'))
        .or_else(|| name.rsplit_once('.'))
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
        RelationType::ExposesRoute => EdgeType::ExposesRoute,
        RelationType::Injects => EdgeType::Injects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts_key_for;
    use rgctl_graph::schema::NodeType;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn resolve_symbol_rejects_ambiguous_bare_names() {
        let a = Node::new(NodeType::Function, "sibling_fn").with_file_path("a.rs");
        let b = Node::new(NodeType::Function, "sibling_fn").with_file_path("b.rs");
        let helper = Node::new(NodeType::Function, "helper").with_file_path("a.rs");
        let index = CompactRelationIndex::from_nodes_for_test(&[a.clone(), b, helper.clone()]);
        assert_eq!(
            index.resolve_symbol("sibling_fn", "a.rs", true, None),
            Some(a.id)
        );
        assert_eq!(index.resolve_symbol("sibling_fn", "c.rs", true, None), None);
        assert_eq!(
            index.resolve_symbol("helper", "missing.rs", true, None),
            Some(helper.id)
        );
    }

    #[test]
    fn relation_rebuild_preserves_kinds_and_rejects_ambiguous_overloads() {
        let field = Node::new(NodeType::Variable, "value").with_file_path("box.js");
        let method = Node::new(NodeType::Function, "value").with_file_path("box.js");
        let index = CompactRelationIndex::from_nodes_for_test(&[field.clone(), method.clone()]);
        assert_eq!(
            index.resolve_symbol("value", "box.js", true, None),
            Some(method.id)
        );
        assert_eq!(index.resolve_symbol("value", "box.js", false, None), None);

        let overload = method.clone().with_identity_discriminator("(int)");
        let overloaded_index =
            CompactRelationIndex::from_nodes_for_test(&[field, method, overload.clone()]);
        assert_eq!(
            overloaded_index.resolve_symbol("value", "box.js", true, None),
            None
        );
        assert_eq!(
            overloaded_index.resolve_symbol("value", "box.js", true, Some("(int)")),
            Some(overload.id)
        );
    }

    #[test]
    fn global_bucket_caps_thousands_of_duplicate_names_to_single_ambiguous_slot() {
        let mut nodes: Vec<Node> = (0..1_000)
            .map(|i| Node::new(NodeType::Function, "new").with_file_path(format!("f{i}.rs")))
            .collect();
        let distinct_overload = Node::new(NodeType::Function, "new")
            .with_file_path("overloaded.java")
            .with_identity_discriminator("(int)");
        let overload_id = distinct_overload.id;
        nodes.push(distinct_overload);

        let index = CompactRelationIndex::from_nodes_for_test(&nodes);
        let bucket = index.global.get("new").expect("new bucket");
        assert_eq!(
            bucket.len(),
            2,
            "1000 non-overloaded `new` callables + 1 `(int)` overload must compress to 2 slots"
        );
        assert_eq!(index.resolve_symbol("new", "caller.rs", true, None), None);
        assert_eq!(
            index.resolve_symbol("new", "caller.rs", true, Some("(int)")),
            Some(overload_id)
        );
    }

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
                &[root.join("src/main.rs"), root.join("src/auth.rs")],
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
            !functions.iter().any(|n| &*n.name == "login"),
            "renamed callee `login` must be dropped from src/auth.rs"
        );
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
    fn facts_cache_warm_hit_skips_reparse() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        let path = root.join("src/lib.rs");
        fs::write(&path, "pub fn alpha() {}\n").unwrap();
        let source = fs::read(&path).unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let facts = Arc::new(FactsCache::new());
        let extractor = Extractor::new(Arc::clone(&registry)).with_facts_cache(Arc::clone(&facts));

        let first = extractor
            .extract_file_with_source(&path, source.clone())
            .unwrap();
        assert_eq!(facts.miss_count(), 1);
        assert_eq!(facts.hit_count(), 0);
        assert!(!first.symbols.is_empty());

        facts.save(root).unwrap();
        assert!(FactsCache::path_for(root).is_file());

        let reloaded = Arc::new(FactsCache::load(root));
        let extractor2 =
            Extractor::new(Arc::clone(&registry)).with_facts_cache(Arc::clone(&reloaded));
        let second = extractor2.extract_file_with_source(&path, source).unwrap();
        assert_eq!(reloaded.hit_count(), 1);
        assert_eq!(reloaded.miss_count(), 0);
        assert_eq!(first.symbols.len(), second.symbols.len());
        assert_eq!(first.symbols[0].name, second.symbols[0].name);

        let plugin = registry.get_plugin_for_file(&path).unwrap();
        let key = facts_key_for(plugin.as_ref(), fs::read(&path).unwrap().as_slice());
        assert!(reloaded.get(&key).is_some());
    }

    #[test]
    fn compact_leaves_prior_snapshot_when_tmp_orphan() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let pipeline = ProcessingPipeline::new(Arc::clone(&registry));
        let (graph, _) = pipeline.process_repository(root).unwrap();
        let mut tracker = FileTracker::new(root);
        tracker
            .index_files(&[root.join("src/main.rs")], &graph)
            .unwrap();
        tracker.save().unwrap();
        graph.save_to_repo(root).unwrap();
        graph.save_snapshot(root).unwrap();

        let snapshot = MmappedGraphSnapshot::default_path(root);
        let before = fs::read(&snapshot).unwrap();
        let nodes_before = graph.node_count();

        // Simulate crash mid-compact: orphan .tmp without replacing the live snapshot.
        let tmp = snapshot.with_extension("bin.tmp");
        fs::write(&tmp, b"truncated-compact").unwrap();
        assert_eq!(fs::read(&snapshot).unwrap(), before);
        assert_eq!(
            CodeGraph::open_snapshot(&snapshot).unwrap().node_count(),
            nodes_before,
            "live snapshot must remain readable after orphan tmp"
        );
        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn single_file_edit_cleans_stale_call_edge() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            "mod util;\nfn main() { util::old_helper(); }\n",
        )
        .unwrap();
        fs::write(root.join("src/util.rs"), "pub fn old_helper() {}\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let pipeline = ProcessingPipeline::new(Arc::clone(&registry));
        let (mut graph, _) = pipeline.process_repository(root).unwrap();
        let mut tracker = FileTracker::new(root);
        tracker
            .index_files(
                &[root.join("src/main.rs"), root.join("src/util.rs")],
                &graph,
            )
            .unwrap();
        tracker.save().unwrap();
        graph.save_to_repo(root).unwrap();
        graph.save_snapshot(root).unwrap();

        let functions_before = graph.find_by_type(NodeType::Function).unwrap();
        let old_id = functions_before
            .iter()
            .find(|n| &*n.name == "old_helper")
            .map(|n| n.id)
            .expect("old_helper before edit");
        let main_id = functions_before
            .iter()
            .find(|n| &*n.name == "main")
            .map(|n| n.id)
            .expect("main");
        assert!(
            graph.backend().has_edge(main_id, old_id, EdgeType::Calls),
            "precondition: main→old_helper"
        );

        fs::write(root.join("src/util.rs"), "pub fn new_helper() {}\n").unwrap();
        fs::write(
            root.join("src/main.rs"),
            "mod util;\nfn main() { util::new_helper(); }\n",
        )
        .unwrap();

        let updater = IncrementalUpdater::with_options(
            Arc::clone(&registry),
            UpdateOptions {
                cascade_depth: 1,
                show_progress: false,
                ..Default::default()
            },
        );
        updater
            .update_files(
                &mut graph,
                root,
                &["src/util.rs".into(), "src/main.rs".into()],
            )
            .unwrap();

        let functions = graph.find_by_type(NodeType::Function).unwrap();
        assert!(
            functions.iter().any(|n| &*n.name == "new_helper"),
            "expected new_helper"
        );
        assert!(
            !functions.iter().any(|n| &*n.name == "old_helper"),
            "old_helper must be dropped on compact"
        );
        let main_id = functions.iter().find(|n| &*n.name == "main").unwrap().id;
        let new_id = functions
            .iter()
            .find(|n| &*n.name == "new_helper")
            .unwrap()
            .id;
        assert!(
            graph.backend().has_edge(main_id, new_id, EdgeType::Calls),
            "main→new_helper call edge required after edit"
        );
        assert!(
            !graph.backend().has_edge(main_id, old_id, EdgeType::Calls),
            "stale main→old_helper edge must be cleaned up"
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
