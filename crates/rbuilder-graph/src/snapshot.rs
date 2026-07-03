//! Memory-mappable binary graph snapshot format.
//!
//! Replaces JSON `graph.db` parsing with a single contiguous binary layout that can
//! be memory-mapped and hydrated into [`MemoryBackend`] without serde_json overhead.

use crate::backend::MemoryBackend;
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
/// Current snapshot format version.
pub const SNAPSHOT_VERSION: u32 = 1;

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
            type_index
                .entry(node.node_type)
                .or_default()
                .push(node.id);
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

    /// Write snapshot to disk with magic header.
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = bincode::serialize(self).map_err(serde_err)?;
        let mut file = File::create(path)?;
        use std::io::Write;
        file.write_all(&SNAPSHOT_MAGIC)?;
        file.write_all(&SNAPSHOT_VERSION.to_le_bytes())?;
        file.write_all(&(payload.len() as u64).to_le_bytes())?;
        file.write_all(&payload)?;
        Ok(())
    }

    /// Hydrate a [`MemoryBackend`] using pre-built indexes (no index rebuild scan).
    pub fn hydrate_backend(&self) -> Result<MemoryBackend> {
        MemoryBackend::from_prepared_snapshot(self)
    }
}

/// Memory-mapped graph snapshot for zero-copy open.
pub struct MmappedGraphSnapshot {
    _mmap: Arc<Mmap>,
    prepared: PreparedGraphSnapshot,
}

impl MmappedGraphSnapshot {
    /// Default path under a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root.join(crate::code_graph::GRAPH_DIR).join(SNAPSHOT_FILE)
    }

    /// Open and parse a snapshot file via mmap.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let prepared = parse_payload(&mmap)?;
        Ok(Self {
            _mmap: Arc::new(mmap),
            prepared,
        })
    }

    /// Access parsed snapshot data.
    pub fn prepared(&self) -> &PreparedGraphSnapshot {
        &self.prepared
    }

    pub fn content_digest(&self) -> &str {
        &self.prepared.content_digest
    }

    pub fn node_count(&self) -> usize {
        self.prepared.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.prepared.edges.len()
    }

    /// Typed edge list for graph projections (no backend required).
    pub fn edge_topology_typed(&self) -> Vec<(Uuid, Uuid, EdgeType)> {
        self.prepared
            .edges
            .iter()
            .map(|e| (e.from, e.to, e.edge_type))
            .collect()
    }

    /// Hydrate into an in-memory backend when mutation or legacy APIs are needed.
    pub fn hydrate_backend(&self) -> Result<MemoryBackend> {
        self.prepared.hydrate_backend()
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
        for (idx, node) in snapshot.prepared().nodes.iter().enumerate() {
            id_to_index.insert(node.id, idx);
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

    /// Prepared graph payload.
    pub fn prepared(&self) -> &PreparedGraphSnapshot {
        self.snapshot.prepared()
    }

    pub fn content_digest(&self) -> &str {
        self.snapshot.content_digest()
    }

    pub fn node_count(&self) -> usize {
        self.snapshot.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.snapshot.edge_count()
    }

    /// Lookup a node by UUID without hydrating [`MemoryBackend`].
    pub fn get_node(&self, id: Uuid) -> Option<&Node> {
        self.id_to_index
            .get(&id)
            .map(|&idx| &self.snapshot.prepared().nodes[idx])
    }

    /// Find nodes by bare name using the pre-built name index.
    pub fn find_nodes_by_name(&self, name: &str) -> Vec<&Node> {
        self.snapshot
            .prepared()
            .indexes
            .name_index
            .get(name)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.get_node(*id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Filter impact-zone UUIDs to function nodes only.
    pub fn filter_function_impact(&self, impact_zone_ids: &[Uuid]) -> Vec<Uuid> {
        impact_zone_ids
            .iter()
            .copied()
            .filter(|id| {
                self.get_node(*id)
                    .is_some_and(|n| n.node_type == NodeType::Function)
            })
            .collect()
    }
}

fn parse_payload(mmap: &[u8]) -> Result<PreparedGraphSnapshot> {
    if mmap.len() < 16 {
        return Err(Error::SerdeError("graph snapshot truncated".into()));
    }
    if mmap[0..4] != SNAPSHOT_MAGIC {
        return Err(Error::SerdeError("invalid graph snapshot magic".into()));
    }
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
    if version != SNAPSHOT_VERSION {
        return Err(Error::SerdeError(format!(
            "unsupported graph snapshot version {version}"
        )));
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
    fn snapshot_round_trip() {
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
