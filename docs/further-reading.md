# Further Reading

Research that informs rBuilder — what we **implement today**, what we **read for inspiration**, and where **you** can push the project forward.

If a paper sparks an idea (new relation type, better slice precision, agent retrieval pattern), open a [GitHub issue](https://github.com/sshaaf/rBuilder/issues) or PR and point at the row below. We treat this doc as a living map, not marketing copy.

**See also:** [Introduction](Introduction.md) (features) · [Performance engineering](performance-engineering.md) (latency tiers)

---

## Research foundations in rBuilder

Legend: **Implemented** = algorithm or structure in the codebase with tests; **Inspired** = aligned design, not a full reproduction; **Reading** = related work, not yet in code.

### Classic program analysis

| Work | Status | rBuilder | CLI / docs |
|------|--------|----------|------------|
| [Ferrante et al. — Program Dependence Graph (TOPLAS 1987)](#14-the-program-dependence-graph-and-its-use-in-optimization-toplas-1987) | **Implemented** | [`pdg.rs`](../crates/rbuilder-analysis/src/pdg.rs) — data + control deps | [`inspect`](Introduction.md#cfg-pdg-and-dominance-deep-structure), [`slice`](Introduction.md#program-slicing) |
| [Weiser — Program slicing (ICSE 1981)](https://dl.acm.org/doi/10.1145/800078.802466) | **Implemented** | [`slicing.rs`](../crates/rbuilder-analysis/src/slicing.rs), [`interprocedural_slicing.rs`](../crates/rbuilder-analysis/src/interprocedural_slicing.rs) | `rbuilder slice` — [User Guide §8](user-guide.md#8-program-slicing-and-taint) |
| Reaching-definitions dataflow (standard compiler analysis) | **Implemented** | [`dataflow.rs`](../crates/rbuilder-analysis/src/dataflow.rs) → PDG data edges | via `discover --cfg` |
| [Cooper, Harvey & Kennedy — Simple fast dominance (SPE 2001)](https://doi.org/10.1002/spe.3780310304) | **Implemented** | [`dominance.rs`](../crates/rbuilder-analysis/src/dominance.rs) (iterative CHK-style) | `rbuilder inspect` |
| Control-flow graphs | **Implemented** | [`cfg.rs`](../crates/rbuilder-analysis/src/cfg.rs), [`cfg_builder.rs`](../crates/rbuilder-analysis/src/cfg_builder.rs) | `discover --cfg`, `inspect` |
| Forward taint (source → sink + sanitizers) | **Implemented** | [`taint.rs`](../crates/rbuilder-analysis/src/taint.rs) | `rbuilder slice --taint` — [Introduction § Taint](Introduction.md#taint-analysis) |

**Tests:** [`slicing.rs`](../tests/slicing.rs), [`taint_security.rs`](../tests/taint_security.rs) (CWE-oriented taint/security patterns).

### Graph algorithms & architecture metrics

| Work | Status | rBuilder | CLI |
|------|--------|----------|-----|
| [Page & Brin — PageRank (1998)](https://doi.org/10.1109/69.681760) | **Implemented** | [`centrality.rs`](../crates/rbuilder-analysis/src/centrality.rs) — `FastPageRank` | `rbuilder metrics --pagerank` |
| [Brandes — Betweenness centrality (2001)](https://doi.org/10.1080/00207160108942084) | **Implemented** | [`centrality.rs`](../crates/rbuilder-analysis/src/centrality.rs) — Brandes on filtered projections | `rbuilder metrics --betweenness` |
| [Raghavan et al. — Label propagation (2007)](https://doi.org/10.1107/S1744309107073516) + Newman modularity | **Implemented** | [`community.rs`](../crates/rbuilder-analysis/src/community.rs) | `rbuilder metrics --communities` |

**Tests:** [`centrality_audit.rs`](../tests/centrality_audit.rs).

### Reachability, blast radius, and the “R”

| Idea | Status | rBuilder | CLI |
|------|--------|----------|-----|
| Sparse pre-computed call reachability | **Implemented** (rBuilder engineering) | Blast engine + compressed snapshots — see [performance-engineering.md](performance-engineering.md) | `rbuilder blast-radius`, `rbuilder check` |
| Rich relation matrix (30+ edge types) | **Implemented** | [`schema.rs`](../crates/rbuilder-graph/src/schema.rs), extraction pipeline | `rbuilder gql`, `rbuilder export` |

This is the core differentiator for **LLM agents**: deterministic reachability answers in compact JSON instead of dumping whole files into context.

### Modern code graphs & LLM agents

| Work | Status | Overlap with rBuilder | Gap / opportunity |
|------|--------|----------------------|-------------------|
| [CodexGraph (NAACL 2025)](#2-codexgraph-bridging-large-language-models-and-code-repositories-via-code-graph-databases-naacl-2025) | **Inspired** | GQL, rich node metadata, `-f json`, [JSON API](json-api.md), export | Dual-agent “write then translate” query planner not implemented — good contribution target |
| [Codebadger — CPG + LLM (ICSE 2026)](https://arxiv.org/abs/2603.24837) | **Inspired** | CFG + PDG + slice + taint stack (CPG-shaped) | No Joern import; interprocedural taint depth varies — see [TASK_PLAN](../.github/TASK_PLAN.md) Phase 12/13 |
| [TAILOR / hybrid AST+CFG+DFG](#7-learning-graph-based-code-representations-for-source-code-tailor) | **Reading** | Tree-sitter AST + CFG/PDG layer | Learned embeddings not in scope today |
| [Reliable Graph-RAG for Codebases (2026)](#5-reliable-graph-rag-for-codebases-ast-derived-graphs-vs-llm-extracted-knowledge-graphs-arxiv-2026) | **Aligned** | AST-derived graph via Tree-sitter, not LLM-extracted KG | Benchmark comparisons welcome |

### Security standards

| Standard | Status | rBuilder |
|----------|--------|----------|
| [CWE](https://cwe.mitre.org/) / OWASP-style categories | **Implemented** (pattern + taint hooks) | [`taint.rs`](../crates/rbuilder-analysis/src/taint.rs), [`taint_security.rs`](../tests/taint_security.rs) — SQLi (CWE-89), XSS (CWE-79), command injection (CWE-78), etc. |

---

## Ideas we welcome

Read a paper above and want to land it in rBuilder? High-value openings:

1. **Interprocedural taint** with sanitizer summaries across call boundaries (Codebadger / CPG literature).
2. **Query planning for agents** — natural language → GQL / blast-radius without token-heavy file reads (CodexGraph direction).
3. **Migration-specific relations** — framework API pairs, deprecated symbol tracking (ReCode / environment-in-the-loop papers).
4. **Benchmarks** — publish repro scripts comparing rBuilder JSON output vs file-grep baselines on coolstore or your repo.
5. **Cross-language reachability** — richer IMPORTS / IMPLEMENTS for polyglot monorepos.

Open an issue with the paper link, which row in the table above you extend, and a sketch of the CLI or JSON shape you want.

---

## External bibliography

Research papers on code graphs, migration, LLM agents, and program analysis — for depth beyond what rBuilder ships today.

### Language & Framework Migration

1. **Environment-in-the-Loop: Rethinking Code Migration with LLM-based Agents** (arXiv 2026, ReCode 2026)
   - [arXiv Paper](https://arxiv.org/html/2602.09944v1)
   - [ReCode 2026 Conference](https://conf.researchr.org/details/recode26/recode-2026-papers/5/Environment-in-the-Loop-Rethinking-Code-Migration-with-LLM-based-Agents)
   - Proposes an LLM-based environment-driven migration framework to replace linear manual processes
   - Highlights that LLMs perform poorly (~30% runtime errors) without actual environment interaction
   - Covers refactoring, API adaptation, and dependency updates

2. **CodexGraph: Bridging Large Language Models and Code Repositories via Code Graph Databases** (NAACL 2025)
   - [arXiv](https://arxiv.org/abs/2408.03910)
   - [PDF](https://aclanthology.org/2025.naacl-long.7.pdf)
   - [ACL Anthology](https://aclanthology.org/2025.naacl-long.7/)
   - Integrates LLM agents with graph database interfaces for code structure-aware context retrieval
   - Evaluated on CrossCodeEval, SWE-bench, and EvoCodeBench benchmarks
   - Uses structural properties of graph databases for precise retrieval
   - **rBuilder:** [Inspired — see table above](#modern-code-graphs--llm-agents) (GQL + JSON; dual-agent planner is a gap)

2b. **Codebadger: Bridging Code Property Graphs and Language Models** (ICSE 2026)
   - [arXiv](https://arxiv.org/abs/2603.24837)
   - CPG + MCP + backward slicing for vulnerability analysis; reports large context reduction via slicing
   - **rBuilder:** [Inspired — see table above](#modern-code-graphs--llm-agents) (CFG/PDG/slice/taint; not Joern-compatible)

3. **Code Graph Model (CGM): A Graph-Integrated Large Language Model** (arXiv 2025, NeurIPS 2025)
   - [arXiv PDF](https://arxiv.org/pdf/2505.16901)
   - [OpenReview](https://openreview.net/forum?id=b98ODdeYq5)
   - Built on open-source LLMs enhanced through agentless Graph RAG framework
   - Four modules: Rewriter, Retriever, Reranker, and Reader

4. **CodeGRAG: Bridging Natural Language and Programming Language via Graphical RAG** (arXiv 2024/2025)
   - [arXiv](https://arxiv.org/abs/2405.02355)
   - Builds graphical views based on control flow and data flow
   - Facilitates LLM understanding of code syntax

### AST & Graph-Based Code Transformation

5. **Reliable Graph-RAG for Codebases: AST-Derived Graphs vs LLM-Extracted Knowledge Graphs** (arXiv 2026)
   - [arXiv](https://arxiv.org/html/2601.08773v1)
   - Benchmarks AST-derived Knowledge Graph RAG built via Tree-sitter parsing
   - Focuses on static code analysis and software maintenance

6. **AST-Enhanced or AST-Overloaded? The Surprising Impact of Hybrid Graph Representations on Code Clone Detection** (arXiv 2025)
   - [arXiv](https://arxiv.org/html/2506.14470v1)
   - Shows ASTs dominate deep learning approaches for code analysis
   - Enriches AST representations with CFGs and DFGs

7. **Learning Graph-based Code Representations for Source Code (TAILOR)**
   - [PDF](https://jun-zeng.github.io/file/tailor_paper.pdf)
   - Uses Code Property Graphs (CPG) combining AST, CFG, and DFG
   - Encodes both syntax and semantics

8. **Improving AST-Level Code Completion with Graph Retrieval and Multi-Field Attention** (ICPC 2024)
   - [ACM DL](https://dl.acm.org/doi/10.1145/3643916.3644420)
   - Graph-based retrieval for AST-level code completion

### Language Translation & Transpilation

9. **MISIM: A Neural Code Semantics Similarity System**
   - [arXiv](https://ar5iv.labs.arxiv.org/html/2006.05265)
   - Explicitly addresses language-to-language translation (transpilation)
   - Uses context-aware semantics structure (CASS) to lift semantic meaning from code syntax
   - Compares classical AST representations (code2vec, code2seq) with XFG and SPT

10. **User-Customizable Transpilation of Scripting Languages (DuoGlot)** (OOPSLA 2023)
    - [ACM DL](https://dl.acm.org/doi/10.1145/3586034)
    - Translates Python to JavaScript with 90% accuracy
    - User-customizable transpilation framework

11. **Code Transformation by Direct Transformation of ASTs** (IWST)
    - [ACM DL](https://dl.acm.org/doi/10.1145/2811237.2811297)
    - Direct AST transformation approaches

### Dependency Management & Migration

12. **DepsRAG: Managing Software Dependencies using Large Language Models** (arXiv 2024)
    - [arXiv](https://arxiv.org/html/2405.20455v3)
    - Constructs direct and transitive dependencies as Knowledge Graph
    - Addresses software supply chain security

13. **Knowledge Graph Based Repository-Level Code Generation** (ICSE 2025, LLM4Code)
    - [ICSE 2025 Conference](https://conf.researchr.org/details/icse-2025/llm4code-2025-papers/26/Knowledge-Graph-Based-Repository-Level-Code-Generation-Virtual-Talk-)
    - [Industry Perspective (Quantiphi)](https://quantiphi.com/blog/bridging-code-and-context-a-knowledge-graph-based-repository-level-code-generation/)
    - Transforms code repositories into knowledge graphs
    - Hybrid search systems for retrieving relevant sub-graphs

### Classic Foundational Work

14. **The Program Dependence Graph and Its Use in Optimization** (TOPLAS 1987)
    - [ACM DL](https://dl.acm.org/doi/10.1145/24039.24041)
    - [PDF](https://www.cs.utexas.edu/~pingali/CS395T/2009fa/papers/ferrante87.pdf)
    - Seminal work on PDG representing data and control dependences
    - Foundation for program slicing and transformation
    - **rBuilder:** [Implemented](../crates/rbuilder-analysis/src/pdg.rs) — see [classic program analysis table](#classic-program-analysis)

15. **Program Slicing** (Weiser, ICSE 1981)
    - [ACM DL](https://dl.acm.org/doi/10.1145/800078.802466)
    - Original backward slicing criterion
    - **rBuilder:** [Implemented](../crates/rbuilder-analysis/src/slicing.rs)

16. **A Simple, Fast Dominance Algorithm** (Cooper, Harvey & Kennedy, SPE 2001)
    - [DOI](https://doi.org/10.1002/spe.3780310304)
    - **rBuilder:** [Implemented](../crates/rbuilder-analysis/src/dominance.rs)

17. **Enhancing program dependency graph based clone detection using approximate subgraph matching**
    - [PDF](https://www.academia.edu/63870307/Enhancing_program_dependency_graph_based_clone_detection_using_approximate_subgraph_matching)
    - **rBuilder:** Reading — clone detection not a CLI feature today; PDG substrate exists

## Survey & Review Papers

- **Graph Retrieval-Augmented Generation: A Survey** (ACM TOIS)
  - [ACM DL](https://dl.acm.org/doi/10.1145/3777378)
  
- **Awesome-Code-as-Agent-Harness-Papers** - Curated list of papers
  - [GitHub](https://github.com/YennNing/Awesome-Code-as-Agent-Harness-Papers)
  
- **AwesomeLLM4SE** (SCIS 2025) - Survey on LLMs for Software Engineering
  - [GitHub](https://github.com/iSEngLab/AwesomeLLM4SE)

- **A Multi-Perspective Investigation into Code Migration for Large Language Models**
  - [Paper](https://ace.ewapub.com/article/view/32228)

## Industry & Applied Research

- **AI-Powered Legacy App Modernization to Reduce Transformation Costs**
  - [Mobisoft Infotech](https://mobisoftinfotech.com/resources/blog/ai-legacy-application-modernization)
  
- **Refactoring & Migrations with AI: Smarter Code Transformation at Scale**
  - [GitNation Talk](https://gitnation.com/contents/refactoring-and-migrations-with-ai-smarter-code-transformation-at-scale)
  
- **LLMs for Legacy System Migration: A Modern Guide**
  - [CloseLoop](https://closeloop.com/blog/llms-in-legacy-system-migration/)

- **How AI Knowledge Graphs Turn Legacy Code into Structured Intelligence**
  - [SoftwareSeni](https://www.softwareseni.com/how-ai-knowledge-graphs-turn-legacy-code-into-structured-intelligence/)

## Key Findings from Recent Research (2025-2026)

### Effectiveness Metrics
- **60-70% reduction** in code-understanding time with LLM-assisted analysis
- **40-60% drop** in migration effort for COBOL-to-Java/Python migrations
- Work that previously took a senior engineer **three weeks** now completed in **three to five days**

### Technical Performance
- Recall improvements: Prompt enhancement yields **15.6% → 86.7%** recall in refactoring opportunity identification
- DuoGlot achieves **90% translation accuracy** for Python to JavaScript
- LLMs show nearly **30% runtime errors** without environment interaction

### Common Graph Structures
- **Code Property Graphs (CPG)**: Combining AST + CFG + DFG dominates modern approaches
- **AST-derived Knowledge Graphs**: Via Tree-sitter parsing for static analysis
- **Hybrid representations**: Enriching ASTs with control flow and data flow graphs

### Emerging Trends
- **Hybrid approaches**: Combining symbolic (graph-based) and neural (LLM) methods
- **Environment-in-the-loop**: Moving beyond static analysis to actual execution feedback
- **Graph RAG**: Retrieval-Augmented Generation using code graphs for context
- **Repository-level understanding**: Moving from file-level to full codebase comprehension

## Research Venues & Conferences

Active publication venues for this research area:
- NeurIPS (Neural Information Processing Systems)
- NAACL (North American Chapter of the ACL)
- ICSE (International Conference on Software Engineering)
- OOPSLA (Object-Oriented Programming, Systems, Languages & Applications)
- TOPLAS (ACM Transactions on Programming Languages and Systems)
- ICPC (International Conference on Program Comprehension)
- ReCode (International Workshop on Refactoring and Code Evolution)
- LLM4Code Workshop

## Related Topics

- **Program Slicing**: Using PDGs to extract relevant program subsets
- **Clone Detection**: Graph-based similarity for identifying code duplicates
- **API Migration**: Automated adaptation to new library versions
- **Monolith to Microservices**: Decomposition using dependency analysis
- **Software Supply Chain Security**: Dependency graph analysis for vulnerabilities
- **Code Completion**: AST-based context for intelligent suggestions

---

*Last updated: 2026-07-06*
