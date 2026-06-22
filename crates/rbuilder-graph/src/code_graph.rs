//! High-level code graph API

use crate::backend::MemoryBackend;
use crate::export::{export_json, import_json, GraphSnapshot};
use crate::query;
use crate::schema::{Edge, Node, NodeType};
use rbuilder_error::{Error, Result};
use std::path::{Path, PathBuf};

/// Default directory for persisted graph data.
pub const GRAPH_DIR: &str = ".rbuilder";

/// Default graph filename.
pub const GRAPH_FILE: &str = "graph.json";

/// A queryable code knowledge graph.
#[derive(Debug, Clone)]
pub struct CodeGraph {
    backend: MemoryBackend,
}

impl CodeGraph {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self {
            backend: MemoryBackend::new(),
        }
    }

    /// Load nodes and edges into the graph.
    pub fn load(&mut self, nodes: Vec<Node>, edges: Vec<Edge>) -> Result<()> {
        self.backend.insert_nodes_batch(nodes)?;
        self.backend.insert_edges_batch(edges)?;
        Ok(())
    }

    /// Build a graph from a repository path.
    ///
    /// Prefer [`rbuilder_pipeline::code_graph_from_repository`] in workspace builds.
    pub fn from_repository(root: &Path) -> Result<Self> {
        let _ = root;
        Err(Error::Other(
            "Use rbuilder_pipeline::code_graph_from_repository from the workspace root crate"
                .into(),
        ))
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.backend.node_count()
    }

    /// Number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.backend.edge_count()
    }

    /// Access the underlying backend.
    pub fn backend(&self) -> &MemoryBackend {
        &self.backend
    }

    /// Mutable access to the underlying backend.
    pub fn backend_mut(&mut self) -> &mut MemoryBackend {
        &mut self.backend
    }

    /// Execute a simple query against the graph.
    pub fn query(&self, query_str: &str) -> Result<Vec<Node>> {
        query::execute(&self.backend, query_str)
    }

    /// Find all nodes of a given type.
    pub fn find_by_type(&self, node_type: NodeType) -> Result<Vec<Node>> {
        self.backend.find_nodes_by_type(node_type)
    }

    /// Export the graph to a JSON string.
    pub fn export_json(&self) -> Result<String> {
        export_json(&self.backend)
    }

    /// Import a graph from a JSON string.
    pub fn import_json(json: &str) -> Result<Self> {
        let snapshot = import_json(json)?;
        let mut graph = Self::new();
        graph.load(snapshot.nodes, snapshot.edges)?;
        Ok(graph)
    }

    /// Save the graph to the default path under a repository root.
    pub fn save_to_repo(&self, repo_root: &Path) -> Result<PathBuf> {
        let dir = repo_root.join(GRAPH_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(GRAPH_FILE);
        std::fs::write(&path, self.export_json()?)?;
        Ok(path)
    }

    /// Load a graph from the default path under a repository root.
    pub fn load_from_repo(repo_root: &Path) -> Result<Self> {
        let path = repo_root.join(GRAPH_DIR).join(GRAPH_FILE);
        if !path.exists() {
            return Err(Error::NotFound(format!(
                "Graph not found at {}. Run `rbuilder init` first.",
                path.display()
            )));
        }
        let json = std::fs::read_to_string(path)?;
        Self::import_json(&json)
    }

    /// Create a snapshot of the graph.
    pub fn snapshot(&self) -> Result<GraphSnapshot> {
        Ok(GraphSnapshot {
            version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: crate::schema::GRAPH_SCHEMA_VERSION,
            nodes: self.backend.all_nodes()?,
            edges: self.backend.all_edges()?,
        })
    }
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::NodeType;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    #[ignore = "moved to rbuilder_pipeline::code_graph_from_repository"]
    fn test_from_repository() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("lib.rs"), "pub fn hello() {}\n").unwrap();

        let graph = CodeGraph::from_repository(temp.path()).unwrap();
        let functions = graph.find_by_type(NodeType::Function).unwrap();
        assert!(!functions.is_empty());
    }

    #[test]
    #[ignore = "moved to rbuilder_pipeline::code_graph_from_repository"]
    fn test_export_import_roundtrip() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("main.rs"), "fn main() {}\n").unwrap();

        let graph = CodeGraph::from_repository(temp.path()).unwrap();
        let json = graph.export_json().unwrap();
        let imported = CodeGraph::import_json(&json).unwrap();

        assert_eq!(graph.node_count(), imported.node_count());
        assert_eq!(graph.edge_count(), imported.edge_count());
    }

    #[test]
    #[ignore = "moved to rbuilder_pipeline::code_graph_from_repository"]
    fn test_save_and_load_repo() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("main.rs"), "fn main() {}\n").unwrap();

        let graph = CodeGraph::from_repository(temp.path()).unwrap();
        graph.save_to_repo(temp.path()).unwrap();
        let loaded = CodeGraph::load_from_repo(temp.path()).unwrap();

        assert_eq!(graph.node_count(), loaded.node_count());
    }
}
