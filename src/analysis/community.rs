//! Community detection
//!
//! Task 2.1.1: Label-propagation community detection with modularity scoring.

use crate::analysis::graph_utils::PetGraphView;
use crate::error::Result;
use crate::graph::backend::GraphBackend;
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::NodeType;
use petgraph::graph::NodeIndex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Community detection engine.
pub struct CommunityDetector {
    max_iterations: usize,
}

/// A detected community.
#[derive(Debug, Clone, PartialEq)]
pub struct Community {
    /// Community identifier
    pub id: usize,
    /// Node UUIDs in this community
    pub members: Vec<Uuid>,
}

/// Result of community detection.
#[derive(Debug, Clone)]
pub struct CommunityResult {
    /// Detected communities
    pub communities: Vec<Community>,
    /// Modularity score (higher is better, typically 0.3-0.7)
    pub modularity: f64,
    /// Node UUID to community ID mapping
    pub assignments: HashMap<Uuid, usize>,
}

impl Default for CommunityDetector {
    fn default() -> Self {
        Self {
            max_iterations: 20,
        }
    }
}

impl CommunityDetector {
    /// Create a new community detector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Detect communities using label propagation (Leiden-like heuristic).
    pub fn detect(&self, backend: &MemoryBackend) -> Result<CommunityResult> {
        let view = PetGraphView::from_backend(backend)?;
        let node_count = view.undirected.node_count();
        if node_count == 0 {
            return Ok(CommunityResult {
                communities: vec![],
                modularity: 0.0,
                assignments: HashMap::new(),
            });
        }

        let mut labels: HashMap<NodeIndex, usize> = view
            .undirected
            .node_indices()
            .enumerate()
            .map(|(i, idx)| (idx, i))
            .collect();

        for _ in 0..self.max_iterations {
            let mut changed = false;
            for node in view.undirected.node_indices() {
                let mut neighbor_counts: HashMap<usize, usize> = HashMap::new();
                for neighbor in view
                    .undirected
                    .neighbors(node)
                    .chain(view.undirected.neighbors_directed(node, petgraph::Direction::Incoming))
                {
                    let label = labels[&neighbor];
                    *neighbor_counts.entry(label).or_default() += 1;
                }
                if let Some((&best_label, _)) = neighbor_counts.iter().max_by_key(|(_, c)| *c) {
                    if labels[&node] != best_label {
                        labels.insert(node, best_label);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        let modularity = self.calculate_modularity(&view, &labels);
        let mut community_members: HashMap<usize, Vec<Uuid>> = HashMap::new();
        for (idx, &label) in &labels {
            if let Some(uuid) = view.undirected.node_weight(*idx) {
                community_members.entry(label).or_default().push(*uuid);
            }
        }

        let communities = community_members
            .into_iter()
            .map(|(id, members)| Community { id, members })
            .collect();

        let assignments = labels
            .iter()
            .filter_map(|(idx, &label)| {
                view.undirected
                    .node_weight(*idx)
                    .map(|uuid| (*uuid, label))
            })
            .collect();

        Ok(CommunityResult {
            communities,
            modularity,
            assignments,
        })
    }

    /// Calculate modularity for a partition.
    pub fn calculate_modularity(&self, view: &PetGraphView, labels: &HashMap<NodeIndex, usize>) -> f64 {
        let m = view.undirected.edge_count() as f64;
        if m == 0.0 {
            return 0.0;
        }

        let mut community_nodes: HashMap<usize, HashSet<NodeIndex>> = HashMap::new();
        for (&idx, &label) in labels {
            community_nodes.entry(label).or_default().insert(idx);
        }

        let mut q = 0.0;
        for members in community_nodes.values() {
            let mut internal = 0.0;
            let mut degree_sum = 0.0;
            for &node in members {
                let degree = view.undirected.neighbors(node).count() as f64;
                degree_sum += degree;
                for neighbor in view.undirected.neighbors(node) {
                    if members.contains(&neighbor) {
                        internal += 1.0;
                    }
                }
            }
            internal /= 2.0;
            let expected = (degree_sum * degree_sum) / (4.0 * m);
            q += internal / m - expected / m;
        }
        q
    }
}

/// Dashboard community with inferred metadata (Phase 14 A+).
#[derive(Debug, Clone, Serialize)]
pub struct DashboardCommunity {
    /// Community identifier
    pub id: usize,
    /// Member node IDs
    pub nodes: Vec<Uuid>,
    /// Member count
    pub size: usize,
    /// Most common node type in the cluster
    pub primary_type: NodeType,
    /// Average cyclomatic complexity
    pub avg_complexity: f64,
    /// Human-readable label (e.g. "auth cluster")
    pub label: String,
}

/// Detect communities for the analytics dashboard.
///
/// Uses label propagation (via [`CommunityDetector`]) and enriches each cluster
/// with labels and complexity metadata. Falls back to connected components when
/// propagation yields a single cluster on disconnected subgraphs.
pub fn detect_communities(backend: &MemoryBackend) -> Result<Vec<DashboardCommunity>> {
    let nodes = backend.all_nodes()?;
    let detection = CommunityDetector::new().detect(backend)?;

    let mut communities: Vec<DashboardCommunity> = detection
        .communities
        .into_iter()
        .filter(|c| c.members.len() >= 2)
        .map(|c| build_dashboard_community(c.id, &c.members, &nodes))
        .collect();

    if communities.len() < 2 {
        let components = connected_components(backend, &nodes)?;
        if components.len() > communities.len() {
            communities = components
                .into_iter()
                .enumerate()
                .filter(|(_, members)| members.len() >= 2)
                .map(|(idx, members)| build_dashboard_community(idx, &members, &nodes))
                .collect();
        }
    }

    communities.sort_by(|a, b| b.size.cmp(&a.size));
    Ok(communities)
}

fn build_dashboard_community(
    id: usize,
    member_ids: &[Uuid],
    all_nodes: &[crate::graph::schema::Node],
) -> DashboardCommunity {
    let community_nodes: Vec<_> = all_nodes
        .iter()
        .filter(|n| member_ids.contains(&n.id))
        .collect();

    DashboardCommunity {
        id,
        nodes: member_ids.to_vec(),
        size: member_ids.len(),
        primary_type: most_common_type(&community_nodes),
        avg_complexity: avg_complexity(&community_nodes),
        label: infer_community_label(&community_nodes, id),
    }
}

fn connected_components(
    backend: &MemoryBackend,
    nodes: &[crate::graph::schema::Node],
) -> Result<Vec<Vec<Uuid>>> {
    let edges = backend.all_edges()?;
    let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for edge in &edges {
        adj.entry(edge.from).or_default().push(edge.to);
        adj.entry(edge.to).or_default().push(edge.from);
    }

    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for node in nodes {
        if visited.contains(&node.id) {
            continue;
        }
        let mut stack = vec![node.id];
        let mut component = Vec::new();
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            component.push(current);
            if let Some(neighbors) = adj.get(&current) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
        }
        if !component.is_empty() {
            components.push(component);
        }
    }
    Ok(components)
}

fn most_common_type(nodes: &[&crate::graph::schema::Node]) -> NodeType {
    let mut counts = HashMap::new();
    for node in nodes {
        *counts.entry(node.node_type).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(t, _)| t)
        .unwrap_or(NodeType::Function)
}

fn avg_complexity(nodes: &[&crate::graph::schema::Node]) -> f64 {
    if nodes.is_empty() {
        return 0.0;
    }
    let sum: f64 = nodes
        .iter()
        .map(|n| node_complexity(n) as f64)
        .sum();
    sum / nodes.len() as f64
}

fn node_complexity(node: &crate::graph::schema::Node) -> i64 {
    node.get_property("cyclomatic")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
}

fn infer_community_label(nodes: &[&crate::graph::schema::Node], idx: usize) -> String {
    let paths: Vec<_> = nodes
        .iter()
        .filter_map(|n| n.file_path.as_ref())
        .collect();

    if let Some(common) = find_common_path_prefix(&paths) {
        if !common.is_empty() {
            return common;
        }
    }

    let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
    if names.iter().any(|n| n.contains("auth") || n.contains("Auth")) {
        return "auth cluster".into();
    }
    if names.iter().any(|n| n.contains("api") || n.contains("Api")) {
        return "API layer".into();
    }
    if names
        .iter()
        .any(|n| n.contains("db") || n.contains("database") || n.contains("query"))
    {
        return "database layer".into();
    }

    format!("cluster_{idx}")
}

fn find_common_path_prefix(paths: &[&String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let first = paths[0].as_str();
    let mut prefix_len = first.len();
    for path in &paths[1..] {
        prefix_len = first
            .chars()
            .zip(path.chars())
            .take(prefix_len)
            .take_while(|(a, b)| a == b)
            .count();
    }
    if prefix_len == 0 {
        return None;
    }
    if let Some(last_slash) = first[..prefix_len].rfind('/') {
        return Some(first[..last_slash].to_string());
    }
    Some(first[..prefix_len].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::{Edge, Node, NodeType};

    fn build_modular_graph() -> MemoryBackend {
        let mut backend = MemoryBackend::new();
        let auth1 = Node::new(NodeType::Function, "login".to_string());
        let auth2 = Node::new(NodeType::Function, "logout".to_string());
        let api1 = Node::new(NodeType::Function, "get_user".to_string());
        let api2 = Node::new(NodeType::Function, "create_user".to_string());
        let ui1 = Node::new(NodeType::Function, "render".to_string());

        let ids: Vec<_> = [&auth1, &auth2, &api1, &api2, &ui1]
            .iter()
            .map(|n| {
                let id = n.id;
                backend.insert_node((*n).clone()).unwrap();
                id
            })
            .collect();

        backend.insert_edge(Edge::new(ids[0], ids[1], crate::graph::schema::EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(ids[2], ids[3], crate::graph::schema::EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(ids[0], ids[2], crate::graph::schema::EdgeType::Uses)).unwrap();
        backend.insert_edge(Edge::new(ids[4], ids[0], crate::graph::schema::EdgeType::Calls)).unwrap();
        backend
    }

    #[test]
    fn test_community_detection() {
        let backend = build_modular_graph();
        let detector = CommunityDetector::new();
        let result = detector.detect(&backend).unwrap();
        assert!(!result.communities.is_empty());
        assert!(result.modularity >= 0.0);
    }
}
