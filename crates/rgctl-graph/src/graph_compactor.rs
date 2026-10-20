//! Streaming log-structured compaction of a columnar snapshot with a delta segment.
//!
//! Pass 1 filters base nodes by invalidated file paths and appends delta nodes while
//! updating name/type indexes. Pass 2 streams base edges without building a full
//! topology `Vec`, keeping only endpoints present in the alive set.
//! Output is written to a temp path then atomically renamed.

use crate::columnar_snapshot::{ColumnarGraphMmap, EdgeRow, StringPool, write_columnar_assembled};
use crate::csr::{edge_type_from_u8, edge_type_to_u8};
use crate::normalize_path_str;
use crate::schema::{Edge, Node};
use crate::snapshot::MmappedGraphSnapshot;
use memmap2::Mmap;
use rgctl_error::{Error, Result};
use std::collections::HashSet;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// Extracted changes to merge into a base columnar snapshot.
#[derive(Debug, Default)]
pub struct DeltaSegment {
    /// Repo-relative (or normalized) file paths whose nodes must be dropped from the base.
    pub invalidated_files: HashSet<String>,
    /// Freshly extracted nodes from added/changed files.
    pub new_nodes: Vec<Node>,
    /// Freshly extracted edges from the delta extract (and optional relation rebuild).
    pub new_edges: Vec<Edge>,
}

impl DeltaSegment {
    /// Create an empty delta.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a path as invalidated (normalized separators).
    pub fn invalidate_file(&mut self, path: impl AsRef<str>) {
        self.invalidated_files
            .insert(normalize_path_str(path.as_ref()).into_owned());
    }
}

/// Compacts a base [`ColumnarGraphMmap`] with a [`DeltaSegment`] into a new snapshot file.
pub struct GraphCompactor<'a> {
    base: &'a ColumnarGraphMmap,
    delta: DeltaSegment,
}

/// Statistics from a compaction run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompactStats {
    /// Nodes kept from the base snapshot.
    pub nodes_kept: usize,
    /// Nodes dropped due to invalidated files.
    pub nodes_dropped: usize,
    /// Nodes appended from the delta.
    pub nodes_from_delta: usize,
    /// Edges kept from the base snapshot.
    pub edges_kept: usize,
    /// Edges dropped (endpoint not alive).
    pub edges_dropped: usize,
    /// Edges appended from the delta.
    pub edges_from_delta: usize,
    /// Content digest of the written snapshot.
    pub content_digest: String,
}

enum CompactNodeSource {
    Base(usize),
    Delta(usize),
}

struct CompactNodeEntry {
    id: Uuid,
    source: CompactNodeSource,
}

fn node_alive(entries: &[CompactNodeEntry], id: Uuid) -> bool {
    entries.binary_search_by_key(&id, |entry| entry.id).is_ok()
}

fn source_is_base(entries: &[CompactNodeEntry], id: Uuid) -> bool {
    entries
        .binary_search_by_key(&id, |entry| entry.id)
        .ok()
        .is_some_and(|i| matches!(entries[i].source, CompactNodeSource::Base(_)))
}

/// Prefer `Delta` over `Base` when the same UUID appears twice (e.g. `<external>` stubs).
fn dedupe_entries_prefer_delta(entries: &mut Vec<CompactNodeEntry>) {
    if entries.len() < 2 {
        return;
    }
    entries.sort_by_key(|entry| entry.id);
    let mut out: Vec<CompactNodeEntry> = Vec::with_capacity(entries.len());
    for entry in entries.drain(..) {
        if let Some(last) = out.last_mut() {
            if last.id == entry.id {
                if matches!(entry.source, CompactNodeSource::Delta(_)) {
                    *last = entry;
                }
                continue;
            }
        }
        out.push(entry);
    }
    *entries = out;
}

fn dedupe_sorted_edges(edge_meta: &mut Vec<(Uuid, Uuid, u8)>) {
    edge_meta.sort();
    edge_meta.dedup();
}

impl<'a> GraphCompactor<'a> {
    /// Create a compactor over a live mmap and owned delta.
    pub fn new(base: &'a ColumnarGraphMmap, delta: DeltaSegment) -> Self {
        Self { base, delta }
    }

    /// Compact to `output_path` via a sibling `.tmp` file and atomic rename.
    ///
    /// Base nodes copy extension bytes from mmap without bincode re-encode; delta nodes
    /// are encoded normally. Scratch dir is unused (kept for API stability).
    pub fn compact_to_path(self, output_path: &Path, _scratch_dir: &Path) -> Result<CompactStats> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut stats = CompactStats::default();
        let mut entries = Vec::new();

        for idx in 0..self.base.node_count() {
            if self
                .base
                .node_invalidated_at(idx, &self.delta.invalidated_files)?
            {
                stats.nodes_dropped += 1;
                continue;
            }
            let row_id = self.base.node_id_at(idx)?;
            entries.push(CompactNodeEntry {
                id: row_id,
                source: CompactNodeSource::Base(idx),
            });
            stats.nodes_kept += 1;
        }

        for (i, node) in self.delta.new_nodes.iter().enumerate() {
            entries.push(CompactNodeEntry {
                id: node.id,
                source: CompactNodeSource::Delta(i),
            });
            stats.nodes_from_delta += 1;
        }
        dedupe_entries_prefer_delta(&mut entries);

        let mut hasher = blake3::Hasher::new();
        let mut strings = StringPool::new();
        let mut node_rows = Vec::with_capacity(entries.len());
        let mut extensions_blob = Vec::new();
        let mut name_index = std::collections::HashMap::new();
        let mut type_index = std::collections::HashMap::new();
        let mut scratch = Vec::with_capacity(512);

        for entry in &entries {
            match entry.source {
                CompactNodeSource::Base(idx) => {
                    self.base.append_node_for_build(
                        idx,
                        &mut hasher,
                        &mut strings,
                        &mut extensions_blob,
                        &mut name_index,
                        &mut type_index,
                        &mut node_rows,
                        &mut scratch,
                    )?;
                }
                CompactNodeSource::Delta(i) => {
                    let node = &self.delta.new_nodes[i];
                    crate::columnar_snapshot::append_node_columnar(
                        node,
                        &mut hasher,
                        &mut strings,
                        &mut extensions_blob,
                        &mut name_index,
                        &mut type_index,
                        &mut node_rows,
                        &mut scratch,
                    )?;
                }
            }
        }

        // Keep base edges only when both endpoints are alive and the source (`from`)
        // is still a kept base node (its file was not invalidated). Invalidated-source
        // edges are replaced by `delta.new_edges` so omitted CALLs stay omitted (#124).
        let mut edge_meta: Vec<(Uuid, Uuid, u8)> = Vec::new();
        self.base.for_each_edge(|from, to, edge_type| {
            if node_alive(&entries, from)
                && node_alive(&entries, to)
                && source_is_base(&entries, from)
            {
                edge_meta.push((from, to, edge_type_to_u8(edge_type)));
                stats.edges_kept += 1;
            } else {
                stats.edges_dropped += 1;
            }
            Ok(())
        })?;

        for edge in &self.delta.new_edges {
            if node_alive(&entries, edge.from) && node_alive(&entries, edge.to) {
                edge_meta.push((edge.from, edge.to, edge_type_to_u8(edge.edge_type)));
                stats.edges_from_delta += 1;
            }
        }

        dedupe_sorted_edges(&mut edge_meta);

        let mut edge_rows = Vec::with_capacity(edge_meta.len());
        for (from, to, edge_type) in edge_meta {
            let canonical = Edge::new(from, to, edge_type_from_u8(edge_type)?);
            hasher.update(&crate::columnar_snapshot::edge_digest_bytes(&canonical)?);
            edge_rows.push(EdgeRow {
                from: *from.as_bytes(),
                to: *to.as_bytes(),
                edge_type,
                _pad: [0; 7],
            });
        }

        let content_digest = hasher.finalize().to_hex().to_string();
        stats.content_digest = content_digest.clone();

        let tmp_path = output_path.with_extension("bin.tmp");
        write_columnar_assembled(
            &tmp_path,
            &node_rows,
            &edge_rows,
            &strings,
            &extensions_blob,
            &name_index,
            &type_index,
            &content_digest,
        )?;

        if output_path.exists() {
            std::fs::remove_file(output_path)?;
        }
        std::fs::rename(&tmp_path, output_path)?;

        Ok(stats)
    }
}

/// Open a columnar snapshot from `path`, compact with `delta`, write atomically.
pub fn compact_snapshot_file(
    base_path: &Path,
    delta: DeltaSegment,
    output_path: &Path,
    scratch_dir: &Path,
) -> Result<CompactStats> {
    let file = File::open(base_path)?;
    // SAFETY: snapshot file is read-only for the duration of compaction; mapping covers
    // the on-disk columnar bytes only.
    let mmap = Arc::new(unsafe { Mmap::map(&file)? });
    let base = ColumnarGraphMmap::open(mmap)?;
    GraphCompactor::new(&base, delta).compact_to_path(output_path, scratch_dir)
}

/// Compact the default repo snapshot in place (via temp + rename).
pub fn compact_repo_snapshot(repo_root: &Path, delta: DeltaSegment) -> Result<CompactStats> {
    let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);
    if !snapshot_path.exists() {
        return Err(Error::NotFound(format!(
            "snapshot not found at {}",
            snapshot_path.display()
        )));
    }
    let scratch = repo_root.join(".rgctl").join("compact-scratch");
    compact_snapshot_file(&snapshot_path, delta, &snapshot_path, &scratch)
}

#[cfg(test)]
use crate::schema::NodeType;

#[cfg(test)]
fn node_matches_invalidated(node: &Node, invalidated: &HashSet<String>) -> bool {
    let path = match (&node.file_path, node.node_type == NodeType::File) {
        (Some(fp), _) => fp.as_ref(),
        (None, true) => node.name.as_str(),
        _ => return false,
    };
    node_matches_invalidated_path(Some(path), node.name.as_str(), node.node_type, invalidated)
}

#[cfg(test)]
fn node_matches_invalidated_path(
    file_path: Option<&str>,
    name: &str,
    node_type: NodeType,
    invalidated: &HashSet<String>,
) -> bool {
    let path = match (file_path, node_type == NodeType::File) {
        (Some(fp), _) => fp,
        (None, true) => name,
        _ => return false,
    };

    let norm = normalize_path_str(path);
    if invalidated.contains(norm.as_ref()) {
        return true;
    }

    norm.rsplit_once('/')
        .is_some_and(|(_, basename)| invalidated.contains(basename))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{EdgeType, NodeType};
    use crate::write_columnar_from_nodes_edges;
    use tempfile::TempDir;

    #[test]
    fn compact_drops_invalidated_file_and_appends_delta() {
        let keep = Node::new(NodeType::Function, "keep").with_file_path("a.rs");
        let drop_n = Node::new(NodeType::Function, "drop_me").with_file_path("b.rs");
        let keep_id = keep.id;
        let drop_id = drop_n.id;
        let e_keep = Edge::new(keep_id, keep_id, EdgeType::Calls);
        let e_drop = Edge::new(keep_id, drop_id, EdgeType::Calls);

        let tmp = TempDir::new().unwrap();
        let base_path = tmp.path().join("base.bin");
        write_columnar_from_nodes_edges(vec![keep, drop_n], vec![e_keep, e_drop], &base_path)
            .unwrap();

        let replacement = Node::new(NodeType::Function, "fresh").with_file_path("b.rs");
        let fresh_id = replacement.id;
        let mut delta = DeltaSegment::new();
        delta.invalidate_file("b.rs");
        delta.new_nodes.push(replacement);
        delta
            .new_edges
            .push(Edge::new(keep_id, fresh_id, EdgeType::Calls));

        let out = tmp.path().join("out.bin");
        let scratch = tmp.path().join("scratch");
        let stats = compact_snapshot_file(&base_path, delta, &out, &scratch).unwrap();

        assert_eq!(stats.nodes_kept, 1);
        assert_eq!(stats.nodes_dropped, 1);
        assert_eq!(stats.nodes_from_delta, 1);
        assert_eq!(stats.edges_kept, 1); // self-call on keep
        assert!(stats.edges_dropped >= 1);
        assert_eq!(stats.edges_from_delta, 1);

        let file = File::open(&out).unwrap();
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        assert_eq!(col.node_count(), 2);
        let names: Vec<_> = (0..col.node_count())
            .map(|i| col.materialize_node_at(i).unwrap().name.to_string())
            .collect();
        assert!(names.iter().any(|n| n == "keep"));
        assert!(names.iter().any(|n| n == "fresh"));
        assert!(!names.iter().any(|n| n == "drop_me"));

        assert_eq!(col.find_nodes_by_name("fresh").unwrap().len(), 1);
        assert!(col.find_nodes_by_name("drop_me").unwrap().is_empty());
    }

    #[test]
    fn node_matches_invalidated_basename() {
        let node = Node::new(NodeType::Function, "foo").with_file_path("src/deep/b.rs");
        let mut invalidated = HashSet::new();
        invalidated.insert("b.rs".to_string());
        assert!(node_matches_invalidated(&node, &invalidated));
    }

    #[test]
    fn cross_file_call_edge_survives_callee_file_reextract() {
        let main_fn = Node::new(NodeType::Function, "main").with_file_path("main.rs");
        let login = Node::new(NodeType::Function, "login").with_file_path("auth.rs");
        let main_id = main_fn.id;
        let login_id = login.id;
        let call = Edge::new(main_id, login_id, EdgeType::Calls);

        let tmp = TempDir::new().unwrap();
        let base_path = tmp.path().join("base.bin");
        write_columnar_from_nodes_edges(vec![main_fn, login], vec![call], &base_path).unwrap();

        let login_reextracted = Node::new(NodeType::Function, "login").with_file_path("auth.rs");
        assert_eq!(login_reextracted.id, login_id);

        let mut delta = DeltaSegment::new();
        delta.invalidate_file("auth.rs");
        delta.new_nodes.push(login_reextracted);
        delta
            .new_edges
            .push(Edge::new(main_id, login_id, EdgeType::Calls));

        let out = tmp.path().join("out.bin");
        let scratch = tmp.path().join("scratch");
        let stats = compact_snapshot_file(&base_path, delta, &out, &scratch).unwrap();
        assert_eq!(stats.edges_dropped, 0, "main→login must survive re-extract");

        let file = File::open(&out).unwrap();
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        let mut found = false;
        col.for_each_edge(|from, to, edge_type| {
            if from == main_id && to == login_id && edge_type == EdgeType::Calls {
                found = true;
            }
            Ok(())
        })
        .unwrap();
        assert!(found);
        let mut call_count = 0usize;
        col.for_each_edge(|from, to, edge_type| {
            if from == main_id && to == login_id && edge_type == EdgeType::Calls {
                call_count += 1;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(call_count, 1, "kept-base + delta must not duplicate CALL");
    }

    #[test]
    fn intra_file_call_deletion_with_stable_ids_drops_omitted_edge() {
        let ship = Node::new(NodeType::Function, "calculateShipping").with_file_path("ship.rs");
        let total = Node::new(NodeType::Function, "getCartItemTotal").with_file_path("ship.rs");
        let free = Node::new(NodeType::Function, "isFreeShipping").with_file_path("ship.rs");
        let ship_id = ship.id;
        let total_id = total.id;
        let free_id = free.id;
        let e_total = Edge::new(ship_id, total_id, EdgeType::Calls);
        let e_free = Edge::new(ship_id, free_id, EdgeType::Calls);

        let tmp = TempDir::new().unwrap();
        let base_path = tmp.path().join("base.bin");
        write_columnar_from_nodes_edges(
            vec![ship, total, free],
            vec![e_total, e_free],
            &base_path,
        )
        .unwrap();

        // Same deterministic IDs; only the total call remains in the new extract.
        let ship2 = Node::new(NodeType::Function, "calculateShipping").with_file_path("ship.rs");
        let total2 = Node::new(NodeType::Function, "getCartItemTotal").with_file_path("ship.rs");
        let free2 = Node::new(NodeType::Function, "isFreeShipping").with_file_path("ship.rs");
        assert_eq!(ship2.id, ship_id);
        assert_eq!(total2.id, total_id);
        assert_eq!(free2.id, free_id);

        let mut delta = DeltaSegment::new();
        delta.invalidate_file("ship.rs");
        delta.new_nodes.extend([ship2, total2, free2]);
        delta
            .new_edges
            .push(Edge::new(ship_id, total_id, EdgeType::Calls));

        let out = tmp.path().join("out.bin");
        let scratch = tmp.path().join("scratch");
        compact_snapshot_file(&base_path, delta, &out, &scratch).unwrap();

        let file = File::open(&out).unwrap();
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        let mut calls = Vec::new();
        col.for_each_edge(|from, to, edge_type| {
            if from == ship_id && edge_type == EdgeType::Calls {
                calls.push(to);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, vec![total_id], "deleted CALL must not resurrect via alive IDs");
    }

    #[test]
    fn surviving_intra_file_call_not_duplicated() {
        let a = Node::new(NodeType::Function, "a").with_file_path("f.rs");
        let b = Node::new(NodeType::Function, "b").with_file_path("f.rs");
        let a_id = a.id;
        let b_id = b.id;
        let call = Edge::new(a_id, b_id, EdgeType::Calls);

        let tmp = TempDir::new().unwrap();
        let base_path = tmp.path().join("base.bin");
        write_columnar_from_nodes_edges(vec![a, b], vec![call.clone()], &base_path).unwrap();

        let a2 = Node::new(NodeType::Function, "a").with_file_path("f.rs");
        let b2 = Node::new(NodeType::Function, "b").with_file_path("f.rs");
        let mut delta = DeltaSegment::new();
        delta.invalidate_file("f.rs");
        delta.new_nodes.extend([a2, b2]);
        delta.new_edges.push(call);

        let out = tmp.path().join("out.bin");
        let scratch = tmp.path().join("scratch");
        compact_snapshot_file(&base_path, delta, &out, &scratch).unwrap();

        let file = File::open(&out).unwrap();
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        let mut n = 0usize;
        col.for_each_edge(|from, to, edge_type| {
            if from == a_id && to == b_id && edge_type == EdgeType::Calls {
                n += 1;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn callee_rename_drops_stale_cross_file_edge() {
        let main_fn = Node::new(NodeType::Function, "main").with_file_path("main.rs");
        let login = Node::new(NodeType::Function, "login").with_file_path("auth.rs");
        let main_id = main_fn.id;
        let login_id = login.id;
        let call = Edge::new(main_id, login_id, EdgeType::Calls);

        let tmp = TempDir::new().unwrap();
        let base_path = tmp.path().join("base.bin");
        write_columnar_from_nodes_edges(vec![main_fn, login], vec![call], &base_path).unwrap();

        let authenticate =
            Node::new(NodeType::Function, "authenticate").with_file_path("auth.rs");
        assert_ne!(authenticate.id, login_id);

        let mut delta = DeltaSegment::new();
        delta.invalidate_file("auth.rs");
        delta.new_nodes.push(authenticate);

        let out = tmp.path().join("out.bin");
        let scratch = tmp.path().join("scratch");
        let stats = compact_snapshot_file(&base_path, delta, &out, &scratch).unwrap();
        assert!(stats.edges_dropped >= 1, "stale main→login edge must drop");

        let file = File::open(&out).unwrap();
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        let mut stale_call = false;
        col.for_each_edge(|from, to, edge_type| {
            if from == main_id && to == login_id && edge_type == EdgeType::Calls {
                stale_call = true;
            }
            Ok(())
        })
        .unwrap();
        assert!(!stale_call);
    }

    #[test]
    fn compactor_preserves_extension_digests_for_multi_property_nodes() {
        let tmp = TempDir::new().unwrap();
        let base_path = tmp.path().join("base.bin");

        let node = Node::new(NodeType::Function, "worker")
            .with_file_path("worker.rs")
            .with_property("cyclomatic".into(), "3".into())
            .with_property("cognitive".into(), "2".into())
            .with_property("loc".into(), "80".into())
            .with_property("nesting_depth".into(), "1".into());

        write_columnar_from_nodes_edges(vec![node], vec![], &base_path).unwrap();

        let file = File::open(&base_path).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let base = ColumnarGraphMmap::open(mmap).unwrap();
        let base_digest = crate::stable_key::node_row_ref(&base, 0)
            .unwrap()
            .extension_digest;

        let out = tmp.path().join("compacted.bin");
        let scratch = tmp.path().join("scratch");
        let stats = GraphCompactor::new(&base, DeltaSegment::default())
            .compact_to_path(&out, &scratch)
            .unwrap();
        assert_eq!(stats.nodes_kept, 1);
        assert_eq!(stats.nodes_dropped, 0);
        assert_eq!(stats.nodes_from_delta, 0);

        let out_file = File::open(&out).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let out_mmap = Arc::new(unsafe { Mmap::map(&out_file).unwrap() });
        let compacted = ColumnarGraphMmap::open(out_mmap).unwrap();
        let compacted_digest = crate::stable_key::node_row_ref(&compacted, 0)
            .unwrap()
            .extension_digest;
        assert_eq!(
            base_digest, compacted_digest,
            "Compacted node must preserve exact extension_digest"
        );
    }
}
