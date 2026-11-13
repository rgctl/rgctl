//! Read-only graph views for dashboard / migration export without `MemoryBackend` hydration.
//!
//! [`GraphSource`] is the Decision-2 `DashboardGraphView` surface: zero-copy hot node/edge
//! fields from [`MmappedGraphSnapshot`] / [`ColumnarGraphMmap`], plus an in-memory path for
//! unit tests that pass synthetic [`MemoryBackend`] graphs.

use crate::backend::MemoryBackend;
use crate::columnar_snapshot::ColumnarGraphMmap;
use crate::schema::{EdgeType, Node, NodeType};
use crate::snapshot::MmappedGraphSnapshot;
use rgctl_error::Result;
use std::collections::HashMap;
use uuid::Uuid;

/// Borrowed hot-column node fields (no cold `NodeExtension` / properties).
#[derive(Debug, Clone, Copy)]
pub struct HotNodeRef<'a> {
    /// Columnar row index (`0..node_count`), or a dense walk index for in-memory graphs.
    pub idx: u32,
    /// Stable node id.
    pub id: Uuid,
    /// Node kind.
    pub node_type: NodeType,
    /// Display name from the string pool / in-memory node.
    pub name: &'a str,
    /// File path, or `""` when absent.
    pub file_path: &'a str,
    /// String-pool offset for `file_path` (0 for in-memory graphs).
    pub file_path_off: u32,
    /// Start line (0 when unset).
    pub line_number: usize,
    /// End line when present.
    pub end_line: Option<usize>,
    /// Reserved: community ids live on analysis results, not the columnar row.
    pub community_id: Option<u32>,
    /// Cold `kind` property when available (in-memory graphs only).
    pub module_kind: Option<&'a str>,
    /// Cold `cyclomatic` property when available (in-memory graphs only).
    pub cyclomatic: Option<f64>,
}

impl HotNodeRef<'_> {
    /// True when this node has a non-empty file path.
    pub fn has_file_path(&self) -> bool {
        !self.file_path.is_empty()
    }
}

/// Hot edge endpoints (UUIDs always; indices when resolvable).
#[derive(Debug, Clone, Copy)]
pub struct HotEdgeRef {
    /// Columnar / walk index of the source node (`u32::MAX` if unknown).
    pub from_idx: u32,
    /// Columnar / walk index of the target node (`u32::MAX` if unknown).
    pub to_idx: u32,
    /// Source node id.
    pub from_id: Uuid,
    /// Target node id.
    pub to_id: Uuid,
    /// Edge kind.
    pub edge_type: EdgeType,
}

/// Read-only graph source for dashboard and migration export.
///
/// Prefer [`GraphSource::Mmap`] after discover writes `graph.snapshot.bin` so exporters never
/// deserialize cold `NodeExtension` blobs into a [`MemoryBackend`]. Unit tests keep using
/// [`GraphSource::Memory`].
#[derive(Clone, Copy)]
pub enum GraphSource<'a> {
    /// In-memory backend (tests / callers that already hold a hydrated graph).
    Memory(&'a MemoryBackend),
    /// Zero-copy mmap snapshot (columnar v2 preferred; legacy v1 still works).
    Mmap(&'a MmappedGraphSnapshot),
}

impl<'a> From<&'a MemoryBackend> for GraphSource<'a> {
    fn from(backend: &'a MemoryBackend) -> Self {
        Self::Memory(backend)
    }
}

impl<'a> From<&'a MmappedGraphSnapshot> for GraphSource<'a> {
    fn from(snapshot: &'a MmappedGraphSnapshot) -> Self {
        Self::Mmap(snapshot)
    }
}

impl<'a> GraphSource<'a> {
    /// Wrap an in-memory backend.
    pub fn memory(backend: &'a MemoryBackend) -> Self {
        Self::Memory(backend)
    }

    /// Wrap an mmap snapshot.
    pub fn mmap(snapshot: &'a MmappedGraphSnapshot) -> Self {
        Self::Mmap(snapshot)
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        match self {
            Self::Memory(b) => b.node_count(),
            Self::Mmap(s) => s.node_count(),
        }
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        match self {
            Self::Memory(b) => b.edge_count(),
            Self::Mmap(s) => s.edge_count(),
        }
    }

    /// Snapshot content digest when backed by a file snapshot (hex BLAKE3).
    pub fn snapshot_content_digest(&self) -> Option<&'a str> {
        match self {
            Self::Memory(_) => None,
            Self::Mmap(s) => s.content_digest().ok(),
        }
    }

    /// Digested bytes when the hex digest parses as 32 raw BLAKE3 bytes.
    pub fn snapshot_content_digest_bytes(&self) -> Option<[u8; 32]> {
        let hex = self.snapshot_content_digest()?;
        if hex.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let hi = hex_nibble(chunk[0])?;
            let lo = hex_nibble(chunk[1])?;
            out[i] = (hi << 4) | lo;
        }
        Some(out)
    }

    /// Resolve a node UUID to its columnar / walk index.
    pub fn node_idx_by_id(&self, id: Uuid) -> Option<u32> {
        match self {
            Self::Memory(b) => {
                let mut idx = 0u32;
                let mut found = None;
                let _ = b.for_each_node(|n| {
                    if n.id == id {
                        found = Some(idx);
                    }
                    idx = idx.saturating_add(1);
                });
                found
            }
            Self::Mmap(s) => {
                if let Some(col) = s.columnar() {
                    return col.find_node_index(id).map(|i| i as u32);
                }
                // Legacy: linear scan of prepared nodes.
                let prepared = s.prepared().ok()?;
                prepared
                    .nodes
                    .iter()
                    .position(|n| n.id == id)
                    .map(|i| i as u32)
            }
        }
    }

    /// Iterate hot node fields without deserializing cold extensions.
    pub fn for_each_hot_node<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(HotNodeRef<'_>),
    {
        match self {
            Self::Memory(b) => {
                let mut idx = 0u32;
                b.for_each_node(|n| {
                    f(hot_node_from_owned(idx, n));
                    idx = idx.saturating_add(1);
                })
            }
            Self::Mmap(s) => s.for_each_hot_node(f),
        }
    }

    /// Iterate hot edge fields (UUID endpoints always; indices when resolvable).
    pub fn for_each_hot_edge<F>(&self, f: F) -> Result<()>
    where
        F: FnMut(HotEdgeRef),
    {
        self.for_each_hot_edge_with_index(None, f)
    }

    /// Iterate hot edge fields, reusing a pre-built `Uuid → u32` index map when available
    /// so callers do not rebuild a `HashMap<Uuid, u32>` on every edge scan.
    pub fn for_each_hot_edge_with_index<F>(
        &self,
        uuid_to_idx: Option<&HashMap<Uuid, u32>>,
        mut f: F,
    ) -> Result<()>
    where
        F: FnMut(HotEdgeRef),
    {
        match self {
            Self::Memory(b) => {
                let owned_map;
                let id_to_idx = match uuid_to_idx.filter(|m| !m.is_empty()) {
                    Some(m) => m,
                    None => {
                        let mut map = HashMap::with_capacity(b.node_count());
                        let mut idx = 0u32;
                        b.for_each_node(|n| {
                            map.insert(n.id, idx);
                            idx = idx.saturating_add(1);
                        })?;
                        owned_map = map;
                        &owned_map
                    }
                };
                b.for_each_edge(|e| {
                    f(HotEdgeRef {
                        from_idx: id_to_idx.get(&e.from).copied().unwrap_or(u32::MAX),
                        to_idx: id_to_idx.get(&e.to).copied().unwrap_or(u32::MAX),
                        from_id: e.from,
                        to_id: e.to,
                        edge_type: e.edge_type,
                    });
                })
            }
            Self::Mmap(s) => s.for_each_hot_edge_with_index(uuid_to_idx, f),
        }
    }

    /// Iterate edge types without resolving endpoint indices or building a `Uuid → u32` map.
    pub fn for_each_edge_type<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(EdgeType),
    {
        match self {
            Self::Memory(b) => b.for_each_edge(|e| f(e.edge_type)),
            Self::Mmap(s) => s.for_each_edge(|_, _, et| {
                f(et);
                Ok(())
            }),
        }
    }

    /// Borrow the in-memory backend when this is [`GraphSource::Memory`].
    pub fn as_memory(&self) -> Option<&'a MemoryBackend> {
        match self {
            Self::Memory(b) => Some(*b),
            Self::Mmap(_) => None,
        }
    }

    /// Borrow the mmap snapshot when this is [`GraphSource::Mmap`].
    pub fn as_mmap(&self) -> Option<&'a MmappedGraphSnapshot> {
        match self {
            Self::Memory(_) => None,
            Self::Mmap(s) => Some(*s),
        }
    }
}

/// Trait surface matching OpenSpec Decision 2 (`DashboardGraphView`).
///
/// Prefer calling methods on [`GraphSource`] directly; this trait exists so callers can
/// accept `impl DashboardGraphView` in generic APIs.
pub trait DashboardGraphView {
    /// Number of nodes.
    fn node_count(&self) -> usize;
    /// Number of edges.
    fn edge_count(&self) -> usize;
    /// Snapshot content digest (hex) when available.
    fn snapshot_content_digest(&self) -> Option<&str>;
    /// Resolve UUID → dense/columnar index.
    fn node_idx_by_id(&self, id: Uuid) -> Option<u32>;
    /// Hot node walk.
    fn for_each_hot_node(&self, f: &mut dyn FnMut(HotNodeRef<'_>)) -> Result<()>;
    /// Hot edge walk.
    fn for_each_hot_edge(&self, f: &mut dyn FnMut(HotEdgeRef)) -> Result<()>;
}

impl DashboardGraphView for GraphSource<'_> {
    fn node_count(&self) -> usize {
        GraphSource::node_count(self)
    }

    fn edge_count(&self) -> usize {
        GraphSource::edge_count(self)
    }

    fn snapshot_content_digest(&self) -> Option<&str> {
        GraphSource::snapshot_content_digest(self)
    }

    fn node_idx_by_id(&self, id: Uuid) -> Option<u32> {
        GraphSource::node_idx_by_id(self, id)
    }

    fn for_each_hot_node(&self, f: &mut dyn FnMut(HotNodeRef<'_>)) -> Result<()> {
        GraphSource::for_each_hot_node(self, f)
    }

    fn for_each_hot_edge(&self, f: &mut dyn FnMut(HotEdgeRef)) -> Result<()> {
        GraphSource::for_each_hot_edge(self, f)
    }
}

fn hot_node_from_owned(idx: u32, n: &Node) -> HotNodeRef<'_> {
    HotNodeRef {
        idx,
        id: n.id,
        node_type: n.node_type,
        name: n.name.as_str(),
        file_path: n.file_path.as_deref().unwrap_or(""),
        file_path_off: 0,
        line_number: n.start_line.unwrap_or(0),
        end_line: n.end_line,
        community_id: None,
        module_kind: n.properties.get("kind").map(|s| s.as_str()),
        cyclomatic: n
            .properties
            .get("cyclomatic")
            .and_then(|v| v.parse::<f64>().ok()),
    }
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

impl ColumnarGraphMmap {
    /// Stream hot node columns without decoding cold extensions.
    pub fn for_each_hot_node<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(HotNodeRef<'_>),
    {
        let (pool_base, pool_len) = self.string_pool_bounds();
        let mmap = self.mmap_bytes();
        for idx in 0..self.node_count() {
            let row = self.node_row_at(idx)?;
            let name = crate::columnar_snapshot::string_at(
                mmap,
                pool_base,
                pool_len,
                row.name_off,
                row.name_len,
            )?;
            let file_path = crate::columnar_snapshot::optional_string_at(
                mmap,
                pool_base,
                pool_len,
                row.file_path_off,
                row.file_path_len,
            )?
            .unwrap_or("");
            f(HotNodeRef {
                idx: idx as u32,
                id: Uuid::from_bytes(row.id),
                node_type: crate::columnar_snapshot::node_type_from_u16(row.node_type)?,
                name,
                file_path,
                file_path_off: row.file_path_off,
                line_number: row.start_line as usize,
                end_line: (row.end_line > 0).then_some(row.end_line as usize),
                community_id: None,
                module_kind: None,
                cyclomatic: None,
            });
        }
        Ok(())
    }

    /// Stream edges with UUID endpoints and dense columnar indices.
    ///
    /// Builds a one-shot `Uuid → idx` map (O(N)) so each edge resolves `from_idx` /
    /// `to_idx` in O(1) instead of O(log N) binary search per endpoint.
    pub fn for_each_hot_edge<F>(&self, f: F) -> Result<()>
    where
        F: FnMut(HotEdgeRef),
    {
        self.for_each_hot_edge_with_index(None, f)
    }

    /// Stream edges with UUID endpoints and dense columnar indices, reusing a pre-built
    /// `Uuid → idx` map when provided.
    pub fn for_each_hot_edge_with_index<F>(
        &self,
        uuid_to_idx: Option<&HashMap<Uuid, u32>>,
        mut f: F,
    ) -> Result<()>
    where
        F: FnMut(HotEdgeRef),
    {
        let owned_map;
        let id_to_idx = match uuid_to_idx.filter(|m| !m.is_empty()) {
            Some(m) => m,
            None => {
                let mut map = HashMap::with_capacity(self.node_count());
                for idx in 0..self.node_count() {
                    if let Ok(id) = self.node_id_at(idx) {
                        map.insert(id, idx as u32);
                    }
                }
                owned_map = map;
                &owned_map
            }
        };
        self.for_each_edge(|from_id, to_id, edge_type| {
            f(HotEdgeRef {
                from_idx: id_to_idx.get(&from_id).copied().unwrap_or(u32::MAX),
                to_idx: id_to_idx.get(&to_id).copied().unwrap_or(u32::MAX),
                from_id,
                to_id,
                edge_type,
            });
            Ok(())
        })
    }
}

impl MmappedGraphSnapshot {
    /// Stream hot node fields from columnar columns or legacy prepared nodes.
    pub fn for_each_hot_node<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(HotNodeRef<'_>),
    {
        if let Some(col) = self.columnar() {
            return col.for_each_hot_node(f);
        }
        let prepared = self.prepared()?;
        for (idx, n) in prepared.nodes.iter().enumerate() {
            f(hot_node_from_owned(idx as u32, n));
        }
        Ok(())
    }

    /// Stream hot edges from columnar columns or legacy prepared edges.
    pub fn for_each_hot_edge<F>(&self, f: F) -> Result<()>
    where
        F: FnMut(HotEdgeRef),
    {
        self.for_each_hot_edge_with_index(None, f)
    }

    /// Stream hot edges from columnar columns or legacy prepared edges, reusing a pre-built
    /// `Uuid → idx` map when provided.
    pub fn for_each_hot_edge_with_index<F>(
        &self,
        uuid_to_idx: Option<&HashMap<Uuid, u32>>,
        mut f: F,
    ) -> Result<()>
    where
        F: FnMut(HotEdgeRef),
    {
        if let Some(col) = self.columnar() {
            return col.for_each_hot_edge_with_index(uuid_to_idx, f);
        }
        let prepared = self.prepared()?;
        let owned_map;
        let id_to_idx = match uuid_to_idx.filter(|m| !m.is_empty()) {
            Some(m) => m,
            None => {
                owned_map = prepared
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.id, i as u32))
                    .collect();
                &owned_map
            }
        };
        for e in &prepared.edges {
            f(HotEdgeRef {
                from_idx: id_to_idx.get(&e.from).copied().unwrap_or(u32::MAX),
                to_idx: id_to_idx.get(&e.to).copied().unwrap_or(u32::MAX),
                from_id: e.from,
                to_id: e.to,
                edge_type: e.edge_type,
            });
        }
        Ok(())
    }
}

/// Re-export helper used by [`ColumnarGraphMmap::for_each_hot_node`] tests.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::GraphBackend;
    use crate::schema::{Edge, Node};
    use tempfile::TempDir;

    #[test]
    fn memory_graph_source_iterates_hot_fields() {
        let mut backend = MemoryBackend::new();
        let mut n = Node::new(NodeType::Function, "main");
        n.file_path = Some("src/main.rs".into());
        n.start_line = Some(10);
        n.end_line = Some(20);
        let id = n.id;
        backend.insert_node(n).unwrap();
        backend
            .insert_edge(Edge::new(id, id, EdgeType::Calls))
            .unwrap();

        let src = GraphSource::memory(&backend);
        assert_eq!(src.node_count(), 1);
        assert_eq!(src.edge_count(), 1);
        assert!(src.snapshot_content_digest().is_none());

        let mut seen = 0;
        src.for_each_hot_node(|n| {
            assert_eq!(n.name, "main");
            assert_eq!(n.file_path, "src/main.rs");
            assert_eq!(n.line_number, 10);
            assert_eq!(n.end_line, Some(20));
            seen += 1;
        })
        .unwrap();
        assert_eq!(seen, 1);

        let mut edges = 0;
        src.for_each_hot_edge(|e| {
            assert_eq!(e.from_id, id);
            assert_eq!(e.from_idx, 0);
            edges += 1;
        })
        .unwrap();
        assert_eq!(edges, 1);
    }

    #[test]
    fn mmap_graph_source_exposes_digest_and_hot_iter() {
        let mut backend = MemoryBackend::new();
        let n = Node::new(NodeType::Function, "f");
        let id = n.id;
        backend.insert_node(n).unwrap();
        backend
            .insert_edge(Edge::new(id, id, EdgeType::Calls))
            .unwrap();
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("graph.snapshot.bin");
        let digest =
            crate::columnar_snapshot::write_columnar_from_backend(&backend, &path).unwrap();

        let snap = MmappedGraphSnapshot::open(&path).unwrap();
        let src = GraphSource::mmap(&snap);
        assert_eq!(src.snapshot_content_digest(), Some(digest.as_str()));
        assert_eq!(src.node_count(), 1);
        let mut names = Vec::new();
        src.for_each_hot_node(|n| names.push(n.name.to_string()))
            .unwrap();
        assert_eq!(names, vec!["f"]);
        assert!(src.node_idx_by_id(Uuid::nil()).is_none());

        let mut edges = 0;
        src.for_each_hot_edge(|e| {
            assert_ne!(e.from_idx, u32::MAX, "mmap edges must emit dense from_idx");
            assert_ne!(e.to_idx, u32::MAX, "mmap edges must emit dense to_idx");
            assert_eq!(e.from_idx, 0);
            assert_eq!(e.to_idx, 0);
            edges += 1;
        })
        .unwrap();
        assert_eq!(edges, 1);
    }

    #[test]
    fn unknown_hex_rejected() {
        assert!(hex_nibble(b'g').is_none());
    }
}
