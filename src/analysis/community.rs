//! Community detection
//!
//! Task 2.1.1: Label-propagation community detection with modularity scoring.

use crate::analysis::graph_utils::PetGraphView;
use crate::error::Result;
use crate::graph::backend::MemoryBackend;
use petgraph::graph::NodeIndex;
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
