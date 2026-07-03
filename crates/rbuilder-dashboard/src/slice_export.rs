//! Export PDG + source bundles for dashboard slicing (Phase 5).

use rbuilder_analysis::cfg_pdg_archive::CfgPdgArchive;
use rbuilder_analysis::pdg::{PdgNodeId, ProgramDependenceGraph};
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::NodeType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

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
    pub source: String,
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
    let archive_path = CfgPdgArchive::default_path(repo_root);
    let index_path = out_dir.join(SLICE_INDEX_FILE);

    if !archive_path.is_file() {
        let index = SliceIndexPayload {
            schema_version: 1,
            available: false,
            function_count: 0,
            functions: vec![],
        };
        write_json(&index_path, &index)?;
        return Ok(SliceExportSummary::default());
    }

    let archive = CfgPdgArchive::load_from_path(&archive_path).map_err(|e| e.to_string())?;
    let names = function_names(backend);
    let detail_dir = out_dir.join(SLICE_DETAIL_DIR);
    if detail_dir.exists() {
        fs::remove_dir_all(&detail_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&detail_dir).map_err(|e| e.to_string())?;

    let mut functions = Vec::with_capacity(archive.records.len());
    let mut source_cache: HashMap<PathBuf, String> = HashMap::new();

    for (function_id, record) in &archive.records {
        let (name, file_path) = names
            .get(function_id)
            .cloned()
            .unwrap_or_else(|| (function_id.to_string(), None));

        let source = file_path
            .as_ref()
            .and_then(|p| read_source_cached(p, &mut source_cache))
            .unwrap_or_default();
        let total_lines = source.lines().count().max(1);
        let pdg = export_pdg(&record.pdg);

        let bundle = SliceBundlePayload {
            schema_version: 1,
            function_id: function_id.to_string(),
            name: name.clone(),
            file_path: file_path.clone(),
            source,
            total_lines,
            pdg: pdg.clone(),
        };
        write_json(&detail_dir.join(format!("{function_id}.json")), &bundle)?;

        functions.push(SliceFunctionEntry {
            function_id: function_id.to_string(),
            name,
            file_path,
            source_lines: total_lines,
            pdg_nodes: pdg.nodes.len(),
        });
    }

    functions.sort_by(|a, b| a.name.cmp(&b.name));

    let index = SliceIndexPayload {
        schema_version: 1,
        available: true,
        function_count: functions.len(),
        functions,
    };
    write_json(&index_path, &index)?;

    Ok(SliceExportSummary {
        available: true,
        function_count: index.function_count,
    })
}

fn function_names(backend: &MemoryBackend) -> HashMap<Uuid, (String, Option<String>)> {
    let mut out = HashMap::new();
    let _ = backend.for_each_node(|n| {
        if n.node_type == NodeType::Function {
            out.insert(n.id, (n.name.clone(), n.file_path.clone()));
        }
    });
    out
}

fn read_source_cached(path: &str, cache: &mut HashMap<PathBuf, String>) -> Option<String> {
    let key = PathBuf::from(path);
    if let Some(s) = cache.get(&key) {
        return Some(s.clone());
    }
    let text = fs::read_to_string(&key).ok()?;
    cache.insert(key, text.clone());
    Some(text)
}

fn export_pdg(pdg: &ProgramDependenceGraph) -> SlicePdgPayload {
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

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
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
        cfg.blocks.get_mut(&entry).unwrap().statements.push(Statement {
            kind: StatementKind::Assignment,
            line: 1,
            text: "x = 1".into(),
            defined_vars: HashSet::from(["x".into()]),
            used_vars: HashSet::new(),
        });
        let pdg = ProgramDependenceGraph::build(&cfg, b"x = 1\n").unwrap();
        let exported = export_pdg(&pdg);
        assert!(!exported.nodes.is_empty());
        assert_eq!(exported.nodes[0].id, "node_0");
    }
}
