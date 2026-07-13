//! CFG detail record decoding for on-demand archive previews.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CfgDetailPayload {
    pub schema_version: u32,
    pub function_id: String,
    pub name: String,
    #[serde(default)]
    pub file_path: Option<String>,
    pub entry: u32,
    pub exits: Vec<u32>,
    pub blocks: Vec<CfgBlockView>,
    pub edges: Vec<CfgEdgeView>,
    #[serde(default)]
    pub idom: Option<Vec<Option<u32>>>,
    #[serde(default)]
    pub dominance_frontiers: Option<Vec<Vec<u32>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CfgBlockView {
    pub id: u32,
    pub label: String,
    pub start_line: usize,
    pub end_line: usize,
    pub statements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CfgEdgeView {
    pub from: u32,
    pub to: u32,
    pub edge_type: String,
}

/// Decode one CFG detail record fetched from `cfg_pdg.record_data.bin`.
#[wasm_bindgen(js_name = parseCfgDetail)]
pub fn parse_cfg_detail(bytes: &[u8]) -> Result<String, JsValue> {
    let detail: CfgDetailPayload =
        serde_json::from_slice(bytes).map_err(|e| JsValue::from_str(&format!("cfg detail decode: {e}")))?;
    serde_json::to_string(&detail).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_json() {
        let detail = CfgDetailPayload {
            schema_version: 2,
            function_id: "00000000-0000-0000-0000-000000000001".into(),
            name: "foo".into(),
            file_path: None,
            entry: 0,
            exits: vec![1],
            blocks: vec![],
            edges: vec![],
            idom: None,
            dominance_frontiers: None,
        };
        let bytes = serde_json::to_vec(&detail).unwrap();
        let json = parse_cfg_detail(&bytes).unwrap();
        assert!(json.contains("\"name\":\"foo\""));
    }
}
