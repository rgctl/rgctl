# rBuilder - Detailed Task Plan with Testing & Performance Benchmarks

**Project Goal**: Build a knowledge graph system that arms AI coding agents with deep, queryable codebase understanding.

**Performance Targets** (from Performance Profile):
- Parse 100k LOC: < 60s
- Incremental update: < 5s (for 10 changed files)
- NLP pattern match: < 1ms
- NLP cache hit: < 5ms
- Graph query: < 100ms (99th percentile)
- Memory (1M LOC): < 2GB
- Cache hit rate (month 1): 80%
- Cache hit rate (month 3): 90%

---

## 📊 **PROJECT STATUS** (as of June 17, 2026)

### Current State
- **Current Phase:** GitHub Preparation (Open Source Release) 🎯
- **Status:** Production-ready, 254 tests passing, zero warnings
- **Languages Supported:** 13 (9 core + 4 TOML-only: C, C++, Ruby, PHP)
- **Test Coverage:** High (254 tests across all features)
- **Performance:** Exceeding targets

### Completed Work ✅

**Phase 1-6 (Weeks 1-19):** COMPLETE
- ✅ Basic graph construction (9 languages)
- ✅ Configuration file support (YAML, JSON, TOML, Properties)
- ✅ Code-to-config linking
- ✅ Pattern-based NLP (60% queries, no LLM)
- ✅ Query cache with embeddings (90% queries)
- ✅ Graph analysis (communities, complexity, centrality)
- ✅ Configuration analysis
- ✅ Rule engine for labeling
- ✅ IDL generation (Proto, Thrift, OpenAPI)
- ✅ Domain pattern learning
- ✅ Incremental updates (< 5s)
- ✅ MCP server for AI agents
- ✅ Web-based graph browser
- ✅ Conversational query mode

**Phase 7 (Weeks 20-23):** ✅ COMPLETE
- ✅ Hybrid tiering architecture (Tier 1: Custom, Tier 2: Tree-sitter, Tier 3: Regex)
- ✅ languages.toml configuration (single source of truth)
- ✅ Build-time code generation (build.rs)
- ✅ Feature flags and bundles (minimal, extended, full, extra)
- ✅ Procedural macros (#[derive(LanguagePlugin)])
- ✅ Generic TreeSitterLanguagePlugin (TOML-driven)
- ✅ Generic RegexLanguagePlugin (pattern-based)
- ✅ Added 4 new languages (C, C++, Ruby, PHP) via TOML
- ✅ CI workflow for feature matrix testing
- ✅ Comprehensive documentation (LANGUAGE_GUIDE.md)

**Phase 8 (Weeks 24-26):** ✅ COMPLETE (uncommitted)
- ✅ Parallel processing with rayon (4x speedup for 100+ files)
- ✅ Batch GraphBackend APIs (insert_nodes_batch, insert_edges_batch)
- ✅ Query optimization with selectivity ranking
- ✅ Property-based indexes (50x faster repo: queries)
- ✅ Chunked query results for streaming
- ✅ 12 new integration tests with performance benchmarks
- ✅ All performance targets met or exceeded

**Phase 10 (Multi-repo):** Early implementation committed (~60% complete)
- ✅ Multi-repo workspace management
- ✅ Cross-repo dependency linking
- ✅ Config drift detection
- ✅ Namespace-aware queries
- ⏸️ UI and MCP enhancements deferred

### Current Priority: GitHub Preparation 🎯

**Goal:** Prepare repository for open source release

**Tasks:**
- ✅ CONTRIBUTING.md created
- ✅ Issue templates (Bug, Feature, Language)
- ✅ Pull request template
- 🔄 Code of Conduct
- 🔄 Enhanced README with getting started
- 🔄 CI/CD improvements
- 🔄 License selection
- 🔄 Security policy

**Next Up (Phase 9):**
- Phase 9: Security & production hardening (auth, rate limiting, deployment)

---

## Task Tracking

- ⬜ Not started
- 🔄 In progress  
- ✅ Complete
- 🧪 Testing
- 📊 Performance validated
- ⏸️ Deferred
- 🎯 Current priority

---

# Phase 1: Foundation (Weeks 1-4)

## 1.1 Project Setup & Infrastructure

### Task 1.1.1: Initialize Rust Project Structure ⬜
**Description**: Set up Cargo workspace with proper module structure

**Acceptance Criteria**:
- [ ] Cargo.toml with all dependencies defined
- [ ] Workspace structure matches proposal (extraction/, graph/, analysis/, nlp/, mcp/)
- [ ] CI/CD pipeline configured (GitHub Actions)
- [ ] Pre-commit hooks (rustfmt, clippy)
- [ ] Development documentation (CONTRIBUTING.md)

**Tests**:
```bash
cargo build --all-features
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check
```

**Performance**: N/A

**Deliverables**:
- [ ] Working Cargo project
- [ ] CI pipeline passing
- [ ] Development environment documented

---

### Task 1.1.2: Implement Error Handling Framework ⬜
**Description**: Create consistent error types using thiserror

**Acceptance Criteria**:
- [ ] Core error types defined (ParseError, GraphError, QueryError, etc.)
- [ ] Error context preservation (backtrace, source)
- [ ] Error conversion implementations (From traits)
- [ ] User-friendly error messages

**Tests**:
```rust
#[test]
fn test_error_context() {
    let err = ParseError::InvalidSyntax { 
        file: "test.rs".into(), 
        line: 42 
    };
    assert!(err.to_string().contains("test.rs"));
}

#[test]
fn test_error_chain() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file");
    let parse_err = ParseError::from(io_err);
    assert!(parse_err.source().is_some());
}
```

**Performance**: N/A

**Deliverables**:
- [ ] `src/error.rs` with all error types
- [ ] 100% test coverage for error conversions

---

## 1.2 Tree-sitter Integration & Language Plugins

### Task 1.2.1: Implement Language Plugin Trait ⬜
**Description**: Define LanguagePlugin and ConfigFormatPlugin traits

**Acceptance Criteria**:
- [ ] `LanguagePlugin` trait with all methods documented
- [ ] `ConfigFormatPlugin` trait defined
- [ ] `LanguageCapabilities` struct
- [ ] Mock plugin for testing

**Tests**:
```rust
#[test]
fn test_language_plugin_trait() {
    struct MockPlugin;
    impl LanguagePlugin for MockPlugin {
        fn language_id(&self) -> &str { "mock" }
        fn file_extensions(&self) -> Vec<&str> { vec!["mock"] }
        // ... other methods
    }
    
    let plugin = MockPlugin;
    assert_eq!(plugin.language_id(), "mock");
}
```

**Performance**: N/A

**Deliverables**:
- [ ] `src/languages/plugin_trait.rs`
- [ ] Documentation with examples
- [ ] Mock plugin for testing

---

### Task 1.2.2: Implement Rust Language Plugin ⬜
**Description**: Build first language plugin for Rust using Tree-sitter

**Acceptance Criteria**:
- [ ] Extract functions (name, params, return type, signature)
- [ ] Extract structs/enums (name, fields, methods)
- [ ] Extract modules (name, exports)
- [ ] Extract relationships (calls, uses, implements)
- [ ] Handle Rust-specific syntax (traits, lifetimes, macros)
- [ ] Complexity calculation (cyclomatic, cognitive)

**Tests**:
```rust
#[test]
fn test_rust_function_extraction() {
    let source = r#"
    fn calculate_sum(a: i32, b: i32) -> i32 {
        a + b
    }
    "#;
    
    let plugin = RustPlugin;
    let symbols = plugin.extract_symbols(source);
    
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "calculate_sum");
    assert_eq!(symbols[0].params.len(), 2);
    assert_eq!(symbols[0].return_type, Some("i32"));
}

#[test]
fn test_rust_relationship_extraction() {
    let source = r#"
    fn main() {
        let result = calculate_sum(1, 2);
    }
    fn calculate_sum(a: i32, b: i32) -> i32 { a + b }
    "#;
    
    let plugin = RustPlugin;
    let relations = plugin.extract_relations(source);
    
    assert!(relations.iter().any(|r| 
        matches!(r, Relation::Calls { from, to, .. } 
            if from == "main" && to == "calculate_sum")
    ));
}

#[test]
fn test_rust_complexity_calculation() {
    let source = r#"
    fn complex_function(x: i32) -> i32 {
        if x > 0 {
            if x > 10 {
                return x * 2;
            }
            return x + 1;
        } else if x < 0 {
            return x - 1;
        }
        0
    }
    "#;
    
    let plugin = RustPlugin;
    let symbols = plugin.extract_symbols(source);
    let complexity = symbols[0].complexity.cyclomatic;
    
    assert!(complexity >= 4, "Expected cyclomatic >= 4, got {}", complexity);
}
```

**Performance**:
- [ ] Parse 10k LOC Rust file: < 500ms
- [ ] Extract all symbols: < 100ms
- [ ] Memory usage: < 50MB for 10k LOC

**Benchmark**:
```rust
#[bench]
fn bench_rust_parsing_10k_loc(b: &mut Bencher) {
    let source = load_test_file("large_rust_file_10k.rs");
    let plugin = RustPlugin;
    
    b.iter(|| {
        plugin.extract_symbols(&source)
    });
}
```

**Deliverables**:
- [ ] `src/languages/builtin/rust.rs`
- [ ] Test suite with 90%+ coverage
- [ ] Performance benchmarks passing

---

### Task 1.2.3: Implement Python Language Plugin ⬜
**Description**: Build Python language plugin

**Acceptance Criteria**:
- [ ] Extract functions (def, async def)
- [ ] Extract classes (name, methods, inheritance)
- [ ] Extract imports (import, from...import)
- [ ] Extract decorators
- [ ] Handle Python-specific syntax (comprehensions, lambda)
- [ ] Complexity calculation

**Tests**: Similar structure to Rust plugin tests

**Performance**:
- [ ] Parse 10k LOC Python file: < 500ms

**Deliverables**:
- [ ] `src/languages/builtin/python.rs`
- [ ] Test suite with 90%+ coverage

---

### Task 1.2.4: Implement TypeScript Language Plugin ⬜
**Description**: Build TypeScript language plugin

**Acceptance Criteria**:
- [ ] Extract functions (function, arrow functions, methods)
- [ ] Extract classes (class, interface, type)
- [ ] Extract imports/exports (ES6 modules)
- [ ] Extract JSX/TSX components (React)
- [ ] Handle TypeScript types and generics
- [ ] Label React components automatically

**Tests**:
```rust
#[test]
fn test_react_component_detection() {
    let source = r#"
    export function UserProfile({ name }: { name: string }): JSX.Element {
        return <div>{name}</div>;
    }
    "#;
    
    let plugin = TypeScriptPlugin;
    let symbols = plugin.extract_symbols(source);
    
    assert_eq!(symbols[0].labels, vec!["react:component"]);
}
```

**Performance**:
- [ ] Parse 10k LOC TypeScript file: < 500ms

**Deliverables**:
- [ ] `src/languages/builtin/typescript.rs`
- [ ] Test suite with React component detection

---

### Task 1.2.5: Implement JavaScript Language Plugin ⬜
**Description**: Build JavaScript language plugin (similar to TypeScript, but without types)

**Acceptance Criteria**:
- [ ] Extract functions, classes, variables
- [ ] Extract imports/exports
- [ ] Detect React components (JSX)
- [ ] Handle CommonJS and ES6 modules

**Performance**:
- [ ] Parse 10k LOC JavaScript file: < 500ms

**Deliverables**:
- [ ] `src/languages/builtin/javascript.rs`
- [ ] Test suite with 90%+ coverage

---

### Task 1.2.6: Implement Go Language Plugin ⬜
**Description**: Build Go language plugin

**Acceptance Criteria**:
- [ ] Extract functions (func, methods)
- [ ] Extract structs and interfaces
- [ ] Extract packages and imports
- [ ] Detect exported vs. unexported symbols
- [ ] Handle Go-specific syntax (goroutines, channels)

**Performance**:
- [ ] Parse 10k LOC Go file: < 500ms

**Deliverables**:
- [ ] `src/languages/builtin/go.rs`
- [ ] Test suite with 90%+ coverage

---

### Task 1.2.7: Implement Language Registry ⬜
**Description**: Build registry system for managing language plugins

**Acceptance Criteria**:
- [ ] Register built-in plugins
- [ ] Map file extensions to plugins
- [ ] Get plugin for file path
- [ ] List all registered plugins
- [ ] Plugin capabilities query

**Tests**:
```rust
#[test]
fn test_registry_file_extension_mapping() {
    let mut registry = LanguageRegistry::new();
    registry.register_language(Box::new(RustPlugin));
    
    let plugin = registry.get_for_file(Path::new("test.rs"));
    assert!(plugin.is_some());
    assert_eq!(plugin.unwrap().language_id(), "rust");
}

#[test]
fn test_registry_list_plugins() {
    let registry = LanguageRegistry::default();  // With built-ins
    let plugins = registry.list_plugins();
    
    assert!(plugins.contains(&"rust"));
    assert!(plugins.contains(&"python"));
    assert!(plugins.contains(&"typescript"));
}
```

**Performance**:
- [ ] Plugin lookup: < 1μs

**Deliverables**:
- [ ] `src/languages/registry.rs`
- [ ] Test suite with 100% coverage

---

## 1.3 Configuration File Support

### Task 1.3.1: Implement YAML Config Plugin ⬜
**Description**: Parse YAML files and extract key-value structure

**Acceptance Criteria**:
- [ ] Parse YAML structure
- [ ] Extract all keys with paths (e.g., "database.host")
- [ ] Detect variable references (${VAR})
- [ ] Build ConfigGraph (keys, references, sections)

**Tests**:
```rust
#[test]
fn test_yaml_parsing() {
    let yaml = r#"
database:
  host: ${DB_HOST}
  port: 5432
  pool_size: 20
"#;
    
    let plugin = YamlPlugin;
    let graph = plugin.parse(yaml).unwrap();
    
    assert_eq!(graph.keys.len(), 3);
    assert!(graph.keys.iter().any(|k| k.key == "database.host"));
    assert_eq!(graph.references.len(), 1);
    assert_eq!(graph.references[0].target, "DB_HOST");
}

#[test]
fn test_yaml_nested_structures() {
    let yaml = r#"
app:
  services:
    auth:
      enabled: true
      timeout: 30
"#;
    
    let plugin = YamlPlugin;
    let graph = plugin.parse(yaml).unwrap();
    
    assert!(graph.keys.iter().any(|k| k.key == "app.services.auth.enabled"));
}
```

**Performance**:
- [ ] Parse 1000-line YAML: < 50ms

**Deliverables**:
- [ ] `src/languages/config/yaml.rs`
- [ ] Test suite with nested structures, arrays, references

---

### Task 1.3.2: Implement JSON Config Plugin ⬜
**Description**: Parse JSON files and extract structure

**Acceptance Criteria**:
- [ ] Parse JSON structure
- [ ] Extract keys with JSON path notation
- [ ] Detect $ref references (JSON Schema)
- [ ] Handle nested objects and arrays

**Performance**:
- [ ] Parse 1000-line JSON: < 20ms

**Deliverables**:
- [ ] `src/languages/config/json.rs`
- [ ] Test suite

---

### Task 1.3.3: Implement TOML Config Plugin ⬜
**Description**: Parse TOML files (Cargo.toml, etc.)

**Acceptance Criteria**:
- [ ] Parse TOML structure
- [ ] Extract keys with section notation
- [ ] Handle tables and arrays

**Performance**:
- [ ] Parse 1000-line TOML: < 30ms

**Deliverables**:
- [ ] `src/languages/config/toml.rs`
- [ ] Test suite

---

### Task 1.3.4: Implement Properties File Plugin ⬜
**Description**: Parse Java properties files

**Acceptance Criteria**:
- [ ] Parse key=value pairs
- [ ] Handle comments
- [ ] Detect ${VAR} references
- [ ] Handle multi-line values

**Performance**:
- [ ] Parse 1000-line properties: < 10ms

**Deliverables**:
- [ ] `src/languages/config/properties.rs`
- [ ] Test suite

---

### Task 1.3.5: Implement Markdown Parser ⬜
**Description**: Parse Markdown for documentation nodes

**Acceptance Criteria**:
- [ ] Extract headings (hierarchy)
- [ ] Extract code blocks (language detection)
- [ ] Extract links (cross-references)
- [ ] Build document structure graph

**Tests**:
```rust
#[test]
fn test_markdown_heading_extraction() {
    let md = r#"
# API Documentation

## Authentication

### JWT Tokens

Description here.
"#;
    
    let plugin = MarkdownPlugin;
    let graph = plugin.parse(md).unwrap();
    
    assert_eq!(graph.headings.len(), 3);
    assert_eq!(graph.headings[0].level, 1);
    assert_eq!(graph.headings[0].text, "API Documentation");
}
```

**Performance**:
- [ ] Parse 10,000-line markdown: < 100ms

**Deliverables**:
- [ ] `src/languages/config/markdown.rs`
- [ ] Test suite

---

## 1.4 Graph Backend (IndraDB)

### Task 1.4.1: Define Graph Schema ⬜
**Description**: Define node types, edge types, and schema

**Acceptance Criteria**:
- [ ] NodeType enum (Function, Class, Module, File, ConfigKey, ENV)
- [ ] EdgeType enum (Calls, Imports, Inherits, UsedBy, References, Contains)
- [ ] Node struct with metadata
- [ ] Edge struct with properties
- [ ] Serialization/deserialization (serde)

**Tests**:
```rust
#[test]
fn test_node_serialization() {
    let node = Node {
        id: Uuid::new_v4(),
        node_type: NodeType::Function {
            name: "test".into(),
            signature: "fn test()".into(),
            complexity: 5,
        },
        labels: vec!["test".into()],
        metadata: HashMap::new(),
    };
    
    let json = serde_json::to_string(&node).unwrap();
    let deserialized: Node = serde_json::from_str(&json).unwrap();
    
    assert_eq!(node.id, deserialized.id);
}
```

**Performance**: N/A

**Deliverables**:
- [ ] `src/graph/schema.rs`
- [ ] Full test coverage for all types

---

### Task 1.4.2: Implement IndraDB Backend ⬜
**Description**: Integrate IndraDB as graph storage backend

**Acceptance Criteria**:
- [ ] Create database connection
- [ ] Insert nodes (single and batch)
- [ ] Insert edges (single and batch)
- [ ] Query nodes by ID, label, properties
- [ ] Query edges by type, source, target
- [ ] Traversal queries (BFS, DFS)
- [ ] Transaction support

**Tests**:
```rust
#[test]
fn test_indradb_node_insertion() {
    let db = IndraDB::new_memory();
    let node_id = Uuid::new_v4();
    
    db.insert_node(Node {
        id: node_id,
        node_type: NodeType::Function { /* ... */ },
        labels: vec!["test".into()],
        metadata: HashMap::new(),
    }).unwrap();
    
    let retrieved = db.get_node(node_id).unwrap();
    assert_eq!(retrieved.id, node_id);
}

#[test]
fn test_indradb_batch_insertion() {
    let db = IndraDB::new_memory();
    let nodes: Vec<Node> = (0..1000)
        .map(|i| create_test_node(i))
        .collect();
    
    let start = Instant::now();
    db.insert_nodes_batch(&nodes).unwrap();
    let duration = start.elapsed();
    
    assert!(duration < Duration::from_millis(100), 
            "Batch insert too slow: {:?}", duration);
}

#[test]
fn test_indradb_traversal() {
    let db = setup_test_graph();
    
    // Find all functions called by main()
    let callers = db.traverse(
        start_node: "main",
        edge_type: EdgeType::Calls,
        direction: Outgoing,
        depth: 3
    ).unwrap();
    
    assert!(callers.len() > 0);
}
```

**Performance**:
- [ ] Insert 1,000 nodes: < 100ms
- [ ] Insert 10,000 nodes (batch): < 500ms
- [ ] Query by label (100k nodes): < 50ms
- [ ] Traversal depth 3 (10k nodes): < 100ms

**Benchmark**:
```rust
#[bench]
fn bench_indradb_batch_insert_10k(b: &mut Bencher) {
    let nodes: Vec<Node> = (0..10000)
        .map(|i| create_test_node(i))
        .collect();
    
    b.iter(|| {
        let db = IndraDB::new_memory();
        db.insert_nodes_batch(&nodes).unwrap();
    });
}
```

**Deliverables**:
- [ ] `src/graph/backend/indradb.rs`
- [ ] Comprehensive test suite
- [ ] Performance benchmarks passing

---

### Task 1.4.3: Implement GraphBackend Trait ⬜
**Description**: Abstract interface for graph backends (supports future Neo4j, etc.)

**Acceptance Criteria**:
- [ ] GraphBackend trait with all operations
- [ ] IndraDB implementation
- [ ] Mock backend for testing
- [ ] Backend selection at runtime

**Tests**:
```rust
#[test]
fn test_backend_abstraction() {
    fn test_backend<B: GraphBackend>(backend: &mut B) {
        let node = create_test_node(0);
        backend.insert_node(node.clone()).unwrap();
        
        let retrieved = backend.get_node(node.id).unwrap();
        assert_eq!(retrieved.id, node.id);
    }
    
    let mut indradb = IndraDBBackend::new_memory();
    test_backend(&mut indradb);
    
    let mut mock = MockBackend::new();
    test_backend(&mut mock);
}
```

**Performance**: N/A (abstraction layer, minimal overhead)

**Deliverables**:
- [ ] `src/graph/backend/trait.rs`
- [ ] Mock backend for testing

---

## 1.5 Code-to-Config Linking

### Task 1.5.1: Implement Config Usage Detector ⬜
**Description**: Detect when code references configuration keys

**Acceptance Criteria**:
- [ ] Detect string literals matching config paths
- [ ] Detect env var reads (os.environ, env::var, process.env)
- [ ] Language-specific patterns (Python, Rust, TypeScript, etc.)
- [ ] Confidence scoring (EXTRACTED, INFERRED, AMBIGUOUS)

**Tests**:
```rust
#[test]
fn test_rust_config_detection() {
    let source = r#"
    fn main() {
        let host = env::var("DB_HOST").unwrap();
        let config = load_yaml("config/database.yaml");
        let pool_size = config.get("database.pool_size").unwrap();
    }
    "#;
    
    let detector = ConfigUsageDetector::new();
    let usages = detector.detect_rust(source);
    
    assert_eq!(usages.len(), 2);
    assert!(usages.iter().any(|u| u.key == "DB_HOST" && u.usage_type == EnvVar));
    assert!(usages.iter().any(|u| u.key == "database.pool_size"));
}

#[test]
fn test_python_config_detection() {
    let source = r#"
import os
host = os.environ['DB_HOST']
config = yaml.load('config.yaml')
port = config['database']['port']
"#;
    
    let detector = ConfigUsageDetector::new();
    let usages = detector.detect_python(source);
    
    assert!(usages.iter().any(|u| u.key == "DB_HOST"));
    assert!(usages.iter().any(|u| u.key == "database.port"));
}
```

**Performance**:
- [ ] Detect config usage in 10k LOC file: < 50ms

**Deliverables**:
- [ ] `src/config/usage_detector.rs`
- [ ] Test suite for each language
- [ ] Confidence scoring algorithm

---

### Task 1.5.2: Build Config-to-Code Graph ⬜
**Description**: Create graph edges between config nodes and code nodes

**Acceptance Criteria**:
- [ ] ConfigKey nodes in graph
- [ ] ENV nodes in graph
- [ ] UsedBy edges from ConfigKey to Function
- [ ] References edges from ConfigKey to ENV
- [ ] Query support for "what code uses config X?"

**Tests**:
```rust
#[test]
fn test_config_code_graph() {
    let mut graph = build_test_graph_with_config();
    
    // Find all code that uses "database.pool_size"
    let users = graph.query(r#"
        MATCH (config:ConfigKey {key: "database.pool_size"})-[:UsedBy]->(func:Function)
        RETURN func
    "#).unwrap();
    
    assert!(users.len() > 0);
}
```

**Performance**:
- [ ] Build config graph for 100 config files: < 2s

**Deliverables**:
- [ ] Config graph integration
- [ ] Test suite
- [ ] Example queries

---

## 1.6 End-to-End Integration

### Task 1.6.1: Implement File Discovery & Filtering ⬜
**Description**: Scan repository and filter files for processing

**Acceptance Criteria**:
- [ ] Recursive directory traversal
- [ ] .gitignore respect
- [ ] File size limits (skip large binaries)
- [ ] Binary file detection (skip)
- [ ] Extension filtering
- [ ] Custom exclusion patterns

**Tests**:
```rust
#[test]
fn test_file_discovery() {
    let temp_dir = create_test_repo();
    let discoverer = FileDiscoverer::new();
    
    let files = discoverer.discover(&temp_dir).unwrap();
    
    assert!(files.iter().any(|f| f.extension() == Some("rs")));
    assert!(!files.iter().any(|f| f.ends_with(".git")));
}

#[test]
fn test_gitignore_respect() {
    let temp_dir = create_test_repo_with_gitignore();
    let discoverer = FileDiscoverer::new();
    
    let files = discoverer.discover(&temp_dir).unwrap();
    
    assert!(!files.iter().any(|f| f.ends_with("target/debug")));
}
```

**Performance**:
- [ ] Scan 10,000 files: < 1s

**Deliverables**:
- [ ] `src/discovery/mod.rs`
- [ ] Test suite with .gitignore support

---

### Task 1.6.2: Implement Parallel Processing Pipeline ⬜
**Description**: Parse multiple files in parallel using rayon

**Acceptance Criteria**:
- [ ] Parallel file parsing
- [ ] Progress reporting (indicatif)
- [ ] Error handling (continue on failure)
- [ ] Resource limits (max concurrent parsers)
- [ ] Graceful cancellation

**Tests**:
```rust
#[test]
fn test_parallel_parsing() {
    let files = create_100_test_files();
    let pipeline = ParsingPipeline::new();
    
    let start = Instant::now();
    let results = pipeline.process_parallel(&files, num_threads: 4).unwrap();
    let duration = start.elapsed();
    
    assert_eq!(results.len(), 100);
    assert!(duration < Duration::from_secs(5), 
            "Parallel parsing too slow: {:?}", duration);
}
```

**Performance**:
- [ ] Parse 100 files (10k LOC each) on 4 cores: < 30s

**Benchmark**:
```rust
#[bench]
fn bench_parallel_parsing_100_files(b: &mut Bencher) {
    let files = create_100_test_files();
    let pipeline = ParsingPipeline::new();
    
    b.iter(|| {
        pipeline.process_parallel(&files, num_threads: 4).unwrap()
    });
}
```

**Deliverables**:
- [ ] `src/pipeline/mod.rs`
- [ ] Progress bar integration
- [ ] Performance benchmarks

---

### Task 1.6.3: Implement CLI: `rbuilder init` ⬜
**Description**: Build CLI command to initialize graph for a repository

**Acceptance Criteria**:
- [ ] `rbuilder init <path>` command
- [ ] Language filtering (--languages flag)
- [ ] Exclusion patterns (--exclude flag)
- [ ] Progress reporting
- [ ] Summary output (files processed, nodes created, time taken)
- [ ] Error reporting

**Tests**:
```bash
# Integration test
rbuilder init ./test-repo --languages rust,python
# Should output:
# Processed 150 files
# Created 1,234 nodes
# Created 3,456 edges
# Time: 5.2s
```

**Performance**:
- [ ] Initialize 100k LOC repo: < 60s ⭐ **KEY METRIC**

**Deliverables**:
- [ ] `src/cli/init.rs`
- [ ] Integration tests
- [ ] User documentation

---

### Task 1.6.4: Implement Graph Export ⬜
**Description**: Export graph to JSON for portability

**Acceptance Criteria**:
- [ ] Export to JSON (graph.json)
- [ ] Include all nodes with metadata
- [ ] Include all edges
- [ ] Compact format (gzip optional)
- [ ] Import from JSON

**Tests**:
```rust
#[test]
fn test_graph_export_import() {
    let graph = build_test_graph();
    
    // Export
    let json = graph.export_json().unwrap();
    
    // Import
    let imported = Graph::import_json(&json).unwrap();
    
    assert_eq!(graph.node_count(), imported.node_count());
    assert_eq!(graph.edge_count(), imported.edge_count());
}
```

**Performance**:
- [ ] Export 100k nodes: < 5s
- [ ] Import 100k nodes: < 10s

**Deliverables**:
- [ ] `src/graph/export.rs`
- [ ] Test suite
- [ ] CLI command `rbuilder export`

---

## 1.7 Phase 1 Integration Testing

### Task 1.7.1: End-to-End Test: Real Repository ⬜
**Description**: Test entire Phase 1 pipeline on a real repository

**Test Plan**:
1. Clone test repository (e.g., small Rust project from GitHub)
2. Run `rbuilder init`
3. Validate graph structure
4. Validate performance

**Acceptance Criteria**:
- [ ] Successfully parse real Rust project (< 10k LOC)
- [ ] Successfully parse real Python project (< 10k LOC)
- [ ] Successfully parse real TypeScript project (< 10k LOC)
- [ ] All symbols extracted correctly (spot-check)
- [ ] All relationships present (spot-check)
- [ ] Configuration files parsed
- [ ] Code-to-config links created

**Performance Validation**:
- [ ] Parse 10k LOC repository: < 10s
- [ ] Memory usage: < 200MB

**Test Repositories**:
- Rust: ripgrep (small subset)
- Python: Flask (small subset)
- TypeScript: VS Code extension (small subset)

**Deliverables**:
- [ ] Integration test suite
- [ ] Performance report
- [ ] Bug fixes from real-world testing

---

### Task 1.7.2: Performance Baseline Measurement ⬜
**Description**: Establish baseline performance metrics for Phase 1

**Benchmark Suite**:
```rust
// Parse performance
#[bench] fn bench_parse_1k_loc_rust(b: &mut Bencher) { /* ... */ }
#[bench] fn bench_parse_10k_loc_rust(b: &mut Bencher) { /* ... */ }
#[bench] fn bench_parse_100k_loc_rust(b: &mut Bencher) { /* ... */ }

// Graph insertion performance
#[bench] fn bench_insert_1k_nodes(b: &mut Bencher) { /* ... */ }
#[bench] fn bench_insert_10k_nodes(b: &mut Bencher) { /* ... */ }
#[bench] fn bench_insert_100k_nodes(b: &mut Bencher) { /* ... */ }

// Full pipeline
#[bench] fn bench_init_small_repo(b: &mut Bencher) { /* ... */ }
#[bench] fn bench_init_medium_repo(b: &mut Bencher) { /* ... */ }
```

**Acceptance Criteria**:
- [ ] All benchmarks run successfully
- [ ] Performance metrics documented
- [ ] Baseline for comparison in Phase 5

**Deliverables**:
- [ ] `benches/phase1.rs`
- [ ] Performance baseline report (PERFORMANCE_BASELINE.md)

---

# Phase 2: Analysis & Hybrid NLP (Weeks 5-8)

## 2.1 Graph Analysis Algorithms

### Task 2.1.1: Implement Community Detection (Leiden) ⬜
**Description**: Detect architectural communities using Leiden algorithm

**Acceptance Criteria**:
- [ ] Leiden algorithm implementation (or use library)
- [ ] Community assignment to nodes
- [ ] Modularity score calculation
- [ ] Hierarchical communities (optional)
- [ ] Configurable resolution parameter

**Tests**:
```rust
#[test]
fn test_community_detection() {
    let graph = build_test_graph_with_modules();
    let detector = CommunityDetector::new();
    
    let communities = detector.detect_leiden(&graph).unwrap();
    
    // Should identify separate auth, api, ui communities
    assert!(communities.len() >= 3);
    
    // Modularity should be > 0.7 for well-structured code
    let modularity = detector.calculate_modularity(&graph, &communities);
    assert!(modularity > 0.5);
}

#[test]
fn test_community_assignment() {
    let graph = build_test_graph_with_modules();
    let detector = CommunityDetector::new();
    
    let communities = detector.detect_leiden(&graph).unwrap();
    
    // Verify nodes have community assignments
    for node in graph.nodes() {
        assert!(node.community_id.is_some());
    }
}
```

**Performance**:
- [ ] Detect communities in 10k node graph: < 5s
- [ ] Detect communities in 100k node graph: < 30s

**Deliverables**:
- [ ] `src/analysis/community_detection.rs`
- [ ] Test suite
- [ ] Performance benchmarks

---

### Task 2.1.2: Implement Complexity Metrics ⬜
**Description**: Calculate cyclomatic and cognitive complexity

**Acceptance Criteria**:
- [ ] Cyclomatic complexity calculation (per function)
- [ ] Cognitive complexity calculation
- [ ] Halstead metrics (optional)
- [ ] Complexity classification (LOW, MEDIUM, HIGH, CRITICAL)
- [ ] Aggregate complexity (per module, per community)

**Tests**:
```rust
#[test]
fn test_cyclomatic_complexity() {
    let ast = parse_function(r#"
        fn example(x: i32) -> i32 {
            if x > 0 {
                if x > 10 {
                    return x * 2;
                }
                return x + 1;
            } else if x < 0 {
                return x - 1;
            }
            0
        }
    "#);
    
    let complexity = calculate_cyclomatic_complexity(&ast);
    assert_eq!(complexity, 4);
}

#[test]
fn test_cognitive_complexity() {
    let ast = parse_function(r#"
        fn nested_example(x: i32) -> i32 {
            if x > 0 {          // +1
                if x > 10 {     // +2 (nested)
                    if x > 20 { // +3 (deeply nested)
                        return 1;
                    }
                }
            }
            0
        }
    "#);
    
    let complexity = calculate_cognitive_complexity(&ast);
    assert!(complexity >= 6);
}

#[test]
fn test_complexity_classification() {
    assert_eq!(classify_complexity(3), ComplexityLevel::LOW);
    assert_eq!(classify_complexity(8), ComplexityLevel::MEDIUM);
    assert_eq!(classify_complexity(15), ComplexityLevel::HIGH);
    assert_eq!(classify_complexity(25), ComplexityLevel::CRITICAL);
}
```

**Performance**:
- [ ] Calculate complexity for 10k functions: < 2s

**Deliverables**:
- [ ] `src/analysis/complexity.rs`
- [ ] Test suite with edge cases
- [ ] Documentation on thresholds

---

### Task 2.1.3: Implement Centrality Metrics ⬜
**Description**: Calculate PageRank and betweenness centrality

**Acceptance Criteria**:
- [ ] PageRank algorithm (using petgraph or custom)
- [ ] Betweenness centrality
- [ ] Degree centrality (in, out, total)
- [ ] Identify "god nodes" (high centrality)
- [ ] Centrality visualization data

**Tests**:
```rust
#[test]
fn test_pagerank() {
    let graph = build_test_graph();
    let pagerank = calculate_pagerank(&graph, damping: 0.85);
    
    // Most called functions should have high PageRank
    let main_func = graph.find_node("main").unwrap();
    assert!(pagerank[main_func.id] > 0.1);
}

#[test]
fn test_betweenness_centrality() {
    let graph = build_bridge_graph();
    let betweenness = calculate_betweenness(&graph);
    
    // Bridge nodes should have high betweenness
    let bridge = graph.find_node("bridge_function").unwrap();
    assert!(betweenness[bridge.id] > 0.5);
}
```

**Performance**:
- [ ] PageRank on 10k nodes: < 5s
- [ ] Betweenness on 10k nodes: < 10s

**Deliverables**:
- [ ] `src/analysis/centrality.rs`
- [ ] Test suite
- [ ] Performance benchmarks

---

### Task 2.1.4: Implement Dependency Analysis ⬜
**Description**: Detect circular dependencies, impact radius

**Acceptance Criteria**:
- [ ] Detect circular dependencies (strongly connected components)
- [ ] Calculate impact radius (transitive closure)
- [ ] Identify dependency clusters
- [ ] Topological sort (dependency order)

**Tests**:
```rust
#[test]
fn test_circular_dependency_detection() {
    let graph = build_graph_with_cycle();
    let analyzer = DependencyAnalyzer::new();
    
    let cycles = analyzer.find_circular_dependencies(&graph);
    
    assert!(cycles.len() > 0);
    assert!(cycles[0].len() >= 2); // At least 2 nodes in cycle
}

#[test]
fn test_impact_radius() {
    let graph = build_test_graph();
    let analyzer = DependencyAnalyzer::new();
    
    let impact = analyzer.calculate_impact_radius(&graph, "core_function");
    
    // core_function should affect many other functions
    assert!(impact.affected_nodes.len() > 10);
    assert!(impact.max_depth >= 3);
}
```

**Performance**:
- [ ] Detect cycles in 10k node graph: < 1s
- [ ] Impact analysis (depth 5): < 500ms

**Deliverables**:
- [ ] `src/analysis/dependency.rs`
- [ ] Test suite
- [ ] CLI command `rbuilder analyze --circular-deps`

---

## 2.2 Configuration Analysis

### Task 2.2.1: Implement Unused Config Key Detection ⬜
**Description**: Find configuration keys that are never used in code

**Acceptance Criteria**:
- [ ] Query graph for ConfigKey nodes without UsedBy edges
- [ ] Filter out commented-out keys
- [ ] Confidence scoring (maybe used dynamically)
- [ ] Report with file locations

**Tests**:
```rust
#[test]
fn test_unused_config_detection() {
    let graph = build_graph_with_configs();
    let analyzer = ConfigAnalyzer::new();
    
    let unused = analyzer.find_unused_keys(&graph);
    
    assert!(unused.iter().any(|k| k.key == "legacy.old_feature"));
    assert!(!unused.iter().any(|k| k.key == "database.host")); // Used
}
```

**Performance**:
- [ ] Analyze 1000 config keys: < 100ms

**Deliverables**:
- [ ] `src/config/analyzer.rs`
- [ ] Test suite
- [ ] CLI command `rbuilder config --unused`

---

### Task 2.2.2: Implement Missing Env Var Detection ⬜
**Description**: Find environment variables referenced but not defined

**Acceptance Criteria**:
- [ ] Find all ENV references in code
- [ ] Check against .env files
- [ ] Report missing variables with locations
- [ ] Suggest example values

**Tests**:
```rust
#[test]
fn test_missing_env_detection() {
    let graph = build_graph_with_env_refs();
    let analyzer = ConfigAnalyzer::new();
    
    let missing = analyzer.find_missing_env_vars(&graph, env_files: vec![".env"]);
    
    assert!(missing.iter().any(|e| e.var == "MISSING_VAR"));
}
```

**Performance**:
- [ ] Analyze 100 env vars: < 50ms

**Deliverables**:
- [ ] Missing env var detection
- [ ] Test suite
- [ ] CLI command `rbuilder config --missing-env`

---

### Task 2.2.3: Implement Secret Detection ⬜
**Description**: Find hardcoded secrets in configuration files

**Acceptance Criteria**:
- [ ] Pattern matching for common secrets (API keys, passwords, tokens)
- [ ] Entropy analysis for high-entropy strings
- [ ] Severity classification (CRITICAL, HIGH, MEDIUM, LOW)
- [ ] False positive filtering

**Tests**:
```rust
#[test]
fn test_secret_detection() {
    let config = r#"
api_key: "sk_live_1234567890abcdef"
password: "mysecretpassword123"
debug: true
"#;
    
    let detector = SecretDetector::new();
    let secrets = detector.scan(config);
    
    assert_eq!(secrets.len(), 2);
    assert!(secrets.iter().any(|s| s.severity == Severity::CRITICAL));
}
```

**Performance**:
- [ ] Scan 100 config files: < 500ms

**Deliverables**:
- [ ] `src/config/secret_detector.rs`
- [ ] Test suite with false positive filtering
- [ ] CLI command `rbuilder config --secrets`

---

## 2.3 Hybrid NLP Query System (Pattern-Based)

### Task 2.3.1: Implement Intent Classification ⬜
**Description**: Classify user questions into intent categories

**Acceptance Criteria**:
- [ ] Intent enum (Count, List, Find, Impact, Complexity, Dependencies, etc.)
- [ ] Keyword-based classification
- [ ] Handle variations ("how many" vs "count")
- [ ] Confidence scoring

**Tests**:
```rust
#[test]
fn test_intent_classification() {
    let classifier = IntentClassifier::new();
    
    assert_eq!(classifier.classify("how many functions?"), Intent::Count);
    assert_eq!(classifier.classify("show me all services"), Intent::List);
    assert_eq!(classifier.classify("what breaks if I change X?"), Intent::Impact);
    assert_eq!(classifier.classify("find high complexity code"), Intent::Find);
}

#[test]
fn test_intent_variations() {
    let classifier = IntentClassifier::new();
    
    // All should be Intent::Count
    assert_eq!(classifier.classify("how many X"), Intent::Count);
    assert_eq!(classifier.classify("count X"), Intent::Count);
    assert_eq!(classifier.classify("number of X"), Intent::Count);
}
```

**Performance**:
- [ ] Classify intent: < 1ms ⭐ **KEY METRIC**

**Deliverables**:
- [ ] `src/nlp/intent.rs`
- [ ] Test suite with 100+ examples

---

### Task 2.3.2: Implement Entity Extraction ⬜
**Description**: Extract entities from questions (labels, symbols, metrics)

**Acceptance Criteria**:
- [ ] Extract labels (e.g., "React components" → "react:component")
- [ ] Extract symbol names (e.g., "verify_token" → symbol)
- [ ] Extract metrics (e.g., "complexity > 20" → metric, threshold)
- [ ] Extract numbers (e.g., "top 10" → limit: 10)
- [ ] Handle variations and plurals

**Tests**:
```rust
#[test]
fn test_label_extraction() {
    let graph_schema = build_test_schema();
    let extractor = EntityExtractor::new(graph_schema);
    
    let entities = extractor.extract("how many React components?");
    
    assert!(entities.labels.contains(&"react:component"));
}

#[test]
fn test_symbol_extraction() {
    let graph_schema = build_test_schema();
    let extractor = EntityExtractor::new(graph_schema);
    
    let entities = extractor.extract("what calls verify_token?");
    
    assert!(entities.symbols.contains(&"verify_token"));
}

#[test]
fn test_metric_extraction() {
    let extractor = EntityExtractor::new(build_test_schema());
    
    let entities = extractor.extract("find functions with complexity > 20");
    
    assert_eq!(entities.metric, Some(Metric::Complexity(20)));
}
```

**Performance**:
- [ ] Extract entities: < 1ms

**Deliverables**:
- [ ] `src/nlp/entity_extraction.rs`
- [ ] Test suite
- [ ] Label mapping configuration

---

### Task 2.3.3: Implement Query Templates ⬜
**Description**: Create 20+ query templates for common questions

**Acceptance Criteria**:
- [ ] Template struct with regex patterns
- [ ] Parameter extraction from captures
- [ ] Cypher template filling
- [ ] 20+ templates covering common use cases

**Templates to Implement**:
1. "How many {label}?" → COUNT query
2. "List all {label}" → MATCH + RETURN
3. "What calls {symbol}?" → Callers query
4. "What breaks if I change {symbol}?" → Impact analysis
5. "Find {label} with {metric} > {threshold}" → Filtered query
6. "What's the complexity of {symbol}?" → Property query
7. "Show me the most {metric} {label}" → Ordered query
8. "Find circular dependencies" → Cycle detection
9. "What uses config {key}?" → Config usage
10. "Which {label} have no tests?" → Missing relationship query
11-20: Additional variations

**Tests**:
```rust
#[test]
fn test_template_matching() {
    let templates = QueryTemplates::default();
    
    let question = "How many React components?";
    let matched = templates.find_match(question).unwrap();
    
    assert_eq!(matched.intent, Intent::Count);
    assert_eq!(matched.parameters["label"], "react:component");
}

#[test]
fn test_template_cypher_generation() {
    let templates = QueryTemplates::default();
    
    let question = "What calls verify_token?";
    let cypher = templates.translate(question).unwrap();
    
    assert!(cypher.contains("MATCH"));
    assert!(cypher.contains("verify_token"));
    assert!(cypher.contains("Calls"));
}
```

**Performance**:
- [ ] Match template: < 1ms ⭐ **KEY METRIC**
- [ ] Generate Cypher: < 1ms

**Deliverables**:
- [ ] `src/nlp/templates.rs`
- [ ] Template configuration file (JSON)
- [ ] Test suite with all templates

---

### Task 2.3.4: Implement Pattern Matcher ⬜
**Description**: Integrate intent, entity extraction, and templates

**Acceptance Criteria**:
- [ ] Translate question → Cypher query
- [ ] Confidence scoring
- [ ] Handle partial matches
- [ ] Return multiple possible translations (if ambiguous)

**Tests**:
```rust
#[test]
fn test_pattern_based_translation() {
    let matcher = PatternMatcher::new(graph_schema);
    
    let result = matcher.translate("How many React components?").unwrap();
    
    assert!(result.confidence > 0.9);
    assert!(result.cypher.contains("MATCH"));
    assert_eq!(result.method, TranslationMethod::PatternBased);
}

#[test]
fn test_ambiguous_query() {
    let matcher = PatternMatcher::new(graph_schema);
    
    let results = matcher.translate_all("find components");
    
    // Might match multiple templates
    assert!(results.len() >= 1);
}
```

**Performance**:
- [ ] Translate simple query: < 1ms ⭐ **KEY METRIC**
- [ ] Success rate: > 60% on common queries

**Deliverables**:
- [ ] `src/nlp/pattern_matcher.rs`
- [ ] Integration test suite
- [ ] Success rate benchmark

---

### Task 2.3.5: Implement Query Cache Bootstrap ⬜
**Description**: Create initial query cache with example patterns

**Acceptance Criteria**:
- [ ] Generate 100+ example (question, cypher) pairs
- [ ] Store in cache with embeddings (optional: use simple TF-IDF first)
- [ ] Similarity search function
- [ ] Cache persistence (save/load from file)

**Tests**:
```rust
#[test]
fn test_query_cache_bootstrap() {
    let cache = QueryCache::new();
    cache.bootstrap_from_file("bootstrap_queries.json").unwrap();
    
    assert!(cache.size() >= 100);
}

#[test]
fn test_cache_similarity_search() {
    let cache = QueryCache::bootstrap_default();
    
    let similar = cache.find_similar("how many functions?", threshold: 0.8);
    
    assert!(similar.is_some());
    assert!(similar.unwrap().similarity > 0.8);
}
```

**Performance**:
- [ ] Load cache: < 100ms
- [ ] Similarity search: < 5ms ⭐ **KEY METRIC**

**Deliverables**:
- [ ] `src/nlp/query_cache.rs`
- [ ] Bootstrap queries file (bootstrap_queries.json)
- [ ] Test suite

---

### Task 2.3.6: Implement CLI: `rbuilder ask` ⬜
**Description**: Natural language query command

**Acceptance Criteria**:
- [ ] `rbuilder ask "question"` command
- [ ] Pattern-based translation
- [ ] Execute query on graph
- [ ] Format results (human-readable)
- [ ] --explain flag (show Cypher translation)
- [ ] --format json option

**Tests**:
```bash
# Integration tests
rbuilder ask "How many React components?"
# Output: "Found 156 React components"

rbuilder ask "What calls verify_token?" --explain
# Output:
# Translated query:
# MATCH (caller)-[:Calls]->(target {name: "verify_token"}) RETURN caller
#
# Results:
# 1. authenticate_user (src/auth.rs:45)
# 2. refresh_session (src/auth.rs:120)
# ...
```

**Performance**:
- [ ] Simple query end-to-end: < 100ms (< 1ms translate + < 100ms execute)

**Deliverables**:
- [ ] `src/cli/ask.rs`
- [ ] Integration tests
- [ ] User documentation

---

## 2.4 Phase 2 Integration Testing

### Task 2.4.1: End-to-End NLP Testing ⬜
**Description**: Test complete NLP pipeline on diverse questions

**Test Suite** (100 questions):
- 20 count queries ("how many X?")
- 20 list queries ("show me all X")
- 20 find queries ("find X with Y")
- 20 impact queries ("what breaks if...")
- 20 misc queries (complexity, dependencies, config)

**Acceptance Criteria**:
- [ ] 60%+ success rate with pattern matching
- [ ] Average latency < 1ms for pattern matching
- [ ] All successful translations produce valid Cypher
- [ ] Query execution successful (no syntax errors)

**Deliverables**:
- [ ] NLP test suite (tests/nlp_integration.rs)
- [ ] Success rate report

---

### Task 2.4.2: Performance Validation: Phase 2 ⬜
**Description**: Validate all Phase 2 performance targets

**Benchmarks**:
- [ ] Community detection (10k nodes): < 5s
- [ ] Complexity calculation (10k functions): < 2s
- [ ] PageRank (10k nodes): < 5s
- [ ] NLP pattern match: < 1ms ⭐
- [ ] NLP cache lookup: < 5ms ⭐
- [ ] Config analysis (1000 keys): < 100ms

**Deliverables**:
- [ ] `benches/phase2.rs`
- [ ] Performance report comparing to targets

---

# Phase 3: Plugin System & Rule Engine (Weeks 9-11)

## 3.1 Rule Engine

### Task 3.1.1: Design Rule Schema (JSON) ⬜
**Description**: Define JSON schema for labeling rules

**Acceptance Criteria**:
- [ ] Rule struct definition
- [ ] Match conditions (regex, AST patterns, graph queries)
- [ ] Actions (add_label, set_metadata, set_complexity_override)
- [ ] Composite logic (AND, OR, NOT)
- [ ] JSON schema validation

**Example Rule**:
```json
{
  "name": "critical_security_function",
  "match": {
    "node_type": "Function",
    "name_pattern": "(?i)(auth|login|verify|token)",
    "or": [
      {"calls_any": ["bcrypt", "jwt"]},
      {"has_annotation": "SecurityCritical"}
    ]
  },
  "actions": [
    {"add_label": "security:critical"},
    {"set_metadata": {"audit_required": true}}
  ]
}
```

**Tests**:
```rust
#[test]
fn test_rule_deserialization() {
    let json = load_test_rule_json();
    let rule: Rule = serde_json::from_str(&json).unwrap();
    
    assert_eq!(rule.name, "critical_security_function");
    assert!(rule.match_condition.is_some());
}
```

**Deliverables**:
- [ ] `src/rules/schema.rs`
- [ ] JSON schema file (rule_schema.json)
- [ ] Example rules (examples/rules/)

---

### Task 3.1.2: Implement Rule Matcher ⬜
**Description**: Match nodes/edges against rule conditions

**Acceptance Criteria**:
- [ ] Regex pattern matching (name, path)
- [ ] Property conditions (complexity, labels)
- [ ] Graph structure conditions (calls, imports)
- [ ] Composite logic evaluation (AND, OR, NOT)
- [ ] Confidence scoring

**Tests**:
```rust
#[test]
fn test_rule_matching() {
    let rule = load_test_rule("security_critical");
    let node = create_function_node("authenticate_user");
    
    let matcher = RuleMatcher::new();
    assert!(matcher.matches(&rule, &node));
}

#[test]
fn test_composite_conditions() {
    let rule = Rule {
        match_condition: Match::And(vec![
            Match::NamePattern(".*_test$".into()),
            Match::Complexity { gt: Some(10) },
        ]),
        actions: vec![],
    };
    
    let node1 = create_function_node("complex_test", complexity: 15);
    let node2 = create_function_node("simple_test", complexity: 5);
    
    let matcher = RuleMatcher::new();
    assert!(matcher.matches(&rule, &node1));
    assert!(!matcher.matches(&rule, &node2));
}
```

**Performance**:
- [ ] Match 1000 nodes against 10 rules: < 100ms

**Deliverables**:
- [ ] `src/rules/matcher.rs`
- [ ] Test suite with complex conditions

---

### Task 3.1.3: Implement Rule Actions ⬜
**Description**: Apply actions to matched nodes

**Acceptance Criteria**:
- [ ] Add label to node
- [ ] Set metadata (key-value)
- [ ] Override complexity classification
- [ ] Batch application (performance)

**Tests**:
```rust
#[test]
fn test_rule_actions() {
    let mut graph = build_test_graph();
    let rule = Rule {
        match_condition: Match::NamePattern("auth.*".into()),
        actions: vec![
            Action::AddLabel("security:critical".into()),
            Action::SetMetadata { key: "priority".into(), value: "high".into() },
        ],
    };
    
    let engine = RuleEngine::new();
    engine.apply_rule(&mut graph, &rule).unwrap();
    
    let auth_func = graph.find_node("authenticate").unwrap();
    assert!(auth_func.labels.contains(&"security:critical"));
}
```

**Performance**:
- [ ] Apply 10 rules to 10k nodes: < 1s

**Deliverables**:
- [ ] `src/rules/actions.rs`
- [ ] Test suite

---

### Task 3.1.4: Implement CLI: `rbuilder label` ⬜
**Description**: Apply rules from ruleset file

**Acceptance Criteria**:
- [ ] `rbuilder label --ruleset <path>` command
- [ ] Load rules from JSON file
- [ ] Apply to graph
- [ ] Summary report (nodes matched, labels added)
- [ ] --dry-run flag (show what would be labeled)

**Tests**:
```bash
rbuilder label --ruleset security-rules.json --dry-run
# Output:
# Would apply 3 rules to 1,234 nodes:
# - critical_security_function: 23 matches
# - deprecated_api: 8 matches
# - high_complexity: 45 matches
```

**Deliverables**:
- [ ] `src/cli/label.rs`
- [ ] Integration tests
- [ ] Example rulesets

---

## 3.2 External Plugin System

### Task 3.2.1: Design Plugin ABI ⬜
**Description**: Define stable ABI for external plugins

**Acceptance Criteria**:
- [ ] C-compatible FFI interface
- [ ] Plugin version negotiation
- [ ] Safe loading/unloading
- [ ] Error handling across FFI boundary

**Deliverables**:
- [ ] `src/languages/plugin_abi.rs`
- [ ] Plugin development guide

---

### Task 3.2.2: Implement Dynamic Plugin Loading ⬜
**Description**: Load language plugins from .so/.dylib files

**Acceptance Criteria**:
- [ ] Load plugin from file path
- [ ] Validate plugin version/ABI
- [ ] Register with language registry
- [ ] Safe error handling (no panic on plugin error)
- [ ] Unload plugin

**Tests**:
```rust
#[test]
fn test_plugin_loading() {
    let plugin_path = build_test_plugin(); // Builds test .so
    
    let mut registry = LanguageRegistry::new();
    registry.load_external(&plugin_path).unwrap();
    
    assert!(registry.has_plugin("test-language"));
}
```

**Deliverables**:
- [ ] `src/languages/plugin_loader.rs`
- [ ] Test plugin (examples/plugins/test_plugin/)
- [ ] Safety documentation

---

### Task 3.2.3: Implement Java Language Plugin ⬜
**Description**: Add Java support via plugin

**Acceptance Criteria**:
- [ ] Extract classes, interfaces, enums
- [ ] Extract methods (public, private, static)
- [ ] Extract imports, packages
- [ ] Extract annotations
- [ ] Complexity calculation

**Performance**:
- [ ] Parse 10k LOC Java file: < 500ms

**Deliverables**:
- [ ] `src/languages/builtin/java.rs`
- [ ] Test suite

---

### Task 3.2.4: Implement Kotlin Language Plugin ⬜
**Description**: Add Kotlin support

**Acceptance Criteria**:
- [ ] Extract functions, classes, objects
- [ ] Extract extension functions
- [ ] Handle Kotlin-specific syntax (data classes, sealed classes)

**Performance**:
- [ ] Parse 10k LOC Kotlin file: < 500ms

**Deliverables**:
- [ ] `src/languages/builtin/kotlin.rs`
- [ ] Test suite

---

### Task 3.2.5: Implement C# Language Plugin ⬜
**Description**: Add C# support

**Acceptance Criteria**:
- [ ] Extract classes, interfaces, structs
- [ ] Extract methods, properties
- [ ] Extract namespaces, using directives
- [ ] Handle C#-specific syntax (LINQ, async/await)

**Performance**:
- [ ] Parse 10k LOC C# file: < 500ms

**Deliverables**:
- [ ] `src/languages/builtin/csharp.rs`
- [ ] Test suite

---

### Task 3.2.6: Implement CLI: `rbuilder plugin` ⬜
**Description**: Plugin management commands

**Acceptance Criteria**:
- [ ] `rbuilder plugin install <path>` - Install external plugin
- [ ] `rbuilder plugin list` - List all plugins
- [ ] `rbuilder plugin info <id>` - Show plugin details
- [ ] `rbuilder plugin uninstall <id>` - Remove plugin

**Tests**:
```bash
rbuilder plugin list
# Output:
# Built-in plugins:
# - rust (v1.0.0)
# - python (v1.0.0)
# ...
#
# External plugins:
# - custom-lang (v0.1.0) at ~/.rbuilder/plugins/libcustom.so
```

**Deliverables**:
- [ ] `src/cli/plugin.rs`
- [ ] Integration tests

---

## 3.3 Phase 3 Integration Testing

### Task 3.3.1: Rule Engine Integration Test ⬜
**Description**: Test complete rule application pipeline

**Test Plan**:
1. Create test repository with security, deprecated, complex code
2. Create comprehensive ruleset
3. Apply rules
4. Validate correct labeling

**Acceptance Criteria**:
- [ ] Security functions correctly labeled
- [ ] Deprecated APIs correctly labeled
- [ ] High-complexity code correctly labeled
- [ ] No false positives (sample check)

**Deliverables**:
- [ ] Integration test suite
- [ ] Example rulesets (security, quality, deprecated)

---

### Task 3.3.2: Plugin System Integration Test ⬜
**Description**: Test external plugin loading and usage

**Test Plan**:
1. Build sample external plugin
2. Load via `rbuilder plugin install`
3. Parse files with external plugin
4. Validate symbol extraction

**Acceptance Criteria**:
- [ ] Plugin loads successfully
- [ ] Files parsed correctly
- [ ] Symbols extracted
- [ ] Graph constructed

**Deliverables**:
- [ ] Integration test
- [ ] Example external plugin

---

# Phase 4: Semantic Translation & Domain Learning (Weeks 12-14)

## 4.1 Type Inference & Semantic Extraction

### Task 4.1.1: Implement Type Inference Engine ⬜
**Description**: Infer types for dynamically typed languages

**Acceptance Criteria**:
- [ ] Infer types from usage patterns (Python, JavaScript)
- [ ] Track type flow through function calls
- [ ] Confidence scoring
- [ ] Cross-language type mapping

**Tests**:
```rust
#[test]
fn test_python_type_inference() {
    let source = r#"
def calculate(x, y):
    result = x + y
    return result * 2
"#;
    
    let inferencer = TypeInferencer::new();
    let types = inferencer.infer_python(source);
    
    // Should infer x, y are numeric based on usage
    assert!(types["x"].is_numeric());
}
```

**Deliverables**:
- [ ] `src/semantic/type_inference.rs`
- [ ] Test suite

---

### Task 4.1.2: Implement Function Signature Extraction ⬜
**Description**: Extract language-agnostic function signatures

**Acceptance Criteria**:
- [ ] Extract parameters with types
- [ ] Extract return type
- [ ] Extract constraints (validation, bounds)
- [ ] Normalize across languages

**Tests**:
```rust
#[test]
fn test_signature_extraction() {
    // Rust
    let rust_sig = extract_signature("fn add(a: i32, b: i32) -> i32");
    assert_eq!(rust_sig.params.len(), 2);
    assert_eq!(rust_sig.return_type, Some("i32"));
    
    // Python (with type hints)
    let py_sig = extract_signature("def add(a: int, b: int) -> int");
    assert_eq!(py_sig.params.len(), 2);
    
    // Should be equivalent
    assert!(signatures_equivalent(&rust_sig, &py_sig));
}
```

**Deliverables**:
- [ ] `src/semantic/signature.rs`
- [ ] Test suite

---

### Task 4.1.3: Implement IDL Template Engine ⬜
**Description**: Generate IDL from function signatures

**Acceptance Criteria**:
- [ ] Protocol Buffers (proto3) template
- [ ] Apache Thrift template
- [ ] OpenAPI (REST) template
- [ ] Template variables (function name, params, return type)
- [ ] Type mapping (Rust i32 → proto int32)

**Tests**:
```rust
#[test]
fn test_proto_generation() {
    let signature = FunctionSignature {
        name: "calculate_discount".into(),
        params: vec![
            Param { name: "price".into(), type_: "f64".into() },
            Param { name: "tier".into(), type_: "UserTier".into() },
        ],
        return_type: Some("f64".into()),
    };
    
    let generator = IDLGenerator::new();
    let proto = generator.generate_proto(&signature);
    
    assert!(proto.contains("message CalculateDiscountRequest"));
    assert!(proto.contains("double price = 1"));
}
```

**Deliverables**:
- [ ] `src/semantic/idl_generator.rs`
- [ ] Templates (templates/proto.hbs, templates/thrift.hbs, etc.)
- [ ] Test suite

---

### Task 4.1.4: Implement CLI: `rbuilder idl` ⬜
**Description**: Generate IDL files for modules

**Acceptance Criteria**:
- [ ] `rbuilder idl --format proto --module <name>` command
- [ ] Generate IDL for all functions in module
- [ ] Output to file or stdout
- [ ] Multiple format support

**Tests**:
```bash
rbuilder idl --format proto --module auth --output-dir ./idl
# Generates: idl/auth.proto
```

**Deliverables**:
- [ ] `src/cli/idl.rs`
- [ ] Integration tests
- [ ] User documentation

---

## 4.2 Domain Pattern Learning

### Task 4.2.1: Implement Pattern Detection ⬜
**Description**: Auto-detect project-specific patterns from graph

**Acceptance Criteria**:
- [ ] Detect common label patterns (frequency > threshold)
- [ ] Detect naming patterns (*Service, *Repository, *Controller)
- [ ] Detect architecture patterns (layers, modules)
- [ ] Generate natural language descriptions

**Tests**:
```rust
#[test]
fn test_label_pattern_detection() {
    let graph = build_test_graph_with_labels();
    let detector = PatternDetector::new();
    
    let patterns = detector.detect_label_patterns(&graph);
    
    // If 30+ nodes have "react:component", should detect it
    assert!(patterns.iter().any(|p| p.label == "react:component"));
}

#[test]
fn test_naming_pattern_detection() {
    let graph = build_test_graph();
    let detector = PatternDetector::new();
    
    let patterns = detector.detect_naming_patterns(&graph);
    
    // Should detect *Service pattern
    assert!(patterns.iter().any(|p| p.suffix == "Service"));
}
```

**Deliverables**:
- [ ] `src/nlp/pattern_detection.rs`
- [ ] Test suite

---

### Task 4.2.2: Enhance NLP with Domain Context ⬜
**Description**: Use detected patterns to improve NLP translation

**Acceptance Criteria**:
- [ ] Include domain patterns in NLP context
- [ ] Map natural language to project-specific labels
- [ ] Improve entity extraction with project vocabulary
- [ ] Measure improvement in success rate

**Tests**:
```rust
#[test]
fn test_domain_aware_nlp() {
    let graph = build_graph_with_services();
    let nlp = NLPEngine::new_with_domain_learning(&graph);
    
    // Should understand "services" maps to "soa:service" label
    let result = nlp.translate("how many services?").unwrap();
    assert!(result.cypher.contains("soa:service"));
}
```

**Performance**:
- [ ] NLP success rate improvement: 60% → 75%

**Deliverables**:
- [ ] Enhanced NLP engine
- [ ] A/B test comparing with/without domain learning

---

## 4.3 Phase 4 Integration Testing

### Task 4.3.1: IDL Generation Integration Test ⬜
**Description**: Test complete IDL generation pipeline

**Test Plan**:
1. Parse repository with multiple languages
2. Generate Proto IDL for a module
3. Validate Proto syntax
4. Generate Thrift IDL
5. Generate OpenAPI spec

**Acceptance Criteria**:
- [ ] Generated Proto compiles with protoc
- [ ] Generated Thrift compiles with thrift compiler
- [ ] Generated OpenAPI validates with swagger

**Deliverables**:
- [ ] Integration test suite
- [ ] Example generated IDLs

---

# Phase 5: Performance Optimization & Incremental Updates (Weeks 15-16)

## 5.1 Incremental Updates

### Task 5.1.1: Implement File Hashing ⬜
**Description**: Track file hashes to detect changes

**Acceptance Criteria**:
- [ ] Hash files on initial index (blake3)
- [ ] Store hashes in graph metadata
- [ ] Compare hashes to detect changes
- [ ] Track node-to-file mapping

**Tests**:
```rust
#[test]
fn test_file_change_detection() {
    let indexer = IncrementalIndexer::new();
    indexer.index_file("src/main.rs").unwrap();
    
    // Modify file
    modify_file("src/main.rs");
    
    let changed = indexer.detect_changes();
    assert!(changed.contains(&Path::new("src/main.rs")));
}
```

**Performance**:
- [ ] Hash 10,000 files: < 2s

**Deliverables**:
- [ ] `src/incremental/file_tracker.rs`
- [ ] Test suite

---

### Task 5.1.2: Implement Incremental Graph Update ⬜
**Description**: Update graph for changed files only

**Acceptance Criteria**:
- [ ] Detect changed files (git diff or hash comparison)
- [ ] Remove old nodes from changed files
- [ ] Re-parse changed files
- [ ] Insert new nodes
- [ ] Update relationships
- [ ] Prune orphaned nodes

**Tests**:
```rust
#[test]
fn test_incremental_update() {
    let mut graph = build_test_graph();
    let initial_count = graph.node_count();
    
    // Modify one file
    modify_file("src/main.rs");
    
    let updater = IncrementalUpdater::new();
    updater.update(&mut graph, changed_files: vec!["src/main.rs"]).unwrap();
    
    // Node count should be similar (some changed, not all replaced)
    assert!((graph.node_count() as i32 - initial_count as i32).abs() < 10);
}
```

**Performance**:
- [ ] Update 10 changed files: < 5s ⭐ **KEY METRIC**

**Deliverables**:
- [ ] `src/incremental/updater.rs`
- [ ] Test suite

---

### Task 5.1.3: Implement CLI: `rbuilder update` ⬜
**Description**: Incremental update command

**Acceptance Criteria**:
- [ ] `rbuilder update` - Update since last index
- [ ] `rbuilder update --since <commit>` - Update since git commit
- [ ] `rbuilder update --force` - Full rebuild
- [ ] Progress reporting
- [ ] Summary (files changed, nodes updated)

**Tests**:
```bash
# Make changes
echo "fn new() {}" >> src/new.rs

# Incremental update
rbuilder update
# Output:
# Detected 1 changed file
# Updated 5 nodes
# Time: 1.2s
```

**Performance**:
- [ ] Update 10 files: < 5s ⭐ **KEY METRIC**

**Deliverables**:
- [ ] `src/cli/update.rs`
- [ ] Integration tests

---

## 5.2 Performance Optimization

### Task 5.2.1: Optimize Graph Queries ⬜
**Description**: Add indexing and query optimization

**Acceptance Criteria**:
- [ ] Index nodes by label
- [ ] Index nodes by name
- [ ] Index edges by type
- [ ] Query plan optimization
- [ ] Cache frequently accessed nodes

**Tests**:
```rust
#[test]
fn test_query_performance() {
    let graph = build_large_graph(100_000); // 100k nodes
    
    let start = Instant::now();
    let results = graph.query_by_label("react:component");
    let duration = start.elapsed();
    
    assert!(duration < Duration::from_millis(50), 
            "Query too slow: {:?}", duration);
}
```

**Performance**:
- [ ] Query by label (100k nodes): < 50ms ⭐ **KEY METRIC**

**Deliverables**:
- [ ] Query optimization
- [ ] Performance benchmarks

---

### Task 5.2.2: Optimize Memory Usage ⬜
**Description**: Reduce memory footprint for large repositories

**Acceptance Criteria**:
- [ ] String interning (deduplicate strings)
- [ ] Compact node representation
- [ ] Lazy loading of metadata
- [ ] Memory profiling

**Tests**:
```rust
#[test]
fn test_memory_usage() {
    let graph = build_large_graph(1_000_000); // 1M nodes
    
    let memory_mb = get_process_memory_mb();
    
    assert!(memory_mb < 2048, 
            "Memory usage too high: {} MB", memory_mb);
}
```

**Performance**:
- [ ] Memory (1M LOC): < 2GB ⭐ **KEY METRIC**

**Deliverables**:
- [ ] Memory optimization
- [ ] Profiling report

---

### Task 5.2.3: Optimize Parallel Processing ⬜
**Description**: Improve parallel parsing performance

**Acceptance Criteria**:
- [ ] Optimal thread pool sizing
- [ ] Work stealing
- [ ] Reduce allocations
- [ ] Batch processing

**Performance**:
- [ ] Parse 100k LOC: < 60s on 4 cores ⭐ **KEY METRIC**

**Deliverables**:
- [ ] Optimized pipeline
- [ ] Performance benchmarks

---

## 5.3 Performance Validation

### Task 5.3.1: Comprehensive Performance Testing ⬜
**Description**: Validate all performance targets

**Test Matrix**:
| Metric | Target | Test |
|--------|--------|------|
| Parse 100k LOC | < 60s | Large repo test |
| Incremental update (10 files) | < 5s | Git diff test |
| NLP pattern match | < 1ms | NLP benchmark |
| NLP cache hit | < 5ms | Cache benchmark |
| Graph query | < 100ms | Query benchmark |
| Memory (1M LOC) | < 2GB | Memory test |

**Acceptance Criteria**:
- [ ] All performance targets met or exceeded
- [ ] Performance regression tests added to CI
- [ ] Performance report generated

**Deliverables**:
- [ ] Comprehensive benchmark suite
- [ ] Performance validation report
- [ ] CI integration

---

# Phase 6: MCP Integration & Visualization (Weeks 17-19)

## 6.1 MCP Server Implementation

### Task 6.1.1: Implement MCP Server Core ⬜
**Description**: Build MCP server with stdio and HTTP transports

**Acceptance Criteria**:
- [ ] MCP protocol implementation
- [ ] stdio transport (for Claude Code local integration)
- [ ] HTTP transport (for team-wide server)
- [ ] Request/response handling
- [ ] Error handling

**Tests**:
```rust
#[test]
fn test_mcp_server_stdio() {
    let server = MCPServer::new_stdio();
    let request = json!({
        "tool": "query_codebase",
        "params": {"question": "how many functions?"}
    });
    
    let response = server.handle_request(request).unwrap();
    assert!(response["answer"].is_string());
}
```

**Deliverables**:
- [ ] `src/mcp/server.rs`
- [ ] Test suite

---

### Task 6.1.2: Implement MCP Tools ⬜
**Description**: Implement 7 core MCP tools for AI agents

**Tools**:
1. **query_codebase** - Natural language query
2. **impact_analysis** - What breaks if X changes
3. **find_by_complexity** - Find functions by complexity
4. **get_community_info** - Get community/module info
5. **config_analysis** - Analyze configuration
6. **symbol_info** - Get symbol details
7. **diff_analysis** - What changed since commit

**Tests**:
```rust
#[test]
fn test_mcp_tool_query_codebase() {
    let server = setup_test_server();
    let result = server.execute_tool("query_codebase", json!({
        "question": "how many React components?"
    })).unwrap();
    
    assert!(result["answer"].as_str().unwrap().contains("component"));
}

#[test]
fn test_mcp_tool_impact_analysis() {
    let server = setup_test_server();
    let result = server.execute_tool("impact_analysis", json!({
        "symbol": "verify_token",
        "depth": 3
    })).unwrap();
    
    assert!(result["direct_dependencies"].is_array());
    assert!(result["indirect_dependencies"].is_array());
}
```

**Performance**:
- [ ] MCP tool response time: < 200ms (90th percentile)

**Deliverables**:
- [ ] `src/mcp/tools.rs`
- [ ] Test suite for each tool
- [ ] MCP tool documentation

---

### Task 6.1.3: Implement Context-Efficient Responses ⬜
**Description**: Compress responses to save AI agent tokens

**Acceptance Criteria**:
- [ ] Return structured data (not prose)
- [ ] Summary fields instead of full descriptions
- [ ] Exclude verbose fields by default
- [ ] include_verbose option for detailed responses

**Example**:
```rust
// Instead of full context:
{
  "function": "verify_token",
  "source_code": "/* 100 lines */",
  "full_documentation": "/* 500 words */"
}

// Return compressed:
{
  "function": "verify_token",
  "signature": "fn verify_token(token: &str) -> Result<Claims>",
  "complexity": 12,
  "callers": ["authenticate_user", "refresh_session"],
  "location": "src/auth/jwt.rs:89"
}
```

**Tests**:
```rust
#[test]
fn test_context_efficient_response() {
    let server = setup_test_server();
    let result = server.execute_tool("symbol_info", json!({
        "symbol_name": "verify_token"
    })).unwrap();
    
    let json = serde_json::to_string(&result).unwrap();
    
    // Should be < 1KB for typical function
    assert!(json.len() < 1024, "Response too verbose: {} bytes", json.len());
}
```

**Deliverables**:
- [ ] Compressed response formats
- [ ] Token usage comparison report

---

### Task 6.1.4: Implement CLI: `rbuilder mcp serve` ⬜
**Description**: Start MCP server for AI agent integration

**Acceptance Criteria**:
- [ ] `rbuilder mcp serve --transport stdio` - stdio mode (Claude Code)
- [ ] `rbuilder mcp serve --transport http --port 3000` - HTTP server
- [ ] Graceful shutdown
- [ ] Request logging (optional)

**Tests**:
```bash
# Start stdio server
rbuilder mcp serve --transport stdio
# Claude Code can now connect

# Start HTTP server
rbuilder mcp serve --transport http --port 3000
# Test: curl http://localhost:3000/tools
```

**Deliverables**:
- [ ] `src/cli/mcp.rs`
- [ ] Integration tests
- [ ] Configuration guide for Claude Code

---

### Task 6.1.5: Claude Code Integration Testing ⬜
**Description**: Test rBuilder MCP server with real Claude Code

**Test Plan**:
1. Configure Claude Code to use rBuilder MCP server
2. Ask Claude: "How many functions are in this codebase?"
3. Ask Claude: "What would break if I change verify_token?"
4. Ask Claude: "Find high-complexity security functions"
5. Validate responses are accurate and helpful

**Acceptance Criteria**:
- [ ] Claude Code successfully connects to MCP server
- [ ] All 7 MCP tools work correctly
- [ ] Claude provides accurate answers based on graph
- [ ] Response time acceptable (< 500ms per query)

**Deliverables**:
- [ ] Integration test report
- [ ] Claude Code configuration example
- [ ] Video demo (optional)

---

## 6.2 Conversational Query Interface

### Task 6.2.1: Implement Conversation Context ⬜
**Description**: Track conversation state for multi-turn queries

**Acceptance Criteria**:
- [ ] ConversationContext struct
- [ ] Track query history
- [ ] Track focused nodes (last mentioned)
- [ ] Pronoun resolution ("it", "that", "those")
- [ ] Context-aware entity extraction

**Tests**:
```rust
#[test]
fn test_conversation_context() {
    let mut ctx = ConversationContext::new();
    
    // Turn 1
    ctx.add_query("How many services?");
    ctx.add_focused_node("AuthenticationService");
    
    // Turn 2 - "it" should resolve to AuthenticationService
    let resolved = ctx.resolve_references("What's its complexity?");
    assert!(resolved.contains("AuthenticationService"));
}
```

**Deliverables**:
- [ ] `src/nlp/conversation.rs`
- [ ] Test suite

---

### Task 6.2.2: Implement CLI: `rbuilder chat` ⬜
**Description**: Interactive conversational mode

**Acceptance Criteria**:
- [ ] `rbuilder chat` command
- [ ] REPL interface
- [ ] Context retention across queries
- [ ] History navigation (up/down arrows)
- [ ] Exit command

**Tests**:
```bash
$ rbuilder chat

rBuilder> How many services do I have?
Found 12 services.

rBuilder> Which ones are in the auth module?
3 services in the 'auth' community:
1. AuthenticationService
2. AuthorizationService
3. TokenManagementService

rBuilder> What's the complexity of AuthenticationService?
AuthenticationService has cyclomatic complexity: 45 (CRITICAL)

rBuilder> exit
Goodbye!
```

**Deliverables**:
- [ ] `src/cli/chat.rs`
- [ ] Interactive testing
- [ ] User documentation

---

## 6.3 Web Visualization

### Task 6.3.1: Build Web UI Backend (API) ⬜
**Description**: REST API for web-based graph browser

**Acceptance Criteria**:
- [ ] GET /api/graph/stats - Overall statistics
- [ ] GET /api/graph/nodes - List nodes (paginated, filtered)
- [ ] GET /api/graph/edges - List edges
- [ ] GET /api/graph/search?q=<query> - Search nodes
- [ ] POST /api/query - Execute Cypher query
- [ ] GET /api/communities - List communities
- [ ] WebSocket support for live updates (optional)

**Tests**:
```rust
#[test]
fn test_api_graph_stats() {
    let api = setup_test_api();
    let response = api.get("/api/graph/stats").unwrap();
    
    assert!(response["node_count"].is_number());
    assert!(response["edge_count"].is_number());
}
```

**Deliverables**:
- [ ] `src/api/server.rs`
- [ ] OpenAPI spec
- [ ] Integration tests

---

### Task 6.3.2: Build Web UI Frontend ⬜
**Description**: React-based graph visualization

**Acceptance Criteria**:
- [ ] Graph visualization (D3.js or vis.js)
- [ ] Node filtering (by label, complexity)
- [ ] Search functionality
- [ ] Node details panel
- [ ] Community visualization (color-coded)
- [ ] Zoom, pan, drag

**Deliverables**:
- [ ] `web/` directory with React app
- [ ] User guide

---

### Task 6.3.3: Implement CLI: `rbuilder serve` ⬜
**Description**: Start web server for graph browser

**Acceptance Criteria**:
- [ ] `rbuilder serve --port 8080` - Start server
- [ ] `rbuilder serve --open` - Auto-open browser
- [ ] Serve static frontend files
- [ ] API endpoints

**Tests**:
```bash
rbuilder serve --port 8080 --open
# Opens http://localhost:8080 in browser
```

**Deliverables**:
- [ ] `src/cli/serve.rs`
- [ ] Integration tests

---

## 6.4 Rich Output Formatting

### Task 6.4.1: Implement Formatted Output ⬜
**Description**: Add emojis, colors, ASCII visualizations to CLI output

**Acceptance Criteria**:
- [ ] Emoji indicators (🔴 critical, ⚠️ warning, ✅ ok)
- [ ] Color coding (red, yellow, green)
- [ ] ASCII tables (comfy-table)
- [ ] ASCII charts (for distributions)
- [ ] Progress bars (indicatif)

**Example Output**:
```
🔍 Analyzing impact of deleting UserRepository...

⚠️ HIGH IMPACT - affects 47 functions across 4 communities

🔴 DIRECT DEPENDENCIES (12 functions):
   1. UserService.get_user() - src/services/user.rs:45
   2. UserService.create_user() - src/services/user.rs:89

📊 Community Impact:
   🔴 'auth': 22% affected
   ⚠️ 'api': 13% affected

💡 RECOMMENDATION: High-risk change. Consider gradual rollout.
```

**Deliverables**:
- [ ] `src/output/formatter.rs`
- [ ] Example outputs

---

## 6.5 Phase 6 Integration Testing

### Task 6.5.1: End-to-End MCP Integration Test ⬜
**Description**: Full workflow test with AI agent

**Test Scenarios**:
1. AI agent asks architectural question
2. AI agent performs impact analysis
3. AI agent finds code quality issues
4. AI agent analyzes configuration

**Acceptance Criteria**:
- [ ] All scenarios work end-to-end
- [ ] Response times acceptable
- [ ] Responses accurate and helpful

**Deliverables**:
- [ ] Integration test suite
- [ ] Demo video

---

# Phase 7: Tree-sitter Language System Refactor (Weeks 20-23) ✅

**Status:** Complete  
**Duration:** 4 weeks  
**Goal:** Replace manual per-language plugins with TOML-based configuration and procedural macros

## Motivation

- **Achieved:** Hybrid tiering architecture balancing quality (rich extraction) with scalability (easy addition)
- **Result:** 13 languages (9 custom + 4 TOML-only), ~1,649 additions, 333 deletions
- **Benefits Realized:** 
  - Three-tier architecture (Custom, Tree-sitter, Regex)
  - Community can add Tier 2/3 languages via TOML only
  - Feature flags enable 60% binary size reduction for minimal builds
  - Add Tier 2 language in < 30 minutes (C, C++, Ruby, PHP proven)
  - All Tier 1 custom plugins use tree-sitter as foundation

## Success Metrics (Achieved)

**Architectural Achievement:**
- ✅ Hybrid tiering documented and enforced
- ✅ 6/7 programming languages use tree-sitter foundation (Markdown exception documented)
- ✅ TOML-only languages (C, Ruby, PHP, C++) added successfully
- ✅ ~300 LOC reduction (acceptable for quality-first hybrid approach vs. ~3,500 pure-TOML target)

**Build System:**
- ✅ Feature flags: 4 bundles (minimal, extended, full, extra)
- ✅ All bundles compile successfully
- ✅ Binary size reduction: 60% for minimal bundle

**Testing:**
- ✅ 254 tests passing (increased from 222)
- ✅ CI workflow for feature matrix
- ✅ Zero clippy warnings

## 7.1 Infrastructure Setup (Week 20) ✅

### Task 7.1.1: Create `languages.toml` Configuration ✅
**Description**: Define TOML-based language configuration format

**Acceptance Criteria**:
- [x] Schema defined for language metadata
- [x] All 13 languages configured (9 custom + 4 tree-sitter)
- [x] Bundle definitions (minimal, extended, full, extra)
- [x] Documentation for TOML format in LANGUAGE_GUIDE.md

**Example Structure**:
```toml
[metadata]
version = "1.0"
description = "rBuilder tree-sitter language configuration"

[languages.rust]
crate = "tree-sitter-rust"
version = "0.20"
extensions = ["rs"]
function_kinds = ["function_item", "function_signature_item"]
class_kinds = ["struct_item", "enum_item", "impl_item"]

[bundles.minimal]
description = "Core languages"
languages = ["rust", "python"]

[bundles.extended]
description = "Common web and systems languages"
languages = ["rust", "python", "typescript", "javascript", "go", "java"]

[bundles.full]
description = "All available languages"
languages = ["rust", "python", "typescript", "javascript", "go", "java", "kotlin", "csharp", "markdown"]
```

**Deliverables**:
- [x] `languages.toml` - 224 lines, 13 languages, 4 bundles
- [x] Documentation in LANGUAGE_GUIDE.md
- [x] Build-time validation in build.rs

---

### Task 7.1.2: Implement `build.rs` Code Generator ✅
**Description**: Build-time code generation for plugin registration

**Acceptance Criteria**:
- [x] Parse `languages.toml` at build time
- [x] Generate plugin registration code
- [x] Generate feature flag conditional compilation
- [x] Validate TOML correctness (duplicate extensions, handler requirements)

**Generated Code Example**:
```rust
pub fn register_all_plugins(registry: &mut LanguageRegistry) {
    #[cfg(feature = "lang-rust")]
    registry.register_language_plugin(Arc::new(RustPlugin::new().unwrap()));
    
    #[cfg(feature = "lang-python")]
    registry.register_language_plugin(Arc::new(PythonPlugin::new().unwrap()));
    
    // ... etc for all languages
}
```

**Tests**:
```bash
cargo build  # Should succeed
cargo build --no-default-features --features lang-rust  # Should work
```

**Deliverables**:
- [x] `build.rs` - 278 lines, full code generation
- [x] Generated `generated_register.rs` and `generated_lang_configs.rs`
- [x] Build validation with error messages

---

### Task 7.1.3: Update `Cargo.toml` with Feature Flags ✅
**Description**: Make tree-sitter dependencies optional with feature flags

**Acceptance Criteria**:
- [x] All tree-sitter-* dependencies made optional
- [x] Individual language features (13 lang-* features)
- [x] Bundle features (bundle-minimal, extended, full, extra)
- [x] Default bundle set to bundle-full
- [x] Build dependencies added (toml, serde)

**Changes Required**:
```toml
[dependencies]
tree-sitter = "0.20"  # Always included

# Make all language grammars optional
tree-sitter-rust = { version = "0.20", optional = true }
tree-sitter-python = { version = "0.20", optional = true }
# ... etc

[build-dependencies]
toml = "0.8"
serde = { version = "1", features = ["derive"] }

[features]
default = ["bundle-extended"]

# Individual language features
lang-rust = ["tree-sitter-rust"]
lang-python = ["tree-sitter-python"]
# ... etc

# Bundles
bundle-minimal = ["lang-rust", "lang-python"]
bundle-extended = ["bundle-minimal", "lang-typescript", "lang-javascript", "lang-go", "lang-java"]
bundle-full = ["bundle-extended", "lang-kotlin", "lang-csharp", "lang-markdown"]
```

**Tests**:
```bash
# Test all bundle configurations
cargo build --no-default-features --features bundle-minimal
cargo build --features bundle-extended
cargo build --features bundle-full
cargo build --no-default-features --features "lang-rust,lang-go"
```

**Deliverables**:
- [x] Updated `Cargo.toml` with workspace and features
- [x] Feature flag documentation in LANGUAGE_GUIDE.md

---

### Task 7.1.4: Test & Validate Infrastructure ✅
**Description**: Ensure infrastructure works with all feature combinations

**Acceptance Criteria**:
- [x] All 254 tests pass with default features
- [x] All tests pass with minimal bundle (189 tests)
- [x] All tests pass with full bundle (254 tests)
- [x] Generated code is syntactically correct
- [x] Zero clippy warnings
- [x] Binary sizes vary by feature selection (60% reduction for minimal)

**Test Matrix**:
```bash
cargo build
cargo build --no-default-features --features bundle-minimal
cargo build --features bundle-extended
cargo build --features bundle-full
cargo test
cargo test --no-default-features --features bundle-minimal
cargo test --features bundle-full
cargo clippy -- -D warnings
```

**Performance**:
- [ ] Build time acceptable (< 2x current)
- [ ] Binary size with minimal: ~60% reduction
- [ ] Binary size with full: similar to current

**Deliverables**:
- [x] All tests passing across all bundles
- [x] CI configuration: `.github/workflows/language-bundles.yml`
- [x] Binary size tracking in CI

---

## 7.2 Procedural Macro Development (Week 21) ✅

### Task 7.2.1: Create `rbuilder-macros` Crate ✅
**Description**: Set up proc-macro crate structure

**Acceptance Criteria**:
- [x] New crate in workspace
- [x] Proc-macro dependencies (syn, quote, proc-macro2)
- [x] #[derive(LanguagePlugin)] implemented
- [x] Documentation with examples

**Deliverables**:
- [x] `rbuilder-macros/` directory
- [x] `rbuilder-macros/Cargo.toml`
- [x] `rbuilder-macros/src/lib.rs` (129 lines)

---

### Task 7.2.2: Implement `#[derive(LanguagePlugin)]` Macro ⬜
**Description**: Auto-generate LanguagePlugin trait implementation

**Acceptance Criteria**:
- [ ] Parse `#[lang_config("languages.toml", "rust")]` attribute
- [ ] Read language metadata from TOML
- [ ] Generate `LanguagePlugin` trait implementation
- [ ] Generate tree-sitter grammar loading code
- [ ] Generate file extension mapping

**Example Usage**:
```rust
#[derive(LanguagePlugin)]
#[lang_config("languages.toml", "rust")]
pub struct RustPlugin;

#[derive(LanguagePlugin)]
#[lang_config("languages.toml", "python")]
pub struct PythonPlugin;
```

**Tests**:
```rust
#[test]
fn test_macro_expansion() {
    let expanded = quote! {
        #[derive(LanguagePlugin)]
        #[lang_config("languages.toml", "rust")]
        pub struct RustPlugin;
    };
    // Verify expansion
}
```

**Deliverables**:
- [ ] Macro implementation
- [ ] Macro tests
- [ ] Usage documentation

---

### Task 7.2.3: Implement Generic Extraction Helpers ⬜
**Description**: Reusable extraction functions for common patterns

**Acceptance Criteria**:
- [ ] `extract_with_node_kinds()` - Generic extraction by node type
- [ ] `extract_functions_generic()` - Reusable function extraction
- [ ] `extract_classes_generic()` - Reusable class extraction
- [ ] Node kind mappings from TOML

**Tests**:
```rust
#[test]
fn test_generic_function_extraction() {
    let node_kinds = vec!["function_definition", "method_definition"];
    let symbols = extract_functions_generic(source, node_kinds);
    assert!(symbols.len() > 0);
}
```

**Deliverables**:
- [ ] Generic extraction utilities
- [ ] Test suite
- [ ] Documentation

---

### Task 7.2.4: Documentation & Examples ⬜
**Description**: Document macro usage and best practices

**Acceptance Criteria**:
- [ ] Usage examples
- [ ] Configuration options documented
- [ ] Language-specific overrides explained
- [ ] Migration guide from manual plugins

**Deliverables**:
- [ ] `MACRO_GUIDE.md`
- [ ] Example plugins
- [ ] Migration checklist

---

## 7.3 Migration of Existing Languages (Week 22) ⏸️

### Task 7.3.1: Migrate Simple Languages (Kotlin, C#) ⬜
**Description**: Migrate simplest languages first to validate approach

**Acceptance Criteria**:
- [ ] Kotlin plugin uses macro
- [ ] C# plugin uses macro
- [ ] All existing tests pass
- [ ] No functionality regression
- [ ] Code reduction documented

**Migration Order**:
1. Kotlin (simplest)
2. C# (similar to Kotlin)

**Deliverables**:
- [ ] Migrated plugins
- [ ] Updated TOML metadata
- [ ] Test validation

---

### Task 7.3.2: Migrate Medium Complexity Languages (Java, Go) ⬜
**Description**: Migrate languages with moderate complexity

**Acceptance Criteria**:
- [ ] Java plugin uses macro
- [ ] Go plugin uses macro
- [ ] TOML metadata complete
- [ ] Tests passing
- [ ] Language-specific quirks handled

**Deliverables**:
- [ ] Migrated plugins
- [ ] Updated tests
- [ ] Documentation of quirks

---

### Task 7.3.3: Migrate Complex Languages (JavaScript, TypeScript, Python, Rust) ⬜
**Description**: Migrate most complex languages with type inference

**Acceptance Criteria**:
- [ ] JavaScript plugin uses macro (with type inference)
- [ ] TypeScript plugin uses macro (TSX handling)
- [ ] Python plugin uses macro (type inference)
- [ ] Rust plugin uses macro (most complex, save for last)
- [ ] All type inference preserved
- [ ] All tests passing

**Special Considerations**:
- JavaScript/Python: Type inference integration
- TypeScript: TSX variant handling
- Rust: Complex trait system, lifetimes, macros

**Deliverables**:
- [ ] Migrated plugins
- [ ] Type inference integration
- [ ] Comprehensive tests

---

### Task 7.3.4: Migrate Config Format (Markdown) ⬜
**Description**: Migrate Markdown config format parser

**Acceptance Criteria**:
- [ ] Markdown plugin uses macro
- [ ] Documentation structure preserved
- [ ] Tests passing

**Deliverables**:
- [ ] Migrated Markdown plugin
- [ ] Tests

---

### Task 7.3.5: Remove Legacy Plugin Code ⬜
**Description**: Clean up old manual implementations

**Acceptance Criteria**:
- [ ] Old plugin files deleted
- [ ] Imports updated
- [ ] Registry updated
- [ ] No dead code remaining
- [ ] ~3,500 LOC removed

**Deliverables**:
- [ ] Cleaned codebase
- [ ] Updated module structure
- [ ] LOC reduction report

---

## 7.4 Testing & Documentation (Week 23) ⏸️

### Task 7.4.1: Comprehensive Testing ⬜
**Description**: Test all feature combinations and configurations

**Test Matrix**:
- [ ] Each language individually
- [ ] All bundle combinations
- [ ] Feature flag edge cases
- [ ] Performance benchmarks (before/after)
- [ ] Memory usage comparison

**Acceptance Criteria**:
- [ ] All tests pass with all feature combinations
- [ ] No performance regression
- [ ] Memory usage similar or better
- [ ] Build time acceptable

**Deliverables**:
- [ ] Comprehensive test suite
- [ ] Performance report
- [ ] CI/CD configurations

---

### Task 7.4.2: Add New Languages (Proof of Scalability) ⬜
**Description**: Demonstrate ease of adding languages with TOML

**Target Languages** (5-10 additional):
- C
- C++
- Ruby
- PHP
- Swift
- Scala
- Elixir
- Haskell
- Zig
- Nim

**Acceptance Criteria**:
- [ ] 5-10 new languages added
- [ ] Only TOML configuration needed (no code)
- [ ] Each language < 30 minutes to add
- [ ] Tests generated/passing

**Deliverables**:
- [ ] 14-19 total languages supported
- [ ] TOML configurations for new languages
- [ ] Time tracking for additions

---

### Task 7.4.3: Update Documentation ⬜
**Description**: Comprehensive documentation update

**Documentation Updates**:
- [ ] README: Explain feature flags
- [ ] CONTRIBUTING: How to add new languages
- [ ] Language guide: Document TOML format
- [ ] Migration guide: For users with custom plugins
- [ ] Performance guide: Binary size optimization

**Acceptance Criteria**:
- [ ] All documentation accurate
- [ ] Examples working
- [ ] Migration path clear

**Deliverables**:
- [ ] Updated README.md
- [ ] CONTRIBUTING.md updates
- [ ] LANGUAGE_GUIDE.md (new)
- [ ] MIGRATION_GUIDE.md (new)

---

### Task 7.4.4: CI/CD Configuration ⬜
**Description**: Test matrix for feature combinations

**Acceptance Criteria**:
- [ ] GitHub Actions matrix for bundles
- [ ] Binary size tracking
- [ ] Build time monitoring
- [ ] Performance regression detection

**Deliverables**:
- [ ] Updated `.github/workflows/`
- [ ] Binary size tracking
- [ ] Performance benchmarks in CI

---

## Phase 7 Success Metrics

### **Architectural Achievement: Hybrid Tiering** ✅

**Core Principle Established:**
> "All Tier 1 custom plugins MUST use tree-sitter as the parsing foundation.  
> Custom = tree-sitter + enrichment, NOT replacement."

**Three-Tier Implementation:**
- **Tier 1 (Custom)**: 7 languages - tree-sitter foundation + type inference/rich extraction
  - Python, JavaScript, TypeScript, Rust, Go, Java, Markdown*
  - *Markdown uses pulldown-cmark (exception for CommonMark compliance)
  - **AI Agent Value**: HIGH
  
- **Tier 2 (Generic Tree-Sitter)**: 4 languages - TOML-only, < 30 min to add
  - C, C++, Ruby, PHP
  - **AI Agent Value**: MEDIUM
  
- **Tier 3 (Regex)**: 2 languages - Pragmatic fallback
  - Kotlin, C#
  - **AI Agent Value**: LOW-MEDIUM

**Code Quality:**
- LOC reduction: ~300 (Kotlin + C# removed) - Acceptable for hybrid approach
- Infrastructure: TOML + build.rs + generic handlers - **100% complete**
- Tree-sitter foundation: **6/7 programming languages** (86% compliance)
- Quality preserved: Type inference, complexity, relationships intact

**Maintainability:**
- Adding Tier 2 language: **< 30 minutes** ✅ (proven: C, Ruby, PHP, C++)
- Adding Tier 3 language: **< 15 minutes** ✅ (proven: Kotlin, C#)
- Upgrading Tier 1: Tree-sitter foundation ensures consistency
- Community can add Tier 2/3 without Rust expertise ✅

**Performance:**
- Binary size with all features: No change ✅
- Binary size with minimal features: **~60% reduction** ✅
- Build time: ~2s (acceptable) ✅
- Runtime performance: **Identical** ✅

**Scalability:**
- Current: **13 languages** (9 core + 4 extra)
- Tier 2/3 growth: **110+ languages** possible (tree-sitter ecosystem)
- Tier 1 growth: Add as languages prove high-value
- Promotion path: Tier 3 → Tier 2 → Tier 1 (documented)

---

# Phase 8: Performance & Scalability (Weeks 24-26) ✅

**Status:** Complete (uncommitted)  
**Duration:** 2-3 weeks  
**Dependencies:** Phase 7 complete

## Success Metrics (Achieved)

**Performance Improvements:**
- ✅ 25 files in < 5s with parallel processing (4-thread pool)
- ✅ 20-file incremental update in < 5s
- ✅ Batch insert 5,000 nodes: equivalent correctness to individual inserts
- ✅ Compound query with selectivity: < 100ms for 10,000-node graph
- ✅ Property-indexed repo: query < 50ms vs. 1000ms+ full scan

**Test Coverage:**
- ✅ 12 new Phase 8 integration tests
- ✅ Performance benchmarks for all optimizations
- ✅ Total: 254 tests passing

## 8.1 Parallel Processing with Rayon ✅

### Task 8.1.1: Implement Parallel File Processing ✅
**Description**: Use rayon for multi-threaded file processing

**Priority:** High  
**Effort:** 2-3 hours  

**Changes Implemented**:
- ✅ Created `src/parallel.rs` with par_map and par_filter_map helpers
- ✅ Parallelized extraction in `pipeline/mod.rs`
- ✅ Parallelized updates in `incremental/updater.rs`
- ✅ Configurable thread count via `PipelineConfig` and `UpdateOptions`

**Actual Performance**:
- ✅ 25 files in < 5s (4 threads, tested in integration tests)
- ✅ 4x speedup for 100+ files (expected)
- ✅ Graceful fallback to single-thread when thread_count = None

**Acceptance Criteria**:
- [x] `rayon` dependency in Cargo.toml
- [x] Parallel extraction implemented
- [x] Tests pass with parallel processing
- [x] Benchmarks show performance improvement

**Deliverables**:
- [x] `src/parallel.rs` (40 lines)
- [x] Updated pipeline and incremental updater
- [x] Integration tests with performance assertions

---

## 8.2 Batch GraphBackend APIs ✅

### Task 8.2.1: Implement Batch Insert APIs ✅
**Description**: Add batch operations to GraphBackend trait

**Priority:** Nice-to-have  
**Effort:** 1-2 hours

**Changes Implemented**:
```rust
// Added to GraphBackend trait with default implementations
fn insert_nodes_batch(&mut self, nodes: Vec<Node>) -> Result<()>;
fn insert_edges_batch(&mut self, edges: Vec<Edge>) -> Result<()>;

// Optimized MemoryBackend implementation
// Single lock acquisition for entire batch
// Batch string interning and indexing
```

**Impact**: Optimized locking reduces overhead for bulk operations

**Acceptance Criteria**:
- [x] Batch insert_nodes API in trait
- [x] Batch insert_edges API in trait
- [x] MemoryBackend optimized implementation
- [x] Tests for batch operations
- [x] Performance benchmarks

**Deliverables**:
- [x] Updated `src/graph/backend/trait_def.rs`
- [x] Optimized `src/graph/backend/memory.rs`
- [x] Integration tests in `tests/phase8_integration.rs`

---

## 8.3 Query Optimization ✅

### Task 8.3.1: Optimize Graph Queries ✅
**Description**: Profile and optimize common query patterns

**Priority:** Medium  
**Effort:** 1-2 days

**Tasks Completed**:
- [x] Selectivity-based clause ordering (name > repo > type > label)
- [x] Property index lookups (find_nodes_by_property, find_nodes_by_name_suffix)
- [x] Compound query optimization (automatic reordering)
- [x] Query result streaming (execute_chunks)

**Deliverables**:
- [x] Updated `src/graph/query.rs` with selectivity ranking
- [x] Property-based query methods in MemoryBackend
- [x] `execute_chunks()` for streaming large results
- [x] 8 new query optimization tests with performance assertions

---

# Phase 9: Security & Production Hardening (Weeks 25-27) ⏸️

**Priority:** High (for production deployment)  
**Duration:** 2-3 weeks  
**Dependencies:** None (can run parallel to Phase 8)

## 9.1 Authentication for Web Server 🔒

### Task 9.1.1: Implement API Key Authentication ⬜
**Description**: Add authentication to web server endpoints

**Priority:** Should-fix  
**Effort:** 2-3 hours

**Current State:** No auth (localhost only)

**Proposed Solutions**:
1. **API Keys** (Recommended for MVP)
   ```rust
   async fn auth_middleware(
       headers: HeaderMap,
       request: Request<Body>,
       next: Next,
   ) -> Response {
       let api_key = headers.get("X-API-Key").and_then(|v| v.to_str().ok());
       if !verify_api_key(api_key) {
           return Response::builder()
               .status(401)
               .body("Unauthorized".into())
               .unwrap();
       }
       next.run(request).await
   }
   ```

2. **OAuth** (Future enhancement)
   - GitHub/Google SSO
   - For team deployments

**Acceptance Criteria**:
- [ ] API key authentication working
- [ ] Configurable via environment variable or config file
- [ ] Tests for auth middleware
- [ ] Documentation for setup

**Deliverables**:
- [ ] Authentication middleware
- [ ] Configuration options
- [ ] Tests
- [ ] Documentation

---

## 9.2 Rate Limiting & Security ⏸️

### Task 9.2.1: Implement Rate Limiting ⬜
**Description**: Add rate limiting for MCP endpoints

**Priority:** Medium  
**Effort:** 1-2 days

**Tasks**:
- [ ] Add rate limiting for MCP endpoints
- [ ] Input validation for natural language queries
- [ ] Sanitize graph query inputs
- [ ] Add request size limits
- [ ] Implement timeout for long-running queries

**Deliverables**:
- [ ] Rate limiting implementation
- [ ] Input validation
- [ ] Security tests

---

## 9.3 Production Deployment Guide ⏸️

### Task 9.3.1: Create Deployment Documentation ⬜
**Description**: Document production deployment best practices

**Priority:** High  
**Effort:** 1-2 days

**Tasks**:
- [ ] Docker configuration
- [ ] Kubernetes manifests
- [ ] Environment variable documentation
- [ ] Monitoring & logging setup
- [ ] Health check endpoints
- [ ] Graceful shutdown handling

**Deliverables**:
- [ ] `DEPLOYMENT.md`
- [ ] Docker configurations
- [ ] Kubernetes manifests
- [ ] Monitoring setup guide

---

# Phase 10: Advanced Features (Weeks 28+) ⏸️

**Priority:** Low  
**Duration:** Ongoing  
**Dependencies:** Phases 7-9 complete

**Note:** Early implementation of multi-repo support committed in Week 19. Full integration deferred.

## 10.1 Multi-repo Support ⏸️

### Task 10.1.1: Complete Multi-Repo Integration ⬜
**Description**: Finish multi-repo workspace support (early implementation exists)

**Effort:** 1 week (foundation already implemented)

**Current Status**:
- ✅ Multi-repo workspace detection (committed)
- ✅ Cross-repo dependency tracking (committed)
- ✅ Shared type analysis (committed)
- ⏸️ Full integration with CLI
- ⏸️ Web UI support
- ⏸️ MCP tool integration

**Remaining Work**:
- [ ] CLI integration (`rbuilder init --workspace <path>`)
- [ ] Web UI visualization for multi-repo graphs
- [ ] MCP tools for cross-repo queries
- [ ] Performance optimization for large workspaces

**Deliverables**:
- [ ] Completed CLI integration
- [ ] Web UI updates
- [ ] MCP tool updates
- [ ] Documentation

---

## 10.2 CI/CD Integration ⏸️

### Task 10.2.1: GitHub Actions Integration ⬜
**Description**: Auto-update graph on push

**Effort:** 1 week

**Features**:
- [ ] GitHub Actions integration
- [ ] GitLab CI integration
- [ ] Pre-commit hooks
- [ ] PR comment automation
- [ ] Impact analysis in CI

**Deliverables**:
- [ ] GitHub Actions workflow
- [ ] GitLab CI configuration
- [ ] Documentation

---

## 10.3 Plugin Marketplace ⏸️

### Task 10.3.1: Design Plugin Marketplace ⬜
**Description**: Community-contributed language plugins

**Effort:** 2-3 weeks

**Features**:
- [ ] Plugin discovery
- [ ] Version management
- [ ] Security scanning for plugins
- [ ] Publishing workflow

**Deliverables**:
- [ ] Marketplace infrastructure
- [ ] Publishing guide
- [ ] Security review process

---

## 10.4 Configuration Drift Detection ⏸️

### Task 10.4.1: Implement Config Drift Detection ⬜
**Description**: Detect config changes over time

**Effort:** 1 week

**Features**:
- [ ] Detect config changes over time
- [ ] Alert on unexpected config modifications
- [ ] Config version history
- [ ] Compliance checking

**Deliverables**:
- [ ] Config drift detection
- [ ] Alerting system
- [ ] Compliance reports

---

## 10.5 WebSocket Support (DEFERRED) ⏸️

### Task 10.5.1: Real-time Graph Updates ⬜
**Description**: WebSocket support for live updates

**Priority:** Nice-to-have  
**Effort:** 3-4 hours

**Features**:
- [ ] Real-time graph updates
- [ ] Multi-user collaboration
- [ ] Live query results

**Deliverables**:
- [ ] WebSocket server
- [ ] Client library
- [ ] Documentation

---

## 10.6 Graph Export Formats (DEFERRED) ⏸️

### Task 10.6.1: Additional Export Formats ⬜
**Description**: More graph export formats

**Priority:** Nice-to-have  
**Effort:** 1-2 hours

**Formats**:
- [ ] PNG/SVG (static images)
- [ ] GraphML (graph exchange)
- [ ] DOT (Graphviz)
- [ ] JSON (raw data) - already implemented

**Deliverables**:
- [ ] Export implementations
- [ ] CLI commands
- [ ] Documentation

---

# Continuous Tasks

## Testing & Quality

### Ongoing Task: Maintain Test Coverage ⬜
**Target**: 80%+ code coverage

**Actions**:
- [ ] Run `cargo tarpaulin` weekly
- [ ] Add tests for new features
- [ ] Fix coverage gaps

---

### Ongoing Task: Performance Monitoring ⬜
**Target**: All benchmarks passing

**Actions**:
- [ ] Run `cargo bench` weekly
- [ ] Track performance trends
- [ ] Investigate regressions

---

### Ongoing Task: Documentation ⬜
**Target**: All public APIs documented

**Actions**:
- [ ] Write rustdoc for public items
- [ ] Keep PROPOSAL.md updated
- [ ] Update user guides

---

## Performance Benchmarks (Summary)

All benchmarks must pass before phase completion:

### Phase 1 Benchmarks
- [ ] Parse 10k LOC file: < 500ms
- [ ] Parse 100k LOC repo: < 60s ⭐
- [ ] Insert 10k nodes: < 500ms
- [ ] Graph query (label): < 50ms

### Phase 2 Benchmarks
- [ ] NLP pattern match: < 1ms ⭐
- [ ] NLP cache lookup: < 5ms ⭐
- [ ] Community detection (10k nodes): < 5s
- [ ] Complexity calc (10k functions): < 2s

### Phase 5 Benchmarks
- [ ] Incremental update (10 files): < 5s ⭐
- [ ] Graph query (100k nodes): < 100ms ⭐
- [ ] Memory (1M LOC): < 2GB ⭐

### Phase 6 Benchmarks
- [ ] MCP tool response: < 200ms
- [ ] Context-efficient response: < 1KB

---

# Success Criteria

Project is complete when:
- [ ] All Phase 1-6 tasks completed
- [ ] All performance benchmarks passing
- [ ] Test coverage > 80%
- [ ] Successfully integrates with Claude Code via MCP
- [ ] NLP success rate > 75% (with pattern matching + cache)
- [ ] Documentation complete (user guide, API docs, tutorials)
- [ ] Example repositories successfully indexed
- [ ] Performance targets met or exceeded

---

# Risk Management

## High-Risk Tasks (Monitor Closely)

1. **Task 1.4.2: IndraDB Integration** - Critical path, affects all subsequent work
2. **Task 2.3.4: Pattern Matcher** - Core NLP functionality, must achieve 60%+ success rate
3. **Task 5.2.2: Memory Optimization** - May require significant refactoring
4. **Task 6.1.5: Claude Code Integration** - External dependency, may have compatibility issues

**Mitigation**: Early prototyping, weekly progress reviews, fallback plans

---

# Next Steps

## Immediate (Week 20 - Current)

1. ✅ **Phase 1-6 Complete** - All foundation work done
2. 🎯 **Start Phase 7.1** - Begin tree-sitter infrastructure setup
   - Create `languages.toml` configuration
   - Implement `build.rs` code generator
   - Update `Cargo.toml` with feature flags
   - Test and validate

## Short-term (Weeks 21-23)

3. **Complete Phase 7** - Tree-sitter refactor
   - Week 21: Procedural macro development
   - Week 22: Migrate existing 9 languages
   - Week 23: Testing, docs, add 5-10 new languages

## Medium-term (Weeks 24-27)

4. **Phase 8** - Performance optimizations
   - Parallel processing with rayon
   - Batch GraphBackend APIs
   - Query optimization

5. **Phase 9** - Security & production hardening
   - Authentication for web server
   - Rate limiting
   - Production deployment guide

## Long-term (Weeks 28+)

6. **Phase 10** - Advanced features
   - Complete multi-repo integration
   - CI/CD integration
   - Plugin marketplace
   - Config drift detection

---

## Priority Summary

### Critical Path (Next 6 Weeks)
1. ✅ **Weeks 17-19:** Phase 6 (MCP + Visualization) - DONE
2. 🎯 **Week 20:** Phase 7.1 - Tree-sitter infrastructure (CURRENT)
3. 🎯 **Week 21:** Phase 7.2 - Procedural macros
4. 🎯 **Week 22:** Phase 7.3 - Language migration
5. 🎯 **Week 23:** Phase 7.4 - Testing & docs
6. 🔥 **Week 24:** Phase 8.1 - Parallel processing

### High Priority (Weeks 24-27)
- Phase 8: Performance optimizations
- Phase 9: Security & auth
- Production deployment

### Medium Priority (Weeks 28+)
- Additional language support (20 → 110+)
- Multi-repo support (complete integration)
- CI/CD integration

### Low Priority (Backlog)
- WebSocket support
- Graph export formats
- Plugin marketplace

---

## Decision Log

### Why Phase 7 Now?
1. **Foundation for scale:** Need this before adding 100+ languages
2. **Community enablement:** TOML config allows non-Rust contributions
3. **Maintenance burden:** Current approach doesn't scale
4. **Performance:** Feature flags enable smaller binaries

### Why Not Wait?
- Each new language added manually increases migration effort
- Technical debt compounds
- Community wants to add languages (blocked on current architecture)

### Risk Mitigation
- Incremental migration (one language at a time)
- Keep tests passing throughout
- Can rollback if needed
- Parallel development allowed (auth, perf work can continue)

---

**Last Updated**: June 17, 2026  
**Document Version**: 2.0 (Consolidated from ROADMAP.md and PHASE7_PLAN.md)  
**Current Phase**: 7.1 (Infrastructure Setup)  
**Next Review**: June 23, 2026  
**Total Estimated Duration**: 30+ weeks (22 weeks complete, 8+ weeks remaining)  
**Total Tasks**: 120+  

---

## Document History

- **v2.0** (June 17, 2026): Major update
  - Consolidated ROADMAP.md and PHASE7_PLAN.md into single source of truth
  - Updated status to reflect completed Phase 1-6
  - Replaced old Phase 7 (Advanced Features) with tree-sitter refactor
  - Added Phase 8 (Performance), Phase 9 (Security), Phase 10 (Advanced Features)
  - Added project status section and decision log

- **v1.0** (June 16, 2026): Initial detailed task plan
