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

/// Aggregated CFG pass results for discover reporting and archive export.
#[derive(Debug, Default)]
pub struct CfgAnalysisBatchResult {
    pub success_count: usize,
    pub error_count: usize,
    pub total_flows: usize,
    pub vulnerable_flows: usize,
    pub archive_records: Vec<CfgPdgRecord>,
}

struct CfgFunctionWork {
    analysis: FunctionAnalysis,
    archive_record: Option<CfgPdgRecord>,
    flow_count: usize,
    vulnerable_count: usize,
}

/// Analyze all repository functions in parallel, persist per-function JSON, and collect archive rows.
pub fn run_cfg_analysis_batch(
    functions: &[Node],
    storage: &AnalysisStorage,
) -> CfgAnalysisBatchResult {
    let sources = preload_file_sources(functions);

    let works: Vec<Option<CfgFunctionWork>> = functions
        .par_iter()
        .map(|func_node| analyze_function_cfg(func_node, &sources))
        .collect();

    let mut result = CfgAnalysisBatchResult::default();
    for work in works {
        match work {
            None => result.error_count += 1,
            Some(w) => {
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
    result
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

fn analyze_function_cfg(
    func_node: &Node,
    sources: &HashMap<String, Option<String>>,
) -> Option<CfgFunctionWork> {
    let file_path = func_node.file_path.as_ref()?;

    let source = match sources.get(file_path) {
        Some(Some(s)) => s.as_str(),
        _ => return None,
    };

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
            code_hash: hash_code(source),
            function_name: func_node.name.clone(),
            file_path: Some(file_path.clone()),
            cfg: cfg.clone(),
            pdg: pdg.clone(),
        }),
        _ => None,
    };

    Some(CfgFunctionWork {
        analysis: FunctionAnalysis {
            function_id: func_node.id,
            function_name: func_node.name.clone(),
            file_path: file_path.clone(),
            cfg: cfg_data,
            pdg: pdg_data,
            dominance: dom_data,
            taint: taint_data,
        },
        archive_record,
        flow_count,
        vulnerable_count,
    })
}
