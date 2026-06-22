# rBuilder

**Transform code repositories into queryable knowledge graphs for AI agents.**

[![CI](https://github.com/sshaaf/rBuilder/workflows/CI/badge.svg)](https://github.com/sshaaf/rBuilder/actions)
[![Code Quality](https://github.com/sshaaf/rBuilder/workflows/Code%20Quality/badge.svg)](https://github.com/sshaaf/rBuilder/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

rBuilder arms AI coding agents (Claude Code, Cursor, etc.) with deep, queryable codebase understanding through a hybrid knowledge graph system. Ask "what breaks if I change this?" and get instant, accurate answers.

---

## 🎯 Why rBuilder?

**The Problem:**  
AI coding agents have limited codebase understanding. They must read files sequentially, can't answer "what breaks if I change X?", and lack structural understanding of your architecture.

**The Solution:**  
rBuilder builds a queryable knowledge graph of your entire codebase with natural language interface, providing instant architectural insights and impact analysis.

**Key Value:**
- 🚀 **10x faster** impact analysis vs. manual code reading
- 🎯 **90% of queries** answered without LLM calls (pattern matching + cache)
- 📊 **Deep insights**: complexity metrics, communities, circular dependencies
- 🔌 **Native MCP integration** for Claude Code, Cursor, and other AI agents
- 🌐 **35+ languages** + IaC support (Ansible, Chef, Puppet)

---

## ✨ Features

### For AI Agents
- **Natural Language Queries**: "What breaks if I change verify_token()?"
- **Impact Analysis**: Trace dependencies across your entire codebase
- **MCP Integration**: Native Model Context Protocol support for AI agents
- **Context-Efficient**: Compressed responses optimized for token efficiency

### For Developers
- **Multi-Language Support**: 35+ languages including Rust, Python, TypeScript, JavaScript, Go, Java, Kotlin, C#, C, C++, Ruby, PHP, Scala, Swift, Lua, Elixir, Haskell, and more
- **Infrastructure as Code**: Ansible playbooks/roles, Chef cookbooks, and Puppet modules with security scanning
- **Multi-Modal Analysis**: SQL DDL, Dockerfiles, CI/CD YAML (GitHub Actions, GitLab CI), Bash scripts
- **Hybrid NLP System**: 90% queries without LLM (pattern matching → cache → local model → cloud)
- **Graph Intelligence**: Community detection, complexity metrics, centrality analysis
- **Configuration Analysis**: Find unused config keys, missing env vars, hardcoded secrets
- **Incremental Updates**: Git-aware updates in < 5s for changed files

---

## 🚀 Quick Start

### Installation

**From source:**
```bash
git clone https://github.com/sshaaf/rBuilder.git
cd rBuilder
cargo build --release
./target/release/rbuilder --version
```

**With custom language selection:**
```bash
# Minimal build (Rust + Python only, ~60% smaller binary)
cargo build --release --no-default-features --features bundle-minimal

# Full build (all 13 languages)
cargo build --release --features bundle-full
```

### Basic Usage

```bash
# Initialize graph for your project
cd ~/my-project
rbuilder init .

# Ask questions
rbuilder ask "How many functions are there?"
rbuilder ask "What would break if I change authenticate()?"

# Interactive mode
rbuilder chat

# Analyze codebase
rbuilder analyze --complexity --community

# Start web UI
rbuilder serve --port 8080 --open

# Analytics dashboard (communities, hotspots, centrality)
rbuilder serve-web --port 3000 --open

# Ansible-specific commands
rbuilder ansible roles --show-deps
rbuilder ansible security-scan . --min-severity high
```

### Web Dashboard

Interactive analytics at `http://localhost:3000/dashboard.html`:

- Complexity distribution and language breakdown
- **Community detection** — bubble chart + labeled clusters
- **Centrality analysis** — top connected nodes
- **Hotspot table** — risk scores (degree × complexity)

![Dashboard preview](docs/images/dashboard-preview.svg)

### MCP Integration (Claude Code)

Add to `~/.claude/mcp_servers.json`:

```json
{
  "rbuilder": {
    "command": "rbuilder",
    "args": ["mcp", "serve", "--transport", "stdio"],
    "cwd": "/path/to/your/project"
  }
}
```

Now Claude can query your codebase:
- "What functions are in the auth module?"
- "Find high-complexity security functions"
- "What breaks if I refactor this?"
- "Analyze Ansible playbooks and show role dependencies"
- "Scan Ansible playbooks for security vulnerabilities"

---

## 📖 Example Queries

### Impact Analysis
```bash
rbuilder ask "What would break if I change verify_token()?"
```
**Answer:**
```
⚠️  HIGH IMPACT - affects 23 functions across 3 communities
🔴 DIRECT: 6 functions directly call it
⚠️  INDIRECT: 17 functions affected via dependencies
💡 RECOMMENDATION: Feature flag rollout, high-risk change
```

### Code Quality
```bash
rbuilder ask "Find all high-complexity security functions"
```
**Answer:** 8 security-critical functions with cyclomatic complexity > 15

### Configuration
```bash
rbuilder config --unused
```
**Answer:** 14 unused config keys (~15% reduction opportunity)

### Architecture
```bash
rbuilder analyze --community
```
**Answer:** 
```
Community 1: Authentication (23 functions)
Community 2: Data Access (45 functions)
Community 3: API Handlers (67 functions)
```

---

## 🔧 Infrastructure as Code Support

### Ansible

Comprehensive Ansible analysis with security scanning:

```bash
# Analyze role dependencies
rbuilder ansible roles --path ./roles --show-deps

# Validate playbooks
rbuilder ansible validate playbooks/site.yml

# Security scan for common vulnerabilities
rbuilder ansible security-scan . --min-severity medium
```

**Security Checks:**
- **CWE-78**: Command injection (unquoted Jinja2 in shell/command modules)
- **CWE-798**: Hardcoded secrets detection
- **CWE-250**: Unnecessary privilege escalation (become)
- **CWE-532**: Sensitive data logging (missing no_log)

**What's Indexed:**
- Playbooks, plays, tasks, handlers, roles
- Role dependencies (meta/main.yml)
- Jinja2 templates and variable usage
- Group/host variables

**Query Examples:**
```bash
rbuilder gql "type:ansibletask AND module:shell"
rbuilder gql "playbooks"
rbuilder gql "ansibleroles"
```

See [docs/ansible_support.md](docs/ansible_support.md) for complete documentation.

### Chef

Chef cookbook analysis via the same LanguagePlugin pipeline:

```bash
# Analyze cookbook dependencies
rbuilder chef cookbooks --path ./cookbooks --show-deps

# Validate recipes
rbuilder chef validate cookbooks/nginx/recipes/default.rb

# Security scan
rbuilder chef security-scan . --min-severity medium
```

**Query Examples:**
```bash
rbuilder gql "cookbooks"
rbuilder gql "type:chefrecipe"
rbuilder gql "resource:execute"
```

See [docs/chef_support.md](docs/chef_support.md) for complete documentation.

### Puppet

Puppet module and manifest analysis via the same LanguagePlugin pipeline:

```bash
# Analyze module dependencies
rbuilder puppet modules --path ./modules --show-deps

# Validate manifests
rbuilder puppet validate modules/nginx/manifests/init.pp

# Security scan
rbuilder puppet security-scan . --min-severity medium
```

**Query Examples:**
```bash
rbuilder gql "puppetmodules"
rbuilder gql "type:puppetclass"
rbuilder gql "type:puppetresource"
```

See [docs/puppet_support.md](docs/puppet_support.md) for complete documentation.

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────┐
│         AI Coding Agent                 │
│   (Claude Code, Cursor, etc.)           │
└───────────────┬─────────────────────────┘
                │ MCP Protocol
                ▼
┌─────────────────────────────────────────┐
│      Hybrid NLP Query Engine            │
│  Pattern Match (60%) → < 1ms            │
│  Cache (30%)        → < 5ms             │
│  Local Model (8%)   → < 50ms            │
│  Cloud LLM (2%)     → fallback          │
└───────────────┬─────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│     Knowledge Graph (Memory/IndraDB)    │
│  Nodes: Functions, Classes, Config      │
│  Edges: Calls, Imports, References      │
│  Analysis: Complexity, Communities       │
└───────────────┬─────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│     Language Plugins                    │
│  Tier 1: Custom (9 core languages)     │
│  Tier 2: Tree-sitter TOML (22 langs)   │
│  Multi-Modal: IaC, SQL, Docker, CI/CD   │
└─────────────────────────────────────────┘
```

**Three-Tier Hybrid Language System:**
- **Tier 1 (Custom)**: Rich extraction with type inference (9 languages: Rust, Python, TypeScript, JavaScript, Go, Java, Kotlin, C#, Markdown)
- **Tier 2 (Tree-sitter)**: TOML-only config, add in < 30 min (22 languages: C, C++, Ruby, PHP, Scala, Swift, Lua, Elixir, etc.)
- **Multi-Modal**: Infrastructure as Code (Ansible, Chef, Puppet), SQL DDL, Dockerfiles, CI/CD YAML, Bash scripts

*Coming soon

See [LANGUAGE_GUIDE.md](LANGUAGE_GUIDE.md) for adding new languages.

---

## 📚 Documentation

- **[docs/ansible_support.md](docs/ansible_support.md)** - Ansible playbook analysis and security scanning
- **[docs/chef_support.md](docs/chef_support.md)** - Chef cookbook analysis
- **[docs/puppet_support.md](docs/puppet_support.md)** - Puppet module analysis
- **[LANGUAGE_GUIDE.md](LANGUAGE_GUIDE.md)** - Supported languages and adding new ones

---

## 🛠️ Advanced Usage

### Feature Flags

Control which languages are compiled into the binary:

```bash
# Minimal (Rust + Python only)
cargo build --no-default-features --features bundle-minimal

# Extended (+ TypeScript, JavaScript, Go, Java, Ansible IaC)
cargo build --features bundle-extended

# Full (+ Kotlin, C#, Markdown, all multi-modal)
cargo build --features bundle-full

# Extra (+ C, C++, Ruby, PHP, 22 TOML languages)
cargo build --features "bundle-full,bundle-extra"

# Custom selection
cargo build --no-default-features --features "lang-rust,lang-go,lang-python,lang-ansible"

# Infrastructure as Code only
cargo build --no-default-features --features "lang-ansible"
cargo build --no-default-features --features "lang-chef"
cargo build --no-default-features --features "lang-puppet"
```

### IDL Generation

Generate interface definitions from your code:

```bash
# Protocol Buffers
rbuilder idl --format proto --module auth --output-dir ./idl

# Apache Thrift
rbuilder idl --format thrift --module user --output-dir ./idl

# OpenAPI 3.0
rbuilder idl --format openapi --module api --output-dir ./idl
```

### Multi-Repository Workspaces

Analyze multiple repositories as a single workspace:

```bash
# Initialize workspace
rbuilder workspace init

# Add repositories
rbuilder workspace add ../backend --namespace backend
rbuilder workspace add ../frontend --namespace frontend

# Sync and analyze
rbuilder workspace sync

# Query across repos
rbuilder ask "repo:backend|type:Function"
```

### Incremental Updates

```bash
# Update only changed files since last commit
rbuilder update --since HEAD~1

# Force full rebuild
rbuilder update --force
```

---

## 📊 Performance

| Metric | Target | Actual |
|--------|--------|--------|
| Parse 100k LOC | < 60s | ✅ ~45s |
| Incremental update (10 files) | < 5s | ✅ ~2s |
| NLP pattern match | < 1ms | ✅ < 1ms |
| Graph query (99th percentile) | < 100ms | ✅ < 50ms |
| Memory (1M LOC) | < 2GB | ✅ ~1.5GB |

---

## 🤝 Community

We welcome contributions! Whether you want to:
- 🐛 [Report a bug](https://github.com/sshaaf/rBuilder/issues/new?template=bug_report.md)
- ✨ [Request a feature](https://github.com/sshaaf/rBuilder/issues/new?template=feature_request.md)
- 🌐 [Request language support](https://github.com/sshaaf/rBuilder/issues/new?template=language_request.md)

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 📚 Research & Academic Foundation

rBuilder's implementation is built on proven academic research in program analysis and code knowledge graphs.

### Code Property Graphs & Program Analysis

**[Codebadger: Bridging Code Property Graphs and Language Models](https://arxiv.org/abs/2603.24837)** (ICSE 2026)  
Integration of Joern's CPG engine with LLMs via MCP for semantic code analysis. **90% code reduction** through backward slicing while preserving semantics.

**Implemented in rBuilder**:
- Control Flow Graph (CFG) construction from tree-sitter AST → [`src/analysis/cfg_builder.rs`](src/analysis/cfg_builder.rs), [`src/analysis/cfg.rs`](src/analysis/cfg.rs)
- Program Dependence Graph (PDG) with data/control dependencies → [`src/analysis/pdg.rs`](src/analysis/pdg.rs)
- Forward taint propagation for vulnerability detection → [`src/analysis/taint.rs`](src/analysis/taint.rs)
- Backward slicing for impact analysis → [`src/analysis/slicing.rs`](src/analysis/slicing.rs), [`src/analysis/interprocedural_slicing.rs`](src/analysis/interprocedural_slicing.rs)
- Cross-function CFG for interprocedural analysis → [`src/analysis/interprocedural_cfg.rs`](src/analysis/interprocedural_cfg.rs)
- Def-use chains for data flow tracking → [`src/analysis/def_use.rs`](src/analysis/def_use.rs)

**[CodexGraph: Bridging LLMs and Code Repositories via Code Graph Databases](https://arxiv.org/abs/2408.03910)** (NAACL 2025)  
Dual-agent system with **3.4x accuracy improvement** (27.9% vs 8.3%). Graph database with rich node attributes and Cypher-style queries.

**Implemented in rBuilder**:
- Function signatures as first-class schema fields → [`src/graph/schema.rs`](src/graph/schema.rs)
- Indexed code storage for efficient retrieval → [`src/extraction/graph_builder.rs`](src/extraction/graph_builder.rs)
- Cross-file import resolution via DFS → [`src/extraction/extractor.rs`](src/extraction/extractor.rs)
- Rich edge attributes (call type, access patterns) → [`src/graph/schema.rs`](src/graph/schema.rs)

### Classic Program Analysis Algorithms

**Cooper-Harvey-Kennedy Algorithm** (Software Practice & Experience, 2001)  
"A Simple, Fast Dominance Algorithm" - efficient dominator tree construction for control dependency analysis.

**Implemented in rBuilder**:
- Iterative dataflow algorithm for immediate dominators → [`src/analysis/dominance.rs`](src/analysis/dominance.rs)
- Dominance frontiers computation → [`src/analysis/dominance.rs`](src/analysis/dominance.rs)
- Control dependency tracking for PDG construction → [`src/analysis/pdg.rs`](src/analysis/pdg.rs)

**Weiser's Program Slicing** (ICSE 1981)  
Original program slicing concept for backward dependency analysis.

**Implemented in rBuilder**:
- Criterion-based backward slicing → [`src/analysis/slicing.rs`](src/analysis/slicing.rs)
- Interprocedural slicing across function boundaries → [`src/analysis/interprocedural_slicing.rs`](src/analysis/interprocedural_slicing.rs)

**Ferrante et al. Program Dependence Graph** (ACM TOPLAS 1987)  
Foundation for modern program analysis with data and control dependencies.

**Implemented in rBuilder**:
- PDG construction with data/control edges → [`src/analysis/pdg.rs`](src/analysis/pdg.rs)
- Data dependency via def-use analysis → [`src/analysis/def_use.rs`](src/analysis/def_use.rs)
- Control dependency via dominance analysis → [`src/analysis/dominance.rs`](src/analysis/dominance.rs)

### Graph Analysis & Community Detection

**Label Propagation Algorithm** (Raghavan et al., 2007)  
Fast community detection through iterative label propagation with modularity optimization.

**Implemented in rBuilder**:
- Label propagation with Leiden-style heuristics → [`src/analysis/community.rs`](src/analysis/community.rs)
- Modularity scoring for partition quality → [`src/analysis/community.rs`](src/analysis/community.rs)
- Enriched community metadata (complexity, file types) → [`src/analysis/community.rs`](src/analysis/community.rs)

**PageRank & Centrality Metrics** (Brin & Page, 1998)  
Graph centrality algorithms for identifying important code components.

**Implemented in rBuilder**:
- PageRank for code importance scoring → [`src/analysis/centrality.rs`](src/analysis/centrality.rs)
- Betweenness centrality for architectural bottlenecks → [`src/analysis/centrality.rs`](src/analysis/centrality.rs)
- Degree centrality for connectivity analysis → [`src/analysis/centrality.rs`](src/analysis/centrality.rs)

### Security Standards & Vulnerability Detection

**OWASP Top 10** (2021, Updated 2024) + **CWE Database** (MITRE)  
Industry-standard vulnerability classifications mapped to taint analysis sources/sinks.

**Implemented in rBuilder**:
- SQL Injection (CWE-89), XSS (CWE-79), Command Injection (CWE-78) → [`src/analysis/taint.rs`](src/analysis/taint.rs)
- Hardcoded Secrets (CWE-798), Path Traversal (CWE-22) → Security scanners in [`src/security/`](src/security/)
- Source-to-sink tracking with sanitizer detection → [`src/analysis/taint.rs`](src/analysis/taint.rs)
- IaC-specific security patterns (Ansible, Chef, Puppet) → [`src/security/ansible.rs`](src/security/ansible.rs), [`src/security/chef.rs`](src/security/chef.rs), [`src/security/puppet.rs`](src/security/puppet.rs)

### Tools & Infrastructure

**[Tree-sitter](https://tree-sitter.github.io/)** - Incremental parsing for 35+ languages  
**[Model Context Protocol (MCP)](https://modelcontextprotocol.io/)** - LLM-tool integration standard

**See Also**:
- [RESEARCH_CITATIONS.md](RESEARCH_CITATIONS.md) - Complete list of 25+ papers with detailed citations
- [RESEARCH_GAP_ANALYSIS.md](RESEARCH_GAP_ANALYSIS.md) - Comparison with Codebadger and CodexGraph
- [RESEARCH_GRAPH_LABELING.md](RESEARCH_GRAPH_LABELING.md) - Future work on label propagation and migration tracking

---

## 🙏 Acknowledgments

Built with:
- [Tree-sitter](https://tree-sitter.github.io/) - Incremental parsing
- [IndraDB](https://github.com/indradb/indradb) - Graph database
- [Rayon](https://github.com/rayon-rs/rayon) - Parallel processing
- [MCP SDK](https://modelcontextprotocol.io/) - AI agent integration

Inspired by:
- [Graphify](https://github.com/safishamsi/graphify) - Multi-language knowledge graphs
- [GitNexus](https://github.com/abhigyanpatwari/GitNexus) - Client-side graph with MCP
