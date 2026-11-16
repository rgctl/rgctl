# `rgctl-analysis`

High-performance graph analytics, reachability, and Hybrid Code Property Graph (CPG) engine for `rgctl`. Operates over zero-copy CSR projections (`CodeGraphCsr`, `FlatGraphIndex`) and the indexed v3 `.rgctl/analysis/cfg_pdg.bin` archive.

For the full 6-section architecture specification, mathematical formulations, and binary layouts, see [**`docs/analysis-architecture.md`**](../../docs/analysis-architecture.md) and the subsystem design documents in [**`docs/design/`**](../../docs/design/README.md).

---

## Core Subsystems & Verified Constants

| Subsystem | Key Modules | Algorithmic Highlights & Verified Constants |
| :--- | :--- | :--- |
| **Centrality & Importance** | [`centrality.rs`](src/centrality.rs), [`centrality_approx.rs`](src/centrality_approx.rs) | • **`FastPageRank`**: Pull-based CSR power iteration ($d = 0.85$, `PARALLEL_THRESHOLD = 4_096`, adaptive 8-iter mode above `LARGE_GRAPH_PAGERANK_NODE_LIMIT = 500_000`) with `NodeRole` filtering.<br>• **`SampledBetweenness`**: Exact Brandes for $V \le 500$ (`DEFAULT_EXACT_CENTRALITY_LIMIT`), switching to $K = 512$ (`DEFAULT_SAMPLE_PIVOTS`) degree-stratified pivots for $V > 500$.<br>• **`HyperBallHarmonic`**: Exact 64-bit `BitSet` propagation for $V \le 8,192$ (`HYPERBALL_EXACT_THRESHOLD`); parallel HyperLogLog ($p = 14$ / $12$ / $10$, `DEFAULT_HYPERBALL_ROUNDS = 16`) for $V > 8,192$. |
| **Community Detection** | [`community.rs`](src/community.rs), [`community_label.rs`](src/community_label.rs) | • **Fast-Leiden** (local modularity moving + BFS connected-subgraph refinement + graph coarsening).<br>• **`HubStripPolicy::MeanPlusFourSigma`**: Strips utility hubs with degree $> \max(32, \mu + 4\sigma)$ before clustering and re-assigns by neighbor majority vote.<br>• TF-IDF + structural anchor community labeling (`rgctl communities label`). |
| **Blast Radius & Reachability** | [`blast_radius.rs`](src/blast_radius.rs), [`blast_radius_scc.rs`](src/blast_radius_scc.rs) | • Kosaraju SCC condensation into a DAG (`CondensedDag`).<br>• Adaptive 64-bit word bitsets vs. flat CSR BFS with exponential distance decay.<br>• Supports both callable (`Function`, `Method`) and container (`Class`, `Interface`, `Struct`, `Enum`, `Module`) seeds. |
| **CFG, Dominance & PDG** | [`cfg.rs`](src/cfg.rs), [`dominance.rs`](src/dominance.rs), [`dataflow.rs`](src/dataflow.rs), [`pdg.rs`](src/pdg.rs), [`cfg_pdg_archive.rs`](src/cfg_pdg_archive.rs) | • Single-parse tree-sitter AST reuse (`extract_with_cfg_pdg`).<br>• Iterative Cooper–Harvey–Kennedy dominance (`idom`) + reverse-CFG post-dominance (`pdom`).<br>• Lexical `(name, scope_id)` reaching definitions.<br>• Indexed v3 `cfg_pdg.archive.bin` archive (`RBCP` v3 header + sorted 32B `TocEntry` footer + `TOC3` trailer) with lazy single-function append (`ensure_cfg_pdg_for_source`). |
| **Program Slicing, CPG & Taint** | [`slicing.rs`](src/slicing.rs), [`interprocedural_slicing.rs`](src/interprocedural_slicing.rs), [`taint.rs`](src/taint.rs), [`field_write.rs`](src/field_write.rs) | • Intra- and inter-procedural backward/forward slicing and chopping (`InterproceduralCFG`).<br>• Field-sensitive taint propagation (`base.field`), CWE rule packs, sanitizer kill sets, and `.sink_taint` parked reachability.<br>• Cross-function field mutation indexing (`FieldWriteIndex`, `rgctl cpg mutations`). |
| **Clones, MRO & Architecture** | [`clones.rs`](src/clones.rs), [`fragment_clones.rs`](src/fragment_clones.rs), [`mro.rs`](src/mro.rs), [`dependency.rs`](src/dependency.rs) | • Type 1/2/3 token/AST clone detection + 2-iteration 1-WL SESE hammock fragment clones.<br>• C3 superclass linearization (`mro.rs`) emitting cross-file `Overrides` edges.<br>• Layered architecture & cycle detection (`rgctl check`). |

---

## Testing & Benchmarks

```bash
# Strict clippy gate (enforced in CI and preflight-pr.sh)
cargo clippy -p rgctl-analysis -- -D warnings

# Unit tests
cargo test -p rgctl-analysis

# Microbenchmarks
cargo bench -p rgctl-analysis --bench centrality_benchmarks
cargo bench -p rgctl-analysis --bench community_benchmarks
cargo bench -p rgctl-analysis --bench blast_radius_benchmarks
cargo bench -p rgctl-analysis --bench analysis_benchmarks
```
