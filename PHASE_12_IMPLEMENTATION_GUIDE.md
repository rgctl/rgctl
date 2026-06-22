# Phase 12 Implementation Guide for Cursor

**Phase**: Advanced Query System (Weeks 31-34)  
**Goal**: Add CFG/PDG analysis, dual-agent query system, and graph query language  
**Research Foundation**: Codebadger (2026), CodexGraph (NAACL 2025)  
**Date**: June 17, 2026

---

## Executive Summary for Cursor

This guide provides step-by-step instructions for implementing Phase 12's advanced query capabilities. The phase is organized into 5 major sections with **strict dependency ordering** to ensure each component builds on previous work.

**Critical Constraints**:
- ✅ **Rust-native only** - No Redis, Neo4j, or external databases
- ✅ **In-memory or file-based** - All caching/storage must be local
- ✅ **Incremental delivery** - Each section should be independently testable
- ✅ **Backward compatible** - Existing APIs must continue working

**Total Estimated Effort**: 24-28 weeks serial, 12-16 weeks parallel (with 3-4 developers)

---

## Implementation Order (CRITICAL - Follow This Sequence)

```
12.0 Graph Schema Enrichment (Foundation)
  └── Enables precise filtering and change detection
      │
      ▼
12.1 Control & Data Flow Analysis (Core Semantic Analysis)
  └── CFG → PDG → Backward Slicing
      │
      ▼
12.2 Blast Radius Analysis (Enhanced with Data Flow)
  └── Uses PDG from 12.1 for precise impact analysis
      │
      ▼
12.3 Semantic Search / NLP Enhancement
  └── Pattern matching → Embeddings → Dual-Agent
      │
      ▼
12.4 Graph Query Language (Advanced Queries)
  └── Parser → Executor → Optimizer
      │
      ▼
12.5 Advanced Query Features (Polish)
  └── Macros, Explain Plan, Visualization
```

**Why This Order Matters**:
1. **12.0 first**: Schema changes affect all downstream code
2. **12.1 before 12.2**: Blast radius needs PDG for data flow analysis
3. **12.3 parallel with 12.4**: Can be developed independently
4. **12.5 last**: Builds on all previous sections

---

# Section 12.0: Graph Schema Enrichment (1-2 weeks)

## Overview
Enhance the graph schema to store richer metadata needed for advanced analysis.

## Task 12.0.1: Add Function Signatures (3-4 days)

### Current State
```rust
// src/graph/schema.rs
pub struct Node {
    pub id: Uuid,
    pub node_type: NodeType,
    pub name: String,
    pub qualified_name: Option<String>,
    pub file_path: Option<String>,
    pub properties: HashMap<String, String>,  // Signature is buried here
    // ...
}
```

### Target State
```rust
// src/graph/schema.rs
pub struct Node {
    pub id: Uuid,
    pub node_type: NodeType,
    pub name: String,
    pub qualified_name: Option<String>,
    
    // NEW: First-class signature fields
    pub signature: Option<String>,           // Full signature
    pub return_type: Option<String>,         // Extracted return type
    pub parameters: Vec<Parameter>,          // Structured parameters
    
    pub file_path: Option<String>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    
    // NEW: Code hash for change detection
    pub code_hash: Option<String>,           // SHA-256 of function body
    
    pub properties: HashMap<String, String>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: Option<String>,
    pub default_value: Option<String>,
}
```

### Implementation Steps

1. **Update Schema** (`src/graph/schema.rs`)
   - Add new fields to `Node` struct
   - Add `Parameter` struct
   - Update builder methods: `with_signature()`, `with_return_type()`, `with_parameters()`
   - **CRITICAL**: Add migration logic for existing graphs

2. **Update Language Plugins** (all plugins in `src/languages/builtin/*.rs`)
   - Rust: Extract from `function_item` → `signature`, `return_type`, `parameters`
   - Python: Extract from `function_definition` → type hints
   - TypeScript: Extract from `function_declaration` → full type info
   - JavaScript: Extract basic signature (no types)
   - Go: Extract from `function_declaration` → return types
   - Java: Extract from `method_declaration` → full signature

3. **Update GraphBuilder** (`src/extraction/graph_builder.rs`)
   ```rust
   impl GraphBuilder {
       pub fn add_symbol(&mut self, symbol: &Symbol, file_id: Uuid) -> Uuid {
           // OLD: signature goes into properties
           // NEW: populate first-class fields
           let mut node = Node::new(...)
               .with_signature(symbol.signature.clone().unwrap_or_default())
               .with_return_type(symbol.return_type.clone())
               .with_parameters(symbol.parameters.clone());
           
           // ... rest of method
       }
   }
   ```

4. **Update Query System** (`src/graph/query.rs`)
   - Add `signature:*pattern*` filter support
   - Add `return_type:Type` filter support
   - Example: `signature:*async*|return_type:Result*`

5. **Add Tests**
   ```rust
   #[test]
   fn test_signature_extraction_rust() {
       let code = "fn process(data: &[u8], count: usize) -> Result<Vec<String>> {}";
       let symbols = extract_symbols(code).unwrap();
       assert_eq!(symbols[0].signature.unwrap(), 
                  "fn process(data: &[u8], count: usize) -> Result<Vec<String>>");
       assert_eq!(symbols[0].return_type.unwrap(), "Result<Vec<String>>");
       assert_eq!(symbols[0].parameters.len(), 2);
   }
   ```

### Migration Strategy
```rust
// src/graph/migration.rs (NEW FILE)
pub fn migrate_v1_to_v2(graph: &mut CodeGraph) -> Result<()> {
    for node in graph.all_nodes_mut() {
        if node.signature.is_none() {
            // Extract from properties map if available
            if let Some(sig) = node.properties.get("signature") {
                node.signature = Some(sig.clone());
            }
        }
    }
    Ok(())
}
```

---

## Task 12.0.2: Add Code Hashing (2 days)

### Purpose
Enable fast "has this code changed?" checks for incremental updates.

### Implementation

1. **Create Code Index** (`src/graph/code_index.rs` - NEW FILE)
   ```rust
   use blake3::hash;
   use std::collections::HashMap;
   use std::path::PathBuf;
   
   pub struct CodeIndex {
       // SHA-256 hash -> code location
       hash_to_code: HashMap<String, CodeLocation>,
       cache_file: PathBuf,
   }
   
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct CodeLocation {
       pub file_path: String,
       pub start_line: usize,
       pub end_line: usize,
       pub code: String,
   }
   
   impl CodeIndex {
       pub fn new(cache_file: PathBuf) -> Self {
           Self {
               hash_to_code: HashMap::new(),
               cache_file,
           }
       }
       
       pub fn add_code(&mut self, code: &str, location: SourceLocation) -> String {
           let hash = blake3::hash(code.as_bytes()).to_hex().to_string();
           self.hash_to_code.insert(hash.clone(), CodeLocation {
               file_path: location.file,
               start_line: location.start_line,
               end_line: location.end_line,
               code: code.to_string(),
           });
           hash
       }
       
       pub fn has_changed(&self, hash: &str, current_code: &str) -> bool {
           let current_hash = blake3::hash(current_code.as_bytes()).to_hex().to_string();
           current_hash != hash
       }
       
       pub fn get_code(&self, hash: &str) -> Option<&str> {
           self.hash_to_code.get(hash).map(|loc| loc.code.as_str())
       }
       
       pub fn save(&self) -> Result<()> {
           let json = serde_json::to_string_pretty(&self.hash_to_code)?;
           std::fs::write(&self.cache_file, json)?;
           Ok(())
       }
       
       pub fn load(cache_file: PathBuf) -> Result<Self> {
           if cache_file.exists() {
               let json = std::fs::read_to_string(&cache_file)?;
               let hash_to_code = serde_json::from_str(&json)?;
               Ok(Self { hash_to_code, cache_file })
           } else {
               Ok(Self::new(cache_file))
           }
       }
   }
   ```

2. **Integrate with GraphBuilder**
   ```rust
   impl GraphBuilder {
       pub fn add_symbol_with_code(&mut self, symbol: &Symbol, code: &str) -> Uuid {
           let hash = self.code_index.add_code(code, symbol.location.clone());
           let node = Node::new(...)
               .with_code_hash(hash);
           // ...
       }
   }
   ```

3. **Use in Incremental Updates**
   ```rust
   // src/incremental/updater.rs
   impl IncrementalUpdater {
       pub fn should_reparse(&self, node_id: Uuid) -> bool {
           let node = self.graph.get_node(node_id)?;
           let current_code = self.read_code_from_file(node)?;
           
           if let Some(hash) = &node.code_hash {
               self.code_index.has_changed(hash, &current_code)
           } else {
               true  // No hash, assume changed
           }
       }
   }
   ```

---

## Task 12.0.3: Add Edge Properties (2 days)

### Current State
```rust
pub struct Edge {
    pub from: Uuid,
    pub to: Uuid,
    pub edge_type: EdgeType,
    pub properties: HashMap<String, String>,  // Generic properties
    pub weight: f64,
}
```

### Target State
```rust
pub struct Edge {
    pub from: Uuid,
    pub to: Uuid,
    pub edge_type: EdgeType,
    
    // NEW: Typed edge metadata
    pub call_type: Option<CallType>,      // For Calls edges
    pub access_type: Option<AccessType>,  // For Uses edges
    
    pub properties: HashMap<String, String>,
    pub weight: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CallType {
    Direct,      // foo()
    Indirect,    // fn_ptr()
    Virtual,     // trait/interface method
    Macro,       // macro invocation
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AccessType {
    Read,        // Reading variable
    Write,       // Writing variable
    ReadWrite,   // Both
}
```

### Implementation
```rust
impl Edge {
    pub fn with_call_type(mut self, call_type: CallType) -> Self {
        self.call_type = Some(call_type);
        self
    }
    
    pub fn with_access_type(mut self, access: AccessType) -> Self {
        self.access_type = Some(access);
        self
    }
}
```

### Query Support
```rust
// src/graph/query.rs
pub fn execute(backend: &MemoryBackend, query: &str) -> Result<Vec<Node>> {
    // NEW: Support edge property filters
    // Example: "calls:foo|call_type:direct"
    if query.contains("call_type:") {
        // Filter by call type
    }
}
```

---

# Section 12.1: Control & Data Flow Analysis (4-6 weeks)

**CRITICAL**: This is the foundation for all semantic reasoning. Take your time to get it right.

## Task 12.1.1: Implement CFG Construction (3 weeks)

### Overview
Build Control Flow Graph from tree-sitter AST to enable execution path analysis.

### Architecture

1. **Create CFG Module** (`src/analysis/cfg.rs` - NEW FILE)
   ```rust
   use uuid::Uuid;
   use std::collections::{HashMap, HashSet, VecDeque};
   
   pub type BlockId = Uuid;
   
   #[derive(Debug, Clone)]
   pub struct ControlFlowGraph {
       pub blocks: HashMap<BlockId, BasicBlock>,
       pub edges: Vec<CfgEdge>,
       pub entry: BlockId,
       pub exits: Vec<BlockId>,
   }
   
   #[derive(Debug, Clone)]
   pub struct BasicBlock {
       pub id: BlockId,
       pub statements: Vec<Statement>,
       pub start_line: usize,
       pub end_line: usize,
   }
   
   #[derive(Debug, Clone)]
   pub struct Statement {
       pub kind: StatementKind,
       pub line: usize,
       pub text: String,
   }
   
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum StatementKind {
       Expression,
       Assignment,
       Declaration,
       FunctionCall,
       Return,
       Branch,      // if condition
       Jump,        // break/continue/goto
   }
   
   #[derive(Debug, Clone)]
   pub struct CfgEdge {
       pub from: BlockId,
       pub to: BlockId,
       pub edge_type: CfgEdgeType,
   }
   
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum CfgEdgeType {
       Next,        // Sequential flow
       IfTrue,      // Conditional true branch
       IfFalse,     // Conditional false branch
       Jump,        // Goto/break/continue
       Return,      // Function return
       Exception,   // Exception handler
   }
   
   impl ControlFlowGraph {
       pub fn new() -> Self {
           let entry = Uuid::new_v4();
           Self {
               blocks: HashMap::new(),
               edges: Vec::new(),
               entry,
               exits: Vec::new(),
           }
       }
       
       pub fn add_block(&mut self, block: BasicBlock) {
           self.blocks.insert(block.id, block);
       }
       
       pub fn add_edge(&mut self, from: BlockId, to: BlockId, edge_type: CfgEdgeType) {
           self.edges.push(CfgEdge { from, to, edge_type });
       }
       
       pub fn predecessors(&self, block_id: BlockId) -> Vec<BlockId> {
           self.edges
               .iter()
               .filter(|e| e.to == block_id)
               .map(|e| e.from)
               .collect()
       }
       
       pub fn successors(&self, block_id: BlockId) -> Vec<BlockId> {
           self.edges
               .iter()
               .filter(|e| e.from == block_id)
               .map(|e| e.to)
               .collect()
       }
       
       pub fn has_cycle(&self) -> bool {
           // DFS cycle detection
           let mut visited = HashSet::new();
           let mut rec_stack = HashSet::new();
           
           fn dfs(
               cfg: &ControlFlowGraph,
               node: BlockId,
               visited: &mut HashSet<BlockId>,
               rec_stack: &mut HashSet<BlockId>,
           ) -> bool {
               visited.insert(node);
               rec_stack.insert(node);
               
               for succ in cfg.successors(node) {
                   if !visited.contains(&succ) {
                       if dfs(cfg, succ, visited, rec_stack) {
                           return true;
                       }
                   } else if rec_stack.contains(&succ) {
                       return true;  // Cycle detected
                   }
               }
               
               rec_stack.remove(&node);
               false
           }
           
           dfs(self, self.entry, &mut visited, &mut rec_stack)
       }
       
       pub fn find_paths(&self, from: BlockId, to: BlockId) -> Vec<Vec<BlockId>> {
           let mut paths = Vec::new();
           let mut current_path = vec![from];
           let mut visited = HashSet::new();
           
           self.dfs_paths(from, to, &mut current_path, &mut visited, &mut paths);
           paths
       }
       
       fn dfs_paths(
           &self,
           current: BlockId,
           target: BlockId,
           path: &mut Vec<BlockId>,
           visited: &mut HashSet<BlockId>,
           paths: &mut Vec<Vec<BlockId>>,
       ) {
           if current == target {
               paths.push(path.clone());
               return;
           }
           
           visited.insert(current);
           
           for succ in self.successors(current) {
               if !visited.contains(&succ) {
                   path.push(succ);
                   self.dfs_paths(succ, target, path, visited, paths);
                   path.pop();
               }
           }
           
           visited.remove(&current);
       }
   }
   ```

2. **Create CFG Builder** (`src/analysis/cfg_builder.rs` - NEW FILE)
   ```rust
   use crate::analysis::cfg::*;
   use crate::graph::schema::Node;
   use tree_sitter::{Tree, Node as TsNode};
   
   pub struct CfgBuilder<'a> {
       cfg: &'a mut ControlFlowGraph,
       current_block: BlockId,
   }
   
   impl<'a> CfgBuilder<'a> {
       pub fn new(cfg: &'a mut ControlFlowGraph) -> Self {
           let entry = cfg.entry;
           Self {
               cfg,
               current_block: entry,
           }
       }
       
       pub fn build_from_function(
           function_node: &Node,
           tree: &Tree,
           source: &[u8],
       ) -> Result<ControlFlowGraph> {
           let mut cfg = ControlFlowGraph::new();
           let mut builder = CfgBuilder::new(&mut cfg);
           
           // Find function body in tree
           let root = tree.root_node();
           let func_node = builder.find_function_node(&root, function_node)?;
           
           // Visit function body
           builder.visit_block(&func_node, source)?;
           
           Ok(cfg)
       }
       
       fn visit_block(&mut self, node: &TsNode, source: &[u8]) -> Result<BlockId> {
           match node.kind() {
               "block" | "function_body" => {
                   // Process each statement in block
                   for child in node.children(&mut node.walk()) {
                       self.visit_statement(&child, source)?;
                   }
                   Ok(self.current_block)
               }
               _ => Ok(self.current_block)
           }
       }
       
       fn visit_statement(&mut self, node: &TsNode, source: &[u8]) -> Result<()> {
           match node.kind() {
               "if_statement" => self.visit_if(node, source),
               "while_statement" | "for_statement" => self.visit_loop(node, source),
               "return_statement" => self.visit_return(node, source),
               "break_statement" | "continue_statement" => self.visit_jump(node, source),
               _ => self.visit_simple_statement(node, source),
           }
       }
       
       fn visit_if(&mut self, node: &TsNode, source: &[u8]) -> Result<()> {
           // Current block ends with conditional
           let cond_block = self.current_block;
           
           // Create true branch block
           let true_block = self.new_block();
           self.cfg.add_edge(cond_block, true_block, CfgEdgeType::IfTrue);
           
           // Create false branch block
           let false_block = self.new_block();
           self.cfg.add_edge(cond_block, false_block, CfgEdgeType::IfFalse);
           
           // Visit true branch
           self.current_block = true_block;
           if let Some(consequence) = node.child_by_field_name("consequence") {
               self.visit_block(&consequence, source)?;
           }
           let true_end = self.current_block;
           
           // Visit false branch (if exists)
           self.current_block = false_block;
           if let Some(alternative) = node.child_by_field_name("alternative") {
               self.visit_block(&alternative, source)?;
           }
           let false_end = self.current_block;
           
           // Create merge block
           let merge_block = self.new_block();
           self.cfg.add_edge(true_end, merge_block, CfgEdgeType::Next);
           self.cfg.add_edge(false_end, merge_block, CfgEdgeType::Next);
           
           self.current_block = merge_block;
           Ok(())
       }
       
       fn visit_loop(&mut self, node: &TsNode, source: &[u8]) -> Result<()> {
           // Loop header (condition check)
           let header_block = self.new_block();
           self.cfg.add_edge(self.current_block, header_block, CfgEdgeType::Next);
           
           // Loop body
           let body_block = self.new_block();
           self.cfg.add_edge(header_block, body_block, CfgEdgeType::IfTrue);
           
           // Visit body
           self.current_block = body_block;
           if let Some(body) = node.child_by_field_name("body") {
               self.visit_block(&body, source)?;
           }
           
           // Back edge to header
           self.cfg.add_edge(self.current_block, header_block, CfgEdgeType::Jump);
           
           // Exit block
           let exit_block = self.new_block();
           self.cfg.add_edge(header_block, exit_block, CfgEdgeType::IfFalse);
           
           self.current_block = exit_block;
           Ok(())
       }
       
       fn visit_return(&mut self, node: &TsNode, source: &[u8]) -> Result<()> {
           // Add return statement to current block
           let stmt = self.extract_statement(node, source, StatementKind::Return);
           self.add_statement_to_current_block(stmt);
           
           // Add edge to function exit
           let exit_block = Uuid::new_v4();
           self.cfg.add_edge(self.current_block, exit_block, CfgEdgeType::Return);
           self.cfg.exits.push(exit_block);
           
           // Create new unreachable block for subsequent statements
           self.current_block = self.new_block();
           Ok(())
       }
       
       fn new_block(&mut self) -> BlockId {
           let id = Uuid::new_v4();
           self.cfg.blocks.insert(id, BasicBlock {
               id,
               statements: Vec::new(),
               start_line: 0,
               end_line: 0,
           });
           id
       }
   }
   ```

3. **Language-Specific CFG Builders**

   Since tree-sitter AST structure varies by language, create language-specific builders:

   ```rust
   // src/analysis/cfg_rust.rs
   pub struct RustCfgBuilder;
   
   impl RustCfgBuilder {
       pub fn build_from_ast(tree: &Tree, source: &[u8]) -> Result<ControlFlowGraph> {
           // Rust-specific node kinds: "if_expression", "match_expression", "loop_expression"
       }
   }
   
   // src/analysis/cfg_python.rs
   pub struct PythonCfgBuilder;
   
   impl PythonCfgBuilder {
       pub fn build_from_ast(tree: &Tree, source: &[u8]) -> Result<ControlFlowGraph> {
           // Python-specific node kinds: "if_statement", "while_statement", "for_statement"
       }
   }
   ```

4. **Integration with Code Graph**
   ```rust
   // src/analysis/mod.rs
   pub struct CfgCache {
       // node_id -> CFG
       cache: HashMap<Uuid, ControlFlowGraph>,
   }
   
   impl CfgCache {
       pub fn get_or_build(&mut self, node: &Node, source: &[u8]) -> Result<&ControlFlowGraph> {
           if !self.cache.contains_key(&node.id) {
               let cfg = self.build_cfg_for_language(node, source)?;
               self.cache.insert(node.id, cfg);
           }
           Ok(&self.cache[&node.id])
       }
       
       fn build_cfg_for_language(&self, node: &Node, source: &[u8]) -> Result<ControlFlowGraph> {
           match detect_language(node.file_path.as_ref().unwrap()) {
               "rust" => RustCfgBuilder::build_from_ast(tree, source),
               "python" => PythonCfgBuilder::build_from_ast(tree, source),
               // ... other languages
               _ => Err(Error::UnsupportedLanguage)
           }
       }
   }
   ```

5. **Testing Strategy**
   ```rust
   #[test]
   fn test_cfg_if_else() {
       let code = r#"
       fn example(x: i32) -> i32 {
           if x > 0 {
               return x;
           } else {
               return -x;
           }
       }
       "#;
       
       let cfg = build_cfg_from_rust(code).unwrap();
       
       // Should have: entry, condition, true-branch, false-branch, (merge unreachable)
       assert_eq!(cfg.blocks.len(), 4);
       
       // Check edges
       let if_true_edges: Vec<_> = cfg.edges.iter()
           .filter(|e| e.edge_type == CfgEdgeType::IfTrue)
           .collect();
       assert_eq!(if_true_edges.len(), 1);
   }
   
   #[test]
   fn test_cfg_loop_has_cycle() {
       let code = r#"
       fn loop_example(n: i32) -> i32 {
           let mut sum = 0;
           for i in 0..n {
               sum += i;
           }
           sum
       }
       "#;
       
       let cfg = build_cfg_from_rust(code).unwrap();
       assert!(cfg.has_cycle());  // Loop creates back-edge
   }
   ```

### Performance Target
- <100ms to build CFG for 1000 LOC function

---

## Task 12.1.2: Implement PDG Construction (4 weeks)

### Overview
Build Program Dependence Graph to track data and control dependencies.

**This is the most complex task in Phase 12. Allocate sufficient time.**

### Architecture

1. **Create PDG Module** (`src/analysis/pdg.rs` - NEW FILE)
   ```rust
   use crate::analysis::cfg::*;
   use std::collections::{HashMap, HashSet};
   
   pub type PdgNodeId = Uuid;
   
   #[derive(Debug, Clone)]
   pub struct ProgramDependenceGraph {
       pub nodes: HashMap<PdgNodeId, PdgNode>,
       pub data_deps: Vec<DataDependency>,
       pub control_deps: Vec<ControlDependency>,
   }
   
   #[derive(Debug, Clone)]
   pub struct PdgNode {
       pub id: PdgNodeId,
       pub statement: Statement,
       pub defined_vars: HashSet<String>,   // Variables defined
       pub used_vars: HashSet<String>,      // Variables used
   }
   
   #[derive(Debug, Clone)]
   pub struct DataDependency {
       pub from: PdgNodeId,  // Variable definition
       pub to: PdgNodeId,    // Variable use
       pub variable: String,
       pub dep_type: DataDepType,
   }
   
   #[derive(Debug, Clone, Copy)]
   pub enum DataDepType {
       Flow,        // x = ...; ... = x;  (true dependency)
       Anti,        // ... = x; x = ...;  (write after read)
       Output,      // x = ...; x = ...;  (write after write)
   }
   
   #[derive(Debug, Clone)]
   pub struct ControlDependency {
       pub controller: PdgNodeId,  // Statement that controls execution
       pub dependent: PdgNodeId,   // Statement that depends on controller
   }
   
   impl ProgramDependenceGraph {
       pub fn build(cfg: &ControlFlowGraph, source: &[u8]) -> Result<Self> {
           let mut pdg = Self::new();
           
           // Step 1: Create PDG nodes from CFG blocks
           pdg.create_nodes_from_cfg(cfg);
           
           // Step 2: Compute reaching definitions (data flow analysis)
           let reaching_defs = compute_reaching_definitions(cfg, &pdg);
           
           // Step 3: Build def-use chains (data dependencies)
           pdg.build_data_dependencies(&reaching_defs);
           
           // Step 4: Compute control dependencies
           pdg.build_control_dependencies(cfg);
           
           Ok(pdg)
       }
       
       pub fn get_dependencies(&self, var: &str) -> Vec<PdgNodeId> {
           self.data_deps
               .iter()
               .filter(|dep| dep.variable == var)
               .map(|dep| dep.from)
               .collect()
       }
       
       pub fn get_dependents(&self, node_id: PdgNodeId) -> Vec<PdgNodeId> {
           self.data_deps
               .iter()
               .filter(|dep| dep.from == node_id)
               .map(|dep| dep.to)
               .collect()
       }
   }
   ```

2. **Reaching Definitions Algorithm** (`src/analysis/dataflow.rs` - NEW FILE)
   ```rust
   use crate::analysis::cfg::*;
   use crate::analysis::pdg::*;
   use std::collections::{HashMap, HashSet, VecDeque};
   
   pub struct ReachingDefs {
       pub in_set: HashMap<BlockId, HashSet<Definition>>,
       pub out_set: HashMap<BlockId, HashSet<Definition>>,
   }
   
   #[derive(Debug, Clone, PartialEq, Eq, Hash)]
   pub struct Definition {
       pub variable: String,
       pub block: BlockId,
       pub statement_index: usize,
   }
   
   pub fn compute_reaching_definitions(
       cfg: &ControlFlowGraph,
       pdg: &ProgramDependenceGraph,
   ) -> ReachingDefs {
       let mut worklist = cfg.blocks.keys().copied().collect::<VecDeque<_>>();
       let mut gen = HashMap::new();   // Definitions generated in block
       let mut kill = HashMap::new();  // Definitions killed in block
       let mut in_set = HashMap::new();
       let mut out_set = HashMap::new();
       
       // Initialize gen/kill sets for each block
       for (block_id, block) in &cfg.blocks {
           let (g, k) = compute_gen_kill(block, pdg);
           gen.insert(*block_id, g);
           kill.insert(*block_id, k);
           in_set.insert(*block_id, HashSet::new());
           out_set.insert(*block_id, HashSet::new());
       }
       
       // Iterative data flow analysis until fixed point
       while let Some(block_id) = worklist.pop_front() {
           // IN[B] = ∪ (OUT[P] for all predecessors P of B)
           let in_b: HashSet<Definition> = cfg.predecessors(block_id)
               .iter()
               .flat_map(|pred| out_set.get(pred).cloned().unwrap_or_default())
               .collect();
           
           // OUT[B] = GEN[B] ∪ (IN[B] - KILL[B])
           let gen_b = gen.get(&block_id).cloned().unwrap_or_default();
           let kill_b = kill.get(&block_id).cloned().unwrap_or_default();
           
           let out_b: HashSet<Definition> = gen_b
               .iter()
               .cloned()
               .chain(
                   in_b.iter()
                       .filter(|def| !kill_b.contains(def))
                       .cloned()
               )
               .collect();
           
           // If OUT[B] changed, add successors to worklist
           if out_set.get(&block_id) != Some(&out_b) {
               for succ in cfg.successors(block_id) {
                   if !worklist.contains(&succ) {
                       worklist.push_back(succ);
                   }
               }
               out_set.insert(block_id, out_b);
           }
           
           in_set.insert(block_id, in_b);
       }
       
       ReachingDefs { in_set, out_set }
   }
   
   fn compute_gen_kill(
       block: &BasicBlock,
       pdg: &ProgramDependenceGraph,
   ) -> (HashSet<Definition>, HashSet<Definition>) {
       let mut gen = HashSet::new();
       let mut kill = HashSet::new();
       
       for (idx, stmt) in block.statements.iter().enumerate() {
           // Find PDG node for this statement
           let pdg_node = pdg.nodes.values()
               .find(|n| n.statement.line == stmt.line)
               .unwrap();
           
           // For each variable defined in this statement
           for var in &pdg_node.defined_vars {
               // Add to GEN set
               gen.insert(Definition {
                   variable: var.clone(),
                   block: block.id,
                   statement_index: idx,
               });
               
               // Kill all previous definitions of this variable
               kill.extend(
                   gen.iter()
                       .filter(|d| d.variable == *var && d.statement_index != idx)
                       .cloned()
               );
           }
       }
       
       (gen, kill)
   }
   ```

3. **Variable Def-Use Analysis** (`src/analysis/def_use.rs` - NEW FILE)
   ```rust
   // Extract defined and used variables from tree-sitter AST
   
   pub fn extract_def_use(stmt_node: &TsNode, source: &[u8]) -> (HashSet<String>, HashSet<String>) {
       let mut defined = HashSet::new();
       let mut used = HashSet::new();
       
       match stmt_node.kind() {
           "assignment_expression" | "variable_declaration" => {
               // Left side = definition
               if let Some(left) = stmt_node.child_by_field_name("left") {
                   defined.insert(extract_variable_name(&left, source));
               }
               // Right side = use
               if let Some(right) = stmt_node.child_by_field_name("right") {
                   used.extend(extract_variables(&right, source));
               }
           }
           "expression_statement" => {
               // Just uses, no definitions
               used.extend(extract_variables(stmt_node, source));
           }
           _ => {}
       }
       
       (defined, used)
   }
   
   fn extract_variables(node: &TsNode, source: &[u8]) -> HashSet<String> {
       let mut vars = HashSet::new();
       
       fn traverse(node: &TsNode, source: &[u8], vars: &mut HashSet<String>) {
           if node.kind() == "identifier" {
               vars.insert(node.utf8_text(source).unwrap().to_string());
           }
           for child in node.children(&mut node.walk()) {
               traverse(&child, source, vars);
           }
       }
       
       traverse(node, source, &mut vars);
       vars
   }
   ```

4. **Control Dependency Computation**
   ```rust
   impl ProgramDependenceGraph {
       fn build_control_dependencies(&mut self, cfg: &ControlFlowGraph) {
           // Use post-dominator tree algorithm
           let post_dom = compute_post_dominators(cfg);
           
           for edge in &cfg.edges {
               // If edge.to does not post-dominate edge.from,
               // then edge.from controls edge.to
               if !post_dom.dominates(edge.to, edge.from) {
                   // Find all nodes control-dependent on this edge
                   let deps = self.find_control_dependents(edge, cfg, &post_dom);
                   for dep in deps {
                       self.control_deps.push(ControlDependency {
                           controller: self.block_to_pdg_node(edge.from),
                           dependent: dep,
                       });
                   }
               }
           }
       }
   }
   
   // Post-dominator computation (standard graph algorithm)
   fn compute_post_dominators(cfg: &ControlFlowGraph) -> PostDominatorTree {
       // Implementation using iterative dataflow algorithm
       // Similar to reaching definitions but in reverse
       todo!("Implement post-dominator tree computation")
   }
   ```

5. **Testing Strategy**
   ```rust
   #[test]
   fn test_pdg_data_dependency() {
       let code = r#"
       fn example(a: i32) -> i32 {
           let x = a + 1;  // Line 2: defines x, uses a
           let y = x * 2;  // Line 3: defines y, uses x (depends on line 2)
           y
       }
       "#;
       
       let cfg = build_cfg(code).unwrap();
       let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
       
       // Find node for "let y = x * 2"
       let y_node = pdg.nodes.values()
           .find(|n| n.defined_vars.contains("y"))
           .unwrap();
       
       // Should have data dependency on x definition
       let deps = pdg.get_dependencies("x");
       assert_eq!(deps.len(), 1);
       
       // Check dependency type
       let dep = pdg.data_deps.iter()
           .find(|d| d.to == y_node.id && d.variable == "x")
           .unwrap();
       assert_eq!(dep.dep_type, DataDepType::Flow);
   }
   
   #[test]
   fn test_reaching_definitions() {
       let code = r#"
       fn example() {
           let mut x = 1;  // Def 1
           if condition {
               x = 2;      // Def 2
           } else {
               x = 3;      // Def 3
           }
           print(x);       // Uses x (reaches from Def 2 or Def 3, not Def 1)
       }
       "#;
       
       let cfg = build_cfg(code).unwrap();
       let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
       let reaching = compute_reaching_definitions(&cfg, &pdg);
       
       // At print(x), both Def 2 and Def 3 reach, but not Def 1
       let print_block = cfg.blocks.values()
           .find(|b| b.statements.iter().any(|s| s.text.contains("print")))
           .unwrap();
       
       let reaching_defs = &reaching.in_set[&print_block.id];
       assert_eq!(reaching_defs.iter().filter(|d| d.variable == "x").count(), 2);
   }
   ```

### Performance Target
- <500ms to build PDG for 5000 LOC file

---

## Task 12.1.3: Implement Backward Slicing (2 weeks)

### Overview
Given a criterion (variable, line), compute minimal upstream code slice.

### Implementation

1. **Create Slicing Module** (`src/analysis/slicing.rs` - NEW FILE)
   ```rust
   use crate::analysis::pdg::*;
   use crate::analysis::cfg::*;
   
   #[derive(Debug, Clone)]
   pub struct SliceCriterion {
       pub variable: String,
       pub line: usize,
   }
   
   #[derive(Debug, Clone)]
   pub struct CodeSlice {
       pub criterion: SliceCriterion,
       pub statements: HashSet<PdgNodeId>,
       pub lines: HashSet<usize>,
       pub reduction_percent: f64,
   }
   
   pub struct BackwardSlicer<'a> {
       pdg: &'a ProgramDependenceGraph,
       cfg: &'a ControlFlowGraph,
   }
   
   impl<'a> BackwardSlicer<'a> {
       pub fn new(pdg: &'a ProgramDependenceGraph, cfg: &'a ControlFlowGraph) -> Self {
           Self { pdg, cfg }
       }
       
       pub fn slice(&self, criterion: SliceCriterion) -> CodeSlice {
           let mut slice = HashSet::new();
           let mut worklist = VecDeque::new();
           
           // Find PDG node for criterion
           let criterion_node = self.find_criterion_node(&criterion);
           worklist.push_back(criterion_node);
           
           while let Some(node_id) = worklist.pop_front() {
               if !slice.insert(node_id) {
                   continue;  // Already visited
               }
               
               // 1. Add data dependencies (backward edges in PDG)
               for dep in self.pdg.data_deps.iter().filter(|d| d.to == node_id) {
                   worklist.push_back(dep.from);
               }
               
               // 2. Add control dependencies
               for ctrl_dep in self.pdg.control_deps.iter().filter(|c| c.dependent == node_id) {
                   worklist.push_back(ctrl_dep.controller);
               }
               
               // 3. For function calls, include argument definitions
               if let Some(call_node) = self.get_call_node(node_id) {
                   for arg_def in self.get_argument_defs(&call_node) {
                       worklist.push_back(arg_def);
                   }
               }
           }
           
           // Collect line numbers
           let lines: HashSet<usize> = slice.iter()
               .map(|id| self.pdg.nodes[id].statement.line)
               .collect();
           
           // Calculate reduction
           let total_lines = self.count_total_lines();
           let reduction_percent = 100.0 * (1.0 - (lines.len() as f64 / total_lines as f64));
           
           CodeSlice {
               criterion,
               statements: slice,
               lines,
               reduction_percent,
           }
       }
       
       fn find_criterion_node(&self, criterion: &SliceCriterion) -> PdgNodeId {
           self.pdg.nodes.values()
               .find(|n| {
                   n.statement.line == criterion.line &&
                   (n.defined_vars.contains(&criterion.variable) ||
                    n.used_vars.contains(&criterion.variable))
               })
               .map(|n| n.id)
               .unwrap()
       }
   }
   ```

2. **MCP Tool Integration**
   ```rust
   // src/mcp/tools.rs
   impl ToolExecutor {
       pub fn execute_backward_slice(&self, input: Value) -> Result<Value> {
           let file = input["file"].as_str().unwrap();
           let line = input["line"].as_u64().unwrap() as usize;
           let variable = input["variable"].as_str().unwrap();
           
           let graph = self.load_graph()?;
           let node = graph.find_node_at(file, line)?;
           
           // Build CFG and PDG
           let source = std::fs::read(file)?;
           let cfg = CfgCache::get_or_build(&node, &source)?;
           let pdg = ProgramDependenceGraph::build(&cfg, &source)?;
           
           // Compute slice
           let slicer = BackwardSlicer::new(&pdg, &cfg);
           let slice = slicer.slice(SliceCriterion { variable: variable.to_string(), line });
           
           Ok(json!({
               "criterion": {
                   "file": file,
                   "line": line,
                   "variable": variable
               },
               "slice_lines": slice.lines.iter().sorted().collect::<Vec<_>>(),
               "total_lines": slice.lines.len(),
               "reduction_percent": slice.reduction_percent,
               "statements": slice.statements.len()
           }))
       }
   }
   ```

3. **CLI Integration**
   ```rust
   // src/cli/slice.rs
   #[derive(Parser)]
   pub struct SliceCommand {
       /// File path
       file: PathBuf,
       
       /// Line number
       #[arg(long)]
       line: usize,
       
       /// Variable name
       #[arg(long)]
       variable: String,
   }
   
   impl SliceCommand {
       pub fn execute(&self) -> Result<()> {
           let slicer = BackwardSlicer::new(...);
           let slice = slicer.slice(SliceCriterion {
               variable: self.variable.clone(),
               line: self.line,
           });
           
           println!("Backward Slice for {}:{} (variable: {})", 
                    self.file.display(), self.line, self.variable);
           println!("Reduction: {:.1}%", slice.reduction_percent);
           println!("\nRelevant lines:");
           for line in slice.lines.iter().sorted() {
               println!("  {}", line);
           }
           
           Ok(())
       }
   }
   ```

4. **Testing**
   ```rust
   #[test]
   fn test_backward_slice_reduction() {
       let code = r#"
       fn process(input: String) -> String {
           let a = 10;           // Line 2: Not in slice
           let b = 20;           // Line 3: Not in slice
           let x = input.len();  // Line 4: In slice (uses input)
           let y = x * 2;        // Line 5: In slice (uses x)
           format!("{}", y)      // Line 6: Criterion - In slice
       }
       "#;
       
       let cfg = build_cfg(code).unwrap();
       let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
       let slicer = BackwardSlicer::new(&pdg, &cfg);
       
       let criterion = SliceCriterion { line: 6, variable: "y".to_string() };
       let slice = slicer.slice(criterion);
       
       // Should include lines 4, 5, 6 but NOT 2, 3
       assert!(slice.lines.contains(&4));  // x definition
       assert!(slice.lines.contains(&5));  // y definition
       assert!(slice.lines.contains(&6));  // criterion
       assert!(!slice.lines.contains(&2)); // a not relevant
       assert!(!slice.lines.contains(&3)); // b not relevant
       
       // Should reduce by at least 30%
       assert!(slice.reduction_percent > 30.0);
   }
   ```

### Performance Target
- 80%+ reduction on typical functions
- <1s for backward slice of 1000 LOC function

---

# Section 12.2: Blast Radius Analysis (1-2 weeks)

Now that we have PDG from 12.1, enhance blast radius with data flow analysis.

## Implementation

```rust
// src/analysis/blast_radius.rs
pub struct BlastRadiusAnalyzer<'a> {
    graph: &'a CodeGraph,
    pdg_cache: &'a PdgCache,
}

impl<'a> BlastRadiusAnalyzer<'a> {
    pub fn analyze(&self, symbol_id: Uuid) -> BlastRadiusReport {
        // 1. Find all direct callers (structural dependency)
        let direct_callers = self.graph.find_callers(symbol_id);
        
        // 2. For each caller, use backward slicing to find HOW it uses the symbol
        let mut impact_details = Vec::new();
        for caller_id in &direct_callers {
            if let Some(pdg) = self.pdg_cache.get(caller_id) {
                // Trace data flow from symbol to caller's outputs
                let data_flow = self.trace_data_flow(symbol_id, caller_id, pdg);
                impact_details.push(ImpactDetail {
                    caller: *caller_id,
                    data_flow_depth: data_flow.depth,
                    affected_outputs: data_flow.sinks,
                });
            }
        }
        
        // 3. Recursively traverse call graph (forward from symbol)
        let mut visited = HashSet::new();
        let mut impact_zone = Vec::new();
        let mut queue = VecDeque::from(direct_callers.clone());
        
        while let Some(node_id) = queue.pop_front() {
            if visited.insert(node_id) {
                impact_zone.push(node_id);
                queue.extend(self.graph.find_callers(node_id));
            }
        }
        
        // 4. Calculate impact score (weighted by data flow depth)
        let score = self.calculate_impact_score(&impact_zone, &impact_details);
        
        BlastRadiusReport {
            symbol: symbol_id,
            direct_dependencies: direct_callers.len(),
            total_impact_zone: impact_zone.len(),
            score,
            data_flow_impact: impact_details,
            // ... other fields
        }
    }
}
```

---

# Section 12.3: Semantic Search / NLP Enhancement (3-4 weeks)

Can be developed in parallel with 12.4.

## Task 12.3.3: Dual-Agent Query Translation (HIGH PRIORITY)

### Overview
Implement "Write Then Translate" architecture proven by CodexGraph research.

### Architecture

```rust
// src/nlp/dual_agent.rs
pub struct DualAgentQuerySystem {
    primary_agent: PrimaryAgent,
    translation_agent: TranslationAgent,
    max_iterations: usize,
}

pub struct PrimaryAgent {
    // LLM client for high-level reasoning
    llm_client: LlmClient,
    system_prompt: String,
}

pub struct TranslationAgent {
    // Converts NL → rBuilder query patterns
    query_examples: Vec<(String, String)>,  // (NL, pattern) pairs
}

pub struct QueryContext {
    original_question: String,
    sub_queries: Vec<SubQuery>,
    accumulated_results: Vec<QueryResult>,
}

pub struct SubQuery {
    natural_language: String,
    translated_pattern: Option<String>,
    results: Vec<Node>,
}

impl DualAgentQuerySystem {
    pub async fn query(&self, question: &str, graph: &CodeGraph) -> Result<QueryResult> {
        let mut context = QueryContext::new(question);
        
        for iteration in 0..self.max_iterations {
            // 1. Primary agent generates sub-queries
            let sub_queries = self.primary_agent
                .decompose(question, &context)
                .await?;
            
            if sub_queries.is_empty() {
                break;  // Agent has enough context
            }
            
            // 2. Translation agent converts each sub-query
            for nl_query in sub_queries {
                let pattern = self.translation_agent.translate(&nl_query)?;
                let results = execute(graph, &pattern)?;
                context.add_results(nl_query, pattern, results);
            }
            
            // 3. Check if primary agent is satisfied
            if self.primary_agent.has_sufficient_context(&context).await? {
                break;
            }
        }
        
        // 4. Synthesize final answer
        self.primary_agent.synthesize_answer(question, &context).await
    }
}
```

### Translation Agent Training Data

Create `query_examples.toml`:
```toml
[[examples]]
nl = "functions that call authenticate"
pattern = "type:Function|calls:authenticate"

[[examples]]
nl = "complex functions"
pattern = "type:Function|complexity:>15"

[[examples]]
nl = "public API endpoints"
pattern = "type:Function|visibility:public|label:api"

# Add 50+ examples covering common queries
```

### Primary Agent System Prompt

```
You are a code analysis query planner. Given a user question:

1. Decompose into specific sub-questions answerable by querying a code graph
2. Ask one sub-question at a time, starting with most specific
3. Review results and determine if you need more information
4. When sufficient context gathered, synthesize the final answer

Available query capabilities:
- Find symbols by name, type, complexity, labels
- Trace call relationships
- Analyze data/control flow dependencies
- Compute impact/blast radius

Example:
User: "What security issues exist in authentication?"
Sub-queries:
1. "Find all authentication-related functions"
2. "Check which functions handle user input"
3. "Find functions that construct SQL queries"
4. "Check for hardcoded credentials"
```

### Implementation Notes

1. **LLM Client** - Use existing `reqwest` for API calls (feature-gated)
2. **Translation Agent** - Can be rule-based or few-shot prompted
3. **Caching** - Cache translations to avoid redundant LLM calls
4. **Fallback** - If LLM unavailable, fall back to pattern matching

---

# Section 12.4: Graph Query Language (4-5 weeks)

Can be developed in parallel with 12.3.

## Task 12.4.1: Design Query Language Syntax (2 weeks)

### Recommended Parser: `lalrpop`

Use `lalrpop` (Rust parser generator) for robust syntax parsing.

1. **Add Dependency**
   ```toml
   [dependencies]
   lalrpop-util = "0.20"
   
   [build-dependencies]
   lalrpop = "0.20"
   ```

2. **Create Grammar** (`src/query/grammar.lalrpop`)
   ```
   use crate::query::ast::*;
   
   grammar;
   
   pub Query: Query = {
       <patterns:MatchPattern+> <where_clause:WhereClause?> <return_clause:ReturnClause> => {
           Query { patterns, where_clause, return_clause, ..Default::default() }
       }
   };
   
   MatchPattern: Pattern = {
       "MATCH" <node:NodePattern> <edges:EdgePattern*> => Pattern { node, edges }
   };
   
   NodePattern: NodePattern = {
       "(" <var:Ident> <node_type:(":" <NodeType>)?> <props:PropertyMap?> ")" => {
           NodePattern { variable: var, node_type, properties: props.unwrap_or_default() }
       }
   };
   
   EdgePattern: EdgePattern = {
       "-" "[" <edge_type:EdgeType> <hops:HopRange?> "]" "->" => {
           EdgePattern {
               edge_type,
               direction: Direction::Forward,
               min_hops: hops.as_ref().map(|h| h.0).unwrap_or(1),
               max_hops: hops.map(|h| h.1),
           }
       }
   };
   
   HopRange: (usize, Option<usize>) = {
       "*" <min:Num> ".." <max:Num> => (min, Some(max)),
       "*" => (1, None),
   };
   ```

3. **Define AST** (`src/query/ast.rs`)
   ```rust
   #[derive(Debug, Clone)]
   pub struct Query {
       pub patterns: Vec<Pattern>,
       pub where_clause: Option<WhereClause>,
       pub return_clause: ReturnClause,
       pub order_by: Option<OrderBy>,
       pub limit: Option<usize>,
   }
   
   #[derive(Debug, Clone)]
   pub struct Pattern {
       pub node: NodePattern,
       pub edges: Vec<EdgePattern>,
   }
   
   #[derive(Debug, Clone)]
   pub struct NodePattern {
       pub variable: String,
       pub node_type: Option<NodeType>,
       pub properties: HashMap<String, PropertyMatcher>,
   }
   
   #[derive(Debug, Clone)]
   pub enum PropertyMatcher {
       Equals(String),
       Like(String),
       GreaterThan(f64),
       LessThan(f64),
       In(Vec<String>),
   }
   ```

---

# Testing Strategy for Phase 12

## Unit Tests
- Each module (`cfg`, `pdg`, `slicing`, `dual_agent`, `query`) has own test file
- Cover edge cases: empty functions, deeply nested loops, complex data flow

## Integration Tests
- `tests/phase12_cfg.rs` - CFG construction across languages
- `tests/phase12_pdg.rs` - PDG and reaching definitions
- `tests/phase12_slicing.rs` - Backward slicing accuracy
- `tests/phase12_dual_agent.rs` - Query translation accuracy
- `tests/phase12_query_lang.rs` - Graph query language

## Benchmarks
- `benches/phase12_cfg.rs` - CFG construction performance
- `benches/phase12_pdg.rs` - PDG construction performance
- `benches/phase12_query.rs` - Query execution performance

## Success Metrics (from research)
- Query accuracy (dual-agent): 90%+ vs 60% baseline
- Code reduction (slicing): 80%+
- Query performance (simple): <100ms
- Query performance (complex): <2s
- CFG construction: <100ms per 1K LOC
- PDG construction: <500ms per 5K LOC

---

# Incremental Delivery Milestones

## Milestone 1: Schema Enrichment (Week 1-2)
- ✅ Signatures added to schema
- ✅ Code hashing implemented
- ✅ Edge properties added
- ✅ All tests passing
- **Deliverable**: Updated schema, migration scripts

## Milestone 2: CFG Construction (Week 3-5)
- ✅ CFG for Rust implemented
- ✅ CFG for Python implemented
- ✅ CFG visualization (DOT export)
- ✅ Performance benchmarks pass
- **Deliverable**: Working CFG for top 2 languages

## Milestone 3: PDG & Slicing (Week 6-10)
- ✅ Reaching definitions algorithm
- ✅ PDG construction
- ✅ Backward slicing
- ✅ 80%+ reduction achieved
- **Deliverable**: End-to-end slicing demo

## Milestone 4: Enhanced Blast Radius (Week 11-12)
- ✅ PDG integration
- ✅ Data flow impact analysis
- ✅ MCP tool updated
- **Deliverable**: Blast radius with data flow

## Milestone 5: Dual-Agent Query (Week 13-16)
- ✅ Translation agent with examples
- ✅ Primary agent integration
- ✅ 90%+ accuracy on test queries
- **Deliverable**: Working dual-agent system

## Milestone 6: Graph Query Language (Week 17-21)
- ✅ Parser implemented
- ✅ Executor working
- ✅ Multi-hop patterns supported
- **Deliverable**: Full query language

## Milestone 7: Polish & Optimize (Week 22-24)
- ✅ Query macros
- ✅ Explain plan
- ✅ Performance optimization
- ✅ Documentation complete
- **Deliverable**: Phase 12 complete

---

# Critical Decision Points for Cursor

## Decision 1: CFG Language Coverage
**Question**: Build CFG for all 41 languages or just top 6?

**Recommendation**: Start with top 6 (Rust, Python, TypeScript, JavaScript, Go, Java)
- Tree-sitter node kinds vary significantly across languages
- 80/20 rule: 6 languages cover 90%+ of users
- Can add more languages iteratively

**Fallback**: If no CFG available, skip PDG-based analysis for that language

---

## Decision 2: LLM Provider for Dual-Agent
**Question**: Which LLM API to use?

**Options**:
1. **OpenAI GPT-4** - Most capable, expensive
2. **Anthropic Claude** - Good balance, $$ moderate
3. **Local model** - Free but requires GPU
4. **Optional/feature-gated** - Let user choose

**Recommendation**: Make LLM optional via feature flag
- Default: Pattern matching only (no dual-agent)
- Feature `nlp-llm`: Enable dual-agent with configurable API
- Let users configure API key in `rbuilder.toml`

---

## Decision 3: Query Language Complexity
**Question**: Full Cypher-like language or simplified subset?

**Recommendation**: Start with simplified subset
- MATCH, WHERE, RETURN, ORDER BY, LIMIT
- Multi-hop patterns: `-[:CALLS*1..3]->`
- Path queries: `shortestPath()`
- **Skip**: Aggregations (COUNT, AVG) until Phase 13
- **Skip**: Complex subqueries until needed

**Rationale**: Simpler = faster implementation, fewer bugs

---

## Decision 4: Storage for PDG Cache
**Question**: In-memory only or persist to disk?

**Recommendation**: Hybrid approach
- **In-memory**: During active session (fast)
- **Disk cache**: Persist to `.rbuilder/pdg_cache/` (optional)
- **Invalidation**: Use code hash to detect stale PDGs

**Implementation**:
```rust
pub struct PdgCache {
    memory: HashMap<Uuid, Arc<ProgramDependenceGraph>>,
    disk_cache: Option<PathBuf>,
}
```

---

# Common Pitfalls to Avoid

## Pitfall 1: Trying to Perfect CFG for All Edge Cases
**Problem**: Control flow in real code is messy (exceptions, early returns, goto, etc.)

**Solution**: Start simple, handle common cases (if/else, loops, sequential)
- Build 80% solution first
- Add edge cases iteratively based on user feedback
- Document known limitations

## Pitfall 2: Over-Engineering Query Language
**Problem**: Temptation to build full Cypher/SQL-like language

**Solution**: MVP first
- MATCH/WHERE/RETURN is 90% of use cases
- Aggregations can wait
- Focus on multi-hop patterns (core value prop)

## Pitfall 3: Underestimating PDG Complexity
**Problem**: Reaching definitions + control dependencies is hard

**Solution**: Allocate 4 full weeks
- Study existing implementations (LLVM, GCC, Soot)
- Test extensively with small examples
- Use visualization to debug (DOT graphs)

## Pitfall 4: Making Dual-Agent Mandatory
**Problem**: Requires LLM API, adds latency, costs money

**Solution**: Feature-gated optional enhancement
- Core query system works without LLM
- Dual-agent is "nice-to-have" not "must-have"
- Fallback to pattern matching always available

---

# Resources for Cursor

## Research Papers (Re-read These)
1. **Codebadger** (2026): https://arxiv.org/html/2603.24837v1
   - Focus on: CFG/PDG construction, backward slicing algorithm
2. **CodexGraph** (NAACL 2025): https://arxiv.org/html/2408.03910v2
   - Focus on: Dual-agent architecture, query translation

## Reference Implementations
1. **Joern** (CPG engine): https://github.com/joernio/joern
   - Study: CFG/PDG construction
2. **tree-sitter queries**: https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries
   - Study: AST pattern matching
3. **LLVM IR**: https://llvm.org/docs/LangRef.html
   - Study: CFG representation

## Rust Crates to Consider
- `lalrpop` - Parser generator (for query language)
- `petgraph` - Already used, good for graph algorithms
- `blake3` - Fast hashing (already in deps)
- `dashmap` - Concurrent HashMap (if multi-threading CFG construction)

---

# Final Recommendations for Cursor

## DO ✅
1. **Follow the dependency order** - 12.0 → 12.1 → 12.2 → 12.3/12.4 → 12.5
2. **Test incrementally** - Each task should have passing tests before moving on
3. **Start simple** - CFG for Rust only, then expand
4. **Visualize early** - Export CFG/PDG to DOT graphs for debugging
5. **Benchmark continuously** - Ensure performance targets are met
6. **Document decisions** - Capture why you chose an approach
7. **Feature-gate heavy features** - Dual-agent, complex query language

## DON'T ❌
1. **Don't try to support all 41 languages for CFG** - Start with 6
2. **Don't make LLM mandatory** - Feature flag it
3. **Don't build full Cypher** - Simplified subset is enough
4. **Don't skip tests** - CFG/PDG bugs are subtle and hard to debug
5. **Don't optimize prematurely** - Correct first, fast second
6. **Don't add external dependencies** - No Redis, Neo4j, etc.

---

# Success Criteria Checklist

Before marking Phase 12 complete, verify:

- [ ] Schema enrichment: Signatures on all nodes, code hashing works
- [ ] CFG: Works for Rust, Python, TypeScript, JavaScript, Go, Java
- [ ] CFG: Handles if/else, loops, returns correctly
- [ ] CFG: Has cycle detection, path finding
- [ ] PDG: Reaching definitions algorithm correct
- [ ] PDG: Data dependencies extracted accurately
- [ ] PDG: Control dependencies computed
- [ ] Slicing: Backward slicing achieves 80%+ reduction
- [ ] Slicing: MCP tool works, CLI works
- [ ] Blast radius: Enhanced with data flow analysis
- [ ] Dual-agent: Translation agent with 50+ examples
- [ ] Dual-agent: 90%+ accuracy on test queries
- [ ] Query language: Parser works (MATCH/WHERE/RETURN)
- [ ] Query language: Multi-hop patterns work
- [ ] Query language: Performance <100ms for simple queries
- [ ] All tests passing (unit + integration + benchmarks)
- [ ] Documentation complete (README, API docs, examples)
- [ ] Performance targets met (see "Success Metrics")

---

**Good luck, Cursor! This is an ambitious phase but the architecture is solid. Take your time, test thoroughly, and deliver incrementally.**
