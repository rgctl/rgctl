//! Parallel CFG/PDG/taint analysis for discover `--cfg` / `--all`.

use crate::analysis::{
    build_cfg_for_function, cfg_language_id_from_path, AnalysisStorage, CfgPdgRecord,
    DominatorTree, FunctionAnalysis, ProgramDependenceGraph, TaintAnalyzer,
};
use rbuilder_graph::code_index::hash_code;
use rbuilder_graph::schema::Node;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use uuid::Uuid;

/// Aggregated CFG pass results for discover reporting and archive export.
#[derive(Debug, Default)]
pub struct CfgAnalysisBatchResult {
    pub success_count: usize,
    pub error_count: usize,
    pub total_flows: usize,
    pub vulnerable_flows: usize,
    pub cache_hits: usize,
    pub recomputed: usize,
    pub orphans_removed: usize,
    pub archive_records: Vec<CfgPdgRecord>,
}

struct CfgFunctionWork {
    analysis: FunctionAnalysis,
    archive_record: Option<CfgPdgRecord>,
    flow_count: usize,
    vulnerable_count: usize,
    from_cache: bool,
}

struct CfgIncrementalCache {
    analysis_by_key: HashMap<String, FunctionAnalysis>,
}

/// Analyze all repository functions in parallel with incremental reuse and bincode persistence.
pub fn run_cfg_analysis_batch(
    functions: &[Node],
    storage: &AnalysisStorage,
    _repo_root: &Path,
) -> CfgAnalysisBatchResult {
    let cache = load_incremental_cache(storage);
    let sources = preload_file_sources(functions);

    let works: Vec<Option<CfgFunctionWork>> = functions
        .par_iter()
        .map(|func_node| analyze_function_cfg(func_node, &sources, &cache))
        .collect();

    let mut result = CfgAnalysisBatchResult::default();
    for work in works {
        match work {
            None => result.error_count += 1,
            Some(w) => {
                if w.from_cache {
                    result.cache_hits += 1;
                } else {
                    result.recomputed += 1;
                }
                if storage.save_function(&w.analysis).is_ok() {
                    result.success_count += 1;
                    result.total_flows += w.flow_count;
                    result.vulnerable_flows += w.vulnerable_count;
                    if let Some(record) = w.archive_record {
                        result.archive_records.push(record);
                    }
                } else {
                    result.error_count += 1;
                }
            }
        }
    }

    let active_ids: HashSet<Uuid> = functions.iter().map(|n| n.id).collect();
    result.orphans_removed = storage.purge_orphans(&active_ids).unwrap_or(0);
    result
}

fn load_incremental_cache(storage: &AnalysisStorage) -> CfgIncrementalCache {
    let analysis_by_key = storage.build_stable_key_index().unwrap_or_default();
    CfgIncrementalCache { analysis_by_key }
}

fn preload_file_sources(functions: &[Node]) -> HashMap<String, Option<String>> {
    let paths: HashSet<String> = functions.iter().filter_map(|n| n.file_path.clone()).collect();
    paths
        .into_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path).ok();
            (path, content)
        })
        .collect()
}

fn resolve_code_hash(func_node: &Node, source: &str) -> String {
    func_node
        .code_hash
        .clone()
        .unwrap_or_else(|| hash_code(source))
}

fn analyze_function_cfg(
    func_node: &Node,
    sources: &HashMap<String, Option<String>>,
    cache: &CfgIncrementalCache,
) -> Option<CfgFunctionWork> {
    let file_path = func_node.file_path.as_ref()?;

    let source = match sources.get(file_path) {
        Some(Some(s)) => s.as_str(),
        _ => return None,
    };

    let code_hash = resolve_code_hash(func_node, source);

    if func_node.code_hash.is_some() {
        if let Some(cached) = try_reuse_cached(func_node, file_path, &code_hash, cache) {
            return Some(cached);
        }
    }

    compute_function_cfg(func_node, file_path, source, &code_hash, false)
}

fn try_reuse_cached(
    func_node: &Node,
    file_path: &str,
    code_hash: &str,
    cache: &CfgIncrementalCache,
) -> Option<CfgFunctionWork> {
    use crate::analysis::storage::stable_function_key;

    let key = stable_function_key(file_path, &func_node.name, code_hash);
    let cached_analysis = cache.analysis_by_key.get(&key)?;
    if cached_analysis.code_hash.as_deref() != Some(code_hash) {
        return None;
    }
    if cached_analysis.cfg.is_none() || cached_analysis.pdg.is_none() {
        return None;
    }

    let mut analysis = cached_analysis.clone();
    analysis.function_id = func_node.id;
    analysis.function_name = func_node.name.clone();
    analysis.file_path = file_path.to_string();
    analysis.code_hash = Some(code_hash.to_string());

    if analysis.dominance.is_none() {
        if let Some(ref cfg) = analysis.cfg {
            analysis.dominance = Some(DominatorTree::build(cfg));
        }
    }

    let (flow_count, vulnerable_count) = taint_counts(analysis.taint.as_ref());

    let archive_record = match (&analysis.cfg, &analysis.pdg) {
        (Some(cfg), Some(pdg)) => Some(CfgPdgRecord {
            function_id: func_node.id,
            code_hash: code_hash.to_string(),
            function_name: func_node.name.clone(),
            file_path: Some(file_path.to_string()),
            cfg: cfg.clone(),
            pdg: pdg.clone(),
        }),
        _ => None,
    };

    Some(CfgFunctionWork {
        analysis,
        archive_record,
        flow_count,
        vulnerable_count,
        from_cache: true,
    })
}

fn compute_function_cfg(
    func_node: &Node,
    file_path: &str,
    source: &str,
    code_hash: &str,
    from_cache: bool,
) -> Option<CfgFunctionWork> {
    let lang = cfg_language_id_from_path(Path::new(file_path))?;

    let cfg_data = match build_cfg_for_function(lang, source, &func_node.name) {
        Ok(c) => Some(c),
        Err(_) => return None,
    };

    let pdg_data = cfg_data
        .as_ref()
        .and_then(|cfg| ProgramDependenceGraph::build(cfg, source.as_bytes()).ok());

    let dom_data = cfg_data.as_ref().map(DominatorTree::build);

    let (taint_data, flow_count, vulnerable_count) =
        if let (Some(ref cfg), Some(ref pdg)) = (&cfg_data, &pdg_data) {
            let mut analyzer = TaintAnalyzer::new(pdg, cfg);
            analyzer.detect_patterns(lang);
            let flows = analyzer.analyze();
            let vulnerable = flows.iter().filter(|f| f.is_vulnerable()).count();
            let count = flows.len();
            let taint = if flows.is_empty() {
                None
            } else {
                Some(flows)
            };
            (taint, count, vulnerable)
        } else {
            (None, 0, 0)
        };

    let archive_record = match (&cfg_data, &pdg_data) {
        (Some(cfg), Some(pdg)) => Some(CfgPdgRecord {
            function_id: func_node.id,
            code_hash: code_hash.to_string(),
            function_name: func_node.name.clone(),
            file_path: Some(file_path.to_string()),
            cfg: cfg.clone(),
            pdg: pdg.clone(),
        }),
        _ => None,
    };

    Some(CfgFunctionWork {
        analysis: FunctionAnalysis {
            function_id: func_node.id,
            function_name: func_node.name.clone(),
            file_path: file_path.to_string(),
            code_hash: Some(code_hash.to_string()),
            cfg: cfg_data,
            pdg: pdg_data,
            dominance: dom_data,
            taint: taint_data,
        },
        archive_record,
        flow_count,
        vulnerable_count,
        from_cache,
    })
}

fn taint_counts(taint: Option<&Vec<crate::analysis::TaintFlow>>) -> (usize, usize) {
    let Some(flows) = taint else {
        return (0, 0);
    };
    let vulnerable = flows.iter().filter(|f| f.is_vulnerable()).count();
    (flows.len(), vulnerable)
}
