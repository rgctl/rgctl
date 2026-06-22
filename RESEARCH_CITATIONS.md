# Research Citations & Algorithm References

**Last Updated**: June 18, 2026  
**Purpose**: Comprehensive list of research papers, algorithms, and systems that inspired rBuilder's implementation

---

## Executive Summary

rBuilder's implementation is built on a foundation of academic research spanning multiple decades, from classic compiler algorithms to cutting-edge 2024-2026 research on code knowledge graphs and LLM integration.

**Total Citations**: 25+ papers and systems  
**Timespan**: 1979-2026 (47 years of research)  
**Key Areas**: Code graphs, program analysis, community detection, label propagation, LLM integration

---

## 1. Code Knowledge Graphs & Repository Analysis (2024-2026)

### Codebadger (2026) ⭐ PRIMARY INFLUENCE
**Full Title**: Bridging Code Property Graphs and Language Models for Program Analysis  
**Author**: Ahmed Lekssays  
**Venue**: IEEE/ACM 4th International Workshop on Software Vulnerability Management @ ICSE 2026  
**arXiv**: [2603.24837](https://arxiv.org/abs/2603.24837)  
**ACM DL**: [10.1145/3786165.3788441](https://dl.acm.org/doi/10.1145/3786165.3788441)  
**GitHub**: [Lekssays/codebadger](https://github.com/Lekssays/codebadger)

**Key Contributions**:
- Integration of Joern's Code Property Graph (CPG) with LLMs via MCP
- High-level semantic tools: `taint_flow`, `backward_slice`, `call_graph`
- **90% code reduction** while preserving semantics through backward slicing
- FastMCP + async processing for performance
- Redis-based caching, CPGs cached by source hash

**What rBuilder Adopted**:
- ✅ CFG + PDG construction for semantic reasoning
- ✅ Taint propagation along PDG edges
- ✅ Backward slicing for upstream dependency analysis
- ✅ MCP tool abstraction (not raw queries)
- ✅ High-level semantic tools for LLM consumption

**rBuilder Implementation**:
- `src/analysis/cfg_builder.rs` - CFG construction from tree-sitter AST
- `src/analysis/pdg.rs` - PDG with data/control dependencies
- `src/analysis/taint.rs` - Forward taint tracking
- `src/analysis/slicing.rs` - Backward slicing implementation
- `src/analysis/interprocedural_cfg.rs` - Cross-function CFG

---

### CodexGraph (NAACL 2025) ⭐ PRIMARY INFLUENCE
**Full Title**: CodexGraph: Bridging Large Language Models and Code Repositories via Code Graph Databases  
**Authors**: Xiangyan Liu, Bo Lan, Zhiyuan Hu, Yang Liu, Zhicheng Zhang, Fei Wang, Michael Qizhe Shieh, Wenmeng Zhou  
**Venue**: NAACL 2025 (North American Chapter of ACL)  
**arXiv**: [2408.03910](https://arxiv.org/abs/2408.03910)  
**ACL Anthology**: [2025.naacl-long.7](https://aclanthology.org/2025.naacl-long.7/)  
**Website**: [CodexGraph Project Page](https://laptype.github.io/CodexGraph-page/)

**Key Contributions**:
- Dual-agent system: Primary agent (reasoning) + Translation agent (NL → Cypher)
- **3.4x accuracy improvement** (27.9% vs 8.3% without translation agent)
- Neo4j graph database with Cypher query interface
- Rich node attributes: `signature`, indexed `code`, file paths
- Two-phase indexing: shallow → complete
- Iterative query refinement

**What rBuilder Adopted**:
- ✅ `signature` as first-class field on function/method nodes
- ✅ Indexed code storage for efficient retrieval
- ✅ Edge attributes (source/target type, call type)
- ✅ Cross-file resolution via DFS traversal
- 🔄 Dual-agent query system (planned Phase 12.3.3)

**rBuilder Implementation**:
- `src/graph/schema.rs` - Extended Node schema with signature field
- `src/languages/*/parser.rs` - Signature extraction for all languages
- `src/extraction/graph_builder.rs` - Cross-file import resolution
- Future: `src/mcp/dual_agent.rs` - Planned dual-agent query translator

---

### Knowledge Graph Based Repository-Level Code Generation (May 2025)
**arXiv**: [2505.14394](https://arxiv.org/abs/2505.14394)  
**Benchmark**: Evolutionary Code Benchmark (EvoCodeBench)

**Key Contributions**:
- Graph-based code representations for evolution tracking
- Captures structural and relational information
- Outperforms baselines on repository-level tasks

**What rBuilder Adopted**:
- ✅ Graph-first architecture for code representation
- ✅ Evolution tracking via git integration
- ✅ Structural + relational node/edge modeling

---

### Code Graph Model (CGM) (NeurIPS 2025)
**arXiv**: [2505.16901](https://arxiv.org/pdf/2505.16901)  
**Venue**: NeurIPS 2025  
**GitHub**: [codefuse-ai/CodeFuse-CGM](https://github.com/codefuse-ai/CodeFuse-CGM)

**Key Contributions**:
- Repository-level code graph framework
- R4 chain: Rewriter → Retriever → Reranker → Reader
- **32.8% improvement** on SWE-bench
- Handles both maintenance and evolution

**What rBuilder Adopted**:
- ✅ Repository-level graph construction
- ✅ Code retrieval from graph queries
- 🔄 RAG-style retrieval (planned)

---

### RepoGraph (ICLR 2025)
**arXiv**: [2408.00234](https://arxiv.org/abs/2408.00234)  
**Venue**: ICLR 2025

**Key Contributions**:
- Repository-level code graphs
- **32.8% relative improvement** for agents on SWE-bench

**What rBuilder Adopted**:
- ✅ Repository-level scope (not just file-level)
- ✅ Agent-friendly graph interface

---

### Codebase-Memory: Tree-Sitter-Based Knowledge Graphs (March 2026)
**arXiv**: [2603.27277](https://arxiv.org/html/2603.27277v1)

**Key Contributions**:
- Tree-sitter integration for code exploration
- MCP (Model Context Protocol) integration

**What rBuilder Adopted**:
- ✅ Tree-sitter as parsing engine
- ✅ MCP as LLM interface protocol
- ✅ Multi-language support via tree-sitter grammars

---

## 2. Label Propagation & Community Detection (2019-2024)

### Knowledge Graph Enhanced Community Detection (ACM WSDM 2019) ⭐
**Authors**: Shreyansh Bhatt, Keke Chen  
**Venue**: ACM International Conference on Web Search and Data Mining (WSDM 2019)  
**DOI**: [10.1145/3289600.3291031](https://dl.acm.org/doi/10.1145/3289600.3291031)  
**GitHub**: [shreyanshbhatt/KnowledgeGraph_in_CommunityDetection](https://github.com/shreyanshbhatt/KnowledgeGraph_in_CommunityDetection)

**Key Contributions**:
- Incorporates domain-specific hierarchical concept graphs
- **~20% improvement** on F-measure and Jaccard over state-of-the-art
- Contextual information from node attributes

**What rBuilder Adopted**:
- ✅ Label-weighted community detection
- ✅ Contextual attributes (complexity, security labels)
- 🔄 Hierarchical label taxonomies (planned)

**rBuilder Implementation**:
- `src/analysis/community.rs` - Louvain algorithm for community detection
- Future: `src/analysis/labeled_communities.rs` - Label-aware communities

---

### Graph Embedding Based Label Propagation (Scientific Reports, Nov 2024)
**DOI**: [10.1038/s41598-025-25905-5](https://www.nature.com/articles/s41598-025-25905-5)  
**Algorithm**: ELP (Embedding-based Label Propagation)

**Key Contributions**:
- Combines label propagation with node embedding
- Integrates local connectivity and global structural data

**What rBuilder Adopted**:
- 🔄 Label propagation algorithm (planned Phase 19+)
- 🔄 Embedding-based similarity for communities

---

### Combining GCN and Label Propagation (ACM TOIS 2021)
**Authors**: Stanford University  
**DOI**: [10.1145/3490478](https://dl.acm.org/doi/10.1145/3490478)  
**PDF**: [Stanford](https://www-cs.stanford.edu/~jure/pubs/gcnlpa-tois21.pdf)

**Key Contributions**:
- Both LPA and GCN are message passing algorithms
- LPA propagates label information, GCN propagates feature information
- Combining structural (GCN) and semantic (LPA) information

**What rBuilder Adopted**:
- 🔄 Future: Combined structural + semantic propagation

---

### Overlapping Community Detection Survey (Multimedia Tools, Dec 2024)
**DOI**: [10.1007/s11042-024-20485-4](https://link.springer.com/article/10.1007/s11042-024-20485-4)

**Key Contributions**:
- Survey of label propagation in overlapping community identification
- LPA's merits and limitations in large-scale networks

**What rBuilder Adopted**:
- ✅ Overlapping community support (nodes can belong to multiple clusters)

---

### Community Detection with Deep Learning Survey (Neurocomputing 2024)
**DOI**: [10.1016/j.neucom.2024.127849](https://www.sciencedirect.com/science/article/abs/pii/S0925231224009408)

**Key Contributions**:
- Comprehensive survey of modern deep learning approaches

**What rBuilder Adopted**:
- 📚 Reference for future GNN integration

---

## 3. Graph Neural Networks & Node Classification (2024)

### Data-Efficient Graph Learning (IJCAI 2024)
**PDF**: [IJCAI Proceedings](https://www.ijcai.org/proceedings/2024/0896.pdf)

**Key Contributions**:
- Self-supervised, semi-supervised, and few-shot graph learning
- GNNs for semi-supervised node classification

**What rBuilder Adopted**:
- 📚 Future: Semi-supervised learning for incomplete labels

---

### NoisyGL: Label Noise Benchmark (NeurIPS 2024)
**GitHub**: [eaglelab-zju/NoisyGL](https://github.com/eaglelab-zju/NoisyGL)  
**Venue**: NeurIPS 2024

**Key Contributions**:
- Comprehensive benchmark for GNNs under label noise
- Important for real-world automated labeling

**What rBuilder Adopted**:
- 📚 Future: Confidence scores for automated labels

---

### Resurrecting Label Propagation (KDD 2024)
**arXiv**: [2310.16560](https://arxiv.org/abs/2310.16560)  
**Venue**: KDD 2024

**Key Contributions**:
- Label propagation with heterophily and label noise

**What rBuilder Adopted**:
- 🔄 Noise-tolerant label propagation (planned)

---

## 4. Code Migration & API Evolution (2024)

### Amazon MigrationBench (2024)
**Source**: [AWS DevOps Blog](https://aws.amazon.com/blogs/devops/amazon-introduces-two-benchmark-datasets-for-evaluating-ai-agents-ability-on-code-migration/)

**Datasets**: MigrationBench and Poly-MigrationBench

**Migration Types**:
- Runtime upgrade
- Deprecated API replacement
- Test framework optimization
- Syntax modernization

**What rBuilder Adopted**:
- ✅ Deprecated API tracking
- ✅ Migration scenarios as use case
- 🔄 Benchmark integration (planned)

---

## 5. Classic Program Analysis (1970s-2000s)

### Cooper-Harvey-Kennedy Algorithm (2001)
**Paper**: "A Simple, Fast Dominance Algorithm"  
**Authors**: Keith D. Cooper, Timothy J. Harvey, Ken Kennedy  
**Venue**: Software Practice & Experience 2001

**Key Contributions**:
- Efficient dominator tree construction
- Iterative dataflow algorithm
- O(n²) worst case, near-linear in practice

**What rBuilder Adopted**:
- ✅ Dominator tree construction in `src/analysis/dominance.rs`
- ✅ Immediate dominators (idom) computation
- ✅ Dominance frontiers for SSA form

**rBuilder Implementation**:
```rust
// src/analysis/dominance.rs:18
/// Build dominator tree via iterative dataflow (Cooper-Harvey-Kennedy style).
pub fn build(cfg: &ControlFlowGraph) -> Self {
    // Iterative algorithm implementation
}
```

---

### Lengauer-Tarjan Algorithm (1979)
**Paper**: "A Fast Algorithm for Finding Dominators in a Flowgraph"  
**Authors**: Thomas Lengauer, Robert E. Tarjan  
**Venue**: ACM Transactions on Programming Languages and Systems 1979

**Key Contributions**:
- Nearly linear-time dominator tree construction
- Path compression + union-find data structure
- Classic algorithm taught in compilers courses

**What rBuilder Adopted**:
- 📚 Theoretical foundation for dominance analysis
- ✅ Alternative considered (chose Cooper-Harvey-Kennedy for simplicity)

---

### Weiser's Program Slicing (1981)
**Paper**: "Program Slicing"  
**Author**: Mark Weiser  
**Venue**: Proceedings of the 5th International Conference on Software Engineering

**Key Contributions**:
- Original program slicing concept
- Backward slicing for debugging
- Slicing criterion: (statement, variable)

**What rBuilder Adopted**:
- ✅ Backward slicing in `src/analysis/slicing.rs`
- ✅ Criterion-based slice computation
- ✅ Dependency-based traversal

---

### Cytron et al. SSA Form (1991)
**Paper**: "Efficiently Computing Static Single Assignment Form and the Control Dependence Graph"  
**Authors**: Ron Cytron, Jeanne Ferrante, Barry K. Rosen, Mark N. Wegman, F. Kenneth Zadeck  
**Venue**: ACM Transactions on Programming Languages and Systems 1991

**Key Contributions**:
- Static Single Assignment (SSA) form
- Dominance frontiers for phi-node placement
- Control Dependence Graph (CDG)

**What rBuilder Adopted**:
- ✅ Dominance frontiers computation
- ✅ Control dependencies in PDG
- 🔄 SSA form (planned for future optimizations)

---

### Ferrante et al. Program Dependence Graph (1987)
**Paper**: "The Program Dependence Graph and Its Use in Optimization"  
**Authors**: Jeanne Ferrante, Karl J. Ottenstein, Joe D. Warren  
**Venue**: ACM Transactions on Programming Languages and Systems 1987

**Key Contributions**:
- Original PDG concept
- Data dependencies (def-use chains)
- Control dependencies
- Foundation for program slicing

**What rBuilder Adopted**:
- ✅ PDG construction in `src/analysis/pdg.rs`
- ✅ Data dependency edges
- ✅ Control dependency edges
- ✅ Def-use analysis in `src/analysis/def_use.rs`

---

## 6. Security & Vulnerability Analysis

### OWASP Top 10 (2021, Updated 2024)
**Source**: [OWASP Foundation](https://owasp.org/www-project-top-ten/)

**What rBuilder Adopted**:
- ✅ SQL Injection (CWE-89) detection
- ✅ Cross-Site Scripting (CWE-79) detection
- ✅ Command Injection (CWE-78) detection
- ✅ Hardcoded secrets (CWE-798) detection
- ✅ Path Traversal (CWE-22) detection

**rBuilder Implementation**:
- `src/analysis/taint.rs` - Taint sources/sinks for OWASP Top 10
- `src/security/ansible.rs` - IaC security patterns
- `src/security/chef.rs` - Chef security patterns
- `src/security/puppet.rs` - Puppet security patterns

---

### CWE Database (MITRE)
**Source**: [CWE MITRE](https://cwe.mitre.org/)

**What rBuilder Adopted**:
- ✅ CWE-78: OS Command Injection
- ✅ CWE-79: Cross-Site Scripting (XSS)
- ✅ CWE-89: SQL Injection
- ✅ CWE-22: Path Traversal
- ✅ CWE-798: Hardcoded Credentials
- ✅ CWE-732: Incorrect Permission Assignment
- ✅ CWE-250: Execution with Unnecessary Privileges
- ✅ CWE-532: Insertion of Sensitive Information into Log

**Total CWE Patterns**: 16+ across all security scanners

---

## 7. Tools & Infrastructure

### Tree-sitter (2018-present)
**Author**: Max Brunsfeld (GitHub)  
**Website**: [tree-sitter.github.io](https://tree-sitter.github.io/)  
**GitHub**: [tree-sitter/tree-sitter](https://github.com/tree-sitter/tree-sitter)

**Key Contributions**:
- Incremental parsing
- Error-tolerant parsing
- Multi-language support (40+ languages)
- Language-agnostic query system

**What rBuilder Adopted**:
- ✅ Core parsing engine for 35+ languages
- ✅ Incremental re-parsing on file changes
- ✅ Query system for AST pattern matching
- ✅ Language plugins for: Rust, Python, TypeScript, JavaScript, Go, Java, etc.

---

### Joern Code Property Graph
**Authors**: Fabian Yamaguchi et al.  
**Website**: [joern.io](https://joern.io/)  
**GitHub**: [joern-cli/joern](https://github.com/joern-cli/joern)

**Key Contributions**:
- Unified CPG combining AST + CFG + PDG
- Multi-language support (Java, C/C++, JavaScript, Python, etc.)
- CPGQL query language
- Open-source static analysis platform

**What rBuilder Adopted**:
- ✅ CPG concept (AST + CFG + PDG unified graph)
- ✅ Multi-language architecture
- ✅ Inspired graph schema design
- ❌ Did NOT adopt: External dependency (rBuilder implements CPG natively in Rust)

---

### Model Context Protocol (MCP)
**Author**: Anthropic  
**Website**: [modelcontextprotocol.io](https://modelcontextprotocol.io/)

**Key Contributions**:
- Standard protocol for LLM-tool integration
- Server-client architecture
- Tool discovery and invocation
- Streaming support

**What rBuilder Adopted**:
- ✅ MCP server implementation
- ✅ Tool-based abstraction (not raw queries)
- ✅ Streaming responses for large results
- ✅ Native Claude Code integration

**rBuilder Implementation**:
- `src/mcp/server.rs` - MCP server
- `src/mcp/tools.rs` - MCP tool definitions
- `src/mcp/executor.rs` - Tool execution engine

---

## 8. Graph Databases & Query Languages

### Neo4j & Cypher
**Cypher Query Language**: Inspired by SQL for graph queries

**What rBuilder Adopted**:
- 🔄 Cypher-inspired query language (planned Phase 12.4)
- 🔄 Pattern matching: `(a)-[:CALLS*1..3]->(b)`
- 🔄 Path queries: `shortestPath()`
- ❌ Did NOT adopt: External database (rBuilder uses in-memory Rust)

**Design Decision**: Stay Rust-native, no external databases for simplicity and performance

---

### GraphQL
**Website**: [graphql.org](https://graphql.org/)

**What rBuilder Adopted**:
- ✅ Query-by-example pattern
- ✅ Field selection concept
- ✅ Type-based filtering

**rBuilder Implementation**:
- `src/gql/` - Custom graph query language (GQL)
- Not GraphQL itself, but inspired by its declarative style

---

## 9. Infrastructure as Code (2024-2025)

### Ansible Security Best Practices
**Source**: Ansible Documentation, Red Hat

**What rBuilder Adopted**:
- ✅ Shell/command/raw module security checks
- ✅ Privilege escalation detection (become: yes)
- ✅ Hardcoded secret detection
- ✅ File permission validation

---

### Chef Security Guidelines
**Source**: Chef/Progress Documentation

**What rBuilder Adopted**:
- ✅ Execute/bash/script resource validation
- ✅ Command injection in resource properties
- ✅ File mode security (0666, 0777 detection)

---

### Puppet Security Patterns
**Source**: Puppet Documentation

**What rBuilder Adopted**:
- ✅ Exec resource validation
- ✅ lookup() vs hardcoded secrets
- ✅ Variable interpolation safety

---

## Implementation Status Summary

| Category | Papers/Systems | Status |
|----------|----------------|--------|
| **Code Graphs** | 6 papers | ✅ Implemented |
| **Program Analysis** | 5 classic papers | ✅ CFG/PDG/Dominance/Slicing |
| **Label Propagation** | 4 papers | 🔄 Partially (Phase 19+) |
| **Community Detection** | 4 papers | ✅ Louvain, 🔄 Label-aware |
| **GNN & Classification** | 3 papers | 📚 Reference only |
| **Migration** | 1 benchmark | ✅ Use case defined |
| **Security** | OWASP + CWE | ✅ 16+ patterns |
| **Tools** | 4 systems | ✅ Tree-sitter, MCP |

**Legend**:
- ✅ Implemented
- 🔄 Partially implemented / Planned
- 📚 Reference only (not yet implemented)
- ❌ Considered but not adopted

---

## Academic Impact

### Citations for rBuilder

If citing rBuilder in academic work, please reference:

```bibtex
@software{rbuilder2026,
  title={rBuilder: Knowledge Graph System for Code Analysis and AI Agents},
  author={Syed, Shaaf},
  year={2026},
  url={https://github.com/sshaaf/rBuilder},
  note={Built on research from Codebadger (ICSE 2026), CodexGraph (NAACL 2025),
        Cooper-Harvey-Kennedy (SoftPrac 2001), and Knowledge Graph Enhanced 
        Community Detection (ACM WSDM 2019)}
}
```

### Primary Research Influences

The four most influential papers for rBuilder:

1. **Codebadger (2026)** - CFG/PDG/taint analysis architecture
2. **CodexGraph (2025)** - Graph schema design, dual-agent queries
3. **Cooper-Harvey-Kennedy (2001)** - Dominator tree algorithm
4. **Knowledge Graph Enhanced Community Detection (2019)** - Label-aware communities

---

## Future Research Directions

Based on recent papers, future enhancements could include:

1. **GraphRAG Integration** - Combine knowledge graphs with RAG for code search
2. **Graph Transformers** - Attention mechanisms for code understanding
3. **Temporal Graphs** - Track code evolution over time
4. **Heterogeneous Graphs** - Integrate code + documentation + issues
5. **Active Learning** - Semi-supervised labeling for incomplete data

---

## Acknowledgments

rBuilder stands on the shoulders of giants. We are grateful to:

- **Codebadger team** (Ahmed Lekssays) for pioneering CPG+LLM integration
- **CodexGraph team** (Xiangyan Liu et al.) for dual-agent query architecture
- **Tree-sitter team** (Max Brunsfeld) for the parsing infrastructure
- **Anthropic** for MCP protocol
- **Classic compiler researchers** (Cooper, Harvey, Kennedy, Lengauer, Tarjan, Ferrante, Weiser, Cytron) for foundational algorithms
- **OWASP & MITRE** for security pattern databases

---

**Document Version**: 1.0  
**Last Updated**: June 18, 2026  
**Next Review**: Quarterly (September 2026)  
**Maintained by**: rBuilder Core Team

For questions about research citations or collaborations, open an issue at:
https://github.com/sshaaf/rBuilder/issues
