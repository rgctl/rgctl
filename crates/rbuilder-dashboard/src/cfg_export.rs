//! Export CFG/dominance previews from `cfg_pdg.archive.bin` for the dashboard.

use crate::function_meta::{function_meta_map, resolve_function_meta};
use rbuilder_analysis::cfg::{CfgEdgeType, ControlFlowGraph};
use rbuilder_analysis::cfg_pdg_archive::CfgPdgArchive;
use rbuilder_analysis::dominance::DominatorTree;
use rbuilder_graph::backend::MemoryBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
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
    pub function_count: usize,
    pub functions: Vec<CfgFunctionEntry>,
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
    /// Immediate dominator per block index (`null` for entry / unreachable).
    pub idom: Vec<Option<u32>>,
    pub dominance_frontiers: Vec<Vec<u32>>,
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

pub fn export_cfg_bundle(
    backend: &MemoryBackend,
    repo_root: &Path,
    out_dir: &Path,
) -> Result<CfgExportSummary, String> {
    let archive_path = CfgPdgArchive::default_path(repo_root);
    let index_path = out_dir.join(CFG_INDEX_FILE);

    if !archive_path.is_file() {
        write_empty_cfg_index(&index_path)?;
        return Ok(CfgExportSummary::default());
    }

    let archive = match CfgPdgArchive::load_from_path(&archive_path) {
        Ok(archive) => archive,
        Err(_) => {
            // Stale or corrupt archive from a prior `--cfg` run must not block dashboard export.
            write_empty_cfg_index(&index_path)?;
            return Ok(CfgExportSummary::default());
        }
    };
    let meta_map = function_meta_map(repo_root, backend);

    let detail_dir = out_dir.join(CFG_DETAIL_DIR);
    if detail_dir.exists() {
        fs::remove_dir_all(&detail_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&detail_dir).map_err(|e| e.to_string())?;

    let mut functions = Vec::with_capacity(archive.records.len());

    for (function_id, record) in &archive.records {
        let (name, file_path) = resolve_function_meta(
            function_id,
            &record.function_name,
            &record.file_path,
            repo_root,
            backend,
            &meta_map,
        );

        let detail = cfg_detail(function_id, &name, file_path.clone(), &record.cfg);
        write_json(&detail_dir.join(format!("{function_id}.json")), &detail)?;

        functions.push(CfgFunctionEntry {
            function_id: function_id.to_string(),
            name,
            file_path,
            block_count: record.cfg.blocks.len(),
            cfg_edge_count: record.cfg.edges.len(),
        });
    }

    functions.sort_by(|a, b| a.name.cmp(&b.name));

    let archive_dest = out_dir.join(CFG_ARCHIVE_BUNDLE_NAME);
    fs::copy(&archive_path, &archive_dest).map_err(|e| e.to_string())?;

    let index = CfgIndexPayload {
        schema_version: 1,
        available: true,
        archive_path: Some(CFG_ARCHIVE_BUNDLE_NAME.into()),
        function_count: functions.len(),
        functions,
    };
    write_json(&index_path, &index)?;

    Ok(CfgExportSummary {
        available: true,
        function_count: index.function_count,
        archive_copied: true,
    })
}

fn cfg_detail(
    function_id: &Uuid,
    name: &str,
    file_path: Option<String>,
    cfg: &ControlFlowGraph,
) -> CfgDetailPayload {
    let block_index = index_blocks(cfg);
    let ordered_blocks = ordered_block_ids(&block_index);
    let dom = DominatorTree::build(cfg);

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

    let idom: Vec<Option<u32>> = ordered_blocks
        .iter()
        .map(|(_, block_id)| {
            dom.idom.get(block_id).and_then(|parent| {
                if *parent == *block_id {
                    None
                } else {
                    block_index.get(parent).copied().map(|i| i as u32)
                }
            })
        })
        .collect();

    let dominance_frontiers: Vec<Vec<u32>> = ordered_blocks
        .iter()
        .map(|(_, block_id)| {
            dom.frontiers
                .get(block_id)
                .map(|set| {
                    set.iter()
                        .filter_map(|id| block_index.get(id).copied())
                        .map(|i| i as u32)
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();

    CfgDetailPayload {
        schema_version: 1,
        function_id: function_id.to_string(),
        name: name.to_string(),
        file_path,
        entry,
        exits,
        blocks,
        edges,
        idom,
        dominance_frontiers,
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

fn write_empty_cfg_index(index_path: &Path) -> Result<(), String> {
    let index = CfgIndexPayload {
        schema_version: 1,
        available: false,
        archive_path: None,
        function_count: 0,
        functions: vec![],
    };
    write_json(index_path, &index)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
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

        let detail = cfg_detail(&uuid::Uuid::new_v4(), "foo", Some("a.rs".into()), &cfg);
        assert_eq!(detail.blocks.len(), 2);
        assert_eq!(detail.edges.len(), 1);
        assert_eq!(detail.edges[0].edge_type, "next");
    }
}
