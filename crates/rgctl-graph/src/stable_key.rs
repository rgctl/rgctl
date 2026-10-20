//! Stable node identity across snapshots with different UUID assignments.

use crate::columnar_snapshot::{ColumnarGraphMmap, optional_string_at, string_at};
use crate::normalize_path_str;
use crate::schema::NodeType;
use rgctl_error::Result;
use uuid::{uuid, Uuid};

/// UUID v5 namespace for deterministic graph node IDs (`rgctl:stable-node-key`).
pub const NAMESPACE_RGCTL: Uuid = uuid!("f47ac10b-58cc-4372-a567-0e02b2c3d479");

/// Per-snapshot mmap string-pool offsets for a node row (not comparable across snapshots).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MmapNodeKey {
    /// Columnar node type discriminator.
    pub node_type: u16,
    /// Byte offset of the node name in the string pool.
    pub name_off: u32,
    /// Byte length of the node name in the string pool.
    pub name_len: u32,
    /// Byte offset of the file path in the string pool (0 when absent).
    pub file_off: u32,
    /// Byte length of the file path in the string pool.
    pub file_len: u32,
}

/// Cross-snapshot stable node identity (BLAKE3 of normalized path + name + type).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StableNodeKey(u64);

impl StableNodeKey {
    /// Raw digest bytes (first 8 bytes of BLAKE3).
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Metadata for a node row used during snapshot diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeRowRef {
    /// Row index in the columnar node table.
    pub idx: usize,
    /// Snapshot-local node UUID.
    pub id: Uuid,
    /// BLAKE3 digest of extension properties (0 when absent).
    pub extension_digest: u64,
    /// Resolved node type.
    pub node_type: NodeType,
    /// Inclusive start line in the source file (0 when unknown).
    pub start_line: u32,
    /// Inclusive end line in the source file (0 when unknown).
    pub end_line: u32,
}

/// BLAKE3 digest of extension blob (0 when absent).
pub fn extension_digest(bytes: Option<&[u8]>) -> u64 {
    match bytes {
        None | Some([]) => 0,
        Some(blob) => {
            let hash = blake3::hash(blob);
            u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("8 bytes"))
        }
    }
}

/// Derive a deterministic node [`Uuid`] from a [`StableNodeKey`].
pub fn stable_key_to_uuid(key: StableNodeKey) -> Uuid {
    Uuid::new_v5(&NAMESPACE_RGCTL, &key.as_u64().to_le_bytes())
}

/// Deterministic node id when file path is known; otherwise a fresh random v4.
pub fn deterministic_node_id(
    file_path: Option<&str>,
    name: &str,
    node_type: NodeType,
    qualified_name: Option<&str>,
) -> Uuid {
    match file_path {
        Some(path) => {
            stable_key_to_uuid(stable_key_from_facets(Some(path), name, node_type, qualified_name))
        }
        None => Uuid::new_v4(),
    }
}

/// Hash normalized file path, name, node type, and optional qualified name into a [`StableNodeKey`].
///
/// When `qualified_name` is present it is hashed (same-file same-simple-name symbols diverge).
/// When absent, an empty facet is hashed — **not** `start_line` — so a one-line insert above a
/// symbol cannot churn cross-commit identity for languages that omit QN.
pub fn stable_key_from_facets(
    file_path: Option<&str>,
    name: &str,
    node_type: NodeType,
    qualified_name: Option<&str>,
) -> StableNodeKey {
    let mut hasher = blake3::Hasher::new();
    if let Some(path) = file_path {
        hasher.update(normalize_path_str(path).as_bytes());
    }
    hasher.update(&[0xff]);
    hasher.update(name.as_bytes());
    hasher.update(&[0xff]);
    hasher.update(&(node_type as u16).to_le_bytes());
    hasher.update(&[0xff]);
    if let Some(qn) = qualified_name.filter(|s| !s.is_empty()) {
        hasher.update(qn.as_bytes());
    } else {
        hasher.update(b"");
    }
    let hash = hasher.finalize();
    StableNodeKey(u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("8 bytes")))
}

/// Stable key facets from a materialized [`Node`] (same contract as [`stable_key_from_row`]).
pub fn stable_key_from_node(node: &crate::schema::Node) -> StableNodeKey {
    stable_key_from_facets(
        node.file_path.as_deref().map(|s| s.as_ref()),
        node.name.as_ref(),
        node.node_type,
        node.qualified_name.as_deref().map(|s| s.as_ref()),
    )
}

/// Read stable key facets from a columnar node row without allocating strings.
pub fn stable_key_from_row(col: &ColumnarGraphMmap, idx: usize) -> Result<StableNodeKey> {
    let row = col.node_row_at(idx)?;
    let (str_base, str_len) = col.string_pool_bounds();
    let mmap = col.mmap_bytes();
    let name = string_at(mmap, str_base, str_len, row.name_off, row.name_len)?;
    let file_path = optional_string_at(
        mmap,
        str_base,
        str_len,
        row.file_path_off,
        row.file_path_len,
    )?;
    let node_type = crate::columnar_snapshot::node_type_from_u16(row.node_type)?;
    let qn = col.node_qualified_name_str(idx)?;
    Ok(stable_key_from_facets(
        file_path,
        name,
        node_type,
        qn,
    ))
}

/// Repo-relative path used for PR scope filtering (file path or file-node name).
pub fn node_scope_path_at(col: &ColumnarGraphMmap, idx: usize) -> Result<Option<&str>> {
    let row = col.node_row_at(idx)?;
    let (str_base, str_len) = col.string_pool_bounds();
    let mmap = col.mmap_bytes();
    let name = string_at(mmap, str_base, str_len, row.name_off, row.name_len)?;
    let file_path = optional_string_at(
        mmap,
        str_base,
        str_len,
        row.file_path_off,
        row.file_path_len,
    )?;
    let node_type = crate::columnar_snapshot::node_type_from_u16(row.node_type)?;
    Ok(file_path.or_else(|| {
        if node_type == NodeType::File {
            Some(name)
        } else {
            None
        }
    }))
}

/// Build [`NodeRowRef`] for diff classification.
pub fn node_row_ref(col: &ColumnarGraphMmap, idx: usize) -> Result<NodeRowRef> {
    let row = col.node_row_at(idx)?;
    let node_type = crate::columnar_snapshot::node_type_from_u16(row.node_type)?;
    let ext_digest = extension_digest(col.extension_bytes_at(idx)?);
    Ok(NodeRowRef {
        idx,
        id: Uuid::from_bytes(row.id),
        extension_digest: ext_digest,
        node_type,
        start_line: row.start_line,
        end_line: row.end_line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Node, NodeType};
    use crate::write_columnar_from_nodes_edges;
    use memmap2::Mmap;
    use std::fs::File;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn open_columnar(nodes: Vec<Node>) -> (TempDir, ColumnarGraphMmap) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("snap.bin");
        write_columnar_from_nodes_edges(nodes, vec![], &path).unwrap();
        let file = File::open(&path).unwrap();
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        (tmp, ColumnarGraphMmap::open(mmap).unwrap())
    }

    #[test]
    fn stable_key_equal_across_different_uuids() {
        let a = Node::new(NodeType::Function, "foo").with_file_path("src/a.rs");
        let b = Node::new(NodeType::Function, "foo").with_file_path("src/a.rs");
        assert_eq!(a.id, b.id);
        let expected_id = a.id;

        let (_tmp_a, col_a) = open_columnar(vec![a]);
        let (_tmp_b, col_b) = open_columnar(vec![b]);

        let key_a = stable_key_from_row(&col_a, 0).unwrap();
        let key_b = stable_key_from_row(&col_b, 0).unwrap();
        assert_eq!(key_a, key_b);
        assert_eq!(expected_id, stable_key_to_uuid(key_a));
    }

    #[test]
    fn deterministic_node_id_without_file_path_is_random() {
        let a = Node::new(NodeType::Variable, "orphan_env");
        let b = Node::new(NodeType::Variable, "orphan_env");
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn deterministic_node_id_changes_on_rename() {
        let login = Node::new(NodeType::Function, "login").with_file_path("auth.rs");
        let auth = Node::new(NodeType::Function, "authenticate").with_file_path("auth.rs");
        assert_ne!(login.id, auth.id);
    }

    #[test]
    fn deterministic_node_id_disambiguates_by_qualified_name() {
        let a = Node::new(NodeType::Function, "$lambda$0")
            .with_file_path("AuthService.java")
            .with_qualified_name("AuthService.login.$lambda$0")
            .with_location(63, 63);
        let b = Node::new(NodeType::Function, "$lambda$0")
            .with_file_path("AuthService.java")
            .with_qualified_name("AuthService.currentUser.$lambda$0")
            .with_location(77, 77);
        assert_ne!(a.id, b.id, "per-owner lambdas must not share a UUID");
    }

    #[test]
    fn start_line_shift_without_qn_keeps_stable_key() {
        // Cross-commit identity must not churn when a line is inserted above a
        // symbol that has no qualified_name (TS/JS/Python/Go/C++ common case).
        let a = Node::new(NodeType::Function, "dup")
            .with_file_path("f.ts")
            .with_location(10, 12);
        let b = Node::new(NodeType::Function, "dup")
            .with_file_path("f.ts")
            .with_location(40, 42);
        assert_eq!(a.id, b.id);
        assert_eq!(
            stable_key_from_facets(Some("f.ts"), "dup", NodeType::Function, None),
            stable_key_from_facets(Some("f.ts"), "dup", NodeType::Function, None),
        );
    }

    #[test]
    fn stable_key_from_row_reads_qn_without_materialize() {
        let node = Node::new(NodeType::Function, "$lambda$0")
            .with_file_path("AuthService.java")
            .with_qualified_name("AuthService.login.$lambda$0");
        let expected = stable_key_from_facets(
            Some("AuthService.java"),
            "$lambda$0",
            NodeType::Function,
            Some("AuthService.login.$lambda$0"),
        );
        let (_tmp, col) = open_columnar(vec![node]);
        assert_eq!(col.node_qualified_name_str(0).unwrap(), Some("AuthService.login.$lambda$0"));
        assert_eq!(stable_key_from_row(&col, 0).unwrap(), expected);
    }

    #[test]
    fn stable_key_differs_for_distinct_nodes() {
        let a = Node::new(NodeType::Function, "foo").with_file_path("src/a.rs");
        let b = Node::new(NodeType::Function, "bar").with_file_path("src/a.rs");
        let (_tmp_a, col_a) = open_columnar(vec![a]);
        let (_tmp_b, col_b) = open_columnar(vec![b]);
        assert_ne!(
            stable_key_from_row(&col_a, 0).unwrap(),
            stable_key_from_row(&col_b, 0).unwrap()
        );
    }

    #[test]
    fn extension_digest_changes_when_extension_differs() {
        let plain = Node::new(NodeType::Function, "fn").with_file_path("x.rs");
        let with_props = Node::new(NodeType::Function, "fn")
            .with_file_path("x.rs")
            .with_property("k".into(), "v".into());

        let (_tmp_plain, col_plain) = open_columnar(vec![plain]);
        let (_tmp_props, col_props) = open_columnar(vec![with_props]);

        let d0 = node_row_ref(&col_plain, 0).unwrap().extension_digest;
        let d1 = node_row_ref(&col_props, 0).unwrap().extension_digest;
        assert_ne!(d0, d1);
        assert_eq!(
            stable_key_from_row(&col_plain, 0).unwrap(),
            stable_key_from_row(&col_props, 0).unwrap()
        );
    }

    #[test]
    fn extension_digest_is_stable_for_identical_properties() {
        let mk = || {
            Node::new(NodeType::Function, "fn")
                .with_file_path("x.rs")
                .with_property("cyclomatic".into(), "1".into())
                .with_property("cognitive".into(), "2".into())
                .with_property("loc".into(), "3".into())
                .with_property("nesting_depth".into(), "4".into())
        };

        let (_t0, c0) = open_columnar(vec![mk()]);
        let baseline = node_row_ref(&c0, 0).unwrap().extension_digest;

        for i in 0..50 {
            let (_t, c) = open_columnar(vec![mk()]);
            let d = node_row_ref(&c, 0).unwrap().extension_digest;
            assert_eq!(
                d, baseline,
                "digest differs on iteration {i} for identical input"
            );
        }
    }
}
