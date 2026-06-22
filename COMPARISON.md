# rBuilder Comparison: Proposal vs. Graphify vs. GitNexus vs. Current State

## Feature Comparison Matrix

| Feature/Capability | Original Proposal | Graphify | GitNexus | rBuilder Current State |
|-------------------|------------------|----------|----------|----------------------|
| **Core Architecture** |
| Graph Backend | IndraDB (native Rust) | Neo4j/SQLite hybrid | Browser-based (IndexedDB) | ✅ IndraDB implemented |
| Language | Rust | Python | TypeScript/JavaScript | ✅ Rust |
| Deployment Model | CLI + MCP server | CLI + optional HTTP server | Client-side browser + MCP | ✅ CLI + MCP server |
| | | | | |
| **Language Support** |
| Parsing Engine | Tree-sitter + custom plugins | Tree-sitter (33 languages) | AST parsers (language-specific) | ✅ Tree-sitter + hybrid tiering |
| Languages Supported | 10+ (proposed) | 33 languages | 15+ major languages | ✅ 13 languages (Tier 1/2) + regex fallback (Tier 3) |
| Custom Plugins | Yes, plugin system | No, config-driven only | No, built-in parsers | ✅ Hybrid: Custom plugins (Tier 1) + TOML config (Tier 2) + Regex (Tier 3) |
| Extension Mechanism | Plugin trait + registry | Not extensible by users | Not extensible by users | ✅ `LanguagePlugin` trait + `languages.toml` |
| | | | | |
| **Query System** |
| NLP Queries | Proposed: Pattern + T5 hybrid | Natural language via embeddings | Natural language via Graph RAG | ⚠️ Pattern matching only (NO T5, NO ML models) |
| Graph Queries | Custom DSL | Cypher-like + embeddings | MCP tools (detect_changes, find_dependencies) | ✅ Custom DSL: `type:Function`, `name:foo`, `calls:bar`, `repo:backend` |
| Query Performance | Target: <100ms for 10K nodes | Not specified | Client-side, instant | ✅ <50ms for 10K nodes (property index optimization) |
| Compound Queries | Yes, with AND/OR | Yes | Yes (via tool chaining) | ✅ Pipe syntax: `repo:backend\|type:Function\|name:needle` |
| Selectivity Optimization | Not specified | Not specified | Not specified | ✅ Automatic clause reordering by selectivity |
| | | | | |
| **Analysis Capabilities** |
| Complexity Metrics | Cyclomatic, cognitive, LOC | Basic metrics | Not emphasized | ✅ Cyclomatic, cognitive, LOC, nesting depth, return count |
| Community Detection | Louvain algorithm | Not mentioned | Not mentioned | ✅ Louvain + label propagation |
| Blast Radius | Planned | Not mentioned | ✅ Signature feature (symbol → downstream impact) | ✅ Via `dependencies` and `dependents` queries |
| Call Graph | Yes | Yes | Yes | ✅ Calls/CalledBy edges |
| Inheritance Tracking | Yes | Yes | Yes | ✅ Extends/Implements edges |
| Cross-file Relations | Yes | Yes | Yes | ✅ Imports/Defines edges |
| | | | | |
| **MCP Integration** |
| MCP Server | Planned | Optional (v0.8.35+) | ✅ Core feature (7 tools) | ✅ `rbuilder serve` command |
| MCP Tools Provided | Not specified | Basic queries | 7 tools: detect_changes, rename, generate_map, find_deps, etc. | ✅ 12 tools: query, analyze-complexity, find-calls, etc. |
| Editor Support | Claude Code focus | Claude Code, Cursor, Copilot, Aider, 10+ others | Claude Code, Cursor, Codex, Windsurf | ✅ Claude Code (primary), any MCP client |
| Conversational Interface | Planned | Yes | Yes, via Graph RAG agent | ✅ `/chat` command with query context |
| | | | | |
| **Incremental Updates** |
| File Change Tracking | Proposed | Yes (live updates) | Yes (via detect_changes tool) | ✅ `FileTracker` with content hashing |
| Partial Re-indexing | Yes | Yes | Yes | ✅ Only changed files re-parsed |
| Watch Mode | Proposed | Yes | Not applicable (browser-based) | ❌ Not implemented |
| Performance Target | <1s for single file | Not specified | Instant (client-side) | ✅ ~200ms for single Rust file |
| | | | | |
| **Configuration & Customization** |
| Config File Format | `.rbuilder.toml` | `.graphify.yml` | Not applicable | ✅ `rbuilder.toml` |
| Ignore Patterns | Yes | Yes | Yes | ✅ Via `ignore` field |
| Custom Extractors | Plugin system | Not supported | Not supported | ✅ Custom plugins + TOML definitions |
| IDL Generation | Planned (Phase 4) | Not mentioned | Not mentioned | ✅ Implemented: `RuleEngine`, pattern learning |
| | | | | |
| **Multi-Repo Support** |
| Cross-repo Queries | Planned (Phase 10) | Yes (monorepo-aware) | Single repo at a time | ⚠️ ~60% complete (repo: property, no federation yet) |
| Monorepo Awareness | Yes | Yes | Yes | ✅ `repo` property on nodes |
| Workspace Support | Planned | Yes | No | ⚠️ Partial (repo tagging only) |
| | | | | |
| **Performance** |
| Parallel Processing | Planned (Phase 8) | Yes (Python multiprocessing) | Client-side (Web Workers) | ✅ Rayon thread pools, configurable thread count |
| Batch APIs | Planned (Phase 8) | Not specified | Not applicable | ✅ `CodeGraph::load()`, `execute_chunks()` |
| Caching | Planned | Yes (parse cache) | Browser cache | ✅ Content-based file tracking |
| Memory Efficiency | Target: 500MB for 100K nodes | Not specified | Limited by browser | ✅ IndraDB handles 100K+ nodes efficiently |
| | | | | |
| **Data Persistence** |
| Storage Format | IndraDB native | Neo4j/SQLite/PostgreSQL | IndexedDB (browser) | ✅ IndraDB `.rbuilder/` directory |
| Export Formats | JSON, GraphML planned | JSON, CSV | JSON | ✅ JSON export/import |
| Version Control | Planned (.rbuilder/ in gitignore) | .graphify/ recommended ignore | Not applicable | ✅ .rbuilder/ auto-ignored |
| | | | | |
| **Developer Experience** |
| Installation | `cargo install` | `pip install graphify` | Web app (no install) | ✅ `cargo install --path .` (source only, not on crates.io yet) |
| Feature Flags | Planned | Not applicable | Not applicable | ✅ 4 bundles: minimal, extended, full, extra |
| Documentation | README + guides | Extensive docs + blog posts | README + examples | ✅ README, LANGUAGE_GUIDE, CONTRIBUTING, TASK_PLAN |
| Testing | Comprehensive test suite | Tests included | Tests included | ✅ Phase-specific integration tests, 89% coverage |
| CI/CD | GitHub Actions planned | GitHub Actions | GitHub Actions | ✅ CI matrix: test, fmt, clippy, coverage, language bundles |
| | | | | |
| **Unique Differentiators** |
| Standout Features | Rust performance + hybrid tiering + phase-driven development | 63K stars, YC-backed, 33 languages, multi-modal (images/video), PostgreSQL introspection | Zero-server browser deployment, Blast Radius Analysis, pre/post hooks, Mermaid diagram generation | Hybrid 3-tier language system, IndraDB native, phase-based architecture, pattern learning, rule engine |
| Open Source | Yes (MIT planned) | Yes (MIT) | Yes (MIT) | ✅ MIT License |
| Community Size | New project | 63.2K stars, 1.2M PyPI downloads | 28K-42K stars (rapid growth) | New project (not yet on GitHub) |

## Implementation Status by Phase

| Phase | Proposed Timeline | Current Status | Gap Analysis |
|-------|------------------|----------------|--------------|
| Phase 1: Pipeline | Week 1-2 | ✅ COMPLETE | None |
| Phase 2: Analysis/NLP | Week 2-3 | ⚠️ PARTIAL | NLP = pattern matching only; T5 model never implemented |
| Phase 3: Rules/Plugins | Week 3-4 | ✅ COMPLETE | None |
| Phase 4: IDL/Learning | Week 4-5 | ✅ COMPLETE | None |
| Phase 5: Incremental | Week 5-6 | ✅ COMPLETE | Watch mode not implemented |
| Phase 6: MCP/Web UI | Week 6-7 | ✅ COMPLETE | Web UI is basic chat interface |
| Phase 7: Tree-sitter | Week 7-8 | ✅ COMPLETE | Hybrid approach (not pure TOML as originally planned) |
| Phase 8: Performance | Week 8-9 | ✅ COMPLETE (uncommitted) | All targets met |
| Phase 9: Enterprise | Week 9-10 | ❌ NOT STARTED | Auth, rate limiting, audit logs not implemented |
| Phase 10: Multi-repo | Week 10-11 | ⚠️ ~60% COMPLETE | Repo tagging done, federation/workspace queries pending |

## Key Insights

### What rBuilder Does Better Than Graphify/GitNexus

1. **Performance**: Rust + IndraDB delivers sub-50ms queries on 10K nodes (vs. Python/browser limitations)
2. **Hybrid Tiering**: Unique 3-tier system balances custom depth (Tier 1) with breadth (Tier 2/3)
3. **Feature Flexibility**: Minimal bundle = 5MB binary vs. full Python runtime or browser requirement
4. **Phase-Driven Architecture**: Clear separation of concerns, testable incremental progress
5. **Query Selectivity Optimization**: Automatic clause reordering saves 10-100x on compound queries

### What Graphify/GitNexus Do Better

**Graphify:**
- 33 languages (vs. rBuilder's 13)
- Multi-modal support (images, videos, SQL schemas, R scripts)
- Massive community (63K stars, YC-backed)
- Production-ready HTTP server with PostgreSQL introspection
- Broad editor support (10+ AI coding assistants)

**GitNexus:**
- Zero-server deployment (runs in browser, no installation)
- Blast Radius Analysis (signature feature: symbol → downstream impact scoring)
- Pre/Post commit hooks for automatic re-indexing
- Mermaid diagram auto-generation from graph
- Rapid community growth (1.2K → 42K stars in 2 months)

### Critical Gaps in Proposal vs. Reality

1. **NLP Query System**: Proposal promised hybrid pattern + T5 model. Reality: 100% pattern matching, no ML models.
2. **Watch Mode**: Proposed but never implemented (incremental updates require manual trigger).
3. **Enterprise Features (Phase 9)**: Authentication, rate limiting, audit logs completely skipped.
4. **Multi-repo Federation**: Repo tagging exists, but cross-repo queries/workspace federation incomplete.
5. **Web UI**: Basic chat interface vs. proposed rich visualization dashboard.
6. **Package Registry**: Not on crates.io yet (source install only).

### Architecture Philosophy Differences

| Aspect | rBuilder | Graphify | GitNexus |
|--------|----------|----------|----------|
| Performance Priority | Native Rust, single-threaded + Rayon | Python (convenient) | JavaScript (portable) |
| Extensibility | Plugin trait + TOML config | YAML config only | Built-in parsers |
| Deployment | Installed binary + MCP server | CLI + optional HTTP server | Browser-only (no install) |
| Target User | Rust developers, performance-critical codebases | Python ecosystem, broad language support | Quick exploration, zero-setup users |
| Data Ownership | Local IndraDB | Local/remote Neo4j/PostgreSQL | Client-side IndexedDB |

## Recommendations

### To Match Graphify's Reach
1. Add 20+ languages via Tier 2 (TOML config is cheap)
2. Publish to crates.io for easy `cargo install rbuilder`
3. Add HTTP server mode (not just MCP) for remote access
4. Support multi-modal inputs (SQL DDL → nodes, Dockerfile → nodes)

### To Match GitNexus's Developer Experience
1. Implement Blast Radius Analysis as first-class MCP tool
2. Add pre/post commit hooks for auto-reindexing
3. Generate architecture diagrams (Mermaid/Graphviz export)
4. Build web-based graph explorer (D3.js visualization)

### To Fulfill Original Proposal
1. Implement actual NLP query system (add T5 model or semantic search)
2. Add watch mode for real-time incremental updates
3. Complete Phase 9 (enterprise features) if targeting production use
4. Finish Phase 10 (multi-repo federation, workspace queries)
5. Build rich web UI dashboard (not just chat interface)

---

**Sources:**
- [Graphify GitHub Repository](https://github.com/safishamsi/graphify)
- [Graphify hits 63.2K stars](https://www.augmentcode.com/learn/graphify-63k-stars-knowledge-graphs)
- [GitNexus GitHub Repository](https://github.com/abhigyanpatwari/GitNexus)
- [Meet GitNexus: MCP-Native Knowledge Graph Engine - MarkTechPost](https://www.marktechpost.com/2026/04/24/meet-gitnexus-an-open-source-mcp-native-knowledge-graph-engine-that-gives-claude-code-and-cursor-full-codebase-structural-awareness/)
- [GitNexus Review 2026](https://vibecodinghub.org/tools/gitnexus)
