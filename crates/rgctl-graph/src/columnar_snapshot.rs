//! Columnar mmap graph snapshot (format v2).
//!
//! Hot columns (node ids, types, edge topology) are fixed-width in the mmap.
//! Open parses only the header + small index sections — not the full node/edge vectors.
//!
//! **Complexity:** open is O(1) for node columns; UUID lookup is O(log N) binary search on sorted rows;
//! `find_nodes_by_name` uses the embedded name index (lazy-parsed) without hydrating a [`MemoryBackend`].

use crate::backend::MemoryBackend;
use crate::csr::{edge_type_from_u8, edge_type_to_u8};
use crate::lazy_collections::LazyStringMap;
use crate::normalize_path_str;
use crate::schema::{Edge, EdgeType, GraphParameter, Node, NodeType, SharedStr};
use crate::snapshot::{PreparedGraphSnapshot, PreparedIndexes, SNAPSHOT_MAGIC};
use memmap2::Mmap;
use rgctl_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
/// Snapshot file format version for columnar layout.
pub const COLUMNAR_SNAPSHOT_VERSION: u32 = 2;

const HEADER_SIZE: usize = 136;
const NODE_ROW_SIZE: usize = 64;
const EDGE_ROW_SIZE: usize = 40;
const _: () = assert!(std::mem::size_of::<NodeRow>() == NODE_ROW_SIZE);
const _: () = assert!(std::mem::size_of::<EdgeRow>() == EDGE_ROW_SIZE);

/// Per-node cold fields stored as a small bincode blob.
///
/// `properties` uses [`LazyStringMap`], which serializes keys in sorted order so
/// extension blob bytes (and [`crate::stable_key::extension_digest`]) are stable (#93).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct NodeExtension {
    qualified_name: Option<String>,
    return_type: Option<String>,
    code_hash: Option<String>,
    #[serde(default)]
    token_bloom: Option<[u64; 4]>,
    parameters: Vec<GraphParameter>,
    properties: LazyStringMap,
    labels: Vec<String>,
}

/// Borrowed view for write-path serialization (no String/Vec clones).
#[derive(Serialize)]
struct NodeExtensionRef<'a> {
    qualified_name: Option<&'a str>,
    return_type: Option<&'a str>,
    code_hash: Option<&'a str>,
    token_bloom: Option<[u64; 4]>,
    parameters: &'a [GraphParameter],
    properties: &'a LazyStringMap,
    labels: &'a [String],
}

impl NodeExtensionRef<'_> {
    fn from_node(node: &Node) -> NodeExtensionRef<'_> {
        NodeExtensionRef {
            qualified_name: node.qualified_name.as_deref(),
            return_type: node.return_type.as_deref(),
            code_hash: node.code_hash.as_deref(),
            token_bloom: node.token_bloom,
            parameters: &node.parameters,
            properties: &node.properties,
            labels: &node.labels,
        }
    }
}

/// Pre-`token_bloom` extension layout for columnar snapshot backward compatibility.
#[derive(Debug, Clone, Deserialize)]
struct NodeExtensionV1 {
    qualified_name: Option<String>,
    return_type: Option<String>,
    code_hash: Option<String>,
    parameters: Vec<GraphParameter>,
    properties: LazyStringMap,
    labels: Vec<String>,
}

fn decode_node_extension(bytes: &[u8]) -> Result<NodeExtension> {
    if let Ok(ext) = bincode::deserialize::<NodeExtension>(bytes) {
        return Ok(ext);
    }
    let legacy = bincode::deserialize::<NodeExtensionV1>(bytes)
        .map_err(|err| Error::SerdeError(format!("node extension: {err}")))?;
    Ok(NodeExtension {
        qualified_name: legacy.qualified_name,
        return_type: legacy.return_type,
        code_hash: legacy.code_hash,
        token_bloom: None,
        parameters: legacy.parameters,
        properties: legacy.properties,
        labels: legacy.labels,
    })
}

/// Fixed-width node column (64 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct NodeRow {
    pub(crate) id: [u8; 16],
    pub(crate) node_type: u16,
    pub(crate) _pad: u16,
    pub(crate) name_off: u32,
    pub(crate) name_len: u32,
    pub(crate) file_path_off: u32,
    pub(crate) file_path_len: u32,
    pub(crate) signature_off: u32,
    pub(crate) signature_len: u32,
    pub(crate) start_line: u32,
    pub(crate) end_line: u32,
    pub(crate) extension_off: u32,
    pub(crate) extension_len: u32,
    pub(crate) _pad_end: u32,
}

/// Fixed-width edge column (40 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct EdgeRow {
    pub(crate) from: [u8; 16],
    pub(crate) to: [u8; 16],
    pub(crate) edge_type: u8,
    pub(crate) _pad: [u8; 7],
}

/// Parsed columnar snapshot backed by mmap (no full graph deserialize at open).
///
/// Prefer [`Self::find_nodes_by_name`] and [`Self::edge_topology_typed`] for read-only access;
/// call [`Self::to_prepared`] only when a full in-memory backend is required.
pub struct ColumnarGraphMmap {
    mmap: Arc<Mmap>,
    schema_version: u32,
    node_count: usize,
    edge_count: usize,
    digest_hex: String,
    offset_nodes: u64,
    offset_edges: u64,
    offset_strings: u64,
    offset_strings_len: u64,
    index_tail_off: usize,
    offset_extensions: u64,
    parsed_indexes: Mutex<Option<Arc<(HashMap<String, Vec<Uuid>>, HashMap<NodeType, Vec<Uuid>>)>>>,
}

impl ColumnarGraphMmap {
    /// Open a v2 columnar snapshot from an already-mmapped file.
    pub fn open(mmap: Arc<Mmap>) -> Result<Self> {
        if mmap.len() < HEADER_SIZE {
            return Err(Error::SerdeError("columnar snapshot truncated".into()));
        }
        if mmap[0..4] != SNAPSHOT_MAGIC {
            return Err(Error::SerdeError("invalid graph snapshot magic".into()));
        }
        let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
        if version != COLUMNAR_SNAPSHOT_VERSION {
            return Err(Error::SerdeError(format!(
                "expected columnar snapshot version {}, got {version}",
                COLUMNAR_SNAPSHOT_VERSION
            )));
        }

        let schema_version = u32::from_le_bytes(mmap[8..12].try_into().unwrap());
        let node_count = u64::from_le_bytes(mmap[12..20].try_into().unwrap()) as usize;
        let edge_count = u64::from_le_bytes(mmap[20..28].try_into().unwrap()) as usize;
        let digest = std::str::from_utf8(&mmap[28..92])
            .map_err(|_| Error::SerdeError("columnar digest utf8".into()))?
            .trim_end_matches('\0')
            .to_string();

        let offset_nodes = u64::from_le_bytes(mmap[92..100].try_into().unwrap());
        let offset_edges = u64::from_le_bytes(mmap[100..108].try_into().unwrap());
        let offset_strings = u64::from_le_bytes(mmap[108..116].try_into().unwrap());
        let offset_strings_len = u64::from_le_bytes(mmap[116..124].try_into().unwrap());
        let offset_extensions = u64::from_le_bytes(mmap[128..136].try_into().unwrap());

        let tail = &mmap[HEADER_SIZE..];
        let name_section = index_section_byte_len(tail, 0)?;
        index_section_byte_len(tail, name_section)?;

        let expected_nodes_end = offset_nodes as usize + node_count * NODE_ROW_SIZE;
        let expected_edges_end = offset_edges as usize + edge_count * EDGE_ROW_SIZE;
        if expected_nodes_end > mmap.len() || expected_edges_end > mmap.len() {
            return Err(Error::SerdeError(
                "columnar snapshot column out of range".into(),
            ));
        }

        Ok(Self {
            mmap,
            schema_version,
            node_count,
            edge_count,
            digest_hex: digest,
            offset_nodes,
            offset_edges,
            offset_strings,
            offset_strings_len,
            index_tail_off: HEADER_SIZE,
            offset_extensions,
            parsed_indexes: Mutex::new(None),
        })
    }

    fn parsed_indexes(
        &self,
    ) -> Result<Arc<(HashMap<String, Vec<Uuid>>, HashMap<NodeType, Vec<Uuid>>)>> {
        let mut guard = self
            .parsed_indexes
            .lock()
            .map_err(|e| Error::SerdeError(e.to_string()))?;
        if guard.is_none() {
            let tail = &self.mmap[self.index_tail_off..];
            let (name_index, name_consumed) = read_index_section(tail, 0)?;
            let (type_index, _) = read_type_index_section(tail, name_consumed)?;
            *guard = Some(Arc::new((name_index, type_index)));
        }
        Ok(Arc::clone(guard.as_ref().unwrap()))
    }

    /// O(log N) lookup in the sorted node column (no reverse index heap allocation).
    pub fn find_node_index(&self, target_id: Uuid) -> Option<usize> {
        let target_bytes = target_id.as_bytes();
        let base = self.offset_nodes as usize;
        let mut low = 0usize;
        let mut high = self.node_count;
        while low < high {
            let mid = low + (high - low) / 2;
            let row = read_node_row(self.mmap.as_ref(), base, mid).ok()?;
            match row.id.as_slice().cmp(target_bytes) {
                Ordering::Less => low = mid + 1,
                Ordering::Greater => high = mid,
                Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    /// Graph schema version stored in the snapshot header.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Number of nodes in the snapshot.
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Number of edges in the snapshot.
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// BLAKE3 content digest for cache invalidation.
    pub fn content_digest(&self) -> &str {
        &self.digest_hex
    }

    /// Shared Arc to lazy-parsed name/type indexes (no HashMap clone).
    ///
    /// Prefer this for structured query hot paths. [`Self::name_index`] / [`Self::type_index`]
    /// clone the maps and are intended for hydration / one-shot tooling only.
    pub fn indexes_shared(
        &self,
    ) -> Result<Arc<(HashMap<String, Vec<Uuid>>, HashMap<NodeType, Vec<Uuid>>)>> {
        self.parsed_indexes()
    }

    /// Name → node id index (lazy-parsed from mmap on first access).
    ///
    /// Clones the map — prefer [`Self::indexes_shared`] on query hot paths.
    pub fn name_index(&self) -> Result<HashMap<String, Vec<Uuid>>> {
        Ok(self.parsed_indexes()?.0.clone())
    }

    /// Node type → node id index (lazy-parsed from mmap on first access).
    ///
    /// Clones the map — prefer [`Self::indexes_shared`] on query hot paths.
    pub fn type_index(&self) -> Result<HashMap<NodeType, Vec<Uuid>>> {
        Ok(self.parsed_indexes()?.1.clone())
    }

    /// Clone embedded indexes for backend hydration.
    pub fn prepared_indexes(&self) -> Result<PreparedIndexes> {
        let indexes = self.parsed_indexes()?;
        Ok(PreparedIndexes {
            name_index: indexes.0.clone(),
            type_index: indexes.1.clone(),
        })
    }

    /// Iterate `(column_index, node_id)` pairs without materializing nodes.
    pub fn node_ids_by_index(&self) -> impl Iterator<Item = (usize, Uuid)> + '_ {
        (0..self.node_count).filter_map(|idx| {
            self.node_id_at(idx).ok().map(|id| (idx, id))
        })
    }

    /// Read typed edge topology directly from mmap columns.
    pub fn edge_topology_typed(&self) -> Result<Vec<(Uuid, Uuid, EdgeType)>> {
        let mut out = Vec::with_capacity(self.edge_count);
        self.for_each_edge(|from, to, edge_type| {
            out.push((from, to, edge_type));
            Ok(())
        })?;
        Ok(out)
    }

    /// Stream edge rows without allocating a full topology `Vec`.
    pub fn for_each_edge(
        &self,
        mut f: impl FnMut(Uuid, Uuid, EdgeType) -> Result<()>,
    ) -> Result<()> {
        for idx in 0..self.edge_count {
            let (from, to, edge_type) = self.edge_at(idx)?;
            f(from, to, edge_type)?;
        }
        Ok(())
    }

    /// Read one edge row by column index.
    pub fn edge_at(&self, idx: usize) -> Result<(Uuid, Uuid, EdgeType)> {
        if idx >= self.edge_count {
            return Err(Error::SerdeError(format!(
                "edge index {idx} out of range (count={})",
                self.edge_count
            )));
        }
        let row = read_edge_row(self.mmap.as_ref(), self.offset_edges as usize, idx)?;
        Ok((
            Uuid::from_bytes(row.from),
            Uuid::from_bytes(row.to),
            edge_type_from_u8(row.edge_type)?,
        ))
    }

    /// Node id at column index (no string pool reads).
    pub(crate) fn node_id_at(&self, idx: usize) -> Result<Uuid> {
        if idx >= self.node_count {
            return Err(Error::SerdeError(format!(
                "node index {idx} out of range (count={})",
                self.node_count
            )));
        }
        let row = read_node_row(self.mmap.as_ref(), self.offset_nodes as usize, idx)?;
        Ok(Uuid::from_bytes(row.id))
    }

    /// Materialize a single node by column index.
    pub fn materialize_node_at(&self, idx: usize) -> Result<Node> {
        self.materialize_node(idx)
    }

    /// Fixed-width node row at column index (no string pool reads).
    pub(crate) fn node_row_at(&self, idx: usize) -> Result<NodeRow> {
        if idx >= self.node_count {
            return Err(Error::SerdeError(format!(
                "node index {idx} out of range (count={})",
                self.node_count
            )));
        }
        read_node_row(self.mmap.as_ref(), self.offset_nodes as usize, idx)
    }

    /// String pool base offset and byte length for [`string_at`].
    pub(crate) fn string_pool_bounds(&self) -> (usize, usize) {
        (self.offset_strings as usize, self.offset_strings_len as usize)
    }

    /// Raw mmap bytes for zero-copy string pool reads.
    pub(crate) fn mmap_bytes(&self) -> &[u8] {
        self.mmap.as_ref()
    }

    /// Raw extension blob for a node row (no bincode decode).
    pub(crate) fn extension_bytes_at(&self, idx: usize) -> Result<Option<&[u8]>> {
        if idx >= self.node_count {
            return Err(Error::SerdeError(format!(
                "node index {idx} out of range (count={})",
                self.node_count
            )));
        }
        let row = read_node_row(self.mmap.as_ref(), self.offset_nodes as usize, idx)?;
        if row.extension_len == 0 {
            return Ok(None);
        }
        let start = self.offset_extensions as usize + row.extension_off as usize;
        let end = start + row.extension_len as usize;
        if end > self.mmap.len() {
            return Err(Error::SerdeError("node extension out of range".into()));
        }
        Ok(Some(&self.mmap[start..end]))
    }

    /// Zero-alloc borrow of `NodeExtension::qualified_name` (first bincode field).
    ///
    /// Layout (bincode `serialize` / fixint): `u8` Option tag, then for `Some`
    /// a little-endian `u64` byte length and the UTF-8 payload. Does not
    /// deserialize the rest of the cold extension blob.
    pub fn node_qualified_name_str(&self, idx: usize) -> Result<Option<&str>> {
        let Some(bytes) = self.extension_bytes_at(idx)? else {
            return Ok(None);
        };
        match bytes.first().copied() {
            Some(0) => Ok(None),
            Some(1) if bytes.len() >= 9 => {
                let slen = u64::from_le_bytes(
                    bytes[1..9]
                        .try_into()
                        .expect("slice length checked"),
                ) as usize;
                let sbytes = bytes.get(9..9 + slen).ok_or_else(|| {
                    Error::SerdeError("node extension qualified_name out of range".into())
                })?;
                std::str::from_utf8(sbytes)
                    .map(Some)
                    .map_err(|e| Error::SerdeError(format!("qualified_name utf8: {e}")))
            }
            _ => Err(Error::SerdeError(
                "invalid qualified_name option tag in node extension".into(),
            )),
        }
    }

    /// Whether a base node should be dropped for invalidated file paths (no full materialize).
    pub(crate) fn node_invalidated_at(
        &self,
        idx: usize,
        invalidated: &std::collections::HashSet<String>,
    ) -> Result<bool> {
        if invalidated.is_empty() {
            return Ok(false);
        }
        let row = read_node_row(self.mmap.as_ref(), self.offset_nodes as usize, idx)?;
        let node_type = node_type_from_u16(row.node_type)?;
        let name = read_string(
            self.mmap.as_ref(),
            self.offset_strings as usize,
            self.offset_strings_len as usize,
            row.name_off,
            row.name_len,
        )?;
        let file_path = optional_string(
            self.mmap.as_ref(),
            self.offset_strings as usize,
            self.offset_strings_len as usize,
            row.file_path_off,
            row.file_path_len,
        )?;
        Ok(node_matches_invalidated_path(
            file_path.as_deref(),
            &name,
            node_type,
            invalidated,
        ))
    }

    /// Append a kept base node into a columnar build, copying extension bytes verbatim.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_node_for_build(
        &self,
        idx: usize,
        hasher: &mut blake3::Hasher,
        strings: &mut StringPool,
        extensions_blob: &mut Vec<u8>,
        name_index: &mut HashMap<String, Vec<Uuid>>,
        type_index: &mut HashMap<NodeType, Vec<Uuid>>,
        node_rows: &mut Vec<NodeRow>,
        scratch: &mut Vec<u8>,
    ) -> Result<()> {
        let node = self.materialize_node(idx)?;
        scratch.clear();
        bincode::serialize_into(&mut *scratch, &node).map_err(bincode_err)?;
        let extension_bytes = self.extension_bytes_at(idx)?;
        append_node_columnar_prehashed(
            &node,
            scratch,
            hasher,
            strings,
            extensions_blob,
            name_index,
            type_index,
            node_rows,
            extension_bytes,
        )
    }

    /// Materialize a single node by id (reads cold extension blob).
    pub fn get_node(&self, id: Uuid) -> Result<Option<Node>> {
        let Some(idx) = self.find_node_index(id) else {
            return Ok(None);
        };
        Ok(Some(self.materialize_node(idx)?))
    }

    /// Find nodes by exact name via the embedded name index.
    pub fn find_nodes_by_name(&self, name: &str) -> Result<Vec<Node>> {
        let indexes = self.parsed_indexes()?;
        let Some(ids) = indexes.0.get(name) else {
            return Ok(Vec::new());
        };
        Ok(ids
            .iter()
            .filter_map(|id| {
                let idx = self.find_node_index(*id)?;
                self.materialize_node(idx).ok()
            })
            .collect())
    }

    fn materialize_node(&self, idx: usize) -> Result<Node> {
        let row = read_node_row(self.mmap.as_ref(), self.offset_nodes as usize, idx)?;
        let id = Uuid::from_bytes(row.id);
        let name = read_string(
            self.mmap.as_ref(),
            self.offset_strings as usize,
            self.offset_strings_len as usize,
            row.name_off,
            row.name_len,
        )?;
        let file_path = optional_string(
            self.mmap.as_ref(),
            self.offset_strings as usize,
            self.offset_strings_len as usize,
            row.file_path_off,
            row.file_path_len,
        )?;
        let signature = optional_string(
            self.mmap.as_ref(),
            self.offset_strings as usize,
            self.offset_strings_len as usize,
            row.signature_off,
            row.signature_len,
        )?;
        let extension = if row.extension_len > 0 {
            let start = self.offset_extensions as usize + row.extension_off as usize;
            let end = start + row.extension_len as usize;
            if end > self.mmap.len() {
                return Err(Error::SerdeError("node extension out of range".into()));
            }
            decode_node_extension(&self.mmap[start..end])?
        } else {
            NodeExtension::default()
        };

        Ok(Node {
            id,
            node_type: node_type_from_u16(row.node_type)?,
            name: SharedStr::from(name),
            qualified_name: extension.qualified_name.map(SharedStr::from),
            signature: signature.map(SharedStr::from),
            return_type: extension.return_type.map(SharedStr::from),
            parameters: extension.parameters,
            code_hash: extension.code_hash.map(SharedStr::from),
            token_bloom: extension.token_bloom,
            file_path: file_path.map(SharedStr::from),
            start_line: (row.start_line > 0).then_some(row.start_line as usize),
            end_line: (row.end_line > 0).then_some(row.end_line as usize),
            properties: extension.properties,
            labels: extension.labels,
        })
    }

    /// Materialize a full [`PreparedGraphSnapshot`] (explicit hydrate / legacy API).
    pub fn to_prepared(&self) -> Result<PreparedGraphSnapshot> {
        let mut nodes = Vec::with_capacity(self.node_count);
        for idx in 0..self.node_count {
            nodes.push(self.materialize_node(idx)?);
        }
        let mut edges = Vec::with_capacity(self.edge_count);
        for idx in 0..self.edge_count {
            let row = read_edge_row(self.mmap.as_ref(), self.offset_edges as usize, idx)?;
            edges.push(Edge::new(
                Uuid::from_bytes(row.from),
                Uuid::from_bytes(row.to),
                edge_type_from_u8(row.edge_type)?,
            ));
        }
        Ok(PreparedGraphSnapshot {
            schema_version: self.schema_version,
            nodes,
            edges,
            indexes: self.prepared_indexes()?,
            content_digest: self.digest_hex.clone(),
        })
    }
}

impl PreparedGraphSnapshot {
    /// Write columnar v2 snapshot (default write path).
    pub fn write_columnar_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut strings = StringPool::new();
        let mut node_rows = Vec::with_capacity(self.nodes.len());
        let mut extensions_blob = Vec::new();
        let mut ext_scratch = Vec::with_capacity(256);

        for node in &self.nodes {
            let name_off = strings.intern(&node.name);
            let file_path_off = strings.intern_opt(node.file_path.as_deref());
            let signature_off = strings.intern_opt(node.signature.as_deref());
            let extension = NodeExtensionRef::from_node(node);
            ext_scratch.clear();
            bincode::serialize_into(&mut ext_scratch, &extension).map_err(bincode_err)?;
            let extension_off = extensions_blob.len() as u32;
            let extension_len = ext_scratch.len() as u32;
            extensions_blob.extend_from_slice(&ext_scratch);

            node_rows.push(NodeRow {
                id: *node.id.as_bytes(),
                node_type: node_type_to_u16(node.node_type),
                _pad: 0,
                name_off: name_off.off,
                name_len: name_off.len,
                file_path_off: file_path_off.off,
                file_path_len: file_path_off.len,
                signature_off: signature_off.off,
                signature_len: signature_off.len,
                start_line: node.start_line.unwrap_or(0) as u32,
                end_line: node.end_line.unwrap_or(0) as u32,
                extension_off,
                extension_len,
                _pad_end: 0,
            });
        }

        let mut edge_rows = Vec::with_capacity(self.edges.len());
        for edge in &self.edges {
            edge_rows.push(EdgeRow {
                from: *edge.from.as_bytes(),
                to: *edge.to.as_bytes(),
                edge_type: edge_type_to_u8(edge.edge_type),
                _pad: [0; 7],
            });
        }

        let name_index_bytes = bincode::serialize(&self.indexes.name_index).map_err(bincode_err)?;
        let type_index_bytes = bincode::serialize(&self.indexes.type_index).map_err(bincode_err)?;

        let offset_nodes = HEADER_SIZE as u64
            + 8
            + name_index_bytes.len() as u64
            + 8
            + type_index_bytes.len() as u64;
        let offset_edges = offset_nodes + (node_rows.len() * NODE_ROW_SIZE) as u64;
        let offset_strings = offset_edges + (edge_rows.len() * EDGE_ROW_SIZE) as u64;
        let offset_strings_len = strings.bytes.len() as u64;
        let offset_extensions = offset_strings + offset_strings_len;

        let mut digest_bytes = [0u8; 64];
        let digest_src = self.content_digest.as_bytes();
        let copy_len = digest_src.len().min(64);
        digest_bytes[..copy_len].copy_from_slice(&digest_src[..copy_len]);

        let mut file = Vec::new();
        file.extend_from_slice(&SNAPSHOT_MAGIC);
        file.extend_from_slice(&COLUMNAR_SNAPSHOT_VERSION.to_le_bytes());
        file.extend_from_slice(&self.schema_version.to_le_bytes());
        file.extend_from_slice(&(self.nodes.len() as u64).to_le_bytes());
        file.extend_from_slice(&(self.edges.len() as u64).to_le_bytes());
        file.extend_from_slice(&digest_bytes);
        file.extend_from_slice(&offset_nodes.to_le_bytes());
        file.extend_from_slice(&offset_edges.to_le_bytes());
        file.extend_from_slice(&offset_strings.to_le_bytes());
        file.extend_from_slice(&offset_strings_len.to_le_bytes());
        file.extend_from_slice(&[0u8; 4]); // reserved
        file.extend_from_slice(&offset_extensions.to_le_bytes());
        debug_assert_eq!(file.len(), HEADER_SIZE);

        file.extend_from_slice(&(name_index_bytes.len() as u64).to_le_bytes());
        file.extend_from_slice(&name_index_bytes);
        file.extend_from_slice(&(type_index_bytes.len() as u64).to_le_bytes());
        file.extend_from_slice(&type_index_bytes);

        for row in &node_rows {
            file.extend_from_slice(&encode_node_row(row));
        }
        for row in &edge_rows {
            file.extend_from_slice(&encode_edge_row(row));
        }
        file.extend_from_slice(&strings.bytes);
    file.extend_from_slice(&extensions_blob);

    let tmp = path.with_file_name(format!(
        "{}.tmp",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_else(|| "graph.snapshot.bin".into())
    ));
    std::fs::write(&tmp, &file)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e
    })?;
    Ok(())
}
}

pub(crate) struct StringPool {
    pub(crate) bytes: Vec<u8>,
    offsets: HashMap<u64, StrRef>,
}

#[derive(Clone, Copy)]
pub(crate) struct StrRef {
    pub(crate) off: u32,
    pub(crate) len: u32,
}

impl StringPool {
    pub(crate) fn new() -> Self {
        Self {
            bytes: Vec::new(),
            offsets: HashMap::new(),
        }
    }

    pub(crate) fn intern(&mut self, s: &str) -> StrRef {
        let hash = hash_string_key(s);
        if let Some(&existing) = self.offsets.get(&hash) {
            if self.str_at(existing) == s {
                return existing;
            }
        }
        let off = self.bytes.len() as u32;
        let bytes = s.as_bytes();
        self.bytes.extend_from_slice(bytes);
        let str_ref = StrRef {
            off,
            len: bytes.len() as u32,
        };
        self.offsets.insert(hash, str_ref);
        str_ref
    }

    pub(crate) fn intern_opt(&mut self, s: Option<&str>) -> StrRef {
        match s {
            Some(v) => self.intern(v),
            None => StrRef { off: 0, len: 0 },
        }
    }

    fn str_at(&self, reference: StrRef) -> &str {
        let start = reference.off as usize;
        let end = start + reference.len as usize;
        std::str::from_utf8(&self.bytes[start..end]).expect("string pool utf8")
    }
}

fn hash_string_key(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

fn read_node_row(mmap: &[u8], base: usize, idx: usize) -> Result<NodeRow> {
    let start = base + idx * NODE_ROW_SIZE;
    let end = start + NODE_ROW_SIZE;
    if end > mmap.len() {
        return Err(Error::SerdeError("node row out of range".into()));
    }
    let mut row = std::mem::MaybeUninit::<NodeRow>::uninit();
    // SAFETY: NodeRow is #[repr(C)] with fixed NODE_ROW_SIZE; bounds checked above.
    unsafe {
        std::ptr::copy_nonoverlapping(
            mmap.as_ptr().add(start),
            row.as_mut_ptr() as *mut u8,
            NODE_ROW_SIZE,
        );
        Ok(row.assume_init())
    }
}

fn read_edge_row(mmap: &[u8], base: usize, idx: usize) -> Result<EdgeRow> {
    let start = base + idx * EDGE_ROW_SIZE;
    let end = start + EDGE_ROW_SIZE;
    if end > mmap.len() {
        return Err(Error::SerdeError("edge row out of range".into()));
    }
    let mut row = std::mem::MaybeUninit::<EdgeRow>::uninit();
    // SAFETY: EdgeRow is #[repr(C)] with fixed EDGE_ROW_SIZE; bounds checked above.
    unsafe {
        std::ptr::copy_nonoverlapping(
            mmap.as_ptr().add(start),
            row.as_mut_ptr() as *mut u8,
            EDGE_ROW_SIZE,
        );
        Ok(row.assume_init())
    }
}

/// Borrow a UTF-8 string from the mmap string pool without allocating.
pub(crate) fn string_at(
    mmap: &[u8],
    base: usize,
    len_limit: usize,
    off: u32,
    len: u32,
) -> Result<&str> {
    if len == 0 {
        return Ok("");
    }
    let start = base + off as usize;
    let end = start + len as usize;
    if end > base + len_limit || end > mmap.len() {
        return Err(Error::SerdeError("string pool out of range".into()));
    }
    std::str::from_utf8(&mmap[start..end])
        .map_err(|e| Error::SerdeError(format!("string utf8: {e}")))
}

/// Optional string pool slice (`None` when `len == 0`).
pub(crate) fn optional_string_at(
    mmap: &[u8],
    base: usize,
    len_limit: usize,
    off: u32,
    len: u32,
) -> Result<Option<&str>> {
    if len == 0 {
        return Ok(None);
    }
    Ok(Some(string_at(mmap, base, len_limit, off, len)?))
}

fn read_string(mmap: &[u8], base: usize, len_limit: usize, off: u32, len: u32) -> Result<String> {
    Ok(string_at(mmap, base, len_limit, off, len)?.to_string())
}

fn optional_string(
    mmap: &[u8],
    base: usize,
    len_limit: usize,
    off: u32,
    len: u32,
) -> Result<Option<String>> {
    optional_string_at(mmap, base, len_limit, off, len)
        .map(|opt| opt.map(str::to_string))
}

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

fn index_section_byte_len(tail: &[u8], cursor: usize) -> Result<usize> {
    if cursor + 8 > tail.len() {
        return Err(Error::SerdeError("index section truncated".into()));
    }
    let len = u64::from_le_bytes(tail[cursor..cursor + 8].try_into().unwrap()) as usize;
    let end = cursor + 8 + len;
    if end > tail.len() {
        return Err(Error::SerdeError("index section payload truncated".into()));
    }
    Ok(end - cursor)
}

fn read_index_section(tail: &[u8], cursor: usize) -> Result<(HashMap<String, Vec<Uuid>>, usize)> {
    if cursor + 8 > tail.len() {
        return Err(Error::SerdeError("name index truncated".into()));
    }
    let len = u64::from_le_bytes(tail[cursor..cursor + 8].try_into().unwrap()) as usize;
    let start = cursor + 8;
    let end = start + len;
    if end > tail.len() {
        return Err(Error::SerdeError("name index payload truncated".into()));
    }
    let index: HashMap<String, Vec<Uuid>> =
        bincode::deserialize(&tail[start..end]).map_err(bincode_err)?;
    Ok((index, 8 + len))
}

fn read_type_index_section(
    tail: &[u8],
    cursor: usize,
) -> Result<(HashMap<NodeType, Vec<Uuid>>, usize)> {
    if cursor + 8 > tail.len() {
        return Err(Error::SerdeError("type index truncated".into()));
    }
    let len = u64::from_le_bytes(tail[cursor..cursor + 8].try_into().unwrap()) as usize;
    let start = cursor + 8;
    let end = start + len;
    if end > tail.len() {
        return Err(Error::SerdeError("type index payload truncated".into()));
    }
    let index: HashMap<NodeType, Vec<Uuid>> =
        bincode::deserialize(&tail[start..end]).map_err(bincode_err)?;
    Ok((index, 8 + len))
}

fn encode_node_row(row: &NodeRow) -> [u8; NODE_ROW_SIZE] {
    let mut buf = [0u8; NODE_ROW_SIZE];
    buf[0..16].copy_from_slice(&row.id);
    buf[16..18].copy_from_slice(&row.node_type.to_le_bytes());
    buf[18..20].copy_from_slice(&row._pad.to_le_bytes());
    buf[20..24].copy_from_slice(&row.name_off.to_le_bytes());
    buf[24..28].copy_from_slice(&row.name_len.to_le_bytes());
    buf[28..32].copy_from_slice(&row.file_path_off.to_le_bytes());
    buf[32..36].copy_from_slice(&row.file_path_len.to_le_bytes());
    buf[36..40].copy_from_slice(&row.signature_off.to_le_bytes());
    buf[40..44].copy_from_slice(&row.signature_len.to_le_bytes());
    buf[44..48].copy_from_slice(&row.start_line.to_le_bytes());
    buf[48..52].copy_from_slice(&row.end_line.to_le_bytes());
    buf[52..56].copy_from_slice(&row.extension_off.to_le_bytes());
    buf[56..60].copy_from_slice(&row.extension_len.to_le_bytes());
    buf[60..64].copy_from_slice(&row._pad_end.to_le_bytes());
    buf
}

fn encode_edge_row(row: &EdgeRow) -> [u8; EDGE_ROW_SIZE] {
    let mut buf = [0u8; EDGE_ROW_SIZE];
    buf[0..16].copy_from_slice(&row.from);
    buf[16..32].copy_from_slice(&row.to);
    buf[32] = row.edge_type;
    buf[33..40].copy_from_slice(&row._pad);
    buf
}

fn node_type_to_u16(t: NodeType) -> u16 {
    match t {
        NodeType::Function => 0,
        NodeType::Class => 1,
        NodeType::Struct => 2,
        NodeType::Enum => 3,
        NodeType::Interface => 4,
        NodeType::Module => 5,
        NodeType::Variable => 6,
        NodeType::File => 7,
        NodeType::ConfigKey => 8,
        NodeType::TypeAlias => 9,
        NodeType::Macro => 10,
        NodeType::Import => 11,
        NodeType::Table => 12,
        NodeType::Dependency => 13,
        NodeType::Job => 14,
        NodeType::BuildStep => 15,
        NodeType::AnsiblePlaybook => 16,
        NodeType::AnsiblePlay => 17,
        NodeType::AnsibleTask => 18,
        NodeType::AnsibleRole => 19,
        NodeType::AnsibleHandler => 20,
        NodeType::AnsibleVariable => 21,
        NodeType::AnsibleTemplate => 22,
        NodeType::ChefCookbook => 23,
        NodeType::ChefRecipe => 24,
        NodeType::ChefResource => 25,
        NodeType::ChefAttribute => 26,
        NodeType::ChefTemplate => 27,
        NodeType::ChefCustomResource => 28,
        NodeType::PuppetModule => 29,
        NodeType::PuppetClass => 30,
        NodeType::PuppetDefinedType => 31,
        NodeType::PuppetResource => 32,
        NodeType::PuppetVariable => 33,
        NodeType::PuppetFact => 34,
        NodeType::Annotation => 35,
        NodeType::KantraRuleset => 36,
        NodeType::KantraRule => 37,
        NodeType::PuppetNode => 38,
    }
}

pub(crate) fn node_type_from_u16(v: u16) -> Result<NodeType> {
    Ok(match v {
        0 => NodeType::Function,
        1 => NodeType::Class,
        2 => NodeType::Struct,
        3 => NodeType::Enum,
        4 => NodeType::Interface,
        5 => NodeType::Module,
        6 => NodeType::Variable,
        7 => NodeType::File,
        8 => NodeType::ConfigKey,
        9 => NodeType::TypeAlias,
        10 => NodeType::Macro,
        11 => NodeType::Import,
        12 => NodeType::Table,
        13 => NodeType::Dependency,
        14 => NodeType::Job,
        15 => NodeType::BuildStep,
        16 => NodeType::AnsiblePlaybook,
        17 => NodeType::AnsiblePlay,
        18 => NodeType::AnsibleTask,
        19 => NodeType::AnsibleRole,
        20 => NodeType::AnsibleHandler,
        21 => NodeType::AnsibleVariable,
        22 => NodeType::AnsibleTemplate,
        23 => NodeType::ChefCookbook,
        24 => NodeType::ChefRecipe,
        25 => NodeType::ChefResource,
        26 => NodeType::ChefAttribute,
        27 => NodeType::ChefTemplate,
        28 => NodeType::ChefCustomResource,
        29 => NodeType::PuppetModule,
        30 => NodeType::PuppetClass,
        31 => NodeType::PuppetDefinedType,
        32 => NodeType::PuppetResource,
        33 => NodeType::PuppetVariable,
        34 => NodeType::PuppetFact,
        35 => NodeType::Annotation,
        36 => NodeType::KantraRuleset,
        37 => NodeType::KantraRule,
        38 => NodeType::PuppetNode,
        _ => return Err(Error::SerdeError(format!("unknown node type code {v}"))),
    })
}

fn bincode_err(e: bincode::Error) -> Error {
    Error::SerdeError(format!("columnar snapshot: {e}"))
}

pub(crate) fn edge_digest_bytes(edge: &Edge) -> Result<Vec<u8>> {
    bincode::serialize(&edge.for_columnar_digest()).map_err(bincode_err)
}

pub(crate) fn append_node_columnar(
    node: &Node,
    hasher: &mut blake3::Hasher,
    strings: &mut StringPool,
    extensions_blob: &mut Vec<u8>,
    name_index: &mut HashMap<String, Vec<Uuid>>,
    type_index: &mut HashMap<NodeType, Vec<Uuid>>,
    node_rows: &mut Vec<NodeRow>,
    scratch: &mut Vec<u8>,
) -> Result<()> {
    scratch.clear();
    bincode::serialize_into(&mut *scratch, node).map_err(bincode_err)?;
    append_node_columnar_prehashed(
        node,
        scratch,
        hasher,
        strings,
        extensions_blob,
        name_index,
        type_index,
        node_rows,
        None,
    )
}

/// Hash pre-serialized node bytes (must match `bincode::serialize(node)`) then encode columns.
///
/// When `extension_bytes` is `Some`, the cold extension blob is copied verbatim instead of
/// re-serializing from `node` fields (compaction pass-through).
#[allow(clippy::too_many_arguments)]
pub(crate) fn append_node_columnar_prehashed(
    node: &Node,
    node_bytes: &[u8],
    hasher: &mut blake3::Hasher,
    strings: &mut StringPool,
    extensions_blob: &mut Vec<u8>,
    name_index: &mut HashMap<String, Vec<Uuid>>,
    type_index: &mut HashMap<NodeType, Vec<Uuid>>,
    node_rows: &mut Vec<NodeRow>,
    extension_bytes: Option<&[u8]>,
) -> Result<()> {
    hasher.update(node_bytes);

    let name_off = strings.intern(&node.name);
    let file_path_off = strings.intern_opt(node.file_path.as_deref());
    let signature_off = strings.intern_opt(node.signature.as_deref());
    let extension_off = extensions_blob.len() as u32;
    let extension_len = match extension_bytes {
        Some(bytes) => {
            extensions_blob.extend_from_slice(bytes);
            bytes.len() as u32
        }
        None => {
            let start = extensions_blob.len();
            let extension = NodeExtensionRef::from_node(node);
            bincode::serialize_into(&mut *extensions_blob, &extension).map_err(bincode_err)?;
            (extensions_blob.len() - start) as u32
        }
    };

    node_rows.push(NodeRow {
        id: *node.id.as_bytes(),
        node_type: node_type_to_u16(node.node_type),
        _pad: 0,
        name_off: name_off.off,
        name_len: name_off.len,
        file_path_off: file_path_off.off,
        file_path_len: file_path_off.len,
        signature_off: signature_off.off,
        signature_len: signature_off.len,
        start_line: node.start_line.unwrap_or(0) as u32,
        end_line: node.end_line.unwrap_or(0) as u32,
        extension_off,
        extension_len,
        _pad_end: 0,
    });

    name_index
        .entry(node.name.to_string())
        .or_default()
        .push(node.id);
    type_index.entry(node.node_type).or_default().push(node.id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_columnar_assembled(
    path: &Path,
    node_rows: &[NodeRow],
    edge_rows: &[EdgeRow],
    strings: &StringPool,
    extensions_blob: &[u8],
    name_index: &HashMap<String, Vec<Uuid>>,
    type_index: &HashMap<NodeType, Vec<Uuid>>,
    content_digest: &str,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let name_index_bytes = bincode::serialize(name_index).map_err(bincode_err)?;
    let type_index_bytes = bincode::serialize(type_index).map_err(bincode_err)?;

    let offset_nodes =
        HEADER_SIZE as u64 + 8 + name_index_bytes.len() as u64 + 8 + type_index_bytes.len() as u64;
    let offset_edges = offset_nodes + (node_rows.len() * NODE_ROW_SIZE) as u64;
    let offset_strings = offset_edges + (edge_rows.len() * EDGE_ROW_SIZE) as u64;
    let offset_strings_len = strings.bytes.len() as u64;
    let offset_extensions = offset_strings + offset_strings_len;

    let total_bytes = HEADER_SIZE
        + 8
        + name_index_bytes.len()
        + 8
        + type_index_bytes.len()
        + node_rows.len() * NODE_ROW_SIZE
        + edge_rows.len() * EDGE_ROW_SIZE
        + strings.bytes.len()
        + extensions_blob.len();

    let mut digest_bytes = [0u8; 64];
    let digest_src = content_digest.as_bytes();
    let copy_len = digest_src.len().min(64);
    digest_bytes[..copy_len].copy_from_slice(&digest_src[..copy_len]);

    let mut file = Vec::with_capacity(total_bytes);
    file.extend_from_slice(&SNAPSHOT_MAGIC);
    file.extend_from_slice(&COLUMNAR_SNAPSHOT_VERSION.to_le_bytes());
    file.extend_from_slice(&crate::schema::GRAPH_SCHEMA_VERSION.to_le_bytes());
    file.extend_from_slice(&(node_rows.len() as u64).to_le_bytes());
    file.extend_from_slice(&(edge_rows.len() as u64).to_le_bytes());
    file.extend_from_slice(&digest_bytes);
    file.extend_from_slice(&offset_nodes.to_le_bytes());
    file.extend_from_slice(&offset_edges.to_le_bytes());
    file.extend_from_slice(&offset_strings.to_le_bytes());
    file.extend_from_slice(&offset_strings_len.to_le_bytes());
    file.extend_from_slice(&[0u8; 4]);
    file.extend_from_slice(&offset_extensions.to_le_bytes());
    debug_assert_eq!(file.len(), HEADER_SIZE);

    file.extend_from_slice(&(name_index_bytes.len() as u64).to_le_bytes());
    file.extend_from_slice(&name_index_bytes);
    file.extend_from_slice(&(type_index_bytes.len() as u64).to_le_bytes());
    file.extend_from_slice(&type_index_bytes);

    for row in node_rows {
        file.extend_from_slice(&encode_node_row(row));
    }
    for row in edge_rows {
        file.extend_from_slice(&encode_edge_row(row));
    }
    file.extend_from_slice(&strings.bytes);
    file.extend_from_slice(extensions_blob);

    std::fs::write(path, file)?;
    Ok(())
}

/// Write a columnar v2 snapshot from owned node/edge vectors (no [`MemoryBackend`]).
///
/// Digests match [`write_columnar_from_backend`]: nodes sorted by id; edges sorted by
/// `(from, to, type)`; BLAKE3 over bincode of each node then each edge.
///
/// Used by discover ingest to avoid dual residency of `GraphBuilder` Vecs + HashMap backend.
pub fn write_columnar_from_nodes_edges(
    mut nodes: Vec<Node>,
    mut edges: Vec<Edge>,
    path: &Path,
) -> Result<String> {
    nodes.sort_by_key(|n| n.id);
    edges.sort_by(|a, b| {
        (a.from, a.to, edge_type_to_u8(a.edge_type)).cmp(&(
            b.from,
            b.to,
            edge_type_to_u8(b.edge_type),
        ))
    });

    let mut hasher = blake3::Hasher::new();
    let mut strings = StringPool::new();
    let mut node_rows = Vec::with_capacity(nodes.len());
    let mut extensions_blob = Vec::new();
    let mut name_index: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut type_index: HashMap<NodeType, Vec<Uuid>> = HashMap::new();
    let mut scratch = Vec::with_capacity(512);

    for node in &nodes {
        append_node_columnar(
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
    drop(nodes);

    let mut edge_rows = Vec::with_capacity(edges.len());
    for edge in &edges {
        let bytes = edge_digest_bytes(edge)?;
        hasher.update(&bytes);
        edge_rows.push(EdgeRow {
            from: *edge.from.as_bytes(),
            to: *edge.to.as_bytes(),
            edge_type: edge_type_to_u8(edge.edge_type),
            _pad: [0; 7],
        });
    }
    drop(edges);

    let content_digest = hasher.finalize().to_hex().to_string();
    write_columnar_assembled(
        path,
        &node_rows,
        &edge_rows,
        &strings,
        &extensions_blob,
        &name_index,
        &type_index,
        &content_digest,
    )?;
    Ok(content_digest)
}

/// Write a columnar v2 snapshot directly from a live [`MemoryBackend`].
///
/// Unlike [`PreparedGraphSnapshot::from_backend`], this does **not** allocate a second
/// full `Vec<Node>` / `Vec<Edge>` clone. Nodes/edges are encoded while holding backend
/// read locks. Returns a stable BLAKE3 hex digest (nodes sorted by id; edges sorted by
/// `(from, to, type)`).
pub fn write_columnar_from_backend(backend: &MemoryBackend, path: &Path) -> Result<String> {
    let mut ids = backend.all_node_ids()?;
    ids.sort_unstable();

    let mut hasher = blake3::Hasher::new();
    let mut strings = StringPool::new();
    let mut node_rows = Vec::with_capacity(ids.len());
    let mut extensions_blob = Vec::new();
    let mut name_index: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut type_index: HashMap<NodeType, Vec<Uuid>> = HashMap::new();
    let mut scratch = Vec::with_capacity(512);

    backend.for_each_node_by_ids(&ids, |node| {
        append_node_columnar(
            node,
            &mut hasher,
            &mut strings,
            &mut extensions_blob,
            &mut name_index,
            &mut type_index,
            &mut node_rows,
            &mut scratch,
        )?;
        Ok(())
    })?;

    let mut edge_meta: Vec<(Uuid, Uuid, EdgeType)> = Vec::with_capacity(backend.edge_count());
    backend.for_each_edge(|edge| {
        edge_meta.push((edge.from, edge.to, edge.edge_type));
    })?;
    edge_meta.sort_by_key(|a| (a.0, a.1, edge_type_to_u8(a.2)));

    let mut edge_rows = Vec::with_capacity(edge_meta.len());
    for (from, to, edge_type) in &edge_meta {
        let canonical = Edge::new(*from, *to, *edge_type);
        hasher.update(&edge_digest_bytes(&canonical)?);
        edge_rows.push(EdgeRow {
            from: *from.as_bytes(),
            to: *to.as_bytes(),
            edge_type: edge_type_to_u8(*edge_type),
            _pad: [0; 7],
        });
    }
    drop(edge_meta);

    let content_digest = hasher.finalize().to_hex().to_string();
    write_columnar_assembled(
        path,
        &node_rows,
        &edge_rows,
        &strings,
        &extensions_blob,
        &name_index,
        &type_index,
        &content_digest,
    )?;
    Ok(content_digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::GraphBackend;
    use crate::schema::{EdgeType, NodeType};
    use crate::snapshot::PreparedGraphSnapshot;
    use tempfile::TempDir;

    #[test]
    fn columnar_round_trip_and_open_without_full_materialize() {
        let mut backend = crate::backend::MemoryBackend::new();
        let n = Node::new(NodeType::Function, "main").with_file_path("main.rs");
        let id = n.id;
        backend.insert_node(n).unwrap();
        backend
            .insert_edge(Edge::new(id, id, EdgeType::Calls))
            .unwrap();

        let prepared = PreparedGraphSnapshot::from_backend(&backend).unwrap();
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("graph.snapshot.bin");
        prepared.write_columnar_to_path(&path).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        assert_eq!(col.node_count(), 1);
        assert_eq!(col.edge_count(), 1);
        assert_eq!(col.content_digest(), prepared.content_digest);
        assert!(col.name_index().unwrap().contains_key("main"));
        assert_eq!(col.find_nodes_by_name("main").unwrap().len(), 1);

        let loaded = col.to_prepared().unwrap();
        assert_eq!(loaded.nodes[0].name, "main");
    }

    #[test]
    fn columnar_name_index_lookup_without_prepared() {
        let mut backend = crate::backend::MemoryBackend::new();
        let n = Node::new(NodeType::Function, "lookup_me");
        backend.insert_node(n).unwrap();
        let prepared = PreparedGraphSnapshot::from_backend(&backend).unwrap();
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("graph.snapshot.bin");
        prepared.write_columnar_to_path(&path).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        assert_eq!(col.find_nodes_by_name("lookup_me").unwrap().len(), 1);
        assert!(!col.name_index().unwrap().is_empty());
    }

    #[test]
    fn write_columnar_from_backend_stable_digest_no_prepared_clone() {
        let mut backend = crate::backend::MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let a_id = a.id;
        let b_id = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend
            .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
            .unwrap();

        let tmp = TempDir::new().unwrap();
        let path1 = tmp.path().join("g1.bin");
        let path2 = tmp.path().join("g2.bin");
        let d1 = write_columnar_from_backend(&backend, &path1).unwrap();
        let d2 = write_columnar_from_backend(&backend, &path2).unwrap();
        assert_eq!(d1, d2);
        assert_eq!(d1.len(), 64); // blake3 hex

        let file = std::fs::File::open(&path1).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        assert_eq!(col.content_digest(), d1);
        assert_eq!(col.node_count(), 2);
        assert_eq!(col.edge_count(), 1);
    }

    #[test]
    fn write_columnar_from_nodes_edges_matches_backend_digest() {
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let a_id = a.id;
        let b_id = b.id;
        let edge = Edge::new(a_id, b_id, EdgeType::Calls);

        let mut backend = crate::backend::MemoryBackend::new();
        backend.insert_node(a.clone()).unwrap();
        backend.insert_node(b.clone()).unwrap();
        backend.insert_edge(edge.clone()).unwrap();

        let tmp = TempDir::new().unwrap();
        let path_backend = tmp.path().join("backend.bin");
        let path_vecs = tmp.path().join("vecs.bin");
        let d_backend = write_columnar_from_backend(&backend, &path_backend).unwrap();
        let d_vecs = write_columnar_from_nodes_edges(vec![b, a], vec![edge], &path_vecs).unwrap();
        assert_eq!(d_backend, d_vecs);

        let open = |path: &std::path::Path| {
            let file = std::fs::File::open(path).unwrap();
            // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
            let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
            ColumnarGraphMmap::open(mmap).unwrap()
        };
        let col_b = open(&path_backend);
        let col_v = open(&path_vecs);
        assert_eq!(col_b.node_count(), col_v.node_count());
        assert_eq!(col_b.edge_count(), col_v.edge_count());
        assert_eq!(col_b.content_digest(), col_v.content_digest());
    }

    #[test]
    fn edge_properties_do_not_affect_columnar_digest() {
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let a_id = a.id;
        let b_id = b.id;
        let plain = Edge::new(a_id, b_id, EdgeType::Calls);
        let rich = Edge::new(a_id, b_id, EdgeType::Calls)
            .with_property("call_site_line".into(), "42".into());

        let tmp = TempDir::new().unwrap();
        let d_plain = write_columnar_from_nodes_edges(
            vec![a.clone(), b.clone()],
            vec![plain],
            &tmp.path().join("p.bin"),
        )
        .unwrap();
        let d_rich =
            write_columnar_from_nodes_edges(vec![a, b], vec![rich], &tmp.path().join("r.bin"))
                .unwrap();
        assert_eq!(d_plain, d_rich);
    }

    #[test]
    fn rematerialize_round_trip_matches_header_digest() {
        let a = Node::new(NodeType::Function, "a")
            .with_file_path("a.rs")
            .with_qualified_name("mod::a");
        let b = Node::new(NodeType::Function, "b").with_file_path("b.rs");
        let a_id = a.id;
        let b_id = b.id;
        let e1 = Edge::new(a_id, b_id, EdgeType::Calls)
            .with_property("call_site_line".into(), "10".into());
        let e2 = Edge::new(b_id, a_id, EdgeType::Calls);

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("g.bin");
        let written = write_columnar_from_nodes_edges(vec![a, b], vec![e1, e2], &path).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();

        let mut nodes: Vec<Node> = (0..col.node_count())
            .map(|i| col.materialize_node_at(i).unwrap())
            .collect();
        nodes.sort_by_key(|n| n.id);

        let mut hasher = blake3::Hasher::new();
        for node in &nodes {
            hasher.update(&bincode::serialize(node).unwrap());
        }

        let mut edges: Vec<Edge> = Vec::new();
        col.for_each_edge(|from, to, edge_type| {
            edges.push(Edge::new(from, to, edge_type));
            Ok(())
        })
        .unwrap();
        edges.sort_by(|a, b| {
            (a.from, a.to, edge_type_to_u8(a.edge_type)).cmp(&(
                b.from,
                b.to,
                edge_type_to_u8(b.edge_type),
            ))
        });
        for edge in &edges {
            hasher.update(&bincode::serialize(&edge.for_columnar_digest()).unwrap());
        }

        let recomputed = hasher.finalize().to_hex().to_string();
        assert_eq!(recomputed, written);
        assert_eq!(recomputed, col.content_digest());
    }

    #[test]
    fn annotation_and_permits_columnar_round_trip() {
        use crate::csr::edge_type_to_u8;
        use memmap2::Mmap;
        use std::sync::Arc;
        use tempfile::TempDir;

        let ann =
            Node::new(NodeType::Annotation, "AddOnStartup").with_file_path("AddOnStartup.java");
        let method = Node::new(NodeType::Function, "bar").with_file_path("Foo.java");
        let sealed = Node::new(NodeType::Class, "Shape").with_file_path("Shape.java");
        let circle = Node::new(NodeType::Class, "Circle").with_file_path("Circle.java");
        let ann_id = ann.id;
        let method_id = method.id;
        let sealed_id = sealed.id;
        let circle_id = circle.id;
        let annotated = Edge::new(method_id, ann_id, EdgeType::AnnotatedWith);
        let permits = Edge::new(sealed_id, circle_id, EdgeType::Permits);

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("ann.bin");
        write_columnar_from_nodes_edges(
            vec![ann, method, sealed, circle],
            vec![annotated, permits],
            &path,
        )
        .unwrap();

        let file = std::fs::File::open(&path).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        let types: Vec<_> = (0..col.node_count())
            .map(|i| col.materialize_node_at(i).unwrap().node_type)
            .collect();
        assert!(types.contains(&NodeType::Annotation));

        let mut edges = Vec::new();
        col.for_each_edge(|_f, _t, et| {
            edges.push(et);
            Ok(())
        })
        .unwrap();
        assert!(edges.contains(&EdgeType::AnnotatedWith));
        assert!(edges.contains(&EdgeType::Permits));
        assert_eq!(edge_type_to_u8(EdgeType::AnnotatedWith), 28);
        assert_eq!(edge_type_to_u8(EdgeType::Permits), 29);
        assert_eq!(node_type_to_u16(NodeType::Annotation), 35);
    }

    #[test]
    #[ignore = "manual: timing comparison for columnar open vs hydrate"]
    fn columnar_open_vs_hydrate_timing() {
        let mut backend = crate::backend::MemoryBackend::new();
        for i in 0..1000 {
            backend
                .insert_node(Node::new(NodeType::Function, format!("fn{i}")))
                .unwrap();
        }
        let prepared = PreparedGraphSnapshot::from_backend(&backend).unwrap();
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("graph.snapshot.bin");
        prepared.write_columnar_to_path(&path).unwrap();

        let open_start = std::time::Instant::now();
        let file = std::fs::File::open(&path).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        let open_elapsed = open_start.elapsed();

        let hydrate_start = std::time::Instant::now();
        let _backend = col.to_prepared().unwrap().hydrate_backend().unwrap();
        let hydrate_elapsed = hydrate_start.elapsed();

        eprintln!(
            "columnar open: {:?}, full hydrate: {:?} (nodes={})",
            open_elapsed,
            hydrate_elapsed,
            col.node_count()
        );
        assert!(open_elapsed <= hydrate_elapsed);
    }

    #[test]
    fn string_pool_deduplicates_identical_strings() {
        let mut pool = StringPool::new();
        let a = pool.intern("src/main.rs");
        let b = pool.intern("src/main.rs");
        assert_eq!(a.off, b.off);
        assert_eq!(a.len, b.len);
        assert!(pool.bytes.len() < "src/main.rs".len() * 2);
    }

    #[test]
    fn read_node_row_rejects_out_of_range() {
        let mmap = vec![0u8; 32];
        assert!(read_node_row(&mmap, 0, 0).is_err());
    }

    /// Pre-#93 writers emitted `HashMap` property pairs in arbitrary order.
    /// Build a current-schema extension blob with reverse-lex key order (not
    /// `BTreeMap` order) to stand in for those on-disk bytes.
    fn legacy_unsorted_metric_extension_bytes() -> Vec<u8> {
        use serde::ser::SerializeMap;
        use serde::{Serialize, Serializer};

        struct UnsortedProps(&'static [(&'static str, &'static str)]);

        impl Serialize for UnsortedProps {
            fn serialize<S: Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(self.0.len()))?;
                for &(k, v) in self.0 {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }

        #[derive(Serialize)]
        struct LegacyUnsortedExtension {
            qualified_name: Option<String>,
            return_type: Option<String>,
            code_hash: Option<String>,
            token_bloom: Option<[u64; 4]>,
            parameters: Vec<crate::schema::GraphParameter>,
            properties: UnsortedProps,
            labels: Vec<String>,
        }

        // Sorted order would be: cognitive, cyclomatic, loc, nesting_depth.
        // Emit the reverse so the blob cannot match a post-fix BTreeMap encode.
        let ext = LegacyUnsortedExtension {
            qualified_name: None,
            return_type: None,
            code_hash: None,
            token_bloom: None,
            parameters: Vec::new(),
            properties: UnsortedProps(&[
                ("nesting_depth", "4"),
                ("loc", "3"),
                ("cyclomatic", "1"),
                ("cognitive", "2"),
            ]),
            labels: Vec::new(),
        };
        bincode::serialize(&ext).expect("serialize legacy unsorted extension")
    }

    #[test]
    fn legacy_unsorted_extension_properties_decode_and_materialize() {
        let legacy_bytes = legacy_unsorted_metric_extension_bytes();

        // Golden pin: reverse-lex metric properties on an otherwise-empty
        // NodeExtension (current schema with `token_bloom`). Stand-in for a
        // pre-#93 HashMap iteration order. Layout (bincode):
        //   4× Option::None (1 byte each) + empty parameters (u64 len 0)
        //   + map{nesting_depth,loc,cyclomatic,cognitive} + empty labels.
        const GOLDEN_HEX: &str = "00000000000000000000000004000000000000000d000000000000006e657374696e675f646570746801000000000000003403000000000000006c6f630100000000000000330a000000000000006379636c6f6d617469630100000000000000310900000000000000636f676e69746976650100000000000000320000000000000000";
        let golden: Vec<u8> = (0..GOLDEN_HEX.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&GOLDEN_HEX[i..i + 2], 16).expect("hex"))
            .collect();
        assert_eq!(
            legacy_bytes, golden,
            "legacy unsorted extension bytes drifted; update GOLDEN_HEX if intentional"
        );

        let decoded = decode_node_extension(&legacy_bytes).expect("decode legacy extension");
        assert_eq!(decoded.properties.get("cyclomatic").map(String::as_str), Some("1"));
        assert_eq!(decoded.properties.get("cognitive").map(String::as_str), Some("2"));
        assert_eq!(decoded.properties.get("loc").map(String::as_str), Some("3"));
        assert_eq!(
            decoded.properties.get("nesting_depth").map(String::as_str),
            Some("4")
        );

        // Rewriting with BTreeMap yields sorted keys — different bytes, same props.
        let rewritten = bincode::serialize(&decoded).expect("re-serialize");
        assert_ne!(rewritten, legacy_bytes);
        let redecoded = decode_node_extension(&rewritten).expect("decode rewritten");
        assert_eq!(
            redecoded.properties.to_hashmap(),
            decoded.properties.to_hashmap()
        );

        // Full path: inject legacy extension bytes into a columnar snapshot and materialize.
        let node = Node::new(NodeType::Function, "legacy_fn").with_file_path("legacy.rs");
        let node_bytes = bincode::serialize(&node).expect("node bytes");
        let mut hasher = blake3::Hasher::new();
        let mut strings = StringPool::new();
        let mut extensions_blob = Vec::new();
        let mut name_index = HashMap::new();
        let mut type_index = HashMap::new();
        let mut node_rows = Vec::new();
        append_node_columnar_prehashed(
            &node,
            &node_bytes,
            &mut hasher,
            &mut strings,
            &mut extensions_blob,
            &mut name_index,
            &mut type_index,
            &mut node_rows,
            Some(&legacy_bytes),
        )
        .expect("append with legacy extension");
        let digest = hasher.finalize().to_hex().to_string();

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("legacy.bin");
        write_columnar_assembled(
            &path,
            &node_rows,
            &[],
            &strings,
            &extensions_blob,
            &name_index,
            &type_index,
            &digest,
        )
        .expect("write snapshot");

        let file = std::fs::File::open(&path).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        let materialized = col.materialize_node_at(0).expect("materialize");
        assert_eq!(materialized.get_property("cyclomatic"), Some("1"));
        assert_eq!(materialized.get_property("cognitive"), Some("2"));
        assert_eq!(materialized.get_property("loc"), Some("3"));
        assert_eq!(materialized.get_property("nesting_depth"), Some("4"));
    }

    #[test]
    fn write_columnar_stable_digest_with_multi_property_nodes() {
        let mk_nodes = || {
            (0..20)
                .map(|i| {
                    Node::new(NodeType::Function, format!("fn_{i}"))
                        .with_file_path("src/lib.rs")
                        .with_property("cyclomatic".into(), (i % 5).to_string())
                        .with_property("cognitive".into(), (i % 3).to_string())
                        .with_property("loc".into(), (i * 10).to_string())
                        .with_property("nesting_depth".into(), "1".into())
                })
                .collect::<Vec<_>>()
        };

        let tmp = TempDir::new().unwrap();
        let p1 = tmp.path().join("s1.bin");
        let p2 = tmp.path().join("s2.bin");

        let d1 = write_columnar_from_nodes_edges(mk_nodes(), vec![], &p1).unwrap();
        let d2 = write_columnar_from_nodes_edges(mk_nodes(), vec![], &p2).unwrap();
        assert_eq!(
            d1, d2,
            "content_digest must match across runs for nodes with properties"
        );

        // Name/type indexes are still HashMaps on the wire, so full-file bytes
        // are not guaranteed identical — only the content digest is.
        let open = |path: &std::path::Path| {
            let file = std::fs::File::open(path).unwrap();
            // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
            let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
            ColumnarGraphMmap::open(mmap).unwrap()
        };
        let c1 = open(&p1);
        let c2 = open(&p2);
        assert_eq!(c1.node_count(), c2.node_count());
        for i in 0..c1.node_count() {
            let d_a = crate::stable_key::node_row_ref(&c1, i)
                .unwrap()
                .extension_digest;
            let d_b = crate::stable_key::node_row_ref(&c2, i)
                .unwrap()
                .extension_digest;
            assert_eq!(d_a, d_b, "extension_digest mismatch at node {i}");
        }
    }

    #[test]
    fn write_columnar_backend_and_nodes_edges_match_with_properties() {
        let n = Node::new(NodeType::Function, "compute")
            .with_file_path("main.rs")
            .with_property("loc".into(), "50".into())
            .with_property("cyclomatic".into(), "4".into())
            .with_property("cognitive".into(), "2".into())
            .with_property("nesting_depth".into(), "1".into());

        let mut backend = crate::backend::MemoryBackend::new();
        backend.insert_node(n.clone()).unwrap();

        let tmp = TempDir::new().unwrap();
        let d_backend =
            write_columnar_from_backend(&backend, &tmp.path().join("backend.bin")).unwrap();
        let d_vecs =
            write_columnar_from_nodes_edges(vec![n], vec![], &tmp.path().join("vecs.bin")).unwrap();
        assert_eq!(
            d_backend, d_vecs,
            "Backend and Vecs ingest must produce matching digests with properties"
        );
    }

    #[test]
    fn rematerialize_round_trip_stable_with_multi_property_nodes() {
        let a = Node::new(NodeType::Function, "a")
            .with_file_path("a.rs")
            .with_property("cyclomatic".into(), "3".into())
            .with_property("cognitive".into(), "2".into())
            .with_property("loc".into(), "40".into())
            .with_property("nesting_depth".into(), "1".into());
        let b = Node::new(NodeType::Function, "b")
            .with_file_path("b.rs")
            .with_property("cyclomatic".into(), "1".into())
            .with_property("cognitive".into(), "1".into())
            .with_property("loc".into(), "10".into())
            .with_property("nesting_depth".into(), "0".into());

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("g.bin");
        let written = write_columnar_from_nodes_edges(vec![a, b], vec![], &path).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        // SAFETY: test file is read-only; mapping covers the written snapshot bytes only.
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();

        let mut nodes: Vec<Node> = (0..col.node_count())
            .map(|i| col.materialize_node_at(i).unwrap())
            .collect();
        nodes.sort_by_key(|n| n.id);

        let rewritten_path = tmp.path().join("g2.bin");
        let rewritten = write_columnar_from_nodes_edges(nodes, vec![], &rewritten_path).unwrap();
        assert_eq!(
            written, rewritten,
            "rematerialize + rewrite must preserve content_digest when properties are present"
        );
    }

    #[test]
    fn deserialize_legacy_hashmap_extension_properties() {
        #[derive(Serialize)]
        struct LegacyNodeExtension {
            qualified_name: Option<String>,
            return_type: Option<String>,
            code_hash: Option<String>,
            token_bloom: Option<[u64; 4]>,
            parameters: Vec<crate::schema::GraphParameter>,
            properties: HashMap<String, String>,
            labels: Vec<String>,
        }

        let mut map = HashMap::new();
        map.insert("loc".into(), "42".into());
        map.insert("cyclomatic".into(), "3".into());

        let legacy = LegacyNodeExtension {
            qualified_name: None,
            return_type: None,
            code_hash: None,
            token_bloom: None,
            parameters: Vec::new(),
            properties: map,
            labels: Vec::new(),
        };
        let legacy_bytes = bincode::serialize(&legacy).unwrap();
        let ext = decode_node_extension(&legacy_bytes).expect("must decode legacy map bytes");
        assert_eq!(ext.properties.get("loc").map(String::as_str), Some("42"));
        assert_eq!(
            ext.properties.get("cyclomatic").map(String::as_str),
            Some("3")
        );
    }

    #[test]
    fn decode_node_extension_handles_truncated_bytes_gracefully() {
        let corrupt_bytes = [0xFF, 0xFE, 0x00, 0x12];
        let result = decode_node_extension(&corrupt_bytes);
        assert!(result.is_err(), "Must return Err on corrupt payload");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("node extension") || err.to_lowercase().contains("serde"),
            "expected serde-style error, got: {err}"
        );
    }

    #[test]
    fn decode_node_extension_handles_empty_slice_gracefully() {
        let result = decode_node_extension(&[]);
        assert!(result.is_err(), "empty slice must not panic");
    }
}
