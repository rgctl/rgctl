# rBuilder - AI-Powered Code Knowledge Graph

**Arm AI coding agents with deep, queryable codebase understanding.**

rBuilder transforms code repositories into knowledge graphs that AI agents can interrogate via natural language, enabling accurate impact analysis, architecture review, and refactoring without reading hundreds of files.

---

## 🎯 Primary Use Case

**For**: AI coding agents (Claude Code, Cursor, etc.)
**Problem**: Agents have limited codebase understanding - must read files sequentially, can't answer "what breaks if I change X?", no structural understanding
**Solution**: Queryable knowledge graph with natural language interface, providing instant architectural insights and impact analysis

---

## ✨ Key Features

### 1. **Extensible Language Support**
- **36+ programming languages** via Tree-sitter (Rust, Python, TypeScript, Go, Java, etc.)
- **10+ configuration formats** (YAML, JSON, TOML, Properties, XML, etc.)
- **Plugin system** for custom languages
- **Code-to-config linking** (which code uses which config keys)

### 2. **Hybrid NLP Query System** (90% queries without LLM calls)
```bash
rbuilder ask "How many React components am I using?"
rbuilder ask "What breaks if I change verify_token()?"
rbuilder ask "Find high-complexity security functions"
```

**Architecture**:
1. **Pattern Matching** (< 1ms, 60% of queries) - Template-based, no LLM
2. **Query Cache** (< 5ms, 30% of queries) - Learned patterns with embeddings
3. **Local Model** (< 50ms, 8% of queries) - Optional fine-tuned T5
4. **Cloud LLM** (500-2000ms, 2% of queries) - Fallback for complex queries

### 3. **MCP Integration** (Model Context Protocol)
- **Native AI agent support** for Claude Code, Cursor, etc.
- **Context-efficient responses** (compressed for token efficiency)
- **7 core MCP tools**: query_codebase, impact_analysis, find_by_complexity, etc.
- **stdio + HTTP transports** (local or team-wide server)

### 4. **Graph Intelligence**
- **Community detection** (Leiden algorithm) - Auto-identify architectural modules
- **Complexity metrics** (cyclomatic, cognitive) - Find refactoring candidates
- **Centrality analysis** (PageRank) - Identify god classes/functions
- **Impact analysis** - What breaks if you change X?
- **Circular dependency detection**

### 5. **Configuration Analysis**
- **Unused config keys** - Reduce config bloat
- **Missing environment variables** - Find before deployment
- **Hardcoded secrets** - Security scanning
- **Config-to-code graph** - Which code uses which config

---

## 🚀 Quick Start

```bash
# Initialize graph for your project
cd ~/my-project
rbuilder init .

# Ask questions in natural language
rbuilder ask "How many React components am I using?"
rbuilder ask "What would break if I change verify_token()?"

# Interactive conversation mode
rbuilder chat

# Start MCP server for Claude Code integration
rbuilder mcp serve --transport stdio

# Web-based graph browser
rbuilder serve --port 8080 --open
```

### Language Feature Flags (Phase 7)

Languages are configured in `languages.toml` and selected at compile time:

```bash
# Default: all 9 built-in languages
cargo build

# Smaller binary — Rust + Python only
cargo build --no-default-features --features bundle-minimal

# Add C, Ruby, PHP, C++
cargo build --no-default-features --features "bundle-full,bundle-extra,mcp-server,nlp-patterns"
```

See [LANGUAGE_GUIDE.md](./LANGUAGE_GUIDE.md) for adding new languages via TOML.

---

## 🔧 IDL Generation

Generate Interface Definition Language (IDL) files from your codebase for API documentation and cross-language interfaces:

```bash
# Generate Protocol Buffers (Proto3) from function signatures
rbuilder idl --format proto --module auth --output-dir ./idl

# Generate Apache Thrift definitions
rbuilder idl --format thrift --module user --output-dir ./idl

# Generate OpenAPI 3.0 specifications
rbuilder idl --format openapi --module api --output-dir ./idl

# Preview to stdout (no output directory)
rbuilder idl --format proto --module auth
```

**Supported formats**:
- `proto` - Protocol Buffers proto3 with gRPC service definitions
- `thrift` - Apache Thrift with service definitions
- `openapi` - OpenAPI 3.0 YAML with REST endpoints

**How it works**:
1. Extracts function signatures from your codebase (types, parameters, return values)
2. Normalizes types across languages (e.g., Rust `i64` → Proto `int64` → Thrift `i64` → OpenAPI `integer`)
3. Generates idiomatic IDL for each format with request/response messages

**Example output** (Proto):
```protobuf
syntax = "proto3";

package CalculateDiscount;

message CalculateDiscountRequest {
  double price = 1;
  string tier = 2;
}

message CalculateDiscountResponse {
  double result = 1;
}

service CalculateDiscountService {
  rpc calculate_discount(CalculateDiscountRequest) returns (CalculateDiscountResponse);
}
```

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   AI Coding Agent                       │
│              (Claude Code, Cursor, etc.)                │
└────────────────────┬────────────────────────────────────┘
                     │ MCP Protocol
                     ▼
┌─────────────────────────────────────────────────────────┐
│                  rBuilder MCP Server                     │
│  Tools: query_codebase, impact_analysis, config_check   │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│              Hybrid NLP Query Engine                     │
│  1. Pattern Matching (60%)  → < 1ms, no LLM            │
│  2. Query Cache (30%)       → < 5ms, learned patterns   │
│  3. Local Model (8%)        → < 50ms, optional T5       │
│  4. Cloud LLM (2%)          → fallback for complex      │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│              Knowledge Graph (IndraDB)                   │
│                                                          │
│  Nodes: Functions, Classes, Modules, Config Keys        │
│  Edges: Calls, Imports, Inherits, UsedBy, References    │
│  Labels: react:component, security:critical, etc.       │
│  Metrics: Complexity, Centrality, Communities           │
└────────────────────┬────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│         Language Plugins (Tree-sitter based)            │
│  Built-in: Rust, Python, TypeScript, Go, Java, +31     │
│  Config: YAML, JSON, TOML, Properties, XML, +5         │
│  External: User-provided plugins via dynamic loading    │
└─────────────────────────────────────────────────────────┘
```

---

## 📚 Documentation

- **[PROPOSAL.md](./PROPOSAL.md)** - Complete technical proposal (architecture, design decisions, roadmap)
- **[AGENT_INTEGRATION.md](./AGENT_INTEGRATION.md)** - AI agent integration guide (MCP, workflows, examples)
- **[NLP_WITHOUT_LLM.md](./NLP_WITHOUT_LLM.md)** - Hybrid NLP design (pattern matching, caching, local models)
- **[NLP_QUERY_EXAMPLES.md](./NLP_QUERY_EXAMPLES.md)** - 50+ example queries organized by category

---

## 🎯 Example Queries

### Inventory & Discovery
```bash
rbuilder ask "How many React components am I using?"
# Answer: 156 components across 3 communities

rbuilder ask "Give me all the SOA services"
# Answer: 12 services with complexity metrics and dependencies
```

### Impact Analysis
```bash
rbuilder ask "What would break if I change verify_token()?"
# Answer:
# ⚠️ HIGH IMPACT - affects 23 functions across 3 communities
# 🔴 DIRECT: 6 functions directly call it
# ⚠️ INDIRECT: 17 functions affected via dependencies
# 💡 RECOMMENDATION: Feature flag rollout, high-risk change
```

### Code Quality
```bash
rbuilder ask "Find all high-complexity security functions"
# Answer: 8 security-critical functions with complexity > 15

rbuilder ask "Which functions have no tests?"
# Answer: 67 functions without tests, prioritized by complexity
```

### Configuration
```bash
rbuilder ask "Which config keys are never used?"
# Answer: 14 unused keys (~15% reduction opportunity)

rbuilder ask "Find missing environment variables"
# Answer: 7 missing env vars with references and examples
```

### Architecture
```bash
rbuilder ask "Show me the most connected modules"
# Answer: Top 10 modules by degree centrality with dependency info

rbuilder ask "Find circular dependencies"
# Answer: 3 circular dependency cycles with recommendations
```

---

## 🤖 AI Agent Integration

### MCP Configuration (Claude Code)

```json
// ~/.claude/mcp_servers.json
{
  "rbuilder": {
    "command": "rbuilder",
    "args": ["mcp", "serve", "--transport", "stdio"],
    "cwd": "/path/to/your/project"
  }
}
```

### Agent Workflow Example

**User**: "Help me refactor the authentication system"

**Claude Code** (using rBuilder):
1. `query_codebase("What functions are in the auth community?")` → 67 functions
2. `find_by_complexity(min=20, labels=["auth"])` → 8 high-complexity functions
3. `impact_analysis("authenticate_with_mfa")` → 23 affected functions
4. Provides refactoring plan with precise impact assessment

**Result**: Claude gives confident, accurate refactoring suggestions based on complete architectural understanding.

---

## 🏗️ Technology Stack

- **Core**: Rust (performance, safety, portability)
- **Parsing**: Tree-sitter (local AST, 36+ languages)
- **Graph**: IndraDB (embedded, portable, Rust-native)
- **NLP**: Pattern matching + optional embeddings + optional LLM
- **Integration**: MCP SDK (AI agent protocol)
- **Config**: serde_yaml, toml, quick-xml, etc.

---

## 📈 Performance Targets

| Metric | Target | Why |
|--------|--------|-----|
| Parse 100k LOC | < 60s | Initial graph construction |
| Incremental update | < 5s | Git-aware, changed files only |
| NLP query (pattern match) | < 1ms | 60% of queries |
| NLP query (cache) | < 5ms | 30% of queries |
| Graph query | < 100ms | 99th percentile |
| Memory (1M LOC) | < 2GB | Large repository support |

---

## 🗺️ Roadmap

### Phase 1: Foundation (Weeks 1-4)
- ✅ Basic graph construction (Rust, Python, TypeScript, Go, JavaScript)
- ✅ Configuration file support (YAML, JSON, TOML, Properties)
- ✅ Code-to-config linking

### Phase 2: Hybrid NLP (Weeks 5-8)
- ✅ Pattern-based NLP (60% queries, no LLM)
- ✅ Query cache with embeddings (90% queries)
- ✅ Graph analysis (communities, complexity, centrality)
- ✅ Configuration analysis

### Phase 3: Plugin System (Weeks 9-11)
- Rule engine for labeling
- External language plugins
- 10+ languages via plugins

### Phase 4: Semantic Translation (Weeks 12-14)
- IDL generation (Proto, Thrift, OpenAPI)
- Domain pattern learning
- Enhanced NLP with project-specific terminology

### Phase 5: Performance (Weeks 15-16)
- Incremental updates (< 5s)
- Parallel processing
- Query optimization

### Phase 6: MCP + Visualization (Weeks 17-19)
- **MCP server for AI agents**
- Web-based graph browser
- Conversational query mode
- Claude Code integration

### Phase 7+: Advanced Features (Weeks 20+)
- Multi-repo support
- CI/CD integration
- Plugin marketplace
- Configuration drift detection

---

## 🤝 Contributing

rBuilder is designed to be extensible:

- **Add a language**: Implement `LanguagePlugin` trait (~500 lines)
- **Add a config format**: Implement `ConfigFormatPlugin` trait
- **Add query templates**: Contribute common patterns to template library
- **Train local model**: Fine-tune T5 on your domain-specific queries

---

## 🎓 Why rBuilder?

**Traditional AI coding assistants**:
- ❌ Reactive: Answer questions by reading files
- ❌ Limited: Can only see immediate context
- ❌ Slow: Re-analyze on every interaction
- ❌ Error-prone: Miss cross-file relationships

**AI coding assistants + rBuilder**:
- ✅ Proactive: Understand architecture upfront
- ✅ Comprehensive: Full codebase knowledge graph
- ✅ Fast: Pre-computed insights, < 5ms queries
- ✅ Accurate: Graph-based relationship tracking

**Result**: AI agents become **10x more effective** at refactoring, architecture review, and impact analysis.

---

## 📄 License

TBD

---

## 🙏 Acknowledgments

Inspired by:
- [Graphify](https://github.com/safishamsi/graphify) - Multi-language knowledge graphs with Tree-sitter
- [GitNexus](https://github.com/abhigyanpatwari/GitNexus) - Client-side graph with MCP
- [Tree-sitter](https://tree-sitter.github.io/) - Incremental parsing library
- [IndraDB](https://github.com/indradb/indradb) - Rust graph database
