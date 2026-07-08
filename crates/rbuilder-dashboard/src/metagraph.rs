//! Package-level metagraph for Phase 2 community / macro visualization.

use crate::communities::{summarize_communities, CommunitiesPayload, COMMUNITIES_FILE};
use rbuilder_analysis::community::CommunityDetector;
use rbuilder_analysis::graph_utils::PetGraphView;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::path::Path;
use uuid::Uuid;

pub const METAGRAPH_SCHEMA_VERSION: u32 = 3;
pub const METAGRAPH_FILE: &str = "metagraph.json";
/// Above this raw node count the UI hides per-function nodes (community-only mode).
pub const COMMUNITY_ONLY_THRESHOLD: u64 = 50_000;
pub const MAX_METANODES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetagraphPayload {
    pub schema_version: u32,
    pub mode: String,
    pub community_only: bool,
    pub threshold_community_only: u64,
    pub source_node_count: u64,
    pub nodes: Vec<Metanode>,
    pub edges: Vec<Metaedge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metanode {
    pub id: u32,
    pub label: String,
    pub size: u32,
    pub functions: u32,
    pub classes: u32,
    pub avg_complexity: f64,
    pub x: f64,
    pub y: f64,
    /// Columnar row indices in `graph_payload.bin` for WASM LOD drill-down.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub member_indices: Vec<u32>,
    /// Louvain community id (majority vote of member nodes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_id: Option<usize>,
}

/// Metagraph plus community summary written beside the bundle.
#[derive(Debug, Clone)]
pub struct MetagraphExport {
    pub meta: MetagraphPayload,
    pub communities: CommunitiesPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metaedge {
    pub source: u32,
    pub target: u32,
    pub weight: u32,
    pub kind: String,
}

struct PackageBucket {
    label: String,
    functions: u32,
    classes: u32,
    complexity_sum: f64,
    complexity_count: u32,
    member_indices: Vec<u32>,
    community_votes: HashMap<usize, u32>,
}

/// Build package metagraph from indexed graph and write JSON beside the dashboard bundle.
pub fn write_metagraph(
    backend: &MemoryBackend,
    snapshot_path: &Path,
    out_dir: &Path,
    source_node_count: u64,
) -> Result<MetagraphExport, String> {
    let uuid_to_col = load_uuid_index_map(snapshot_path);
    let mut uuid_to_pkg: HashMap<Uuid, u32> = HashMap::new();
    let mut packages: HashMap<String, PackageBucket> = HashMap::new();

    let (community_assignments, modularity) = detect_node_communities(backend)?;

    let _ = backend.for_each_node(|n| {
        if !matches!(n.node_type, NodeType::Function | NodeType::Class) {
            return;
        }
        let label = package_label(n.file_path.as_deref().unwrap_or(""));
        let bucket = packages
            .entry(label.clone())
            .or_insert_with(|| PackageBucket {
                label: label.clone(),
                functions: 0,
                classes: 0,
                complexity_sum: 0.0,
                complexity_count: 0,
                member_indices: Vec::new(),
                community_votes: HashMap::new(),
            });
        if let Some(cid) = community_assignments.get(&n.id) {
            *bucket.community_votes.entry(*cid).or_insert(0) += 1;
        }
        match n.node_type {
            NodeType::Function => bucket.functions += 1,
            NodeType::Class => bucket.classes += 1,
            _ => {}
        }
        if let Some(c) = n
            .properties
            .get("cyclomatic")
            .and_then(|v| v.parse::<f64>().ok())
        {
            bucket.complexity_sum += c;
            bucket.complexity_count += 1;
        }
        if let Some(col_idx) = uuid_to_col.get(&n.id) {
            bucket.member_indices.push(*col_idx);
        }
    });

    if packages.is_empty() {
        packages.insert(
            "default".into(),
            PackageBucket {
                label: "default".into(),
                functions: 0,
                classes: 0,
                complexity_sum: 0.0,
                complexity_count: 0,
                member_indices: Vec::new(),
                community_votes: HashMap::new(),
            },
        );
    }

    let mut ranked: Vec<_> = packages.into_values().collect();
    ranked.sort_by_key(|b| std::cmp::Reverse(b.functions + b.classes));

    let tail = if ranked.len() > MAX_METANODES {
        ranked.split_off(MAX_METANODES - 1)
    } else {
        vec![]
    };
    let top = ranked;

    let mut metanodes: Vec<Metanode> = Vec::new();
    let mut label_to_id: HashMap<String, u32> = HashMap::new();

    for (idx, bucket) in top.into_iter().enumerate() {
        let id = idx as u32;
        label_to_id.insert(bucket.label.clone(), id);
        metanodes.push(bucket_to_metanode(id, bucket));
    }

    if !tail.is_empty() {
        let id = metanodes.len() as u32;
        let mut merged = PackageBucket {
            label: "(other)".into(),
            functions: 0,
            classes: 0,
            complexity_sum: 0.0,
            complexity_count: 0,
            member_indices: Vec::new(),
            community_votes: HashMap::new(),
        };
        for b in &tail {
            for (cid, votes) in &b.community_votes {
                *merged.community_votes.entry(*cid).or_insert(0) += votes;
            }
        }
        for b in tail {
            merged.functions += b.functions;
            merged.classes += b.classes;
            merged.complexity_sum += b.complexity_sum;
            merged.complexity_count += b.complexity_count;
            merged.member_indices.extend(b.member_indices);
        }
        label_to_id.insert(merged.label.clone(), id);
        metanodes.push(bucket_to_metanode(id, merged));
    }

    let _ = backend.for_each_node(|n| {
        if !matches!(n.node_type, NodeType::Function | NodeType::Class) {
            return;
        }
        let label = package_label(n.file_path.as_deref().unwrap_or(""));
        let pkg_id = *label_to_id
            .get(&label)
            .or_else(|| label_to_id.get("(other)"))
            .unwrap_or(&0);
        uuid_to_pkg.insert(n.id, pkg_id);
    });

    let mut edge_weights: HashMap<(u32, u32), u32> = HashMap::new();
    let _ = backend.for_each_edge(|e| {
        if e.edge_type != EdgeType::Calls {
            return;
        }
        let Some(&from) = uuid_to_pkg.get(&e.from) else {
            return;
        };
        let Some(&to) = uuid_to_pkg.get(&e.to) else {
            return;
        };
        if from == to {
            return;
        }
        *edge_weights.entry((from, to)).or_insert(0) += 1;
    });

    let edges: Vec<Metaedge> = edge_weights
        .into_iter()
        .map(|((source, target), weight)| Metaedge {
            source,
            target,
            weight,
            kind: "calls".into(),
        })
        .collect();

    layout_circle(&mut metanodes);

    let community_only = source_node_count >= COMMUNITY_ONLY_THRESHOLD;
    let payload = MetagraphPayload {
        schema_version: METAGRAPH_SCHEMA_VERSION,
        mode: if community_only {
            "community_only".into()
        } else {
            "package_metagraph".into()
        },
        community_only,
        threshold_community_only: COMMUNITY_ONLY_THRESHOLD,
        source_node_count,
        nodes: metanodes,
        edges,
    };

    let communities = summarize_communities(modularity, &payload.nodes);

    let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs_write(out_dir.join(METAGRAPH_FILE), json.as_bytes())?;

    let communities_json =
        serde_json::to_string_pretty(&communities).map_err(|e| e.to_string())?;
    fs_write(out_dir.join(COMMUNITIES_FILE), communities_json.as_bytes())?;

    Ok(MetagraphExport {
        meta: payload,
        communities,
    })
}

fn detect_node_communities(
    backend: &MemoryBackend,
) -> Result<(HashMap<Uuid, usize>, f64), String> {
    let view = PetGraphView::from_backend(backend).map_err(|e| e.to_string())?;
    let result = CommunityDetector::new()
        .detect_with_view(&view)
        .map_err(|e| e.to_string())?;
    Ok((result.assignments, result.modularity))
}

fn bucket_to_metanode(id: u32, bucket: PackageBucket) -> Metanode {
    let size = bucket.functions + bucket.classes;
    let avg_complexity = if bucket.complexity_count > 0 {
        bucket.complexity_sum / bucket.complexity_count as f64
    } else {
        0.0
    };
    let community_id = bucket
        .community_votes
        .into_iter()
        .max_by_key(|(_, votes)| *votes)
        .map(|(cid, _)| cid);
    Metanode {
        id,
        label: bucket.label,
        size,
        functions: bucket.functions,
        classes: bucket.classes,
        avg_complexity,
        x: 0.0,
        y: 0.0,
        member_indices: bucket.member_indices,
        community_id,
    }
}

fn layout_circle(nodes: &mut [Metanode]) {
    let n = nodes.len().max(1) as f64;
    let radius = (n * 12.0).sqrt().max(40.0);
    for (i, node) in nodes.iter_mut().enumerate() {
        let angle = 2.0 * PI * i as f64 / n;
        node.x = angle.cos() * radius;
        node.y = angle.sin() * radius;
    }
}

/// Derive a stable package / module label from a source file path.
pub fn package_label(file_path: &str) -> String {
    let path = file_path.replace('\\', "/");
    if let Some(idx) = path.find("/java/") {
        let after = &path[idx + 6..];
        if let Some(parent) = std::path::Path::new(after).parent() {
            let pkg = parent.to_string_lossy().replace('/', ".");
            if !pkg.is_empty() {
                return pkg;
            }
        }
    }
    if let Some(idx) = path.find("/src/") {
        let after = &path[idx + 5..];
        if let Some(parent) = std::path::Path::new(after).parent() {
            let pkg = parent.to_string_lossy().replace('/', ".");
            if !pkg.is_empty() {
                return pkg;
            }
        }
    }
    std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().replace('/', "."))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "root".into())
}

fn fs_write(path: std::path::PathBuf, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

fn load_uuid_index_map(snapshot_path: &Path) -> HashMap<Uuid, u32> {
    if !snapshot_path.is_file() {
        return HashMap::new();
    }
    let Ok(bytes) = std::fs::read(snapshot_path) else {
        return HashMap::new();
    };
    scan_columnar_uuid_indices(&bytes)
        .unwrap_or_default()
        .into_iter()
        .map(|(raw, idx)| (Uuid::from_bytes(raw), idx))
        .collect()
}

/// Scan columnar v2 node id column without pulling in memmap (export-time only).
fn scan_columnar_uuid_indices(bytes: &[u8]) -> Result<HashMap<[u8; 16], u32>, String> {
    const HEADER_SIZE: usize = 136;
    const NODE_ROW_SIZE: usize = 64;
    if bytes.len() < HEADER_SIZE || &bytes[0..4] != b"RBGR" {
        return Err("not columnar v2".into());
    }
    let node_count = u64::from_le_bytes(bytes[12..20].try_into().unwrap()) as usize;
    let offset_nodes = u64::from_le_bytes(bytes[92..100].try_into().unwrap()) as usize;
    let end = offset_nodes + node_count * NODE_ROW_SIZE;
    if end > bytes.len() {
        return Err("node column out of range".into());
    }
    let mut map = HashMap::with_capacity(node_count);
    for idx in 0..node_count {
        let off = offset_nodes + idx * NODE_ROW_SIZE;
        let id: [u8; 16] = bytes[off..off + 16].try_into().unwrap();
        map.insert(id, idx as u32);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_package_label() {
        assert_eq!(
            package_label("src/main/java/com/example/foo/Bar.java"),
            "com.example.foo"
        );
    }

    #[test]
    fn rust_module_label() {
        assert_eq!(
            package_label("src/graph/detection/mod.rs"),
            "src.graph.detection"
        );
    }
}
