//! Parallel CFG/PDG/taint analysis for discover `--with-cfg` / `--with-taint`.

use crate::analysis::storage::stable_function_key;
use crate::analysis::{
    AnalysisIndexEntry, AnalysisStorage, AstSkeletonRecord, CfgPdgRecord, ControlFlowGraph,
    DominatorTree, FieldWrite, FunctionAnalysis, FunctionIdSyncEntry, ParsedSourceFile,
    PdgBuildOptions, ProgramDependenceGraph, TaintAnalyzer, TaintRuleSet, build_cfg_for_function,
    build_file_local_types_from_tree, build_function_skeleton_from_tree, cfg_language_id_from_path,
    extract_writes_from_cfg,
};
use crate::analysis::hash_maps::FxHashMap;
use rayon::prelude::*;
use rgctl_graph::code_index::hash_code;
use rgctl_graph::schema::Node;
use rgctl_pipeline::{with_large_pool, with_large_stack, with_pool};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
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
}

/// Why a function was not CFG-analyzed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CfgSkipReason {
    /// Extension / language has no CFG builder.
    UnsupportedLanguage,
    /// Source text was not available for the function's file.
    MissingSource,
    /// Parse or CFG/PDG construction failed (or empty body).
    AnalysisError,
}

impl CfgSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedLanguage => "unsupported_language",
            Self::MissingSource => "missing_source",
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
    pub skip_analysis_error: usize,
    pub skips: Vec<CfgSkipEvent>,
    pub total_flows: usize,
    pub vulnerable_flows: usize,
    pub cache_hits: usize,
    pub recomputed: usize,
    pub skipped_unchanged: usize,
    pub orphans_removed: usize,
    /// Pre-encoded CFG/PDG archive records `(function_id, bincode)`.
    pub archive_record_bytes: Vec<(Uuid, Vec<u8>)>,
    /// Legacy in-memory records (empty when worker-side encode is used).
    pub archive_records: Vec<CfgPdgRecord>,
    pub archive_unchanged: bool,
    /// Field writes extracted on workers (fused; skip archive re-open when non-empty).
    pub field_writes: Vec<FieldWrite>,
    /// Optional AST skeletons extracted on workers.
    pub skeletons: Vec<AstSkeletonRecord>,
    /// Index entries for recomputed functions (worker-side serialize path).
    pub index_entries: Vec<AnalysisIndexEntry>,
    pub stage_profile: Option<CfgStageProfile>,
}

impl CfgAnalysisBatchResult {
    fn record_skip(
        &mut self,
        reason: CfgSkipReason,
        file_path: &str,
        symbol: &str,
        verbose: bool,
    ) {
        self.error_count += 1;
        match reason {
            CfgSkipReason::UnsupportedLanguage => self.skip_unsupported_language += 1,
            CfgSkipReason::MissingSource => self.skip_missing_source += 1,
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
    /// Pre-encoded [`FunctionAnalysis`] bincode (worker-side).
    analysis_bytes: Option<Vec<u8>>,
    /// Pre-encoded [`CfgPdgRecord`] bincode (worker-side).
    archive_bytes: Option<Vec<u8>>,
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

/// Parallel file read cache keyed by node `file_path` strings.
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

fn resolve_read_path(repo_root: &Path, file_path: &str) -> std::path::PathBuf {
    let path = Path::new(file_path);
    if path.is_file() {
        path.to_path_buf()
    } else {
        repo_root.join(file_path)
    }
}

/// Read each unique function source file once (parallel when `thread_count` is set).
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
    sources: &'a FileSourceCache,
    cache: &'a CfgIncrementalCache,
    stage: Option<&'a CfgStageTimings>,
    timings: Option<&'a Mutex<Vec<CfgFunctionTiming>>>,
    enable_taint: bool,
    dfg_loops: bool,
    taint_rules: Option<&'a Arc<TaintRuleSet>>,
    with_ast_skeleton: bool,
}

/// Analyze all repository functions in parallel with incremental reuse and bincode persistence.
pub fn run_cfg_analysis_batch(
    functions: &[Node],
    storage: &AnalysisStorage,
    repo_root: &Path,
    options: CfgAnalysisOptions,
    file_sources: Option<&FileSourceCache>,
) -> CfgAnalysisBatchResult {
    let cache = load_incremental_cache(storage, repo_root);

    if let Some(shortcut) = try_full_incremental_shortcut(functions, &cache) {
        let active_keys = active_stable_keys(functions, None);
        let _ = sync_storage_function_ids(storage, functions);
        let mut result = shortcut;
        result.orphans_removed = storage
            .purge_stale_by_stable_keys(&active_keys)
            .unwrap_or(0);
        return result;
    }

    let owned_sources;
    let sources = match file_sources {
        Some(preloaded) => preloaded,
        None => {
            owned_sources = preload_file_sources(functions, repo_root, options.thread_count);
            &owned_sources
        }
    };
    let groups = group_work_items_by_file(functions);
    let stage = options.verbose.then(CfgStageTimings::default);
    let stage_ref = stage.as_ref();
    let timing_log = options
        .verbose
        .then(|| Mutex::new(Vec::<CfgFunctionTiming>::new()));
    let timing_ref = timing_log.as_ref();

    let ctx = CfgWorkContext {
        functions,
        sources,
        cache: &cache,
        stage: stage_ref,
        timings: timing_ref,
        enable_taint: options.enable_taint,
        dfg_loops: options.dfg_loops,
        taint_rules: options.taint_rules.as_ref(),
        with_ast_skeleton: options.with_ast_skeleton,
    };

    let nested: Vec<Vec<Result<CfgFunctionWork, CfgSkipReason>>> = with_large_stack(|| {
        with_large_pool(options.thread_count, || {
            groups
                .par_iter()
                .map(|group| process_file_group(group, &ctx))
                .collect()
        })
    });

    let mut in_group = vec![false; functions.len()];
    for group in &groups {
        for &idx in &group.func_indices {
            in_group[idx] = true;
        }
    }

    let stage_profile = stage_ref.map(|stage| {
        let analyzed = nested
            .iter()
            .flatten()
            .filter(|w| w.is_ok())
            .count();
        emit_stage_profile(stage, analyzed)
    });
    if let (Some(log), true) = (timing_log, options.verbose) {
        emit_tail_profile(&log);
    }

    let mut save_bytes: Vec<(Uuid, Vec<u8>)> = Vec::new();
    let mut result = CfgAnalysisBatchResult::default();

    for (idx, func) in functions.iter().enumerate() {
        if in_group[idx] {
            continue;
        }
        let path = func.file_path.as_deref().unwrap_or("<unknown>");
        result.record_skip(
            CfgSkipReason::UnsupportedLanguage,
            path,
            &func.name,
            options.verbose,
        );
    }

    for (group, outcomes) in groups.iter().zip(nested.into_iter()) {
        for (&func_idx, outcome) in group.func_indices.iter().zip(outcomes.into_iter()) {
            let func = &functions[func_idx];
            match outcome {
                Err(reason) => {
                    result.record_skip(
                        reason,
                        &group.file_path,
                        &func.name,
                        options.verbose,
                    );
                }
                Ok(w) => {
                    if w.from_cache {
                        result.cache_hits += 1;
                    } else {
                        result.recomputed += 1;
                    }
                    if w.skip_persist {
                        result.skipped_unchanged += 1;
                    }
                    result.success_count += 1;
                    result.total_flows += w.flow_count;
                    result.vulnerable_flows += w.vulnerable_count;
                    result.field_writes.extend(w.field_writes);
                    if let Some(skel) = w.skeleton {
                        result.skeletons.push(skel);
                    }
                    if let Some(entry) = w.index_entry {
                        result.index_entries.push(entry);
                    }
                    if let Some(bytes) = w.archive_bytes {
                        result.archive_record_bytes.push((w.function_id, bytes));
                    }
                    if let Some(bytes) = w.analysis_bytes {
                        if !w.skip_persist {
                            save_bytes.push((w.function_id, bytes));
                        }
                    }
                }
            }
        }
    }

    let pack = storage.open_pack_writer().unwrap_or_else(|e| {
        tracing::warn!(error = %e, "analysis pack writer failed; skipping CFG persistence");
        Mutex::new(None)
    });
    for (function_id, bytes) in &save_bytes {
        let _ = storage.save_preencoded_no_index_packed(*function_id, bytes, &pack);
    }
    let _ = AnalysisStorage::finish_pack(pack);
    let _ = storage.refresh_analysis_index_from_entries(&result.index_entries);

    let active_keys = active_stable_keys(functions, Some(sources));
    let _ = sync_storage_function_ids(storage, functions);
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
    let Some(source) = ctx.sources.get(&group.file_path) else {
        return (0..group.func_indices.len())
            .map(|_| Err(CfgSkipReason::MissingSource))
            .collect();
    };

    let parsed = ParsedSourceFile::parse(&group.language, source.as_bytes()).ok();
    let file_locals: FxHashMap<(usize, String), HashMap<String, String>> =
        parsed
            .as_ref()
            .map(|p| {
                build_file_local_types_from_tree(
                    &group.language,
                    p.tree().root_node(),
                    source.as_bytes(),
                )
            })
            .unwrap_or_default();

    group
        .func_indices
        .iter()
        .map(|&func_idx| {
            let func = &ctx.functions[func_idx];
            analyze_function_in_file(
                func,
                &group.language,
                &group.file_path,
                source,
                parsed.as_ref(),
                &file_locals,
                ctx.cache,
                ctx.stage,
                ctx.timings,
                ctx.enable_taint,
                ctx.dfg_loops,
                ctx.taint_rules,
                ctx.with_ast_skeleton,
            )
            .ok_or(CfgSkipReason::AnalysisError)
        })
        .collect()
}

fn sync_storage_function_ids(storage: &AnalysisStorage, functions: &[Node]) -> usize {
    let refs: Vec<FunctionIdSyncEntry<'_>> = functions
        .iter()
        .filter_map(|func| {
            Some(FunctionIdSyncEntry {
                function_id: func.id,
                function_name: &func.name,
                file_path: func.file_path.as_ref()?,
                code_hash: func.code_hash.as_ref()?,
            })
        })
        .collect();
    storage.sync_index_function_ids(&refs).unwrap_or(0)
}

fn load_incremental_cache(storage: &AnalysisStorage, _repo_root: &Path) -> CfgIncrementalCache {
    let index = storage.load_analysis_index().unwrap_or_default();
    CfgIncrementalCache { index }
}

fn active_stable_keys(functions: &[Node], sources: Option<&FileSourceCache>) -> HashSet<String> {
    let mut keys = HashSet::new();
    for func in functions {
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
        keys.insert(stable_function_key(file_path, &func.name, &hash));
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
    if let Some(enclosing) = node
        .properties
        .get("member_of")
        .cloned()
        .or_else(|| {
            node.qualified_name.as_ref().and_then(|qn| {
                qn.rsplit_once('#')
                    .map(|(o, _)| o.to_string())
                    .or_else(|| qn.rsplit_once('.').map(|(o, _)| o.to_string()))
            })
        })
    {
        return name == enclosing || enclosing.ends_with(&format!(".{name}"));
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn analyze_function_in_file(
    func_node: &Node,
    language: &str,
    file_path: &str,
    source: &str,
    parsed: Option<&ParsedSourceFile>,
    file_locals: &FxHashMap<(usize, String), HashMap<String, String>>,
    cache: &CfgIncrementalCache,
    stage: Option<&CfgStageTimings>,
    timings: Option<&Mutex<Vec<CfgFunctionTiming>>>,
    enable_taint: bool,
    dfg_loops: bool,
    taint_rules: Option<&Arc<TaintRuleSet>>,
    with_ast_skeleton: bool,
) -> Option<CfgFunctionWork> {
    let code_hash = resolve_code_hash(func_node, source);

    if func_node.code_hash.is_some() {
        if let Some(hit) = try_fast_cache_hit(func_node, file_path, &code_hash, cache) {
            return Some(hit);
        }
        if let Some(hit) = try_remap_cached(func_node, file_path, &code_hash, cache) {
            return Some(hit);
        }
    }

    compute_function_cfg(
        func_node,
        language,
        file_path,
        source,
        &code_hash,
        parsed,
        file_locals,
        false,
        stage,
        timings,
        enable_taint,
        dfg_loops,
        taint_rules,
        with_ast_skeleton,
    )
}

fn try_fast_cache_hit(
    func_node: &Node,
    file_path: &str,
    code_hash: &str,
    cache: &CfgIncrementalCache,
) -> Option<CfgFunctionWork> {
    let key = stable_function_key(file_path, &func_node.name, code_hash);
    let entry = cache.index.get(&key)?;
    if entry.code_hash != code_hash || entry.function_id != func_node.id {
        return None;
    }
    Some(CfgFunctionWork {
        function_id: func_node.id,
        analysis_bytes: None,
        archive_bytes: None,
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

fn try_remap_cached(
    func_node: &Node,
    file_path: &str,
    code_hash: &str,
    cache: &CfgIncrementalCache,
) -> Option<CfgFunctionWork> {
    let key = stable_function_key(file_path, &func_node.name, code_hash);
    let entry = cache.index.get(&key)?;
    if entry.code_hash != code_hash || entry.function_id == func_node.id {
        return None;
    }
    Some(CfgFunctionWork {
        function_id: func_node.id,
        analysis_bytes: None,
        archive_bytes: None,
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
) -> Option<CfgFunctionWork> {
    let total_start = timings.is_some().then(Instant::now);

    let build_start = stage.map(|_| Instant::now());
    let bytes = source.as_bytes();
    let mut cfg_data = parsed
        .and_then(|p| match (func_node.start_line, func_node.end_line) {
            (Some(start), Some(end)) => p
                .build_cfg_for_node_span(language, bytes, &func_node.name, start, end)
                .ok(),
            _ => p.build_cfg(language, bytes, &func_node.name).ok(),
        })
        .or_else(|| build_cfg_for_function(language, source, &func_node.name).ok())?;
    if let (Some(stage), Some(start)) = (stage, build_start) {
        stage
            .build_cfg_ns
            .fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    // Merge single-pass file locals into CFG before field-write extraction / PDG.
    if let Some(start) = func_node.start_line {
        if let Some(locals) = file_locals.get(&(start, func_node.name.to_string())) {
            for (k, v) in locals {
                cfg_data
                    .local_types
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
        }
    }

    let is_constructor =
        is_constructor_node(func_node) || function_name_looks_like_ctor(&func_node.name, func_node);
    let mut field_writes = Vec::new();
    extract_writes_from_cfg(
        &cfg_data,
        func_node.id,
        &func_node.name,
        is_constructor,
        file_path,
        &cfg_data.local_types,
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
    )?;

    if let (Some(start), Some(log)) = (total_start, timings) {
        if let Ok(mut entries) = log.lock() {
            entries.push(CfgFunctionTiming {
                file_path: file_path.to_string(),
                function_name: func_node.name.to_string(),
                blocks: work.blocks,
                total_ns: start.elapsed().as_nanos() as u64,
            });
        }
    }

    Some(work)
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
    let cfg_arc = Arc::new(cfg_data);
    let pdg_arc = pdg_data.map(Arc::new);
    let (taint_data, flow_count, vulnerable_count) = if enable_taint {
        if let Some(ref pdg) = pdg_arc {
            let mut analyzer =
                TaintAnalyzer::with_dominator(pdg.as_ref(), cfg_arc.as_ref(), dom_data);
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
    if enable_taint {
        if let (Some(stage), Some(start)) = (stage, taint_start) {
            stage
                .taint_ns
                .fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
    }

    let analysis = FunctionAnalysis {
        function_id: func_node.id,
        function_name: func_node.name.to_string(),
        file_path: file_path.to_string(),
        code_hash: Some(code_hash.to_string()),
        cfg: Some(Arc::clone(&cfg_arc)),
        pdg: pdg_arc.as_ref().map(Arc::clone),
        dominance: None,
        taint: taint_data,
    };

    let archive_record = pdg_arc.as_ref().map(|pdg| CfgPdgRecord {
        function_id: func_node.id,
        code_hash: code_hash.to_string(),
        function_name: func_node.name.to_string(),
        file_path: Some(file_path.to_string()),
        cfg: Arc::clone(&cfg_arc),
        pdg: Arc::clone(pdg),
    });

    let analysis_bytes = bincode::serialize(&analysis).ok();
    let archive_bytes = archive_record
        .as_ref()
        .and_then(|r| bincode::serialize(r).ok());
    // Drop CFG/PDG Arcs before returning from the worker.
    drop(analysis);
    drop(archive_record);
    drop(cfg_arc);
    drop(pdg_arc);

    let index_entry = Some(AnalysisIndexEntry {
        stable_key: stable_function_key(file_path, &func_node.name, code_hash),
        function_id: func_node.id,
        code_hash: code_hash.to_string(),
        flow_count,
        vulnerable_count,
    });

    Some(CfgFunctionWork {
        function_id: func_node.id,
        analysis_bytes,
        archive_bytes,
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

/// When every CFG-eligible function is already indexed with the same body hash, skip the batch.
fn try_full_incremental_shortcut(
    functions: &[Node],
    cache: &CfgIncrementalCache,
) -> Option<CfgAnalysisBatchResult> {
    let mut total_flows = 0usize;
    let mut vulnerable_flows = 0usize;
    let mut matched = 0usize;
    let mut eligible = 0usize;

    for func in functions {
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
        let key = stable_function_key(file_path, &func.name, code_hash);
        let entry = cache.index.get(&key)?;
        if *code_hash != entry.code_hash {
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
        total_flows,
        vulnerable_flows,
        cache_hits: matched,
        skipped_unchanged: matched,
        archive_unchanged: true,
        ..CfgAnalysisBatchResult::default()
    })
}
