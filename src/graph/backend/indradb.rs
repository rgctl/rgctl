//! IndraDB backend implementation
//!
//! Task 1.4.2: IndraDB integration
//!
//! Full native IndraDB requires `protoc` at build time. Until that is available,
//! this backend delegates to the in-memory implementation while preserving the
//! same [`GraphBackend`] interface.

use crate::error::Result;
use crate::graph::backend::{GraphBackend, MemoryBackend};
use crate::graph::schema::{Edge, Node};
use uuid::Uuid;

/// Graph backend backed by in-memory storage.
///
/// This type is API-compatible with the planned IndraDB backend and can be
/// swapped once native IndraDB builds are enabled in the environment.
#[derive(Debug, Clone, Default)]
pub struct IndraDbBackend {
    inner: MemoryBackend,
}

impl IndraDbBackend {
    /// Create a new in-memory IndraDB-compatible backend.
    pub fn new_memory() -> Self {
        Self::default()
    }

    /// Access the underlying memory backend.
    pub fn inner(&self) -> &MemoryBackend {
        &self.inner
    }

    /// Mutable access to the underlying memory backend.
    pub fn inner_mut(&mut self) -> &mut MemoryBackend {
        &mut self.inner
    }
}

impl GraphBackend for IndraDbBackend {
    fn insert_node(&mut self, node: Node) -> Result<()> {
        self.inner.insert_node(node)
    }

    fn get_node(&self, id: Uuid) -> Result<Option<Node>> {
        self.inner.get_node(id)
    }

    fn insert_edge(&mut self, edge: Edge) -> Result<()> {
        self.inner.insert_edge(edge)
    }

    fn insert_nodes_batch(&mut self, nodes: Vec<Node>) -> Result<()> {
        self.inner.insert_nodes_batch(nodes)
    }

    fn insert_edges_batch(&mut self, edges: Vec<Edge>) -> Result<()> {
        self.inner.insert_edges_batch(edges)
    }

    fn delete_node(&mut self, id: Uuid) -> Result<()> {
        self.inner.delete_node(id)
    }

    fn find_nodes(&self, filter: &str) -> Result<Vec<Node>> {
        self.inner.find_nodes(filter)
    }

    fn query(&self, query: &str) -> Result<Vec<Node>> {
        crate::graph::query::execute(&self.inner, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::schema::NodeType;

    #[test]
    fn test_indradb_node_insertion() {
        let mut db = IndraDbBackend::new_memory();
        let node = Node::new(NodeType::Function, "main".to_string());
        let id = node.id;
        db.insert_node(node).unwrap();

        let retrieved = db.get_node(id).unwrap().unwrap();
        assert_eq!(retrieved.name, "main");
    }

    #[test]
    fn test_indradb_query() {
        let mut db = IndraDbBackend::new_memory();
        db.insert_node(Node::new(NodeType::Function, "foo".to_string()))
            .unwrap();

        let results = db.query("functions").unwrap();
        assert_eq!(results.len(), 1);
    }
}
