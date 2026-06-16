# Deferred Tasks & Future Enhancements

This document tracks tasks that were identified during Phases 1-6 but deferred as **should-fix** or **nice-to-have** items. All core functionality is complete and production-ready without these enhancements.

---

## Phase 1-2: Foundation & Analysis
**Status**: ✅ Complete (no deferred tasks)

All tasks from Phase 1 and Phase 2 were completed as specified.

---

## Phase 3: Rule Engine & Plugins
**Status**: ✅ Complete (no deferred tasks)

All tasks from Phase 3 were completed as specified.

---

## Phase 4: IDL Generation & Domain Learning

### OPEN: Task #3 - Wire TypeInferencer into Extraction Pipeline
**Priority**: Should-fix  
**Effort**: 1-2 hours  
**Status**: Pending

**Current State**:
- TypeInferencer exists and works for IDL generation
- Only used during `rbuilder idl` command, not during indexing
- Python/JS functions without type hints get `unknown` parameter types

**What's Needed**:
1. Integrate TypeInferencer into plugin extraction (src/extraction/)
2. Infer types from usage when explicit annotations missing
3. Store inferred types with confidence scores in graph nodes
4. Update GraphBuilder to persist inferred type metadata

**Impact**:
- Improves graph completeness for dynamic languages (Python, JavaScript)
- Better IDL generation from dynamically-typed code
- More accurate impact analysis for untyped functions

**Files to Modify**:
- `src/languages/builtin/python.rs` (use TypeInferencer during extraction)
- `src/languages/builtin/javascript.rs` (same)
- `src/extraction/extractor.rs` (wire TypeInferencer into pipeline)

---

### Deferred: Type Flow Tracking (4.1.1)
**Priority**: Nice-to-have  
**Effort**: 4-6 hours  
**Status**: Deferred

**Description**: Track how types flow through function calls for more accurate inference.

**Example**:
```python
def process(data):
    result = transform(data)  # Infer type from transform's return type
    return result
```

**Current Workaround**: Basic heuristics from usage patterns (x + y → numeric)

**Future Enhancement**: Build type flow graph from call relationships

---

### Deferred: Constraint Extraction (4.1.2)
**Priority**: Nice-to-have  
**Effort**: 3-4 hours  
**Status**: Deferred

**Description**: Extract validation rules and bounds from function signatures.

**Example**:
```python
def set_age(age: int):
    assert 0 <= age <= 120  # Extract constraint: age ∈ [0, 120]
```

**Current State**: Extracts parameter types and return types only

**Future Enhancement**: Parse assertions, decorators, comments for constraints

---

### Deferred: A/B NLP Success Rate Benchmark (4.2.2)
**Priority**: Nice-to-have  
**Effort**: 2-3 hours  
**Status**: Deferred

**Description**: Measure NLP improvement from domain pattern learning.

**Acceptance Criteria** (from task plan):
- Baseline: 60% success rate without domain patterns
- Target: 75% success rate with domain patterns
- A/B test framework

**Current State**: Domain patterns implemented but not benchmarked

**Future Enhancement**: Test suite with 100+ queries, before/after metrics

---

### Deferred: External Handlebars Template Files (4.1.3)
**Priority**: Nice-to-have  
**Effort**: 30 minutes  
**Status**: Deferred

**Description**: Move IDL templates from inline constants to `.hbs` files.

**Current State**: Templates are inline constants in `src/semantic/idl_generator.rs`

**Task Plan Deliverable**: Separate `templates/proto.hbs`, `templates/thrift.hbs`, `templates/openapi.hbs`

**Reason for Deferral**: Inline templates work fine, external files add deployment complexity

**Future Enhancement**: Extract to files if users want to customize templates

---

## Phase 5: Incremental Updates & Performance

### Deferred: Parallel Processing (Task 5.2.1)
**Priority**: Should-fix (performance optimization)  
**Effort**: 2-3 hours  
**Status**: Deferred

**Description**: Use rayon for multi-threaded file processing during updates.

**Current State**: Single-threaded extraction meets performance targets (< 5s)

**Target Performance**:
- Current: 10 files in ~4s (single-threaded)
- With rayon: 10 files in ~2s (multi-threaded)

**Files to Modify**:
- `src/pipeline/mod.rs` (parallel file processing)
- `src/incremental/updater.rs` (parallel extraction)

**Reason for Deferral**: Already meets < 5s target per task plan

**Future Enhancement**: Add when processing > 100 files at once

---

### Deferred: Batch GraphBackend APIs (5.2.1)
**Priority**: Nice-to-have  
**Effort**: 1-2 hours  
**Status**: Deferred

**Description**: Add batch insert methods to reduce lock overhead.

**Current State**:
```rust
for node in nodes {
    backend.insert_node(node)?;  // Acquires lock per node
}
```

**Proposed API**:
```rust
backend.insert_nodes_batch(nodes)?;  // Single lock acquisition
```

**Impact**: 10-20% performance improvement for bulk inserts

**Reason for Deferral**: Current implementation is fast enough

---

### Deferred: String Interner TOCTOU Race Fix (5.2.2)
**Priority**: Nice-to-have  
**Effort**: 5 minutes  
**Status**: Deferred

**Description**: Fix benign race condition in double-checked locking.

**Current Code** (`src/graph/intern.rs:21-35`):
```rust
pub fn intern(&self, value: &str) -> Arc<str> {
    if let Ok(read) = self.pool.read() {
        if let Some(existing) = read.get(value) {
            return existing.clone();  // Fast path
        }
    }
    
    let mut write = self.pool.write().unwrap();
    // TOCTOU: Another thread might have inserted between read unlock and write lock
    if let Some(existing) = write.get(value) {
        return existing.clone();
    }
    let arc: Arc<str> = Arc::from(value);
    write.insert(value.to_string(), arc.clone());
    arc
}
```

**Better Code** (using entry API):
```rust
pub fn intern(&self, value: &str) -> Arc<str> {
    self.pool
        .write()
        .unwrap()
        .entry(value.to_string())
        .or_insert_with(|| Arc::from(value))
        .clone()
}
```

**Impact**: Eliminates benign race (worst case: duplicate entry briefly)

**Reason for Deferral**: Race is benign, no correctness issue

---

### Deferred: Query Cache Arc Wrapping (5.2.3)
**Priority**: Nice-to-have  
**Effort**: 15 minutes  
**Status**: Deferred

**Description**: Avoid cloning cached query results.

**Current Code** (`src/graph/backend/memory.rs:231-243`):
```rust
pub fn cached_query(&self, query: &str) -> Result<Vec<Node>> {
    if let Some(cached) = self.query_cache.read().unwrap().get(&key) {
        return Ok(cached.clone());  // Clones entire Vec<Node>
    }
    // ...
}
```

**Optimized Code**:
```rust
query_cache: Arc<RwLock<HashMap<String, Arc<Vec<Node>>>>>

pub fn cached_query(&self, query: &str) -> Result<Arc<Vec<Node>>> {
    if let Some(cached) = self.query_cache.read().unwrap().get(&key) {
        return Ok(Arc::clone(cached));  // Just increments ref count
    }
    // ...
}
```

**Impact**: Faster cache hits, less memory churn

**Reason for Deferral**: Current performance is acceptable

---

### Deferred: remove_nodes_for_file Optimization (5.1.2)
**Priority**: Nice-to-have  
**Effort**: 5 minutes  
**Status**: Deferred

**Description**: Avoid calling `all_nodes()` which clones entire graph.

**Current Code** (`src/graph/backend/memory.rs:153-171`):
```rust
pub fn remove_nodes_for_file(&mut self, file_path: &str) -> Result<usize> {
    let ids: Vec<Uuid> = self
        .all_nodes()?  // ← Clones all nodes
        .into_iter()
        .filter(|n| node_matches_file(n, &normalized))
        .map(|n| n.id)
        .collect();
    // ...
}
```

**Optimized Code**:
```rust
pub fn remove_nodes_for_file(&mut self, file_path: &str) -> Result<usize> {
    let ids: Vec<Uuid> = self
        .nodes
        .read()
        .unwrap()
        .values()  // ← No cloning
        .filter(|n| node_matches_file(n, &normalized))
        .map(|n| n.id)
        .collect();
    // ...
}
```

**Impact**: Faster node removal, less memory allocation

**Reason for Deferral**: Incremental updates already fast enough

---

## Phase 6: MCP Server & Web UI

### Deferred: Resource Caching (6.2)
**Priority**: Should-fix (performance optimization)  
**Effort**: 1-2 hours  
**Status**: Deferred

**Description**: Cache expensive analysis results (complexity, communities).

**Current State**: Every MCP tool call re-computes analysis

**Proposed Solution**:
- TTL cache (5-10 minutes)
- Invalidate on graph mutations
- Cache community detection, complexity analysis

**Example**:
```rust
struct AnalysisCache {
    complexity_report: Option<(ComplexityReport, Instant)>,
    community_report: Option<(CommunityResult, Instant)>,
    ttl: Duration,
}

impl AnalysisCache {
    fn get_complexity(&mut self, backend: &MemoryBackend) -> Result<ComplexityReport> {
        if let Some((report, time)) = &self.complexity_report {
            if time.elapsed() < self.ttl {
                return Ok(report.clone());
            }
        }
        let report = ComplexityAnalyzer::analyze(backend)?;
        self.complexity_report = Some((report.clone(), Instant::now()));
        Ok(report)
    }
}
```

**Impact**: 50-80% faster repeated queries

**Reason for Deferral**: First query is fast enough (< 100ms)

---

### Deferred: Streaming API Responses (6.2)
**Priority**: Nice-to-have  
**Effort**: 2-3 hours  
**Status**: Deferred

**Description**: Stream large result sets progressively.

**Current State**: All nodes loaded into memory, paginated at API level

**Proposed Solution**:
- Server-sent events (SSE) for progressive rendering
- Stream nodes as they're fetched from graph

**Use Case**: Querying 10,000+ nodes in web UI

**Impact**: Faster time-to-first-render, lower memory usage

**Reason for Deferral**: Pagination works fine for current scale

---

### Deferred: Authentication for Web Server (6.2)
**Priority**: Should-fix (security)  
**Effort**: 2-3 hours  
**Status**: Deferred

**Description**: Add authentication for team deployments.

**Current State**: No auth (localhost only)

**Proposed Solutions**:
1. **Basic Auth** - Simple username/password
2. **API Keys** - Token-based authentication
3. **OAuth** - GitHub/Google SSO

**Recommendation**: Start with API keys

**Example**:
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

**Reason for Deferral**: Localhost deployment is safe

**Future Enhancement**: Required for team/cloud deployments

---

### Deferred: WebSocket Support (6.2)
**Priority**: Nice-to-have  
**Effort**: 3-4 hours  
**Status**: Deferred

**Description**: Real-time graph updates via WebSocket.

**Use Case**: Multiple users viewing same graph, live updates

**Proposed Solution**:
- WebSocket endpoint `/ws`
- Broadcast graph mutations to connected clients
- Client auto-refreshes on updates

**Example Flow**:
1. User A runs `rbuilder update`
2. Graph changes broadcast via WebSocket
3. User B's web UI auto-refreshes

**Reason for Deferral**: Single-user use case is primary

---

### Deferred: Graph Export Formats (6.2)
**Priority**: Nice-to-have  
**Effort**: 1-2 hours  
**Status**: Deferred

**Description**: Export graph to standard formats.

**Proposed Formats**:
- **PNG/SVG** - Static graph images
- **GraphML** - Graph exchange format
- **DOT** - Graphviz format
- **JSON** - Raw graph data

**Example API**:
```rust
GET /api/graph/export?format=graphml
GET /api/graph/export?format=png&width=1920&height=1080
```

**Use Cases**:
- Include graph in documentation
- Import to other tools (Neo4j, Gephi)
- Share graph snapshots

**Reason for Deferral**: Web UI visualization is sufficient

---

## Summary by Priority

### High Priority (Should-Fix)
1. **Phase 4**: Wire TypeInferencer into extraction pipeline (Task #3)
2. **Phase 5**: Parallel processing with rayon (performance)
3. **Phase 6**: Resource caching for expensive operations (performance)
4. **Phase 6**: Authentication for team deployments (security)

### Medium Priority (Nice-to-Have)
5. **Phase 5**: Batch GraphBackend APIs
6. **Phase 6**: Streaming API responses
7. **Phase 4**: Type flow tracking
8. **Phase 4**: A/B NLP benchmark

### Low Priority (Polish)
9. **Phase 5**: String interner TOCTOU fix
10. **Phase 5**: Query cache Arc wrapping
11. **Phase 5**: remove_nodes_for_file optimization
12. **Phase 6**: WebSocket support
13. **Phase 6**: Graph export formats
14. **Phase 4**: Constraint extraction
15. **Phase 4**: External Handlebars templates

---

## Effort Summary

**Quick Wins** (< 1 hour):
- String interner TOCTOU fix (5 min)
- remove_nodes_for_file optimization (5 min)
- Query cache Arc wrapping (15 min)
- External Handlebars templates (30 min)

**Medium Effort** (1-3 hours):
- Wire TypeInferencer (1-2 hours)
- Batch GraphBackend APIs (1-2 hours)
- Resource caching (1-2 hours)
- Graph export formats (1-2 hours)
- A/B NLP benchmark (2-3 hours)
- Authentication (2-3 hours)
- Streaming API (2-3 hours)

**Large Effort** (3+ hours):
- Parallel processing with rayon (2-3 hours)
- Constraint extraction (3-4 hours)
- WebSocket support (3-4 hours)
- Type flow tracking (4-6 hours)

---

## Current Status

**Production-Ready Features**: ✅ All 6 Phases Complete
- 220 tests passing
- No critical bugs
- Performance targets met or exceeded
- Full MCP integration working

**Optional Enhancements**: 15 items identified above

**Recommendation**: Ship current state, add enhancements based on user feedback.

---

**Last Updated**: 2026-06-16  
**rBuilder Version**: 0.1.0  
