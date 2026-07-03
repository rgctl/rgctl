//! Utilities for converting rBuilder graphs into petgraph structures.
//!
//! ## Type-Aware Topology Projection
//!
//! Builds lightweight petgraph views using `EdgeType` edge weights so consumers can
//! filter projections (call graph, structural graph, etc.) without cloning full
//! [`Edge`] structs from the backend.

use petgraph::graph::{DiGraph, NodeIndex, UnGraph};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::snapshot::PreparedGraphSnapshot;
use rbuilder_graph::schema::EdgeType;
use std::collections::HashMap;
use uuid::Uuid;

/// A petgraph view of the code graph with UUID mapping and typed edges.
pub struct PetGraphView {
    /// Directed graph with [`EdgeType`] weights
    pub directed: DiGraph<(), EdgeType>,
    /// Undirected graph for community detection
    pub undirected: UnGraph<(), EdgeType>,
    /// Map from node UUID to directed graph index
    pub uuid_to_index: HashMap<Uuid, NodeIndex>,
    /// Map from directed graph index to UUID
    pub index_to_uuid: HashMap<NodeIndex, Uuid>,
    /// Map from undirected graph index to UUID
    pub undirected_to_uuid: HashMap<NodeIndex, Uuid>,
}

impl PetGraphView {
    /// Build petgraph views from a memory backend using zero-clone typed topology projection.
    pub fn from_backend(backend: &MemoryBackend) -> Result<Self> {
        let node_count = backend.node_count();
        let edge_count = backend.edge_count();

        let mut directed = DiGraph::<(), EdgeType>::with_capacity(node_count, edge_count);
        let mut undirected = UnGraph::<(), EdgeType>::with_capacity(node_count, edge_count);
        let mut uuid_to_index = HashMap::with_capacity(node_count);
        let mut index_to_uuid = HashMap::with_capacity(node_count);
        let mut uuid_to_undirected = HashMap::with_capacity(node_count);
        let mut undirected_to_uuid = HashMap::with_capacity(node_count);

        let node_ids = backend.all_node_ids()?;

        for node_id in node_ids {
            let d_idx = directed.add_node(());
            let u_idx = undirected.add_node(());
            uuid_to_index.insert(node_id, d_idx);
            index_to_uuid.insert(d_idx, node_id);
            uuid_to_undirected.insert(node_id, u_idx);
            undirected_to_uuid.insert(u_idx, node_id);
        }

        Self::wire_edges(
            backend.edge_topology_typed()?,
            &mut directed,
            &mut undirected,
            &uuid_to_index,
            &uuid_to_undirected,
        )?;

        Ok(Self {
            directed,
            undirected,
            uuid_to_index,
            index_to_uuid,
            undirected_to_uuid,
        })
    }

    /// Build petgraph views directly from a prepared mmap snapshot (no backend hydration).
    pub fn from_prepared(prepared: &PreparedGraphSnapshot) -> Result<Self> {
        let node_count = prepared.nodes.len();
        let edge_count = prepared.edges.len();

        let mut directed = DiGraph::<(), EdgeType>::with_capacity(node_count, edge_count);
        let mut undirected = UnGraph::<(), EdgeType>::with_capacity(node_count, edge_count);
        let mut uuid_to_index = HashMap::with_capacity(node_count);
        let mut index_to_uuid = HashMap::with_capacity(node_count);
        let mut uuid_to_undirected = HashMap::with_capacity(node_count);
        let mut undirected_to_uuid = HashMap::with_capacity(node_count);

        for node in &prepared.nodes {
            let d_idx = directed.add_node(());
            let u_idx = undirected.add_node(());
            uuid_to_index.insert(node.id, d_idx);
            index_to_uuid.insert(d_idx, node.id);
            uuid_to_undirected.insert(node.id, u_idx);
            undirected_to_uuid.insert(u_idx, node.id);
        }

        let edge_topology = prepared
            .edges
            .iter()
            .map(|e| (e.from, e.to, e.edge_type))
            .collect::<Vec<_>>();

        Self::wire_edges(
            edge_topology,
            &mut directed,
            &mut undirected,
            &uuid_to_index,
            &uuid_to_undirected,
        )?;

        Ok(Self {
            directed,
            undirected,
            uuid_to_index,
            index_to_uuid,
            undirected_to_uuid,
        })
    }

    fn wire_edges(
        edge_topology: Vec<(Uuid, Uuid, EdgeType)>,
        directed: &mut DiGraph<(), EdgeType>,
        undirected: &mut UnGraph<(), EdgeType>,
        uuid_to_index: &HashMap<Uuid, NodeIndex>,
        uuid_to_undirected: &HashMap<Uuid, NodeIndex>,
    ) -> Result<()> {
        for (from_uuid, to_uuid, edge_type) in edge_topology {
            if let (Some(&from), Some(&to)) = (
                uuid_to_index.get(&from_uuid),
                uuid_to_index.get(&to_uuid),
            ) {
                directed.add_edge(from, to, edge_type);
            }

            if let (Some(&from), Some(&to)) = (
                uuid_to_undirected.get(&from_uuid),
                uuid_to_undirected.get(&to_uuid),
            ) {
                undirected.add_edge(from, to, edge_type);
            }
        }
        Ok(())
    }

    /// Incoming neighbors reachable via one of `allowed` edge types.
    pub fn incoming_filtered<'a>(
        &'a self,
        idx: NodeIndex,
        allowed: &'a [EdgeType],
    ) -> impl Iterator<Item = NodeIndex> + 'a {
        self.directed
            .edges_directed(idx, Direction::Incoming)
            .filter(move |e| allowed.contains(e.weight()))
            .map(|e| e.source())
    }

    /// Outgoing neighbors reachable via one of `allowed` edge types.
    pub fn outgoing_filtered<'a>(
        &'a self,
        idx: NodeIndex,
        allowed: &'a [EdgeType],
    ) -> impl Iterator<Item = NodeIndex> + 'a {
        self.directed
            .edges_directed(idx, Direction::Outgoing)
            .filter(move |e| allowed.contains(e.weight()))
            .map(|e| e.target())
    }

    /// Whether a directed edge of the given type exists between two nodes.
    pub fn has_edge_type(&self, from: NodeIndex, to: NodeIndex, edge_type: EdgeType) -> bool {
        self.directed
            .find_edge(from, to)
            .is_some_and(|e| *self.directed.edge_weight(e).unwrap_or(&EdgeType::Calls) == edge_type)
    }

    /// Build a call-only directed graph sharing the same node indices as [`Self::directed`].
    pub fn call_only_directed(&self) -> DiGraph<(), ()> {
        let mut call_only =
            DiGraph::<(), ()>::with_capacity(self.directed.node_count(), self.directed.edge_count());
        for _ in self.directed.node_indices() {
            call_only.add_node(());
        }
        for edge in self.directed.edge_references() {
            if *edge.weight() == EdgeType::Calls {
                call_only.add_edge(edge.source(), edge.target(), ());
            }
        }
        call_only
    }

    /// Get petgraph NodeIndex for a UUID.
    pub fn get_index(&self, uuid: Uuid) -> Option<NodeIndex> {
        self.uuid_to_index.get(&uuid).copied()
    }

    /// Get UUID for a petgraph NodeIndex.
    pub fn get_uuid(&self, index: NodeIndex) -> Option<Uuid> {
        self.index_to_uuid.get(&index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, Node, NodeType};

    #[test]
    fn test_build_petgraph_view() {
        let mut backend = MemoryBackend::new();
        let n1 = Node::new(NodeType::Function, "main".to_string());
        let n2 = Node::new(NodeType::Function, "helper".to_string());
        let id1 = n1.id;
        let id2 = n2.id;
        backend.insert_node(n1).unwrap();
        backend.insert_node(n2).unwrap();
        backend
            .insert_edge(Edge::new(id1, id2, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        assert_eq!(view.directed.node_count(), 2);
        assert_eq!(view.directed.edge_count(), 1);
        let idx1 = view.uuid_to_index[&id1];
        let idx2 = view.uuid_to_index[&id2];
        assert!(view.has_edge_type(idx1, idx2, EdgeType::Calls));
    }

    #[test]
    fn test_build_petgraph_view_from_prepared() {
        let mut backend = MemoryBackend::new();
        let n1 = Node::new(NodeType::Function, "main".to_string());
        let n2 = Node::new(NodeType::Function, "helper".to_string());
        let id1 = n1.id;
        let id2 = n2.id;
        backend.insert_node(n1).unwrap();
        backend.insert_node(n2).unwrap();
        backend
            .insert_edge(Edge::new(id1, id2, EdgeType::Calls))
            .unwrap();

        let prepared = rbuilder_graph::PreparedGraphSnapshot::from_backend(&backend).unwrap();
        let view = PetGraphView::from_prepared(&prepared).unwrap();
        assert_eq!(view.directed.node_count(), 2);
        assert_eq!(view.directed.edge_count(), 1);
    }
}
