# rBuilder Development Roadmap

This document outlines the phased development plan for rBuilder, including completed work, current priorities, and future enhancements.

---

## **Completed Work** ✅

### Phase 1-6: Core Features (Weeks 1-19)
- ✅ Basic graph construction (Rust, Python, TypeScript, Go, JavaScript)
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

### Recent Production Fixes (June 2026)
- ✅ Fixed indradb dependency warning
- ✅ Removed all unused imports and dead code
- ✅ Applied all clippy suggestions (zero warnings)
- ✅ Fixed TOCTOU race in StringInterner
- ✅ Optimized remove_nodes_for_file (no graph cloning)
- ✅ Integrated TypeInferencer into extraction pipeline
- ✅ Added resource caching for MCP tools (50-80% speedup)

**Current State:** 222 tests passing, production-ready, zero warnings

---

## **Phase 7: Tree-sitter Language System Refactor** 🎯 **CURRENT PRIORITY**

**Goal:** Replace manual per-language plugins with TOML-based configuration and procedural macros

**Duration:** 3-4 weeks  
**Start Date:** Week of June 16, 2026  
**Status:** 🟡 Planning

### Motivation
- Current: 9 languages, ~5,000 LOC of repetitive plugin code
- Target: 110+ languages via simple TOML configuration
- Reduce maintenance burden by 80%
- Enable community contributions without Rust knowledge
- Smaller binaries via feature flags

### Sub-phases

#### **Phase 7.1: Infrastructure Setup** (Week 1)
**Priority:** Critical  
**Effort:** 3-4 days

**Tasks:**
1. Create `languages.toml` configuration format
   - Define schema for language metadata
   - Add all current 9 languages to TOML
   - Document TOML format and options

2. Implement `build.rs` code generator
   - Parse `languages.toml` at build time
   - Generate plugin registration code
   - Generate feature flag conditional compilation
   - Add validation for TOML correctness

3. Update `Cargo.toml` with feature flags
   - Make all tree-sitter-* dependencies optional
   - Create language features (lang-rust, lang-python, etc.)
   - Create bundle features (bundle-minimal, bundle-extended, bundle-full)
   - Set default bundle

4. Add build dependencies
   ```toml
   [build-dependencies]
   toml = "0.8"
   serde = { version = "1", features = ["derive"] }
   ```

**Acceptance Criteria:**
- ✅ `languages.toml` defines all 9 current languages
- ✅ `build.rs` successfully generates registration code
- ✅ `cargo build` works with default features
- ✅ `cargo build --no-default-features --features lang-rust` builds with only Rust support
- ✅ All 222 tests still pass

**Deliverables:**
- `languages.toml` - Language configuration file
- `build.rs` - Build-time code generator
- Updated `Cargo.toml` - Feature flags
- Generated `target/.../generated_plugins.rs` - Auto-generated registration

---

#### **Phase 7.2: Procedural Macro Development** (Week 2)
**Priority:** High  
**Effort:** 5-7 days

**Tasks:**
1. Create `rbuilder-macros` crate
   - Set up proc-macro crate structure
   - Add dependencies: `syn`, `quote`, `proc-macro2`

2. Implement `#[derive(LanguagePlugin)]` macro
   - Parse `#[lang_config("languages.toml", "rust")]` attribute
   - Read language metadata from TOML
   - Generate `LanguagePlugin` trait implementation
   - Generate tree-sitter grammar loading code
   - Generate file extension mapping

3. Implement generic extraction helpers
   - `extract_with_node_kinds()` - Generic extraction by node type
   - `extract_functions_generic()` - Reusable function extraction
   - `extract_classes_generic()` - Reusable class extraction
   - Node kind mappings from TOML

4. Create macro documentation
   - Usage examples
   - Configuration options
   - Language-specific overrides

**Acceptance Criteria:**
- ✅ Macro can generate a working plugin from TOML metadata
- ✅ Generated code is equivalent to hand-written plugins
- ✅ Macro expands without errors
- ✅ Documentation shows how to use macro

**Example Usage:**
```rust
#[derive(LanguagePlugin)]
#[lang_config("languages.toml", "rust")]
pub struct RustPlugin;

#[derive(LanguagePlugin)]
#[lang_config("languages.toml", "python")]
pub struct PythonPlugin;
```

**Deliverables:**
- `rbuilder-macros/` - Procedural macro crate
- Macro tests and documentation
- Generic extraction utilities

---

#### **Phase 7.3: Migration of Existing Languages** (Week 3)
**Priority:** High  
**Effort:** 5-7 days

**Tasks:**
1. Migrate languages to macro-based approach (one at a time)
   - Start with simplest: Kotlin, C#
   - Middle complexity: Go, Java
   - Most complex: Rust, Python, TypeScript, JavaScript
   - Markdown (config format)

2. Add TOML metadata for each language
   - Node type mappings (function_kinds, class_kinds)
   - Type inference settings
   - Complexity calculation settings
   - Language-specific quirks

3. Remove old manual plugin implementations
   - Delete old plugin files after migration
   - Update imports in `mod.rs`
   - Update registry

4. Create language-specific override mechanism
   - Allow manual trait method overrides when needed
   - Document when to use overrides vs TOML config

**Acceptance Criteria:**
- ✅ All 9 languages work with macro-based approach
- ✅ All 222 tests still pass
- ✅ No functionality regression
- ✅ Code reduction: ~3,500 LOC removed

**Migration Order:**
1. Kotlin, C# (simplest, good test cases)
2. Java, Go (medium complexity)
3. JavaScript (has type inference)
4. TypeScript (TSX variant handling)
5. Python (has type inference, complex)
6. Rust (most complex, save for last)
7. Markdown (config format)

**Deliverables:**
- Migrated language plugins using macros
- Updated TOML configurations
- Removed legacy plugin files

---

#### **Phase 7.4: Testing & Documentation** (Week 4)
**Priority:** High  
**Effort:** 3-4 days

**Tasks:**
1. Comprehensive testing
   - Test each language individually
   - Test feature flag combinations
   - Test bundle builds (minimal, extended, full)
   - Performance benchmarks (before/after)
   - Memory usage comparison

2. Update documentation
   - README: Explain feature flags
   - CONTRIBUTING: How to add new languages
   - Language guide: Document TOML format
   - Migration guide: For users with custom plugins

3. Add new languages (proof of scalability)
   - Add 5-10 additional languages to demonstrate ease
   - Candidates: C, C++, Ruby, PHP, Swift, Scala, Elixir, Haskell
   - Only TOML config needed (no code)

4. Create CI/CD configurations
   - Test matrix for different feature combinations
   - Binary size tracking
   - Build time monitoring

**Acceptance Criteria:**
- ✅ All tests pass with all feature combinations
- ✅ Documentation is complete and accurate
- ✅ Can add a new language in < 30 minutes
- ✅ CI/CD validates all feature combinations

**Deliverables:**
- Updated documentation
- 5-10 additional languages
- CI/CD configurations
- Migration guide

---

### **Phase 7: Success Metrics**

**Code Quality:**
- LOC reduction: ~3,500 (from 5,000 to 1,500)
- Duplication: Reduced by 80%
- Complexity: Each language now ~10 lines (macro invocation)

**Maintainability:**
- Adding new language: 30 minutes (vs. 4-8 hours)
- Configuration-driven vs. code-driven
- Community can add languages without Rust expertise

**Performance:**
- Binary size with all features: No change
- Binary size with minimal features: ~60% reduction
- Build time: Slightly longer (proc macros), acceptable
- Runtime performance: Identical

**Scalability:**
- Current: 9 languages
- After Phase 7: 15-20 languages
- Potential: 110+ languages (via TOML only)

---

## **Phase 8: Performance & Scalability** (Weeks 20-22)

**Priority:** Medium  
**Duration:** 2-3 weeks  
**Dependencies:** Phase 7 complete

### Tasks

#### **8.1: Parallel Processing with Rayon** 🔥
**Priority:** High  
**Effort:** 2-3 hours  
**From:** DEFERRED_TASKS.md Phase 5

**Changes:**
- Use rayon for multi-threaded file processing
- Parallelize extraction in `pipeline/mod.rs`
- Parallelize updates in `incremental/updater.rs`

**Target Performance:**
- Current: 10 files in ~4s (single-threaded)
- With rayon: 10 files in ~2s (multi-threaded)
- 100+ files: 4x speedup expected

**Acceptance Criteria:**
- ✅ `rayon` dependency already in Cargo.toml
- ✅ Parallel extraction implemented
- ✅ Tests pass with parallel processing
- ✅ Benchmarks show performance improvement

---

#### **8.2: Batch GraphBackend APIs**
**Priority:** Nice-to-have  
**Effort:** 1-2 hours  
**From:** DEFERRED_TASKS.md Phase 5

**Changes:**
```rust
// Current
for node in nodes {
    backend.insert_node(node)?;  // Acquires lock per node
}

// Proposed
backend.insert_nodes_batch(nodes)?;  // Single lock acquisition
```

**Impact:** 10-20% performance improvement for bulk inserts

---

#### **8.3: Query Optimization**
**Priority:** Medium  
**Effort:** 1-2 days

**Tasks:**
- Profile common query patterns
- Add specialized query methods for hot paths
- Optimize graph traversal algorithms
- Add query result streaming for large datasets

---

## **Phase 9: Security & Production Hardening** (Weeks 23-25)

**Priority:** High (for production deployment)  
**Duration:** 2-3 weeks  
**Dependencies:** None (can run parallel to Phase 8)

### Tasks

#### **9.1: Authentication for Web Server** 🔒
**Priority:** Should-fix  
**Effort:** 2-3 hours  
**From:** DEFERRED_TASKS.md Phase 6

**Current State:** No auth (localhost only)

**Proposed Solutions:**
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

**Acceptance Criteria:**
- ✅ API key authentication working
- ✅ Configurable via environment variable or config file
- ✅ Tests for auth middleware
- ✅ Documentation for setup

---

#### **9.2: Rate Limiting & Security**
**Priority:** Medium  
**Effort:** 1-2 days

**Tasks:**
- Add rate limiting for MCP endpoints
- Input validation for natural language queries
- Sanitize graph query inputs
- Add request size limits
- Implement timeout for long-running queries

---

#### **9.3: Production Deployment Guide**
**Priority:** High  
**Effort:** 1-2 days

**Tasks:**
- Docker configuration
- Kubernetes manifests
- Environment variable documentation
- Monitoring & logging setup
- Health check endpoints
- Graceful shutdown handling

---

## **Phase 10: Advanced Features** (Weeks 26+)

**Priority:** Low  
**Duration:** Ongoing  
**Dependencies:** Phases 7-9 complete

### **10.1: Multi-repo Support**
**Effort:** 1-2 weeks

Support analyzing multiple repositories as a unified graph:
- Cross-repo dependency analysis
- Monorepo support
- Workspace detection
- Shared type definitions

---

### **10.2: CI/CD Integration**
**Effort:** 1 week

**Features:**
- GitHub Actions integration
- GitLab CI integration
- Pre-commit hooks
- PR comment automation
- Impact analysis in CI

---

### **10.3: Plugin Marketplace**
**Effort:** 2-3 weeks

**Features:**
- Community-contributed language plugins
- Plugin discovery
- Version management
- Security scanning for plugins

---

### **10.4: Configuration Drift Detection**
**Effort:** 1 week

**Features:**
- Detect config changes over time
- Alert on unexpected config modifications
- Config version history
- Compliance checking

---

### **10.5: WebSocket Support** (DEFERRED)
**Priority:** Nice-to-have  
**Effort:** 3-4 hours  
**From:** DEFERRED_TASKS.md Phase 6

**Features:**
- Real-time graph updates
- Multi-user collaboration
- Live query results

---

### **10.6: Graph Export Formats** (DEFERRED)
**Priority:** Nice-to-have  
**Effort:** 1-2 hours  
**From:** DEFERRED_TASKS.md Phase 6

**Formats:**
- PNG/SVG (static images)
- GraphML (graph exchange)
- DOT (Graphviz)
- JSON (raw data)

---

## **Priority Summary**

### **Critical Path (Next 6 Weeks)**
1. ✅ **Week 17-19:** Complete Phase 6 (MCP + Visualization) - DONE
2. 🎯 **Week 20:** Phase 7.1 - Tree-sitter infrastructure
3. 🎯 **Week 21:** Phase 7.2 - Procedural macros
4. 🎯 **Week 22:** Phase 7.3 - Language migration
5. 🎯 **Week 23:** Phase 7.4 - Testing & docs
6. 🔥 **Week 24:** Phase 8.1 - Parallel processing

### **High Priority (Weeks 24-26)**
- Phase 8: Performance optimizations
- Phase 9: Security & auth
- Production deployment

### **Medium Priority (Weeks 27+)**
- Additional language support (20 → 110+)
- Multi-repo support
- CI/CD integration

### **Low Priority (Backlog)**
- WebSocket support
- Graph export formats
- Plugin marketplace

---

## **Decision Log**

### **Why Phase 7 Now?**
1. **Foundation for scale:** Need this before adding 100+ languages
2. **Community enablement:** TOML config allows non-Rust contributions
3. **Maintenance burden:** Current approach doesn't scale
4. **Performance:** Feature flags enable smaller binaries

### **Why Not Wait?**
- Each new language added manually increases migration effort
- Technical debt compounds
- Community wants to add languages (blocked on current architecture)

### **Risk Mitigation**
- Incremental migration (one language at a time)
- Keep tests passing throughout
- Can rollback if needed
- Parallel development allowed (auth, perf work can continue)

---

## **Resources & Effort**

### **Phase 7 Breakdown**
- **Week 1:** Infrastructure (build.rs, TOML, features) - 30 hours
- **Week 2:** Proc macros - 35 hours
- **Week 3:** Migration - 35 hours
- **Week 4:** Testing & docs - 25 hours
- **Total:** ~125 hours (~3 weeks full-time)

### **Phase 8-9 (Parallel Track)**
- **Parallel processing:** 2-3 hours
- **Authentication:** 2-3 hours
- **Security hardening:** 8-10 hours
- **Total:** ~15 hours (can run during Phase 7 if resources available)

---

## **Milestones**

### **M1: Tree-sitter Refactor Complete** (End of Week 23)
- ✅ All languages using macro-based approach
- ✅ TOML configuration working
- ✅ Feature flags functional
- ✅ 15-20 languages supported
- ✅ Documentation complete
- ✅ All tests passing

### **M2: Production Ready** (End of Week 26)
- ✅ Performance optimized
- ✅ Authentication implemented
- ✅ Security hardened
- ✅ Deployment guides complete
- ✅ Monitoring in place

### **M3: Scale to 50+ Languages** (End of Week 30)
- ✅ 50+ languages via TOML
- ✅ Community contributions enabled
- ✅ Plugin marketplace beta

---

## **Success Criteria (Overall)**

**Technical:**
- Support 50+ languages via TOML configuration
- < 5s incremental updates
- < 100ms graph queries (p99)
- < 2GB memory for 1M LOC
- Zero compiler/clippy warnings

**Business:**
- Community can add languages without Rust knowledge
- Binary size customizable (1MB - 50MB depending on features)
- Production deployments running
- CI/CD integrations active

**Quality:**
- Test coverage > 80%
- All deferred tasks addressed
- Documentation complete
- Migration guides available

---

**Last Updated:** June 16, 2026  
**Current Phase:** 7.1 (Planning)  
**Next Review:** June 23, 2026
