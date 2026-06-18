//! Graph schema definitions
//!
//! Defines the schema for the code knowledge graph including node and edge types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Current graph schema version (Phase 12.0 enrichment).
pub const GRAPH_SCHEMA_VERSION: u32 = 2;

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
    /// Ansible playbook (Phase 16)
    AnsiblePlaybook,
    /// Ansible play
    AnsiblePlay,
    /// Ansible task
    AnsibleTask,
    /// Ansible role
    AnsibleRole,
    /// Ansible handler
    AnsibleHandler,
    /// Ansible variable reference
    AnsibleVariable,
    /// Ansible Jinja2 template
    AnsibleTemplate,
    /// Chef cookbook (Phase 17)
    ChefCookbook,
    /// Chef recipe
    ChefRecipe,
    /// Chef resource declaration
    ChefResource,
    /// Chef node attribute
    ChefAttribute,
    /// Chef ERB template
    ChefTemplate,
    /// Chef custom resource (LWRP/HWRP)
    ChefCustomResource,
    /// Puppet module (Phase 18)
    PuppetModule,
    /// Puppet class
    PuppetClass,
    /// Puppet defined type
    PuppetDefinedType,
    /// Puppet resource declaration
    PuppetResource,
    /// Puppet variable
    PuppetVariable,
    /// Puppet fact reference
    PuppetFact,
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
    /// Playbook or play includes a role (Phase 16)
    IncludesRole,
    /// Role depends on another role via meta/main.yml
    DependsOnRole,
    /// Play executes a task
    ExecutesTask,
    /// Task notifies a handler
    NotifiesHandler,
    /// Playbook imports another playbook
    IncludesPlaybook,
    /// Task renders a template file
    RendersTemplate,
    /// Cookbook depends on another cookbook (Phase 17)
    DependsOnCookbook,
    /// Recipe includes another recipe
    IncludesRecipe,
    /// Recipe declares a Chef resource
    DeclaresResource,
    /// Resource uses an ERB template
    UsesTemplate,
    /// Cookbook defines an attribute
    DefinesAttribute,
    /// Resource notifies another resource
    NotifiesResource,
    /// Puppet module depends on another module (Phase 18)
    DependsOnModule,
    /// Puppet class includes another class
    IncludesClass,
    /// Puppet class inherits from another class
    InheritsClass,
    /// Puppet resource requires another resource
    RequiresResource,
    /// Puppet class or resource uses a fact
    UsesFact,
}

/// Function parameter stored on graph nodes (Phase 12.0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type if known
    pub param_type: Option<String>,
    /// Default value if any
    pub default_value: Option<String>,
}

/// Call classification for `Calls` edges (Phase 12.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallType {
    /// Direct function call `foo()`
    Direct,
    /// Indirect call via function pointer
    Indirect,
    /// Virtual / trait / interface dispatch
    Virtual,
    /// Macro expansion
    Macro,
}

/// Variable access classification for `Uses` / `Modifies` edges (Phase 12.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccessType {
    /// Read access
    Read,
    /// Write access
    Write,
    /// Read and write
    ReadWrite,
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

    /// Full function/method signature (Phase 12.0)
    #[serde(default)]
    pub signature: Option<String>,

    /// Return type if known (Phase 12.0)
    #[serde(default)]
    pub return_type: Option<String>,

    /// Structured parameters (Phase 12.0)
    #[serde(default)]
    pub parameters: Vec<GraphParameter>,

    /// BLAKE3 hash of symbol body for change detection (Phase 12.0)
    #[serde(default)]
    pub code_hash: Option<String>,

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
            signature: None,
            return_type: None,
            parameters: Vec::new(),
            code_hash: None,
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

    /// Set the function signature.
    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }

    /// Set the return type.
    pub fn with_return_type(mut self, return_type: impl Into<String>) -> Self {
        self.return_type = Some(return_type.into());
        self
    }

    /// Set structured parameters.
    pub fn with_parameters(mut self, parameters: Vec<GraphParameter>) -> Self {
        self.parameters = parameters;
        self
    }

    /// Set the code body hash for change detection.
    pub fn with_code_hash(mut self, code_hash: impl Into<String>) -> Self {
        self.code_hash = Some(code_hash.into());
        self
    }

    /// Signature text, preferring first-class field over legacy property.
    pub fn signature_text(&self) -> Option<&str> {
        self.signature
            .as_deref()
            .or_else(|| self.properties.get("signature").map(String::as_str))
    }

    /// Return type, preferring first-class field over legacy property.
    pub fn return_type_text(&self) -> Option<&str> {
        self.return_type
            .as_deref()
            .or_else(|| self.properties.get("return_type").map(String::as_str))
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

    /// Call kind for `Calls` edges (Phase 12.0)
    #[serde(default)]
    pub call_type: Option<CallType>,

    /// Access kind for `Uses` / `Modifies` edges (Phase 12.0)
    #[serde(default)]
    pub access_type: Option<AccessType>,

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
            call_type: None,
            access_type: None,
            properties: HashMap::new(),
            weight: 1.0,
        }
    }

    /// Set the weight
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    /// Set call type metadata.
    pub fn with_call_type(mut self, call_type: CallType) -> Self {
        self.call_type = Some(call_type);
        self
    }

    /// Set variable access metadata.
    pub fn with_access_type(mut self, access_type: AccessType) -> Self {
        self.access_type = Some(access_type);
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
    fn test_node_signature_fields() {
        let node = Node::new(NodeType::Function, "process".to_string())
            .with_signature("fn process(data: &[u8]) -> Result<()>")
            .with_return_type("Result<()>".to_string())
            .with_parameters(vec![GraphParameter {
                name: "data".to_string(),
                param_type: Some("&[u8]".to_string()),
                default_value: None,
            }])
            .with_code_hash("abc123");

        assert_eq!(
            node.signature_text(),
            Some("fn process(data: &[u8]) -> Result<()>")
        );
        assert_eq!(node.return_type_text(), Some("Result<()>"));
        assert_eq!(node.parameters.len(), 1);
        assert_eq!(node.code_hash.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_edge_call_type() {
        let edge = Edge::new(Uuid::new_v4(), Uuid::new_v4(), EdgeType::Calls)
            .with_call_type(CallType::Virtual);
        assert_eq!(edge.call_type, Some(CallType::Virtual));
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
            NodeType::AnsiblePlaybook,
            NodeType::AnsiblePlay,
            NodeType::AnsibleTask,
            NodeType::AnsibleRole,
            NodeType::AnsibleHandler,
            NodeType::AnsibleVariable,
            NodeType::AnsibleTemplate,
            NodeType::ChefCookbook,
            NodeType::ChefRecipe,
            NodeType::ChefResource,
            NodeType::ChefAttribute,
            NodeType::ChefTemplate,
            NodeType::ChefCustomResource,
            NodeType::PuppetModule,
            NodeType::PuppetClass,
            NodeType::PuppetDefinedType,
            NodeType::PuppetResource,
            NodeType::PuppetVariable,
            NodeType::PuppetFact,
        ];
        assert_eq!(types.len(), 35);
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
            EdgeType::IncludesRole,
            EdgeType::DependsOnRole,
            EdgeType::ExecutesTask,
            EdgeType::NotifiesHandler,
            EdgeType::IncludesPlaybook,
            EdgeType::RendersTemplate,
            EdgeType::DependsOnCookbook,
            EdgeType::IncludesRecipe,
            EdgeType::DeclaresResource,
            EdgeType::UsesTemplate,
            EdgeType::DefinesAttribute,
            EdgeType::NotifiesResource,
            EdgeType::DependsOnModule,
            EdgeType::IncludesClass,
            EdgeType::InheritsClass,
            EdgeType::RequiresResource,
            EdgeType::UsesFact,
        ];
        assert_eq!(types.len(), 28);
    }
}
