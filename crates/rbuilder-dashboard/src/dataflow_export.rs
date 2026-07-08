//! Dataflow index for dashboard Phase 7 (PDG bundles live under `slice/`).

use crate::slice_export::{SliceExportSummary, SLICE_DETAIL_DIR};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DATAFLOW_INDEX_FILE: &str = "dataflow_index.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataflowIndexPayload {
    pub schema_version: u32,
    pub available: bool,
    /// Relative path prefix for per-function PDG bundles (shared with slice export).
    pub detail_dir: String,
    pub function_count: usize,
    pub functions: Vec<DataflowFunctionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataflowFunctionEntry {
    pub function_id: String,
    pub name: String,
    pub file_path: Option<String>,
    pub pdg_nodes: usize,
    pub data_edges: usize,
    pub block_count: usize,
}

#[derive(Debug, Default)]
pub struct DataflowExportSummary {
    pub available: bool,
    pub function_count: usize,
}

pub fn export_dataflow_index(
    slice: &SliceExportSummary,
    out_dir: &Path,
) -> Result<DataflowExportSummary, String> {
    if !slice.available {
        let index = DataflowIndexPayload {
            schema_version: 1,
            available: false,
            detail_dir: SLICE_DETAIL_DIR.into(),
            function_count: 0,
            functions: vec![],
        };
        write_json(&out_dir.join(DATAFLOW_INDEX_FILE), &index)?;
        return Ok(DataflowExportSummary::default());
    }

    let slice_index: crate::slice_export::SliceIndexPayload = serde_json::from_slice(
        &fs::read(out_dir.join(crate::slice_export::SLICE_INDEX_FILE))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let cfg_index: Option<crate::cfg_export::CfgIndexPayload> = fs::read(out_dir.join(crate::cfg_export::CFG_INDEX_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let block_counts: std::collections::HashMap<String, usize> = cfg_index
        .map(|idx| {
            idx.functions
                .into_iter()
                .map(|f| (f.function_id, f.block_count))
                .collect()
        })
        .unwrap_or_default();

    let mut functions = Vec::with_capacity(slice_index.functions.len());
    for entry in &slice_index.functions {
        let bundle_path = out_dir
            .join(SLICE_DETAIL_DIR)
            .join(format!("{}.json", entry.function_id));
        let data_edges = if bundle_path.is_file() {
            let bundle: serde_json::Value =
                serde_json::from_slice(&fs::read(&bundle_path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            bundle["pdg"]["edges"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|e| e["kind"].as_str() == Some("data"))
                        .count()
                })
                .unwrap_or(0)
        } else {
            0
        };
        functions.push(DataflowFunctionEntry {
            function_id: entry.function_id.clone(),
            name: entry.name.clone(),
            file_path: entry.file_path.clone(),
            pdg_nodes: entry.pdg_nodes,
            data_edges,
            block_count: block_counts
                .get(&entry.function_id)
                .copied()
                .unwrap_or(0),
        });
    }

    let index = DataflowIndexPayload {
        schema_version: 1,
        available: true,
        detail_dir: SLICE_DETAIL_DIR.into(),
        function_count: functions.len(),
        functions,
    };
    write_json(&out_dir.join(DATAFLOW_INDEX_FILE), &index)?;

    Ok(DataflowExportSummary {
        available: true,
        function_count: index.function_count,
    })
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}
