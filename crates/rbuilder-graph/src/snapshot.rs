//! Memory-mappable binary graph snapshot format.
//!
//! v1: bincode blob (legacy). v2: columnar mmap — see [`columnar_snapshot`].
//!
//! **Complexity:** columnar open is O(header + index sections); legacy v1 deserialize is O(N+E).

use crate::backend::MemoryBackend;
use crate::columnar_snapshot::ColumnarGraphMmap;
use crate::schema::{Edge, EdgeType, Node, NodeType, GRAPH_SCHEMA_VERSION};
use memmap2::Mmap;
use rbuilder_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// Magic bytes for graph snapshot files (`RBGR`).
pub const SNAPSHOT_MAGIC: [u8; 4] = *b"RBGR";
/// Legacy bincode snapshot format version.
pub const SNAPSHOT_VERSION_V1: u32 = 1;

/// Default snapshot filename under `.rbuilder/`.
pub const SNAPSHOT_FILE: &str = "graph.snapshot.bin";

/// Pre-built indexes bundled with the graph for fast hydration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedIndexes {
    /// Name → node ids
    pub name_index: HashMap<String, Vec<Uuid>>,
    /// Node type → node ids
    pub type_index: HashMap<NodeType, Vec<Uuid>>,
}

/// On-disk graph bundle written at discover time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedGraphSnapshot {
    /// Graph schema version
    pub schema_version: u32,
    /// All nodes
    pub nodes: Vec<Node>,
    /// All edges
    pub edges: Vec<Edge>,
    /// Secondary indexes captured at write time
    pub indexes: PreparedIndexes,
    /// BLAKE3 digest of nodes+edges payload for cache invalidation
    pub content_digest: String,
}

impl PreparedGraphSnapshot {
    /// Build a prepared snapshot from an in-memory backend.
    pub fn from_backend(backend: &MemoryBackend) -> Result<Self> {
        let nodes = backend.all_nodes()?;
        let edges = backend.all_edges()?;
        let mut name_index: HashMap<String, Vec<Uuid>> = HashMap::new();
        let mut type_index: HashMap<NodeType, Vec<Uuid>> = HashMap::new();

        for node in &nodes {
            name_index
                .entry(node.name.clone())
                .or_default()
                .push(node.id);
            type_index.entry(node.node_type).or_default().push(node.id);
        }

        let mut hasher = blake3::Hasher::new();
        hasher.update(&bincode::serialize(&nodes).map_err(serde_err)?);
        hasher.update(&bincode::serialize(&edges).map_err(serde_err)?);
        let content_digest = hasher.finalize().to_hex().to_string();

        Ok(Self {
            schema_version: GRAPH_SCHEMA_VERSION,
            nodes,
            edges,
            indexes: PreparedIndexes {
                name_index,
                type_index,
            },
            content_digest,
        })
    }

    /// Write columnar v2 snapshot to disk (default).
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        self.write_columnar_to_path(path)
    }

    /// Hydrate a [`MemoryBackend`] using pre-built indexes (no index rebuild scan).
    pub fn hydrate_backend(&self) -> Result<MemoryBackend> {
        MemoryBackend::from_prepared_snapshot(self)
    }
}

enum SnapshotBacking {
    Legacy(PreparedGraphSnapshot),
    Columnar(ColumnarGraphMmap),
}

/// Memory-mapped graph snapshot for zero-copy open.
pub struct MmappedGraphSnapshot {
    _mmap: Arc<Mmap>,
    backing: SnapshotBacking,
}

impl MmappedGraphSnapshot {
    /// Default path under a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root
            .join(crate::code_graph::GRAPH_DIR)
            .join(SNAPSHOT_FILE)
    }

    /// Open and parse a snapshot file via mmap (v1 bincode or v2 columnar).
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        // SAFETY: `file` is opened read-only; the mapping is valid for the file's lifetime and
        // snapshot bytes are treated as immutable (no concurrent writes while mapped).
        let mmap = Arc::new(unsafe { Mmap::map(&file)? });
        if mmap.len() < 8 {
            return Err(Error::SerdeError("graph snapshot truncated".into()));
        }
        if mmap[0..4] != SNAPSHOT_MAGIC {
            return Err(Error::SerdeError("invalid graph snapshot magic".into()));
        }
        let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
        let backing = match version {
            SNAPSHOT_VERSION_V1 => SnapshotBacking::Legacy(parse_v1_payload(&mmap)?),
            v if v == crate::columnar_snapshot::COLUMNAR_SNAPSHOT_VERSION => {
                SnapshotBacking::Columnar(ColumnarGraphMmap::open(Arc::clone(&mmap))?)
            }
            other => {
                return Err(Error::SerdeError(format!(
                    "unsupported graph snapshot version {other}"
                )));
            }
        };
        Ok(Self {
            _mmap: mmap,
            backing,
        })
    }

    /// True when opened as columnar v2 (no full-graph bincode deserialize).
    pub fn is_columnar(&self) -> bool {
        matches!(self.backing, SnapshotBacking::Columnar(_))
    }

    /// Access legacy prepared snapshot (materializes from columnar when needed).
    pub fn prepared(&self) -> Result<PreparedGraphSnapshot> {
        match &self.backing {
            SnapshotBacking::Legacy(p) => Ok(p.clone()),
            SnapshotBacking::Columnar(c) => c.to_prepared(),
        }
    }

    /// Content digest for cache invalidation.
    pub fn content_digest(&self) -> Result<&str> {
        match &self.backing {
            SnapshotBacking::Legacy(p) => Ok(&p.content_digest),
            SnapshotBacking::Columnar(c) => Ok(c.content_digest()),
        }
    }

    /// Number of nodes in the snapshot.
    pub fn node_count(&self) -> usize {
        match &self.backing {
            SnapshotBacking::Legacy(p) => p.nodes.len(),
            SnapshotBacking::Columnar(c) => c.node_count(),
        }
    }

    /// Number of edges in the snapshot.
    pub fn edge_count(&self) -> usize {
        match &self.backing {
            SnapshotBacking::Legacy(p) => p.edges.len(),
            SnapshotBacking::Columnar(c) => c.edge_count(),
        }
    }

    /// Typed edge list for graph projections (columnar reads mmap columns directly).
    pub fn edge_topology_typed(&self) -> Result<Vec<(Uuid, Uuid, EdgeType)>> {
        match &self.backing {
            SnapshotBacking::Legacy(p) => Ok(p
                .edges
                .iter()
                .map(|e| (e.from, e.to, e.edge_type))
                .collect()),
            SnapshotBacking::Columnar(c) => c.edge_topology_typed(),
        }
    }

    /// Columnar view when available.
    pub fn columnar(&self) -> Option<&ColumnarGraphMmap> {
        match &self.backing {
            SnapshotBacking::Columnar(c) => Some(c),
            SnapshotBacking::Legacy(_) => None,
        }
    }

    /// Hydrate into an in-memory backend when mutation or legacy APIs are needed.
    pub fn hydrate_backend(&self) -> Result<MemoryBackend> {
        self.prepared()?.hydrate_backend()
    }
}

/// Read-only node access from a memory-mapped graph snapshot (no backend hydration).
pub struct SnapshotNodeStore {
    snapshot: MmappedGraphSnapshot,
    id_to_index: HashMap<Uuid, usize>,
}

impl SnapshotNodeStore {
    /// Open the default snapshot path under a repository root.
    pub fn open_from_repo(repo_root: &Path) -> Result<Self> {
        Self::open(&MmappedGraphSnapshot::default_path(repo_root))
    }

    /// Open a snapshot file and build UUID indexes for O(1) node lookup.
    pub fn open(path: &Path) -> Result<Self> {
        let snapshot = MmappedGraphSnapshot::open(path)?;
        let mut id_to_index = HashMap::with_capacity(snapshot.node_count());
        if let Some(col) = snapshot.columnar() {
            for (idx, id) in col.node_ids_by_index() {
                id_to_index.insert(id, idx);
            }
        } else {
            let prepared = snapshot.prepared()?;
            for (idx, node) in prepared.nodes.iter().enumerate() {
                id_to_index.insert(node.id, idx);
            }
        }
        Ok(Self {
            snapshot,
            id_to_index,
        })
    }

    /// Underlying mmap snapshot.
    pub fn mmap(&self) -> &MmappedGraphSnapshot {
        &self.snapshot
    }

    /// Legacy prepared payload (explicit materialize for columnar v2).
    pub fn prepared(&self) -> Result<PreparedGraphSnapshot> {
        self.snapshot.prepared()
    }

    /// Content digest for cache invalidation.
    pub fn content_digest(&self) -> Result<&str> {
        self.snapshot.content_digest()
    }

    /// Whether the backing file uses columnar v2 layout.
    pub fn is_columnar(&self) -> bool {
        self.snapshot.is_columnar()
    }

    /// Number of nodes in the snapshot.
    pub fn node_count(&self) -> usize {
        self.snapshot.node_count()
    }

    /// Number of edges in the snapshot.
    pub fn edge_count(&self) -> usize {
        self.snapshot.edge_count()
    }

    /// All node UUIDs indexed by this store (no full node materialize).
    pub fn all_node_ids(&self) -> Vec<Uuid> {
        self.id_to_index.keys().copied().collect()
    }

    /// Typed edge topology without hydrating a backend.
    pub fn edge_topology_typed(&self) -> Result<Vec<(Uuid, Uuid, EdgeType)>> {
        self.snapshot.edge_topology_typed()
    }

    /// Lookup a node by UUID without hydrating [`MemoryBackend`].
    pub fn get_node(&self, id: Uuid) -> Result<Option<Node>> {
        if let Some(col) = self.snapshot.columnar() {
            return col.get_node(id);
        }
        let prepared = self.snapshot.prepared()?;
        Ok(self
            .id_to_index
            .get(&id)
            .map(|&idx| prepared.nodes[idx].clone()))
    }

    /// Find nodes by bare name using the pre-built name index.
    pub fn find_nodes_by_name(&self, name: &str) -> Result<Vec<Node>> {
        if let Some(col) = self.snapshot.columnar() {
            return col.find_nodes_by_name(name);
        }
        let prepared = self.snapshot.prepared()?;
        Ok(prepared
            .indexes
            .name_index
            .get(name)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| {
                        self.id_to_index
                            .get(id)
                            .map(|&idx| prepared.nodes[idx].clone())
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Filter impact-zone UUIDs to function nodes only.
    pub fn filter_function_impact(&self, impact_zone_ids: &[Uuid]) -> Result<Vec<Uuid>> {
        let mut out = Vec::new();
        for id in impact_zone_ids {
            if let Some(node) = self.get_node(*id)? {
                if node.node_type == NodeType::Function {
                    out.push(*id);
                }
            }
        }
        Ok(out)
    }
}

fn parse_v1_payload(mmap: &[u8]) -> Result<PreparedGraphSnapshot> {
    if mmap.len() < 16 {
        return Err(Error::SerdeError("graph snapshot truncated".into()));
    }
    let payload_len = u64::from_le_bytes(mmap[8..16].try_into().unwrap()) as usize;
    if mmap.len() < 16 + payload_len {
        return Err(Error::SerdeError("graph snapshot payload truncated".into()));
    }
    bincode::deserialize(&mmap[16..16 + payload_len]).map_err(serde_err)
}

fn serde_err(e: bincode::Error) -> Error {
    Error::SerdeError(format!("graph snapshot: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::GraphBackend;
    use crate::schema::{EdgeType, NodeType};
    use tempfile::TempDir;

    #[test]
    fn snapshot_round_trip_columnar_v2() {
        let mut backend = MemoryBackend::new();
        let n = Node::new(NodeType::Function, "main".into());
        let id = n.id;
        backend.insert_node(n).unwrap();
        backend
            .insert_edge(Edge::new(id, id, EdgeType::Calls))
            .unwrap();

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("graph.snapshot.bin");
        let snap = PreparedGraphSnapshot::from_backend(&backend).unwrap();
        snap.write_to_path(&path).unwrap();

        let mmap = MmappedGraphSnapshot::open(&path).unwrap();
        assert!(mmap.is_columnar());
        assert_eq!(mmap.node_count(), 1);
        assert_eq!(mmap.edge_count(), 1);

        let loaded = mmap.hydrate_backend().unwrap();
        assert_eq!(loaded.node_count(), 1);
        assert_eq!(loaded.find_nodes_by_name("main").unwrap().len(), 1);
    }

    #[test]
    fn hydrate_uses_prepared_indexes_without_rescan() {
        let mut backend = MemoryBackend::new();
        for name in ["alpha", "beta"] {
            let n = Node::new(NodeType::Function, name.into());
            backend.insert_node(n).unwrap();
        }
        let prepared = PreparedGraphSnapshot::from_backend(&backend).unwrap();
        let loaded = prepared.hydrate_backend().unwrap();
        assert_eq!(loaded.find_nodes_by_name("alpha").unwrap().len(), 1);
        assert_eq!(loaded.find_nodes_by_name("beta").unwrap().len(), 1);
        assert_eq!(
            loaded.find_nodes_by_type(NodeType::Function).unwrap().len(),
            2
        );
    }
}
