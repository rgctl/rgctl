//! Graph export functionality
//!
//! Task 1.6.4: Implement graph export (JSON)

use crate::error::{Error, Result};
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::{Edge, Node};
use serde::{Deserialize, Serialize};

/// Serializable graph snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSnapshot {
    /// rBuilder version that created the snapshot
    pub version: String,
    /// All nodes
    pub nodes: Vec<Node>,
    /// All edges
    pub edges: Vec<Edge>,
}

/// Export a graph backend to compact JSON.
pub fn export_json(backend: &MemoryBackend) -> Result<String> {
    let snapshot = GraphSnapshot {
        version: crate::VERSION.to_string(),
        nodes: backend.all_nodes()?,
        edges: backend.all_edges()?,
    };
    serde_json::to_string_pretty(&snapshot).map_err(|e| Error::SerdeError(e.to_string()))
}

/// Import a graph snapshot from JSON.
pub fn import_json(json: &str) -> Result<GraphSnapshot> {
    serde_json::from_str(json).map_err(|e| Error::SerdeError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::{EdgeType, NodeType};

    #[test]
    fn test_graph_export_import() {
        let mut backend = MemoryBackend::new();
        let n1 = Node::new(NodeType::Function, "main".to_string());
        let n2 = Node::new(NodeType::File, "main.rs".to_string());
        let id1 = n1.id;
        let id2 = n2.id;
        backend.insert_node(n1).unwrap();
        backend.insert_node(n2).unwrap();
        backend.insert_edge(Edge::new(id1, id2, EdgeType::DefinedIn)).unwrap();

        let json = export_json(&backend).unwrap();
        let imported = import_json(&json).unwrap();

        assert_eq!(imported.nodes.len(), 2);
        assert_eq!(imported.edges.len(), 1);
    }
}
