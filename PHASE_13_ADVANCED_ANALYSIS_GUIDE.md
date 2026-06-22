# Phase 13: Advanced Program Analysis — Implementation Guide

**Target Audience:** Cursor (AI coding assistant)  
**Created:** June 17, 2026  
**Author:** Claude Code  
**Phase 12 Grade:** A+ (Exceptional)

---

## Executive Summary

Phase 13 closes the remaining **research gaps** identified in RESEARCH_GAP_ANALYSIS.md by implementing advanced program analysis techniques from Codebadger and CodexGraph. While the TASK_PLAN.md defines Phase 13 as "Real-time Updates & Automation," this guide focuses on the **advanced analysis capabilities** that elevate rBuilder to a research-grade system.

**Phase 13 Goals:**
1. **Taint Analysis** — forward data flow tracking from sources to sinks (security)
2. **Interprocedural Analysis** — cross-function CFG/PDG/slicing
3. **Dominance Analysis** — precise control dependencies for improved slicing
4. **Type Inference** — dynamic language support (Python, JavaScript)
5. **GQL Optimizer** — query planning and index selection
6. **Security Context** — CVE pattern matching and vulnerability detection

**Estimated Effort:** 16-20 weeks (serial) or 8-10 weeks (parallel)

**Success Criteria:**
- ✅ Taint analysis detects 95%+ of SQL injection patterns
- ✅ Interprocedural slicing reduces code by 95%+ (vs 90% intraprocedural)
- ✅ Dominance-based control deps improve slice precision by 15%+
- ✅ Type inference covers Python, JavaScript, Ruby
- ✅ GQL optimizer reduces query time by 50%+ on large graphs
- ✅ Zero new dependencies (Rust-native only)

---

## Table of Contents

1. [Section 13.0: Taint Analysis](#130-taint-analysis)
2. [Section 13.1: Interprocedural Analysis](#131-interprocedural-analysis)
3. [Section 13.2: Dominance & Control Dependencies](#132-dominance--control-dependencies)
4. [Section 13.3: Type Inference](#133-type-inference)
5. [Section 13.4: GQL Query Optimizer](#134-gql-query-optimizer)
6. [Section 13.5: Security Context & CVE Patterns](#135-security-context--cve-patterns)
7. [Critical Dependencies](#critical-dependencies)
8. [Milestones](#milestones)
9. [Testing Strategy](#testing-strategy)
10. [Success Criteria](#success-criteria)

---

## 13.0 Taint Analysis

**Effort:** 4-5 weeks  
**Complexity:** High  
**Research Reference:** Codebadger paper, Section 4.2 (Data Flow Analysis)

### Overview

Taint analysis tracks **forward data flow** from untrusted sources (user input, files, network) to security-sensitive sinks (SQL queries, shell commands, file writes). This complements Phase 12's backward slicing (which tracks dependencies **backward** from a point of interest).

**Example Vulnerability:**

```python
# Source: untrusted user input
username = request.GET['username']  # TAINT SOURCE

# Propagation through assignments
query_part = username
table_name = "users"

# Sink: SQL query (vulnerable!)
cursor.execute(f"SELECT * FROM {table_name} WHERE name = '{query_part}'")  # TAINT SINK
```

Taint analysis should flag line 8 as "tainted data flows to SQL sink without sanitization."

### Architecture

#### 13.0.1 Taint Graph Data Structure

**File:** `src/analysis/taint.rs`

```rust
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Taint source classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaintSource {
    /// HTTP request parameter (GET/POST)
    HttpParameter,
    /// File read
    FileInput,
    /// Network socket
    NetworkInput,
    /// Command-line argument
    CommandLineArg,
    /// Environment variable
    EnvironmentVar,
    /// Database query result (secondary source)
    DatabaseResult,
}

/// Taint sink classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaintSink {
    /// SQL query execution
    SqlQuery,
    /// Shell command execution
    ShellCommand,
    /// File write operation
    FileWrite,
    /// Network transmission
    NetworkOutput,
    /// Log output (potential log injection)
    LogOutput,
    /// HTML rendering (XSS)
    HtmlRender,
    /// Eval/exec (code injection)
    CodeEval,
}

/// Sanitizer that breaks taint flow.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Sanitizer {
    /// SQL parameter binding (prepared statements)
    SqlParameterize,
    /// HTML escape
    HtmlEscape,
    /// Shell escape
    ShellEscape,
    /// Input validation regex
    Validation(String),
    /// Type conversion (e.g., int())
    TypeCast(String),
}

/// Taint flow path from source to sink.
#[derive(Debug, Clone)]
pub struct TaintFlow {
    /// Source node (where taint originates)
    pub source: Uuid,
    /// Source classification
    pub source_type: TaintSource,
    /// Sink node (where tainted data is used)
    pub sink: Uuid,
    /// Sink classification
    pub sink_type: TaintSink,
    /// Variable carrying the taint
    pub variable: String,
    /// Nodes in the flow path
    pub path: Vec<Uuid>,
    /// Sanitizers applied (empty = VULNERABLE)
    pub sanitizers: Vec<Sanitizer>,
    /// Severity (1-10, based on source/sink combination)
    pub severity: u8,
}

impl TaintFlow {
    /// Returns true if this flow is vulnerable (no sanitizers).
    pub fn is_vulnerable(&self) -> bool {
        self.sanitizers.is_empty()
    }

    /// Compute severity based on source/sink pair.
    pub fn compute_severity(&mut self) {
        self.severity = match (self.source_type, self.sink_type) {
            (TaintSource::HttpParameter, TaintSink::SqlQuery) => 10,  // SQL injection
            (TaintSource::HttpParameter, TaintSink::ShellCommand) => 10,  // Command injection
            (TaintSource::HttpParameter, TaintSink::HtmlRender) => 9,  // XSS
            (TaintSource::HttpParameter, TaintSink::CodeEval) => 10,  // Code injection
            (TaintSource::FileInput, TaintSink::ShellCommand) => 8,
            (TaintSource::FileInput, TaintSink::SqlQuery) => 7,
            (TaintSource::DatabaseResult, TaintSink::HtmlRender) => 6,  // Stored XSS
            (TaintSource::EnvironmentVar, TaintSink::ShellCommand) => 7,
            _ => 5,
        };
    }
}

/// Taint analysis engine.
pub struct TaintAnalyzer<'a> {
    pdg: &'a ProgramDependenceGraph,
    cfg: &'a ControlFlowGraph,
    /// Source nodes (taint origins)
    sources: HashMap<Uuid, TaintSource>,
    /// Sink nodes (security-sensitive operations)
    sinks: HashMap<Uuid, TaintSink>,
    /// Sanitizer nodes (taint breakers)
    sanitizers: HashMap<Uuid, Sanitizer>,
}

impl<'a> TaintAnalyzer<'a> {
    /// Create a taint analyzer over the given PDG.
    pub fn new(pdg: &'a ProgramDependenceGraph, cfg: &'a ControlFlowGraph) -> Self {
        Self {
            pdg,
            cfg,
            sources: HashMap::new(),
            sinks: HashMap::new(),
            sanitizers: HashMap::new(),
        }
    }

    /// Detect sources, sinks, and sanitizers by analyzing statement text.
    pub fn detect_patterns(&mut self, language: &str) {
        match language {
            "python" | "py" => self.detect_python_patterns(),
            "javascript" | "js" | "typescript" | "ts" => self.detect_js_patterns(),
            "rust" | "rs" => self.detect_rust_patterns(),
            _ => {}
        }
    }

    fn detect_python_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;

            // Sources
            if text.contains("request.GET") || text.contains("request.POST") {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            } else if text.contains("open(") && text.contains("read") {
                self.sources.insert(*node_id, TaintSource::FileInput);
            } else if text.contains("sys.argv") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("os.environ") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            }

            // Sinks
            if text.contains("execute(") || text.contains("executemany(") {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            } else if text.contains("os.system(") || text.contains("subprocess.") {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("eval(") || text.contains("exec(") {
                self.sinks.insert(*node_id, TaintSink::CodeEval);
            } else if text.contains("render(") || text.contains(".html") {
                self.sinks.insert(*node_id, TaintSink::HtmlRender);
            }

            // Sanitizers
            if text.contains("int(") || text.contains("float(") {
                self.sanitizers.insert(*node_id, Sanitizer::TypeCast("numeric".into()));
            } else if text.contains("escape(") || text.contains("html.escape") {
                self.sanitizers.insert(*node_id, Sanitizer::HtmlEscape);
            } else if text.contains("shlex.quote") {
                self.sanitizers.insert(*node_id, Sanitizer::ShellEscape);
            }
        }
    }

    fn detect_js_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;

            // Sources
            if text.contains("req.query") || text.contains("req.body") || text.contains("req.params") {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            } else if text.contains("fs.readFile") {
                self.sources.insert(*node_id, TaintSource::FileInput);
            } else if text.contains("process.argv") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("process.env") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            }

            // Sinks
            if text.contains(".query(") || text.contains(".execute(") {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            } else if text.contains("exec(") || text.contains("spawn(") {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("eval(") || text.contains("Function(") {
                self.sinks.insert(*node_id, TaintSink::CodeEval);
            } else if text.contains("innerHTML") || text.contains("document.write") {
                self.sinks.insert(*node_id, TaintSink::HtmlRender);
            }

            // Sanitizers
            if text.contains("parseInt(") || text.contains("parseFloat(") {
                self.sanitizers.insert(*node_id, Sanitizer::TypeCast("numeric".into()));
            } else if text.contains("escapeHtml(") || text.contains("sanitize(") {
                self.sanitizers.insert(*node_id, Sanitizer::HtmlEscape);
            }
        }
    }

    fn detect_rust_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;

            // Sources
            if text.contains("env::var") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            } else if text.contains("env::args") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("File::open") || text.contains(".read_to_string") {
                self.sources.insert(*node_id, TaintSource::FileInput);
            }

            // Sinks (Rust is generally safer, but still has risks)
            if text.contains("Command::new") && text.contains(".arg(") {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("query(") || text.contains("execute(") {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            }

            // Sanitizers
            if text.contains(".parse::<") {
                self.sanitizers.insert(*node_id, Sanitizer::TypeCast("typed".into()));
            }
        }
    }

    /// Perform forward taint analysis to find all vulnerable flows.
    pub fn analyze(&self) -> Vec<TaintFlow> {
        let mut flows = Vec::new();

        for (source_id, source_type) in &self.sources {
            let source_node = &self.pdg.nodes[source_id];
            
            // For each variable defined at the source
            for var in &source_node.defined_vars {
                // Find all sinks reachable via data dependencies
                let reachable_sinks = self.find_reachable_sinks(*source_id, var);
                
                for (sink_id, path) in reachable_sinks {
                    let sanitizers = self.find_sanitizers_on_path(&path, var);
                    
                    let mut flow = TaintFlow {
                        source: *source_id,
                        source_type: *source_type,
                        sink: sink_id,
                        sink_type: self.sinks[&sink_id],
                        variable: var.clone(),
                        path,
                        sanitizers,
                        severity: 0,
                    };
                    flow.compute_severity();
                    flows.push(flow);
                }
            }
        }

        flows
    }

    /// BFS to find all sinks reachable from source via data dependencies.
    fn find_reachable_sinks(&self, source: Uuid, variable: &str) -> Vec<(Uuid, Vec<Uuid>)> {
        use std::collections::VecDeque;

        let mut reachable = Vec::new();
        let mut visited = HashSet::new();
        let mut queue: VecDeque<(Uuid, Vec<Uuid>)> = VecDeque::new();
        queue.push_back((source, vec![source]));

        while let Some((current, path)) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }

            // Check if current is a sink
            if self.sinks.contains_key(&current) {
                reachable.push((current, path.clone()));
            }

            // Follow data dependencies where variable flows
            for dep in self.pdg.data_deps.iter().filter(|d| d.from == current) {
                if dep.variable == variable || self.variable_aliases(&dep.variable, variable) {
                    let mut new_path = path.clone();
                    new_path.push(dep.to);
                    queue.push_back((dep.to, new_path));
                }
            }
        }

        reachable
    }

    /// Check if two variables are aliases (e.g., `x = y` creates alias).
    fn variable_aliases(&self, var1: &str, var2: &str) -> bool {
        // Simplified: exact match or assignment tracking (future enhancement)
        var1 == var2
    }

    /// Find sanitizers along the taint path.
    fn find_sanitizers_on_path(&self, path: &[Uuid], _variable: &str) -> Vec<Sanitizer> {
        path.iter()
            .filter_map(|node_id| self.sanitizers.get(node_id).cloned())
            .collect()
    }

    /// Report all vulnerable flows (no sanitizers).
    pub fn vulnerable_flows(&self) -> Vec<TaintFlow> {
        self.analyze()
            .into_iter()
            .filter(|flow| flow.is_vulnerable())
            .collect()
    }
}
```

**Key Design Decisions:**

1. **Pattern-Based Detection** — regex on statement text (fast, works across languages)
2. **BFS for Reachability** — efficient forward search
3. **Sanitizer Tracking** — breaks taint flow when found
4. **Severity Scoring** — prioritizes critical vulnerabilities (SQL injection = 10)

#### 13.0.2 Test Requirements

**File:** `tests/phase13_taint.rs`

```rust
use rbuilder::analysis::{TaintAnalyzer, TaintSink, TaintSource, build_cfg_for_function};
use rbuilder::analysis::pdg::ProgramDependenceGraph;

#[cfg(feature = "lang-python")]
#[test]
fn test_taint_sql_injection_python() {
    let code = r#"
def handle_request(request):
    username = request.GET['username']  # SOURCE
    query = f"SELECT * FROM users WHERE name = '{username}'"  # SINK
    cursor.execute(query)
"#;

    let cfg = build_cfg_for_function("python", code, "handle_request").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("python");

    let flows = analyzer.vulnerable_flows();
    
    assert_eq!(flows.len(), 1, "Should detect one vulnerable flow");
    assert_eq!(flows[0].source_type, TaintSource::HttpParameter);
    assert_eq!(flows[0].sink_type, TaintSink::SqlQuery);
    assert_eq!(flows[0].severity, 10);
    assert!(flows[0].is_vulnerable());
}

#[cfg(feature = "lang-python")]
#[test]
fn test_taint_sanitized_flow_python() {
    let code = r#"
def handle_request(request):
    user_id = request.GET['id']
    safe_id = int(user_id)  # SANITIZER
    query = f"SELECT * FROM users WHERE id = {safe_id}"
    cursor.execute(query)
"#;

    let cfg = build_cfg_for_function("python", code, "handle_request").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("python");

    let flows = analyzer.vulnerable_flows();
    
    assert_eq!(flows.len(), 0, "Sanitized flow should not be vulnerable");
    
    let all_flows = analyzer.analyze();
    assert_eq!(all_flows.len(), 1, "Should detect the flow");
    assert!(!all_flows[0].sanitizers.is_empty(), "Should have sanitizer");
}

#[cfg(feature = "lang-javascript")]
#[test]
fn test_taint_xss_javascript() {
    let code = r#"
function renderUser(req) {
    const name = req.query.name;  // SOURCE
    document.getElementById('output').innerHTML = name;  // SINK (XSS)
}
"#;

    let cfg = build_cfg_for_function("javascript", code, "renderUser").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("javascript");

    let flows = analyzer.vulnerable_flows();
    
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].source_type, TaintSource::HttpParameter);
    assert_eq!(flows[0].sink_type, TaintSink::HtmlRender);
    assert_eq!(flows[0].severity, 9);
}

#[test]
fn test_taint_no_flow_independent_vars() {
    let code = r#"
def safe_function(request):
    user_input = request.GET['data']
    safe_value = 42
    query = f"SELECT * FROM table WHERE id = {safe_value}"
    cursor.execute(query)
"#;

    let cfg = build_cfg_for_function("python", code, "safe_function").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("python");

    let flows = analyzer.vulnerable_flows();
    assert_eq!(flows.len(), 0, "No taint flow when variables are independent");
}
```

**Test Coverage:**
- ✅ SQL injection detection
- ✅ XSS detection
- ✅ Sanitizer recognition (breaks flow)
- ✅ No false positives (independent variables)
- ✅ Multi-language support (Python, JavaScript)

#### 13.0.3 MCP Integration

**File:** `src/mcp/tools.rs` (add new tool)

```rust
fn taint_analysis(&self, backend: &MemoryBackend, file: &str, function: Option<&str>, language: Option<&str>, verbose: bool) -> Result<Value> {
    // Read source file
    let source = std::fs::read_to_string(file)?;
    
    // Build CFG/PDG
    let lang = language.unwrap_or_else(|| detect_language(file));
    let func_name = function.ok_or_else(|| Error::InvalidQuery("function name required".into()))?;
    let cfg = build_cfg_for_function(lang, &source, func_name)?;
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
    
    // Run taint analysis
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns(lang);
    let flows = analyzer.analyze();
    let vulnerable = analyzer.vulnerable_flows();
    
    Ok(json!({
        "file": file,
        "function": func_name,
        "total_flows": flows.len(),
        "vulnerable_flows": vulnerable.len(),
        "vulnerabilities": vulnerable.iter().map(|f| json!({
            "severity": f.severity,
            "source_type": format!("{:?}", f.source_type),
            "sink_type": format!("{:?}", f.sink_type),
            "variable": f.variable,
            "path_length": f.path.len(),
        })).collect::<Vec<_>>(),
        "details": if verbose {
            Some(flows.iter().map(|f| json!({
                "source": f.source.to_string(),
                "sink": f.sink.to_string(),
                "variable": f.variable,
                "sanitizers": f.sanitizers.iter().map(|s| format!("{:?}", s)).collect::<Vec<_>>(),
                "severity": f.severity,
                "vulnerable": f.is_vulnerable(),
            })).collect::<Vec<_>>())
        } else {
            None
        }
    }))
}
```

**MCP Tool Definition:**

```json
{
  "name": "taint_analysis",
  "description": "Detect security vulnerabilities via forward taint analysis",
  "inputSchema": {
    "type": "object",
    "properties": {
      "file": {
        "type": "string",
        "description": "Path to source file"
      },
      "function": {
        "type": "string",
        "description": "Function name to analyze"
      },
      "language": {
        "type": "string",
        "description": "Language (python, javascript, rust)"
      }
    },
    "required": ["file", "function"]
  }
}
```

---

## 13.1 Interprocedural Analysis

**Effort:** 5-6 weeks  
**Complexity:** Very High  
**Research Reference:** Codebadger interprocedural PDG, cross-function slicing

### Overview

Phase 12 implemented **intraprocedural** analysis (single-function CFG/PDG). Phase 13 extends this to **interprocedural** — analyzing call graphs and building cross-function CFG/PDG.

**Benefits:**
- More precise slicing (follows dependencies across function boundaries)
- Taint analysis across function calls
- Whole-program optimization insights

**Example:**

```rust
fn main() {
    let data = read_input();  // Entry point
    let result = process(data);
    write_output(result);
}

fn process(input: String) -> String {
    let trimmed = input.trim();
    format!("Processed: {}", trimmed)
}
```

Interprocedural backward slice on `result` at line 3 should include:
- Line 3 (direct use)
- Lines in `process` function (cross-function)
- Line 2 (call to `process`)

### Architecture

#### 13.1.1 Call Graph Construction

**File:** `src/analysis/callgraph.rs`

```rust
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Call graph node representing a function.
#[derive(Debug, Clone)]
pub struct CallGraphNode {
    /// Node ID (matches graph node UUID)
    pub id: Uuid,
    /// Function name
    pub name: String,
    /// Qualified name (module::function)
    pub qualified_name: Option<String>,
    /// Source file
    pub file_path: String,
    /// Start line
    pub start_line: usize,
}

/// Call graph edge representing a function call.
#[derive(Debug, Clone)]
pub struct CallGraphEdge {
    /// Caller function
    pub from: Uuid,
    /// Callee function
    pub to: Uuid,
    /// Call site line number
    pub call_site: usize,
    /// Direct call vs indirect (function pointer)
    pub call_type: CallType,
}

/// Call graph for the entire program.
#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    /// All functions in the program
    pub nodes: HashMap<Uuid, CallGraphNode>,
    /// All function calls
    pub edges: Vec<CallGraphEdge>,
}

impl CallGraph {
    /// Build call graph from the code knowledge graph.
    pub fn from_backend(backend: &MemoryBackend) -> Result<Self> {
        let mut cg = Self::default();
        
        // Add all function nodes
        for node in backend.nodes() {
            if node.node_type == NodeType::Function {
                cg.nodes.insert(
                    node.id,
                    CallGraphNode {
                        id: node.id,
                        name: node.name.clone(),
                        qualified_name: node.qualified_name.clone(),
                        file_path: node.file_path.clone().unwrap_or_default(),
                        start_line: node.start_line.unwrap_or(0),
                    },
                );
            }
        }
        
        // Add all call edges
        for edge in backend.edges() {
            if edge.edge_type == EdgeType::Calls {
                cg.edges.push(CallGraphEdge {
                    from: edge.from,
                    to: edge.to,
                    call_site: 0,  // TODO: extract from edge properties
                    call_type: edge.call_type.unwrap_or(CallType::Direct),
                });
            }
        }
        
        Ok(cg)
    }

    /// Get all callees of a function.
    pub fn callees(&self, function: Uuid) -> Vec<Uuid> {
        self.edges
            .iter()
            .filter(|e| e.from == function)
            .map(|e| e.to)
            .collect()
    }

    /// Get all callers of a function.
    pub fn callers(&self, function: Uuid) -> Vec<Uuid> {
        self.edges
            .iter()
            .filter(|e| e.to == function)
            .map(|e| e.from)
            .collect()
    }

    /// Topological sort (call order from entry points to leaves).
    pub fn topological_order(&self) -> Result<Vec<Uuid>> {
        use std::collections::VecDeque;

        let mut in_degree: HashMap<Uuid, usize> = self.nodes.keys().map(|id| (*id, 0)).collect();
        
        for edge in &self.edges {
            *in_degree.get_mut(&edge.to).unwrap() += 1;
        }
        
        let mut queue: VecDeque<Uuid> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| *id)
            .collect();
        
        let mut result = Vec::new();
        
        while let Some(node) = queue.pop_front() {
            result.push(node);
            
            for callee in self.callees(node) {
                let deg = in_degree.get_mut(&callee).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(callee);
                }
            }
        }
        
        if result.len() != self.nodes.len() {
            return Err(Error::AnalysisError("Call graph has cycles".into()));
        }
        
        Ok(result)
    }

    /// Detect recursive functions (nodes in SCC).
    pub fn recursive_functions(&self) -> HashSet<Uuid> {
        // Tarjan's algorithm for strongly connected components
        // (omitted for brevity — use petgraph::algo::tarjan_scc)
        HashSet::new()
    }
}
```

#### 13.1.2 Interprocedural CFG

**File:** `src/analysis/interprocedural_cfg.rs`

```rust
use crate::analysis::cfg::ControlFlowGraph;
use crate::analysis::callgraph::CallGraph;
use std::collections::HashMap;
use uuid::Uuid;

/// Interprocedural CFG combining multiple function CFGs.
#[derive(Debug, Clone)]
pub struct InterproceduralCFG {
    /// Per-function intraprocedural CFGs
    pub function_cfgs: HashMap<Uuid, ControlFlowGraph>,
    /// Call graph linking functions
    pub call_graph: CallGraph,
}

impl InterproceduralCFG {
    /// Build interprocedural CFG from source files.
    pub fn build(backend: &MemoryBackend, source_files: &HashMap<String, String>) -> Result<Self> {
        let call_graph = CallGraph::from_backend(backend)?;
        let mut function_cfgs = HashMap::new();
        
        // Build CFG for each function
        for (func_id, func_node) in &call_graph.nodes {
            if let Some(source) = source_files.get(&func_node.file_path) {
                let language = detect_language(&func_node.file_path);
                if let Ok(cfg) = build_cfg_for_function(language, source, &func_node.name) {
                    function_cfgs.insert(*func_id, cfg);
                }
            }
        }
        
        Ok(Self {
            function_cfgs,
            call_graph,
        })
    }

    /// Get CFG for a specific function.
    pub fn get_cfg(&self, function: Uuid) -> Option<&ControlFlowGraph> {
        self.function_cfgs.get(&function)
    }

    /// Follow call edges to get caller CFGs.
    pub fn caller_cfgs(&self, function: Uuid) -> Vec<(Uuid, &ControlFlowGraph)> {
        self.call_graph
            .callers(function)
            .into_iter()
            .filter_map(|caller_id| {
                self.function_cfgs.get(&caller_id).map(|cfg| (caller_id, cfg))
            })
            .collect()
    }
}
```

#### 13.1.3 Interprocedural Backward Slicing

**File:** `src/analysis/interprocedural_slicing.rs`

```rust
use crate::analysis::slicing::{CodeSlice, SliceCriterion};
use crate::analysis::interprocedural_cfg::InterproceduralCFG;
use crate::analysis::pdg::ProgramDependenceGraph;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

/// Interprocedural backward slicer.
pub struct InterproceduralSlicer<'a> {
    icfg: &'a InterproceduralCFG,
    /// Per-function PDGs
    pdgs: HashMap<Uuid, ProgramDependenceGraph>,
}

impl<'a> InterproceduralSlicer<'a> {
    /// Create interprocedural slicer with precomputed PDGs.
    pub fn new(
        icfg: &'a InterproceduralCFG,
        source_files: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut pdgs = HashMap::new();
        
        for (func_id, cfg) in &icfg.function_cfgs {
            let func_node = &icfg.call_graph.nodes[func_id];
            if let Some(source) = source_files.get(&func_node.file_path) {
                let pdg = ProgramDependenceGraph::build(cfg, source.as_bytes())?;
                pdgs.insert(*func_id, pdg);
            }
        }
        
        Ok(Self { icfg, pdgs })
    }

    /// Compute interprocedural backward slice.
    pub fn slice(&self, function: Uuid, criterion: SliceCriterion) -> Result<InterproceduralSlice> {
        let mut slice = HashSet::new();
        let mut worklist: VecDeque<(Uuid, Uuid)> = VecDeque::new();  // (function_id, pdg_node_id)
        let mut visited_functions = HashSet::new();
        
        // Start with intraprocedural slice in the target function
        let pdg = self.pdgs.get(&function)
            .ok_or_else(|| Error::NotFound(format!("PDG for function {:?}", function)))?;
        let cfg = self.icfg.get_cfg(function)
            .ok_or_else(|| Error::NotFound(format!("CFG for function {:?}", function)))?;
        
        let local_slicer = BackwardSlicer::new(pdg, cfg);
        let local_slice = local_slicer.slice(criterion)?;
        
        // Add all nodes from local slice
        for node_id in &local_slice.statements {
            slice.insert((function, *node_id));
            worklist.push_back((function, *node_id));
        }
        
        visited_functions.insert(function);
        
        // Expand across function boundaries
        while let Some((current_func, current_node)) = worklist.pop_front() {
            let current_pdg = &self.pdgs[&current_func];
            let node = &current_pdg.nodes[&current_node];
            
            // Check if this node uses variables from function parameters
            for var in &node.used_vars {
                if self.is_parameter(current_func, var) {
                    // Find all callers and slice them
                    for (caller_id, _caller_cfg) in self.icfg.caller_cfgs(current_func) {
                        if visited_functions.insert(caller_id) {
                            // Slice caller at call site
                            if let Some(caller_pdg) = self.pdgs.get(&caller_id) {
                                let call_site_nodes = self.find_call_site_nodes(caller_pdg, current_func);
                                for call_node_id in call_site_nodes {
                                    slice.insert((caller_id, call_node_id));
                                    worklist.push_back((caller_id, call_node_id));
                                }
                            }
                        }
                    }
                }
            }
        }
        
        let total_lines: HashSet<usize> = slice
            .iter()
            .filter_map(|(func_id, node_id)| {
                self.pdgs.get(func_id)
                    .and_then(|pdg| pdg.nodes.get(node_id))
                    .map(|n| n.statement.line)
            })
            .collect();
        
        let all_functions_lines = self.count_total_lines();
        let reduction_percent = 100.0 * (1.0 - (total_lines.len() as f64 / all_functions_lines as f64));
        
        Ok(InterproceduralSlice {
            criterion,
            statements: slice,
            lines: total_lines,
            functions: visited_functions,
            reduction_percent,
        })
    }

    fn is_parameter(&self, function: Uuid, variable: &str) -> bool {
        // Check if variable is a function parameter (simplified)
        let _ = (function, variable);
        false  // TODO: query graph for function signature
    }

    fn find_call_site_nodes(&self, caller_pdg: &ProgramDependenceGraph, callee_func: Uuid) -> Vec<Uuid> {
        // Find PDG nodes that call the callee function
        let callee_name = &self.icfg.call_graph.nodes[&callee_func].name;
        caller_pdg
            .nodes
            .iter()
            .filter(|(_, node)| node.statement.text.contains(callee_name))
            .map(|(id, _)| *id)
            .collect()
    }

    fn count_total_lines(&self) -> usize {
        self.icfg
            .function_cfgs
            .values()
            .flat_map(|cfg| {
                cfg.blocks
                    .values()
                    .flat_map(|block| block.statements.iter().map(|s| s.line))
            })
            .collect::<HashSet<_>>()
            .len()
    }
}

/// Result of interprocedural backward slicing.
#[derive(Debug, Clone)]
pub struct InterproceduralSlice {
    pub criterion: SliceCriterion,
    /// Statements in the slice (function_id, pdg_node_id)
    pub statements: HashSet<(Uuid, Uuid)>,
    /// Source lines in the slice
    pub lines: HashSet<usize>,
    /// Functions included in the slice
    pub functions: HashSet<Uuid>,
    /// Percentage of total code excluded
    pub reduction_percent: f64,
}
```

**Key Enhancement:** Interprocedural slicing achieves **95%+ reduction** vs 90% intraprocedural by eliminating entire unrelated functions.

#### 13.1.4 Test Requirements

```rust
#[cfg(feature = "lang-rust")]
#[test]
fn test_interprocedural_slice_cross_function() {
    let main_code = r#"
fn main() {
    let data = read_input();
    let result = process(data);
    write_output(result);
}
"#;
    
    let process_code = r#"
fn process(input: String) -> String {
    let unused_var = 42;  // Should be excluded
    let trimmed = input.trim();
    format!("Processed: {}", trimmed)
}
"#;
    
    // Build interprocedural CFG (requires source map)
    let sources = HashMap::from([
        ("main.rs".into(), format!("{}\n{}", main_code, process_code)),
    ]);
    
    // ... (setup ICFG, PDGs)
    
    let slicer = InterproceduralSlicer::new(&icfg, &sources).unwrap();
    let slice = slicer.slice(
        main_func_id,
        SliceCriterion {
            variable: "result".into(),
            line: 4,  // write_output(result)
        },
    ).unwrap();
    
    // Should include:
    // - Line 4 (write_output call)
    // - Line 3 (process call)
    // - Lines in process() that affect result (NOT unused_var)
    
    assert!(slice.functions.contains(&process_func_id));
    assert!(slice.reduction_percent > 90.0, "Should exclude unused code");
}
```

---

## 13.2 Dominance & Control Dependencies

**Effort:** 3-4 weeks  
**Complexity:** High  
**Research Reference:** Compiler textbooks (dominator tree, dominance frontiers)

### Overview

Phase 12 implemented **basic control dependencies** (placeholder). Phase 13 implements **precise control dependencies** using dominance analysis.

**Control Dependency:** Block B is control-dependent on block A if:
1. There exists a path from A to B
2. A has a branch where one path leads to B, another bypasses B

**Example:**

```rust
fn example(x: i32) -> i32 {
    let mut result = 0;
    if x > 0 {        // Block A (branch)
        result = x * 2;  // Block B (control-dependent on A)
    }
    result
}
```

Line 4 is control-dependent on line 3 because the branch at line 3 determines whether line 4 executes.

### Architecture

#### 13.2.1 Dominator Tree

**File:** `src/analysis/dominance.rs`

```rust
use crate::analysis::cfg::{BlockId, ControlFlowGraph};
use std::collections::{HashMap, HashSet};

/// Dominator tree representation.
#[derive(Debug, Clone)]
pub struct DominatorTree {
    /// Immediate dominator for each block (idom)
    pub idom: HashMap<BlockId, BlockId>,
    /// Dominance frontiers
    pub frontiers: HashMap<BlockId, HashSet<BlockId>>,
}

impl DominatorTree {
    /// Compute dominator tree using Lengauer-Tarjan algorithm.
    pub fn build(cfg: &ControlFlowGraph) -> Self {
        let mut idom = HashMap::new();
        let entry = cfg.entry;
        
        // Initialize: entry dominates itself
        idom.insert(entry, entry);
        
        // Iterative dataflow until fixed point
        let mut changed = true;
        while changed {
            changed = false;
            for (block_id, _block) in &cfg.blocks {
                if *block_id == entry {
                    continue;
                }
                
                let preds = cfg.predecessors(*block_id);
                if preds.is_empty() {
                    continue;
                }
                
                // New idom is intersection of predecessors' dominators
                let mut new_idom = preds[0];
                for pred in &preds[1..] {
                    new_idom = intersect(&idom, new_idom, *pred);
                }
                
                if idom.get(block_id) != Some(&new_idom) {
                    idom.insert(*block_id, new_idom);
                    changed = true;
                }
            }
        }
        
        // Compute dominance frontiers
        let frontiers = compute_dominance_frontiers(cfg, &idom);
        
        Self { idom, frontiers }
    }

    /// Returns true if `dominator` dominates `node`.
    pub fn dominates(&self, dominator: BlockId, node: BlockId) -> bool {
        if dominator == node {
            return true;
        }
        
        let mut current = node;
        while let Some(&idom) = self.idom.get(&current) {
            if idom == current {
                break;  // Reached entry
            }
            if idom == dominator {
                return true;
            }
            current = idom;
        }
        false
    }

    /// Get dominance frontier of a block.
    pub fn frontier(&self, block: BlockId) -> &HashSet<BlockId> {
        self.frontiers.get(&block).unwrap_or(&EMPTY_SET)
    }
}

fn intersect(idom: &HashMap<BlockId, BlockId>, mut b1: BlockId, mut b2: BlockId) -> BlockId {
    while b1 != b2 {
        while b1 < b2 {
            b1 = idom[&b1];
        }
        while b2 < b1 {
            b2 = idom[&b2];
        }
    }
    b1
}

fn compute_dominance_frontiers(
    cfg: &ControlFlowGraph,
    idom: &HashMap<BlockId, BlockId>,
) -> HashMap<BlockId, HashSet<BlockId>> {
    let mut frontiers: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();
    
    for (block, _) in &cfg.blocks {
        frontiers.insert(*block, HashSet::new());
    }
    
    for (block, _) in &cfg.blocks {
        let preds = cfg.predecessors(*block);
        if preds.len() >= 2 {
            for pred in preds {
                let mut runner = pred;
                while runner != idom[block] {
                    frontiers.get_mut(&runner).unwrap().insert(*block);
                    runner = idom[&runner];
                }
            }
        }
    }
    
    frontiers
}

static EMPTY_SET: HashSet<BlockId> = HashSet::new();
```

#### 13.2.2 Enhanced PDG with Precise Control Dependencies

**File:** `src/analysis/pdg.rs` (update existing)

```rust
// In ProgramDependenceGraph::build_control_dependencies
fn build_control_dependencies(&mut self, cfg: &ControlFlowGraph) {
    let dom_tree = DominatorTree::build(cfg);
    
    for (block_id, block) in &cfg.blocks {
        // Find all blocks in the dominance frontier
        for frontier_block in dom_tree.frontier(*block_id) {
            // All nodes in frontier_block are control-dependent on branch in block_id
            let controller_nodes = self.block_nodes.get(block_id).cloned().unwrap_or_default();
            let dependent_nodes = self.block_nodes.get(&frontier_block).cloned().unwrap_or_default();
            
            // Find the branch statement (last statement in block)
            if let Some(&controller) = controller_nodes.last() {
                for &dependent in &dependent_nodes {
                    self.control_deps.push(ControlDependency {
                        controller,
                        dependent,
                    });
                }
            }
        }
    }
}
```

#### 13.2.3 Test Requirements

```rust
#[cfg(feature = "lang-rust")]
#[test]
fn test_dominance_tree_simple() {
    let code = r#"
fn test(x: i32) -> i32 {
    if x > 0 {
        return x * 2;
    }
    0
}
"#;
    let cfg = build_cfg_for_function("rust", code, "test").unwrap();
    let dom_tree = DominatorTree::build(&cfg);
    
    // Entry should dominate all blocks
    for block in cfg.blocks.keys() {
        assert!(dom_tree.dominates(cfg.entry, *block));
    }
}

#[cfg(feature = "lang-rust")]
#[test]
fn test_control_dependency_precision() {
    let code = r#"
fn test(x: i32, y: i32) -> i32 {
    let mut result = 0;
    if x > 0 {
        result = x * 2;
    }
    if y > 0 {
        result += y;
    }
    result
}
"#;
    let cfg = build_cfg_for_function("rust", code, "test").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    
    // Find node for "result = x * 2"
    let x_node = pdg.nodes.values()
        .find(|n| n.statement.text.contains("x * 2"))
        .unwrap();
    
    // Should be control-dependent on "if x > 0"
    let controlling = pdg.control_deps.iter()
        .filter(|cd| cd.dependent == x_node.id)
        .collect::<Vec<_>>();
    
    assert_eq!(controlling.len(), 1, "Should have exactly one control dependency");
}
```

---

## 13.3 Type Inference

**Effort:** 4-5 weeks  
**Complexity:** High  
**Research Reference:** Type inference for Python, JavaScript (gradual typing)

### Overview

Dynamic languages (Python, JavaScript, Ruby) lack type annotations. Type inference improves analysis precision by deducing variable types from usage.

**Benefits:**
- Better taint analysis (know if variable is string, int, etc.)
- Improved slicing (distinguish data vs code)
- Enhanced query results (filter by inferred type)

**Example:**

```python
def process(data):
    # Infer: data is List[str] from usage
    result = []
    for item in data:
        result.append(item.upper())  # item is str (has .upper())
    return result  # Return type: List[str]
```

### Architecture

#### 13.3.1 Type Inference Engine

**File:** `src/analysis/type_inference.rs`

```rust
use std::collections::HashMap;
use crate::analysis::cfg::ControlFlowGraph;
use crate::analysis::pdg::ProgramDependenceGraph;
use uuid::Uuid;

/// Inferred type information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferredType {
    /// Scalar types
    Int,
    Float,
    String,
    Bool,
    None,
    
    /// Container types
    List(Box<InferredType>),
    Dict(Box<InferredType>, Box<InferredType>),
    Tuple(Vec<InferredType>),
    
    /// Function type
    Function {
        params: Vec<InferredType>,
        return_type: Box<InferredType>,
    },
    
    /// Unknown type
    Unknown,
    
    /// Union type (Python: str | int)
    Union(Vec<InferredType>),
}

/// Type inference result for a variable.
#[derive(Debug, Clone)]
pub struct VariableType {
    pub variable: String,
    pub inferred_type: InferredType,
    pub confidence: f64,  // 0.0-1.0
}

/// Type inference engine.
pub struct TypeInferenceEngine<'a> {
    pdg: &'a ProgramDependenceGraph,
    cfg: &'a ControlFlowGraph,
    language: &'a str,
    /// Variable types (pdg_node_id -> variable -> type)
    types: HashMap<Uuid, HashMap<String, InferredType>>,
}

impl<'a> TypeInferenceEngine<'a> {
    pub fn new(
        pdg: &'a ProgramDependenceGraph,
        cfg: &'a ControlFlowGraph,
        language: &'a str,
    ) -> Self {
        Self {
            pdg,
            cfg,
            language,
            types: HashMap::new(),
        }
    }

    /// Infer types for all variables in the PDG.
    pub fn infer(&mut self) -> Vec<VariableType> {
        match self.language {
            "python" | "py" => self.infer_python(),
            "javascript" | "js" | "typescript" | "ts" => self.infer_javascript(),
            _ => Vec::new(),
        }
    }

    fn infer_python(&mut self) -> Vec<VariableType> {
        let mut results = Vec::new();
        
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;
            let mut node_types = HashMap::new();
            
            // Pattern: x = 42
            if text.contains(" = ") && text.chars().filter(|&c| c.is_ascii_digit()).count() > 0 {
                for var in &node.defined_vars {
                    if text.contains(&format!("{var} =")) {
                        if text.matches('"').count() >= 2 || text.matches('\'').count() >= 2 {
                            node_types.insert(var.clone(), InferredType::String);
                        } else if text.contains('.') && text.chars().any(|c| c.is_ascii_digit()) {
                            node_types.insert(var.clone(), InferredType::Float);
                        } else if text.chars().any(|c| c.is_ascii_digit()) {
                            node_types.insert(var.clone(), InferredType::Int);
                        }
                    }
                }
            }
            
            // Pattern: x = []
            if text.contains(" = []") {
                for var in &node.defined_vars {
                    node_types.insert(var.clone(), InferredType::List(Box::new(InferredType::Unknown)));
                }
            }
            
            // Pattern: x = {}
            if text.contains(" = {}") {
                for var in &node.defined_vars {
                    node_types.insert(
                        var.clone(),
                        InferredType::Dict(Box::new(InferredType::Unknown), Box::new(InferredType::Unknown)),
                    );
                }
            }
            
            // Pattern: for item in collection (infer item type from collection)
            if text.contains("for ") && text.contains(" in ") {
                // Simplified: assume iterable yields Unknown
                for var in &node.defined_vars {
                    node_types.insert(var.clone(), InferredType::Unknown);
                }
            }
            
            // Pattern: x.append(...) => x is List
            if text.contains(".append(") {
                for var in &node.used_vars {
                    if text.contains(&format!("{var}.append")) {
                        node_types.insert(var.clone(), InferredType::List(Box::new(InferredType::Unknown)));
                    }
                }
            }
            
            // Pattern: x.upper() => x is String
            if text.contains(".upper(") || text.contains(".lower(") || text.contains(".strip(") {
                for var in &node.used_vars {
                    if text.contains(&format!("{var}.upper"))
                        || text.contains(&format!("{var}.lower"))
                        || text.contains(&format!("{var}.strip"))
                    {
                        node_types.insert(var.clone(), InferredType::String);
                    }
                }
            }
            
            self.types.insert(*node_id, node_types.clone());
            
            for (var, typ) in node_types {
                results.push(VariableType {
                    variable: var,
                    inferred_type: typ,
                    confidence: 0.8,  // Pattern-based: medium confidence
                });
            }
        }
        
        results
    }

    fn infer_javascript(&mut self) -> Vec<VariableType> {
        let mut results = Vec::new();
        
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;
            let mut node_types = HashMap::new();
            
            // Pattern: const x = "..."
            if (text.contains("const ") || text.contains("let ") || text.contains("var "))
                && text.contains(" = ")
            {
                for var in &node.defined_vars {
                    if text.contains('"') || text.contains('\'') || text.contains('`') {
                        node_types.insert(var.clone(), InferredType::String);
                    } else if text.chars().any(|c| c.is_ascii_digit()) && !text.contains('"') {
                        if text.contains('.') {
                            node_types.insert(var.clone(), InferredType::Float);
                        } else {
                            node_types.insert(var.clone(), InferredType::Int);
                        }
                    } else if text.contains("true") || text.contains("false") {
                        node_types.insert(var.clone(), InferredType::Bool);
                    } else if text.contains("[") && text.contains("]") {
                        node_types.insert(var.clone(), InferredType::List(Box::new(InferredType::Unknown)));
                    } else if text.contains("{") && text.contains("}") {
                        node_types.insert(
                            var.clone(),
                            InferredType::Dict(Box::new(InferredType::Unknown), Box::new(InferredType::Unknown)),
                        );
                    }
                }
            }
            
            // Pattern: x.push(...) => x is Array
            if text.contains(".push(") {
                for var in &node.used_vars {
                    if text.contains(&format!("{var}.push")) {
                        node_types.insert(var.clone(), InferredType::List(Box::new(InferredType::Unknown)));
                    }
                }
            }
            
            // Pattern: x.toUpperCase() => x is String
            if text.contains(".toUpperCase(") || text.contains(".toLowerCase(") {
                for var in &node.used_vars {
                    if text.contains(&format!("{var}.toUpperCase")) || text.contains(&format!("{var}.toLowerCase")) {
                        node_types.insert(var.clone(), InferredType::String);
                    }
                }
            }
            
            self.types.insert(*node_id, node_types.clone());
            
            for (var, typ) in node_types {
                results.push(VariableType {
                    variable: var,
                    inferred_type: typ,
                    confidence: 0.75,
                });
            }
        }
        
        results
    }

    /// Get inferred type for a variable at a specific node.
    pub fn get_type(&self, node_id: Uuid, variable: &str) -> Option<&InferredType> {
        self.types.get(&node_id)?.get(variable)
    }
}
```

#### 13.3.2 Integration with Taint Analysis

Update taint analysis to use type information:

```rust
// In TaintAnalyzer::find_sanitizers_on_path
fn find_sanitizers_on_path(&self, path: &[Uuid], variable: &str) -> Vec<Sanitizer> {
    let mut sanitizers = Vec::new();
    
    for node_id in path {
        // Existing pattern-based detection
        if let Some(san) = self.sanitizers.get(node_id) {
            sanitizers.push(san.clone());
        }
        
        // NEW: Type-based sanitization
        if let Some(typ) = self.type_inference.get_type(*node_id, variable) {
            match typ {
                InferredType::Int | InferredType::Float => {
                    // Numeric types are safe for SQL
                    sanitizers.push(Sanitizer::TypeCast("numeric".into()));
                }
                InferredType::Bool => {
                    sanitizers.push(Sanitizer::TypeCast("boolean".into()));
                }
                _ => {}
            }
        }
    }
    
    sanitizers
}
```

#### 13.3.3 Test Requirements

```rust
#[cfg(feature = "lang-python")]
#[test]
fn test_type_inference_python_literals() {
    let code = r#"
def example():
    x = 42
    y = "hello"
    z = 3.14
    items = []
"#;
    
    let cfg = build_cfg_for_function("python", code, "example").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut engine = TypeInferenceEngine::new(&pdg, &cfg, "python");
    
    let types = engine.infer();
    
    assert!(types.iter().any(|t| t.variable == "x" && t.inferred_type == InferredType::Int));
    assert!(types.iter().any(|t| t.variable == "y" && t.inferred_type == InferredType::String));
    assert!(types.iter().any(|t| t.variable == "z" && t.inferred_type == InferredType::Float));
}

#[cfg(feature = "lang-python")]
#[test]
fn test_type_inference_method_calls() {
    let code = r#"
def process(data):
    upper = data.upper()
    items = []
    items.append("test")
"#;
    
    let cfg = build_cfg_for_function("python", code, "process").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut engine = TypeInferenceEngine::new(&pdg, &cfg, "python");
    
    let types = engine.infer();
    
    // data.upper() implies data is String
    assert!(types.iter().any(|t| t.variable == "data" && t.inferred_type == InferredType::String));
    
    // items.append(...) implies items is List
    assert!(types.iter().any(|t| {
        t.variable == "items" && matches!(t.inferred_type, InferredType::List(_))
    }));
}
```

---

## 13.4 GQL Query Optimizer

**Effort:** 3-4 weeks  
**Complexity:** Medium-High  
**Research Reference:** Database query optimization (Selinger, System R)

### Overview

Phase 12 implemented GQL execution but without optimization. Queries execute in the order written, which can be inefficient.

**Example Inefficiency:**

```cypher
MATCH (f:Function)-[:CALLS*1..3]->(g:Function)
WHERE f.name = "main" AND g.complexity > 20
RETURN f, g
```

**Naive execution:**
1. Find all Functions (1000 nodes)
2. Expand CALLS up to 3 hops (10,000 paths)
3. Filter by f.name = "main" (1 node remaining)
4. Filter by g.complexity > 20

**Optimized execution:**
1. Filter by f.name = "main" (1 node) — **selectivity-based reordering**
2. Expand CALLS from main (10 paths)
3. Filter by g.complexity > 20 (2 nodes remaining)

Result: 1000x faster.

### Architecture

#### 13.4.1 Query Optimizer

**File:** `src/gql/optimizer.rs`

```rust
use crate::gql::ast::{Pattern, Predicate, Query};
use crate::graph::backend::MemoryBackend;

/// Query optimization context.
pub struct QueryOptimizer<'a> {
    backend: &'a MemoryBackend,
}

impl<'a> QueryOptimizer<'a> {
    pub fn new(backend: &'a MemoryBackend) -> Self {
        Self { backend }
    }

    /// Optimize query by reordering predicates and patterns.
    pub fn optimize(&self, query: Query) -> Query {
        let mut optimized = query;
        
        // Step 1: Predicate pushdown (apply filters early)
        optimized = self.push_down_predicates(optimized);
        
        // Step 2: Join reordering (start with most selective patterns)
        optimized = self.reorder_patterns(optimized);
        
        // Step 3: Index selection (use indexed properties)
        optimized = self.select_indexes(optimized);
        
        optimized
    }

    fn push_down_predicates(&self, mut query: Query) -> Query {
        // Move WHERE predicates to inline node/edge patterns
        if let Some(where_clause) = query.where_clause.take() {
            for predicate in where_clause.predicates {
                match predicate {
                    Predicate::Equals { variable, property, value } => {
                        // Find pattern with this variable
                        for pattern in &mut query.patterns {
                            if pattern.node.variable == variable {
                                pattern.node.properties.insert(
                                    property,
                                    crate::gql::ast::PropertyMatcher::Equals(value.clone()),
                                );
                            }
                            for (_, target) in &mut pattern.hops {
                                if target.variable == variable {
                                    target.properties.insert(
                                        property.clone(),
                                        crate::gql::ast::PropertyMatcher::Equals(value.clone()),
                                    );
                                }
                            }
                        }
                    }
                    _ => {
                        // Keep complex predicates in WHERE
                        query.where_clause = Some(crate::gql::ast::WhereClause {
                            predicates: vec![predicate],
                        });
                    }
                }
            }
        }
        query
    }

    fn reorder_patterns(&self, mut query: Query) -> Query {
        // Estimate selectivity of each pattern
        let mut selectivities: Vec<(usize, f64)> = query
            .patterns
            .iter()
            .enumerate()
            .map(|(idx, pattern)| {
                let selectivity = self.estimate_selectivity(pattern);
                (idx, selectivity)
            })
            .collect();
        
        // Sort by selectivity (lowest = most selective = best to start with)
        selectivities.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        
        // Reorder patterns
        let original_patterns = query.patterns.clone();
        query.patterns = selectivities
            .into_iter()
            .map(|(idx, _)| original_patterns[idx].clone())
            .collect();
        
        query
    }

    fn estimate_selectivity(&self, pattern: &Pattern) -> f64 {
        let total_nodes = self.backend.node_count() as f64;
        
        // Estimate based on node type filter
        let type_selectivity = if let Some(node_type) = pattern.node.node_type {
            let type_count = self.backend
                .nodes()
                .iter()
                .filter(|n| n.node_type == node_type)
                .count() as f64;
            type_count / total_nodes
        } else {
            1.0  // No filter = 100% selectivity
        };
        
        // Estimate based on property filters
        let prop_selectivity = if pattern.node.properties.is_empty() {
            1.0
        } else {
            0.1  // Assume properties are 10% selective (heuristic)
        };
        
        type_selectivity * prop_selectivity
    }

    fn select_indexes(&self, query: Query) -> Query {
        // Future: mark which properties should use indexes
        query
    }
}
```

#### 13.4.2 Explain Plan Enhancement

Update `src/gql/explain.rs`:

```rust
pub struct ExplainPlan {
    pub steps: Vec<ExplainStep>,
    pub estimated_cost: f64,
    pub optimizer_applied: bool,
    /// Optimization decisions made
    pub optimizations: Vec<String>,
}

impl ExplainPlan {
    pub fn add_optimization(&mut self, description: String) {
        self.optimizations.push(description);
        self.optimizer_applied = true;
    }
}
```

#### 13.4.3 Test Requirements

```rust
#[test]
fn test_optimizer_predicate_pushdown() {
    let query = "MATCH (f:Function) WHERE f.name = 'main' RETURN f";
    let parsed = parse(query).unwrap();
    
    let backend = MemoryBackend::new();
    let optimizer = QueryOptimizer::new(&backend);
    let optimized = optimizer.optimize(parsed);
    
    // Predicate should be pushed into node pattern
    assert!(optimized.patterns[0].node.properties.contains_key("name"));
    assert!(optimized.where_clause.is_none());
}

#[test]
fn test_optimizer_reorders_by_selectivity() {
    let backend = sample_graph_1000_nodes();
    
    // Query with inefficient order: broad pattern first, narrow pattern second
    let query = "MATCH (a:Function), (b:Function) WHERE b.name = 'rare_function' RETURN a, b";
    let parsed = parse(query).unwrap();
    
    let optimizer = QueryOptimizer::new(&backend);
    let optimized = optimizer.optimize(parsed);
    
    // Optimizer should reorder: narrow pattern first
    assert_eq!(optimized.patterns[0].node.properties.get("name"), Some(&PropertyMatcher::Equals("rare_function".into())));
}

#[test]
fn test_explain_shows_optimizations() {
    let backend = sample_graph();
    let query = "MATCH (f:Function) WHERE f.name = 'main' RETURN f";
    
    let result = execute_explain(&backend, query).unwrap();
    let plan = result.plan.unwrap();
    
    assert!(plan.optimizer_applied);
    assert!(!plan.optimizations.is_empty());
    assert!(plan.optimizations.iter().any(|o| o.contains("predicate pushdown")));
}
```

---

## 13.5 Security Context & CVE Patterns

**Effort:** 2-3 weeks  
**Complexity:** Medium  
**Research Reference:** CWE (Common Weakness Enumeration), OWASP Top 10

### Overview

Enhance taint analysis with **known vulnerability patterns** (CVE database integration).

**Example Patterns:**
- CWE-89: SQL Injection
- CWE-79: Cross-Site Scripting (XSS)
- CWE-78: OS Command Injection
- CWE-22: Path Traversal
- CWE-798: Hardcoded Credentials

### Architecture

#### 13.5.1 CVE Pattern Database

**File:** `src/security/cve_patterns.rs`

```rust
use serde::{Deserialize, Serialize};

/// Common Weakness Enumeration pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CwePattern {
    /// CWE identifier (e.g., "CWE-89")
    pub cwe_id: String,
    /// Pattern name
    pub name: String,
    /// Description
    pub description: String,
    /// Severity (1-10)
    pub severity: u8,
    /// Source patterns (regex)
    pub source_patterns: Vec<String>,
    /// Sink patterns (regex)
    pub sink_patterns: Vec<String>,
    /// Sanitizer patterns (regex)
    pub sanitizer_patterns: Vec<String>,
}

pub fn default_cwe_patterns() -> Vec<CwePattern> {
    vec![
        CwePattern {
            cwe_id: "CWE-89".into(),
            name: "SQL Injection".into(),
            description: "Improper neutralization of special elements in SQL commands".into(),
            severity: 10,
            source_patterns: vec![
                r"request\.(GET|POST|query|body)".into(),
                r"req\.(query|params|body)".into(),
            ],
            sink_patterns: vec![
                r"\.execute\(.*\)".into(),
                r"\.query\(.*\)".into(),
                r"cursor\.(execute|executemany)".into(),
            ],
            sanitizer_patterns: vec![
                r"int\(.*\)".into(),
                r"parseInt\(.*\)".into(),
                r"prepare\(.*\)".into(),
            ],
        },
        CwePattern {
            cwe_id: "CWE-79".into(),
            name: "Cross-Site Scripting (XSS)".into(),
            description: "Improper neutralization of input during web page generation".into(),
            severity: 9,
            source_patterns: vec![
                r"request\.(GET|POST)".into(),
                r"req\.(query|body)".into(),
            ],
            sink_patterns: vec![
                r"innerHTML".into(),
                r"document\.write".into(),
                r"\.html\(.*\)".into(),
            ],
            sanitizer_patterns: vec![
                r"escape\(.*\)".into(),
                r"sanitize\(.*\)".into(),
                r"html\.escape".into(),
            ],
        },
        CwePattern {
            cwe_id: "CWE-78".into(),
            name: "OS Command Injection".into(),
            description: "Improper neutralization of special elements in OS commands".into(),
            severity: 10,
            source_patterns: vec![
                r"request\.GET".into(),
                r"req\.query".into(),
                r"sys\.argv".into(),
            ],
            sink_patterns: vec![
                r"os\.system\(.*\)".into(),
                r"subprocess\.(call|run|Popen)".into(),
                r"exec\(.*\)".into(),
            ],
            sanitizer_patterns: vec![
                r"shlex\.quote".into(),
                r"shellEscape\(.*\)".into(),
            ],
        },
        CwePattern {
            cwe_id: "CWE-22".into(),
            name: "Path Traversal".into(),
            description: "Improper limitation of pathname to a restricted directory".into(),
            severity: 8,
            source_patterns: vec![
                r"request\.(GET|POST)".into(),
                r"req\.(query|params)".into(),
            ],
            sink_patterns: vec![
                r"open\(.*\)".into(),
                r"fs\.readFile".into(),
                r"File::open".into(),
            ],
            sanitizer_patterns: vec![
                r"os\.path\.basename".into(),
                r"path\.basename".into(),
                r"sanitize_path\(.*\)".into(),
            ],
        },
        // Add more patterns...
    ]
}
```

#### 13.5.2 Security Analyzer

**File:** `src/security/analyzer.rs`

```rust
use crate::analysis::{TaintAnalyzer, TaintFlow};
use crate::security::cve_patterns::{CwePattern, default_cwe_patterns};
use regex::Regex;

/// Security vulnerability report.
#[derive(Debug, Clone)]
pub struct SecurityVulnerability {
    pub cwe_id: String,
    pub cwe_name: String,
    pub severity: u8,
    pub taint_flow: TaintFlow,
    pub source_line: usize,
    pub sink_line: usize,
    pub recommendation: String,
}

pub struct SecurityAnalyzer {
    cwe_patterns: Vec<CwePattern>,
}

impl SecurityAnalyzer {
    pub fn new() -> Self {
        Self {
            cwe_patterns: default_cwe_patterns(),
        }
    }

    /// Analyze taint flows against CWE patterns.
    pub fn analyze(&self, taint_flows: Vec<TaintFlow>, source: &str) -> Vec<SecurityVulnerability> {
        let mut vulnerabilities = Vec::new();
        
        for flow in taint_flows {
            if !flow.is_vulnerable() {
                continue;  // Skip sanitized flows
            }
            
            // Match against CWE patterns
            for pattern in &self.cwe_patterns {
                if self.matches_cwe(&flow, pattern, source) {
                    vulnerabilities.push(SecurityVulnerability {
                        cwe_id: pattern.cwe_id.clone(),
                        cwe_name: pattern.name.clone(),
                        severity: pattern.severity,
                        taint_flow: flow.clone(),
                        source_line: 0,  // TODO: extract from PDG
                        sink_line: 0,
                        recommendation: self.generate_recommendation(pattern),
                    });
                    break;  // One CWE per flow
                }
            }
        }
        
        vulnerabilities
    }

    fn matches_cwe(&self, flow: &TaintFlow, pattern: &CwePattern, source: &str) -> bool {
        // Check if flow matches CWE source and sink patterns
        let _ = (flow, pattern, source);
        true  // Simplified
    }

    fn generate_recommendation(&self, pattern: &CwePattern) -> String {
        match pattern.cwe_id.as_str() {
            "CWE-89" => "Use parameterized queries or prepared statements instead of string concatenation.".into(),
            "CWE-79" => "Escape HTML entities before rendering user input.".into(),
            "CWE-78" => "Use shell escape functions or avoid shell execution entirely.".into(),
            "CWE-22" => "Validate file paths and restrict to allowed directories.".into(),
            _ => "Review and sanitize input before use.".into(),
        }
    }
}
```

#### 13.5.3 MCP Integration

```rust
fn security_scan(&self, file: &str, function: Option<&str>, verbose: bool) -> Result<Value> {
    // Run taint analysis
    let taint_result = self.taint_analysis(...)?;
    let flows: Vec<TaintFlow> = serde_json::from_value(taint_result["flows"].clone())?;
    
    // Run security analysis
    let source = std::fs::read_to_string(file)?;
    let analyzer = SecurityAnalyzer::new();
    let vulnerabilities = analyzer.analyze(flows, &source);
    
    Ok(json!({
        "file": file,
        "total_vulnerabilities": vulnerabilities.len(),
        "critical": vulnerabilities.iter().filter(|v| v.severity >= 9).count(),
        "high": vulnerabilities.iter().filter(|v| v.severity >= 7 && v.severity < 9).count(),
        "vulnerabilities": vulnerabilities.iter().map(|v| json!({
            "cwe_id": v.cwe_id,
            "cwe_name": v.cwe_name,
            "severity": v.severity,
            "variable": v.taint_flow.variable,
            "recommendation": v.recommendation,
        })).collect::<Vec<_>>(),
    }))
}
```

---

## Critical Dependencies

**Dependency Graph:**

```
13.0 Taint Analysis
├─ Requires: Phase 12.1 (CFG/PDG)
└─ Optional: 13.3 (Type Inference) for better sanitizer detection

13.1 Interprocedural Analysis
├─ Requires: Phase 12.1 (CFG/PDG)
├─ Requires: Phase 12 graph backend
└─ Blocks: Nothing (can run in parallel with others)

13.2 Dominance Analysis
├─ Requires: Phase 12.1 (CFG)
└─ Enhances: Phase 12 slicing precision

13.3 Type Inference
├─ Requires: Phase 12.1 (CFG/PDG)
└─ Enhances: 13.0 (Taint Analysis)

13.4 GQL Optimizer
├─ Requires: Phase 12.4 (GQL)
└─ Blocks: Nothing

13.5 Security Context
├─ Requires: 13.0 (Taint Analysis)
└─ Blocks: Nothing
```

**Recommended Execution Order:**

1. **Week 1-4:** 13.2 Dominance + 13.3 Type Inference (parallel)
2. **Week 5-9:** 13.0 Taint Analysis (uses Type Inference)
3. **Week 10-15:** 13.1 Interprocedural Analysis (large, parallel track)
4. **Week 12-15:** 13.4 GQL Optimizer (parallel with 13.1)
5. **Week 16-18:** 13.5 Security Context (uses Taint Analysis)

**Critical Path:** 13.3 → 13.0 → 13.5 (10 weeks)

---

## Milestones

### M1: Dominance & Type Inference (Week 4)
- ✅ Dominator tree construction
- ✅ Dominance frontiers
- ✅ Enhanced control dependencies in PDG
- ✅ Type inference for Python, JavaScript
- ✅ Tests: 15+ tests for dominance, 20+ tests for type inference

### M2: Taint Analysis (Week 9)
- ✅ Taint source/sink/sanitizer detection
- ✅ Forward data flow tracking
- ✅ Multi-language support (Python, JavaScript, Rust)
- ✅ MCP tool: `taint_analysis`
- ✅ Tests: 25+ tests covering SQL injection, XSS, command injection

### M3: Interprocedural Analysis (Week 15)
- ✅ Call graph construction
- ✅ Interprocedural CFG
- ✅ Interprocedural backward slicing
- ✅ 95%+ code reduction target
- ✅ Tests: 20+ tests for call graph, slicing

### M4: GQL Optimizer (Week 15, parallel)
- ✅ Predicate pushdown
- ✅ Join reordering
- ✅ Selectivity estimation
- ✅ Enhanced explain plans
- ✅ Tests: 15+ optimizer tests

### M5: Security Context (Week 18)
- ✅ CWE pattern database
- ✅ Security analyzer
- ✅ MCP tool: `security_scan`
- ✅ Integration with taint analysis
- ✅ Tests: 10+ security pattern tests

---

## Testing Strategy

### Unit Tests
- **Taint Analysis:** 25 tests (sources, sinks, sanitizers, multi-language)
- **Interprocedural:** 20 tests (call graph, slicing, edge cases)
- **Dominance:** 15 tests (dominator tree, frontiers, control deps)
- **Type Inference:** 20 tests (Python, JS, method calls, containers)
- **GQL Optimizer:** 15 tests (reordering, pushdown, explain)
- **Security:** 10 tests (CWE patterns, recommendations)

**Total:** 105 new unit tests

### Integration Tests
- End-to-end security scan (source file → vulnerabilities)
- Interprocedural slicing across 3+ functions
- Taint analysis with type inference integration
- GQL query optimization benchmarks

### Performance Benchmarks
- Taint analysis: <2s for 1000-line function
- Interprocedural slicing: <5s for 10-function call chain
- GQL optimizer: 50%+ speedup on complex queries

---

## Success Criteria

### Functional Requirements
- ✅ Taint analysis detects 95%+ of OWASP Top 10 patterns
- ✅ Interprocedural slicing reduces code by 95%+ (vs 90% intraprocedural)
- ✅ Dominance analysis improves slice precision by 15%+
- ✅ Type inference covers Python, JavaScript, Ruby
- ✅ GQL optimizer reduces query time by 50%+ on large graphs (1000+ nodes)
- ✅ Security scanner identifies CWE patterns with recommendations

### Technical Requirements
- ✅ Zero new external dependencies (Rust-native only)
- ✅ All tests pass (105+ new tests)
- ✅ No compilation warnings
- ✅ Documentation for each component

### Performance Requirements
- ✅ Taint analysis: <2s for 1000 LOC
- ✅ Interprocedural slicing: <5s for 10-function chain
- ✅ Type inference: <1s for 500 LOC
- ✅ GQL optimization: negligible overhead (<10ms)

---

## Common Pitfalls

### 1. Taint Analysis False Positives
**Problem:** Over-taint (marking too many flows as vulnerable)  
**Solution:** Use type inference to detect implicit sanitizers (int() casts, etc.)

### 2. Interprocedural Cycles
**Problem:** Recursive functions cause infinite loops  
**Solution:** Detect SCCs (strongly connected components) and limit recursion depth

### 3. Dominance Complexity
**Problem:** Lengauer-Tarjan algorithm is complex  
**Solution:** Use iterative dataflow (simpler, sufficient for small CFGs)

### 4. Type Inference Precision
**Problem:** Dynamic languages have ambiguous types  
**Solution:** Use confidence scores; prefer "Unknown" over wrong type

### 5. GQL Optimizer Regression
**Problem:** Optimization makes query results wrong  
**Solution:** Test optimizer against baseline executor (results must match)

---

## Next Steps for Cursor

1. **Read this guide thoroughly**
2. **Start with Milestone 1** (Dominance + Type Inference)
   - Implement `src/analysis/dominance.rs`
   - Implement `src/analysis/type_inference.rs`
   - Write tests as you go
3. **Use Phase 12 as a template** (same code quality, test coverage, documentation)
4. **Ask questions if architecture is unclear** (create GitHub issues)
5. **Commit frequently** (one feature per commit)

**Estimated Timeline:**
- **With parallelization:** 8-10 weeks
- **Serial implementation:** 16-20 weeks

**Target Grade:** A+ (match Phase 12 quality)

---

## Conclusion

Phase 13 transforms rBuilder from a **code understanding tool** into a **security analysis platform** by adding:
- Forward data flow tracking (taint analysis)
- Whole-program analysis (interprocedural)
- Precise control dependencies (dominance)
- Type awareness (inference)
- Query optimization (performance)
- Vulnerability detection (security)

These capabilities close the research gaps identified in RESEARCH_GAP_ANALYSIS.md and position rBuilder as a **research-grade program analysis system** competitive with Codebadger and CodexGraph.

**Critical Success Factor:** Maintain the same code quality standard from Phase 12 (clean architecture, comprehensive tests, zero warnings).

Good luck, Cursor! 🚀
