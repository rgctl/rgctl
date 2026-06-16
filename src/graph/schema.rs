//! Graph schema definitions
//!
//! Task 1.4.1: Define graph schema (NodeType, EdgeType, Node, Edge)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// Placeholder types - will be fully implemented in Task 1.4.1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: Uuid,
    pub to: Uuid,
}
