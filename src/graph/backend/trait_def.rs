//! GraphBackend trait
//!
//! Task 1.4.3: Implement GraphBackend trait

use crate::error::Result;
use crate::graph::schema::{Edge, Node};
use uuid::Uuid;

// Placeholder trait - will be fully implemented in Task 1.4.3
pub trait GraphBackend: Send + Sync {
    // Placeholder methods
    fn insert_node(&mut self, node: Node) -> Result<()>;
    fn get_node(&self, id: Uuid) -> Result<Option<Node>>;
}
