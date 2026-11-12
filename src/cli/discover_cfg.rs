//! Parallel streaming CFG/PDG/taint analysis for discover `--with-cfg` / `--with-taint`.

use crate::analysis::cfg_pdg_archive::{StreamingCfgPdgArchiveWriter, stable_record_uuid};
use crate::analysis::hash_maps::FxHashMap;
use crate::analysis::storage::analysis_cache_key;
use crate::analysis::{
    AnalysisIndexEntry, AnalysisStorage, AstSkeletonRecord, CfgPdgRecord, ControlFlowGraph,
    DominatorTree, FieldWrite, ParsedSourceFile, PdgBuildOptions, ProgramDependenceGraph,
    TaintAnalyzer, TaintFlow, TaintRuleSet, build_cfg_for_function,
    build_file_local_types_from_tree, build_function_skeleton_from_tree, cfg_language_id_from_path,
    extract_writes_from_cfg, type_env_for_field_writes,
};
use rayon::prelude::*;
use rgctl_graph::code_index::hash_code;
use rgctl_graph::columnar_snapshot::ColumnarGraphMmap;
use rgctl_graph::schema::{Node, NodeType};
use rgctl_graph::snapshot::SnapshotNodeStore;
use rgctl_pipeline::{with_large_pool, with_large_stack, with_pool};
use serde::Serialize;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;
use tracing::info;
use uuid::Uuid;

/// Options for the CFG analysis batch.
#[derive(Debug, Clone, Default)]
pub struct CfgAnalysisOptions {
    /// Emit per-stage and tail-latency profile lines.
    pub verbose: bool,
    /// Optional Rayon thread count (`None` = global pool default).
    pub thread_count: Option<usize>,
    /// Run discover-time taint after PDG (`--with-taint`).
    pub enable_taint: bool,
    /// Tag loop-carried data deps on the PDG (`--with-dfg-loops`).
    pub dfg_loops: bool,
    /// Compiled taint rule packs (built-in + overlays). Shared across workers.
    pub taint_rules: Option<Arc<TaintRuleSet>>,
    /// Extract AST skeletons on workers (`--with-ast-skeleton`).
    pub with_ast_skeleton: bool,
    /// Optional target `(archive_path, graph_digest)` for direct streaming v3 archive writes.
    pub archive_output: Option<(PathBuf, String)>,
}

/// Why a function was not CFG-analyzed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CfgSkipReason {
    /// Extension / language has no CFG builder.
    UnsupportedLanguage,
    /// Source text was not available for the function's file.
    MissingSource,
    /// Prototype, interface method, or abstract declaration with no executable body.
    BodilessDeclaration,
    /// Parse or CFG/PDG construction failed.
    AnalysisError,
}

impl CfgSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedLanguage => "unsupported_language",
            Self::MissingSource => "missing_source",
            Self::BodilessDeclaration => "bodiless_declaration",
            Self::AnalysisError => "analysis_error",
        }
    }
}

/// One skipped function (populated when `verbose` is set).
#[derive(Debug, Clone)]
pub struct CfgSkipEvent {
    pub file_path: String,
    pub symbol: String,
    pub reason: CfgSkipReason,
}

/// Wall-clock totals for CFG sub-stages (sum of per-function work).
#[derive(Debug, Clone, Copy, Default)]
pub struct CfgStageProfile {
    pub build_cfg_secs: f64,
    pub dominator_secs: f64,
    pub pdg_secs: f64,
    pub taint_secs: f64,
}

/// Aggregated CFG pass results for discover reporting and archive export.
#[derive(Debug, Default)]
pub struct CfgAnalysisBatchResult {
    pub success_count: usize,
    pub error_count: usize,
    pub skip_unsupported_language: usize,
    pub skip_missing_source: usize,
    pub skip_bodiless_declaration: usize,
    pub skip_analysis_error: usize,
    pub skips: Vec<CfgSkipEvent>,
    pub total_flows: usize,
    pub vulnerable_flows: usize,
    pub cache_hits: usize,
    pub recomputed: usize,
    pub skipped_unchanged: usize,
    pub orphans_removed: usize,
    /// Pre-encoded CFG/PDG archive records `(function_id, bincode)` when `archive_output` is `None`.
    pub archive_record_bytes: Vec<(Uuid, Vec<u8>)>,
    /// Legacy in-memory records (empty when worker-side encode is used).
    pub archive_records: Vec<CfgPdgRecord>,
    pub archive_unchanged: bool,
    /// True when the v3 CFG/PDG archive was streamed directly to disk during the batch.
    pub archive_streamed: bool,
    /// Wall-clock seconds spent finalizing/flushing the streamed archive TOC + rename.
    pub archive_finish_secs: f64,
    /// Field writes extracted on workers (fused; skip archive re-open when non-empty).
    pub field_writes: Vec<FieldWrite>,
    /// Optional AST skeletons extracted on workers.
    pub skeletons: Vec<AstSkeletonRecord>,
    /// Index entries for recomputed functions (worker-side serialize path).
    pub index_entries: Vec<AnalysisIndexEntry>,
    pub stage_profile: Option<CfgStageProfile>,
}

impl CfgAnalysisBatchResult {
    fn record_skip(&mut self, reason: CfgSkipReason, file_path: &str, symbol: &str, verbose: bool) {
        if reason == CfgSkipReason::BodilessDeclaration {
            self.skip_bodiless_declaration += 1;
            return;
        }
        self.error_count += 1;
        match reason {
            CfgSkipReason::UnsupportedLanguage => self.skip_unsupported_language += 1,
            CfgSkipReason::MissingSource => self.skip_missing_source += 1,
            CfgSkipReason::BodilessDeclaration => {}
            CfgSkipReason::AnalysisError => self.skip_analysis_error += 1,
        }
        if verbose {
            self.skips.push(CfgSkipEvent {
                file_path: file_path.to_string(),
                symbol: symbol.to_string(),
                reason,
            });
        }
    }
}

#[derive(Default)]
struct CfgStageTimings {
    build_cfg_ns: AtomicU64,
    dominator_ns: AtomicU64,
    pdg_ns: AtomicU64,
    taint_ns: AtomicU64,
    functions: AtomicU64,
}

struct CfgFunctionTiming {
    file_path: String,
    function_name: String,
    blocks: usize,
    total_ns: u64,
}

struct CfgFunctionWork {
    function_id: Uuid,
    /// Stable cache key (`analysis_cache_key`) for orphan purging without a full `Vec<Node>`.
    active_key: Option<String>,
    /// Pre-encoded [`crate::analysis::FunctionAnalysis`] bincode (worker-side).
    analysis_bytes: Option<Vec<u8>>,
    /// Pre-encoded [`CfgPdgRecord`] bincode (worker-side).
    archive_bytes: Option<Vec<u8>>,
    /// Precomputed [`stable_record_uuid`] for v3 archive TOC alias (avoids writer deserialize).
    archive_alias_uuid: Option<Uuid>,
    index_entry: Option<AnalysisIndexEntry>,
    field_writes: Vec<FieldWrite>,
    skeleton: Option<AstSkeletonRecord>,
    flow_count: usize,
    vulnerable_count: usize,
    from_cache: bool,
    skip_persist: bool,
    blocks: usize,
}

struct CfgIncrementalCache {
    index: HashMap<String, AnalysisIndexEntry>,
}

struct FileWorkGroup {
    file_path: String,
    language: String,
    func_indices: Vec<usize>,
}

/// Optional preloaded file read cache keyed by node `file_path` strings.
pub struct FileSourceCache {
    sources: HashMap<String, Arc<String>>,
}

impl FileSourceCache {
    /// Source text for a function's `file_path` metadata field.
    pub fn get(&self, file_path: &str) -> Option<&str> {
        self.sources.get(file_path).map(|s| s.as_str())
    }

    /// All preloaded sources (for field-write fallback indexing).
    pub fn sources(&self) -> &HashMap<String, Arc<String>> {
        &self.sources
    }
}

fn resolve_read_path(repo_root: &Path, file_path: &str) -> PathBuf {
    let path = Path::new(file_path);
    if path.is_file() {
        path.to_path_buf()
    } else {
        repo_root.join(file_path)
    }
}

/// Read each unique function source file once (parallel when `thread_count` is set).
#[allow(dead_code)]
pub fn preload_file_sources(
    functions: &[Node],
    repo_root: &Path,
    thread_count: Option<usize>,
) -> FileSourceCache {
    let paths: HashSet<String> = functions
        .iter()
        .filter_map(|n| n.file_path.as_ref().map(|s| s.to_string()))
        .collect();
    let sources: HashMap<String, Arc<String>> = with_pool(thread_count, || {
        paths
            .par_iter()
            .filter_map(|path| {
                let read_path = resolve_read_path(repo_root, path);
                let content = std::fs::read_to_string(&read_path).ok()?;
                Some((path.clone(), Arc::new(content)))
            })
            .collect()
    });
    FileSourceCache { sources }
}

struct CfgWorkContext<'a> {
    functions: &'a [Node],
    columnar: Option<&'a ColumnarGraphMmap>,
    repo_root: &'a Path,
    sources: Option<&'a FileSourceCache>,
    cache: &'a CfgIncrementalCache,
    stage: Option<&'a CfgStageTimings>,
    timings: Option<&'a Mutex<Vec<CfgFunctionTiming>>>,
    enable_taint: bool,
    dfg_loops: bool,
    taint_rules: Option<&'a Arc<TaintRuleSet>>,
    with_ast_skeleton: bool,
}

fn is_bodiless_declaration_node(func: &Node) -> bool {
    func.get_property("callable_role") == Some("declaration")
}

fn alias_uuid_for(file_path: &str, function_name: &str, code_hash: &str) -> Option<Uuid> {
    if file_path.is_empty() || function_name.is_empty() || code_hash.is_empty() {
        None
    } else {
        Some(stable_record_uuid(file_path, function_name, code_hash))
    }
}

/// Analyze all repository functions in parallel with per-file streaming reads and bounded
/// channel persistence into [`AnalysisStorage`] and [`StreamingCfgPdgArchiveWriter`].
pub fn run_cfg_analysis_batch(
    functions: &[Node],
    node_store: Option<&SnapshotNodeStore>,
    storage: &AnalysisStorage,
    repo_root: &Path,
    options: CfgAnalysisOptions,
    file_sources: Option<&FileSourceCache>,
) -> CfgAnalysisBatchResult {
    let cache = load_incremental_cache(storage, repo_root);
    let columnar = if functions.is_empty() {
        node_store.and_then(|s| s.columnar())
    } else {
        None
    };

    if columnar.is_none()
        && let Some(shortcut) = try_full_incremental_shortcut(functions, &cache)
    {
        let active_keys = active_stable_keys(functions, None);
        let mut result = shortcut;
        result.orphans_removed = storage
            .purge_stale_by_stable_keys(&active_keys)
            .unwrap_or(0);
        return result;
    }

    let mut unsupported_skips: Vec<(String, String)> = Vec::new();
    let mut unsupported_count = 0usize;
    let mut groups = if let Some(col) = columnar {
        group_work_items_from_columnar(
            col,
            options.verbose,
            &mut unsupported_count,
            &mut unsupported_skips,
        )
    } else {
        group_work_items_by_file(functions)
    };
    groups.sort_by(|a, b| a.file_path.cmp(&b.file_path));

    let mut in_group = if columnar.is_none() {
        vec![false; functions.len()]
    } else {
        Vec::new()
    };
    let mut retained_ids: Vec<Uuid> = Vec::new();
    let mut retained_archive_entries: Vec<(Uuid, Option<Uuid>)> = Vec::new();

    if let Some(col) = columnar {
        // On cold runs `cache.index` is empty — skip decoding any cold extensions up front!
        if !cache.index.is_empty() {
            let mut all_cached = true;
            let mut total_flows = 0usize;
            let mut vulnerable_flows = 0usize;
            let mut matched = 0usize;
            let mut skipped_decls = 0usize;
            let mut active_keys = HashSet::new();
            for group in &groups {
                for &col_idx in &group.func_indices {
                    let Ok(func) = col.materialize_node_at(col_idx) else {
                        all_cached = false;
                        continue;
                    };
                    if is_bodiless_declaration_node(&func) {
                        skipped_decls += 1;
                        continue;
                    }
                    if let Some(code_hash) = func.code_hash.as_deref() {
                        active_keys.insert(analysis_cache_key(func.id, code_hash));
                        if let Some(hit) = try_fast_cache_hit(&func, code_hash, &cache) {
                            retained_ids.push(func.id);
                            let alias = alias_uuid_for(&group.file_path, &func.name, code_hash);
                            retained_archive_entries.push((func.id, alias));
                            matched += 1;
                            total_flows += hit.flow_count;
                            vulnerable_flows += hit.vulnerable_count;
                        } else {
                            all_cached = false;
                        }
                    } else {
                        all_cached = false;
                    }
                }
            }
            if all_cached && matched > 0 {
                let mut shortcut = CfgAnalysisBatchResult {
                    success_count: matched,
                    skip_bodiless_declaration: skipped_decls,
                    total_flows,
                    vulnerable_flows,
                    cache_hits: matched,
                    skipped_unchanged: matched,
                    archive_unchanged: true,
                    ..CfgAnalysisBatchResult::default()
                };
                for (path, name) in &unsupported_skips {
                    shortcut.record_skip(
                        CfgSkipReason::UnsupportedLanguage,
                        path,
                        name,
                        options.verbose,
                    );
                }
                if !options.verbose {
                    shortcut.error_count += unsupported_count;
                    shortcut.skip_unsupported_language += unsupported_count;
                }
                shortcut.orphans_removed = storage
                    .purge_stale_by_stable_keys(&active_keys)
                    .unwrap_or(0);
                return shortcut;
            }
        }
    } else {
        for group in &groups {
            for &idx in &group.func_indices {
                in_group[idx] = true;
                let func = &functions[idx];
                if !is_bodiless_declaration_node(func)
                    && let Some(code_hash) = func.code_hash.as_deref()
                    && try_fast_cache_hit(func, code_hash, &cache).is_some()
                {
                    retained_ids.push(func.id);
                    let alias = alias_uuid_for(&group.file_path, &func.name, code_hash);
                    retained_archive_entries.push((func.id, alias));
                }
            }
        }
    }

    let pack = storage
        .open_pack_writer_preserving(&retained_ids)
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "analysis pack writer failed; skipping CFG persistence");
            Mutex::new(None)
        });

    let archive_writer = options.archive_output.as_ref().and_then(|(path, digest)| {
        match StreamingCfgPdgArchiveWriter::create_preserving(
            path,
            Some(digest.as_str()),
            &retained_archive_entries,
        ) {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "streaming cfg_pdg archive writer failed");
                None
            }
        }
    });

    let stage = options.verbose.then(CfgStageTimings::default);
    let stage_ref = stage.as_ref();
    let timing_log = options
        .verbose
        .then(|| Mutex::new(Vec::<CfgFunctionTiming>::new()));
    let timing_ref = timing_log.as_ref();

    let ctx = CfgWorkContext {
        functions,
        columnar,
        repo_root,
        sources: file_sources,
        cache: &cache,
        stage: stage_ref,
        timings: timing_ref,
        enable_taint: options.enable_taint,
        dfg_loops: options.dfg_loops,
        taint_rules: options.taint_rules.as_ref(),
        with_ast_skeleton: options.with_ast_skeleton,
    };

    let worker_threads = options
        .thread_count
        .unwrap_or_else(rayon::current_num_threads)
        .clamp(2, 16);
    let channel_bound = worker_threads * 4;
    let (tx, rx) =
        mpsc::sync_channel::<(usize, Vec<Result<CfgFunctionWork, CfgSkipReason>>)>(channel_bound);

    let groups_ref = &groups;
    let pack_ref = &pack;
    let verbose = options.verbose;

    let (mut result, archive_writer_out, analyzed_count, streamed_active_keys) =
        std::thread::scope(|s| {
            let consumer = s.spawn(move || {
                let mut res = CfgAnalysisBatchResult::default();
                let mut writer = archive_writer;
                let mut analyzed = 0usize;
                let mut active_keys: HashSet<String> = HashSet::new();

                while let Ok((group_idx, outcomes)) = rx.recv() {
                    let group = &groups_ref[group_idx];
                    for (&func_idx, outcome) in group.func_indices.iter().zip(outcomes) {
                        match outcome {
                            Err(reason) => {
                                let sym_name: Cow<'_, str> = if let Some(col) = columnar {
                                    col.node_name_str(func_idx)
                                        .map(Cow::Borrowed)
                                        .unwrap_or(Cow::Borrowed("<unknown>"))
                                } else {
                                    Cow::Borrowed(functions[func_idx].name.as_str())
                                };
                                res.record_skip(reason, &group.file_path, &sym_name, verbose);
                            }
                            Ok(w) => {
                                analyzed += 1;
                                if let Some(k) = w.active_key {
                                    active_keys.insert(k);
                                }
                                if w.from_cache {
                                    res.cache_hits += 1;
                                } else {
                                    res.recomputed += 1;
                                }
                                if w.skip_persist {
                                    res.skipped_unchanged += 1;
                                }
                                res.success_count += 1;
                                res.total_flows += w.flow_count;
                                res.vulnerable_flows += w.vulnerable_count;
                                res.field_writes.extend(w.field_writes);
                                if let Some(skel) = w.skeleton {
                                    res.skeletons.push(skel);
                                }
                                if let Some(entry) = w.index_entry {
                                    res.index_entries.push(entry);
                                }
                                if let Some(bytes) = w.analysis_bytes
                                    && !w.skip_persist
                                {
                                    let _ = storage.save_preencoded_no_index_packed(
                                        w.function_id,
                                        &bytes,
                                        pack_ref,
                                    );
                                }
                                if let Some(bytes) = w.archive_bytes {
                                    if let Some(ref mut aw) = writer {
                                        let _ =
                                            aw.append(w.function_id, w.archive_alias_uuid, &bytes);
                                    } else {
                                        res.archive_record_bytes.push((w.function_id, bytes));
                                    }
                                }
                            }
                        }
                    }
                }

                (res, writer, analyzed, active_keys)
            });

            with_large_stack(|| {
                with_large_pool(options.thread_count, || {
                    groups_ref.par_iter().enumerate().for_each_with(
                        tx,
                        |sender, (group_idx, group)| {
                            let outcomes = process_file_group(group, &ctx);
                            let _ = sender.send((group_idx, outcomes));
                        },
                    );
                });
            });

            consumer
                .join()
                .unwrap_or_else(|_| (CfgAnalysisBatchResult::default(), None, 0, HashSet::new()))
        });

    if columnar.is_some() {
        for (path, name) in &unsupported_skips {
            result.record_skip(
                CfgSkipReason::UnsupportedLanguage,
                path,
                name,
                options.verbose,
            );
        }
        if !options.verbose {
            result.error_count += unsupported_count;
            result.skip_unsupported_language += unsupported_count;
        }
    } else {
        for (idx, func) in functions.iter().enumerate() {
            if in_group[idx] {
                continue;
            }
            if is_bodiless_declaration_node(func) {
                result.record_skip(
                    CfgSkipReason::BodilessDeclaration,
                    func.file_path.as_deref().unwrap_or("<unknown>"),
                    &func.name,
                    options.verbose,
                );
                continue;
            }
            let Some(path) = func.file_path.as_deref() else {
                continue;
            };
            result.record_skip(
                CfgSkipReason::UnsupportedLanguage,
                path,
                &func.name,
                options.verbose,
            );
        }
    }

    let stage_profile = stage_ref.map(|stage| emit_stage_profile(stage, analyzed_count));
    if let (Some(log), true) = (timing_log, options.verbose) {
        emit_tail_profile(&log);
    }

    // Deterministic order independent of worker completion order.
    result
        .index_entries
        .sort_by(|a, b| a.stable_key.cmp(&b.stable_key));
    result.skeletons.sort_by(|a, b| {
        (&a.file_path, &a.function_name, &a.function_id).cmp(&(
            &b.file_path,
            &b.function_name,
            &b.function_id,
        ))
    });

    let _ = AnalysisStorage::finish_pack(pack);
    if let Some(writer) = archive_writer_out {
        let archive_finish_start = Instant::now();
        if writer.records_written() > 0 {
            if let Err(err) = writer.finish() {
                tracing::warn!(error = %err, "Failed to finalize streamed cfg_pdg archive");
            } else {
                result.archive_streamed = true;
            }
        }
        result.archive_finish_secs = archive_finish_start.elapsed().as_secs_f64();
    }

    let _ = storage.refresh_analysis_index_from_entries(&result.index_entries);

    let active_keys = if columnar.is_some() {
        streamed_active_keys
    } else {
        active_stable_keys(functions, file_sources)
    };
    result.orphans_removed = storage
        .purge_stale_by_stable_keys(&active_keys)
        .unwrap_or(0);
    result.archive_unchanged =
        result.skipped_unchanged == result.success_count && result.recomputed == 0;
    result.stage_profile = stage_profile;
    result
}

fn emit_stage_profile(stage: &CfgStageTimings, analyzed: usize) -> CfgStageProfile {
    let build_ns = stage.build_cfg_ns.load(Ordering::Relaxed);
    let dom_ns = stage.dominator_ns.load(Ordering::Relaxed);
    let pdg_ns = stage.pdg_ns.load(Ordering::Relaxed);
    let taint_ns = stage.taint_ns.load(Ordering::Relaxed);
    let profile = CfgStageProfile {
        build_cfg_secs: build_ns as f64 / 1_000_000_000.0,
        dominator_secs: dom_ns as f64 / 1_000_000_000.0,
        pdg_secs: pdg_ns as f64 / 1_000_000_000.0,
        taint_secs: taint_ns as f64 / 1_000_000_000.0,
    };

    let fns = stage.functions.load(Ordering::Relaxed).max(analyzed as u64);
    let denom = fns.max(1) as f64;
    for (stage_name, secs) in [
        ("cfg_build", profile.build_cfg_secs),
        ("cfg_dominator", profile.dominator_secs),
        ("cfg_pdg", profile.pdg_secs),
        ("cfg_taint", profile.taint_secs),
    ] {
        if secs > 0.0 {
            info!(
                target: "profile",
                stage = stage_name,
                secs,
                avg_ms_per_fn = secs * 1000.0 / denom,
                analyzed,
                "[profile] stage"
            );
        }
    }

    profile
}

fn emit_tail_profile(log: &Mutex<Vec<CfgFunctionTiming>>) {
    let Ok(mut entries) = log.lock() else {
        return;
    };
    if entries.is_empty() {
        return;
    }
    entries.sort_by_key(|b| std::cmp::Reverse(b.total_ns));
    let p99_idx = ((entries.len() as f64 * 0.99).ceil() as usize).saturating_sub(1);
    let p99 = entries[p99_idx].total_ns as f64 / 1_000_000.0;
    info!(
        target: "profile",
        p99_ms = p99,
        "[profile] cfg function tail latency"
    );
    for entry in entries.iter().take(10) {
        info!(
            target: "profile",
            file = %entry.file_path,
            function = %entry.function_name,
            blocks = entry.blocks,
            total_ms = entry.total_ns as f64 / 1_000_000.0,
            "[profile] cfg slow function"
        );
    }
}

fn group_work_items_from_columnar(
    col: &ColumnarGraphMmap,
    verbose: bool,
    unsupported_count: &mut usize,
    unsupported_skips: &mut Vec<(String, String)>,
) -> Vec<FileWorkGroup> {
    let mut by_file: HashMap<String, FileWorkGroup> = HashMap::new();
    for idx in 0..col.node_count() {
        let Ok(nt) = col.node_type_at(idx) else {
            continue;
        };
        if nt != NodeType::Function {
            continue;
        }
        let Ok(Some(file_path)) = col.node_file_path_str(idx) else {
            continue;
        };
        if let Some(group) = by_file.get_mut(file_path) {
            group.func_indices.push(idx);
            continue;
        }
        let language = cfg_language_id_from_path(Path::new(file_path)).unwrap_or("");
        if language.is_empty() {
            if verbose {
                let name = col.node_name_str(idx).unwrap_or("<unknown>");
                unsupported_skips.push((file_path.to_string(), name.to_string()));
            } else {
                *unsupported_count += 1;
            }
            continue;
        }
        by_file.insert(
            file_path.to_string(),
            FileWorkGroup {
                file_path: file_path.to_string(),
                language: language.to_string(),
                func_indices: vec![idx],
            },
        );
    }
    by_file.into_values().collect()
}

fn group_work_items_by_file(functions: &[Node]) -> Vec<FileWorkGroup> {
    let mut by_file: HashMap<String, FileWorkGroup> = HashMap::new();
    for (idx, func) in functions.iter().enumerate() {
        let Some(file_path) = func.file_path.as_ref() else {
            continue;
        };
        let language = cfg_language_id_from_path(Path::new(file_path))
            .unwrap_or("")
            .to_string();
        if language.is_empty() {
            continue;
        }
        by_file
            .entry(file_path.to_string())
            .or_insert_with(|| FileWorkGroup {
                file_path: file_path.to_string(),
                language,
                func_indices: Vec::new(),
            })
            .func_indices
            .push(idx);
    }
    by_file.into_values().collect()
}

fn process_file_group(
    group: &FileWorkGroup,
    ctx: &CfgWorkContext<'_>,
) -> Vec<Result<CfgFunctionWork, CfgSkipReason>> {
    // Materialize only this single file's Function nodes when streaming from ColumnarGraphMmap.
    let local_nodes: Vec<Option<Node>> = if let Some(col) = ctx.columnar {
        group
            .func_indices
            .iter()
            .map(|&col_idx| col.materialize_node_at(col_idx).ok())
            .collect()
    } else {
        Vec::new()
    };
    let func_at = |slot_idx: usize| -> Option<&Node> {
        if ctx.columnar.is_some() {
            local_nodes[slot_idx].as_ref()
        } else {
            Some(&ctx.functions[group.func_indices[slot_idx]])
        }
    };

    let mut outcomes: Vec<Option<Result<CfgFunctionWork, CfgSkipReason>>> =
        Vec::with_capacity(group.func_indices.len());
    let mut needs_source = false;

    for slot_idx in 0..group.func_indices.len() {
        let Some(func) = func_at(slot_idx) else {
            outcomes.push(Some(Err(CfgSkipReason::AnalysisError)));
            continue;
        };
        if is_bodiless_declaration_node(func) {
            outcomes.push(Some(Err(CfgSkipReason::BodilessDeclaration)));
            continue;
        }
        if let Some(code_hash) = func.code_hash.as_deref()
            && let Some(hit) = try_fast_cache_hit(func, code_hash, ctx.cache)
        {
            outcomes.push(Some(Ok(hit)));
            continue;
        }
        needs_source = true;
        outcomes.push(None);
    }

    if !needs_source {
        return outcomes
            .into_iter()
            .map(|o| o.unwrap_or(Err(CfgSkipReason::BodilessDeclaration)))
            .collect();
    }

    let source: Option<Cow<'_, str>> = match ctx.sources {
        Some(cache) => cache.get(&group.file_path).map(Cow::Borrowed),
        None => std::fs::read_to_string(resolve_read_path(ctx.repo_root, &group.file_path))
            .ok()
            .map(Cow::Owned),
    };

    let Some(source) = source else {
        return outcomes
            .into_iter()
            .map(|o| o.unwrap_or(Err(CfgSkipReason::MissingSource)))
            .collect();
    };

    let parsed = ParsedSourceFile::parse(&group.language, source.as_bytes()).ok();
    let file_locals: FxHashMap<(usize, String), HashMap<String, String>> = parsed
        .as_ref()
        .map(|p| {
            build_file_local_types_from_tree(
                &group.language,
                p.tree().root_node(),
                source.as_bytes(),
            )
        })
        .unwrap_or_default();

    outcomes
        .into_iter()
        .enumerate()
        .map(|(slot_idx, pre)| {
            if let Some(done) = pre {
                return done;
            }
            let Some(func) = func_at(slot_idx) else {
                return Err(CfgSkipReason::AnalysisError);
            };
            let code_hash = resolve_code_hash(func, &source);
            compute_function_cfg(
                func,
                &group.language,
                &group.file_path,
                &source,
                &code_hash,
                parsed.as_ref(),
                &file_locals,
                false,
                ctx.stage,
                ctx.timings,
                ctx.enable_taint,
                ctx.dfg_loops,
                ctx.taint_rules,
                ctx.with_ast_skeleton,
            )
        })
        .collect()
}

fn load_incremental_cache(storage: &AnalysisStorage, _repo_root: &Path) -> CfgIncrementalCache {
    let index = storage.load_analysis_index().unwrap_or_default();
    CfgIncrementalCache { index }
}

fn active_stable_keys(functions: &[Node], sources: Option<&FileSourceCache>) -> HashSet<String> {
    let mut keys = HashSet::new();
    for func in functions {
        if is_bodiless_declaration_node(func) {
            continue;
        }
        let Some(file_path) = func.file_path.as_ref() else {
            continue;
        };
        let hash = if let Some(code_hash) = func.code_hash.as_ref() {
            code_hash.to_string()
        } else if let Some(cache) = sources {
            let Some(source) = cache.sources.get(file_path.as_str()) else {
                continue;
            };
            resolve_code_hash(func, source)
        } else {
            continue;
        };
        keys.insert(analysis_cache_key(func.id, &hash));
    }
    keys
}

fn resolve_code_hash(func_node: &Node, source: &str) -> String {
    func_node
        .code_hash
        .as_ref()
        .map(|h| h.to_string())
        .unwrap_or_else(|| hash_code(source))
}

fn is_constructor_node(node: &Node) -> bool {
    node.properties
        .get("is_constructor")
        .map(|v| v == "true")
        .unwrap_or(false)
        || node
            .qualified_name
            .as_deref()
            .is_some_and(|q| q.ends_with(".<init>") || q.contains("::<init>"))
}

fn function_name_looks_like_ctor(name: &str, node: &Node) -> bool {
    if let Some(enclosing) = node.properties.get("member_of").cloned().or_else(|| {
        node.qualified_name.as_ref().and_then(|qn| {
            qn.rsplit_once('#')
                .map(|(o, _)| o.to_string())
                .or_else(|| qn.rsplit_once('.').map(|(o, _)| o.to_string()))
        })
    }) {
        return name == enclosing || enclosing.ends_with(&format!(".{name}"));
    }
    false
}

fn try_fast_cache_hit(
    func_node: &Node,
    code_hash: &str,
    cache: &CfgIncrementalCache,
) -> Option<CfgFunctionWork> {
    let key = analysis_cache_key(func_node.id, code_hash);
    let entry = cache.index.get(&key)?;
    if entry.code_hash != code_hash
        || entry.function_id != func_node.id
        || !entry.matches_source(func_node)
    {
        return None;
    }
    Some(CfgFunctionWork {
        function_id: func_node.id,
        active_key: Some(key),
        analysis_bytes: None,
        archive_bytes: None,
        archive_alias_uuid: None,
        index_entry: None,
        field_writes: Vec::new(),
        skeleton: None,
        flow_count: entry.flow_count,
        vulnerable_count: entry.vulnerable_count,
        from_cache: true,
        skip_persist: true,
        blocks: 0,
    })
}

fn map_cfg_build_error(err: rgctl_error::Error) -> CfgSkipReason {
    match err {
        rgctl_error::Error::ParseError { ref message, .. } if message == "Function has no body" => {
            CfgSkipReason::BodilessDeclaration
        }
        _ => CfgSkipReason::AnalysisError,
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_function_cfg(
    func_node: &Node,
    language: &str,
    file_path: &str,
    source: &str,
    code_hash: &str,
    parsed: Option<&ParsedSourceFile>,
    file_locals: &FxHashMap<(usize, String), HashMap<String, String>>,
    from_cache: bool,
    stage: Option<&CfgStageTimings>,
    timings: Option<&Mutex<Vec<CfgFunctionTiming>>>,
    enable_taint: bool,
    dfg_loops: bool,
    taint_rules: Option<&Arc<TaintRuleSet>>,
    with_ast_skeleton: bool,
) -> Result<CfgFunctionWork, CfgSkipReason> {
    let total_start = timings.is_some().then(Instant::now);

    let build_start = stage.map(|_| Instant::now());
    let bytes = source.as_bytes();
    let exact_bytes = func_node
        .start_byte
        .zip(func_node.end_byte)
        .map(|(start, end)| (start as usize, end as usize))
        .or_else(|| {
            func_node
                .get_property("source_start_byte")
                .and_then(|s| s.parse::<usize>().ok())
                .zip(
                    func_node
                        .get_property("source_end_byte")
                        .and_then(|s| s.parse::<usize>().ok()),
                )
        });
    let mut cfg_data = if let Some(p) = parsed {
        if let Some((start, end)) = exact_bytes {
            p.build_cfg_for_byte_range(language, bytes, &func_node.name, start, end)
                .map_err(map_cfg_build_error)?
        } else {
            match (func_node.start_line, func_node.end_line) {
                (Some(start), Some(end)) => p
                    .build_cfg_for_node_span(language, bytes, &func_node.name, start, end)
                    .map_err(map_cfg_build_error)?,
                _ => p
                    .build_cfg(language, bytes, &func_node.name)
                    .map_err(map_cfg_build_error)?,
            }
        }
    } else {
        build_cfg_for_function(language, source, &func_node.name).map_err(map_cfg_build_error)?
    };
    if let (Some(stage), Some(start)) = (stage, build_start) {
        stage
            .build_cfg_ns
            .fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    // Merge single-pass file locals into CFG before field-write extraction / PDG.
    if let Some(start) = func_node.start_line
        && let Some(locals) = file_locals.get(&(start, func_node.name.to_string()))
    {
        for (k, v) in locals {
            cfg_data
                .local_types
                .entry(k.clone())
                .or_insert_with(|| v.clone());
        }
    }

    let is_constructor =
        is_constructor_node(func_node) || function_name_looks_like_ctor(&func_node.name, func_node);
    let type_env = type_env_for_field_writes(Some(func_node), &cfg_data);
    let mut field_writes = Vec::new();
    extract_writes_from_cfg(
        &cfg_data,
        func_node.id,
        &func_node.name,
        is_constructor,
        file_path,
        &type_env,
        &mut field_writes,
    );

    let skeleton = if with_ast_skeleton {
        parsed.and_then(|p| {
            let node = match (func_node.start_line, func_node.end_line) {
                (Some(start), Some(end)) => p.function_node_for_span(&func_node.name, start, end),
                _ => None,
            };
            node.and_then(|n| {
                build_function_skeleton_from_tree(
                    n,
                    bytes,
                    &func_node.name,
                    file_path,
                    Some(func_node.id),
                )
                .ok()
            })
        })
    } else {
        None
    };

    let work = compute_from_cfg(
        func_node,
        file_path,
        source,
        code_hash,
        language,
        cfg_data,
        field_writes,
        skeleton,
        from_cache,
        stage,
        enable_taint,
        dfg_loops,
        taint_rules,
    )
    .ok_or(CfgSkipReason::AnalysisError)?;

    if let (Some(start), Some(log)) = (total_start, timings)
        && let Ok(mut entries) = log.lock()
    {
        entries.push(CfgFunctionTiming {
            file_path: file_path.to_string(),
            function_name: func_node.name.to_string(),
            blocks: work.blocks,
            total_ns: start.elapsed().as_nanos() as u64,
        });
    }

    Ok(work)
}

#[derive(Serialize)]
struct FunctionAnalysisPrefix<'a> {
    function_id: Uuid,
    function_name: &'a str,
    file_path: &'a str,
    code_hash: Option<&'a str>,
}

#[derive(Serialize)]
struct FunctionAnalysisSuffix<'a> {
    dominance: Option<&'a DominatorTree>,
    taint: &'a Option<Vec<TaintFlow>>,
}

#[derive(Serialize)]
struct ArchiveRecordPrefix<'a> {
    function_id: Uuid,
    code_hash: &'a str,
    function_name: &'a str,
    file_path: Option<&'a str>,
}

/// Serialize `cfg` and `pdg` once and splice them into the exact bincode layouts of
/// [`crate::analysis::FunctionAnalysis`] and [`CfgPdgRecord`].
fn encode_analysis_and_archive_bytes(
    function_id: Uuid,
    function_name: &str,
    file_path: &str,
    code_hash: &str,
    cfg: &ControlFlowGraph,
    pdg: Option<&ProgramDependenceGraph>,
    taint: &Option<Vec<TaintFlow>>,
) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
    let cfg_bytes = bincode::serialize(cfg).ok()?;
    let pdg_bytes = match pdg {
        Some(p) => Some(bincode::serialize(p).ok()?),
        None => None,
    };

    let pdg_len = pdg_bytes.as_ref().map_or(0, Vec::len);
    let mut analysis_bytes = Vec::with_capacity(
        64 + function_name.len() + file_path.len() + code_hash.len() + cfg_bytes.len() + pdg_len,
    );
    bincode::serialize_into(
        &mut analysis_bytes,
        &FunctionAnalysisPrefix {
            function_id,
            function_name,
            file_path,
            code_hash: Some(code_hash),
        },
    )
    .ok()?;
    analysis_bytes.push(1u8); // cfg: Some(...)
    analysis_bytes.extend_from_slice(&cfg_bytes);
    match &pdg_bytes {
        Some(pb) => {
            analysis_bytes.push(1u8); // pdg: Some(...)
            analysis_bytes.extend_from_slice(pb);
        }
        None => {
            analysis_bytes.push(0u8); // pdg: None
        }
    }
    bincode::serialize_into(
        &mut analysis_bytes,
        &FunctionAnalysisSuffix {
            dominance: None,
            taint,
        },
    )
    .ok()?;

    let archive_bytes = match &pdg_bytes {
        Some(pb) => {
            let mut buf = Vec::with_capacity(
                48 + code_hash.len()
                    + function_name.len()
                    + file_path.len()
                    + cfg_bytes.len()
                    + pb.len(),
            );
            bincode::serialize_into(
                &mut buf,
                &ArchiveRecordPrefix {
                    function_id,
                    code_hash,
                    function_name,
                    file_path: Some(file_path),
                },
            )
            .ok()?;
            buf.extend_from_slice(&cfg_bytes);
            buf.extend_from_slice(pb);
            Some(buf)
        }
        None => None,
    };

    Some((analysis_bytes, archive_bytes))
}

#[allow(clippy::too_many_arguments)]
fn compute_from_cfg(
    func_node: &Node,
    file_path: &str,
    source: &str,
    code_hash: &str,
    language: &str,
    cfg_data: ControlFlowGraph,
    field_writes: Vec<FieldWrite>,
    skeleton: Option<AstSkeletonRecord>,
    from_cache: bool,
    stage: Option<&CfgStageTimings>,
    enable_taint: bool,
    dfg_loops: bool,
    taint_rules: Option<&Arc<TaintRuleSet>>,
) -> Option<CfgFunctionWork> {
    if let Some(stage) = stage {
        stage.functions.fetch_add(1, Ordering::Relaxed);
    }

    let blocks = cfg_data.blocks.len();

    let dom_start = stage.map(|_| Instant::now());
    let dom_data = DominatorTree::build(&cfg_data);
    if let (Some(stage), Some(start)) = (stage, dom_start) {
        stage
            .dominator_ns
            .fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    let pdg_start = stage.map(|_| Instant::now());
    let pdg_opts = PdgBuildOptions {
        classify_loop_carried: dfg_loops,
    };
    let pdg_data = ProgramDependenceGraph::build_with_dominator_options(
        &cfg_data,
        source.as_bytes(),
        &dom_data,
        pdg_opts,
    )
    .ok();
    if let (Some(stage), Some(start)) = (stage, pdg_start) {
        stage
            .pdg_ns
            .fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    let taint_start = stage.map(|_| Instant::now());
    let (taint_data, flow_count, vulnerable_count) = if enable_taint {
        if let Some(ref pdg) = pdg_data {
            let mut analyzer = TaintAnalyzer::with_dominator(pdg, &cfg_data, dom_data);
            if let Some(rules) = taint_rules {
                analyzer.detect_patterns_with_rules(language, rules.as_ref());
            } else {
                analyzer.detect_patterns(language);
            }
            let flows = analyzer.analyze();
            let vulnerable = flows.iter().filter(|f| f.is_vulnerable()).count();
            let count = flows.len();
            let taint = if flows.is_empty() { None } else { Some(flows) };
            (taint, count, vulnerable)
        } else {
            (None, 0, 0)
        }
    } else {
        (None, 0, 0)
    };
    if enable_taint && let (Some(stage), Some(start)) = (stage, taint_start) {
        stage
            .taint_ns
            .fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    let (analysis_bytes, archive_bytes) = encode_analysis_and_archive_bytes(
        func_node.id,
        &func_node.name,
        file_path,
        code_hash,
        &cfg_data,
        pdg_data.as_ref(),
        &taint_data,
    )?;
    drop(pdg_data);
    drop(cfg_data);

    let archive_alias_uuid = archive_bytes
        .as_ref()
        .and_then(|_| alias_uuid_for(file_path, &func_node.name, code_hash));

    let stable_key = analysis_cache_key(func_node.id, code_hash);
    let index_entry = Some(AnalysisIndexEntry {
        stable_key: stable_key.clone(),
        function_id: func_node.id,
        code_hash: code_hash.to_string(),
        source_location: crate::analysis::storage::AnalysisSourceLocation::from_node(func_node),
        flow_count,
        vulnerable_count,
    });

    Some(CfgFunctionWork {
        function_id: func_node.id,
        active_key: Some(stable_key),
        analysis_bytes: Some(analysis_bytes),
        archive_bytes,
        archive_alias_uuid,
        index_entry,
        field_writes,
        skeleton,
        flow_count,
        vulnerable_count,
        from_cache,
        skip_persist: false,
        blocks,
    })
}

/// When every CFG-eligible non-declaration function is already indexed with the same body hash,
/// skip the batch completely.
fn try_full_incremental_shortcut(
    functions: &[Node],
    cache: &CfgIncrementalCache,
) -> Option<CfgAnalysisBatchResult> {
    let mut total_flows = 0usize;
    let mut vulnerable_flows = 0usize;
    let mut matched = 0usize;
    let mut eligible = 0usize;
    let mut skipped_decls = 0usize;

    for func in functions {
        if is_bodiless_declaration_node(func) {
            skipped_decls += 1;
            continue;
        }
        let Some(file_path) = func.file_path.as_ref() else {
            continue;
        };
        let Some(code_hash) = func.code_hash.as_ref() else {
            continue;
        };
        if cfg_language_id_from_path(Path::new(file_path)).is_none() {
            continue;
        }
        eligible += 1;
        let key = analysis_cache_key(func.id, code_hash);
        let entry = cache.index.get(&key)?;
        if *code_hash != entry.code_hash
            || entry.function_id != func.id
            || !entry.matches_source(func)
        {
            return None;
        }
        matched += 1;
        total_flows += entry.flow_count;
        vulnerable_flows += entry.vulnerable_count;
    }

    if eligible == 0 || matched != eligible {
        return None;
    }

    Some(CfgAnalysisBatchResult {
        success_count: matched,
        skip_bodiless_declaration: skipped_decls,
        total_flows,
        vulnerable_flows,
        cache_hits: matched,
        skipped_unchanged: matched,
        archive_unchanged: true,
        ..CfgAnalysisBatchResult::default()
    })
}

#[cfg(test)]
mod overload_cache_tests {
    use super::*;
    use crate::analysis::FunctionAnalysis;

    #[test]
    fn relocated_or_unknown_source_cannot_reuse_absolute_operand_spans() {
        let mut original = Node::new(rgctl_graph::schema::NodeType::Function, "example")
            .with_file_path("example.js")
            .with_code_hash("same")
            .with_byte_spans(Some(0), Some(40), None, None, None);
        original.start_line = Some(1);
        let key = analysis_cache_key(original.id, "same");
        let cache = CfgIncrementalCache {
            index: HashMap::from([(
                key.clone(),
                AnalysisIndexEntry {
                    stable_key: key,
                    function_id: original.id,
                    code_hash: "same".into(),
                    source_location: crate::analysis::storage::AnalysisSourceLocation::from_node(
                        &original,
                    ),
                    flow_count: 0,
                    vulnerable_count: 0,
                },
            )]),
        };
        assert!(try_fast_cache_hit(&original, "same", &cache).is_some());
        assert!(try_full_incremental_shortcut(std::slice::from_ref(&original), &cache).is_some());
        let mut moved = original.clone();
        moved.start_byte = Some(20);
        moved.end_byte = Some(60);
        moved.start_line = Some(3);
        assert!(try_fast_cache_hit(&moved, "same", &cache).is_none());
        assert!(try_full_incremental_shortcut(std::slice::from_ref(&moved), &cache).is_none());
        let mut unknown = original;
        unknown.start_byte = None;
        assert!(try_fast_cache_hit(&unknown, "same", &cache).is_none());
    }

    #[test]
    fn identical_name_and_body_cache_cannot_remap_another_overload() {
        let mut first = Node::new(rgctl_graph::schema::NodeType::Function, "quote")
            .with_file_path("Library.java")
            .with_identity_discriminator("(int)")
            .with_code_hash("same")
            .with_byte_spans(Some(0), Some(10), None, None, None);
        let mut second = Node::new(rgctl_graph::schema::NodeType::Function, "quote")
            .with_file_path("Library.java")
            .with_identity_discriminator("(String)")
            .with_code_hash("same")
            .with_byte_spans(Some(0), Some(10), None, None, None);
        first.start_line = Some(1);
        second.start_line = Some(1);
        let key = analysis_cache_key(first.id, "same");
        let cache = CfgIncrementalCache {
            index: std::collections::HashMap::from([(
                key.clone(),
                AnalysisIndexEntry {
                    stable_key: key,
                    function_id: first.id,
                    code_hash: "same".into(),
                    source_location: crate::analysis::storage::AnalysisSourceLocation::from_node(
                        &first,
                    ),
                    flow_count: 0,
                    vulnerable_count: 0,
                },
            )]),
        };
        assert!(try_fast_cache_hit(&first, "same", &cache).is_some());
        assert!(try_fast_cache_hit(&second, "same", &cache).is_none());
        assert!(try_full_incremental_shortcut(&[first, second], &cache).is_none());
    }

    #[test]
    fn single_encode_matches_bincode_struct_serialization_byte_for_byte() {
        let src = "fn compute(x: i32) -> i32 { if x > 0 { x + 1 } else { x - 1 } }";
        let cfg = build_cfg_for_function("rust", src, "compute").expect("cfg");
        let pdg = ProgramDependenceGraph::build(&cfg, src.as_bytes()).expect("pdg");
        let fn_id = Uuid::new_v4();
        let cfg_arc = Arc::new(cfg.clone());
        let pdg_arc = Arc::new(pdg.clone());

        let expected_analysis = FunctionAnalysis {
            function_id: fn_id,
            function_name: "compute".to_string(),
            file_path: "src/lib.rs".to_string(),
            code_hash: Some("hash123".to_string()),
            cfg: Some(Arc::clone(&cfg_arc)),
            pdg: Some(Arc::clone(&pdg_arc)),
            dominance: None,
            taint: None,
        };
        let expected_archive = CfgPdgRecord {
            function_id: fn_id,
            code_hash: "hash123".to_string(),
            function_name: "compute".to_string(),
            file_path: Some("src/lib.rs".to_string()),
            cfg: cfg_arc,
            pdg: pdg_arc,
        };

        let (actual_analysis, actual_archive) = encode_analysis_and_archive_bytes(
            fn_id,
            "compute",
            "src/lib.rs",
            "hash123",
            &cfg,
            Some(&pdg),
            &None,
        )
        .expect("single encode");

        assert_eq!(
            actual_analysis,
            bincode::serialize(&expected_analysis).expect("expected analysis"),
        );
        assert_eq!(
            actual_archive.expect("archive bytes"),
            bincode::serialize(&expected_archive).expect("expected archive"),
        );
    }

    #[test]
    fn bodiless_declarations_skip_without_incrementing_error_count() {
        let mut decl = Node::new(rgctl_graph::schema::NodeType::Function, "onEvent")
            .with_file_path("Listener.java")
            .with_code_hash("decl_hash");
        decl.properties
            .insert("callable_role".to_string(), "declaration".to_string());

        let impl_fn = Node::new(rgctl_graph::schema::NodeType::Function, "handle")
            .with_file_path("Handler.java")
            .with_code_hash("impl_hash");
        let key = analysis_cache_key(impl_fn.id, "impl_hash");
        let cache = CfgIncrementalCache {
            index: std::collections::HashMap::from([(
                key.clone(),
                AnalysisIndexEntry {
                    stable_key: key,
                    function_id: impl_fn.id,
                    code_hash: "impl_hash".into(),
                    flow_count: 2,
                    vulnerable_count: 1,
                },
            )]),
        };

        let shortcut =
            try_full_incremental_shortcut(&[decl, impl_fn], &cache).expect("shortcut hits");
        assert_eq!(shortcut.error_count, 0);
        assert_eq!(shortcut.skip_bodiless_declaration, 1);
        assert_eq!(shortcut.success_count, 1);
    }
}
