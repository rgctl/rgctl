//! Columnar v2 engine for the browser WASM worker (Phases 1–3).

mod cfg_preview;
mod columnar;

pub use cfg_preview::parse_cfg_detail;

use columnar::ColumnarView;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct EngineContext {
    view: ColumnarView,
}

#[wasm_bindgen]
impl EngineContext {
    #[wasm_bindgen(constructor)]
    pub fn from_bytes(bytes: &[u8]) -> Result<EngineContext, JsValue> {
        ColumnarView::open(bytes.to_vec())
            .map(|view| EngineContext { view })
            .map_err(|e| JsValue::from_str(&e))
    }

    #[wasm_bindgen(getter)]
    pub fn schema_version(&self) -> u32 {
        self.view.schema_version()
    }

    #[wasm_bindgen(getter)]
    pub fn node_count(&self) -> u32 {
        self.view.node_count()
    }

    #[wasm_bindgen(getter)]
    pub fn edge_count(&self) -> u32 {
        self.view.edge_count()
    }

    #[wasm_bindgen(getter)]
    pub fn digest(&self) -> String {
        self.view.digest()
    }

    /// Expand metanode member indices into a subgraph JSON payload.
    #[wasm_bindgen(js_name = expandIndices)]
    pub fn expand_indices(&self, indices: &[u32], type_mask: u32) -> Result<String, JsValue> {
        let payload = self
            .view
            .expand_indices(indices, type_mask)
            .map_err(|e| JsValue::from_str(&e))?;
        serde_json::to_string(&payload).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Paginated node list filtered by node-type bitmask.
    #[wasm_bindgen(js_name = listNodes)]
    pub fn list_nodes(&self, type_mask: u32, offset: u32, limit: u32) -> Result<String, JsValue> {
        let payload = self
            .view
            .list_nodes(type_mask, offset, limit)
            .map_err(|e| JsValue::from_str(&e))?;
        serde_json::to_string(&payload).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Caller blast radius up to `max_depth` hops on the reverse call graph.
    #[wasm_bindgen(js_name = blastRadius)]
    pub fn blast_radius(&self, start_index: u32, max_depth: u32) -> Result<String, JsValue> {
        let payload = self
            .view
            .blast_radius(start_index, max_depth)
            .map_err(|e| JsValue::from_str(&e))?;
        serde_json::to_string(&payload).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
