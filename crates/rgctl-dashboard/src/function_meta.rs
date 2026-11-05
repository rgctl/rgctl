//! Resolve function display names for CFG/slice/dataflow dashboard exports.

use rgctl_analysis::storage::{AnalysisIndexEntry, AnalysisStorage};
use rgctl_graph::graph_source::GraphSource;
use rgctl_graph::schema::NodeType;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// Human-readable function label and optional source path.
pub type FunctionMeta = (String, Option<String>);

/// Build UUID → (name, file_path) map for dashboard function lists.
#[allow(dead_code)] // Convenience wrapper used in unit tests; production uses `_with_index`.
pub fn function_meta_map(repo_root: &Path, graph: GraphSource<'_>) -> HashMap<Uuid, FunctionMeta> {
    function_meta_map_with_index(repo_root, graph, None)
}

/// Build UUID → (name, file_path) map, reusing a pre-loaded `analysis_index.bin` map
/// when available so `analysis_index.bin` is not deserialized twice during dashboard export.
pub fn function_meta_map_with_index(
    repo_root: &Path,
    graph: GraphSource<'_>,
    analysis_index: Option<&HashMap<String, AnalysisIndexEntry>>,
) -> HashMap<Uuid, FunctionMeta> {
    let mut map = function_meta_from_graph(graph);
    match analysis_index {
        Some(index) => merge_analysis_entries(index, &mut map),
        None => merge_analysis_storage(repo_root, &mut map),
    }
    map
}

/// Lookup with fallbacks: explicit record fields → analysis index cache → UUID.
pub fn resolve_function_meta(
    function_id: &Uuid,
    record_name: &str,
    record_path: &Option<String>,
    _repo_root: &Path,
    _graph: GraphSource<'_>,
    cache: &HashMap<Uuid, FunctionMeta>,
) -> FunctionMeta {
    if !record_name.is_empty() && !looks_like_uuid(record_name) {
        return (record_name.to_string(), record_path.clone());
    }
    if let Some(meta) = cache.get(function_id).filter(|m| !looks_like_uuid(&m.0)) {
        return meta.clone();
    }
    (function_id.to_string(), record_path.clone())
}

fn merge_analysis_storage(repo_root: &Path, map: &mut HashMap<Uuid, FunctionMeta>) {
    let storage = AnalysisStorage::new(repo_root.join(".rgctl/analysis"));
    let Ok(index) = storage.load_analysis_index() else {
        return;
    };
    merge_analysis_entries(&index, map);
}

fn merge_analysis_entries(
    index: &HashMap<String, AnalysisIndexEntry>,
    map: &mut HashMap<Uuid, FunctionMeta>,
) {
    for entry in index.values() {
        let Some((name, path)) = meta_from_stable_key(&entry.stable_key) else {
            continue;
        };
        map.insert(entry.function_id, (name, path));
    }
}

fn meta_from_stable_key(key: &str) -> Option<(String, Option<String>)> {
    let mut parts = key.split('\x1f');
    let file = parts.next()?;
    let name = parts.next()?;
    if name.is_empty() {
        return None;
    }
    let path = if file.is_empty() {
        None
    } else {
        Some(file.to_string())
    };
    Some((name.to_string(), path))
}

fn function_meta_from_graph(graph: GraphSource<'_>) -> HashMap<Uuid, FunctionMeta> {
    let mut out = HashMap::new();
    let _ = graph.for_each_hot_node(|n| {
        if n.node_type == NodeType::Function {
            out.insert(
                n.id,
                (
                    n.name.to_string(),
                    (!n.file_path.is_empty()).then(|| n.file_path.to_string()),
                ),
            );
        }
    });
    out
}

fn looks_like_uuid(name: &str) -> bool {
    name.len() == 36 && name.as_bytes().get(8) == Some(&b'-') && Uuid::parse_str(name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_analysis::storage::FunctionAnalysis;
    use rgctl_graph::backend::{GraphBackend, MemoryBackend};
    use rgctl_graph::schema::Node;
    use tempfile::TempDir;

    #[test]
    fn analysis_storage_supplies_names_for_archive_ids() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path();
        let id = Uuid::new_v4();
        let storage = AnalysisStorage::new(repo.join(".rgctl/analysis"));
        storage
            .save_function(&FunctionAnalysis {
                function_id: id,
                function_name: "parseOrder".into(),
                file_path: "src/Order.java".into(),
                code_hash: Some("h1".into()),
                cfg: None,
                pdg: None,
                dominance: None,
                taint: None,
            })
            .unwrap();

        let backend = MemoryBackend::new();
        let map = function_meta_map(repo, GraphSource::memory(&backend));
        let (name, path) = map.get(&id).expect("meta");
        assert_eq!(name, "parseOrder");
        assert_eq!(path.as_deref(), Some("src/Order.java"));
    }

    #[test]
    fn resolve_prefers_record_name_over_uuid_fallback() {
        let tmp = TempDir::new().unwrap();
        let backend = MemoryBackend::new();
        let id = Uuid::new_v4();
        let cache = function_meta_map(tmp.path(), GraphSource::memory(&backend));
        let (name, _) = resolve_function_meta(
            &id,
            "checkout",
            &Some("src/Checkout.java".into()),
            tmp.path(),
            GraphSource::memory(&backend),
            &cache,
        );
        assert_eq!(name, "checkout");
    }

    #[test]
    fn backend_names_used_when_present() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Function, "main");
        let id = node.id;
        backend.insert_node(node).unwrap();
        let cache = function_meta_map(
            std::env::temp_dir().as_path(),
            GraphSource::memory(&backend),
        );
        let (name, _) = resolve_function_meta(
            &id,
            "",
            &None,
            std::env::temp_dir().as_path(),
            GraphSource::memory(&backend),
            &cache,
        );
        assert_eq!(name, "main");
    }

    #[test]
    fn preloaded_analysis_index_avoids_disk_read() {
        let tmp = TempDir::new().unwrap();
        let backend = MemoryBackend::new();
        let id = Uuid::new_v4();
        let mut index = HashMap::new();
        let stable_key = "src/Billing.java\x1fcomputeInvoiceTotal";
        index.insert(
            stable_key.to_string(),
            AnalysisIndexEntry {
                function_id: id,
                stable_key: stable_key.to_string(),
                code_hash: "hash".into(),
                flow_count: 0,
                vulnerable_count: 0,
            },
        );
        // No `.rgctl/analysis/analysis_index.bin` exists on disk in `tmp.path()`,
        // proving `function_meta_map_with_index` uses the in-memory `Some(&index)`.
        let map =
            function_meta_map_with_index(tmp.path(), GraphSource::memory(&backend), Some(&index));
        let (name, path) = map.get(&id).expect("meta from preloaded index");
        assert_eq!(name, "computeInvoiceTotal");
        assert_eq!(path.as_deref(), Some("src/Billing.java"));
    }
}
