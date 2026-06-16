//! Graph backend trait definition
//!
//! Defines the interface for graph storage backends.

use crate::error::Result;
use crate::graph::schema::{Edge, Node};
use uuid::Uuid;

/// Graph storage backend trait
pub trait GraphBackend: Send + Sync {
    /// Insert a node into the graph
    fn insert_node(&mut self, node: Node) -> Result<()>;

    /// Get a node by ID
    fn get_node(&self, id: Uuid) -> Result<Option<Node>>;

    /// Insert an edge into the graph
    fn insert_edge(&mut self, edge: Edge) -> Result<()>;

    /// Delete a node and its associated edges
    fn delete_node(&mut self, id: Uuid) -> Result<()>;

    /// Find nodes matching a filter (implementation-dependent)
    fn find_nodes(&self, filter: &str) -> Result<Vec<Node>>;

    /// Execute a query (query language depends on backend)
    fn query(&self, query: &str) -> Result<Vec<Node>>;
}
