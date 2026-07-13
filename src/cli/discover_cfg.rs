//! Parallel CFG/PDG/taint analysis for discover `--cfg` / `--all`.

use crate::analysis::{
    build_cfg_for_function, cfg_language_id_from_path, AnalysisIndexEntry, AnalysisStorage,
    CfgPdgRecord, ControlFlowGraph, DominatorTree, FunctionAnalysis, FunctionIdSyncEntry,
    ParsedSourceFile, ProgramDependenceGraph, TaintAnalyzer,
};
use crate::analysis::storage::stable_function_key;
use rbuilder_graph::code_index::hash_code;
use rbuilder_graph::schema::Node;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// Aggregated CFG pass results for discover reporting and archive export.
#[derive(Debug, Default)]
pub struct CfgAnalysisBatchResult {
    pub success_count: usize,
    pub error_count: usize,
    pub total_flows: usize,
    pub vulnerable_flows: usize,
    pub cache_hits: usize,
    pub recomputed: usize,
    pub skipped_unchanged: usize,
    pub orphans_removed: usize,
    pub archive_records: Vec<CfgPdgRecord>,
    pub archive_unchanged: bool,
}

struct CfgFunctionWork {
    analysis: Option<FunctionAnalysis>,
    archive_record: Option<CfgPdgRecord>,
    flow_count: usize,
    vulnerable_count: usize,
    from_cache: bool,
    skip_persist: bool,
}

struct CfgIncrementalCache {
    index: HashMap<String, AnalysisIndexEntry>,
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
        if entry.code_hash != *code_hash {
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

struct FileSourceCache {
    sources: HashMap<String, Arc<String>>,
}

struct FileBatch {
    file_path: String,
    language: String,
    functions: Vec<usize>,
}

/// Analyze all repository functions in parallel with incremental reuse and bincode persistence.
pub fn run_cfg_analysis_batch(
    functions: &[Node],
    storage: &AnalysisStorage,
    repo_root: &Path,
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

    let sources = preload_file_sources(functions);
    let batches = group_functions_by_file(functions);

    let works: Vec<Vec<Option<CfgFunctionWork>>> = batches
        .par_iter()
        .map(|batch| process_file_batch(batch, functions, &sources, &cache))
        .collect();

    let flat: Vec<Option<CfgFunctionWork>> = works.into_iter().flatten().collect();

    let mut saves: Vec<FunctionAnalysis> = Vec::new();
    let mut result = CfgAnalysisBatchResult::default();
    for work in flat {
        match work {
            None => result.error_count += 1,
            Some(w) => {
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
                if let Some(record) = w.archive_record {
                    result.archive_records.push(record);
                }
                if let Some(analysis) = w.analysis {
                    if !w.skip_persist {
                        saves.push(analysis);
                    }
                }
            }
        }
    }

    saves.par_iter().for_each(|analysis| {
        let _ = storage.save_function_no_index(analysis);
    });
    let _ = storage.refresh_analysis_index_from_analyses(&saves);

    let active_keys = active_stable_keys(functions, Some(&sources));
    let _ = sync_storage_function_ids(storage, functions);
    result.orphans_removed = storage
        .purge_stale_by_stable_keys(&active_keys)
        .unwrap_or(0);
    result.archive_unchanged =
        result.skipped_unchanged == result.success_count && result.recomputed == 0;
    result
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

fn preload_file_sources(functions: &[Node]) -> FileSourceCache {
    let paths: HashSet<String> = functions.iter().filter_map(|n| n.file_path.clone()).collect();
    let sources: HashMap<String, Arc<String>> = paths
        .par_iter()
        .filter_map(|path| {
            let content = std::fs::read_to_string(path).ok()?;
            Some((path.clone(), Arc::new(content)))
        })
        .collect();
    FileSourceCache { sources }
}

fn group_functions_by_file(functions: &[Node]) -> Vec<FileBatch> {
    let mut by_file: HashMap<String, (String, Vec<usize>)> = HashMap::new();
    for (idx, func) in functions.iter().enumerate() {
        let Some(file_path) = func.file_path.as_ref() else {
            continue;
        };
        let lang = cfg_language_id_from_path(Path::new(file_path))
            .unwrap_or("")
            .to_string();
        by_file
            .entry(file_path.clone())
            .or_insert_with(|| (lang, Vec::new()))
            .1
            .push(idx);
    }
    by_file
        .into_iter()
        .map(|(file_path, (language, functions))| FileBatch {
            file_path,
            language,
            functions,
        })
        .collect()
}

fn active_stable_keys(
    functions: &[Node],
    sources: Option<&FileSourceCache>,
) -> HashSet<String> {
    let mut keys = HashSet::new();
    for func in functions {
        let Some(file_path) = func.file_path.as_ref() else {
            continue;
        };
        let hash = if let Some(code_hash) = func.code_hash.as_ref() {
            code_hash.clone()
        } else if let Some(cache) = sources {
            let Some(source) = cache.sources.get(file_path) else {
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

fn process_file_batch(
    batch: &FileBatch,
    functions: &[Node],
    sources: &FileSourceCache,
    cache: &CfgIncrementalCache,
) -> Vec<Option<CfgFunctionWork>> {
    if batch.language.is_empty() {
        return batch.functions.iter().map(|_| None).collect();
    }

    let source_arc = match sources.sources.get(&batch.file_path) {
        Some(s) => s.clone(),
        None => return batch.functions.iter().map(|_| None).collect(),
    };
    let source = source_arc.as_str();
    let bytes = source.as_bytes();

    let parsed = ParsedSourceFile::parse(&batch.language, bytes).ok();

    batch
        .functions
        .iter()
        .map(|&idx| {
            let func = &functions[idx];
            analyze_function_in_file(
                func,
                &batch.language,
                &batch.file_path,
                source,
                parsed.as_ref(),
                cache,
            )
        })
        .collect()
}

fn resolve_code_hash(func_node: &Node, source: &str) -> String {
    func_node
        .code_hash
        .clone()
        .unwrap_or_else(|| hash_code(source))
}

fn analyze_function_in_file(
    func_node: &Node,
    language: &str,
    file_path: &str,
    source: &str,
    parsed: Option<&ParsedSourceFile>,
    cache: &CfgIncrementalCache,
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

    compute_function_cfg(func_node, language, file_path, source, &code_hash, parsed, false)
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
        analysis: None,
        archive_record: None,
        flow_count: entry.flow_count,
        vulnerable_count: entry.vulnerable_count,
        from_cache: true,
        skip_persist: true,
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
        analysis: None,
        archive_record: None,
        flow_count: entry.flow_count,
        vulnerable_count: entry.vulnerable_count,
        from_cache: true,
        skip_persist: true,
    })
}

fn compute_function_cfg(
    func_node: &Node,
    language: &str,
    file_path: &str,
    source: &str,
    code_hash: &str,
    parsed: Option<&ParsedSourceFile>,
    from_cache: bool,
) -> Option<CfgFunctionWork> {
    let cfg_data = parsed
        .and_then(|file| file.build_cfg(language, source.as_bytes(), &func_node.name).ok())
        .or_else(|| build_cfg_for_function(language, source, &func_node.name).ok())?;

    compute_from_cfg(
        func_node,
        file_path,
        source,
        code_hash,
        language,
        cfg_data,
        from_cache,
    )
}

fn compute_from_cfg(
    func_node: &Node,
    file_path: &str,
    source: &str,
    code_hash: &str,
    language: &str,
    cfg_data: ControlFlowGraph,
    from_cache: bool,
) -> Option<CfgFunctionWork> {
    let dom_data = DominatorTree::build(&cfg_data);
    let pdg_data =
        ProgramDependenceGraph::build_with_dominator(&cfg_data, source.as_bytes(), &dom_data).ok();

    let (taint_data, flow_count, vulnerable_count) = if let Some(ref pdg) = pdg_data {
        let mut analyzer = TaintAnalyzer::with_dominator(pdg, &cfg_data, dom_data);
        analyzer.detect_patterns(language);
        let flows = analyzer.analyze();
        let vulnerable = flows.iter().filter(|f| f.is_vulnerable()).count();
        let count = flows.len();
        let taint = if flows.is_empty() { None } else { Some(flows) };
        (taint, count, vulnerable)
    } else {
        (None, 0, 0)
    };

    let analysis = FunctionAnalysis {
        function_id: func_node.id,
        function_name: func_node.name.clone(),
        file_path: file_path.to_string(),
        code_hash: Some(code_hash.to_string()),
        cfg: Some(cfg_data),
        pdg: pdg_data,
        dominance: None,
        taint: taint_data,
    };

    let archive_record = archive_record_from_analysis(func_node, file_path, code_hash, &analysis);

    Some(CfgFunctionWork {
        analysis: Some(analysis),
        archive_record,
        flow_count,
        vulnerable_count,
        from_cache,
        skip_persist: false,
    })
}

fn archive_record_from_analysis(
    func_node: &Node,
    file_path: &str,
    code_hash: &str,
    analysis: &FunctionAnalysis,
) -> Option<CfgPdgRecord> {
    match (&analysis.cfg, &analysis.pdg) {
        (Some(cfg), Some(pdg)) => Some(CfgPdgRecord {
            function_id: func_node.id,
            code_hash: code_hash.to_string(),
            function_name: func_node.name.clone(),
            file_path: Some(file_path.to_string()),
            cfg: cfg.clone(),
            pdg: pdg.clone(),
        }),
        _ => None,
    }
}
