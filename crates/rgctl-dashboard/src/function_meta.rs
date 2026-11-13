//! Resolve function display names for CFG/slice/dataflow dashboard exports.

use rgctl_analysis::storage::AnalysisStorage;
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::schema::NodeType;
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

/// Lookup with fallbacks: explicit record fields → analysis index cache → UUID.
pub fn resolve_function_meta(
    function_id: &Uuid,
    record_name: &str,
    record_path: &Option<String>,
    _repo_root: &Path,
    _backend: &MemoryBackend,
    cache: &HashMap<Uuid, FunctionMeta>,
) -> FunctionMeta {
    let cached = cache.get(function_id);
    let name = if !record_name.is_empty() && !looks_like_uuid(record_name) {
        record_name.to_string()
    } else if let Some(meta) = cached.filter(|m| !m.0.is_empty() && !looks_like_uuid(&m.0)) {
        meta.0.clone()
    } else {
        function_id.to_string()
    };
    let path = record_path
        .clone()
        .or_else(|| cached.and_then(|meta| meta.1.clone()));
    (name, path)
}

fn merge_analysis_storage(repo_root: &Path, map: &mut HashMap<Uuid, FunctionMeta>) {
    let storage = AnalysisStorage::new(repo_root.join(".rgctl/analysis"));
    let Ok(index) = storage.load_analysis_index() else {
        return;
    };
    for entry in index.values() {
        if map.get(&entry.function_id).is_some_and(|(name, path)| {
            !name.is_empty() && !looks_like_uuid(name) && path.is_some()
        }) {
            continue;
        }
        let Ok(Some(metadata)) = storage.load_function_metadata(entry.function_id) else {
            continue;
        };
        let name = metadata.function_name;
        let path = (!metadata.file_path.is_empty()).then_some(metadata.file_path);
        map.entry(entry.function_id)
            .and_modify(|existing| {
                if existing.0.is_empty() || looks_like_uuid(&existing.0) {
                    existing.0 = name.clone();
                }
                if existing.1.is_none() {
                    existing.1 = path.clone();
                }
            })
            .or_insert((name, path));
    }
}

fn function_meta_from_backend(backend: &MemoryBackend) -> HashMap<Uuid, FunctionMeta> {
    let mut out = HashMap::new();
    let _ = backend.for_each_node(|n| {
        if n.node_type == NodeType::Function {
            out.insert(
                n.id,
                (
                    n.name.to_string(),
                    n.file_path.as_ref().map(|s| s.to_string()),
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
    use rgctl_graph::backend::GraphBackend;
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

        let backend = rgctl_graph::backend::MemoryBackend::new();
        let map = function_meta_map(repo, &backend);
        let (name, path) = map.get(&id).expect("meta");
        assert_eq!(name, "parseOrder");
        assert_eq!(path.as_deref(), Some("src/Order.java"));
    }

    #[test]
    fn resolve_prefers_record_name_over_uuid_fallback() {
        let tmp = TempDir::new().unwrap();
        let backend = rgctl_graph::backend::MemoryBackend::new();
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
    fn missing_record_path_uses_cache_without_replacing_record_name() {
        let tmp = TempDir::new().unwrap();
        let backend = MemoryBackend::new();
        let id = Uuid::new_v4();
        let cache = HashMap::from([(id, ("cached".into(), Some("src/file.rs".into())))]);
        let metadata = resolve_function_meta(&id, "record", &None, tmp.path(), &backend, &cache);
        assert_eq!(metadata, ("record".into(), Some("src/file.rs".into())));
    }

    #[test]
    fn storage_metadata_cannot_replace_current_graph_names() {
        let tmp = TempDir::new().unwrap();
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Function, "current").with_file_path("src/current.rs");
        let id = node.id;
        backend.insert_node(node).unwrap();
        AnalysisStorage::new(tmp.path().join(".rgctl/analysis"))
            .save_function(&FunctionAnalysis {
                function_id: id,
                function_name: "stale".into(),
                file_path: "src/stale.rs".into(),
                code_hash: Some("hash".into()),
                cfg: None,
                pdg: None,
                dominance: None,
                taint: None,
            })
            .unwrap();
        let cache = function_meta_map(tmp.path(), &backend);
        assert_eq!(
            cache[&id],
            ("current".into(), Some("src/current.rs".into()))
        );
    }

    #[test]
    fn backend_names_used_when_present() {
        let mut backend = rgctl_graph::backend::MemoryBackend::new();
        let node = Node::new(NodeType::Function, "main");
        let id = node.id;
        backend.insert_node(node).unwrap();
        let cache = function_meta_map(std::env::temp_dir().as_path(), &backend);
        let (name, _) = resolve_function_meta(
            &id,
            "",
            &None,
            std::env::temp_dir().as_path(),
            &backend,
            &cache,
        );
        assert_eq!(name, "main");
    }
}
