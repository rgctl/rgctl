//! Reverse-dependency expansion for incremental graph updates.

use rgctl_error::Result;
use rgctl_graph::columnar_snapshot::ColumnarGraphMmap;
use rgctl_graph::normalize_path_str;
use rgctl_graph::schema::{EdgeType, NodeType};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::Path;
use uuid::Uuid;

struct InternedFile {
    path: String,
    stripped_repo: bool,
    node_ids: Vec<Uuid>,
}

#[inline]
fn matches_slash_boundary_suffix(path: &str, seeds: &FxHashSet<String>) -> bool {
    if seeds.contains(path) {
        return true;
    }
    for (idx, byte) in path.bytes().enumerate() {
        if byte == b'/' && seeds.contains(&path[idx + 1..]) {
            return true;
        }
    }
    false
}

fn canonicalize_file_path(raw: &str, repo_prefix: Option<&str>) -> (String, bool) {
    let norm = normalize_path_str(raw);
    let s = norm.as_ref();
    if let Some(prefix) = repo_prefix {
        if let Some(rest) = s.strip_prefix(prefix) {
            let trimmed = rest.trim_start_matches('/');
            if !trimmed.is_empty() {
                return (trimmed.to_owned(), true);
            }
        }
    }
    if !s.starts_with('/') {
        return (s.trim_start_matches("./").to_owned(), repo_prefix.is_some());
    }
    (norm.into_owned(), false)
}

/// Collect declaring files for nodes with incoming `Calls` edges into `seed_files` (1 hop).
pub fn incoming_callers_files(
    col: &ColumnarGraphMmap,
    seed_files: &[String],
) -> Result<Vec<String>> {
    incoming_callers_files_depth_in_repo(col, None, seed_files, 1)
}

/// Expand `seed_files` by up to `depth` hops of reverse call dependencies.
pub fn incoming_callers_files_depth(
    col: &ColumnarGraphMmap,
    seed_files: &[String],
    depth: usize,
) -> Result<Vec<String>> {
    incoming_callers_files_depth_in_repo(col, None, seed_files, depth)
}

/// Expand `seed_files` by up to `depth` hops of reverse call dependencies, stripping `repo_root`
/// when present so returned caller paths are canonical repo-relative paths and root-level files
/// (e.g. `mod.rs`) never collide with nested files (`src/sub/mod.rs`) or filename substrings
/// (`foo_mod.rs`).
pub fn incoming_callers_files_depth_in_repo(
    col: &ColumnarGraphMmap,
    repo_root: Option<&Path>,
    seed_files: &[String],
    depth: usize,
) -> Result<Vec<String>> {
    if depth == 0 || seed_files.is_empty() || col.node_count() == 0 {
        return Ok(Vec::new());
    }

    let repo_prefix = repo_root.map(|root| {
        normalize_path_str(root.to_string_lossy().as_ref())
            .trim_end_matches('/')
            .to_owned()
    });
    let repo_prefix_str = repo_prefix.as_deref().filter(|s| !s.is_empty());

    let mut seeds: FxHashSet<String> =
        FxHashSet::with_capacity_and_hasher(seed_files.len(), Default::default());
    for seed in seed_files {
        let (canon, _) = canonicalize_file_path(seed, repo_prefix_str);
        if !canon.is_empty() {
            seeds.insert(canon);
        }
    }
    if seeds.is_empty() {
        return Ok(Vec::new());
    }

    let node_count = col.node_count();
    let mut files: Vec<InternedFile> = Vec::new();
    let mut raw_to_file_id: FxHashMap<&str, u32> = FxHashMap::default();
    let mut canon_to_file_id: FxHashMap<String, u32> = FxHashMap::default();
    let mut id_to_file_id: FxHashMap<Uuid, u32> =
        FxHashMap::with_capacity_and_hasher(node_count, Default::default());

    for idx in 0..node_count {
        let raw_path = match col.node_file_path_str(idx)? {
            Some(p) if !p.is_empty() => p,
            _ if col.node_type_at(idx)? == NodeType::File => {
                let name = col.node_name_str(idx)?;
                if name.is_empty() {
                    continue;
                }
                name
            }
            _ => continue,
        };

        let file_id = if let Some(&fid) = raw_to_file_id.get(raw_path) {
            fid
        } else {
            let (canon, stripped_repo) = canonicalize_file_path(raw_path, repo_prefix_str);
            let fid = if let Some(&existing) = canon_to_file_id.get(canon.as_str()) {
                if stripped_repo {
                    files[existing as usize].stripped_repo = true;
                }
                existing
            } else {
                let next_id = files.len() as u32;
                canon_to_file_id.insert(canon.clone(), next_id);
                files.push(InternedFile {
                    path: canon,
                    stripped_repo,
                    node_ids: Vec::new(),
                });
                next_id
            };
            raw_to_file_id.insert(raw_path, fid);
            fid
        };

        let id = col.node_id_at(idx)?;
        id_to_file_id.insert(id, file_id);
        files[file_id as usize].node_ids.push(id);
    }

    let mut visited_files = vec![false; files.len()];
    let mut target_nodes: FxHashSet<Uuid> = FxHashSet::default();

    for (idx, entry) in files.iter().enumerate() {
        let matched = if entry.stripped_repo {
            seeds.contains(entry.path.as_str())
        } else {
            matches_slash_boundary_suffix(&entry.path, &seeds)
        };
        if matched {
            visited_files[idx] = true;
            target_nodes.extend(entry.node_ids.iter().copied());
        }
    }

    if target_nodes.is_empty() {
        return Ok(Vec::new());
    }

    let mut expanded_file_ids: Vec<u32> = Vec::new();
    let mut hop_file_ids: Vec<u32> = Vec::new();

    for _ in 0..depth {
        hop_file_ids.clear();
        col.for_each_edge(|from, to, edge_type| {
            if edge_type == EdgeType::Calls && target_nodes.contains(&to) {
                if let Some(&caller_fid) = id_to_file_id.get(&from) {
                    let slot = &mut visited_files[caller_fid as usize];
                    if !*slot {
                        *slot = true;
                        hop_file_ids.push(caller_fid);
                    }
                }
            }
            Ok(())
        })?;

        if hop_file_ids.is_empty() {
            break;
        }

        target_nodes.clear();
        for &fid in &hop_file_ids {
            expanded_file_ids.push(fid);
            target_nodes.extend(files[fid as usize].node_ids.iter().copied());
        }
    }

    Ok(expanded_file_ids
        .into_iter()
        .map(|fid| files[fid as usize].path.clone())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::schema::{Edge, Node, NodeType};
    use rgctl_graph::{MmappedGraphSnapshot, write_columnar_from_nodes_edges};
    use tempfile::TempDir;

    fn open_fixture(nodes: Vec<Node>, edges: Vec<Edge>) -> (TempDir, MmappedGraphSnapshot) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("snap.bin");
        write_columnar_from_nodes_edges(nodes, edges, &path).unwrap();
        let store = MmappedGraphSnapshot::open(&path).unwrap();
        (tmp, store)
    }

    #[test]
    fn incoming_callers_files_one_hop() {
        let main_fn = Node::new(NodeType::Function, "main").with_file_path("main.rs");
        let login = Node::new(NodeType::Function, "login").with_file_path("auth.rs");
        let call = Edge::new(main_fn.id, login.id, EdgeType::Calls);
        let (_tmp, store) = open_fixture(vec![main_fn, login], vec![call]);
        let col = store.columnar().expect("columnar");

        let callers = incoming_callers_files(col, &["auth.rs".into()]).unwrap();
        assert_eq!(callers, vec!["main.rs".to_string()]);
    }

    #[test]
    fn incoming_callers_files_depth_zero_is_empty() {
        let main_fn = Node::new(NodeType::Function, "main").with_file_path("main.rs");
        let login = Node::new(NodeType::Function, "login").with_file_path("auth.rs");
        let call = Edge::new(main_fn.id, login.id, EdgeType::Calls);
        let (_tmp, store) = open_fixture(vec![main_fn, login], vec![call]);
        let col = store.columnar().expect("columnar");

        let callers = incoming_callers_files_depth(col, &["auth.rs".into()], 0).unwrap();
        assert!(callers.is_empty());
    }

    #[test]
    fn incoming_callers_rejects_unanchored_filename_substrings() {
        let caller_good = Node::new(NodeType::Function, "use_mod").with_file_path("src/caller.rs");
        let target_mod = Node::new(NodeType::Function, "real_mod").with_file_path("src/mod.rs");
        let caller_false =
            Node::new(NodeType::Function, "use_foo_mod").with_file_path("src/unrelated.rs");
        let foo_mod = Node::new(NodeType::Function, "foo_helper").with_file_path("src/foo_mod.rs");

        let edges = vec![
            Edge::new(caller_good.id, target_mod.id, EdgeType::Calls),
            Edge::new(caller_false.id, foo_mod.id, EdgeType::Calls),
        ];
        let (_tmp, store) =
            open_fixture(vec![caller_good, target_mod, caller_false, foo_mod], edges);
        let col = store.columnar().expect("columnar");

        let callers = incoming_callers_files(col, &["mod.rs".into()]).unwrap();
        assert_eq!(
            callers,
            vec!["src/caller.rs".to_string()],
            "mod.rs must match /mod.rs segment boundary and never foo_mod.rs"
        );
    }

    #[test]
    fn incoming_callers_in_repo_strips_root_and_avoids_nested_basename_collision() {
        let root = Path::new("/workspace/repo");
        let root_caller =
            Node::new(NodeType::Function, "call_root").with_file_path("/workspace/repo/a.rs");
        let root_mod =
            Node::new(NodeType::Function, "root_fn").with_file_path("/workspace/repo/mod.rs");
        let nested_caller =
            Node::new(NodeType::Function, "call_nested").with_file_path("/workspace/repo/b.rs");
        let nested_mod = Node::new(NodeType::Function, "nested_fn")
            .with_file_path("/workspace/repo/src/sub/mod.rs");

        let edges = vec![
            Edge::new(root_caller.id, root_mod.id, EdgeType::Calls),
            Edge::new(nested_caller.id, nested_mod.id, EdgeType::Calls),
        ];
        let (_tmp, store) = open_fixture(
            vec![root_caller, root_mod, nested_caller, nested_mod],
            edges,
        );
        let col = store.columnar().expect("columnar");

        let callers =
            incoming_callers_files_depth_in_repo(col, Some(root), &["mod.rs".into()], 1).unwrap();
        assert_eq!(
            callers,
            vec!["a.rs".to_string()],
            "editing root mod.rs must not cascade callers of src/sub/mod.rs, and must return repo-relative paths"
        );
    }
}
