//! In-memory graph backend
//!
//! Simple in-memory implementation of GraphBackend for testing and small repositories.

use crate::error::Result;
use crate::graph::backend::trait_def::GraphBackend;
use crate::graph::schema::{Edge, EdgeType, Node, NodeType};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// In-memory graph backend
#[derive(Debug, Clone)]
pub struct MemoryBackend {
    nodes: Arc<RwLock<HashMap<Uuid, Node>>>,
    edges: Arc<RwLock<Vec<Edge>>>,
    node_name_index: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,
}

impl MemoryBackend {
    /// Create a new in-memory backend
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            edges: Arc::new(RwLock::new(Vec::new())),
            node_name_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Result<Vec<Node>> {
        let nodes = self.nodes.read().unwrap();
        Ok(nodes.values().cloned().collect())
    }

    /// Get all edges
    pub fn all_edges(&self) -> Result<Vec<Edge>> {
        let edges = self.edges.read().unwrap();
        Ok(edges.clone())
    }

    /// Find nodes by name
    pub fn find_nodes_by_name(&self, name: &str) -> Result<Vec<Node>> {
        let index = self.node_name_index.read().unwrap();
        if let Some(ids) = index.get(name) {
            let nodes = self.nodes.read().unwrap();
            Ok(ids.iter().filter_map(|id| nodes.get(id).cloned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Find nodes by type
    pub fn find_nodes_by_type(&self, node_type: NodeType) -> Result<Vec<Node>> {
        let nodes = self.nodes.read().unwrap();
        Ok(nodes
            .values()
            .filter(|n| n.node_type == node_type)
            .cloned()
            .collect())
    }

    /// Find edges by type
    pub fn find_edges_by_type(&self, edge_type: EdgeType) -> Result<Vec<Edge>> {
        let edges = self.edges.read().unwrap();
        Ok(edges
            .iter()
            .filter(|e| e.edge_type == edge_type)
            .cloned()
            .collect())
    }

    /// Get outgoing edges from a node
    pub fn get_outgoing_edges(&self, node_id: Uuid) -> Result<Vec<Edge>> {
        let edges = self.edges.read().unwrap();
        Ok(edges
            .iter()
            .filter(|e| e.from == node_id)
            .cloned()
            .collect())
    }

    /// Get incoming edges to a node
    pub fn get_incoming_edges(&self, node_id: Uuid) -> Result<Vec<Edge>> {
        let edges = self.edges.read().unwrap();
        Ok(edges
            .iter()
            .filter(|e| e.to == node_id)
            .cloned()
            .collect())
    }

    /// Get neighbors of a node (nodes connected by edges)
    pub fn get_neighbors(&self, node_id: Uuid) -> Result<Vec<Node>> {
        let edges = self.edges.read().unwrap();
        let mut neighbor_ids: Vec<Uuid> = edges
            .iter()
            .filter_map(|e| {
                if e.from == node_id {
                    Some(e.to)
                } else if e.to == node_id {
                    Some(e.from)
                } else {
                    None
                }
            })
            .collect();
        neighbor_ids.dedup();

        let nodes = self.nodes.read().unwrap();
        Ok(neighbor_ids
            .iter()
            .filter_map(|id| nodes.get(id).cloned())
            .collect())
    }

    /// Count nodes
    pub fn node_count(&self) -> usize {
        self.nodes.read().unwrap().len()
    }

    /// Count edges
    pub fn edge_count(&self) -> usize {
        self.edges.read().unwrap().len()
    }

    /// Clear all data
    pub fn clear(&self) -> Result<()> {
        self.nodes.write().unwrap().clear();
        self.edges.write().unwrap().clear();
        self.node_name_index.write().unwrap().clear();
        Ok(())
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphBackend for MemoryBackend {
    fn insert_node(&mut self, node: Node) -> Result<()> {
        let id = node.id;
        let name = node.name.clone();

        // Insert into main storage
        self.nodes.write().unwrap().insert(id, node);

        // Update name index
        self.node_name_index
            .write()
            .unwrap()
            .entry(name)
            .or_insert_with(Vec::new)
            .push(id);

        Ok(())
    }

    fn get_node(&self, id: Uuid) -> Result<Option<Node>> {
        Ok(self.nodes.read().unwrap().get(&id).cloned())
    }

    fn insert_edge(&mut self, edge: Edge) -> Result<()> {
        self.edges.write().unwrap().push(edge);
        Ok(())
    }

    fn delete_node(&mut self, id: Uuid) -> Result<()> {
        // Remove node
        if let Some(node) = self.nodes.write().unwrap().remove(&id) {
            // Update name index
            if let Some(ids) = self.node_name_index.write().unwrap().get_mut(&node.name) {
                ids.retain(|&x| x != id);
            }

            // Remove related edges
            self.edges.write().unwrap().retain(|e| e.from != id && e.to != id);
        }
        Ok(())
    }

    fn find_nodes(&self, filter: &str) -> Result<Vec<Node>> {
        // Simple name-based search for now
        self.find_nodes_by_name(filter)
    }

    fn query(&self, _query: &str) -> Result<Vec<Node>> {
        // Placeholder - will implement query DSL later
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_backend() {
        let backend = MemoryBackend::new();
        assert_eq!(backend.node_count(), 0);
        assert_eq!(backend.edge_count(), 0);
    }

    #[test]
    fn test_insert_and_get_node() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Function, "test".to_string());
        let id = node.id;

        backend.insert_node(node.clone()).unwrap();
        assert_eq!(backend.node_count(), 1);

        let retrieved = backend.get_node(id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test");
    }

    #[test]
    fn test_insert_edge() {
        let mut backend = MemoryBackend::new();
        let node1 = Node::new(NodeType::Function, "func1".to_string());
        let node2 = Node::new(NodeType::Function, "func2".to_string());
        let id1 = node1.id;
        let id2 = node2.id;

        backend.insert_node(node1).unwrap();
        backend.insert_node(node2).unwrap();

        let edge = Edge::new(id1, id2, EdgeType::Calls);
        backend.insert_edge(edge).unwrap();

        assert_eq!(backend.edge_count(), 1);
    }

    #[test]
    fn test_find_nodes_by_name() {
        let mut backend = MemoryBackend::new();
        let node1 = Node::new(NodeType::Function, "test".to_string());
        let node2 = Node::new(NodeType::Function, "test".to_string());
        let node3 = Node::new(NodeType::Function, "other".to_string());

        backend.insert_node(node1).unwrap();
        backend.insert_node(node2).unwrap();
        backend.insert_node(node3).unwrap();

        let results = backend.find_nodes_by_name("test").unwrap();
        assert_eq!(results.len(), 2);

        let results = backend.find_nodes_by_name("other").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_find_nodes_by_type() {
        let mut backend = MemoryBackend::new();
        backend.insert_node(Node::new(NodeType::Function, "func1".to_string())).unwrap();
        backend.insert_node(Node::new(NodeType::Function, "func2".to_string())).unwrap();
        backend.insert_node(Node::new(NodeType::Class, "Class1".to_string())).unwrap();

        let functions = backend.find_nodes_by_type(NodeType::Function).unwrap();
        assert_eq!(functions.len(), 2);

        let classes = backend.find_nodes_by_type(NodeType::Class).unwrap();
        assert_eq!(classes.len(), 1);
    }

    #[test]
    fn test_get_neighbors() {
        let mut backend = MemoryBackend::new();
        let node1 = Node::new(NodeType::Function, "func1".to_string());
        let node2 = Node::new(NodeType::Function, "func2".to_string());
        let node3 = Node::new(NodeType::Function, "func3".to_string());
        let id1 = node1.id;
        let id2 = node2.id;
        let id3 = node3.id;

        backend.insert_node(node1).unwrap();
        backend.insert_node(node2).unwrap();
        backend.insert_node(node3).unwrap();

        backend.insert_edge(Edge::new(id1, id2, EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(id1, id3, EdgeType::Calls)).unwrap();

        let neighbors = backend.get_neighbors(id1).unwrap();
        assert_eq!(neighbors.len(), 2);
    }

    #[test]
    fn test_delete_node() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Function, "test".to_string());
        let id = node.id;

        backend.insert_node(node).unwrap();
        assert_eq!(backend.node_count(), 1);

        backend.delete_node(id).unwrap();
        assert_eq!(backend.node_count(), 0);
    }

    #[test]
    fn test_clear() {
        let mut backend = MemoryBackend::new();
        backend.insert_node(Node::new(NodeType::Function, "func1".to_string())).unwrap();
        backend.insert_node(Node::new(NodeType::Function, "func2".to_string())).unwrap();

        assert_eq!(backend.node_count(), 2);

        backend.clear().unwrap();
        assert_eq!(backend.node_count(), 0);
        assert_eq!(backend.edge_count(), 0);
    }
}
