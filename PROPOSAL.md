# rBuilder: Code Knowledge Graph System - Proposal

## Executive Summary

**rBuilder is a knowledge graph system designed to arm AI coding agents with deep, queryable codebase understanding.**

It transforms code repositories into semantically-rich graph databases that AI agents (like Claude Code) can interrogate via natural language. This enables agents to perform accurate impact analysis, architecture review, and refactoring suggestions without reading hundreds of files.

**Key Differentiator**: Hybrid NLP query system that answers 90% of questions without LLM calls (using pattern matching and learned patterns), with LLM fallback for complex queries.

**Core Technologies**: Rust, Tree-sitter (local AST parsing), IndraDB (embedded graph), MCP protocol (AI agent integration), hybrid NLP (pattern matching + optional LLM).

---

## 1. Architecture Overview

### 1.1 Core Philosophy

**Hybrid Processing Model:**
- **Code artifacts** → Tree-sitter AST extraction (local, no network)
- **Non-code assets** → Optional LLM extraction (configurable)
- **Graph storage** → Portable, embedded graph database
- **Analysis** → Precomputed at index-time (communities, complexity, dependencies)

**Key Differentiators:**
- Native Rust performance for large-scale repositories
- Pluggable graph backends with export/import capabilities
- Rule-based labeling engine (JSON-configurable)
- Cross-language semantic IDL generation

---

## 2. Core Components

### 2.1 MCP Server Layer (`mcp/`)

**Purpose**: Integrate with AI coding agents (Claude Code, Cursor, etc.) via Model Context Protocol.

```
mcp/
├── server.rs           # MCP server implementation
├── tools.rs            # MCP tool definitions
├── resources.rs        # MCP resource providers
└── transport.rs        # stdio/HTTP transport
```

**Responsibilities:**
- Expose graph as queryable MCP tools
- Handle natural language queries from agents
- Provide compressed, context-efficient responses
- Support both stdio (local) and HTTP (shared server) transports

**Key MCP Tools:**
```rust
pub enum MCPTool {
    QueryCodebase { question: String },          // NLP query
    ImpactAnalysis { symbol: String, depth: u32 },
    FindByComplexity { min: u32, labels: Vec<String> },
    GetCommunityInfo { community: Option<String> },
    ConfigAnalysis { analysis_type: ConfigAnalysisType },
    SymbolInfo { name: String, include_callers: bool },
    DiffAnalysis { since: String },              // What changed?
}
```

**Example MCP Configuration** (for Claude Code):
```json
{
  "rbuilder": {
    "command": "rbuilder",
    "args": ["mcp", "serve", "--transport", "stdio"],
    "cwd": "/path/to/project"
  }
}
```

### 2.2 Extraction Layer (`extraction/`)

```
extraction/
├── ast_parser.rs       # Tree-sitter integration
├── language_support.rs # 36+ language grammars
├── symbol_extractor.rs # Nodes: functions, classes, types
├── relation_builder.rs # Edges: calls, imports, inheritance
└── confidence.rs       # EXTRACTED | INFERRED | AMBIGUOUS
```

**Responsibilities:**
- Parse source files using Tree-sitter grammars
- Extract symbols (functions, classes, variables, types)
- Build relationships (calls, imports, inheritance, data flow)
- Tag extraction confidence levels
- Handle multi-language repositories

**Key Technologies:**
- `tree-sitter` crate for AST parsing
- Grammar support: Rust, Python, TypeScript/JavaScript, Go, Java, C/C++, etc.
- Parallel processing via `rayon` for large codebases

### 2.2 Graph Backend Layer (`graph/`)

```
graph/
├── backend/
│   ├── trait.rs        # GraphBackend trait (abstraction)
│   ├── indradb.rs      # IndraDB implementation (default)
│   ├── neo4j.rs        # Neo4j connector (optional)
│   └── sled_graph.rs   # Sled-based embedded option
├── schema.rs           # Node/Edge type definitions
├── query.rs            # Query DSL
└── export.rs           # GraphML, Cypher, JSON export
```

**Graph Backend Choice: IndraDB**

**Why IndraDB:**
- Pure Rust, embeddable, no external dependencies
- Supports complex graph queries and traversals
- Pluggable storage (RocksDB, in-memory, custom)
- Portable: entire graph exports as binary or JSON
- Performance: designed for large-scale graphs
- Active development, MIT licensed

**Alternative Backends (Pluggable via Trait):**
- **Neo4j**: For teams with existing Neo4j infrastructure
- **Sled**: Ultra-lightweight embedded KV with graph layer
- **Custom**: Export to GraphML/JSON for external tools

**Node Schema:**
```rust
enum NodeType {
    File { path: String, language: String },
    Function { name: String, signature: String, complexity: u32 },
    Class { name: String, methods: Vec<String> },
    Module { name: String, exports: Vec<String> },
    Variable { name: String, type_annotation: Option<String> },
    Type { name: String, definition: String },
}

struct Node {
    id: Uuid,
    node_type: NodeType,
    metadata: HashMap<String, Value>,
    labels: Vec<String>,  // Rule-based labels
    community_id: Option<u32>,
    confidence: Confidence,
}
```

**Edge Schema:**
```rust
enum EdgeType {
    Calls { from: Uuid, to: Uuid, call_count: u32 },
    Imports { from: Uuid, to: Uuid },
    Inherits { child: Uuid, parent: Uuid },
    Implements { class: Uuid, interface: Uuid },
    References { from: Uuid, to: Uuid },
    Contains { parent: Uuid, child: Uuid },  // File contains function
    DataFlow { from: Uuid, to: Uuid, variable: String },
}
```

### 2.3 Analysis Layer (`analysis/`)

```
analysis/
├── community_detection.rs  # Leiden algorithm
├── complexity.rs           # Cyclomatic, cognitive complexity
├── centrality.rs           # PageRank, betweenness
├── pattern_detection.rs    # Design patterns, anti-patterns
└── dependency_analysis.rs  # Impact analysis, circular deps
```

**Community Detection:**
- **Algorithm**: Leiden (superior to Louvain)
- **Crate**: Implement using `petgraph` + custom Leiden
- **Output**: Hierarchical communities, modularity scores
- **Use case**: Group related code, identify architectural modules

**Complexity Metrics:**
- **Cyclomatic complexity**: Control flow branches
- **Cognitive complexity**: Human readability metric
- **Halstead metrics**: Volume, difficulty, effort
- **Classification**: `LOW (0-5) | MEDIUM (6-10) | HIGH (11-20) | CRITICAL (21+)`

**Centrality Metrics:**
- **PageRank**: Identify "god classes/functions"
- **Betweenness**: Find architectural bridges
- **Degree centrality**: Most connected nodes

### 2.4 Semantic Translation Layer (`semantic/`)

```
semantic/
├── idl_generator.rs    # Generate IDL from AST
├── type_inferencer.rs  # Cross-language type mapping
├── behavior_extractor.rs # Semantic function signatures
└── templates/
    ├── proto.hbs       # Protobuf IDL template
    ├── thrift.hbs      # Thrift IDL template
    └── openapi.hbs     # OpenAPI spec template
```

**Semantic IDL Generation:**

**Goal**: Extract language-agnostic function signatures for cross-language implementation.

**Example Flow:**
```rust
// Input: Rust function
fn calculate_discount(price: f64, user_tier: UserTier) -> f64 {
    match user_tier {
        UserTier::Gold => price * 0.8,
        UserTier::Silver => price * 0.9,
        UserTier::Bronze => price * 0.95,
    }
}

// Output: IDL (Protobuf-style)
message CalculateDiscountRequest {
    double price = 1;
    UserTier user_tier = 2;
}

message CalculateDiscountResponse {
    double discounted_price = 1;
}

service DiscountService {
    rpc CalculateDiscount(CalculateDiscountRequest) returns (CalculateDiscountResponse);
}
```

**Semantic Metadata:**
```json
{
  "function": "calculate_discount",
  "semantic_purpose": "Applies tiered discount based on user membership level",
  "input_constraints": ["price >= 0"],
  "output_constraints": ["result <= price"],
  "side_effects": "none",
  "complexity": "LOW",
  "idempotent": true
}
```

**Supported IDL Formats:**
- Protocol Buffers (proto3)
- Apache Thrift
- OpenAPI/Swagger
- GraphQL SDL
- Custom JSON schema

### 2.5 Rule Engine (`rules/`)

```
rules/
├── engine.rs           # Rule evaluation engine
├── matchers.rs         # Pattern matching logic
├── labeler.rs          # Apply labels to nodes/edges
└── ruleset_schema.json # JSON schema for rulesets
```

**Rule-Based Labeling:**

**Ruleset Format (JSON):**
```json
{
  "version": "1.0",
  "rules": [
    {
      "name": "critical_security_function",
      "description": "Mark authentication/authorization functions",
      "match": {
        "node_type": "Function",
        "name_pattern": "(?i)(auth|login|verify|token|session)",
        "or": [
          {"calls_any": ["bcrypt", "jwt", "oauth"]},
          {"has_annotation": "SecurityCritical"}
        ]
      },
      "actions": [
        {"add_label": "security:critical"},
        {"set_metadata": {"audit_required": true}},
        {"set_complexity_override": "HIGH"}
      ]
    },
    {
      "name": "deprecated_api",
      "match": {
        "has_annotation": "deprecated",
        "or": {"name_pattern": ".*_v1$"}
      },
      "actions": [
        {"add_label": "deprecated"},
        {"add_label": "migration_needed"}
      ]
    },
    {
      "name": "test_coverage_low",
      "match": {
        "node_type": "Function",
        "not": {"has_test": true},
        "complexity": {"gt": 10}
      },
      "actions": [
        {"add_label": "needs_tests"},
        {"set_metadata": {"priority": "high"}}
      ]
    }
  ]
}
```

**Rule Capabilities:**
- Pattern matching (regex, glob, AST patterns)
- Metadata conditions (complexity, community, centrality)
- Graph structure conditions (calls, imports, inheritance)
- Composite logic (AND, OR, NOT)
- Actions: labels, metadata, overrides

---

## 3. Technology Stack

### 3.1 Core Dependencies

```toml
[dependencies]
# Parsing & AST - Programming Languages
tree-sitter = "0.20"
tree-sitter-rust = "0.20"
tree-sitter-python = "0.20"
tree-sitter-typescript = "0.20"
tree-sitter-javascript = "0.20"
tree-sitter-go = "0.20"
tree-sitter-java = "0.20"
tree-sitter-kotlin = "0.20"
tree-sitter-c-sharp = "0.20"
tree-sitter-cpp = "0.20"
tree-sitter-c = "0.20"
tree-sitter-swift = "0.20"
tree-sitter-php = "0.20"
tree-sitter-ruby = "0.20"
tree-sitter-scala = "0.20"
tree-sitter-elixir = "0.20"

# Parsing - Data & Schema Languages
tree-sitter-json = "0.20"
tree-sitter-yaml = "0.20"
tree-sitter-toml = "0.20"
tree-sitter-xml = "0.20"
tree-sitter-proto = "0.20"      # Protocol Buffers
tree-sitter-sql = "0.20"
tree-sitter-graphql = "0.20"
tree-sitter-hcl = "0.20"        # Terraform/HCL

# Configuration Format Parsers (non-Tree-sitter)
serde_yaml = "0.9"
serde_json = "1"
toml = "0.8"
quick-xml = "0.31"
ini = "1.3"
java-properties = "2.0"

# Markdown & Documentation
pulldown-cmark = "0.9"          # Markdown parser

# Plugin System
libloading = "0.8"              # Dynamic library loading
abi_stable = "0.11"             # Stable ABI for plugins

# Graph Backend
indradb = "4.0"                 # Primary graph database
rocksdb = "0.21"                # Storage backend for IndraDB

# Graph Algorithms
petgraph = "0.6"                # Graph data structures
pathfinding = "4.0"             # Path algorithms

# Parallelization
rayon = "1.7"                   # Data parallelism
tokio = { version = "1", features = ["full"] }
crossbeam = "0.8"               # Concurrent data structures

# Serialization
serde = { version = "1", features = ["derive"] }
bincode = "1.3"                 # Binary serialization

# CLI & Output
clap = { version = "4", features = ["derive"] }
comfy-table = "7"               # Terminal tables
indicatif = "0.17"              # Progress bars
console = "0.15"                # Terminal styling

# Templating (for IDL generation)
handlebars = "4"
tera = "1"                      # Alternative template engine

# Regex & String Processing
regex = "1"
aho-corasick = "1"              # Fast multi-pattern matching
fancy-regex = "0.11"            # Advanced regex features

# Hashing & Fingerprinting
blake3 = "1"                    # Fast file hashing for incremental updates
xxhash-rust = "0.8"

# Error Handling
anyhow = "1"
thiserror = "1"

# MCP (Model Context Protocol) Integration
mcp-sdk = "0.1"                     # MCP server implementation
axum = "0.7"                        # HTTP server for MCP
tower-http = "0.5"                  # HTTP middleware

# NLP - Pattern Matching & Local Processing (no LLM required)
regex = "1"
aho-corasick = "1"                  # Fast multi-pattern matching
fancy-regex = "0.11"                # Advanced regex features
unicode-normalization = "0.1"       # Text normalization
strsim = "0.11"                     # String similarity (Levenshtein, etc.)

# NLP - Embeddings & Similarity (for query cache)
rust-bert = { version = "0.21", optional = true }  # Sentence embeddings (local)
ndarray = "0.15"                    # Array operations for embeddings

# LLM Integration (optional fallback for complex queries)
reqwest = { version = "0.11", features = ["json"] }
anthropic-sdk = { version = "0.1", optional = true }
async-openai = { version = "0.14", optional = true }
tokenizers = { version = "0.15", optional = true }  # For token counting

# Optional: Neo4j Backend
neo4rs = { version = "0.7", optional = true }

# Optional: Additional Export Formats
graphml = { version = "0.1", optional = true }

[dev-dependencies]
criterion = "0.5"               # Benchmarking
proptest = "1"                  # Property testing
tempfile = "3"                  # Temporary directories for tests
pretty_assertions = "1"         # Better assertion output
insta = "1"                     # Snapshot testing

[features]
default = ["all-languages", "nlp-patterns", "mcp-server"]
all-languages = []

# NLP Query Features (progressive enhancement)
nlp-patterns = []                           # Pattern-based NLP (no LLM, 60% queries)
nlp-cache = ["nlp-patterns", "rust-bert"]   # + Learned patterns with embeddings (90% queries)
nlp-llm-claude = ["nlp-cache", "anthropic-sdk"]  # + Claude fallback (98% queries)
nlp-llm-openai = ["nlp-cache", "async-openai"]   # + OpenAI fallback (98% queries)
nlp-llm-local = ["nlp-cache", "tokenizers"]      # + Local LLM fallback (95% queries)

# Integration Features
mcp-server = ["mcp-sdk", "axum", "tower-http"]  # MCP server for AI agents
neo4j = ["neo4rs"]
export-graphml = ["graphml"]
```

### 3.2 Language Plugin System

**Design Philosophy**: Languages are first-class plugins. Adding a new language should require minimal code changes to the core system.

**Architecture:**
```
languages/
├── registry.rs           # Language registry and loader
├── plugin_trait.rs       # LanguagePlugin trait definition
├── builtin/             # Built-in language plugins
│   ├── rust.rs
│   ├── python.rs
│   ├── typescript.rs
│   └── ...
├── config/              # Configuration format plugins
│   ├── yaml.rs
│   ├── json.rs
│   ├── toml.rs
│   ├── properties.rs
│   └── xml.rs
└── external/            # User-provided plugins (dynamic loading)
    └── .so/.dylib files
```

**Language Plugin Trait:**
```rust
pub trait LanguagePlugin: Send + Sync {
    /// Language identifier (e.g., "rust", "python", "yaml")
    fn language_id(&self) -> &str;
    
    /// File extensions this plugin handles
    fn file_extensions(&self) -> Vec<&str>;
    
    /// Tree-sitter grammar (if applicable)
    fn grammar(&self) -> Option<tree_sitter::Language>;
    
    /// Extract symbols from AST
    fn extract_symbols(&self, ast: &Tree, source: &[u8]) -> Vec<Symbol>;
    
    /// Extract relationships between symbols
    fn extract_relations(&self, ast: &Tree, source: &[u8]) -> Vec<Relation>;
    
    /// Language-specific complexity calculation
    fn calculate_complexity(&self, node: &Node) -> Option<ComplexityMetrics>;
    
    /// Extract semantic metadata (optional, for IDL generation)
    fn extract_semantics(&self, symbol: &Symbol) -> Option<SemanticMetadata> {
        None
    }
    
    /// Language capabilities
    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities::default()
    }
}

pub struct LanguageCapabilities {
    pub has_types: bool,              // Statically typed?
    pub has_classes: bool,            // OOP support?
    pub has_functions: bool,
    pub has_modules: bool,
    pub supports_idl_generation: bool,
    pub complexity_metrics: Vec<ComplexityType>,
}
```

**Configuration Format Plugin Trait:**
```rust
pub trait ConfigFormatPlugin: Send + Sync {
    fn format_id(&self) -> &str;
    fn file_extensions(&self) -> Vec<&str>;
    
    /// Parse config file and extract key-value structure
    fn parse(&self, content: &str) -> Result<ConfigGraph>;
    
    /// Extract schema information (if structured)
    fn extract_schema(&self, content: &str) -> Option<SchemaInfo>;
}

pub struct ConfigGraph {
    pub keys: Vec<ConfigKey>,
    pub references: Vec<ConfigReference>,  // e.g., ${VAR} references
    pub schema_violations: Vec<Violation>,
}
```

**Language Registry:**
```rust
pub struct LanguageRegistry {
    code_languages: HashMap<String, Box<dyn LanguagePlugin>>,
    config_formats: HashMap<String, Box<dyn ConfigFormatPlugin>>,
    extension_map: HashMap<String, String>,  // .rs -> rust, .yml -> yaml
}

impl LanguageRegistry {
    /// Register a built-in language
    pub fn register_language(&mut self, plugin: Box<dyn LanguagePlugin>) {
        let id = plugin.language_id().to_string();
        for ext in plugin.file_extensions() {
            self.extension_map.insert(ext.to_string(), id.clone());
        }
        self.code_languages.insert(id, plugin);
    }
    
    /// Load external plugin from shared library
    pub fn load_external(&mut self, path: &Path) -> Result<()> {
        // Dynamic loading via libloading crate
        unsafe {
            let lib = libloading::Library::new(path)?;
            let constructor: libloading::Symbol<fn() -> Box<dyn LanguagePlugin>> =
                lib.get(b"create_language_plugin")?;
            let plugin = constructor();
            self.register_language(plugin);
        }
        Ok(())
    }
    
    /// Get plugin for file extension
    pub fn get_for_file(&self, path: &Path) -> Option<&dyn LanguagePlugin> {
        let ext = path.extension()?.to_str()?;
        let lang_id = self.extension_map.get(ext)?;
        self.code_languages.get(lang_id).map(|p| p.as_ref())
    }
}
```

### 3.3 Language Support Matrix

**Programming Languages (Tier 1 - Full Support):**

| Language   | AST Parsing | Symbol Extraction | Semantic IDL | Complexity | Status |
|------------|-------------|-------------------|--------------|------------|--------|
| Rust       | ✅ Tree-sitter | ✅ Full        | ✅ Full      | ✅ Full    | Phase 1 |
| Python     | ✅ Tree-sitter | ✅ Full        | ✅ Full      | ✅ Full    | Phase 1 |
| TypeScript | ✅ Tree-sitter | ✅ Full        | ✅ Full      | ✅ Full    | Phase 1 |
| JavaScript | ✅ Tree-sitter | ✅ Full        | ⚠️ Partial   | ✅ Full    | Phase 1 |
| Go         | ✅ Tree-sitter | ✅ Full        | ✅ Full      | ✅ Full    | Phase 1 |
| Java       | ✅ Tree-sitter | ✅ Full        | ✅ Full      | ✅ Full    | Phase 2 |
| Kotlin     | ✅ Tree-sitter | ✅ Full        | ✅ Full      | ✅ Full    | Phase 2 |
| C#         | ✅ Tree-sitter | ✅ Full        | ✅ Full      | ✅ Full    | Phase 2 |

**Programming Languages (Tier 2 - Partial Support):**

| Language   | AST Parsing | Symbol Extraction | Semantic IDL | Complexity | Status |
|------------|-------------|-------------------|--------------|------------|--------|
| C++        | ✅ Tree-sitter | ⚠️ Partial     | ⚠️ Partial   | ✅ Full    | Phase 2 |
| C          | ✅ Tree-sitter | ✅ Full        | ⚠️ Partial   | ✅ Full    | Phase 2 |
| Swift      | ✅ Tree-sitter | ✅ Full        | ⚠️ Partial   | ✅ Full    | Phase 3 |
| PHP        | ✅ Tree-sitter | ✅ Full        | ⚠️ Limited   | ✅ Full    | Phase 3 |
| Ruby       | ✅ Tree-sitter | ✅ Full        | ⚠️ Limited   | ✅ Full    | Phase 3 |
| Scala      | ✅ Tree-sitter | ✅ Full        | ✅ Full      | ✅ Full    | Phase 3 |
| Elixir     | ✅ Tree-sitter | ✅ Full        | ⚠️ Limited   | ✅ Full    | Phase 3 |
| Haskell    | ✅ Tree-sitter | ⚠️ Partial     | ⚠️ Limited   | ⚠️ Partial | Phase 4 |

**Configuration & Data Formats:**

| Format     | Parser      | Schema Extraction | Validation | References | Status |
|------------|-------------|-------------------|------------|------------|--------|
| YAML       | ✅ serde_yaml | ✅ Full         | ✅ Full    | ✅ `${}`   | Phase 1 |
| JSON       | ✅ serde_json | ✅ Full         | ✅ Full    | ✅ `$ref`  | Phase 1 |
| TOML       | ✅ toml      | ✅ Full         | ✅ Full    | ⚠️ Limited | Phase 1 |
| Properties | ✅ Custom    | ✅ Full         | ⚠️ Limited | ✅ `${}`   | Phase 1 |
| XML        | ✅ quick-xml | ✅ Full         | ✅ XSD     | ✅ XPath   | Phase 2 |
| INI        | ✅ ini       | ✅ Full         | ⚠️ Limited | ❌         | Phase 2 |
| ENV        | ✅ Custom    | ✅ Full         | ⚠️ Limited | ❌         | Phase 2 |
| Protobuf   | ✅ Tree-sitter | ✅ Full       | ✅ Full    | ✅ import  | Phase 2 |
| GraphQL    | ✅ Tree-sitter | ✅ Full       | ✅ Full    | ✅ Full    | Phase 3 |
| SQL        | ✅ Tree-sitter | ✅ Full       | ⚠️ Partial | ✅ Foreign Keys | Phase 3 |
| HCL/Terraform | ✅ Tree-sitter | ✅ Full    | ✅ Full    | ✅ Variables | Phase 3 |

**Markup & Documentation:**

| Format     | Parser      | Symbol Extraction | Cross-refs | Status |
|------------|-------------|-------------------|------------|--------|
| Markdown   | ✅ pulldown-cmark | ✅ Headings, code blocks | ✅ Links | Phase 1 |
| AsciiDoc   | ✅ Custom    | ✅ Sections      | ✅ Includes | Phase 3 |
| ReStructuredText | ✅ Custom | ✅ Directives  | ✅ Refs    | Phase 3 |
| HTML       | ✅ scraper   | ⚠️ Limited       | ✅ Links   | Phase 4 |

### 3.4 How to Add a New Language

Adding a new language to rBuilder requires implementing the `LanguagePlugin` trait. Here's a step-by-step guide:

**Step 1: Create the Plugin File**
```bash
# For built-in languages
touch src/languages/builtin/elixir.rs

# For external plugins (separate crate)
cargo new --lib rbuilder-plugin-elixir
```

**Step 2: Implement the LanguagePlugin Trait**

```rust
// src/languages/builtin/elixir.rs
use crate::languages::plugin_trait::*;
use tree_sitter::{Language, Node, Tree};

pub struct ElixirPlugin;

impl LanguagePlugin for ElixirPlugin {
    fn language_id(&self) -> &str {
        "elixir"
    }
    
    fn file_extensions(&self) -> Vec<&str> {
        vec!["ex", "exs"]
    }
    
    fn grammar(&self) -> Option<Language> {
        // Tree-sitter grammar for Elixir
        Some(tree_sitter_elixir::language())
    }
    
    fn extract_symbols(&self, ast: &Tree, source: &[u8]) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = ast.root_node();
        
        // Walk the AST and extract Elixir-specific symbols
        let mut cursor = root.walk();
        self.visit_node(&root, source, &mut symbols, &mut cursor);
        
        symbols
    }
    
    fn extract_relations(&self, ast: &Tree, source: &[u8]) -> Vec<Relation> {
        let mut relations = Vec::new();
        
        // Extract function calls, imports (use, require, alias)
        // ...
        
        relations
    }
    
    fn calculate_complexity(&self, node: &Node) -> Option<ComplexityMetrics> {
        // Elixir-specific complexity calculation
        // Count case, cond, with statements, pipeline depth, etc.
        Some(ComplexityMetrics {
            cyclomatic: self.calculate_cyclomatic(node),
            cognitive: self.calculate_cognitive(node),
            halstead: None,
        })
    }
    
    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities {
            has_types: true,  // Elixir has typespecs
            has_classes: false,
            has_functions: true,
            has_modules: true,
            supports_idl_generation: true,
            complexity_metrics: vec![
                ComplexityType::Cyclomatic,
                ComplexityType::Cognitive,
            ],
        }
    }
}

impl ElixirPlugin {
    fn visit_node(&self, node: &Node, source: &[u8], symbols: &mut Vec<Symbol>, cursor: &mut TreeCursor) {
        match node.kind() {
            "call" => {
                // Check if it's a def, defp, defmacro, etc.
                if let Some(function) = self.extract_function_def(node, source) {
                    symbols.push(function);
                }
            }
            "module" | "defmodule" => {
                if let Some(module) = self.extract_module(node, source) {
                    symbols.push(module);
                }
            }
            _ => {}
        }
        
        // Recursively visit children
        if cursor.goto_first_child() {
            loop {
                self.visit_node(&cursor.node(), source, symbols, cursor);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }
    
    fn extract_function_def(&self, node: &Node, source: &[u8]) -> Option<Symbol> {
        // Parse def/defp and extract name, params, return type (if typespec exists)
        // ...
        None
    }
}
```

**Step 3: Register the Plugin**

```rust
// src/languages/registry.rs
pub fn create_default_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    
    // Register built-in languages
    registry.register_language(Box::new(RustPlugin));
    registry.register_language(Box::new(PythonPlugin));
    registry.register_language(Box::new(ElixirPlugin));  // Add new language here
    // ...
    
    registry
}
```

**Step 4: Add Tree-sitter Grammar Dependency**

```toml
# Cargo.toml
[dependencies]
tree-sitter-elixir = "0.1"  # Add the grammar crate
```

**Step 5: Add Tests**

```rust
// src/languages/builtin/elixir.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_function() {
        let source = r#"
        defmodule MyModule do
          def hello(name) do
            "Hello, #{name}"
          end
        end
        "#;
        
        let plugin = ElixirPlugin;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(plugin.grammar().unwrap()).unwrap();
        
        let tree = parser.parse(source, None).unwrap();
        let symbols = plugin.extract_symbols(&tree, source.as_bytes());
        
        assert_eq!(symbols.len(), 2);  // Module + function
        assert_eq!(symbols[1].name, "hello");
    }
}
```

**Creating External Plugins (Dynamic Loading):**

For users who want to add custom languages without modifying rBuilder:

```rust
// external-plugin/src/lib.rs
use rbuilder_plugin_api::{LanguagePlugin, Symbol, Relation};

pub struct MyCustomLanguagePlugin;

impl LanguagePlugin for MyCustomLanguagePlugin {
    // ... implement trait
}

// Export the plugin constructor
#[no_mangle]
pub extern "C" fn create_language_plugin() -> Box<dyn LanguagePlugin> {
    Box::new(MyCustomLanguagePlugin)
}
```

**Loading External Plugins:**

```bash
# Compile the plugin
cd external-plugin
cargo build --release

# Load it in rBuilder
rbuilder plugin install ./target/release/libmy_custom_language.so

# Or configure in .rbuilder/config.toml
[plugins]
custom_languages = [
    "~/.rbuilder/plugins/libmy_custom_language.so"
]
```

**Configuration Format Plugins:**

Similar process for adding config formats:

```rust
// src/languages/config/dotenv.rs
pub struct DotEnvPlugin;

impl ConfigFormatPlugin for DotEnvPlugin {
    fn format_id(&self) -> &str { "dotenv" }
    
    fn file_extensions(&self) -> Vec<&str> {
        vec!["env", "env.local", "env.development"]
    }
    
    fn parse(&self, content: &str) -> Result<ConfigGraph> {
        let mut keys = Vec::new();
        let mut references = Vec::new();
        
        for line in content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                keys.push(ConfigKey {
                    name: key.trim().to_string(),
                    value: value.trim().to_string(),
                    line_number: /* ... */,
                });
                
                // Detect ${VAR} references
                if value.contains("${") {
                    // Extract and record reference
                }
            }
        }
        
        Ok(ConfigGraph { keys, references, schema_violations: vec![] })
    }
}
```

### 3.5 Configuration Files as First-Class Graph Entities

Configuration files are treated as first-class citizens in the knowledge graph, with relationships to code that consumes them.

**Configuration Graph Integration:**

```rust
// Configuration files create their own node types
enum ConfigNodeType {
    ConfigFile { 
        path: String, 
        format: String,  // "yaml", "toml", "properties"
    },
    ConfigKey { 
        key: String, 
        value: String, 
        file: String,
        line: u32,
    },
    ConfigSection {
        section: String,
        file: String,
    },
    Schema {
        schema_type: String,  // "json-schema", "xsd", "protobuf"
        definition: String,
    },
}

// Configuration edges
enum ConfigEdgeType {
    Contains { file: Uuid, key: Uuid },
    References { from: Uuid, to: Uuid, reference_syntax: String },  // ${VAR}
    UsedBy { config_key: Uuid, code_symbol: Uuid },  // Code reads config
    Validates { schema: Uuid, config: Uuid },
    Overrides { local: Uuid, base: Uuid },  // e.g., .env.local overrides .env
}
```

**Example: YAML Configuration Graph**

```yaml
# config/database.yaml
database:
  host: ${DB_HOST}
  port: 5432
  pool_size: 20
  
logging:
  level: ${LOG_LEVEL}
  output: stdout
```

**Resulting Graph Nodes:**
1. `ConfigFile("config/database.yaml")`
2. `ConfigKey("database.host", "${DB_HOST}")`
3. `ConfigKey("database.port", "5432")`
4. `ConfigKey("database.pool_size", "20")`
5. `ConfigKey("logging.level", "${LOG_LEVEL}")`
6. `ConfigKey("logging.output", "stdout")`

**Edges:**
1. `ConfigFile` --[Contains]--> `ConfigKey("database.host")`
2. `ConfigKey("database.host")` --[References]--> `ENV("DB_HOST")` (if found in .env)
3. `ConfigKey("database.pool_size")` --[UsedBy]--> `Function("create_connection_pool")`

**Code-to-Config Linking:**

```rust
// src/database.rs
fn create_connection_pool() -> Pool {
    let config = load_yaml("config/database.yaml");
    let pool_size = config.get("database.pool_size").unwrap();  // <-- Link detected
    // ...
}
```

**Detection Strategy:**
1. **String literal matching**: `load_yaml("config/database.yaml")` → links to ConfigFile
2. **Key access patterns**: `.get("database.pool_size")` → links to ConfigKey
3. **Environment variable reads**: `env::var("DB_HOST")` → links to ENV node

**Rule-Based Config Labeling:**

```json
{
  "rules": [
    {
      "name": "sensitive_config",
      "match": {
        "node_type": "ConfigKey",
        "key_pattern": "(?i)(password|secret|token|api_key|private_key)"
      },
      "actions": [
        {"add_label": "sensitive"},
        {"add_label": "security:audit_required"}
      ]
    },
    {
      "name": "hardcoded_url",
      "match": {
        "node_type": "ConfigKey",
        "value_pattern": "^https?://.*",
        "not": {"key_pattern": ".*_url$"}
      },
      "actions": [
        {"add_label": "antipattern:hardcoded_url"},
        {"set_metadata": {"suggestion": "Use environment variable"}}
      ]
    },
    {
      "name": "missing_env_var",
      "match": {
        "node_type": "ConfigKey",
        "value_pattern": "^\\$\\{[A-Z_]+\\}$",
        "not_exists": {"referenced_env": true}
      },
      "actions": [
        {"add_label": "error:missing_env_var"},
        {"set_metadata": {"severity": "high"}}
      ]
    }
  ]
}
```

**Configuration Analysis Queries:**

```bash
# Find all hardcoded secrets
rbuilder query "MATCH (c:ConfigKey) WHERE 'sensitive' IN c.labels AND c.value != '' RETURN c"

# Find unused configuration keys
rbuilder query "MATCH (c:ConfigKey) WHERE NOT (c)-[:UsedBy]->() RETURN c"

# Find missing environment variables
rbuilder query "MATCH (c:ConfigKey)-[:References]->(e:ENV) WHERE e.defined = false RETURN c, e"

# Configuration dependency tree
rbuilder query "MATCH path = (code:Function)-[:UsesConfig*]->(config:ConfigKey)-[:References*]->(env:ENV) RETURN path"

# Configuration override chains
rbuilder query "MATCH path = (local:ConfigFile)-[:Overrides*]->(base:ConfigFile) RETURN path"
```

**Cross-Language Config Understanding:**

Different languages have different config conventions. rBuilder normalizes them:

| Language | Config Pattern | Detection |
|----------|----------------|-----------|
| Python   | `os.environ['KEY']`, `config['section.key']` | AST pattern match |
| Rust     | `env::var("KEY")`, `config.get("key")` | Tree-sitter query |
| JavaScript | `process.env.KEY`, `config.get('key')` | Tree-sitter query |
| Go       | `os.Getenv("KEY")`, `viper.Get("key")` | Tree-sitter query |
| Java     | `System.getenv("KEY")`, `props.getProperty()` | Tree-sitter query |

**Schema Validation Integration:**

For structured configs with schemas (JSON Schema, XSD, Protobuf):

```rust
// Validate YAML against JSON Schema
pub fn validate_config(config_node: &Node, schema_node: &Node) -> Vec<Violation> {
    let schema = load_schema(schema_node);
    let config = load_config(config_node);
    
    schema.validate(&config)
        .iter()
        .map(|err| Violation {
            config_key: err.path,
            expected: err.schema_constraint,
            actual: err.value,
            severity: ViolationSeverity::Error,
        })
        .collect()
}
```

**Benefits:**
- **Dead config detection**: Find unused keys
- **Missing dependency detection**: Find missing env vars before runtime
- **Security audit**: Automatically flag sensitive data
- **Configuration drift**: Compare configs across environments
- **Impact analysis**: "What breaks if I remove this config key?"

### 3.6 Language Support Roadmap

**Phase 1 (Weeks 1-4): Foundation**
- Rust, Python, TypeScript, JavaScript, Go
- YAML, JSON, TOML, Properties, Markdown
- Plugin system architecture

**Phase 2 (Weeks 5-8): Expansion**
- Java, Kotlin, C#, C++, C
- XML, INI, ENV, Protobuf
- External plugin loading

**Phase 3 (Weeks 9-12): Specialized**
- Swift, PHP, Ruby, Scala, Elixir
- GraphQL, SQL, HCL/Terraform
- AsciiDoc, ReStructuredText

**Phase 4 (Weeks 13+): Community-Driven**
- Haskell, OCaml, F#, Dart, Zig
- Custom domain-specific languages
- Language-specific optimizations

**Prioritization Criteria:**
1. Ecosystem size (GitHub usage statistics)
2. Tree-sitter grammar maturity
3. Community requests
4. Enterprise adoption

---

## 4. Data Flow Architecture

```
┌─────────────────┐
│   Repository    │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  File Discovery & Filtering             │
│  (.gitignore, size limits, binary skip) │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  Tree-sitter AST Parsing (Parallel)     │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐   │
│  │ file.rs │ │ file.py │ │ file.ts │   │
│  └────┬────┘ └────┬────┘ └────┬────┘   │
└───────┼───────────┼───────────┼─────────┘
        │           │           │
        └───────────┴───────────┘
                    ▼
┌─────────────────────────────────────────┐
│  Symbol & Relation Extraction           │
│  - Functions, Classes, Variables, Types │
│  - Calls, Imports, Inheritance, Refs    │
│  - Confidence tagging                   │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  Graph Construction (IndraDB)           │
│  - Create nodes (symbols)               │
│  - Create edges (relationships)         │
│  - Batch insert for performance         │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  Analysis Pipeline                      │
│  ┌───────────────────────────────────┐  │
│  │ 1. Community Detection (Leiden)   │  │
│  │ 2. Complexity Calculation         │  │
│  │ 3. Centrality Metrics (PageRank)  │  │
│  │ 4. Pattern Detection              │  │
│  │ 5. Dependency Analysis            │  │
│  └───────────────────────────────────┘  │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  Rule Engine Application                │
│  - Load ruleset.json                    │
│  - Match nodes/edges against rules      │
│  - Apply labels and metadata            │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  Semantic Translation (Optional)        │
│  - Extract function signatures          │
│  - Infer semantic meaning               │
│  - Generate IDL (proto/thrift/openapi)  │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  Output Generation                      │
│  ├─ graph.db (IndraDB binary)          │
│  ├─ graph.json (Portable JSON)         │
│  ├─ graph.graphml (Visualization)      │
│  ├─ GRAPH_REPORT.md (Human-readable)   │
│  └─ idl/ (Generated IDLs by module)    │
└─────────────────────────────────────────┘
```

---

## 5. Query Interface

### 5.1 Natural Language Query (NLP)

**Vision**: Query your codebase like talking to a colleague, **without heavy LLM dependency**.

**Hybrid Architecture** (90% queries answered without LLM calls):
```
User NLP Query
      ↓
┌─────────────────────────────────────┐
│   Hybrid NLP Engine                 │
│                                     │
│  1. Pattern Matching (< 1ms)       │ ← 60% of queries
│     ↓ (if no match)                 │
│  2. Query Cache (< 5ms)            │ ← 30% of queries
│     ↓ (if no match)                 │
│  3. Local T5 Model (< 50ms)        │ ← 8% of queries (optional)
│     ↓ (if no match or unavailable) │
│  4. Cloud LLM (500-2000ms)         │ ← 2% of queries (fallback)
└─────────────────────────────────────┘
      ↓
Graph Query (Cypher-like DSL)
      ↓
Query Engine (IndraDB)
      ↓
Results Formatter
      ↓
Human-readable answer + visualizations
```

**Key Innovation**: Most queries use **pattern matching and learned patterns** (no LLM), with LLM only for complex novel queries.

See [NLP_WITHOUT_LLM.md](./NLP_WITHOUT_LLM.md) for detailed design.

**Implementation:**
```rust
pub struct NLPQueryEngine {
    llm_client: LLMClient,  // Claude, OpenAI, Ollama, etc.
    graph: Arc<GraphBackend>,
    context: GraphContext,  // Schema, labels, communities
}

impl NLPQueryEngine {
    pub async fn query(&self, natural_language: &str) -> Result<QueryResult> {
        // 1. Build context for LLM
        let schema_context = self.build_schema_context();
        
        // 2. Send to LLM for translation
        let prompt = format!(
            r#"You are a graph query translator. Convert natural language to Cypher.
            
Graph Schema:
{schema_context}

User Question: {natural_language}

Translate to Cypher query:"#
        );
        
        let cypher_query = self.llm_client.complete(&prompt).await?;
        
        // 3. Execute query
        let results = self.graph.execute_cypher(&cypher_query)?;
        
        // 4. Format results with LLM
        let answer = self.format_results_nlp(&results, natural_language).await?;
        
        Ok(QueryResult {
            query: cypher_query,
            raw_results: results,
            natural_answer: answer,
        })
    }
    
    fn build_schema_context(&self) -> String {
        format!(
            r#"
Node Types: Function, Class, Module, File, ConfigKey, ENV
Edge Types: Calls, Imports, Inherits, UsedBy, References
Labels: security:critical, deprecated, high-complexity, react:component
Communities: auth (functions: 45), api (functions: 120), ui (components: 80)
            "#
        )
    }
}
```

**Example Queries:**

**Q: "Hey give me all the SOA services you have?"**
```
LLM Translation:
MATCH (m:Module)
WHERE m.labels CONTAINS 'soa:service' 
   OR m.name CONTAINS 'Service'
   OR (m)-[:Exposes]->(e:Endpoint)
RETURN m.name, m.path, COUNT((m)-[:Contains]->()) as methods

Natural Answer:
"I found 12 SOA services in your codebase:
1. AuthenticationService (src/services/auth.rs) - 8 methods
2. UserManagementService (src/services/user.py) - 15 methods
3. PaymentService (src/services/payment.go) - 22 methods
..."
```

**Q: "How many React components am I using?"**
```
LLM Translation:
MATCH (c:Function)
WHERE c.labels CONTAINS 'react:component'
   OR c.name =~ '.*Component$'
   OR (c)-[:Returns]->(t:Type {name: 'JSX.Element'})
RETURN COUNT(c) as total,
       c.file_path,
       c.community

Natural Answer:
"You have 156 React components across your codebase:
- 82 in the 'ui' community (src/components/)
- 45 in the 'dashboard' community (src/pages/)
- 29 in the 'shared' community (src/common/)

Top 5 most connected components:
1. AppLayout - called by 34 other components
2. DataTable - called by 28 other components
..."
```

**Q: "What if I wanted to change this method, what other things will be affected?"**
```
User provides: src/auth/verify_token.rs:45 (the method location)

LLM Translation:
MATCH (target:Function {file: 'src/auth/verify_token.rs', line: 45})
MATCH path = (caller)-[:Calls*1..3]->(target)
RETURN DISTINCT caller.name, 
       caller.file_path,
       LENGTH(path) as depth,
       caller.complexity,
       caller.labels
ORDER BY depth ASC, caller.complexity DESC

Natural Answer:
"Changing `verify_token()` would affect 23 functions:

🔴 CRITICAL IMPACT (directly calls it, high complexity):
  1. authenticate_user() in src/auth/middleware.rs:120 
     - Complexity: HIGH (18)
     - Labels: security:critical
     - Called by 15 endpoints
  
  2. refresh_session() in src/auth/session.rs:67
     - Complexity: MEDIUM (12)
     - Called by 8 other functions

⚠️ MODERATE IMPACT (calls it indirectly):
  3. api_gateway_handler() in src/api/gateway.rs:200
     - 2 hops away
     - Complexity: HIGH (22)
  
  [20 more...]

📊 Community Impact:
  - 'auth' community: 15 functions (75% of community)
  - 'api' community: 5 functions
  - 'admin' community: 3 functions

💡 Suggestion: This is a high-impact change. Consider:
  1. Writing integration tests for all affected endpoints
  2. Adding feature flag for gradual rollout
  3. Updating the 3 CRITICAL security functions first
"
```

**Advanced Queries:**

**Q: "Find all database queries that aren't using connection pooling"**
```
LLM Translation:
MATCH (f:Function)-[:Calls]->(db:Function)
WHERE db.name IN ['query', 'execute', 'raw_query']
  AND NOT EXISTS((f)-[:Uses]->(pool:Variable {type: 'ConnectionPool'}))
RETURN f.name, f.file_path, f.line

Natural Answer:
"Found 7 functions making direct database calls without pooling:
1. get_user_by_id() in src/user/repository.rs:45
2. save_transaction() in src/payment/db.rs:120
..."
```

**Q: "Which configuration keys are never used in production code?"**
```
LLM Translation:
MATCH (c:ConfigKey)
WHERE NOT EXISTS((c)-[:UsedBy]->(:Function))
   OR ALL(f IN [(c)-[:UsedBy]->(func) | func] WHERE 'test' IN f.labels)
RETURN c.key, c.file, c.value

Natural Answer:
"Found 14 unused config keys:
1. 'legacy.feature_flag' in config/app.yaml - likely deprecated
2. 'debug.verbose_logging' in .env.example - only in example file
..."
```

### 5.2 CLI Interface Design

```bash
# Natural Language Query (primary interface)
rbuilder ask "How many React components am I using?"
rbuilder ask "What would break if I delete this function?"
rbuilder ask "Find all security-critical code with high complexity"
rbuilder ask "Show me the most connected modules"

# Alternative: Conversational mode
rbuilder chat
> How many services do I have?
> Which ones are in the auth module?
> What's the complexity of the authentication service?
> Show me its dependencies
> exit

# Traditional query interface (for advanced users)
rbuilder query <QUERY> [OPTIONS]
  --format <table|json|graph>
  --explain                 # Show the translation from NLP

# Initialize graph for repository
rbuilder init [PATH] [OPTIONS]
  --languages <LANGS>       # Filter to specific languages
  --exclude <PATTERNS>      # Additional exclusion patterns
  --backend <indradb|neo4j> # Graph backend choice
  --config <PATH>           # Custom config file

# Incremental update (like git diff)
rbuilder update [OPTIONS]
  --since <COMMIT>          # Only process changed files
  --force                   # Full rebuild

# Run analysis
rbuilder analyze [OPTIONS]
  --community               # Run community detection
  --complexity              # Calculate complexity metrics
  --centrality              # Compute centrality scores
  --all                     # Run all analyses

# Apply rules
rbuilder label --ruleset <PATH>
  --dry-run                 # Show what would be labeled
  --verbose                 # Show matching details

# Generate semantic IDL
rbuilder idl [OPTIONS]
  --format <proto|thrift|openapi>
  --module <MODULE>         # Generate for specific module
  --output-dir <PATH>

# Export graph
rbuilder export --format <graphml|json|cypher> --output <PATH>

# Interactive visualization
rbuilder serve [OPTIONS]
  --port <PORT>
  --open                    # Auto-open browser

# Impact analysis
rbuilder impact <SYMBOL> [OPTIONS]
  --depth <N>               # Traversal depth
  --direction <in|out|both>

# Find path between symbols
rbuilder path <FROM> <TO> [OPTIONS]
  --max-depth <N>
  --avoid-labels <LABELS>   # Don't traverse certain labels

# Statistics and reporting
rbuilder stats [OPTIONS]
  --community-report        # Community structure analysis
  --complexity-report       # Complexity distribution
  --hotspots                # High-complexity, high-centrality nodes

# Configuration analysis
rbuilder config [OPTIONS]
  --unused                  # Find unused config keys
  --missing-env             # Find missing environment variables
  --validate                # Validate against schemas
  --secrets                 # Find potential secrets in config
  --drift <ENV1> <ENV2>     # Compare configs between environments

# Plugin management
rbuilder plugin [COMMAND]
  install <PATH>            # Install external language plugin
  list                      # List all loaded plugins
  info <PLUGIN_ID>          # Show plugin capabilities
  uninstall <PLUGIN_ID>     # Remove plugin
```

### 5.3 Domain-Specific Pattern Recognition

**Problem**: The LLM needs to understand project-specific terminology (e.g., "SOA service", "React component", "gRPC endpoint").

**Solution**: Auto-generate query patterns from graph structure and labels.

```rust
pub struct DomainPatternRegistry {
    patterns: HashMap<String, QueryPattern>,
}

pub struct QueryPattern {
    keywords: Vec<String>,
    cypher_template: String,
    examples: Vec<String>,
}

impl DomainPatternRegistry {
    /// Auto-detect patterns from graph labels
    pub fn learn_from_graph(&mut self, graph: &Graph) {
        // If 30+ nodes have label "react:component"
        if graph.count_label("react:component") > 30 {
            self.patterns.insert("react_component".to_string(), QueryPattern {
                keywords: vec!["react component", "components", "react UI"],
                cypher_template: "MATCH (n) WHERE 'react:component' IN n.labels RETURN n",
                examples: vec![
                    "How many React components?",
                    "List all components",
                    "Show me the React UI",
                ],
            });
        }
        
        // If 10+ nodes have label "soa:service"
        if graph.count_label("soa:service") > 10 {
            self.patterns.insert("soa_service".to_string(), QueryPattern {
                keywords: vec!["SOA service", "services", "microservice"],
                cypher_template: "MATCH (n) WHERE 'soa:service' IN n.labels RETURN n",
                examples: vec![
                    "List all SOA services",
                    "How many services?",
                ],
            });
        }
        
        // Detect common suffixes (e.g., *Service, *Repository, *Controller)
        let naming_patterns = graph.analyze_naming_patterns();
        for pattern in naming_patterns {
            self.patterns.insert(pattern.name, pattern.query);
        }
    }
    
    /// Include patterns in LLM context
    pub fn to_llm_context(&self) -> String {
        let mut context = String::from("Common Patterns in this codebase:\n");
        for (name, pattern) in &self.patterns {
            context.push_str(&format!(
                "- {}: Keywords: {:?}, Example: '{}'\n",
                name, pattern.keywords, pattern.examples[0]
            ));
        }
        context
    }
}
```

**Enhanced LLM Prompt with Domain Knowledge:**

```rust
let prompt = format!(
    r#"You are a graph query translator for a specific codebase.

Graph Schema:
{schema_context}

Domain Patterns (auto-detected from this project):
{domain_patterns}

Label Distribution:
- react:component: 156 nodes
- soa:service: 12 nodes
- security:critical: 34 nodes
- deprecated: 8 nodes
- high-complexity: 45 nodes

Community Structure:
- 'auth' community: 67 functions (authentication, authorization)
- 'ui' community: 156 functions (React components, UI logic)
- 'api' community: 120 functions (REST endpoints, gRPC services)
- 'database' community: 45 functions (queries, repositories)

User Question: {natural_language}

Translate to Cypher query. Be specific to this codebase's patterns."#
);
```

### 5.4 Conversational Query Interface

**Interactive mode** with context retention:

```bash
$ rbuilder chat

rBuilder> How many services do I have?
Found 12 SOA services in your codebase.

rBuilder> Which ones are in the auth module?
3 services in the 'auth' community:
1. AuthenticationService
2. AuthorizationService  
3. TokenManagementService

rBuilder> What's the complexity of AuthenticationService?
AuthenticationService has:
- Cyclomatic complexity: 45 (CRITICAL)
- Cognitive complexity: 38 (CRITICAL)
- 8 methods, average complexity: 12 (HIGH)

rBuilder> Show me its most complex method
The most complex method is `authenticate_with_mfa()`:
- Cyclomatic: 22 (CRITICAL)
- Cognitive: 28 (CRITICAL)
- Location: src/auth/service.rs:245
- Called by 5 other functions
- Calls 12 external dependencies

rBuilder> What calls that method?
5 callers:
1. login_handler() - src/api/auth_controller.rs:45
2. refresh_token_handler() - src/api/auth_controller.rs:120
3. admin_impersonate() - src/admin/actions.rs:200
4. cli_login() - src/cli/auth.rs:80
5. test_mfa_flow() - tests/auth_test.rs:150

rBuilder> exit
```

**Conversation State:**
```rust
pub struct ConversationContext {
    history: Vec<QueryExchange>,
    focused_nodes: Vec<Uuid>,  // Nodes mentioned in conversation
    active_community: Option<String>,
}

pub struct QueryExchange {
    user_question: String,
    translated_query: String,
    results: Vec<Node>,
}

impl ConversationContext {
    /// Resolve pronouns and context references
    pub fn resolve_references(&self, question: &str) -> String {
        // "Show me its dependencies" -> "Show me AuthenticationService's dependencies"
        // "What calls that?" -> "What calls authenticate_with_mfa()?"
        
        if question.contains("it") || question.contains("that") {
            if let Some(last_node) = self.focused_nodes.last() {
                return question.replace("it", &last_node.name)
                              .replace("that", &last_node.name);
            }
        }
        
        question.to_string()
    }
}
```

### Example Usage:

```bash
# Build graph for a Rust project
cd ~/my-rust-project
rbuilder init . --languages rust --backend indradb

# Natural Language Queries (Primary Interface)
rbuilder ask "How many React components am I using?"
rbuilder ask "Give me all the SOA services"
rbuilder ask "What would break if I change verify_token()?"
rbuilder ask "Find all high-complexity security functions"
rbuilder ask "Which config keys are never used?"
rbuilder ask "Show me all deprecated APIs"

# Interactive conversation mode
rbuilder chat
# ... (see conversational examples above)

# Apply custom security labeling rules
rbuilder label --ruleset security-rules.json

# Traditional Cypher queries (for advanced users)
rbuilder query "MATCH (n:Function) WHERE 'security:critical' IN n.labels RETURN n"

# Generate Protocol Buffer IDL for a module
rbuilder idl --format proto --module auth --output-dir ./idl

# Impact analysis (can also use NLP: "what breaks if I change X?")
rbuilder impact "auth::verify_token" --depth 3

# Export graph for Neo4j
rbuilder export --format cypher --output graph.cypher

# Start interactive server with visual graph browser
rbuilder serve --port 8080 --open

# Configuration analysis examples
rbuilder config --unused              # Or: rbuilder ask "Which config keys are unused?"
rbuilder config --validate
rbuilder config --missing-env         # Or: rbuilder ask "What env vars are missing?"
rbuilder config --drift config/production.yaml config/development.yaml
rbuilder config --secrets             # Or: rbuilder ask "Find hardcoded secrets"

# Plugin management examples
rbuilder plugin install ~/.rbuilder/plugins/libzig_plugin.so
rbuilder plugin list
rbuilder plugin info python
```

**NLP Query Output Examples:**

```bash
$ rbuilder ask "What are my most complex functions?"

🔍 Analyzing codebase complexity...

Top 10 Most Complex Functions:

1. 🔴 process_payment_with_retry() - CRITICAL
   📍 src/payment/processor.rs:245
   📊 Cyclomatic: 45, Cognitive: 52
   🏷️  Labels: security:critical, payment:core
   🔗 Called by 8 functions
   💡 Suggestion: Consider splitting into smaller functions

2. 🔴 authenticate_with_mfa() - CRITICAL
   📍 src/auth/service.rs:120
   📊 Cyclomatic: 38, Cognitive: 42
   🏷️  Labels: security:critical, auth:core
   🔗 Called by 5 functions

3. ⚠️  render_dashboard_layout() - HIGH
   📍 src/ui/components/Dashboard.tsx:89
   📊 Cyclomatic: 28, Cognitive: 35
   🏷️  Labels: react:component, ui:critical
   🔗 Called by 12 components

[... 7 more ...]

📈 Complexity Distribution:
   CRITICAL (21+): 15 functions (3%)
   HIGH (11-20):   78 functions (15%)
   MEDIUM (6-10):  234 functions (45%)
   LOW (0-5):      192 functions (37%)

💡 Recommendations:
   - Refactor 15 CRITICAL functions
   - Add tests for high-complexity security functions
   - Consider using complexity gates in CI/CD
```

```bash
$ rbuilder ask "What would break if I delete UserRepository?"

🔍 Analyzing impact of deleting UserRepository...

⚠️  HIGH IMPACT - This change affects 47 functions across 4 communities

🔴 DIRECT DEPENDENCIES (12 functions directly use UserRepository):
   1. UserService.get_user() - src/services/user.rs:45
   2. UserService.create_user() - src/services/user.rs:89
   3. AuthService.find_by_email() - src/auth/service.rs:120
   4. AdminController.list_users() - src/api/admin.rs:200
   [... 8 more ...]

⚠️  INDIRECT DEPENDENCIES (35 functions call the direct dependents):
   - 18 API endpoints (authentication would fail)
   - 12 background jobs (user sync jobs would fail)
   - 5 scheduled tasks

📊 Community Impact:
   🔴 'auth' community: 18/67 functions affected (27%)
   🔴 'api' community: 15/120 functions affected (13%)
   ⚠️  'admin' community: 8/45 functions affected (18%)
   ✅ 'ui' community: 6/156 functions affected (4%)

🧪 Test Coverage:
   ✅ UserRepository has 89% test coverage
   ⚠️  Only 23% of dependent functions have integration tests

💡 Migration Path:
   1. Create alternative repository implementation
   2. Update 12 direct dependents first
   3. Run integration tests for all affected endpoints
   4. Update 35 indirect dependents
   5. Remove UserRepository

🚨 RECOMMENDATION: This is a high-risk change. Consider:
   - Feature flag rollout
   - Parallel implementation during transition
   - Extended integration test suite
```


---

## 6. Advanced Features

### 6.1 Natural Language Query System (AI-Powered)

**Vision**: Make the knowledge graph accessible to anyone, not just graph query experts.

**Key Capabilities:**

1. **NLP to Graph Query Translation**
   - User asks in plain English
   - LLM translates to Cypher/graph query
   - Execute query and format results
   - Show translation for transparency (`--explain` flag)

2. **Domain Pattern Learning**
   - Auto-detect project-specific patterns from labels
   - Learn naming conventions (*Service, *Repository, *Component)
   - Build domain vocabulary from graph structure
   - Include in LLM context for better translations

3. **Conversational Context**
   - Multi-turn conversations with context retention
   - Pronoun resolution ("it", "that", "those")
   - Focus tracking (current module, function, community)
   - Follow-up questions without re-specifying context

4. **Rich Output Formatting**
   - Emoji indicators for severity/status
   - ASCII visualizations for distributions
   - Actionable recommendations
   - Migration paths for breaking changes

**Use Cases:**

| Question | Translation | Use Case |
|----------|-------------|----------|
| "How many React components?" | `MATCH (n) WHERE 'react:component' IN n.labels RETURN COUNT(n)` | Inventory |
| "Find all SOA services" | `MATCH (n) WHERE 'soa:service' IN n.labels RETURN n` | Architecture review |
| "What breaks if I change X?" | `MATCH path = (caller)-[:Calls*]->(X) RETURN caller, path` | Impact analysis |
| "Which config keys are unused?" | `MATCH (c:ConfigKey) WHERE NOT (c)-[:UsedBy]->() RETURN c` | Config cleanup |
| "Show high-complexity security code" | `MATCH (f:Function) WHERE 'security:critical' IN f.labels AND f.complexity > 20 RETURN f` | Code review |
| "What's the most connected module?" | `MATCH (m:Module) RETURN m, COUNT((m)--()) AS degree ORDER BY degree DESC LIMIT 1` | Architectural hotspots |

**Implementation Priority:**
- **Phase 1**: Basic NLP → Cypher translation
- **Phase 2**: Domain pattern learning
- **Phase 3**: Conversational context
- **Phase 4**: Rich formatting and recommendations

**LLM Provider Support:**
- Claude (Anthropic) - Primary
- OpenAI GPT-4
- Local models (Ollama, llama.cpp)
- Custom endpoints

**Offline Mode:**
When LLM unavailable:
- Fall back to keyword matching
- Template-based queries
- Suggest exact Cypher syntax

### 6.2 Incremental Updates

**Challenge**: Re-parsing entire repositories on every change is slow.

**Solution**: Git-aware incremental updates
- Track file hashes in graph metadata
- On `update`: compare working tree vs. last indexed state
- Only re-parse changed files
- Prune orphaned nodes (from deleted files)
- Re-run analysis only on affected communities

**Implementation:**
```rust
struct IncrementalIndex {
    file_hashes: HashMap<PathBuf, u64>,  // Last indexed hash
    node_to_file: HashMap<Uuid, PathBuf>, // Node ownership
}

fn incremental_update(repo: &Repo, index: &mut IncrementalIndex) {
    let changed_files = git_diff_files(index.last_commit, "HEAD");
    
    for file in changed_files {
        // Remove old nodes from this file
        let old_nodes = index.node_to_file.iter()
            .filter(|(_, path)| *path == file)
            .map(|(id, _)| *id)
            .collect();
        
        graph.remove_nodes(old_nodes);
        
        // Re-parse and insert new nodes
        let new_nodes = parse_file(file);
        graph.insert_nodes(new_nodes);
    }
    
    // Re-run analysis on affected communities
    reanalyze_affected_communities();
}
```

### 6.2 Multi-Repo Support

**Use Case**: Microservices, monorepos with subprojects

**Approach:**
- Each repository is a separate graph database
- Cross-repo edges (API calls, shared libraries)
- Unified query interface across graphs

```bash
# Index multiple repos
rbuilder init ~/frontend --name frontend
rbuilder init ~/backend --name backend
rbuilder init ~/shared-lib --name shared

# Link repos (detect cross-repo dependencies)
rbuilder link frontend backend shared

# Query across all repos
rbuilder query --repos all "MATCH (f:Frontend)-[:CALLS]->(b:Backend) RETURN f, b"
```

### 6.3 Complexity Classification

**Metrics:**
- **Cyclomatic Complexity**: `CC = edges - nodes + 2 * connected_components`
- **Cognitive Complexity**: Weighted by nesting depth
- **Halstead Metrics**: Operands, operators, vocabulary size

**Classification Thresholds:**
```json
{
  "complexity_levels": {
    "LOW": {"cyclomatic": [0, 5], "cognitive": [0, 7]},
    "MEDIUM": {"cyclomatic": [6, 10], "cognitive": [8, 15]},
    "HIGH": {"cyclomatic": [11, 20], "cognitive": [16, 25]},
    "CRITICAL": {"cyclomatic": [21, null], "cognitive": [26, null]}
  },
  "actions": {
    "CRITICAL": ["flag_for_refactor", "require_review", "add_label:needs_simplification"]
  }
}
```

**Community Complexity Rollup:**
- Aggregate complexity across community members
- Identify "high-complexity communities" (technical debt hotspots)

### 6.4 Semantic Understanding

**Approach**: Hybrid Tree-sitter + Optional LLM

**Pure AST (No LLM):**
- Extract function signatures, parameters, return types
- Infer basic semantics from names (CRUD, auth, validation)
- Type flow analysis (static types only)

**LLM-Enhanced (Optional):**
- Send function AST + docstring to LLM
- Ask: "Describe this function's purpose in one sentence"
- Extract constraints, side effects, domain meaning
- Cache results in graph metadata

**Example:**
```rust
// Function AST + context → LLM → Semantic metadata
{
  "function": "process_payment",
  "signature": "fn process_payment(amount: Decimal, card: &Card) -> Result<Receipt>",
  "semantic_purpose": "Charges a credit card and returns a receipt on success",
  "input_constraints": ["amount > 0", "card.is_valid()"],
  "side_effects": ["external_api_call", "database_write"],
  "error_conditions": ["insufficient_funds", "invalid_card", "network_failure"],
  "idempotent": false,
  "domain": "payment_processing"
}
```

### 6.5 Visualization & Exploration

**Web UI (Interactive Graph):**
- Technology: D3.js + React + WebAssembly (for IndraDB queries)
- Features:
  - Zoom, pan, filter by labels/communities
  - Click node → show code snippet
  - Highlight paths between nodes
  - Community color-coding
  - Complexity heatmap overlay

**Graph Layout Algorithms:**
- Force-directed (default)
- Hierarchical (for inheritance trees)
- Circular (for cyclic dependencies)
- Community-based clustering

---

## 7. Implementation Roadmap

### Phase 1: Foundation (Weeks 1-4)
**Goal**: Basic graph construction from code + config files

- [ ] Set up Rust project structure with plugin architecture
- [ ] Integrate Tree-sitter for 5 core languages (Rust, Python, TypeScript, JavaScript, Go)
- [ ] Implement IndraDB backend integration
- [ ] Build symbol extractor (functions, classes, types)
- [ ] Build relation extractor (calls, imports, inheritance)
- [ ] **Configuration file support** (YAML, JSON, TOML, Properties, Markdown)
- [ ] **Code-to-config linking** (basic literal string matching)
- [ ] CLI: `rbuilder init <path>`
- [ ] Output: `graph.json` export

**Deliverable**: Parse a codebase and output a JSON graph of symbols, relationships, and configuration entities.

### Phase 2: Analysis + Hybrid NLP (Weeks 5-8)
**Goal**: Add graph intelligence + pattern-based NLP (no LLM required for most queries)

- [ ] Implement Leiden community detection
- [ ] Add complexity calculators (cyclomatic, cognitive)
- [ ] Add centrality metrics (PageRank, betweenness)
- [ ] Build dependency analysis (circular deps, impact radius)
- [ ] **Configuration analysis** (unused keys, missing env vars, secrets detection)
- [ ] **Pattern-based NLP engine** (intent classification, template matching)
- [ ] **Query cache system** with bootstrapping
- [ ] **20+ query templates** for common questions
- [ ] CLI: `rbuilder analyze --all`, `rbuilder ask <question>`
- [ ] Output: Enhanced `GRAPH_REPORT.md` with insights

**Deliverable**: Analyze graph to identify communities, complexity hotspots, architectural patterns, and answer 60%+ of NLP queries via pattern matching (no LLM calls).

### Phase 3: Rule Engine + Language Plugin System (Weeks 9-11)
**Goal**: Configurable labeling + extensible languages

- [ ] Design JSON ruleset schema
- [ ] Implement rule matcher (regex, AST patterns, graph patterns)
- [ ] Implement rule actions (labels, metadata, overrides)
- [ ] **Finalize LanguagePlugin and ConfigFormatPlugin traits**
- [ ] **External plugin loading** (dynamic library support)
- [ ] Add 3 more languages via plugins (Java, Kotlin, C#)
- [ ] CLI: `rbuilder label --ruleset <path>`, `rbuilder plugin install <path>`
- [ ] Example rulesets: security, deprecated, test coverage

**Deliverable**: Apply custom rules to label nodes + plugin system for adding languages.

### Phase 4: Semantic Translation + Domain Learning (Weeks 12-14)
**Goal**: Cross-language IDL + smart NLP

- [ ] Build type inference engine
- [ ] Extract semantic function signatures
- [ ] Implement IDL templates (Protocol Buffers, Thrift, OpenAPI)
- [ ] **Domain pattern learning** (auto-detect project-specific terminology)
- [ ] **Enhanced NLP** with domain context
- [ ] CLI: `rbuilder idl --format proto`
- [ ] Optional LLM integration for semantic descriptions

**Deliverable**: Generate IDL files + NLP queries that understand project-specific terminology.

### Phase 5: Incremental Updates & Performance (Weeks 15-16)
**Goal**: Production-ready performance

- [ ] Implement git-aware incremental indexing
- [ ] Add parallel processing for large repos
- [ ] Optimize graph queries (indexing, caching)
- [ ] Benchmark on large codebases (100k+ LOC)
- [ ] **Config file incremental updates**
- [ ] CLI: `rbuilder update --since <commit>`

**Deliverable**: Sub-second updates for incremental changes.

### Phase 6: MCP Integration + Visualization (Weeks 17-19)
**Goal**: AI agent integration + interactive exploration

- [ ] **MCP server implementation** (stdio + HTTP transports)
- [ ] **MCP tools for AI agents** (query, impact, complexity, config)
- [ ] **Context-efficient responses** (compressed for AI agents)
- [ ] **Claude Code integration testing**
- [ ] Build web-based graph visualizer (React + D3.js)
- [ ] Implement query DSL (Cypher-like)
- [ ] Add interactive filters (labels, complexity, communities)
- [ ] **Conversational query mode** (`rbuilder chat`)
- [ ] **Context retention and pronoun resolution**
- [ ] **Rich output formatting** (emojis, ASCII viz, recommendations)
- [ ] CLI: `rbuilder mcp serve`, `rbuilder serve`, `rbuilder query <query>`, `rbuilder chat`
- [ ] Export to Neo4j/Gephi for advanced visualization

**Deliverable**: MCP server for AI agents + Web UI + conversational interface for exploring the knowledge graph.

### Phase 7: Advanced Features (Weeks 20-22+)
**Goal**: Production enhancements + ecosystem

- [ ] Multi-repo support and cross-repo linking
- [ ] MCP server for AI agent integration
- [ ] Plugin marketplace/registry
- [ ] CI/CD integration (pre-commit hooks, GitHub Actions)
- [ ] **Configuration drift detection** across environments
- [ ] **Cross-language call detection** (FFI, REST APIs)
- [ ] Performance monitoring and profiling tools
- [ ] **Offline NLP mode** (template-based queries without LLM)

### Phase 8: Advanced Language Support (Weeks 23+)
**Goal**: Expand language ecosystem

- [ ] Add tier 2 languages (C++, C, Swift, PHP, Ruby, Scala, Elixir)
- [ ] Additional config formats (XML, INI, ENV, Protobuf, GraphQL, SQL, HCL)
- [ ] Advanced semantic extraction for complex languages
- [ ] Language-specific optimization passes
- [ ] Community-contributed language plugins

---

## 8. Key Design Decisions

### 8.1 Why Rust?
- **Performance**: Tree-sitter parsing + graph algorithms on large codebases
- **Portability**: Single binary, no runtime dependencies
- **Safety**: Memory safety for graph mutations
- **Ecosystem**: Excellent crates for parsing, graphs, CLI, async

### 8.2 Why IndraDB?
- **Embeddable**: No external database required
- **Portable**: Export/import entire graphs
- **Rust-native**: Performance and type safety
- **Flexible**: Pluggable storage backends

### 8.3 Why Tree-sitter?
- **Local**: No API calls, privacy-preserving
- **Fast**: Incremental parsing, error-tolerant
- **Multi-language**: 36+ grammars maintained
- **AST Quality**: High-fidelity syntax trees

### 8.4 Why Rule-Based Labeling?
- **Flexibility**: Users define domain-specific rules
- **Transparency**: Explainable labels (vs. ML black box)
- **Fast**: No model inference required
- **Composable**: Combine rules for complex classifications

---

## 9. Open Questions for Discussion

1. **Language Support Priority**: 
   - Which programming languages are most critical for your use case?
   - Which configuration formats must be supported in Phase 1?
   - Should we prioritize breadth (many languages, basic support) or depth (fewer languages, full semantic extraction)?

2. **Plugin System Approach**:
   - Should external plugins use stable ABI (slower evolution) or dynamic linking (more flexible)?
   - Do we need a plugin marketplace/registry?
   - Should plugins be sandboxed for security?

3. **Configuration Integration Depth**:
   - How aggressively should we link code to config? (Conservative: only literal strings, Aggressive: inferred usage patterns)
   - Should we parse schema files (JSON Schema, XSD) to validate configs?
   - Environment-specific config comparison: how to detect drift?

4. **Natural Language Query Priority**:
   - Is NLP querying a must-have or nice-to-have feature?
   - Preferred LLM provider? (Claude, OpenAI, local models, flexible?)
   - Should the system work without LLM (offline mode with templates)?
   - How important is conversational mode vs. one-shot queries?

5. **LLM Integration Scope**: 
   - Should semantic IDL generation *require* LLM, or fall back to heuristics?
   - Which providers to support for semantic analysis? (Claude, OpenAI, Ollama, custom)
   - Use LLM for semantic analysis of configuration files?
   - Budget for LLM API calls (if using cloud providers)?

6. **Graph Backend Flexibility**:
   - Should we support Neo4j as a first-class backend, or keep it optional?
   - Export-only vs. live Neo4j querying?
   - Support for distributed graph backends (e.g., for large monorepos)?

7. **IDL Format Priority**:
   - Which IDL formats are most critical? (Proto, Thrift, OpenAPI, GraphQL?)
   - Should we support custom templating?
   - Generate IDLs for configuration structures?

8. **Community Detection Algorithm**:
   - Leiden is state-of-art but complex. Start with Louvain for MVP?
   - Hierarchical communities (multi-level) or single-level?
   - Should communities span across languages and config files?

9. **Rule Engine Complexity**:
   - How expressive should the rule language be?
   - Support for graph queries in rules (e.g., "functions called by at least 5 other functions")?
   - Should rules be language-specific or universal?

10. **Visualization Approach**:
   - Build custom web UI, or integrate with existing tools (Gephi, Neo4j Browser)?
   - Offline HTML export vs. server-based?
   - Should visualization differentiate between code, config, and documentation nodes?

11. **Multi-Language Repositories**:
    - How to handle polyglot codebases? (e.g., Python backend + TypeScript frontend + Terraform infra)
    - Cross-language call detection (e.g., Python calling Rust via FFI, JavaScript calling REST API)?
    - Unified complexity metrics across languages?

---

## 10. Success Metrics

**Technical Performance:**
- Parse 100k LOC repository in < 60 seconds
- Incremental update < 5 seconds for 10 changed files
- Graph query response time < 100ms (99th percentile)
- Memory usage < 2GB for 1M LOC repository

**Language Support:**
- 10+ programming languages with full semantic extraction (Phase 1-2)
- 10+ configuration formats fully supported (Phase 1-2)
- External plugin loading working for custom languages
- < 500 lines of code to add a new basic language plugin

**Graph Quality:**
- > 95% symbol extraction accuracy for tier 1 languages
- > 90% relationship extraction accuracy
- < 5% false positives in code-to-config linking
- Community detection matches human-identified module boundaries (> 80% overlap)

**User Value:**
- Identify architectural modules (communities) without manual labeling
- Auto-detect high-complexity, low-test-coverage code
- Find unused configuration keys (reduce config bloat by > 20%)
- Detect missing environment variables before deployment
- Generate cross-language IDLs for API reimplementation
- Reduce codebase onboarding time by 50% (via graph exploration)
- Security: auto-flag > 90% of sensitive config keys

---

## 11. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Tree-sitter parsing errors in complex code | Medium | Error-tolerant parsing, confidence tagging |
| Large repositories exceed memory limits | High | Streaming graph construction, chunked processing |
| Community detection is too slow | Medium | Pre-compute at index time, cache results |
| Rule matching is too simplistic | Low | Iterative rule language design, user feedback |
| IDL generation lacks semantic accuracy | Medium | Hybrid approach (AST + optional LLM) |
| Graph backend migration complexity | Low | Backend abstraction trait, export/import tools |
| Language plugin API becomes unstable | Medium | Stable ABI with versioning, comprehensive tests |
| Config-to-code linking has high false positive rate | Medium | Conservative detection, confidence scoring, user feedback loop |
| Tree-sitter grammar incompatibilities across versions | Low | Pin grammar versions, test suite for each grammar |
| Plugin ecosystem doesn't materialize | Low | Focus on built-in languages first, clear documentation |
| Config file format variety overwhelming | Medium | Prioritize top 10 formats, plugin system for others |

---

## 12. References & Inspiration

**Projects:**
- [Graphify](https://github.com/safishamsi/graphify): Multi-language knowledge graph with Tree-sitter
- [GitNexus](https://github.com/abhigyanpatwari/GitNexus): Client-side graph with LadybugDB and MCP
- [IndraDB](https://github.com/indradb/indradb): Rust graph database
- [Tree-sitter](https://tree-sitter.github.io/tree-sitter/): Incremental parsing library

**Papers:**
- Leiden Algorithm: "From Louvain to Leiden: guaranteeing well-connected communities" (2019)
- Cognitive Complexity: SonarSource white paper
- Code Embedding: Microsoft CodeBERT, Salesforce CodeT5

**Standards:**
- Protocol Buffers: Google IDL
- Apache Thrift: Cross-language RPC
- OpenAPI: REST API specification

---

## Next Steps

### Immediate (Week 1-2)

1. **Validate Proposal**: Review and refine based on your feedback
2. **Design Language Plugin API**: Finalize `LanguagePlugin` and `ConfigFormatPlugin` traits
3. **Design Hybrid NLP System**: Finalize pattern matching, caching, and LLM fallback architecture
4. **Design MCP Integration**: Finalize MCP tools and resources for AI agents
5. **Prototype Phase 1**: Build minimal graph construction with 3 languages (Rust, Python, TypeScript)

### Short-term (Week 3-4)

6. **Implement Config Parsing**: Prototype YAML, JSON, TOML parsers with code linking
7. **Build Pattern-Based NLP**: Implement 20+ query templates for common questions
8. **Evaluate IndraDB**: Confirm performance characteristics with real repos
9. **Design Rule Schema**: Iterate on JSON ruleset format
10. **Prototype MCP Server**: Basic stdio transport with 3 core tools

### Medium-term (Week 5-8)

11. **Build Query Cache**: Implement learning system with embeddings
12. **Test with Claude Code**: Validate MCP integration with real AI agent
13. **Test External Plugin Loading**: Validate dynamic plugin system with a sample plugin
14. **Optimize NLP Performance**: Achieve < 5ms for 90% of queries

**Questions to Address:**

**Language Support:**
- Must-have programming languages for Phase 1? (Current: Rust, Python, TypeScript, JavaScript, Go)
- Must-have configuration formats for Phase 1? (Current: YAML, JSON, TOML, Properties, Markdown)
- Any domain-specific languages needed? (GraphQL, SQL, Terraform, etc.)
- Should we support markup languages deeply? (HTML, LaTeX, etc.)

**Use Case & Scale:**
- What is your primary use case? (Onboarding, refactoring, API migration, config audit, security analysis?)
- Target repository characteristics:
  - Lines of code? (10k, 100k, 1M+?)
  - Number of languages? (monoglot, 2-3, 5+?)
  - Number of config files? (handful, dozens, hundreds?)

**Technical Preferences:**
- Preferred graph backend? (Embedded IndraDB, Neo4j, flexible?)
- LLM integration: required, optional, or avoid?
- Plugin system priority: high (many languages via plugins) or low (focus on built-in support)?

**Configuration Analysis Depth:**
- How important is code-to-config linking? (Critical, nice-to-have, low-priority?)
- Should we validate configs against schemas? (JSON Schema, XSD, etc.)
- Do you need cross-environment config comparison? (dev vs prod drift detection?)
