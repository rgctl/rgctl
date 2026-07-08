//! Language plugin trait definitions
//!
//! This module defines the core plugin system for language and configuration format support.
//! Plugins enable extraction of symbols, relationships, and complexity metrics from source code.

mod call_extraction;
mod error;
mod registrar;

pub use call_extraction::{
    callee_name, containing_function, push_call_relation, walk_calls, CSHARP_CALL_KINDS,
    GO_CALL_KINDS, PYTHON_CALL_KINDS, RUST_CALL_KINDS,
};

pub use error::{Error, Result};
pub use registrar::ConfigFormatRegistrar;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A symbol extracted from source code (function, class, struct, variable, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol identifier (e.g., function name, class name)
    pub name: String,

    /// Symbol type (function, class, struct, enum, variable, etc.)
    pub symbol_type: SymbolType,

    /// Fully qualified name including module/namespace path
    pub qualified_name: Option<String>,

    /// Source location
    pub location: SourceLocation,

    /// Function signature (for functions/methods)
    pub signature: Option<String>,

    /// Return type (for functions)
    pub return_type: Option<String>,

    /// Parameters (for functions/methods)
    pub parameters: Vec<Parameter>,

    /// Fields (for structs/classes)
    pub fields: Vec<Field>,

    /// Modifiers (pub, async, static, etc.)
    pub modifiers: Vec<String>,

    /// Documentation comment
    pub documentation: Option<String>,

    /// Language-specific metadata
    pub metadata: serde_json::Value,
}

/// Type of symbol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolType {
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
    /// Type alias
    TypeAlias,
    /// Macro
    Macro,
    /// Import/use statement
    Import,
    /// SQL table definition (Phase 11.2)
    Table,
    /// External dependency (e.g. Docker base image)
    Dependency,
    /// CI/CD job definition
    Job,
    /// Build or pipeline step
    BuildStep,
    /// Ansible playbook
    AnsiblePlaybook,
    /// Ansible play within a playbook
    AnsiblePlay,
    /// Ansible task or handler step
    AnsibleTask,
    /// Ansible role
    AnsibleRole,
    /// Ansible handler
    AnsibleHandler,
    /// Ansible variable usage
    AnsibleVariable,
    /// Ansible template file
    AnsibleTemplate,
    /// Chef cookbook
    ChefCookbook,
    /// Chef recipe
    ChefRecipe,
    /// Chef resource declaration
    ChefResource,
    /// Chef attribute
    ChefAttribute,
    /// Chef ERB template
    ChefTemplate,
    /// Chef custom resource
    ChefCustomResource,
    /// Puppet module
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

/// Source code location
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    /// File path
    pub file: String,

    /// Start line (1-indexed)
    pub start_line: usize,

    /// End line (1-indexed)
    pub end_line: usize,

    /// Start column (0-indexed)
    pub start_column: usize,

    /// End column (0-indexed)
    pub end_column: usize,
}

/// Function/method parameter
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name
    pub name: String,

    /// Parameter type
    pub param_type: Option<String>,

    /// Default value
    pub default_value: Option<String>,
}

/// Struct/class field
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// Field name
    pub name: String,

    /// Field type
    pub field_type: Option<String>,

    /// Visibility (public, private, etc.)
    pub visibility: Option<String>,
}

/// A relationship between symbols (calls, uses, implements, extends, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    /// Source symbol identifier
    pub from: String,

    /// Target symbol identifier
    pub to: String,

    /// Relationship type
    pub relation_type: RelationType,

    /// Source location where relation occurs
    pub location: SourceLocation,

    /// Additional metadata
    pub metadata: serde_json::Value,

    /// Best-effort hint for target's qualified name (e.g., "Helper.transform")
    /// Used for cross-file symbol resolution when `to` is a simple name.
    /// This is a best-effort guess based on local context (variable types, imports, etc.)
    /// and may not always be accurate. The graph builder will fall back to fuzzy matching
    /// if the hint doesn't resolve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_qualified_hint: Option<String>,

    /// Best-effort hint for target's containing type/class (e.g., "Helper")
    /// Used when we can infer the class from variable type but not the full qualified name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_type_hint: Option<String>,
}

/// Type of relationship between symbols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationType {
    /// Function call
    Calls,
    /// Uses/imports
    Uses,
    /// Implements interface/trait
    Implements,
    /// Extends class/inherits
    Extends,
    /// Defines (module defines symbol)
    Defines,
    /// References (variable reference)
    References,
    /// Instantiates (creates instance)
    Instantiates,
    /// Modifies (writes to variable)
    Modifies,
    /// Job/step depends on another (CI pipelines)
    DependsOn,
    /// Playbook/play includes a role
    IncludesRole,
    /// Role meta dependency
    DependsOnRole,
    /// Play runs a task
    ExecutesTask,
    /// Task notifies handler
    NotifiesHandler,
    /// Playbook imports another playbook
    IncludesPlaybook,
    /// Task uses a Jinja2 variable
    UsesVariable,
    /// Task renders template
    RendersTemplate,
    /// Cookbook depends on another cookbook
    DependsOnCookbook,
    /// Recipe includes another recipe
    IncludesRecipe,
    /// Recipe declares a resource
    DeclaresResource,
    /// Resource uses ERB template
    UsesTemplate,
    /// Cookbook defines attribute
    DefinesAttribute,
    /// Resource notifies another resource
    NotifiesResource,
    /// Puppet module depends on another module
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

/// Code complexity metrics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplexityMetrics {
    /// Cyclomatic complexity (decision points + 1)
    pub cyclomatic: usize,

    /// Cognitive complexity (nested conditions, recursion weight)
    pub cognitive: usize,

    /// Lines of code
    pub loc: usize,

    /// Number of parameters
    pub parameters: usize,

    /// Nesting depth
    pub nesting_depth: usize,

    /// Number of return statements
    pub returns: usize,
}

/// Language plugin capabilities
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageCapabilities {
    /// Can extract function definitions
    pub extracts_functions: bool,

    /// Can extract class/struct definitions
    pub extracts_types: bool,

    /// Can extract module/namespace structure
    pub extracts_modules: bool,

    /// Can extract relationships (calls, uses, etc.)
    pub extracts_relations: bool,

    /// Can calculate complexity metrics
    pub calculates_complexity: bool,

    /// Can extract documentation comments
    pub extracts_documentation: bool,

    /// Supports incremental parsing
    pub supports_incremental: bool,
}

impl Default for LanguageCapabilities {
    fn default() -> Self {
        Self {
            extracts_functions: true,
            extracts_types: true,
            extracts_modules: true,
            extracts_relations: true,
            calculates_complexity: true,
            extracts_documentation: true,
            supports_incremental: false,
        }
    }
}

/// Language plugin trait for extracting symbols and relationships from source code
pub trait LanguagePlugin: Send + Sync {
    /// Unique language identifier (e.g., "rust", "python", "typescript")
    fn language_id(&self) -> &str;

    /// File extensions this plugin handles (e.g., ["rs", "rust"])
    fn file_extensions(&self) -> Vec<&str>;

    /// TreeSitter grammar for this language (if available)
    fn grammar(&self) -> Option<tree_sitter::Language>;

    /// Plugin capabilities
    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities::default()
    }

    /// Extract symbols from source code
    ///
    /// # Arguments
    /// * `file_path` - Path to the source file
    /// * `source` - Source code as bytes
    ///
    /// # Returns
    /// Vector of extracted symbols
    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>>;

    /// Extract relationships between symbols
    ///
    /// # Arguments
    /// * `file_path` - Path to the source file
    /// * `source` - Source code as bytes
    /// * `symbols` - Previously extracted symbols (for context)
    ///
    /// # Returns
    /// Vector of extracted relationships
    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>>;

    /// Calculate complexity metrics for a symbol
    ///
    /// # Arguments
    /// * `symbol` - The symbol to analyze
    /// * `source` - Source code as bytes
    ///
    /// # Returns
    /// Complexity metrics if calculable
    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        source: &[u8],
    ) -> Result<Option<ComplexityMetrics>>;

    /// Check if this plugin can handle a file
    fn can_handle(&self, file_path: &Path) -> bool {
        self.matches_path(&file_path.to_string_lossy())
    }

    /// Path-based routing (supports extensionless or path-heuristic languages).
    fn matches_path(&self, path: &str) -> bool {
        let normalized = path.replace('\\', "/");
        let path_obj = Path::new(&normalized);
        if let Some(ext) = path_obj.extension().and_then(|e| e.to_str()) {
            if self.file_extensions().contains(&ext) {
                return true;
            }
        }
        if path_obj
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| {
                self.file_extensions()
                    .iter()
                    .any(|ext| name.eq_ignore_ascii_case(ext))
            })
        {
            return true;
        }
        false
    }
}

/// Configuration format plugin for extracting key-value pairs from config files
pub trait ConfigFormatPlugin: Send + Sync {
    /// Format identifier (e.g., "yaml", "json", "toml")
    fn format_id(&self) -> &str;

    /// File extensions this plugin handles
    fn file_extensions(&self) -> Vec<&str>;

    /// Extract configuration keys from a config file
    ///
    /// # Arguments
    /// * `file_path` - Path to config file
    /// * `source` - File content as bytes
    ///
    /// # Returns
    /// Vector of (key_path, value) tuples where key_path is dot-separated (e.g., "server.port")
    fn extract_config_keys(&self, file_path: &Path, source: &[u8]) -> Result<Vec<ConfigKey>>;

    /// Check if this plugin can handle a file
    fn can_handle(&self, file_path: &Path) -> bool {
        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            self.file_extensions().contains(&ext)
        } else {
            false
        }
    }
}

/// A configuration key extracted from a config file
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigKey {
    /// Dot-separated key path (e.g., "database.connection.timeout")
    pub key_path: String,

    /// String representation of value
    pub value: String,

    /// Value type
    pub value_type: ConfigValueType,

    /// Source location
    pub location: SourceLocation,
}

/// Type of configuration value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConfigValueType {
    /// String value
    String,
    /// Number (int or float)
    Number,
    /// Boolean
    Boolean,
    /// Array/list
    Array,
    /// Object/map
    Object,
    /// Null
    Null,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Mock language plugin for testing
    struct MockPlugin;

    impl LanguagePlugin for MockPlugin {
        fn language_id(&self) -> &str {
            "mock"
        }

        fn file_extensions(&self) -> Vec<&str> {
            vec!["mock", "mck"]
        }

        fn grammar(&self) -> Option<tree_sitter::Language> {
            None
        }

        fn extract_symbols(&self, file_path: &Path, _source: &[u8]) -> Result<Vec<Symbol>> {
            Ok(vec![Symbol {
                name: "test_function".to_string(),
                symbol_type: SymbolType::Function,
                qualified_name: Some("mock::test_function".to_string()),
                location: SourceLocation {
                    file: file_path.to_string_lossy().to_string(),
                    start_line: 1,
                    end_line: 5,
                    start_column: 0,
                    end_column: 1,
                },
                signature: Some("fn test_function(x: i32) -> i32".to_string()),
                return_type: Some("i32".to_string()),
                parameters: vec![Parameter {
                    name: "x".to_string(),
                    param_type: Some("i32".to_string()),
                    default_value: None,
                }],
                fields: vec![],
                modifiers: vec!["pub".to_string()],
                documentation: Some("Test function".to_string()),
                metadata: serde_json::json!({}),
            }])
        }

        fn extract_relations(
            &self,
            file_path: &Path,
            _source: &[u8],
            _symbols: &[Symbol],
        ) -> Result<Vec<Relation>> {
            Ok(vec![Relation {
                from: "test_function".to_string(),
                to: "helper_function".to_string(),
                relation_type: RelationType::Calls,
                location: SourceLocation {
                    file: file_path.to_string_lossy().to_string(),
                    start_line: 3,
                    end_line: 3,
                    start_column: 4,
                    end_column: 20,
                },
                metadata: serde_json::json!({}),
                to_qualified_hint: None,
                to_type_hint: None,
            }])
        }

        fn calculate_complexity(
            &self,
            _symbol: &Symbol,
            _source: &[u8],
        ) -> Result<Option<ComplexityMetrics>> {
            Ok(Some(ComplexityMetrics {
                cyclomatic: 3,
                cognitive: 5,
                loc: 10,
                parameters: 1,
                nesting_depth: 2,
                returns: 1,
            }))
        }
    }

    /// Mock config plugin for testing
    struct MockConfigPlugin;

    impl ConfigFormatPlugin for MockConfigPlugin {
        fn format_id(&self) -> &str {
            "mock-config"
        }

        fn file_extensions(&self) -> Vec<&str> {
            vec!["conf"]
        }

        fn extract_config_keys(&self, file_path: &Path, _source: &[u8]) -> Result<Vec<ConfigKey>> {
            Ok(vec![ConfigKey {
                key_path: "server.port".to_string(),
                value: "8080".to_string(),
                value_type: ConfigValueType::Number,
                location: SourceLocation {
                    file: file_path.to_string_lossy().to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 16,
                },
            }])
        }
    }

    #[test]
    fn test_mock_plugin_language_id() {
        let plugin = MockPlugin;
        assert_eq!(plugin.language_id(), "mock");
    }

    #[test]
    fn test_mock_plugin_file_extensions() {
        let plugin = MockPlugin;
        assert_eq!(plugin.file_extensions(), vec!["mock", "mck"]);
    }

    #[test]
    fn test_mock_plugin_can_handle() {
        let plugin = MockPlugin;
        assert!(plugin.can_handle(Path::new("test.mock")));
        assert!(plugin.can_handle(Path::new("test.mck")));
        assert!(!plugin.can_handle(Path::new("test.rs")));
    }

    #[test]
    fn test_mock_plugin_extract_symbols() {
        let plugin = MockPlugin;
        let path = PathBuf::from("test.mock");
        let source = b"mock source code";
        let symbols = plugin.extract_symbols(&path, source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "test_function");
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
        assert_eq!(
            symbols[0].qualified_name,
            Some("mock::test_function".to_string())
        );
        assert_eq!(symbols[0].parameters.len(), 1);
        assert_eq!(symbols[0].parameters[0].name, "x");
    }

    #[test]
    fn test_mock_plugin_extract_relations() {
        let plugin = MockPlugin;
        let path = PathBuf::from("test.mock");
        let source = b"mock source code";
        let symbols = vec![];
        let relations = plugin.extract_relations(&path, source, &symbols).unwrap();

        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].from, "test_function");
        assert_eq!(relations[0].to, "helper_function");
        assert_eq!(relations[0].relation_type, RelationType::Calls);
    }

    #[test]
    fn test_mock_plugin_calculate_complexity() {
        let plugin = MockPlugin;
        let symbol = Symbol {
            name: "test".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: None,
            location: SourceLocation {
                file: "test.mock".to_string(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 10,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
        };
        let source = b"mock source";
        let complexity = plugin.calculate_complexity(&symbol, source).unwrap();

        assert!(complexity.is_some());
        let complexity = complexity.unwrap();
        assert_eq!(complexity.cyclomatic, 3);
        assert_eq!(complexity.cognitive, 5);
        assert_eq!(complexity.loc, 10);
    }

    #[test]
    fn test_mock_config_plugin_format_id() {
        let plugin = MockConfigPlugin;
        assert_eq!(plugin.format_id(), "mock-config");
    }

    #[test]
    fn test_mock_config_plugin_extract_config_keys() {
        let plugin = MockConfigPlugin;
        let path = PathBuf::from("test.conf");
        let source = b"server.port = 8080";
        let keys = plugin.extract_config_keys(&path, source).unwrap();

        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key_path, "server.port");
        assert_eq!(keys[0].value, "8080");
        assert_eq!(keys[0].value_type, ConfigValueType::Number);
    }

    #[test]
    fn test_symbol_type_variants() {
        let types = vec![
            SymbolType::Function,
            SymbolType::Class,
            SymbolType::Struct,
            SymbolType::Enum,
            SymbolType::Interface,
            SymbolType::Module,
            SymbolType::Variable,
            SymbolType::TypeAlias,
            SymbolType::Macro,
            SymbolType::Import,
            SymbolType::Table,
            SymbolType::Dependency,
            SymbolType::Job,
            SymbolType::BuildStep,
        ];
        assert_eq!(types.len(), 14);
    }

    #[test]
    fn test_relation_type_variants() {
        let types = [
            RelationType::Calls,
            RelationType::Uses,
            RelationType::Implements,
            RelationType::Extends,
            RelationType::Defines,
            RelationType::References,
            RelationType::Instantiates,
            RelationType::Modifies,
            RelationType::DependsOn,
        ];
        assert_eq!(types.len(), 9);
    }

    #[test]
    fn test_capabilities_default() {
        let caps = LanguageCapabilities::default();
        assert!(caps.extracts_functions);
        assert!(caps.extracts_types);
        assert!(caps.extracts_modules);
        assert!(caps.extracts_relations);
        assert!(caps.calculates_complexity);
        assert!(caps.extracts_documentation);
        assert!(!caps.supports_incremental);
    }

    #[test]
    fn test_source_location_serialization() {
        let loc = SourceLocation {
            file: "test.rs".to_string(),
            start_line: 10,
            end_line: 20,
            start_column: 4,
            end_column: 8,
        };
        let json = serde_json::to_string(&loc).unwrap();
        let deserialized: SourceLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(loc, deserialized);
    }

    #[test]
    fn test_complexity_metrics_serialization() {
        let metrics = ComplexityMetrics {
            cyclomatic: 5,
            cognitive: 8,
            loc: 42,
            parameters: 3,
            nesting_depth: 4,
            returns: 2,
        };
        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: ComplexityMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(metrics, deserialized);
    }
}
