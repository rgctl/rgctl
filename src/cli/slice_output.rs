//! Structured slice CLI JSON responses with composable graph topology.

use crate::analysis::cfg::{BlockId, ControlFlowGraph, CfgEdgeType};
use crate::analysis::pdg::{ProgramDependenceGraph, PdgNodeId};
use crate::analysis::{CodeSlice, SliceCriterion};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Current slice JSON schema version.
pub const SLICE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SliceCriterionJson {
    pub line: usize,
    pub variable: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CfgBlockNode {
    pub id: String,
    pub block_index: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub statements: Vec<CfgStatementNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CfgStatementNode {
    pub line: usize,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CfgEdgeNode {
    pub source: String,
    pub target: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PdgGraphNode {
    pub id: String,
    pub line: usize,
    pub label: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defined: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PdgGraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SliceCfgResponse {
    pub schema_version: u32,
    pub file: String,
    pub function: String,
    pub view: String,
    pub nodes: Vec<CfgBlockNode>,
    pub edges: Vec<CfgEdgeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SlicePdgResponse {
    pub schema_version: u32,
    pub file: String,
    pub function: String,
    pub view: String,
    pub nodes: Vec<PdgGraphNode>,
    pub edges: Vec<PdgGraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SliceTextResponse {
    pub schema_version: u32,
    pub file: String,
    pub criterion: SliceCriterionJson,
    pub direction: String,
    pub reduction_percent: f64,
    pub lines: Vec<usize>,
    pub nodes: Vec<PdgGraphNode>,
    pub edges: Vec<PdgGraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SliceTaintResponse {
    pub schema_version: u32,
    pub file: String,
    pub function: String,
    pub line: usize,
    pub variable: String,
    pub taint: bool,
    pub flows: usize,
    pub vulnerable: usize,
}

fn block_index_map(cfg: &ControlFlowGraph) -> HashMap<BlockId, usize> {
    let mut ids: Vec<_> = cfg.blocks.keys().copied().collect();
    ids.sort_by_key(|id| cfg.blocks[id].start_line);
    ids.into_iter()
        .enumerate()
        .map(|(idx, id)| (id, idx))
        .collect()
}

fn block_label(index: usize) -> String {
    format!("block_{index}")
}

fn cfg_edge_kind(edge_type: CfgEdgeType) -> String {
    format!("{edge_type:?}")
        .to_lowercase()
        .replace('_', "")
}

pub fn cfg_topology_json(
    file: &str,
    function: &str,
    cfg: &ControlFlowGraph,
) -> SliceCfgResponse {
    let index_map = block_index_map(cfg);
    let mut nodes = Vec::with_capacity(index_map.len());
    for (&block_id, &block_index) in &index_map {
        let block = &cfg.blocks[&block_id];
        nodes.push(CfgBlockNode {
            id: block_label(block_index),
            block_index,
            start_line: block.start_line,
            end_line: block.end_line,
            statements: block
                .statements
                .iter()
                .map(|s| CfgStatementNode {
                    line: s.line,
                    kind: format!("{:?}", s.kind),
                    text: s.text.clone(),
                })
                .collect(),
        });
    }
    nodes.sort_by_key(|n| n.block_index);

    let edges: Vec<CfgEdgeNode> = cfg
        .edges
        .iter()
        .filter_map(|e| {
            let from = index_map.get(&e.from).copied()?;
            let to = index_map.get(&e.to).copied()?;
            Some(CfgEdgeNode {
                source: block_label(from),
                target: block_label(to),
                kind: cfg_edge_kind(e.edge_type),
            })
        })
        .collect();

    SliceCfgResponse {
        schema_version: SLICE_SCHEMA_VERSION,
        file: file.to_string(),
        function: function.to_string(),
        view: "cfg".into(),
        nodes,
        edges,
    }
}

fn pdg_node_label(index: usize) -> String {
    format!("node_{index}")
}

pub fn pdg_topology_json(
    file: &str,
    function: &str,
    pdg: &ProgramDependenceGraph,
    include_def_use: bool,
) -> SlicePdgResponse {
    let mut ordered: Vec<_> = pdg.nodes.values().collect();
    ordered.sort_by_key(|n| (n.statement.line, n.id));

    let id_map: HashMap<PdgNodeId, usize> = ordered
        .iter()
        .enumerate()
        .map(|(idx, n)| (n.id, idx))
        .collect();

    let nodes: Vec<PdgGraphNode> = ordered
        .iter()
        .enumerate()
        .map(|(idx, n)| {
            let mut node = PdgGraphNode {
                id: pdg_node_label(idx),
                line: n.statement.line,
                label: n.statement.text.clone(),
                kind: format!("{:?}", n.statement.kind),
                defined: None,
                used: None,
            };
            if include_def_use {
                node.defined = Some(n.defined_vars.iter().cloned().collect());
                node.used = Some(n.used_vars.iter().cloned().collect());
            }
            node
        })
        .collect();

    let mut edges = Vec::new();
    for dep in &pdg.data_deps {
        if let (Some(&from), Some(&to)) = (id_map.get(&dep.from), id_map.get(&dep.to)) {
            edges.push(PdgGraphEdge {
                source: pdg_node_label(from),
                target: pdg_node_label(to),
                kind: "data".into(),
                variable: Some(dep.variable.clone()),
            });
        }
    }
    for dep in &pdg.control_deps {
        if let (Some(&from), Some(&to)) = (id_map.get(&dep.controller), id_map.get(&dep.dependent)) {
            edges.push(PdgGraphEdge {
                source: pdg_node_label(from),
                target: pdg_node_label(to),
                kind: "control".into(),
                variable: None,
            });
        }
    }

    SlicePdgResponse {
        schema_version: SLICE_SCHEMA_VERSION,
        file: file.to_string(),
        function: function.to_string(),
        view: "pdg".into(),
        nodes,
        edges,
    }
}

pub fn slice_subgraph_json(
    pdg: &ProgramDependenceGraph,
    statement_ids: &HashSet<PdgNodeId>,
    include_def_use: bool,
) -> (Vec<PdgGraphNode>, Vec<PdgGraphEdge>) {
    let mut ordered: Vec<_> = pdg
        .nodes
        .values()
        .filter(|n| statement_ids.contains(&n.id))
        .collect();
    ordered.sort_by_key(|n| (n.statement.line, n.id));

    let id_map: HashMap<PdgNodeId, usize> = ordered
        .iter()
        .enumerate()
        .map(|(idx, n)| (n.id, idx))
        .collect();

    let nodes: Vec<PdgGraphNode> = ordered
        .iter()
        .enumerate()
        .map(|(idx, n)| {
            let mut node = PdgGraphNode {
                id: pdg_node_label(idx),
                line: n.statement.line,
                label: n.statement.text.clone(),
                kind: format!("{:?}", n.statement.kind),
                defined: None,
                used: None,
            };
            if include_def_use {
                node.defined = Some(n.defined_vars.iter().cloned().collect());
                node.used = Some(n.used_vars.iter().cloned().collect());
            }
            node
        })
        .collect();

    let mut edges = Vec::new();
    for dep in &pdg.data_deps {
        if !statement_ids.contains(&dep.from) || !statement_ids.contains(&dep.to) {
            continue;
        }
        if let (Some(&from), Some(&to)) = (id_map.get(&dep.from), id_map.get(&dep.to)) {
            edges.push(PdgGraphEdge {
                source: pdg_node_label(from),
                target: pdg_node_label(to),
                kind: "data".into(),
                variable: Some(dep.variable.clone()),
            });
        }
    }
    for dep in &pdg.control_deps {
        if !statement_ids.contains(&dep.controller) || !statement_ids.contains(&dep.dependent) {
            continue;
        }
        if let (Some(&from), Some(&to)) = (id_map.get(&dep.controller), id_map.get(&dep.dependent)) {
            edges.push(PdgGraphEdge {
                source: pdg_node_label(from),
                target: pdg_node_label(to),
                kind: "control".into(),
                variable: None,
            });
        }
    }

    (nodes, edges)
}

pub fn text_slice_json(
    file: &str,
    criterion: &SliceCriterion,
    direction: &str,
    slice: &CodeSlice,
    pdg: &ProgramDependenceGraph,
) -> SliceTextResponse {
    let mut lines: Vec<_> = slice.lines.iter().copied().collect();
    lines.sort_unstable();
    let (nodes, edges) = slice_subgraph_json(pdg, &slice.statements, false);
    SliceTextResponse {
        schema_version: SLICE_SCHEMA_VERSION,
        file: file.to_string(),
        criterion: SliceCriterionJson {
            line: criterion.line,
            variable: criterion.variable.clone(),
        },
        direction: direction.to_string(),
        reduction_percent: slice.reduction_percent,
        lines,
        nodes,
        edges,
    }
}

pub fn taint_slice_json(
    file: &str,
    function: &str,
    line: usize,
    variable: &str,
    flows: usize,
    vulnerable: usize,
) -> SliceTaintResponse {
    SliceTaintResponse {
        schema_version: SLICE_SCHEMA_VERSION,
        file: file.to_string(),
        function: function.to_string(),
        line,
        variable: variable.to_string(),
        taint: true,
        flows,
        vulnerable,
    }
}

pub fn fixture_cfg_response() -> SliceCfgResponse {
    SliceCfgResponse {
        schema_version: SLICE_SCHEMA_VERSION,
        file: "src/main.rs".into(),
        function: "main".into(),
        view: "cfg".into(),
        nodes: vec![CfgBlockNode {
            id: "block_0".into(),
            block_index: 0,
            start_line: 1,
            end_line: 2,
            statements: vec![CfgStatementNode {
                line: 1,
                kind: "Expression".into(),
                text: "let x = 1;".into(),
            }],
        }],
        edges: vec![],
    }
}
