//! Graph backend trait definition
//!
//! Defines the interface for graph storage backends.
//!
//! [`GraphBackend`] is the minimal portable surface (insert, get, delete, string query).
//! [`MemoryBackend`](crate::backend::MemoryBackend) extends it with batch APIs, typed
//! indexes, streaming iterators, snapshot hydration, and Petgraph export — the only
//! production backend today. See `crates/rbuilder-graph/README.md`.

use crate::schema::{Edge, Node};
use rbuilder_error::Result;
use uuid::Uuid;

/// Graph storage backend trait.
///
/// Implementors must be `Send + Sync` for use across pipeline stages. Extension methods
/// on [`MemoryBackend`](crate::backend::MemoryBackend) are intentionally not part of this
/// trait to keep plugin backends small.
pub trait GraphBackend: Send + Sync {
    /// Insert a node into the graph
    fn insert_node(&mut self, node: Node) -> Result<()>;

    /// Insert multiple nodes in one batch (single lock acquisition when supported)
    fn insert_nodes_batch(&mut self, nodes: Vec<Node>) -> Result<()> {
        for node in nodes {
            self.insert_node(node)?;
        }
        Ok(())
    }

    /// Get a node by ID
    fn get_node(&self, id: Uuid) -> Result<Option<Node>>;

    /// Insert an edge into the graph
    fn insert_edge(&mut self, edge: Edge) -> Result<()>;

    /// Insert multiple edges in one batch (single lock acquisition when supported)
    fn insert_edges_batch(&mut self, edges: Vec<Edge>) -> Result<()> {
        for edge in edges {
            self.insert_edge(edge)?;
        }
        Ok(())
    }

    /// Delete a node and its associated edges
    fn delete_node(&mut self, id: Uuid) -> Result<()>;

    /// Find nodes matching a filter (implementation-dependent)
    fn find_nodes(&self, filter: &str) -> Result<Vec<Node>>;

    /// Execute a query (query language depends on backend)
    fn query(&self, query: &str) -> Result<Vec<Node>>;
}
