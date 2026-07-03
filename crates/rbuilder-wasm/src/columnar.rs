//! Minimal columnar v2 reader for browser WASM (Phase 3 LOD + filters).

use serde::Serialize;
use std::collections::{HashMap, HashSet};

const SNAPSHOT_MAGIC: [u8; 4] = *b"RBGR";
const COLUMNAR_VERSION: u32 = 2;
const HEADER_SIZE: usize = 136;
const NODE_ROW_SIZE: usize = 64;
const EDGE_ROW_SIZE: usize = 40;
const CALLS_EDGE: u8 = 0;

/// Bitmask bit for each node type (matches columnar u16 encoding).
pub fn node_type_bit(node_type: u16) -> u32 {
    if node_type < 32 {
        1u32 << node_type
    } else {
        0
    }
}

pub fn node_type_name(node_type: u16) -> &'static str {
    match node_type {
        0 => "Function",
        1 => "Class",
        2 => "Struct",
        3 => "Enum",
        4 => "Interface",
        5 => "Module",
        6 => "Variable",
        7 => "File",
        8 => "ConfigKey",
        9 => "TypeAlias",
        10 => "Macro",
        11 => "Import",
        12 => "Table",
        13 => "Dependency",
        14 => "Job",
        15 => "BuildStep",
        _ => "Other",
    }
}

#[derive(Clone)]
pub struct ColumnarView {
    bytes: Vec<u8>,
    schema_version: u32,
    node_count: usize,
    edge_count: usize,
    offset_nodes: u64,
    offset_edges: u64,
    offset_strings: u64,
    offset_strings_len: u64,
    offset_extensions: u64,
    uuid_to_index: HashMap<[u8; 16], u32>,
}

#[derive(Debug, Serialize)]
pub struct SubgraphNode {
    pub index: u32,
    pub name: String,
    pub node_type: u16,
    pub node_type_name: &'static str,
    pub complexity: f64,
    pub file_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SubgraphEdge {
    pub source: u32,
    pub target: u32,
    pub edge_type: u8,
}

#[derive(Debug, Serialize)]
pub struct SubgraphPayload {
    pub nodes: Vec<SubgraphNode>,
    pub edges: Vec<SubgraphEdge>,
}

#[derive(Debug, Serialize)]
pub struct NodeListEntry {
    pub index: u32,
    pub name: String,
    pub node_type: u16,
    pub node_type_name: &'static str,
    pub complexity: f64,
    pub file_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NodeListPayload {
    pub total: u32,
    pub offset: u32,
    pub items: Vec<NodeListEntry>,
}

impl ColumnarView {
    pub fn open(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() < HEADER_SIZE {
            return Err(format!(
                "payload truncated: {} bytes (need {HEADER_SIZE})",
                bytes.len()
            ));
        }
        if bytes[0..4] != SNAPSHOT_MAGIC {
            return Err("invalid graph payload magic (expected RBGR columnar v2)".into());
        }
        let format_version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if format_version != COLUMNAR_VERSION {
            return Err(format!(
                "unsupported payload format version {format_version} (expected {COLUMNAR_VERSION})"
            ));
        }

        let schema_version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let node_count = u64::from_le_bytes(bytes[12..20].try_into().unwrap()) as usize;
        let edge_count = u64::from_le_bytes(bytes[20..28].try_into().unwrap()) as usize;
        let offset_nodes = u64::from_le_bytes(bytes[92..100].try_into().unwrap());
        let offset_edges = u64::from_le_bytes(bytes[100..108].try_into().unwrap());
        let offset_strings = u64::from_le_bytes(bytes[108..116].try_into().unwrap());
        let offset_strings_len = u64::from_le_bytes(bytes[116..124].try_into().unwrap());
        let offset_extensions = u64::from_le_bytes(bytes[128..136].try_into().unwrap());

        let expected_nodes_end = offset_nodes as usize + node_count * NODE_ROW_SIZE;
        let expected_edges_end = offset_edges as usize + edge_count * EDGE_ROW_SIZE;
        if expected_nodes_end > bytes.len() || expected_edges_end > bytes.len() {
            return Err("columnar snapshot column out of range".into());
        }

        let mut uuid_to_index = HashMap::with_capacity(node_count);
        for idx in 0..node_count {
            let row = read_node_row(&bytes, offset_nodes as usize, idx)?;
            uuid_to_index.insert(row.id, idx as u32);
        }

        Ok(Self {
            bytes,
            schema_version,
            node_count,
            edge_count,
            offset_nodes,
            offset_edges,
            offset_strings,
            offset_strings_len,
            offset_extensions,
            uuid_to_index,
        })
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn node_count(&self) -> u32 {
        self.node_count as u32
    }

    pub fn edge_count(&self) -> u32 {
        self.edge_count as u32
    }

    pub fn digest(&self) -> String {
        std::str::from_utf8(&self.bytes[28..92])
            .unwrap_or("")
            .trim_end_matches('\0')
            .to_string()
    }

    pub fn expand_indices(&self, indices: &[u32], type_mask: u32) -> Result<SubgraphPayload, String> {
        let mut filtered: Vec<u32> = indices
            .iter()
            .copied()
            .filter(|&idx| idx < self.node_count as u32)
            .filter(|&idx| {
                let row = read_node_row(&self.bytes, self.offset_nodes as usize, idx as usize)
                    .expect("bounds checked");
                type_mask == 0 || (node_type_bit(row.node_type) & type_mask) != 0
            })
            .collect();
        filtered.sort_unstable();
        filtered.dedup();

        let set: HashSet<u32> = filtered.iter().copied().collect();
        let mut nodes = Vec::with_capacity(filtered.len());
        for idx in &filtered {
            nodes.push(self.materialize_light(*idx)?);
        }

        let mut edges = Vec::new();
        for edge_idx in 0..self.edge_count {
            let row = read_edge_row(&self.bytes, self.offset_edges as usize, edge_idx)?;
            if row.edge_type != CALLS_EDGE {
                continue;
            }
            let Some(&from) = self.uuid_to_index.get(&row.from) else {
                continue;
            };
            let Some(&to) = self.uuid_to_index.get(&row.to) else {
                continue;
            };
            if set.contains(&from) && set.contains(&to) {
                edges.push(SubgraphEdge {
                    source: from,
                    target: to,
                    edge_type: row.edge_type,
                });
            }
        }

        Ok(SubgraphPayload { nodes, edges })
    }

    pub fn list_nodes(
        &self,
        type_mask: u32,
        offset: u32,
        limit: u32,
    ) -> Result<NodeListPayload, String> {
        let limit = limit.clamp(1, 500);
        let mut total = 0u32;
        for idx in 0..self.node_count as u32 {
            let row = read_node_row(&self.bytes, self.offset_nodes as usize, idx as usize)?;
            if type_mask == 0 || (node_type_bit(row.node_type) & type_mask) != 0 {
                total += 1;
            }
        }

        let mut skip = offset;
        let mut items = Vec::new();
        for idx in 0..self.node_count as u32 {
            let row = read_node_row(&self.bytes, self.offset_nodes as usize, idx as usize)?;
            if type_mask != 0 && (node_type_bit(row.node_type) & type_mask) == 0 {
                continue;
            }
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if items.len() >= limit as usize {
                break;
            }
            let node = self.materialize_light(idx)?;
            items.push(NodeListEntry {
                index: node.index,
                name: node.name,
                node_type: node.node_type,
                node_type_name: node.node_type_name,
                complexity: node.complexity,
                file_path: node.file_path,
            });
        }

        Ok(NodeListPayload {
            total,
            offset,
            items,
        })
    }

    fn materialize_light(&self, idx: u32) -> Result<SubgraphNode, String> {
        let row = read_node_row(&self.bytes, self.offset_nodes as usize, idx as usize)?;
        let name = read_string(
            &self.bytes,
            self.offset_strings as usize,
            self.offset_strings_len,
            row.name_off,
            row.name_len,
        )?;
        let file_path = optional_string(
            &self.bytes,
            self.offset_strings as usize,
            self.offset_strings_len,
            row.file_path_off,
            row.file_path_len,
        )?;
        let complexity = extension_complexity(
            &self.bytes,
            self.offset_extensions as usize,
            row.extension_off,
            row.extension_len,
        )
        .unwrap_or(0.0);

        Ok(SubgraphNode {
            index: idx,
            name,
            node_type: row.node_type,
            node_type_name: node_type_name(row.node_type),
            complexity,
            file_path,
        })
    }
}

struct NodeRow {
    id: [u8; 16],
    node_type: u16,
    name_off: u32,
    name_len: u32,
    file_path_off: u32,
    file_path_len: u32,
    extension_off: u32,
    extension_len: u32,
}

struct EdgeRow {
    from: [u8; 16],
    to: [u8; 16],
    edge_type: u8,
}

fn read_node_row(bytes: &[u8], base: usize, idx: usize) -> Result<NodeRow, String> {
    let off = base + idx * NODE_ROW_SIZE;
    if off + NODE_ROW_SIZE > bytes.len() {
        return Err("node row out of range".into());
    }
    let slice = &bytes[off..off + NODE_ROW_SIZE];
    Ok(NodeRow {
        id: slice[0..16].try_into().unwrap(),
        node_type: u16::from_le_bytes(slice[16..18].try_into().unwrap()),
        name_off: u32::from_le_bytes(slice[20..24].try_into().unwrap()),
        name_len: u32::from_le_bytes(slice[24..28].try_into().unwrap()),
        file_path_off: u32::from_le_bytes(slice[28..32].try_into().unwrap()),
        file_path_len: u32::from_le_bytes(slice[32..36].try_into().unwrap()),
        extension_off: u32::from_le_bytes(slice[48..52].try_into().unwrap()),
        extension_len: u32::from_le_bytes(slice[52..56].try_into().unwrap()),
    })
}

fn read_edge_row(bytes: &[u8], base: usize, idx: usize) -> Result<EdgeRow, String> {
    let off = base + idx * EDGE_ROW_SIZE;
    if off + EDGE_ROW_SIZE > bytes.len() {
        return Err("edge row out of range".into());
    }
    let slice = &bytes[off..off + EDGE_ROW_SIZE];
    Ok(EdgeRow {
        from: slice[0..16].try_into().unwrap(),
        to: slice[16..32].try_into().unwrap(),
        edge_type: slice[32],
    })
}

fn read_string(
    bytes: &[u8],
    pool_base: usize,
    pool_len: u64,
    off: u32,
    len: u32,
) -> Result<String, String> {
    if len == 0 {
        return Ok(String::new());
    }
    let start = pool_base + off as usize;
    let end = start + len as usize;
    if end > pool_base + pool_len as usize || end > bytes.len() {
        return Err("string pool out of range".into());
    }
    std::str::from_utf8(&bytes[start..end])
        .map(|s| s.to_string())
        .map_err(|_| "string pool invalid utf-8".into())
}

fn optional_string(
    bytes: &[u8],
    pool_base: usize,
    pool_len: u64,
    off: u32,
    len: u32,
) -> Result<Option<String>, String> {
    if len == 0 {
        return Ok(None);
    }
    read_string(bytes, pool_base, pool_len, off, len).map(Some)
}

#[derive(serde::Deserialize, Default)]
struct NodeExtension {
    #[serde(default)]
    properties: HashMap<String, String>,
}

fn extension_complexity(
    bytes: &[u8],
    ext_base: usize,
    off: u32,
    len: u32,
) -> Option<f64> {
    if len == 0 {
        return None;
    }
    let start = ext_base + off as usize;
    let end = start + len as usize;
    if end > bytes.len() {
        return None;
    }
    let ext: NodeExtension = bincode::deserialize(&bytes[start..end]).ok()?;
    ext.properties
        .get("cyclomatic")
        .and_then(|v| v.parse::<f64>().ok())
}

/// Build uuid → columnar index map from snapshot bytes (for Rust export side).
pub fn uuid_index_map(bytes: &[u8]) -> Result<HashMap<[u8; 16], u32>, String> {
    Ok(ColumnarView::open(bytes.to_vec())?.uuid_to_index_map().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_type_bits() {
        assert_eq!(node_type_bit(0), 1);
        assert_eq!(node_type_bit(1), 2);
    }
}
