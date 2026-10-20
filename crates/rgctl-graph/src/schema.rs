//! Graph schema definitions
//!
//! Defines the schema for the code knowledge graph including node and edge types.

use crate::lazy_collections::LazyStringMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;
use uuid::Uuid;

use crate::structural_sketch::TokenBloom;

mod serde_arc_str {
    use super::*;

    pub fn serialize<S>(value: &SharedStr, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(value.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SharedStr, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(SharedStr::from(s))
    }
}

mod serde_arc_str_option {
    use super::*;

    pub fn serialize<S>(value: &Option<SharedStr>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => serializer.serialize_some(v.as_str()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<SharedStr>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(deserializer)?;
        Ok(opt.map(SharedStr::from))
    }
}

/// Interned graph string stored as a shared handle.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SharedStr(Arc<str>);

impl SharedStr {
    /// Borrow the UTF-8 contents.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SharedStr {
    fn default() -> Self {
        Self::from("")
    }
}

impl std::ops::Deref for SharedStr {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for SharedStr {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for SharedStr {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<std::path::Path> for SharedStr {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(self.as_str())
    }
}

impl AsRef<std::ffi::OsStr> for SharedStr {
    fn as_ref(&self) -> &std::ffi::OsStr {
        std::ffi::OsStr::new(self.as_str())
    }
}

impl From<String> for SharedStr {
    fn from(value: String) -> Self {
        Self(Arc::from(value))
    }
}

impl From<&str> for SharedStr {
    fn from(value: &str) -> Self {
        Self(Arc::from(value))
    }
}

impl From<Arc<str>> for SharedStr {
    fn from(value: Arc<str>) -> Self {
        Self(value)
    }
}

impl From<SharedStr> for String {
    fn from(value: SharedStr) -> Self {
        value.0.to_string()
    }
}

impl From<SharedStr> for Arc<str> {
    fn from(value: SharedStr) -> Self {
        value.0
    }
}

impl std::fmt::Display for SharedStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<&str> for SharedStr {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_ref() == *other
    }
}

impl PartialEq<str> for SharedStr {
    fn eq(&self, other: &str) -> bool {
        self.0.as_ref() == other
    }
}

impl PartialEq<String> for SharedStr {
    fn eq(&self, other: &String) -> bool {
        self.0.as_ref() == other.as_str()
    }
}

/// Wrap a string slice in a shared handle for hot node fields.
pub fn arc_str(value: impl AsRef<str>) -> SharedStr {
    SharedStr::from(value.as_ref())
}

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
    /// Annotation type (e.g. Java `@interface`)
    Annotation,
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
    /// Puppet node definition (`node { ... }`)
    PuppetNode,
    /// Konveyor Kantra ruleset container (discover `--with-kantra`)
    KantraRuleset,
    /// Konveyor Kantra migration rule
    KantraRule,
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
    /// Subject is annotated with a type
    AnnotatedWith,
    /// Sealed type permits another type
    Permits,
    /// Kantra rule violation on a code graph node
    Violates,
    /// Unknown or forward-compatible edge type; excluded from call-graph traversals.
    #[serde(other)]
    Unknown,
}

impl EdgeType {
    /// Whether this edge type participates in call-graph blast-radius traversals.
    pub fn is_call_traversal(self) -> bool {
        matches!(self, EdgeType::Calls)
    }
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
    #[serde(with = "serde_arc_str")]
    pub name: SharedStr,

    /// Fully qualified name
    #[serde(with = "serde_arc_str_option", default)]
    pub qualified_name: Option<SharedStr>,

    /// Full function/method signature (Phase 12.0)
    #[serde(with = "serde_arc_str_option", default)]
    pub signature: Option<SharedStr>,

    /// Return type if known (Phase 12.0)
    #[serde(with = "serde_arc_str_option", default)]
    pub return_type: Option<SharedStr>,

    /// Structured parameters (Phase 12.0)
    #[serde(default)]
    pub parameters: Vec<GraphParameter>,

    /// BLAKE3 hash of symbol body for change detection (Phase 12.0)
    #[serde(with = "serde_arc_str_option", default)]
    pub code_hash: Option<SharedStr>,

    /// 256-bit token bloom sketch (eager structural index at extract time).
    #[serde(default)]
    pub token_bloom: Option<TokenBloom>,

    /// Source file path
    #[serde(with = "serde_arc_str_option", default)]
    pub file_path: Option<SharedStr>,

    /// Start line in source file
    pub start_line: Option<usize>,

    /// End line in source file
    pub end_line: Option<usize>,

    /// Additional properties as key-value pairs
    #[serde(default)]
    pub properties: LazyStringMap,

    /// Labels for categorization
    pub labels: Vec<String>,
}

impl Node {
    /// Create a new node
    pub fn new(node_type: NodeType, name: impl Into<SharedStr>) -> Self {
        Self {
            id: Uuid::new_v4(),
            node_type,
            name: name.into(),
            qualified_name: None,
            signature: None,
            return_type: None,
            parameters: Vec::new(),
            code_hash: None,
            token_bloom: None,
            file_path: None,
            start_line: None,
            end_line: None,
            properties: LazyStringMap::new(),
            labels: Vec::new(),
        }
    }

    /// Set the qualified name and refresh the deterministic [`id`](Self::id) when a file path is set.
    pub fn with_qualified_name(mut self, qualified_name: impl Into<SharedStr>) -> Self {
        self.qualified_name = Some(qualified_name.into());
        self.refresh_deterministic_id();
        self
    }

    /// Set the file path and assign a deterministic [`id`](Self::id) from stable facets.
    pub fn with_file_path(mut self, file_path: impl Into<SharedStr>) -> Self {
        self.file_path = Some(file_path.into());
        self.refresh_deterministic_id();
        self
    }

    /// Set the source location (does not affect deterministic [`id`](Self::id)).
    pub fn with_location(mut self, start_line: usize, end_line: usize) -> Self {
        self.start_line = Some(start_line);
        self.end_line = Some(end_line);
        self
    }

    fn refresh_deterministic_id(&mut self) {
        if self.file_path.is_none() {
            return;
        }
        self.id = crate::stable_key::deterministic_node_id(
            self.file_path.as_deref().map(|s| s.as_ref()),
            self.name.as_ref(),
            self.node_type,
            self.qualified_name.as_deref().map(|s| s.as_ref()),
        );
    }

    /// Set the function signature.
    pub fn with_signature(mut self, signature: impl Into<SharedStr>) -> Self {
        self.signature = Some(signature.into());
        self
    }

    /// Set the return type.
    pub fn with_return_type(mut self, return_type: impl Into<SharedStr>) -> Self {
        self.return_type = Some(return_type.into());
        self
    }

    /// Set structured parameters.
    pub fn with_parameters(mut self, parameters: Vec<GraphParameter>) -> Self {
        self.parameters = parameters;
        self
    }

    /// Set the code body hash for change detection.
    pub fn with_code_hash(mut self, code_hash: impl Into<SharedStr>) -> Self {
        self.code_hash = Some(code_hash.into());
        self
    }

    /// Set the eager 256-bit token bloom sketch for this symbol.
    pub fn with_token_bloom(mut self, token_bloom: TokenBloom) -> Self {
        self.token_bloom = Some(token_bloom);
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
    pub fn get_property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }

    /// Check if node has a label
    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l == label)
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
    #[serde(default)]
    pub properties: LazyStringMap,

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
            properties: LazyStringMap::new(),
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
    pub fn get_property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }

    /// Topology-only edge used for columnar content digests.
    ///
    /// Columnar v2 edge rows store only `(from, to, edge_type)`. Properties,
    /// `call_type`, `access_type`, and non-default weights are not persisted, so
    /// digest hashing must ignore them — otherwise compact/rematerialize cycles
    /// thrash the header digest without a real topology change.
    pub fn for_columnar_digest(&self) -> Self {
        Self::new(self.from, self.to, self.edge_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_node() {
        let node = Node::new(NodeType::Function, "test_function".to_string());
        assert_eq!(node.name.as_str(), "test_function");
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

        assert_eq!(node.name.as_str(), "add");
        assert_eq!(node.qualified_name.as_deref(), Some("math::add"));
        assert_eq!(node.file_path.as_deref(), Some("src/math.rs"));
        assert_eq!(node.start_line, Some(10));
        assert_eq!(node.end_line, Some(15));
        assert_eq!(node.get_property("visibility"), Some("public"));
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
        assert_eq!(edge.get_property("frequency"), Some("high"));
    }

    #[test]
    fn test_node_type_variants() {
        let types = vec![
            NodeType::Function,
            NodeType::Class,
            NodeType::Struct,
            NodeType::Enum,
            NodeType::Interface,
            NodeType::Annotation,
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
            NodeType::PuppetNode,
            NodeType::KantraRuleset,
            NodeType::KantraRule,
        ];
        assert_eq!(types.len(), 39);
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
            EdgeType::AnnotatedWith,
            EdgeType::Permits,
            EdgeType::Violates,
            EdgeType::Unknown,
        ];
        assert_eq!(types.len(), 32);
    }

    #[test]
    fn annotation_node_and_edge_types_exist() {
        let node = Node::new(NodeType::Annotation, "AddOnStartup".to_string());
        assert_eq!(node.node_type, NodeType::Annotation);
        let edge = Edge::new(Uuid::new_v4(), Uuid::new_v4(), EdgeType::AnnotatedWith);
        assert_eq!(edge.edge_type, EdgeType::AnnotatedWith);
        let permits = Edge::new(Uuid::new_v4(), Uuid::new_v4(), EdgeType::Permits);
        assert_eq!(permits.edge_type, EdgeType::Permits);
    }
}
