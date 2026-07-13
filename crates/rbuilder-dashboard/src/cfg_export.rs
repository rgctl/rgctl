//! Export CFG previews from `cfg_pdg.archive.bin` for the dashboard.

use crate::export_util::write_json_compact;
use rbuilder_analysis::cfg::{CfgEdgeType, ControlFlowGraph};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

pub const CFG_INDEX_FILE: &str = "cfg_index.json";
pub const CFG_DETAIL_DIR: &str = "cfg";
pub const CFG_ARCHIVE_BUNDLE_NAME: &str = "cfg_pdg.archive.bin";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgIndexPayload {
    pub schema_version: u32,
    pub available: bool,
    pub archive_path: Option<String>,
    /// `per_file` (default) or `archive_only` when detail JSON is omitted.
    #[serde(default = "default_detail_mode")]
    pub detail_mode: String,
    pub function_count: usize,
    pub functions: Vec<CfgFunctionEntry>,
}

fn default_detail_mode() -> String {
    "per_file".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgFunctionEntry {
    pub function_id: String,
    pub name: String,
    pub file_path: Option<String>,
    pub block_count: usize,
    pub cfg_edge_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgDetailPayload {
    pub schema_version: u32,
    pub function_id: String,
    pub name: String,
    pub file_path: Option<String>,
    pub entry: u32,
    pub exits: Vec<u32>,
    pub blocks: Vec<CfgBlockView>,
    pub edges: Vec<CfgEdgeView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idom: Option<Vec<Option<u32>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dominance_frontiers: Option<Vec<Vec<u32>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgBlockView {
    pub id: u32,
    pub label: String,
    pub start_line: usize,
    pub end_line: usize,
    pub statements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgEdgeView {
    pub from: u32,
    pub to: u32,
    pub edge_type: String,
}

#[derive(Debug, Default)]
pub struct CfgExportSummary {
    pub available: bool,
    pub function_count: usize,
    pub archive_copied: bool,
}

/// Skip per-function CFG JSON when function count exceeds this (archive still copied).
pub(crate) const CFG_DETAIL_INLINE_LIMIT: usize = 5_000;

pub fn export_cfg_bundle(
    _backend: &rbuilder_graph::backend::MemoryBackend,
    repo_root: &Path,
    out_dir: &Path,
) -> Result<CfgExportSummary, String> {
    Ok(crate::analysis_stream_export::export_cfg_slice_from_storage(
        _backend, repo_root, out_dir,
    )?
    .cfg)
}

pub(crate) fn cfg_detail_light(
    function_id: &Uuid,
    name: &str,
    file_path: Option<String>,
    cfg: &ControlFlowGraph,
) -> CfgDetailPayload {
    let block_index = index_blocks(cfg);
    let ordered_blocks = ordered_block_ids(&block_index);

    let blocks: Vec<CfgBlockView> = ordered_blocks
        .iter()
        .map(|(idx, block_id)| {
            let block = cfg.blocks.get(block_id).expect("block in index");
            let stmt_preview: Vec<String> = block
                .statements
                .iter()
                .take(6)
                .map(|s| truncate(&s.text, 96))
                .collect();
            CfgBlockView {
                id: *idx as u32,
                label: format!(
                    "B{idx} L{}–{} ({} stmts)",
                    block.start_line,
                    block.end_line,
                    block.statements.len()
                ),
                start_line: block.start_line,
                end_line: block.end_line,
                statements: stmt_preview,
            }
        })
        .collect();

    let edges: Vec<CfgEdgeView> = cfg
        .edges
        .iter()
        .filter_map(|e| {
            let from = *block_index.get(&e.from)?;
            let to = *block_index.get(&e.to)?;
            Some(CfgEdgeView {
                from: from as u32,
                to: to as u32,
                edge_type: edge_type_name(e.edge_type).into(),
            })
        })
        .collect();

    let entry = block_index.get(&cfg.entry).copied().unwrap_or(0) as u32;
    let exits: Vec<u32> = cfg
        .exits
        .iter()
        .filter_map(|id| block_index.get(id).copied())
        .map(|i| i as u32)
        .collect();

    CfgDetailPayload {
        schema_version: 2,
        function_id: function_id.to_string(),
        name: name.to_string(),
        file_path,
        entry,
        exits,
        blocks,
        edges,
        idom: None,
        dominance_frontiers: None,
    }
}

fn ordered_block_ids(index: &HashMap<Uuid, usize>) -> Vec<(usize, Uuid)> {
    let mut ordered: Vec<(usize, Uuid)> = index.iter().map(|(id, idx)| (*idx, *id)).collect();
    ordered.sort_by_key(|(idx, _)| *idx);
    ordered
}

fn index_blocks(cfg: &ControlFlowGraph) -> HashMap<Uuid, usize> {
    let mut ids: Vec<Uuid> = cfg.blocks.keys().copied().collect();
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

fn edge_type_name(t: CfgEdgeType) -> &'static str {
    match t {
        CfgEdgeType::Next => "next",
        CfgEdgeType::IfTrue => "if_true",
        CfgEdgeType::IfFalse => "if_false",
        CfgEdgeType::Jump => "jump",
        CfgEdgeType::Return => "return",
        CfgEdgeType::Exception => "exception",
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

pub(crate) fn write_empty_cfg_index(index_path: &Path) -> Result<(), String> {
    let index = CfgIndexPayload {
        schema_version: 2,
        available: false,
        archive_path: None,
        detail_mode: "per_file".into(),
        function_count: 0,
        functions: vec![],
    };
    write_json_compact(index_path, &index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_analysis::cfg::{BasicBlock, CfgEdge, ControlFlowGraph, Statement, StatementKind};
    use std::collections::HashSet;

    #[test]
    fn cfg_detail_assigns_block_indices() {
        let mut cfg = ControlFlowGraph::new();
        let entry = cfg.entry;
        let exit = uuid::Uuid::new_v4();
        cfg.blocks.insert(
            exit,
            BasicBlock {
                id: exit,
                statements: vec![Statement {
                    kind: StatementKind::Return,
                    line: 2,
                    text: "return x;".into(),
                    defined_vars: HashSet::new(),
                    used_vars: HashSet::new(),
                }],
                start_line: 2,
                end_line: 2,
            },
        );
        cfg.exits.push(exit);
        cfg.edges.push(CfgEdge {
            from: entry,
            to: exit,
            edge_type: CfgEdgeType::Next,
        });

        let detail = cfg_detail_light(&uuid::Uuid::new_v4(), "foo", Some("a.rs".into()), &cfg);
        assert_eq!(detail.blocks.len(), 2);
        assert_eq!(detail.edges.len(), 1);
        assert_eq!(detail.edges[0].edge_type, "next");
        assert!(detail.idom.is_none());
    }
}
