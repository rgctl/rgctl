//! Graph schema definitions
//!
//! Defines the schema for the code knowledge graph including node and edge types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Node types in the code knowledge graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeType {
    /// Function or method
    Function,
    /// Class or struct
    Class,
    /// Struct (languages without classes)
    Struct,
    /// Enum
    Enum,
    /// Interface or trait
    Interface,
    /// Module or namespace
    Module,
    /// Variable or constant
    Variable,
    /// File
    File,
    /// Configuration key
    ConfigKey,
    /// Type alias
    TypeAlias,
    /// Macro
    Macro,
    /// Import statement
    Import,
    /// SQL table
    Table,
    /// External dependency (Docker image, package)
    Dependency,
    /// CI/CD job
    Job,
    /// Build/pipeline step
    BuildStep,
}

/// Edge types representing relationships between nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeType {
    /// Function calls another function
    Calls,
    /// Module/class contains a symbol
    Contains,
    /// Uses/imports
    Uses,
    /// Implements interface/trait
    Implements,
    /// Extends class/inherits
    Extends,
    /// References (variable reference)
    References,
    /// Instantiates (creates instance)
    Instantiates,
    /// Modifies (writes to variable)
    Modifies,
    /// Code uses a config key
    UsesConfig,
    /// Defined in (symbol defined in file)
    DefinedIn,
    /// Depends on (CI job dependency, pipeline ordering)
    DependsOn,
}

/// Node in the code knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Unique node identifier
    pub id: Uuid,

    /// Node type
    pub node_type: NodeType,

    /// Node name/identifier
    pub name: String,

    /// Fully qualified name
    pub qualified_name: Option<String>,

    /// Source file path
    pub file_path: Option<String>,

    /// Start line in source file
    pub start_line: Option<usize>,

    /// End line in source file
    pub end_line: Option<usize>,

    /// Additional properties as key-value pairs
    pub properties: HashMap<String, String>,

    /// Labels for categorization
    pub labels: Vec<String>,
}

impl Node {
    /// Create a new node
    pub fn new(node_type: NodeType, name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            node_type,
            name,
            qualified_name: None,
            file_path: None,
            start_line: None,
            end_line: None,
            properties: HashMap::new(),
            labels: Vec::new(),
        }
    }

    /// Set the qualified name
    pub fn with_qualified_name(mut self, qualified_name: String) -> Self {
        self.qualified_name = Some(qualified_name);
        self
    }

    /// Set the file path
    pub fn with_file_path(mut self, file_path: String) -> Self {
        self.file_path = Some(file_path);
        self
    }

    /// Set the source location
    pub fn with_location(mut self, start_line: usize, end_line: usize) -> Self {
        self.start_line = Some(start_line);
        self.end_line = Some(end_line);
        self
    }

    /// Add a property
    pub fn with_property(mut self, key: String, value: String) -> Self {
        self.properties.insert(key, value);
        self
    }

    /// Add a label
    pub fn with_label(mut self, label: String) -> Self {
        self.labels.push(label);
        self
    }

    /// Get a property value
    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.properties.get(key)
    }

    /// Check if node has a label
    pub fn has_label(&self, label: &str) -> bool {
        self.labels.contains(&label.to_string())
    }
}

/// Edge in the code knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source node ID
    pub from: Uuid,

    /// Target node ID
    pub to: Uuid,

    /// Edge type
    pub edge_type: EdgeType,

    /// Additional properties
    pub properties: HashMap<String, String>,

    /// Weight (for analysis algorithms)
    pub weight: f64,
}

impl Edge {
    /// Create a new edge
    pub fn new(from: Uuid, to: Uuid, edge_type: EdgeType) -> Self {
        Self {
            from,
            to,
            edge_type,
            properties: HashMap::new(),
            weight: 1.0,
        }
    }

    /// Set the weight
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    /// Add a property
    pub fn with_property(mut self, key: String, value: String) -> Self {
        self.properties.insert(key, value);
        self
    }

    /// Get a property value
    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.properties.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_node() {
        let node = Node::new(NodeType::Function, "test_function".to_string());
        assert_eq!(node.name, "test_function");
        assert_eq!(node.node_type, NodeType::Function);
        assert!(node.qualified_name.is_none());
    }

    #[test]
    fn test_node_builder() {
        let node = Node::new(NodeType::Function, "add".to_string())
            .with_qualified_name("math::add".to_string())
            .with_file_path("src/math.rs".to_string())
            .with_location(10, 15)
            .with_property("visibility".to_string(), "public".to_string())
            .with_label("critical".to_string());

        assert_eq!(node.name, "add");
        assert_eq!(node.qualified_name, Some("math::add".to_string()));
        assert_eq!(node.file_path, Some("src/math.rs".to_string()));
        assert_eq!(node.start_line, Some(10));
        assert_eq!(node.end_line, Some(15));
        assert_eq!(node.get_property("visibility"), Some(&"public".to_string()));
        assert!(node.has_label("critical"));
    }

    #[test]
    fn test_create_edge() {
        let from_id = Uuid::new_v4();
        let to_id = Uuid::new_v4();
        let edge = Edge::new(from_id, to_id, EdgeType::Calls);

        assert_eq!(edge.from, from_id);
        assert_eq!(edge.to, to_id);
        assert_eq!(edge.edge_type, EdgeType::Calls);
        assert_eq!(edge.weight, 1.0);
    }

    #[test]
    fn test_edge_builder() {
        let from_id = Uuid::new_v4();
        let to_id = Uuid::new_v4();
        let edge = Edge::new(from_id, to_id, EdgeType::Calls)
            .with_weight(2.5)
            .with_property("frequency".to_string(), "high".to_string());

        assert_eq!(edge.weight, 2.5);
        assert_eq!(edge.get_property("frequency"), Some(&"high".to_string()));
    }

    #[test]
    fn test_node_type_variants() {
        let types = vec![
            NodeType::Function,
            NodeType::Class,
            NodeType::Struct,
            NodeType::Enum,
            NodeType::Interface,
            NodeType::Module,
            NodeType::Variable,
            NodeType::File,
            NodeType::ConfigKey,
            NodeType::TypeAlias,
            NodeType::Macro,
            NodeType::Import,
            NodeType::Table,
            NodeType::Dependency,
            NodeType::Job,
            NodeType::BuildStep,
        ];
        assert_eq!(types.len(), 16);
    }

    #[test]
    fn test_edge_type_variants() {
        let types = vec![
            EdgeType::Calls,
            EdgeType::Contains,
            EdgeType::Uses,
            EdgeType::Implements,
            EdgeType::Extends,
            EdgeType::References,
            EdgeType::Instantiates,
            EdgeType::Modifies,
            EdgeType::UsesConfig,
            EdgeType::DefinedIn,
            EdgeType::DependsOn,
        ];
        assert_eq!(types.len(), 11);
    }
}
