//! Dataflow index for dashboard Phase 7 (PDG bundles live under `slice/`).
//!
//! Counts come from the streamed CFG/slice pass — this writer does not re-parse
//! per-function PDG JSON.

use crate::slice_export::SLICE_DETAIL_DIR;
use serde::{Deserialize, Serialize};
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

/// Write `dataflow_index.json` from rows collected during CFG/slice export.
pub fn export_dataflow_index(
    mut functions: Vec<DataflowFunctionEntry>,
    out_dir: &Path,
) -> Result<DataflowExportSummary, String> {
    if functions.is_empty() {
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

    functions.sort_by(|a, b| a.name.cmp(&b.name));
    let function_count = functions.len();
    let index = DataflowIndexPayload {
        schema_version: 1,
        available: true,
        detail_dir: SLICE_DETAIL_DIR.into(),
        function_count,
        functions,
    };
    write_json(&out_dir.join(DATAFLOW_INDEX_FILE), &index)?;

    Ok(DataflowExportSummary {
        available: true,
        function_count,
    })
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    crate::export_util::write_json_compact(path, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn empty_functions_writes_unavailable_index() {
        let tmp = TempDir::new().unwrap();
        let summary = export_dataflow_index(Vec::new(), tmp.path()).unwrap();
        assert!(!summary.available);
        let payload: DataflowIndexPayload =
            serde_json::from_slice(&fs::read(tmp.path().join(DATAFLOW_INDEX_FILE)).unwrap())
                .unwrap();
        assert!(!payload.available);
        assert!(payload.functions.is_empty());
    }

    #[test]
    fn writes_streamed_rows_without_slice_files() {
        let tmp = TempDir::new().unwrap();
        let summary = export_dataflow_index(
            vec![DataflowFunctionEntry {
                function_id: "abc".into(),
                name: "checkout".into(),
                file_path: Some("Cart.java".into()),
                pdg_nodes: 4,
                data_edges: 2,
                block_count: 3,
            }],
            tmp.path(),
        )
        .unwrap();
        assert!(summary.available);
        assert_eq!(summary.function_count, 1);
        let payload: DataflowIndexPayload =
            serde_json::from_slice(&fs::read(tmp.path().join(DATAFLOW_INDEX_FILE)).unwrap())
                .unwrap();
        assert_eq!(payload.functions[0].data_edges, 2);
        assert_eq!(payload.functions[0].block_count, 3);
    }
}
