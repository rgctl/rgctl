//! Resolve function display names for CFG/slice/dataflow dashboard exports.

use rbuilder_analysis::storage::AnalysisStorage;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::NodeType;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// Human-readable function label and optional source path.
pub type FunctionMeta = (String, Option<String>);

/// Build UUID → (name, file_path) map for dashboard function lists.
pub fn function_meta_map(repo_root: &Path, backend: &MemoryBackend) -> HashMap<Uuid, FunctionMeta> {
    let mut map = function_meta_from_backend(backend);
    merge_analysis_storage(repo_root, &mut map);
    map
}

/// Lookup with fallbacks: explicit record fields → analysis storage → backend → UUID.
pub fn resolve_function_meta(
    function_id: &Uuid,
    record_name: &str,
    record_path: &Option<String>,
    _repo_root: &Path,
    backend: &MemoryBackend,
    cache: &HashMap<Uuid, FunctionMeta>,
) -> FunctionMeta {
    if !record_name.is_empty() && !looks_like_uuid(record_name) {
        return (record_name.to_string(), record_path.clone());
    }
    if let Some(meta) = cache.get(function_id) {
        if !looks_like_uuid(&meta.0) {
            return meta.clone();
        }
    }
    if let Some((name, path)) = function_meta_from_backend(backend).get(function_id) {
        if !looks_like_uuid(name) {
            return (name.clone(), path.clone());
        }
    }
    (function_id.to_string(), record_path.clone())
}

fn merge_analysis_storage(repo_root: &Path, map: &mut HashMap<Uuid, FunctionMeta>) {
    let storage = AnalysisStorage::new(repo_root.join(".rbuilder/analysis"));
    let Ok(analyses) = storage.load_all() else {
        return;
    };
    for analysis in analyses {
        if analysis.function_name.is_empty() {
            continue;
        }
        let path = if analysis.file_path.is_empty() {
            None
        } else {
            Some(analysis.file_path)
        };
        map.insert(analysis.function_id, (analysis.function_name, path));
    }
}

fn function_meta_from_backend(backend: &MemoryBackend) -> HashMap<Uuid, FunctionMeta> {
    let mut out = HashMap::new();
    let _ = backend.for_each_node(|n| {
        if n.node_type == NodeType::Function {
            out.insert(n.id, (n.name.clone(), n.file_path.clone()));
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
    use rbuilder_analysis::storage::FunctionAnalysis;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::Node;
    use tempfile::TempDir;

    #[test]
    fn analysis_storage_supplies_names_for_archive_ids() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path();
        let id = Uuid::new_v4();
        let storage = AnalysisStorage::new(repo.join(".rbuilder/analysis"));
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

        let backend = rbuilder_graph::backend::MemoryBackend::new();
        let map = function_meta_map(repo, &backend);
        let (name, path) = map.get(&id).expect("meta");
        assert_eq!(name, "parseOrder");
        assert_eq!(path.as_deref(), Some("src/Order.java"));
    }

    #[test]
    fn resolve_prefers_record_name_over_uuid_fallback() {
        let tmp = TempDir::new().unwrap();
        let backend = rbuilder_graph::backend::MemoryBackend::new();
        let id = Uuid::new_v4();
        let cache = function_meta_map(tmp.path(), &backend);
        let (name, _) = resolve_function_meta(
            &id,
            "checkout",
            &Some("src/Checkout.java".into()),
            tmp.path(),
            &backend,
            &cache,
        );
        assert_eq!(name, "checkout");
    }

    #[test]
    fn backend_names_used_when_present() {
        let mut backend = rbuilder_graph::backend::MemoryBackend::new();
        let node = Node::new(NodeType::Function, "main".into());
        let id = node.id;
        backend.insert_node(node).unwrap();
        let cache = function_meta_map(std::env::temp_dir().as_path(), &backend);
        let (name, _) = resolve_function_meta(&id, "", &None, std::env::temp_dir().as_path(), &backend, &cache);
        assert_eq!(name, "main");
    }
}
