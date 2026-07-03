//! Structured inspect CLI JSON responses with composable graph topology.

use crate::analysis::cfg::{BlockId, ControlFlowGraph};
use crate::analysis::dominance::DominatorTree;
use crate::analysis::pdg::ProgramDependenceGraph;
use super::slice_output::{cfg_topology_json, pdg_topology_json, CfgBlockNode, CfgEdgeNode, PdgGraphEdge, PdgGraphNode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Current inspect JSON schema version.
pub const INSPECT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InspectCfgResponse {
    pub schema_version: u32,
    pub symbol: String,
    pub layer: String,
    pub pruned: bool,
    pub nodes: Vec<CfgBlockNode>,
    pub edges: Vec<CfgEdgeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InspectPdgResponse {
    pub schema_version: u32,
    pub symbol: String,
    pub layer: String,
    pub nodes: Vec<PdgGraphNode>,
    pub edges: Vec<PdgGraphEdge>,
    pub data_deps: usize,
    pub control_deps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomBlockNode {
    pub block_index: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomRelation {
    pub block: usize,
    pub immediate_dominator: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomFrontier {
    pub block: usize,
    pub frontier_blocks: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InspectDomResponse {
    pub schema_version: u32,
    pub symbol: String,
    pub layer: String,
    pub nodes: Vec<DomBlockNode>,
    pub idom: Vec<DomRelation>,
    pub frontiers: Vec<DomFrontier>,
}

fn block_index_map(cfg: &ControlFlowGraph) -> HashMap<BlockId, usize> {
    let mut ids: Vec<_> = cfg.blocks.keys().copied().collect();
    ids.sort_by_key(|id| cfg.blocks[id].start_line);
    ids.into_iter()
        .enumerate()
        .map(|(idx, id)| (id, idx))
        .collect()
}

pub fn inspect_cfg_json(symbol: &str, cfg: &ControlFlowGraph, pruned: bool) -> InspectCfgResponse {
    let topo = cfg_topology_json("", symbol, cfg);
    InspectCfgResponse {
        schema_version: INSPECT_SCHEMA_VERSION,
        symbol: symbol.to_string(),
        layer: "cfg".into(),
        pruned,
        nodes: topo.nodes,
        edges: topo.edges,
    }
}

pub fn inspect_pdg_json(
    symbol: &str,
    pdg: &ProgramDependenceGraph,
    include_def_use: bool,
    data_deps: usize,
    control_deps: usize,
) -> InspectPdgResponse {
    let topo = pdg_topology_json("", symbol, pdg, include_def_use);
    InspectPdgResponse {
        schema_version: INSPECT_SCHEMA_VERSION,
        symbol: symbol.to_string(),
        layer: "pdg".into(),
        nodes: topo.nodes,
        edges: topo.edges,
        data_deps,
        control_deps,
    }
}

pub fn inspect_dom_json(
    symbol: &str,
    cfg: &ControlFlowGraph,
    dom: &DominatorTree,
    include_frontiers: bool,
) -> InspectDomResponse {
    let index_map = block_index_map(cfg);
    let mut nodes: Vec<DomBlockNode> = index_map
        .iter()
        .map(|(&block_id, &block_index)| {
            let block = &cfg.blocks[&block_id];
            DomBlockNode {
                block_index,
                start_line: block.start_line,
                end_line: block.end_line,
            }
        })
        .collect();
    nodes.sort_by_key(|n| n.block_index);

    let idom: Vec<DomRelation> = dom
        .idom
        .iter()
        .filter_map(|(child, parent)| {
            let child_idx = index_map.get(child).copied()?;
            let parent_idx = index_map.get(parent).copied()?;
            Some(DomRelation {
                block: child_idx,
                immediate_dominator: parent_idx,
            })
        })
        .collect();

    let frontiers = if include_frontiers {
        dom.frontiers
            .iter()
            .filter_map(|(block, frontier)| {
                let block_idx = index_map.get(block).copied()?;
                let frontier_blocks: Vec<usize> = frontier
                    .iter()
                    .filter_map(|f| index_map.get(f).copied())
                    .collect();
                Some(DomFrontier {
                    block: block_idx,
                    frontier_blocks,
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    InspectDomResponse {
        schema_version: INSPECT_SCHEMA_VERSION,
        symbol: symbol.to_string(),
        layer: "dom".into(),
        nodes,
        idom,
        frontiers,
    }
}

pub fn fixture_inspect_cfg_response() -> InspectCfgResponse {
    use super::slice_output::fixture_cfg_response;
    let cfg = fixture_cfg_response();
    InspectCfgResponse {
        schema_version: INSPECT_SCHEMA_VERSION,
        symbol: "main".into(),
        layer: "cfg".into(),
        pruned: false,
        nodes: cfg.nodes,
        edges: cfg.edges,
    }
}
