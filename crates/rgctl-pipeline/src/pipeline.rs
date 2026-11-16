//! Parallel processing pipeline
//!
//! Task 1.6.2: Parallel file parsing with rayon

use crate::stream::{DEFAULT_STREAM_CHANNEL_CAPACITY, LosslessSidecarBatch, stream_into_graph};
use indicatif::{ProgressBar, ProgressStyle};
use rgctl_error::Result;
use rgctl_extraction::discovery::{DiscoveryConfig, FileDiscoverer};
use rgctl_extraction::{Extractor, FactsCache, GraphBuilder};
use rgctl_graph::code_graph::CodeGraph;
use rgctl_graph::content_store::ContentStore;
use rgctl_graph::schema::{Edge, Node};
use rgctl_graph::write_columnar_from_spill;
use rgctl_registry::LanguageRegistry;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

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
    /// Max in-flight extractions between parallel workers and graph merge
    pub stream_channel_capacity: usize,
    /// Materialize `Symbol.fields` as Variable graph nodes (CPG / `--with-cfg`).
    pub materialize_fields: bool,
    /// Populate / consult `.rgctl/cache/facts.bin` during extract (warm re-runs).
    ///
    /// Disable for constrained cold profiles that must not retain per-file JSON
    /// payloads in RAM. Default `true` — cache stores encoded bytes only (no
    /// decoded `ExtractAllResult` trees).
    pub facts_cache: bool,
    /// Persist RGCS v2 content store + RGCT Green-CST (`discover --lossless`).
    pub lossless: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            discovery: DiscoveryConfig::default(),
            show_progress: true,
            thread_count: None,
            batch_size: 64,
            stream_channel_capacity: DEFAULT_STREAM_CHANNEL_CAPACITY,
            materialize_fields: false,
            facts_cache: true,
            lossless: false,
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
    /// Wall time for parallel extract + sequential pass-1 (`stream_into_graph`)
    pub extract_duration: Duration,
    /// Sum of `fs::read` across extract workers (may exceed [`Self::extract_duration`])
    pub extract_read_cpu: Duration,
    /// Sum of plugin parse/extract across workers (may exceed [`Self::extract_duration`])
    pub extract_parse_cpu: Duration,
    /// Sequential pass-1 merge wall inside extract
    pub extract_pass1_wall: Duration,
    /// Time spent merging extractions into the graph (indexes + pass-2 + spill/columnar)
    pub graph_build_duration: Duration,
    /// Build symbol resolution indexes (subset of [`Self::graph_build_duration`])
    pub graph_resolution_index: Duration,
    /// Pass-2 relation / config-usage resolution
    pub graph_pass2: Duration,
    /// Spill finish + columnar snapshot compile (snapshot path only)
    pub graph_spill_columnar: Duration,
    /// Path → node ids collected during extract (skips full mmap scan in save_tracker)
    pub node_path_mapping: HashMap<String, Vec<Uuid>>,
    /// Absolute path → BLAKE3 hex from extract workers (skips re-hash in save_tracker)
    pub file_hashes: HashMap<String, String>,
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
        let (nodes, edges, mut stats) = self.extract_repository(root)?;
        let load_start = Instant::now();
        let mut graph = CodeGraph::new();
        graph.load(nodes, edges)?;
        stats.graph_build_duration += load_start.elapsed();
        stats.duration = start.elapsed();
        Ok((graph, stats))
    }

    /// Discover, extract, and write a columnar snapshot without building [`CodeGraph`].
    ///
    /// Spills nodes/edges to disk during extract, then externally sorts and compiles
    /// the columnar snapshot (no full `Vec<Node>` / `Vec<Edge>` residency).
    ///
    /// `source_root` is scanned for files; `artifact_root` receives spill/content/code
    /// artifacts (defaults to `source_root` when unset).
    pub fn process_repository_to_snapshot(
        &self,
        source_root: &Path,
        snapshot_path: &Path,
        artifact_root: Option<&Path>,
    ) -> Result<(PipelineStats, String)> {
        let start = Instant::now();
        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let files = discoverer.discover(source_root)?;
        let (mut stats, digest) =
            self.process_files_to_snapshot(source_root, &files, snapshot_path, artifact_root)?;
        stats.duration = start.elapsed();
        Ok((stats, digest))
    }

    /// Extract a pre-discovered file list and write a columnar snapshot without re-walking `source_root`.
    pub fn process_files_to_snapshot(
        &self,
        source_root: &Path,
        files: &[std::path::PathBuf],
        snapshot_path: &Path,
        artifact_root: Option<&Path>,
    ) -> Result<(PipelineStats, String)> {
        let start = Instant::now();
        let files_discovered = files.len();

        let progress = self.make_progress(files_discovered);
        let store = artifact_root.unwrap_or(source_root);
        let facts = if self.config.facts_cache {
            let loaded = FactsCache::load(store);
            // On cold large repos (>5k files with no existing facts.bin), skip populating
            // per-file JSON payloads during the initial full spill index so cold discover
            // does not pay multi-hundred-MB JSON encode + disk write + Drop overhead.
            if loaded.is_empty() && files_discovered > 5_000 {
                None
            } else {
                Some(Arc::new(loaded))
            }
        } else {
            None
        };
        let mut extractor = Extractor::new(Arc::clone(&self.registry))
            .with_lossless(self.config.lossless)
            .with_repo_root(store.to_path_buf());
        if let Some(ref facts) = facts {
            extractor = extractor.with_facts_cache(Arc::clone(facts));
        }
        let extract_start = Instant::now();

        let spill_dir = rgctl_graph::paths::artifact_path(store, "spill");
        if spill_dir.exists() {
            std::fs::remove_dir_all(&spill_dir)?;
        }
        std::fs::create_dir_all(&spill_dir)?;
        let mut builder = GraphBuilder::with_spill(&spill_dir)?;
        builder.set_materialize_fields(self.config.materialize_fields);
        // Default discover: do not load/store CodeIndex bodies (multi-GB on linux).
        // Nodes still get `code_hash` via worker-precomputed prep / `hash_code`.
        // When `--lossless`, RGCS v2 owns `content_store.bin` — skip the legacy blob vault.
        if !self.config.lossless {
            builder.set_content_store(ContentStore::load(ContentStore::default_path(store))?);
        }

        let progress_for_stream = progress.clone();
        let (stream_stats, tails, lossless_sidecars) = stream_into_graph(
            self.config.thread_count,
            &extractor,
            Arc::clone(&self.registry),
            files,
            self.config.stream_channel_capacity,
            &mut builder,
            move || {
                if let Some(pb) = &progress_for_stream {
                    pb.inc(1);
                }
            },
        )?;
        let extract_duration = extract_start.elapsed();

        if let Some(pb) = progress {
            pb.finish_with_message("done");
        }

        if self.config.lossless {
            write_lossless_sidecars(store, lossless_sidecars)?;
        } else {
            drop(lossless_sidecars);
        }

        let files_processed = stream_stats.files_processed;
        let files_failed = stream_stats.extraction_failures.len();
        let extract_phases = stream_stats.extract_phases;
        let file_hashes = stream_stats.file_hashes;

        let graph_start = Instant::now();
        let index_start = Instant::now();
        builder.build_resolution_indexes();
        let index_elapsed = index_start.elapsed();
        let pass2_start = Instant::now();
        extractor.populate_pass2(&tails, &mut builder)?;
        let pass2_elapsed = pass2_start.elapsed();
        rgctl_extraction::callable_resolution::save(store, &mut builder)?;
        builder.log_resolution_stats();
        let nodes_created = builder.node_count();
        let edges_created = builder.edge_count();
        let content_store = builder.take_content_store();
        let node_path_mapping = builder.take_tracker_mapping();
        let annotation_args = builder.take_annotation_args();
        let parked_relations = builder.take_parked_relations();
        let spill_start = Instant::now();
        let finished = builder.finish_spill()?;
        let digest = write_columnar_from_spill(finished, snapshot_path)?;
        rgctl_extraction::callable_resolution::mark_current(store)?;
        if !annotation_args.is_empty()
            && let Some(parent) = snapshot_path.parent()
        {
            let args_path = parent.join("annotation_args.json");
            let payload = serde_json::json!({
                "schema_version": 1,
                "command": "annotation_args",
                "entries": annotation_args,
            });
            if let Ok(json) = serde_json::to_string_pretty(&payload) {
                let _ = std::fs::write(&args_path, json);
            }
        }
        if !parked_relations.is_empty()
            && let Some(parent) = snapshot_path.parent()
        {
            let parked_path = parent.join(rgctl_extraction::PARKED_RELATIONS_FILE);
            if let Err(err) =
                rgctl_extraction::save_parked_relations(&parked_path, &parked_relations)
            {
                tracing::warn!(error = %err, "failed to persist parked_relations.bin");
            } else {
                tracing::debug!(
                    parked = parked_relations.len(),
                    path = %parked_path.display(),
                    "wrote parked_relations.bin"
                );
            }
        }
        let spill_elapsed = spill_start.elapsed();
        tracing::info!(
            resolution_index_secs = index_elapsed.as_secs_f64(),
            pass2_relation_resolution_secs = pass2_elapsed.as_secs_f64(),
            spill_and_columnar_secs = spill_elapsed.as_secs_f64(),
            extract_read_cpu_secs = extract_phases.read_cpu.as_secs_f64(),
            extract_parse_cpu_secs = extract_phases.parse_cpu.as_secs_f64(),
            extract_pass1_wall_secs = extract_phases.pass1_wall.as_secs_f64(),
            "graph build sub-phase timings"
        );
        if let Some(store) = content_store {
            store.save()?;
        }
        if let Some(ref facts) = facts {
            if let Err(err) = facts.save(store) {
                tracing::warn!(error = %err, "failed to persist facts.bin cache");
            } else {
                tracing::debug!(
                    facts_hits = facts.hit_count(),
                    facts_misses = facts.miss_count(),
                    facts_entries = facts.len(),
                    "facts.bin cache saved"
                );
            }
        }
        let graph_build_duration = graph_start.elapsed();

        Ok((
            PipelineStats {
                files_discovered,
                files_processed,
                files_failed,
                nodes_created,
                edges_created,
                duration: start.elapsed(),
                extract_duration,
                extract_read_cpu: extract_phases.read_cpu,
                extract_parse_cpu: extract_phases.parse_cpu,
                extract_pass1_wall: extract_phases.pass1_wall,
                graph_build_duration,
                graph_resolution_index: index_elapsed,
                graph_pass2: pass2_elapsed,
                graph_spill_columnar: spill_elapsed,
                node_path_mapping,
                file_hashes,
            },
            digest,
        ))
    }

    fn make_progress(&self, files_discovered: usize) -> Option<ProgressBar> {
        if self.config.show_progress && files_discovered > 0 {
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
        }
    }

    fn extract_repository(&self, root: &Path) -> Result<(Vec<Node>, Vec<Edge>, PipelineStats)> {
        let start = Instant::now();
        let discoverer =
            FileDiscoverer::with_config(Arc::clone(&self.registry), self.config.discovery.clone());
        let files = discoverer.discover(root)?;
        let files_discovered = files.len();

        let progress = self.make_progress(files_discovered);
        let facts = if self.config.facts_cache {
            Some(Arc::new(FactsCache::load(root)))
        } else {
            None
        };
        let mut extractor = Extractor::new(Arc::clone(&self.registry))
            .with_lossless(self.config.lossless)
            .with_repo_root(root.to_path_buf());
        if let Some(ref facts) = facts {
            extractor = extractor.with_facts_cache(Arc::clone(facts));
        }
        let extract_start = Instant::now();
        let mut builder = GraphBuilder::new();
        builder.set_materialize_fields(self.config.materialize_fields);
        if !self.config.lossless {
            builder.set_content_store(ContentStore::load(ContentStore::default_path(root))?);
        }
        let progress_for_stream = progress.clone();
        let (stream_stats, tails, lossless_sidecars) = stream_into_graph(
            self.config.thread_count,
            &extractor,
            Arc::clone(&self.registry),
            &files,
            self.config.stream_channel_capacity,
            &mut builder,
            move || {
                if let Some(pb) = &progress_for_stream {
                    pb.inc(1);
                }
            },
        )?;
        let extract_duration = extract_start.elapsed();

        if let Some(pb) = progress {
            pb.finish_with_message("done");
        }

        if self.config.lossless {
            write_lossless_sidecars(root, lossless_sidecars)?;
        } else {
            drop(lossless_sidecars);
        }

        let files_processed = stream_stats.files_processed;
        let files_failed = stream_stats.extraction_failures.len();
        let extract_phases = stream_stats.extract_phases;
        let file_hashes = stream_stats.file_hashes;

        let graph_start = Instant::now();
        let index_start = Instant::now();
        builder.build_resolution_indexes();
        let index_elapsed = index_start.elapsed();
        let pass2_start = Instant::now();
        extractor.populate_pass2(&tails, &mut builder)?;
        let pass2_elapsed = pass2_start.elapsed();
        rgctl_extraction::callable_resolution::save(root, &mut builder)?;
        if let Some(store) = builder.take_content_store() {
            store.save()?;
        }
        if let Some(ref facts) = facts
            && let Err(err) = facts.save(root)
        {
            tracing::warn!(error = %err, "failed to persist facts.bin cache");
        }
        let node_path_mapping = builder.take_tracker_mapping();
        let (nodes, edges): (Vec<Node>, Vec<Edge>) = builder.into_graph();
        let graph_build_duration = graph_start.elapsed();

        let nodes_created = nodes.len();
        let edges_created = edges.len();
        Ok((
            nodes,
            edges,
            PipelineStats {
                files_discovered,
                files_processed,
                files_failed,
                nodes_created,
                edges_created,
                duration: start.elapsed(),
                extract_duration,
                extract_read_cpu: extract_phases.read_cpu,
                extract_parse_cpu: extract_phases.parse_cpu,
                extract_pass1_wall: extract_phases.pass1_wall,
                graph_build_duration,
                graph_resolution_index: index_elapsed,
                graph_pass2: pass2_elapsed,
                graph_spill_columnar: Duration::ZERO,
                node_path_mapping,
                file_hashes,
            },
        ))
    }
}

fn write_lossless_sidecars(artifact_root: &Path, batch: LosslessSidecarBatch) -> Result<()> {
    if batch.frames.is_empty() && batch.cst_files.is_empty() {
        return Ok(());
    }
    let rgctl = artifact_root.join(".rgctl");
    std::fs::create_dir_all(&rgctl)?;
    if !batch.frames.is_empty() {
        let mut writer = rgctl_graph::LosslessContentWriter::new();
        for frame in batch.frames {
            writer.push(frame);
        }
        writer.write_to_path(&rgctl.join(rgctl_graph::LOSSLESS_CONTENT_STORE_FILE))?;
    }
    if !batch.cst_files.is_empty() {
        let mut writer = rgctl_graph::CstArchiveWriter::new();
        for file in batch.cst_files {
            writer.push_file(file);
        }
        writer.write_to_path(&rgctl.join(rgctl_graph::CST_ARCHIVE_FILE))?;
    }
    Ok(())
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
        let pipeline =
            ProcessingPipeline::with_config(Arc::new(rgctl_languages::default_registry()), config);
        let (graph, stats) = pipeline.process_repository(temp.path()).unwrap();

        assert_eq!(stats.files_processed, 10);
        assert!(graph.node_count() > 10);
    }
}
