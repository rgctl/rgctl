//! Export PDG bundles for dashboard slicing (streamed from per-function analysis files).

use crate::export_util::write_json_compact;
use rbuilder_analysis::cfg::ControlFlowGraph;
use rbuilder_analysis::pdg::{PdgNodeId, ProgramDependenceGraph};
use rbuilder_graph::backend::MemoryBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const SLICE_INDEX_FILE: &str = "slice_index.json";
pub const SLICE_DETAIL_DIR: &str = "slice";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceIndexPayload {
    pub schema_version: u32,
    pub available: bool,
    pub function_count: usize,
    pub functions: Vec<SliceFunctionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceFunctionEntry {
    pub function_id: String,
    pub name: String,
    pub file_path: Option<String>,
    pub source_lines: usize,
    pub pdg_nodes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceBundlePayload {
    pub schema_version: u32,
    pub function_id: String,
    pub name: String,
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    pub total_lines: usize,
    pub pdg: SlicePdgPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicePdgPayload {
    pub nodes: Vec<SlicePdgNode>,
    pub edges: Vec<SlicePdgEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicePdgNode {
    pub id: String,
    pub line: usize,
    pub label: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_index: Option<u32>,
    pub defined: Vec<String>,
    pub used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicePdgEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
}

#[derive(Debug, Default)]
pub struct SliceExportSummary {
    pub available: bool,
    pub function_count: usize,
}

pub fn export_slice_bundle(
    backend: &MemoryBackend,
    repo_root: &Path,
    out_dir: &Path,
) -> Result<SliceExportSummary, String> {
    Ok(crate::analysis_stream_export::export_cfg_slice_from_storage(
        backend, repo_root, out_dir,
    )?
    .slice)
}

pub(crate) fn function_line_span(cfg: &ControlFlowGraph) -> (usize, usize) {
    let mut min_line = usize::MAX;
    let mut max_line = 0usize;
    for block in cfg.blocks.values() {
        if block.start_line > 0 {
            min_line = min_line.min(block.start_line);
        }
        max_line = max_line.max(block.end_line);
    }
    if min_line == usize::MAX {
        (1, max_line.max(1))
    } else {
        (min_line, max_line.max(min_line))
    }
}

pub(crate) fn export_pdg(pdg: &ProgramDependenceGraph, cfg: &ControlFlowGraph) -> SlicePdgPayload {
    let block_index_map = index_cfg_blocks(cfg);
    let mut ordered: Vec<_> = pdg.nodes.values().collect();
    ordered.sort_by_key(|n| (n.statement.line, n.id));

    let id_map: HashMap<PdgNodeId, usize> = ordered
        .iter()
        .enumerate()
        .map(|(idx, n)| (n.id, idx))
        .collect();

    let nodes: Vec<SlicePdgNode> = ordered
        .iter()
        .enumerate()
        .map(|(idx, n)| SlicePdgNode {
            id: pdg_node_label(idx),
            line: n.statement.line,
            label: n.statement.text.clone(),
            kind: format!("{:?}", n.statement.kind),
            block_index: block_index_map.get(&n.block).copied().map(|i| i as u32),
            defined: n.defined_vars.iter().cloned().collect(),
            used: n.used_vars.iter().cloned().collect(),
        })
        .collect();

    let mut edges = Vec::new();
    for dep in &pdg.data_deps {
        if let (Some(&from), Some(&to)) = (id_map.get(&dep.from), id_map.get(&dep.to)) {
            edges.push(SlicePdgEdge {
                source: pdg_node_label(from),
                target: pdg_node_label(to),
                kind: "data".into(),
                variable: Some(dep.variable.clone()),
            });
        }
    }
    for dep in &pdg.control_deps {
        if let (Some(&from), Some(&to)) = (id_map.get(&dep.controller), id_map.get(&dep.dependent))
        {
            edges.push(SlicePdgEdge {
                source: pdg_node_label(from),
                target: pdg_node_label(to),
                kind: "control".into(),
                variable: None,
            });
        }
    }

    SlicePdgPayload { nodes, edges }
}

fn pdg_node_label(index: usize) -> String {
    format!("node_{index}")
}

fn index_cfg_blocks(cfg: &ControlFlowGraph) -> HashMap<uuid::Uuid, usize> {
    let mut ids: Vec<uuid::Uuid> = cfg.blocks.keys().copied().collect();
    ids.sort_by_key(|id| {
        cfg.blocks
            .get(id)
            .map(|b| (b.start_line, b.end_line))
            .unwrap_or((0, 0))
    });
    ids.into_iter()
        .enumerate()
        .map(|(idx, id)| (id, idx))
        .collect()
}

pub(crate) fn write_empty_slice_index(index_path: &Path) -> Result<(), String> {
    let index = SliceIndexPayload {
        schema_version: 2,
        available: false,
        function_count: 0,
        functions: vec![],
    };
    write_json_compact(index_path, &index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_analysis::cfg::{ControlFlowGraph, Statement, StatementKind};
    use rbuilder_analysis::pdg::ProgramDependenceGraph;
    use std::collections::HashSet;

    #[test]
    fn export_pdg_assigns_node_labels() {
        let mut cfg = ControlFlowGraph::new();
        let entry = cfg.entry;
        cfg.blocks
            .get_mut(&entry)
            .unwrap()
            .statements
            .push(Statement {
                kind: StatementKind::Assignment,
                line: 1,
                text: "x = 1".into(),
                defined_vars: HashSet::from(["x".into()]),
                used_vars: HashSet::new(),
            });
        let pdg = ProgramDependenceGraph::build(&cfg, b"x = 1\n").unwrap();
        let exported = export_pdg(&pdg, &cfg);
        assert!(!exported.nodes.is_empty());
        assert_eq!(exported.nodes[0].id, "node_0");
        assert_eq!(exported.nodes[0].block_index, Some(0));
    }

    #[test]
    fn function_line_span_from_cfg() {
        let mut cfg = ControlFlowGraph::new();
        let entry = cfg.entry;
        let block = cfg.blocks.get_mut(&entry).unwrap();
        block.start_line = 10;
        block.end_line = 42;
        assert_eq!(function_line_span(&cfg), (10, 42));
    }
}
