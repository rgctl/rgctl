# rBuilder vs. Research Systems: Gap Analysis

**Date**: June 17, 2026  
**Research Papers Analyzed**:
- **Codebadger** (2026): Bridging Code Property Graphs and Language Models for Program Analysis
- **CodexGraph** (NAACL 2025): Bridging Large Language Models and Code Repositories via Code Graph Databases

---

## Executive Summary

This document analyzes the gaps between rBuilder's current implementation and two state-of-the-art research systems. The analysis informed the Phase 12 updates to the task plan.

**Key Finding**: rBuilder has a solid foundation but lacks critical semantic analysis capabilities (CFG/PDG) and advanced query systems (dual-agent translation, graph query language) needed for production-grade code reasoning.

---

## 1. Graph Schema & Representation

### Codebadger Approach
- **Code Property Graph (CPG)**: Unified structure combining:
  - Abstract Syntax Trees (AST) - syntactic structure
  - Control Flow Graphs (CFG) - execution paths
  - Program Dependence Graphs (PDG) - data & control dependencies
- **Engine**: Uses Joern's CPG engine
- **Enables**: Semantic reasoning about program behavior, not just structure

### CodexGraph Approach
- **Property Graph**: Stored in Neo4j with Cypher query interface
- **Node Types**: MODULE, CLASS, METHOD, FUNCTION, FIELD, GLOBAL_VARIABLE
- **Rich Attributes**:
  - METHOD nodes: `name`, `file_path`, `class`, `code` (indexed), `signature`
  - CLASS nodes: `name`, `file_path`, `signature`, `full code`
- **Edge Types**: CONTAINS, HAS_METHOD, HAS_FIELD, INHERITS, USES (with `source/target type` attributes)
- **Two-phase indexing**:
  1. Shallow intra-file indexing (AST parsing)
  2. Cross-file analysis (DFS for imports/inheritance)

### rBuilder Current State
✅ **Strengths**:
- Similar node types: `Function`, `Class`, `Module`, `Struct`, `Enum`, `Interface`, etc.
- Similar edge types: `Calls`, `Contains`, `Uses`, `Implements`, `Extends`, `References`
- Clean Rust implementation with UUID-based node IDs
- Properties map for extensibility

❌ **Gaps**:
1. **No CFG**: Cannot reason about execution paths, loops, conditionals
2. **No PDG**: Cannot track data flow or control dependencies
3. **Missing signature as first-class field**: Stored in properties map, not schema-level
4. **No code hash indexing**: Can't efficiently detect if code changed
5. **Limited edge properties**: No call type (direct/indirect), no access type (read/write)

**Impact**: rBuilder can answer "what calls X?" but NOT "what data flows from A to B?" or "what's the execution path from entry to sink?"

---

## 2. LLM Integration Strategy

### Codebadger Approach: MCP Tool Abstraction
- **Philosophy**: LLM doesn't write queries; instead calls high-level semantic tools
- **Tool Examples**:
  - `taint_flow(source, sink)` - finds data propagation paths
  - `backward_slice(criterion)` - upstream dependency analysis
  - `find_bounds_checks(variable)` - security verification
  - `call_graph(method)` - call graph construction
- **Interface**: Model Context Protocol (MCP) with FastMCP + async processing
- **Caching**: Redis-based caching, CPGs cached by source hash
- **Key Insight**: Abstract graph operations into semantic tools, not raw queries

### CodexGraph Approach: Dual-Agent System
- **Architecture**: "Write Then Translate"
  1. **Primary Agent**: High-level reasoning, decomposes questions into natural language sub-queries
  2. **Translation Agent**: Converts NL → Cypher graph queries
- **Iterative Refinement**:
  - Formulates multiple queries per round
  - Analyzes aggregated results
  - Determines if sufficient context gathered
  - Continues or concludes
- **Performance**: Ablation study shows removing translation agent drops accuracy from **27.9% → 8.3%** (3.4x degradation)
- **Token Cost**: Higher than baselines (22.16k tokens vs BM25's 1.47k), but better accuracy

### rBuilder Current State
✅ **Strengths**:
- MCP tool interface implemented (`query_codebase`, `impact_analysis`, `symbol_info`)
- Pattern matching with selectivity ranking (`type:Function|name:foo`)
- Analysis caching (5-minute TTL for complexity/community)
- Clean separation: query layer → backend layer

❌ **Gaps**:
1. **No dual-agent architecture**: LLM must understand our pattern syntax directly
2. **No translation layer**: No mapping from NL → optimized patterns
3. **Single-shot queries**: No iterative refinement based on intermediate results
4. **Higher LLM reasoning burden**: Forces LLM to learn query syntax vs. natural language

**Impact**: Lower query accuracy, more token consumption, steeper learning curve for users

---

## 3. Advanced Analysis Capabilities

### Codebadger Algorithms

#### 1. Taint Propagation
```
Forward traversal along PDG edges
Tracks variable propagation from sources to sinks
Path capping to prevent exponential growth
Use case: "Does user input reach this SQL query?"
```

#### 2. Backward Slicing
```
Iterative upstream dependency collection via PDG + CFG
Reduces codebase by 90% while preserving semantics
Use case: "What code could affect this variable?"
```

#### 3. Vulnerability Detection
- Bounds checking verification
- Use-after-free detection
- Data flow from untrusted sources to sensitive sinks

### CodexGraph Algorithms

#### 1. Cross-File Resolution
```
DFS traversal to establish inherited FIELD/METHOD edges
Converts relative imports → absolute imports
Enables accurate cross-file CONTAINS relationships
```

#### 2. Structure-Aware Search
```cypher
MATCH (m:MODULE)-[:CONTAINS]->(c:CLASS)-[:HAS_METHOD]->(method)
WHERE m.name = 'auth' AND method.name LIKE '%validate%'
RETURN c, method
```

### rBuilder Current State
✅ **Strengths**:
- Complexity analysis (cyclomatic, cognitive, nesting depth)
- Community detection (Louvain algorithm)
- Centrality metrics (PageRank, betweenness)
- Basic impact analysis (call graph traversal)
- Incremental updates (file tracker + git integration)

❌ **Gaps**:
1. **No taint analysis**: Cannot trace data flow from sources to sinks
2. **No slicing**: Cannot compute minimal code slice affecting a criterion
3. **No vulnerability detection**: No security-focused analysis patterns
4. **Limited cross-file resolution**: Basic import tracking, no DFS for inheritance chains
5. **No PDG-based dependency tracking**: Can only track structural dependencies, not data flow

**Impact**: Cannot answer critical security questions like "Can user input reach this SQL query?" or "What's the minimal code that affects this variable?"

---

## 4. Query Capabilities

### CodexGraph Query Language (Cypher-based)
```cypher
// Multi-hop inheritance
MATCH (c:CLASS)-[:INHERITS*]->(base)
WHERE base.name = 'BaseController'
RETURN c

// Cross-module structure query
MATCH (m:MODULE)-[:CONTAINS]->(c:CLASS)-[:HAS_METHOD]->(method)
WHERE m.name = 'auth' AND method.name LIKE '%validate%'
RETURN c, method

// Path queries
MATCH path = shortestPath((a:Function)-[:CALLS*]-(b:Function))
WHERE a.name = 'main' AND b.name = 'critical'
RETURN path, length(path)
```

**Capabilities**:
- Multi-hop patterns: `(a)-[:CALLS*1..3]->(b)`
- Path queries: `shortestPath()`, `allShortestPaths()`
- Filtering: `WHERE`, `AND`, `OR`, `LIKE`
- Aggregation: `COUNT()`, `AVG()`, `SUM()`
- Ordering: `ORDER BY`, `LIMIT`

### rBuilder Current Query Syntax
```
type:Function|name:auth
repo:backend|type:Function|complexity:>20
name_suffix:Service
```

**Capabilities**:
- Simple filters: `type:`, `name:`, `label:`, `repo:`
- Compound filters with `|` (AND semantics)
- Shortcuts: `functions`, `classes`, `files`, `config`
- Selectivity ranking for optimization
- Glob patterns: `name:*auth*`

✅ **Strengths**:
- Fast pattern matching (<1ms for simple queries)
- Selectivity-based query optimization
- Clean, concise syntax for common cases
- Multi-repo support

❌ **Gaps**:
1. **No multi-hop patterns**: Can't express "functions reachable in 2-3 hops"
2. **No path queries**: Can't find "all paths from A to B"
3. **No structural composition**: Can't say "classes that inherit X and implement Y"
4. **No aggregation**: Can't compute "average complexity of methods in module X"
5. **No graph query language**: Limited to flat filters, no nested patterns

**Impact**: Limited to simple lookups; can't express complex structural or relational queries

---

## 5. Performance & Scalability

### Codebadger
**Limitations**:
- Joern's CPG generation creates overhead for very large repos
- Resource-intensive for massive codebases

**Optimizations**:
- Redis-based caching
- CPGs cached by source hash
- Docker containerization
- FastMCP with async processing

### CodexGraph
**Limitations**:
- 43 SymPy samples caused OOM on SWE-bench due to "numerous files and complex dependencies"
- Token consumption: 22.16k vs BM25's 1.47k (15x higher)

**Optimizations**:
- Two-phase indexing (shallow → complete)
- Neo4j graph database for efficient queries
- Iterative query refinement to minimize wasted work

### rBuilder Current State
✅ **Strengths**:
- In-memory graph with O(1) node lookup
- 5-minute cache TTL for expensive analysis
- Incremental updates (only re-parse changed files)
- Parallel file parsing
- Meeting performance targets:
  - Parse 100k LOC: <60s ✅
  - Incremental update: <5s ✅
  - Pattern match: <1ms ✅
  - Graph query: <100ms (99th percentile) ✅

❌ **Gaps**:
1. **No benchmark on very large repos**: Not tested on 100K+ file codebases
2. **No query optimizer**: No cost-based query planning
3. **No index structures**: Linear scan for some operations
4. **Memory scaling unknown**: Not benchmarked on 1M+ node graphs

**Impact**: Unknown behavior at extreme scale; may need optimization for enterprise codebases

---

## Top 5 Critical Gaps (Prioritized)

### 1. Control & Data Flow Analysis (CFG + PDG) 🔴 CRITICAL
**Why**: Foundation for all semantic reasoning
**What**: Build CFG from tree-sitter AST, compute PDG via data flow analysis
**Use Cases**:
- Security: "Does user input reach this SQL query?"
- Debugging: "What data flows to this crash site?"
- Refactoring: "What data dependencies exist?"

**Implementation Plan**: Phase 12.1 (Tasks 12.1.1-12.1.2)

---

### 2. Dual-Agent Query Translation System 🟡 HIGH
**Why**: 3.4x accuracy improvement proven by research
**What**: Separate reasoning agent + translation agent
**Use Cases**:
- Natural language queries: "Find security issues in auth"
- Complex decomposition: Break down into executable sub-queries
- Iterative refinement: Gather context until sufficient

**Implementation Plan**: Phase 12.3.3

---

### 3. Graph Schema Enrichment 🟡 HIGH
**Why**: Enables precise filtering and change detection
**What**:
- Add `signature` as first-class field on function nodes
- Store code hashes for incremental change detection
- Add edge properties (call_type, access_type)

**Implementation Plan**: Phase 12.0 (Tasks 12.0.1-12.0.3)

---

### 4. Graph Query Language 🟠 MEDIUM
**Why**: Express complex structural queries
**What**: Cypher-inspired query language with multi-hop patterns
**Use Cases**:
- "Find all classes that inherit BaseController"
- "Shortest path from main to critical_function"
- "All functions reachable in 2-3 calls from entry"

**Implementation Plan**: Phase 12.4 (Tasks 12.4.1-12.4.3)

---

### 5. Backward Slicing 🟠 MEDIUM
**Why**: 90% code reduction while preserving semantics
**What**: Given criterion (variable, line), compute minimal upstream code
**Use Cases**:
- Impact analysis: "What affects this variable?"
- Debugging: "What code could cause this state?"
- Testing: "What tests needed for this change?"

**Implementation Plan**: Phase 12.1.3

---

## Quick Wins (Low-Hanging Fruit)

1. **Add signature extraction** to all language plugins (1 week)
2. **Store full method/class code** in node properties (3 days)
3. **Add cross-file import resolution** using DFS (1 week)
4. **Implement query explain plan** (1 week, already in task plan 12.5.2)
5. **Add query result caching** (not just analysis caching) (3 days)

---

## Technology Stack Comparison

| Component | Codebadger | CodexGraph | rBuilder (Current) | rBuilder (Planned) |
|-----------|-----------|-----------|-------------------|-------------------|
| **Graph Engine** | Joern CPG | Neo4j | In-memory (Rust HashMap) | In-memory + file cache |
| **Query Language** | CPGQL | Cypher | Pattern syntax | Custom (Cypher-inspired) |
| **Caching** | Redis | Neo4j native | In-memory (5min TTL) | In-memory (no Redis) |
| **LLM Interface** | MCP tools | Dual-agent + Cypher | MCP tools + patterns | MCP tools + dual-agent |
| **CFG/PDG** | Joern (built-in) | ❌ None | ❌ None | ✅ Custom (tree-sitter) |
| **Parsing** | Joern (multi-lang) | Python-focused | Tree-sitter (13 langs) | Tree-sitter (expand) |
| **Performance** | Slow on large repos | OOM on complex repos | Fast (<60s per 100k LOC) | Fast + optimized |

**Key Decision**: rBuilder stays **Rust-native** - no external databases (Redis, Neo4j). All components implemented in Rust for simplicity and performance.

---

## Recommended Phasing

### Phase 12 (Current): Advanced Query System
- **12.0**: Schema enrichment (signatures, hashes, edge props)
- **12.1**: CFG + PDG + backward slicing
- **12.2**: Blast radius (enhanced with data flow)
- **12.3**: Semantic search + dual-agent query
- **12.4**: Graph query language
- **12.5**: Query macros + explain plan

### Estimated Effort
- **Serial**: 24-28 weeks
- **Parallel** (with 3-4 developers): 12-16 weeks

---

## Success Metrics (from Research)

Based on research papers, Phase 12 should achieve:

| Metric | Target | Source |
|--------|--------|--------|
| Query accuracy (dual-agent) | 90%+ | CodexGraph: 27.9% vs 8.3% baseline |
| Code reduction (slicing) | 80%+ | Codebadger: 90% reduction |
| Query performance (simple) | <100ms | Both papers + rBuilder target |
| Query performance (complex) | <2s | CodexGraph iterative queries |
| Schema coverage | 100% signatures | CodexGraph METHOD nodes |
| CFG construction | <100ms per 1K LOC | Practical target |
| PDG construction | <500ms per 5K LOC | Practical target |

---

## References

1. **Codebadger** (2026): Bridging Code Property Graphs and Language Models for Program Analysis
   - arXiv: https://arxiv.org/html/2603.24837v1
   - Key contribution: CFG+PDG for semantic reasoning, backward slicing

2. **CodexGraph** (NAACL 2025): Bridging Large Language Models and Code Repositories via Code Graph Databases
   - arXiv: https://arxiv.org/html/2408.03910v2
   - ACL Anthology: https://aclanthology.org/2025.naacl-long.7/
   - Key contribution: Dual-agent query translation, 3.4x accuracy improvement

3. **Related Work**:
   - RepoHyper (FORGE 2025): Repo-level Semantic Graph
   - ReGraphRAG (EMNLP 2025): Knowledge graph enrichment
   - jina-code-embeddings (arXiv 2508.21290): Code embeddings

---

## Conclusion

rBuilder has a **solid foundation** but needs **semantic analysis capabilities** (CFG/PDG) and **advanced query systems** (dual-agent, graph QL) to match research state-of-the-art.

**Strategic Advantage**: By staying Rust-native and avoiding external dependencies (Redis, Neo4j), rBuilder can offer:
- ✅ **Simpler deployment**: No database setup
- ✅ **Better performance**: In-memory, no network overhead
- ✅ **Easier maintenance**: Single codebase, no polyglot complexity
- ✅ **Lower resource usage**: No separate DB process

**Next Steps**: Execute Phase 12 task plan as outlined, prioritizing CFG/PDG construction and dual-agent query system for maximum impact.
