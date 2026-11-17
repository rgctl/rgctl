//! Complete function index for bundles that have no CFG/slice index.
//!
//! A lightweight discover (`--with-universe` without `--with-cfg`) still has every function node in
//! the graph, but no `cfg_index.json` / `slice_index.json`. The shell uses those indexes to resolve a
//! Focus key, so semantic hits would otherwise be display-only. This sidecar gives the shell one row
//! per function without paying for CFG/PDG details.

use crate::export_util::write_json_compact;
use rgctl_graph::graph_source::GraphSource;
use rgctl_graph::schema::NodeType;
use serde::Serialize;
use std::path::Path;

/// Filename under `.rgctl/dashboard/`.
pub const FUNCTION_INDEX_FILE: &str = "function_index.json";

#[derive(Debug, Serialize)]
struct FunctionIndexPayload {
    schema_version: u32,
    available: bool,
    function_count: usize,
    functions: Vec<FunctionIndexEntry>,
}

#[derive(Debug, Serialize)]
struct FunctionIndexEntry {
    function_id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_line: Option<usize>,
}

/// Write `function_index.json` and return the number of rows.
pub fn export_function_index(graph: GraphSource<'_>, out_dir: &Path) -> Result<usize, String> {
    let mut functions = Vec::new();
    graph
        .for_each_hot_node(|n| {
            if n.node_type != NodeType::Function {
                return;
            }
            functions.push(FunctionIndexEntry {
                function_id: n.id.to_string(),
                name: n.name.to_string(),
                file_path: (!n.file_path.is_empty()).then(|| n.file_path.to_string()),
                start_line: (n.line_number > 0).then_some(n.line_number),
                end_line: n.end_line,
            });
        })
        .map_err(|e| e.to_string())?;

    let count = functions.len();
    let payload = FunctionIndexPayload {
        schema_version: 1,
        available: true,
        function_count: count,
        functions,
    };
    write_json_compact(&out_dir.join(FUNCTION_INDEX_FILE), &payload)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::{GraphBackend, MemoryBackend};
    use rgctl_graph::schema::Node;
    use tempfile::TempDir;

    #[test]
    fn exports_every_function_with_its_stable_id() {
        let mut backend = MemoryBackend::new();
        let f = Node::new(NodeType::Function, "handleAuth");
        let id = f.id;
        backend.insert_node(f).unwrap();
        backend
            .insert_node(Node::new(NodeType::Class, "AuthService"))
            .unwrap();

        let tmp = TempDir::new().unwrap();
        let count = export_function_index(GraphSource::memory(&backend), tmp.path()).unwrap();
        assert_eq!(count, 1);

        let text = std::fs::read_to_string(tmp.path().join(FUNCTION_INDEX_FILE)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["function_count"], 1);
        assert_eq!(value["functions"][0]["name"], "handleAuth");
        assert_eq!(value["functions"][0]["function_id"], id.to_string());
        assert_eq!(value["functions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn skips_non_function_nodes() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(Node::new(NodeType::Class, "Service"))
            .unwrap();
        let tmp = TempDir::new().unwrap();
        assert_eq!(
            export_function_index(GraphSource::memory(&backend), tmp.path()).unwrap(),
            0
        );
    }
}
