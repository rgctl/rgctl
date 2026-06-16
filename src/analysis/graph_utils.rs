//! Utilities for converting rBuilder graphs into petgraph structures.

use crate::error::Result;
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::{EdgeType, Node};
use petgraph::graph::{DiGraph, NodeIndex, UnGraph};
use std::collections::HashMap;
use uuid::Uuid;

/// A petgraph view of the code graph with UUID mapping.
pub struct PetGraphView {
    /// Directed graph for dependency/call analysis
    pub directed: DiGraph<Uuid, EdgeType>,
    /// Undirected graph for community detection
    pub undirected: UnGraph<Uuid, ()>,
    /// Map from node UUID to petgraph index (directed)
    pub uuid_to_directed: HashMap<Uuid, NodeIndex>,
    /// Map from directed index to UUID
    pub directed_to_uuid: HashMap<NodeIndex, Uuid>,
    /// All nodes from the backend
    pub nodes: Vec<Node>,
}

impl PetGraphView {
    /// Build petgraph views from a memory backend.
    pub fn from_backend(backend: &MemoryBackend) -> Result<Self> {
        let nodes = backend.all_nodes()?;
        let edges = backend.all_edges()?;

        let mut directed = DiGraph::<Uuid, EdgeType>::new();
        let mut undirected = UnGraph::<Uuid, ()>::new_undirected();
        let mut uuid_to_directed = HashMap::new();
        let mut directed_to_uuid = HashMap::new();
        let mut uuid_to_undirected = HashMap::new();

        for node in &nodes {
            let d_idx = directed.add_node(node.id);
            let u_idx = undirected.add_node(node.id);
            uuid_to_directed.insert(node.id, d_idx);
            directed_to_uuid.insert(d_idx, node.id);
            uuid_to_undirected.insert(node.id, u_idx);
        }

        for edge in &edges {
            if let (Some(&from), Some(&to)) = (
                uuid_to_directed.get(&edge.from),
                uuid_to_directed.get(&edge.to),
            ) {
                directed.add_edge(from, to, edge.edge_type);
            }

            if matches!(
                edge.edge_type,
                EdgeType::Calls
                    | EdgeType::Uses
                    | EdgeType::Contains
                    | EdgeType::DefinedIn
                    | EdgeType::UsesConfig
            ) {
                if let (Some(&from), Some(&to)) = (
                    uuid_to_undirected.get(&edge.from),
                    uuid_to_undirected.get(&edge.to),
                ) {
                    undirected.add_edge(from, to, ());
                }
            }
        }

        Ok(Self {
            directed,
            undirected,
            uuid_to_directed,
            directed_to_uuid,
            nodes,
        })
    }

    /// Find a node by name (first match).
    pub fn find_node_by_name(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }

    /// Find node UUID by name.
    pub fn find_uuid_by_name(&self, name: &str) -> Option<Uuid> {
        self.find_node_by_name(name).map(|n| n.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::{Edge, NodeType};

    #[test]
    fn test_build_petgraph_view() {
        let mut backend = MemoryBackend::new();
        let n1 = Node::new(NodeType::Function, "main".to_string());
        let n2 = Node::new(NodeType::Function, "helper".to_string());
        let id1 = n1.id;
        let id2 = n2.id;
        backend.insert_node(n1).unwrap();
        backend.insert_node(n2).unwrap();
        backend.insert_edge(Edge::new(id1, id2, EdgeType::Calls)).unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        assert_eq!(view.directed.node_count(), 2);
        assert_eq!(view.directed.edge_count(), 1);
    }
}
