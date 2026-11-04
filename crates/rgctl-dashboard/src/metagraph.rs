//! Package-level metagraph for Phase 2 community / macro visualization.

use crate::communities::{COMMUNITIES_FILE, CommunitiesPayload, summarize_communities};
use rgctl_analysis::AnalysisResults;
use rgctl_analysis::community::CommunityDetector;
use rgctl_analysis::graph_utils::PetGraphView;
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{EdgeType, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::path::Path;
use uuid::Uuid;

pub const METAGRAPH_SCHEMA_VERSION: u32 = 3;
pub const METAGRAPH_FILE: &str = "metagraph.json";
/// Member indices exported out of line, as little-endian `u32` — one package's run after another, in
/// metanode order. Present only above [`COMMUNITY_ONLY_THRESHOLD`]; below it they stay inline in
/// `metagraph.json`, where the payload is small enough not to matter and existing consumers read them.
pub const METANODE_MEMBERS_FILE: &str = "metanode_members.bin";
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
    /// Bundle-relative path to the member-index sidecar, when membership was exported out of line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members_path: Option<String>,
    /// Total `u32` member indices in the sidecar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members_total: Option<u32>,
    /// Element format of the sidecar, stated so a reader does not have to infer it from the size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members_format: Option<String>,
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
    ///
    /// Inline only for graphs below [`COMMUNITY_ONLY_THRESHOLD`]. Above it the same indices live in
    /// `metanode_members.bin` and are addressed by `member_offset` / `member_count` — the file is the part
    /// that used to be dropped entirely, which is why a district could not be opened.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub member_indices: Vec<u32>,
    /// Byte offset of this package's run in `metanode_members.bin`.
    ///
    /// Always serialized, including when it is `0`: the first package's run starts at byte zero, and an
    /// elided "0" is indistinguishable from "no field" — which is how a consumer ends up unable to open the
    /// largest district in the bundle while the others work.
    #[serde(default)]
    pub member_offset: u32,
    /// Number of `u32` indices in this package's run. `0` means this package exported no members.
    #[serde(default)]
    pub member_count: u32,
    /// Fast-Leiden community id (majority vote of member nodes).
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
    modules: u32,
    files: u32,
    complexity_sum: f64,
    complexity_count: u32,
    member_indices: Vec<u32>,
    /// Votes from code/doc symbols (`Function`, `Class`, `Module`). Preferred for plurality.
    community_votes: HashMap<usize, u32>,
    /// Votes from `File` nodes — used only when the bucket has no code/doc symbol votes.
    file_community_votes: HashMap<usize, u32>,
}

fn bucket_member_count(bucket: &PackageBucket) -> u32 {
    bucket.functions + bucket.classes + bucket.modules + bucket.files
}

/// Returns `true` when `file_path` is inside a generated build or frontend bundle directory.
fn is_generated_bundle_path(file_path: &str) -> bool {
    let norm = file_path.replace('\\', "/");
    let slash_path = format!("/{}", norm.trim_start_matches('/'));
    slash_path.contains("/dist/")
        || slash_path.contains("/dist-")
        || slash_path.contains("/node_modules/")
        || slash_path.contains("/.next/")
        || slash_path.contains("/target/")
}

/// Node types included in package metagraph buckets (code + doc structure).
fn is_metagraph_member(node: &rgctl_graph::schema::Node) -> bool {
    if let Some(fp) = node.file_path.as_deref() {
        if is_generated_bundle_path(fp) {
            return false;
        }
    }
    match node.node_type {
        NodeType::Function | NodeType::Class | NodeType::File => true,
        NodeType::Module => module_kind_allowed(node.properties.get("kind").map(|s| s.as_str())),
        _ => false,
    }
}

/// Doc `:Module` kinds we bucket; code package modules have no `kind` property.
fn module_kind_allowed(kind: Option<&str>) -> bool {
    matches!(kind, None | Some("heading") | Some("code_block"))
}

fn metagraph_edge_kind(edge_type: EdgeType) -> Option<&'static str> {
    match edge_type {
        EdgeType::Calls => Some("calls"),
        EdgeType::References => Some("references"),
        EdgeType::Uses => Some("uses"),
        _ => None,
    }
}

/// Build package metagraph from indexed graph and write JSON beside the dashboard bundle.
pub fn write_metagraph(
    backend: &MemoryBackend,
    _snapshot_path: &Path,
    out_dir: &Path,
    source_node_count: u64,
    analysis: Option<&AnalysisResults>,
    uuid_to_col: &HashMap<Uuid, u32>,
) -> Result<MetagraphExport, String> {
    let mut uuid_to_pkg: HashMap<Uuid, u32> = HashMap::new();
    let mut packages: HashMap<String, PackageBucket> = HashMap::new();
    let include_member_indices = source_node_count < COMMUNITY_ONLY_THRESHOLD;

    let (modularity, detected_communities) = if analysis.is_some() {
        (
            analysis
                .and_then(|a| a.community.as_ref())
                .map(|c| c.modularity)
                .unwrap_or(0.0),
            None,
        )
    } else {
        let start = std::time::Instant::now();
        let (assignments, modularity) = detect_node_communities(backend)?;
        tracing::info!(
            target: "profile",
            secs = start.elapsed().as_secs_f64(),
            "[profile] write_metagraph community_detect"
        );
        (modularity, Some(assignments))
    };

    let _ = backend.for_each_node(|n| {
        if !is_metagraph_member(n) {
            return;
        }
        let label = package_label(n.file_path.as_deref().unwrap_or(""));
        let bucket = packages
            .entry(label.clone())
            .or_insert_with(|| PackageBucket {
                label: label.clone(),
                functions: 0,
                classes: 0,
                modules: 0,
                files: 0,
                complexity_sum: 0.0,
                complexity_count: 0,
                member_indices: Vec::new(),
                community_votes: HashMap::new(),
                file_community_votes: HashMap::new(),
            });
        if let Some(cid) = community_id_for_node(analysis, detected_communities.as_ref(), n.id) {
            // File nodes lack behavioral Calls/Uses/References edges — do not dilute
            // plurality when the bucket already has code/doc symbol votes.
            if n.node_type == NodeType::File {
                *bucket.file_community_votes.entry(cid).or_insert(0) += 1;
            } else {
                *bucket.community_votes.entry(cid).or_insert(0) += 1;
            }
        }
        match n.node_type {
            NodeType::Function => bucket.functions += 1,
            NodeType::Class => bucket.classes += 1,
            NodeType::Module => bucket.modules += 1,
            NodeType::File => bucket.files += 1,
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
        // Collected unconditionally: above the threshold these go to the sidecar instead of the JSON, which
        // is a serialization choice, not a reason to throw the data away during the walk.
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
                modules: 0,
                files: 0,
                complexity_sum: 0.0,
                complexity_count: 0,
                member_indices: Vec::new(),
                community_votes: HashMap::new(),
                file_community_votes: HashMap::new(),
            },
        );
    }

    let mut ranked: Vec<_> = packages.into_values().collect();
    ranked.sort_by_key(|b| std::cmp::Reverse(bucket_member_count(b)));

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
            modules: 0,
            files: 0,
            complexity_sum: 0.0,
            complexity_count: 0,
            member_indices: Vec::new(),
            community_votes: HashMap::new(),
            file_community_votes: HashMap::new(),
        };
        for b in &tail {
            for (cid, votes) in &b.community_votes {
                *merged.community_votes.entry(*cid).or_insert(0) += votes;
            }
            for (cid, votes) in &b.file_community_votes {
                *merged.file_community_votes.entry(*cid).or_insert(0) += votes;
            }
        }
        for b in tail {
            merged.functions += b.functions;
            merged.classes += b.classes;
            merged.modules += b.modules;
            merged.files += b.files;
            merged.complexity_sum += b.complexity_sum;
            merged.complexity_count += b.complexity_count;
            merged.member_indices.extend(b.member_indices);
        }
        label_to_id.insert(merged.label.clone(), id);
        metanodes.push(bucket_to_metanode(id, merged));
    }

    let _ = backend.for_each_node(|n| {
        if !is_metagraph_member(n) {
            return;
        }
        let label = package_label(n.file_path.as_deref().unwrap_or(""));
        let pkg_id = *label_to_id
            .get(&label)
            .or_else(|| label_to_id.get("(other)"))
            .unwrap_or(&0);
        uuid_to_pkg.insert(n.id, pkg_id);
    });

    let mut edge_weights: HashMap<(u32, u32, &'static str), u32> = HashMap::new();
    let _ = backend.for_each_edge(|e| {
        let Some(kind) = metagraph_edge_kind(e.edge_type) else {
            return;
        };
        let Some(&from) = uuid_to_pkg.get(&e.from) else {
            return;
        };
        let Some(&to) = uuid_to_pkg.get(&e.to) else {
            return;
        };
        if from == to {
            return;
        }
        *edge_weights.entry((from, to, kind)).or_insert(0) += 1;
    });

    let edges: Vec<Metaedge> = edge_weights
        .into_iter()
        .map(|((source, target, kind), weight)| Metaedge {
            source,
            target,
            weight,
            kind: kind.into(),
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
        members_path: None,
        members_total: None,
        members_format: None,
        nodes: metanodes,
        edges,
    };

    let label_map = {
        let mut map = analysis
            .and_then(|a| a.community.as_ref())
            .map(|c| c.labels.clone())
            .unwrap_or_default();
        if map.is_empty() {
            if let Some(a) = analysis {
                let ctx = rgctl_analysis::CommunityQueryContext::from_analysis(a, |uuid| {
                    backend.get_node(uuid).ok().flatten().map(|n| {
                        (
                            n.name.to_string(),
                            n.file_path.as_ref().map(|s| s.to_string()),
                        )
                    })
                });
                map = ctx
                    .communities
                    .into_iter()
                    .map(|c| (c.id, c.label))
                    .collect();
            }
        }
        map
    };
    // Membership: inline for small graphs (unchanged shape for every existing consumer), out of line above
    // the threshold, where it used to be dropped.
    let mut payload = payload;
    let mut members_bytes: Vec<u8> = Vec::new();
    let mut members_total: u32 = 0;
    if include_member_indices {
        for node in payload.nodes.iter_mut() {
            node.member_offset = 0;
            node.member_count = node.member_indices.len() as u32;
        }
    } else {
        for node in payload.nodes.iter_mut() {
            let offset = members_bytes.len() as u32;
            for index in &node.member_indices {
                members_bytes.extend_from_slice(&index.to_le_bytes());
            }
            node.member_offset = offset;
            node.member_count = (members_bytes.len() as u32 - offset) / 4;
            members_total += node.member_count;
            node.member_indices = Vec::new();
        }
        if members_total > 0 {
            payload.members_path = Some(METANODE_MEMBERS_FILE.to_string());
            payload.members_total = Some(members_total);
            payload.members_format = Some("u32le".to_string());
        }
    }

    if !members_bytes.is_empty() {
        let start = std::time::Instant::now();
        fs_write(out_dir.join(METANODE_MEMBERS_FILE), &members_bytes)?;
        tracing::info!(
            target = "profile",
            file = METANODE_MEMBERS_FILE,
            write_secs = start.elapsed().as_secs_f64(),
            bytes = members_bytes.len(),
            indices = members_total,
            "[profile] save_dashboard member index write"
        );
    }

    let communities = summarize_communities(modularity, &payload.nodes, &label_map);

    let metagraph_json = {
        let start = std::time::Instant::now();
        let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
        tracing::info!(
            target: "profile",
            file = METAGRAPH_FILE,
            serialize_secs = start.elapsed().as_secs_f64(),
            json_bytes = json.len(),
            member_index_total = payload
                .nodes
                .iter()
                .map(|n| n.member_indices.len())
                .sum::<usize>(),
            "[profile] save_dashboard json serialize"
        );
        json
    };
    let write_start = std::time::Instant::now();
    fs_write(out_dir.join(METAGRAPH_FILE), metagraph_json.as_bytes())?;
    tracing::info!(
        target: "profile",
        file = METAGRAPH_FILE,
        write_secs = write_start.elapsed().as_secs_f64(),
        "[profile] save_dashboard json write"
    );

    let communities_json = {
        let start = std::time::Instant::now();
        let json = serde_json::to_string_pretty(&communities).map_err(|e| e.to_string())?;
        tracing::info!(
            target: "profile",
            file = COMMUNITIES_FILE,
            serialize_secs = start.elapsed().as_secs_f64(),
            json_bytes = json.len(),
            "[profile] save_dashboard json serialize"
        );
        json
    };
    let write_start = std::time::Instant::now();
    fs_write(out_dir.join(COMMUNITIES_FILE), communities_json.as_bytes())?;
    tracing::info!(
        target: "profile",
        file = COMMUNITIES_FILE,
        write_secs = write_start.elapsed().as_secs_f64(),
        "[profile] save_dashboard json write"
    );

    Ok(MetagraphExport {
        meta: payload,
        communities,
    })
}

fn community_id_for_node(
    analysis: Option<&AnalysisResults>,
    detected: Option<&HashMap<Uuid, usize>>,
    node_id: Uuid,
) -> Option<usize> {
    if let Some(ar) = analysis {
        let compact = ar.get_compact_id(node_id)?;
        return ar.community.as_ref()?.get(compact);
    }
    detected.and_then(|m| m.get(&node_id).copied())
}

fn detect_node_communities(backend: &MemoryBackend) -> Result<(HashMap<Uuid, usize>, f64), String> {
    let view = PetGraphView::from_backend(backend).map_err(|e| e.to_string())?;
    let result = CommunityDetector::new()
        .detect_with_view(&view)
        .map_err(|e| e.to_string())?;
    Ok((result.assignments, result.modularity))
}

fn bucket_to_metanode(id: u32, bucket: PackageBucket) -> Metanode {
    let size = bucket_member_count(&bucket);
    let avg_complexity = if bucket.complexity_count > 0 {
        bucket.complexity_sum / bucket.complexity_count as f64
    } else {
        0.0
    };
    // Prefer code/doc symbol votes; fall back to File votes for doc-only buckets.
    let votes = if !bucket.community_votes.is_empty() {
        bucket.community_votes
    } else {
        bucket.file_community_votes
    };
    let community_id = pick_plurality_community(votes);
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
        member_offset: 0,
        member_count: 0,
        community_id,
    }
}

/// Plurality vote with deterministic tie-break: highest vote wins; on ties, smallest
/// community id wins (`votes_a.cmp(votes_b).then(cid_b.cmp(cid_a))` → min id).
fn pick_plurality_community(votes: HashMap<usize, u32>) -> Option<usize> {
    votes
        .into_iter()
        .max_by(|(cid_a, votes_a), (cid_b, votes_b)| {
            votes_a.cmp(votes_b).then_with(|| cid_b.cmp(cid_a))
        })
        .map(|(cid, _)| cid)
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
    rgctl_analysis::migration::package_label(file_path)
}

fn fs_write(path: std::path::PathBuf, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::schema::{Edge, Node};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn doc_heading(name: &str, file: &str) -> Node {
        Node::new(NodeType::Module, name)
            .with_file_path(file)
            .with_property("kind".to_string(), "heading".to_string())
    }

    /// Above the threshold, membership goes out of line — and the offsets must address exactly what the
    /// package owns.
    ///
    /// This is the data a district needs to open. It used to be dropped entirely above
    /// `COMMUNITY_ONLY_THRESHOLD`, which is why the Architecture room could not expand a package: the engine
    /// `expand_indices` has always been able to draw a member set, but nothing said which nodes belong to
    /// which package. The sidecar is that answer, so these assertions are about arithmetic that has to line
    /// up byte for byte rather than about a field existing.
    #[test]
    fn large_graphs_export_member_indices_out_of_line() {
        let mut backend = MemoryBackend::new();
        let mut uuid_to_col: HashMap<Uuid, u32> = HashMap::new();

        // Two packages' worth of nodes, tagged with the columnar positions the payload would use.
        let mut nodes = Vec::new();
        for (n, path) in [
            "crates/alpha/src/lib.rs",
            "crates/alpha/src/util.rs",
            "crates/beta/src/lib.rs",
        ]
        .iter()
        .enumerate()
        {
            let node = Node::new(NodeType::Function, format!("f{n}")).with_file_path(*path);
            uuid_to_col.insert(node.id, n as u32);
            nodes.push(node);
        }
        for node in nodes {
            backend.insert_node(node).unwrap();
        }

        let dir = tempdir().unwrap();
        let export = write_metagraph(
            &backend,
            Path::new("graph.snapshot.bin"),
            dir.path(),
            COMMUNITY_ONLY_THRESHOLD,
            None,
            &uuid_to_col,
        )
        .unwrap();

        let meta = &export.meta;
        assert!(
            meta.community_only,
            "this fixture must take the out-of-line path"
        );
        assert_eq!(
            meta.members_path.as_deref(),
            Some(METANODE_MEMBERS_FILE),
            "the sidecar path must be declared"
        );
        assert_eq!(meta.members_format.as_deref(), Some("u32le"));

        let total = meta.members_total.expect("members_total") as usize;
        assert_eq!(
            total,
            meta.nodes
                .iter()
                .map(|n| n.member_count as usize)
                .sum::<usize>(),
            "the declared total must be the sum of the packages' runs"
        );
        assert_eq!(total, 3, "every function belongs to exactly one package");

        let bytes = std::fs::read(dir.path().join(METANODE_MEMBERS_FILE)).expect("sidecar");
        assert_eq!(bytes.len(), total * 4, "one u32 per member, no padding");

        // No node may keep an inline copy above the threshold: two sources of truth is the bug this prevents.
        assert!(
            meta.nodes.iter().all(|n| n.member_indices.is_empty()),
            "inline indices must be cleared when the sidecar carries them"
        );

        // The first package's run starts at byte 0. An elided zero here is indistinguishable from a missing
        // field, and the client would fail to open exactly the largest district in the bundle.
        let zero_addressed = meta
            .nodes
            .iter()
            .find(|n| n.member_count > 0)
            .expect("a package with members");
        assert_eq!(
            zero_addressed.member_offset, 0,
            "the first run is at offset zero and must be serialized as such"
        );
        let json = serde_json::to_string(&meta).unwrap();
        assert!(
            json.contains("\"member_offset\": 0") || json.contains("\"member_offset\":0"),
            "a zero offset must appear in the JSON, not be skipped"
        );

        let mut seen: Vec<u32> = Vec::new();
        for node in &meta.nodes {
            assert_eq!(
                node.member_offset % 4,
                0,
                "offsets are byte offsets into a u32 stream"
            );
            assert_eq!(node.member_indices.len(), 0);
            let start = node.member_offset as usize;
            let end = start + node.member_count as usize * 4;
            for chunk in bytes[start..end].chunks_exact(4) {
                seen.push(u32::from_le_bytes(chunk.try_into().unwrap()));
            }
            // A run must not overlap the next package's run.
            assert!(end <= bytes.len(), "run must stay inside the sidecar");
        }
        seen.sort_unstable();
        assert_eq!(
            seen,
            vec![0, 1, 2],
            "every member index appears exactly once"
        );
    }

    #[test]
    fn markdown_only_graph_produces_communities_and_metanodes() {
        let mut backend = MemoryBackend::new();

        let file_a = Node::new(NodeType::File, "content/en/docs/a.md")
            .with_file_path("content/en/docs/a.md");
        let file_b = Node::new(NodeType::File, "content/en/blog/b.md")
            .with_file_path("content/en/blog/b.md");
        let h_a = doc_heading("Alpha", "content/en/docs/a.md");
        let h_b = doc_heading("Beta", "content/en/blog/b.md");
        let h_c = doc_heading("Gamma", "content/en/blog/b.md");

        let id_file_a = file_a.id;
        let id_file_b = file_b.id;
        let id_h_a = h_a.id;
        let id_h_b = h_b.id;
        let id_h_c = h_c.id;

        backend.insert_node(file_a).unwrap();
        backend.insert_node(file_b).unwrap();
        backend.insert_node(h_a).unwrap();
        backend.insert_node(h_b).unwrap();
        backend.insert_node(h_c).unwrap();

        backend
            .insert_edge(Edge::new(id_h_a, id_h_b, EdgeType::References))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_h_b, id_h_c, EdgeType::References))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_h_a, id_file_b, EdgeType::References))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_file_a, id_h_a, EdgeType::Contains))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_file_b, id_h_b, EdgeType::Contains))
            .unwrap();

        let dir = tempdir().unwrap();
        let export = write_metagraph(
            &backend,
            Path::new("graph.snapshot.bin"),
            dir.path(),
            5,
            None,
            &HashMap::new(),
        )
        .unwrap();

        assert!(
            export.meta.nodes.iter().any(|n| n.size > 0),
            "metanodes must count doc modules/files"
        );
        assert!(
            !export.meta.nodes.iter().all(|n| n.label == "default"),
            "expected directory buckets, not placeholder default only"
        );
        assert!(
            export.meta.edges.iter().any(|e| e.kind == "references"),
            "metaedges must include REFERENCES for doc graphs"
        );
        assert!(
            !export.communities.communities.is_empty(),
            "communities.json must list communities for markdown REFERENCES graph"
        );
    }

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
        assert_eq!(
            package_label("/Users/sami/code/rgctl/crates/rgctl-dashboard/src/metagraph.rs"),
            "rgctl-dashboard"
        );
        assert_ne!(
            package_label("crates/a/src/models/m.rs"),
            package_label("crates/b/src/models/m.rs")
        );
    }

    #[test]
    fn generated_bundle_nodes_excluded_from_metagraph() {
        let dist_fn = rgctl_graph::schema::Node::new(NodeType::Function, "minified")
            .with_file_path("/Users/sami/code/rgctl/dashboard/dist/assets/index-abc.js");
        let dist_v2_fn = rgctl_graph::schema::Node::new(NodeType::Function, "minified2")
            .with_file_path("dashboard/dist-v2/assets/app.js");
        let src_fn = rgctl_graph::schema::Node::new(NodeType::Function, "real_fn")
            .with_file_path("dashboard/src/App.tsx");
        assert!(!is_metagraph_member(&dist_fn));
        assert!(!is_metagraph_member(&dist_v2_fn));
        assert!(is_metagraph_member(&src_fn));
    }

    #[test]
    fn markdown_heading_modules_are_metagraph_members() {
        let heading = rgctl_graph::schema::Node::new(NodeType::Module, "Intro")
            .with_property("kind".to_string(), "heading".to_string());
        let link = rgctl_graph::schema::Node::new(NodeType::Import, "link");
        assert!(is_metagraph_member(&heading));
        assert!(!is_metagraph_member(&link));
    }

    #[test]
    fn references_edges_map_to_metagraph_kind() {
        assert_eq!(
            metagraph_edge_kind(EdgeType::References),
            Some("references")
        );
        assert_eq!(metagraph_edge_kind(EdgeType::Calls), Some("calls"));
        assert_eq!(metagraph_edge_kind(EdgeType::Contains), None);
    }

    #[test]
    fn plurality_tie_breaks_to_smallest_community_id() {
        let mut votes = HashMap::new();
        votes.insert(7, 5);
        votes.insert(3, 5);
        assert_eq!(pick_plurality_community(votes), Some(3));
    }
}
