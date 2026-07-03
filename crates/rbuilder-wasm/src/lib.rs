//! Columnar v2 header parser for the browser WASM worker (Phase 1).
//!
//! Full graph algorithms stay in native rbuilder-analysis for now; WASM only
//! validates and exposes counts from `graph_payload.bin`.

use wasm_bindgen::prelude::*;

const SNAPSHOT_MAGIC: [u8; 4] = *b"RBGR";
const COLUMNAR_VERSION: u32 = 2;
const HEADER_SIZE: usize = 136;

#[wasm_bindgen]
pub struct EngineContext {
    schema_version: u32,
    node_count: u32,
    edge_count: u32,
    digest: String,
}

#[wasm_bindgen]
impl EngineContext {
    /// Parse columnar v2 snapshot header from raw bytes (full file mmap in worker).
    #[wasm_bindgen(constructor)]
    pub fn from_bytes(bytes: &[u8]) -> Result<EngineContext, JsValue> {
        parse_header(bytes).map_err(|e| JsValue::from_str(&e))
    }

    #[wasm_bindgen(getter)]
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[wasm_bindgen(getter)]
    pub fn node_count(&self) -> u32 {
        self.node_count
    }

    #[wasm_bindgen(getter)]
    pub fn edge_count(&self) -> u32 {
        self.edge_count
    }

    #[wasm_bindgen(getter)]
    pub fn digest(&self) -> String {
        self.digest.clone()
    }
}

fn parse_header(bytes: &[u8]) -> Result<EngineContext, String> {
    if bytes.len() < HEADER_SIZE {
        return Err(format!(
            "payload truncated: {} bytes (need {HEADER_SIZE})",
            bytes.len()
        ));
    }
    if bytes[0..4] != SNAPSHOT_MAGIC {
        return Err("invalid graph payload magic (expected RBGR columnar v2)".into());
    }
    let format_version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if format_version != COLUMNAR_VERSION {
        return Err(format!(
            "unsupported payload format version {format_version} (expected {COLUMNAR_VERSION})"
        ));
    }
    let schema_version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let node_count = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
    let edge_count = u64::from_le_bytes(bytes[20..28].try_into().unwrap());
    if node_count > u32::MAX as u64 || edge_count > u32::MAX as u64 {
        return Err("node or edge count exceeds u32 (WASM API limit)".into());
    }
    let digest = std::str::from_utf8(&bytes[28..92])
        .map_err(|_| "digest field invalid utf-8".to_string())?
        .trim_end_matches('\0')
        .to_string();

    Ok(EngineContext {
        schema_version,
        node_count: node_count as u32,
        edge_count: edge_count as u32,
        digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_buffer() {
        assert!(parse_header(&[0u8; 8]).is_err());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut buf = vec![0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(b"XXXX");
        assert!(parse_header(&buf).is_err());
    }
}
