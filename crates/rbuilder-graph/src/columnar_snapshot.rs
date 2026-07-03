//! Columnar mmap graph snapshot (format v2).
//!
//! Hot columns (node ids, types, edge topology) are fixed-width in the mmap.
//! Open parses only the header + small index sections — not the full node/edge vectors.

use crate::schema::{Edge, EdgeType, GraphParameter, Node, NodeType};
use memmap2::Mmap;
use rbuilder_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use crate::snapshot::{PreparedGraphSnapshot, PreparedIndexes, SNAPSHOT_MAGIC};

/// Snapshot file format version for columnar layout.
pub const COLUMNAR_SNAPSHOT_VERSION: u32 = 2;

const HEADER_SIZE: usize = 136;
const NODE_ROW_SIZE: usize = 64;
const EDGE_ROW_SIZE: usize = 40;
const _: () = assert!(std::mem::size_of::<NodeRow>() == NODE_ROW_SIZE);
const _: () = assert!(std::mem::size_of::<EdgeRow>() == EDGE_ROW_SIZE);

/// Per-node cold fields stored as a small bincode blob.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct NodeExtension {
    qualified_name: Option<String>,
    return_type: Option<String>,
    code_hash: Option<String>,
    parameters: Vec<GraphParameter>,
    properties: HashMap<String, String>,
    labels: Vec<String>,
}

/// Fixed-width node column (64 bytes).
#[repr(C)]
struct NodeRow {
    id: [u8; 16],
    node_type: u16,
    _pad: u16,
    name_off: u32,
    name_len: u32,
    file_path_off: u32,
    file_path_len: u32,
    signature_off: u32,
    signature_len: u32,
    start_line: u32,
    end_line: u32,
    extension_off: u32,
    extension_len: u32,
    _pad_end: u32,
}

/// Fixed-width edge column (40 bytes).
#[repr(C)]
struct EdgeRow {
    from: [u8; 16],
    to: [u8; 16],
    edge_type: u8,
    _pad: [u8; 7],
}

/// Parsed columnar snapshot backed by mmap (no full graph deserialize at open).
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
    name_index: HashMap<String, Vec<Uuid>>,
    type_index: HashMap<NodeType, Vec<Uuid>>,
    offset_extensions: u64,
    id_to_index: HashMap<Uuid, usize>,
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
        let (name_index, name_consumed) = read_index_section(tail, 0)?;
        let (type_index, _type_consumed) = read_type_index_section(tail, name_consumed)?;

        let expected_nodes_end = offset_nodes as usize + node_count * NODE_ROW_SIZE;
        let expected_edges_end = offset_edges as usize + edge_count * EDGE_ROW_SIZE;
        if expected_nodes_end > mmap.len() || expected_edges_end > mmap.len() {
            return Err(Error::SerdeError("columnar snapshot column out of range".into()));
        }

        let mut id_to_index = HashMap::with_capacity(node_count);
        for idx in 0..node_count {
            let row = read_node_row(mmap.as_ref(), offset_nodes as usize, idx)?;
            let id = Uuid::from_bytes(row.id);
            id_to_index.insert(id, idx);
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
            name_index,
            type_index,
            offset_extensions,
            id_to_index,
        })
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn node_count(&self) -> usize {
        self.node_count
    }

    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    pub fn content_digest(&self) -> &str {
        &self.digest_hex
    }

    pub fn name_index(&self) -> &HashMap<String, Vec<Uuid>> {
        &self.name_index
    }

    pub fn type_index(&self) -> &HashMap<NodeType, Vec<Uuid>> {
        &self.type_index
    }

    pub fn prepared_indexes(&self) -> PreparedIndexes {
        PreparedIndexes {
            name_index: self.name_index.clone(),
            type_index: self.type_index.clone(),
        }
    }

    pub fn node_ids_by_index(&self) -> impl Iterator<Item = (usize, Uuid)> + '_ {
        self.id_to_index.iter().map(|(id, idx)| (*idx, *id))
    }

    pub fn edge_topology_typed(&self) -> Result<Vec<(Uuid, Uuid, EdgeType)>> {
        let mut out = Vec::with_capacity(self.edge_count);
        for idx in 0..self.edge_count {
            let row = read_edge_row(self.mmap.as_ref(), self.offset_edges as usize, idx)?;
            out.push((
                Uuid::from_bytes(row.from),
                Uuid::from_bytes(row.to),
                edge_type_from_u8(row.edge_type)?,
            ));
        }
        Ok(out)
    }

    pub fn get_node(&self, id: Uuid) -> Result<Option<Node>> {
        let Some(&idx) = self.id_to_index.get(&id) else {
            return Ok(None);
        };
        Ok(Some(self.materialize_node(idx)?))
    }

    pub fn find_nodes_by_name(&self, name: &str) -> Result<Vec<Node>> {
        let Some(ids) = self.name_index.get(name) else {
            return Ok(Vec::new());
        };
        ids.iter().map(|id| self.materialize_node(self.id_to_index[id])).collect()
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
            bincode::deserialize::<NodeExtension>(&self.mmap[start..end])
                .map_err(|e| Error::SerdeError(format!("node extension: {e}")))?
        } else {
            NodeExtension::default()
        };

        Ok(Node {
            id,
            node_type: node_type_from_u16(row.node_type)?,
            name,
            qualified_name: extension.qualified_name,
            signature,
            return_type: extension.return_type,
            parameters: extension.parameters,
            code_hash: extension.code_hash,
            file_path,
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
            indexes: self.prepared_indexes(),
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

        for node in &self.nodes {
            let name_off = strings.intern(&node.name);
            let file_path_off = strings.intern_opt(node.file_path.as_deref());
            let signature_off = strings.intern_opt(node.signature.as_deref());
            let extension = NodeExtension {
                qualified_name: node.qualified_name.clone(),
                return_type: node.return_type.clone(),
                code_hash: node.code_hash.clone(),
                parameters: node.parameters.clone(),
                properties: node.properties.clone(),
                labels: node.labels.clone(),
            };
            let ext_bytes = bincode::serialize(&extension).map_err(bincode_err)?;
            let extension_off = extensions_blob.len() as u32;
            extensions_blob.extend_from_slice(&ext_bytes);

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
                extension_len: ext_bytes.len() as u32,
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
            file.extend_from_slice(as_bytes(row));
        }
        for row in &edge_rows {
            file.extend_from_slice(as_bytes(row));
        }
        file.extend_from_slice(&strings.bytes);
        file.extend_from_slice(&extensions_blob);

        std::fs::write(path, file)?;
        Ok(())
    }
}

struct StringPool {
    bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
struct StrRef {
    off: u32,
    len: u32,
}

impl StringPool {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn intern(&mut self, s: &str) -> StrRef {
        let off = self.bytes.len() as u32;
        let bytes = s.as_bytes();
        self.bytes.extend_from_slice(bytes);
        StrRef {
            off,
            len: bytes.len() as u32,
        }
    }

    fn intern_opt(&mut self, s: Option<&str>) -> StrRef {
        match s {
            Some(v) => self.intern(v),
            None => StrRef { off: 0, len: 0 },
        }
    }
}

fn read_node_row(mmap: &[u8], base: usize, idx: usize) -> Result<NodeRow> {
    let start = base + idx * NODE_ROW_SIZE;
    let end = start + NODE_ROW_SIZE;
    if end > mmap.len() {
        return Err(Error::SerdeError("node row out of range".into()));
    }
    let mut row = NodeRow {
        id: [0; 16],
        node_type: 0,
        _pad: 0,
        name_off: 0,
        name_len: 0,
        file_path_off: 0,
        file_path_len: 0,
        signature_off: 0,
        signature_len: 0,
        start_line: 0,
        end_line: 0,
        extension_off: 0,
        extension_len: 0,
        _pad_end: 0,
    };
    row.id.copy_from_slice(&mmap[start..start + 16]);
    row.node_type = u16::from_le_bytes(mmap[start + 16..start + 18].try_into().unwrap());
    row.name_off = u32::from_le_bytes(mmap[start + 20..start + 24].try_into().unwrap());
    row.name_len = u32::from_le_bytes(mmap[start + 24..start + 28].try_into().unwrap());
    row.file_path_off = u32::from_le_bytes(mmap[start + 28..start + 32].try_into().unwrap());
    row.file_path_len = u32::from_le_bytes(mmap[start + 32..start + 36].try_into().unwrap());
    row.signature_off = u32::from_le_bytes(mmap[start + 36..start + 40].try_into().unwrap());
    row.signature_len = u32::from_le_bytes(mmap[start + 40..start + 44].try_into().unwrap());
    row.start_line = u32::from_le_bytes(mmap[start + 44..start + 48].try_into().unwrap());
    row.end_line = u32::from_le_bytes(mmap[start + 48..start + 52].try_into().unwrap());
    row.extension_off = u32::from_le_bytes(mmap[start + 52..start + 56].try_into().unwrap());
    row.extension_len = u32::from_le_bytes(mmap[start + 56..start + 60].try_into().unwrap());
    Ok(row)
}

fn read_edge_row(mmap: &[u8], base: usize, idx: usize) -> Result<EdgeRow> {
    let start = base + idx * EDGE_ROW_SIZE;
    let end = start + EDGE_ROW_SIZE;
    if end > mmap.len() {
        return Err(Error::SerdeError("edge row out of range".into()));
    }
    let mut row = EdgeRow {
        from: [0; 16],
        to: [0; 16],
        edge_type: 0,
        _pad: [0; 7],
    };
    row.from.copy_from_slice(&mmap[start..start + 16]);
    row.to.copy_from_slice(&mmap[start + 16..start + 32]);
    row.edge_type = mmap[start + 32];
    Ok(row)
}

fn read_string(mmap: &[u8], base: usize, len_limit: usize, off: u32, len: u32) -> Result<String> {
    if len == 0 {
        return Ok(String::new());
    }
    let start = base + off as usize;
    let end = start + len as usize;
    if end > base + len_limit || end > mmap.len() {
        return Err(Error::SerdeError("string pool out of range".into()));
    }
    Ok(std::str::from_utf8(&mmap[start..end])
        .map_err(|e| Error::SerdeError(format!("string utf8: {e}")))?
        .to_string())
}

fn optional_string(
    mmap: &[u8],
    base: usize,
    len_limit: usize,
    off: u32,
    len: u32,
) -> Result<Option<String>> {
    if len == 0 {
        return Ok(None);
    }
    Ok(Some(read_string(mmap, base, len_limit, off, len)?))
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

fn read_type_index_section(tail: &[u8], cursor: usize) -> Result<(HashMap<NodeType, Vec<Uuid>>, usize)> {
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

fn as_bytes<T: Sized>(val: &T) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            (val as *const T) as *const u8,
            std::mem::size_of::<T>(),
        )
    }
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
    }
}

fn node_type_from_u16(v: u16) -> Result<NodeType> {
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
        _ => return Err(Error::SerdeError(format!("unknown node type code {v}"))),
    })
}

fn edge_type_to_u8(t: EdgeType) -> u8 {
    match t {
        EdgeType::Calls => 0,
        EdgeType::Contains => 1,
        EdgeType::Uses => 2,
        EdgeType::Implements => 3,
        EdgeType::Extends => 4,
        EdgeType::References => 5,
        EdgeType::Instantiates => 6,
        EdgeType::Modifies => 7,
        EdgeType::UsesConfig => 8,
        EdgeType::DefinedIn => 9,
        EdgeType::DependsOn => 10,
        EdgeType::IncludesRole => 11,
        EdgeType::DependsOnRole => 12,
        EdgeType::ExecutesTask => 13,
        EdgeType::NotifiesHandler => 14,
        EdgeType::IncludesPlaybook => 15,
        EdgeType::RendersTemplate => 16,
        EdgeType::DependsOnCookbook => 17,
        EdgeType::IncludesRecipe => 18,
        EdgeType::DeclaresResource => 19,
        EdgeType::UsesTemplate => 20,
        EdgeType::DefinesAttribute => 21,
        EdgeType::NotifiesResource => 22,
        EdgeType::DependsOnModule => 23,
        EdgeType::IncludesClass => 24,
        EdgeType::InheritsClass => 25,
        EdgeType::RequiresResource => 26,
        EdgeType::UsesFact => 27,
        EdgeType::Unknown => 255,
    }
}

fn edge_type_from_u8(v: u8) -> Result<EdgeType> {
    Ok(match v {
        0 => EdgeType::Calls,
        1 => EdgeType::Contains,
        2 => EdgeType::Uses,
        3 => EdgeType::Implements,
        4 => EdgeType::Extends,
        5 => EdgeType::References,
        6 => EdgeType::Instantiates,
        7 => EdgeType::Modifies,
        8 => EdgeType::UsesConfig,
        9 => EdgeType::DefinedIn,
        10 => EdgeType::DependsOn,
        11 => EdgeType::IncludesRole,
        12 => EdgeType::DependsOnRole,
        13 => EdgeType::ExecutesTask,
        14 => EdgeType::NotifiesHandler,
        15 => EdgeType::IncludesPlaybook,
        16 => EdgeType::RendersTemplate,
        17 => EdgeType::DependsOnCookbook,
        18 => EdgeType::IncludesRecipe,
        19 => EdgeType::DeclaresResource,
        20 => EdgeType::UsesTemplate,
        21 => EdgeType::DefinesAttribute,
        22 => EdgeType::NotifiesResource,
        23 => EdgeType::DependsOnModule,
        24 => EdgeType::IncludesClass,
        25 => EdgeType::InheritsClass,
        26 => EdgeType::RequiresResource,
        27 => EdgeType::UsesFact,
        255 => EdgeType::Unknown,
        _ => return Err(Error::SerdeError(format!("unknown edge type code {v}"))),
    })
}

fn bincode_err(e: bincode::Error) -> Error {
    Error::SerdeError(format!("columnar snapshot: {e}"))
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
        let n = Node::new(NodeType::Function, "main".into()).with_file_path("main.rs".into());
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
        let mmap = Arc::new(unsafe { Mmap::map(&file).unwrap() });
        let col = ColumnarGraphMmap::open(mmap).unwrap();
        assert_eq!(col.node_count(), 1);
        assert_eq!(col.edge_count(), 1);
        assert_eq!(col.content_digest(), prepared.content_digest);
        assert_eq!(col.find_nodes_by_name("main").unwrap().len(), 1);

        let loaded = col.to_prepared().unwrap();
        assert_eq!(loaded.nodes[0].name, "main");
    }
}
