# `rgctl` Graph Analysis, CPG & Reachability Architecture

This document specifies the **macro-level graph analytics and intra-procedural Code Property Graph (CPG) architecture** of `rgctl` (`crates/rgctl-analysis`). It details the three-tier memory model, CSR projection and topology residency, centrality and community detection algorithms, SCC-condensed blast-radius reachability, and the indexed v3 `cfg_pdg.bin` archive.

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ TIER 1: BOUNDED STREAMING INGEST (`crates/rgctl-extraction` + `rgctl-graph`) │
│                                                                              │
│  Source Files ──► Rayon Workers ──► `SegmentedSpill` (`256 MiB` sort runs)   │
│    • Optional single-parse AST reuse (`extract_with_cfg_pdg`)                │
│    • Writes `.rgctl/graph.snapshot.bin` (`RGCL` v2, `content_digest`)        │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ TIER 2: ZERO-COPY MMAP & CSR TOPOLOGY PROJECTION (`crates/rgctl-analysis`)   │
│                                                                              │
│  `ColumnarGraphMmap` (`.rgctl/graph.snapshot.bin`)                           │
│    ├─► `StructuralTopology` / `CodeGraphCsr` / `FlatGraphIndex`:             │
│    │     • Dense `0..V` `u32` vertex indexing                                │
│    │     • Filtered architectural CSR (`Calls`, `Imports`, `Inherits`,       │
│    │       `Implements`, `References`, `Uses`)                               │
│    │     • `NodeRole` classification (excludes `Test`, `Generated`, `File`,  │
│    │       `Module`, `Directory` from architectural centrality by default)   │
│    └─► `ColdMetadataDb` (`.rgctl/metadata.db/`):                             │
│          • Fetched ONLY on demand for output formatting, slicing, or diffing │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ TIER 3: MACRO ANALYTICS & HYBRID CPG ARTIFACTS (`.rgctl/`)                   │
│                                                                              │
│  A. Macro Graph Analytics (`.rgctl/analysis.snapshot.bin`):                  │
│     • `FastPageRank`: pull-based CSR power iteration (`d = 0.85`)            │
│     • `SampledBetweenness`: Brandes exact (`V <= 500`) or `K = 512` pivots   │
│     • `HyperBallHarmonic`: exact bitset (`V <= 8,192`) or HLL (`p=14/12/10`) │
│     • `Fast-Leiden`: local moving + BFS connected refinement + coarsening    │
│       with `mu + 4*sigma` `HubStripPolicy`                                   │
│                                                                              │
│  B. Blast Radius & Call Reachability:                                        │
│     • `.rgctl/blast_engine.snapshot.bin` (Kosaraju SCC DAG + bitset/BFS)     │
│     • `.rgctl/macro_call_index.db` (transitive function call bitsets)        │
│                                                                              │
│  C. Hybrid CPG Archive (`.rgctl/analysis/cfg_pdg.bin`, v3 TOC):              │
│     • 40-byte header + 32-byte `TocEntry` table sorted by `func_idx`         │
│     • Per-function bincode `FunctionCfgPdg` (CFG + CHK Dominance + PDG)      │
│     • Populated eagerly via `discover --with-cfg` or lazily via              │
│       `ensure_cfg_pdg_for_source` during `slice` / `cpg flows`               │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Pull-Based CSR PageRank** | Page, Brin, Motwani & Winograd (1998); Gargiulo et al. (2015), *Componentwise PageRank* | [`FastPageRank`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality.rs#L170-L360) precomputes `inv_out_deg[u] = 1.0 / out_deg(u)` and pulls contributions over incoming CSR slices (`in_offsets`, `in_neighbors`) with Rayon parallelism above `PARALLEL_THRESHOLD = 4_096` and `NodeRole` filtering. |
| **Exact & Degree-Stratified Brandes Betweenness** | Brandes (2001), *A Faster Algorithm for Betweenness Centrality*; Geisberger, Sanders & Schultes (2008) | [`CentralityCalculator::betweenness_centrality`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality.rs#L520-L640) runs exact Brandes for $V \le 500$ (`DEFAULT_EXACT_CENTRALITY_LIMIT`) and switches to [`SampledBetweenness`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality_approx.rs#L65-L260) ($K = 512$ degree-stratified pivots, `DEFAULT_SAMPLE_PIVOTS`) with linear scaling $N/K$ for $V > 500$. |
| **HyperBall Harmonic Closeness Centrality** | Boldi & Vigna (2013), *In-Core Computation of Geometric Centralities with HyperBall* | [`HyperBallHarmonic`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality_approx.rs#L275-L520) computes $H(v) = \sum_{d \ge 1} \frac{|B(v, d)| - |B(v, d-1)|}{d}$ using exact 64-bit word `BitSet` propagation when $V \le 8,192$ (`HYPERBALL_EXACT_THRESHOLD`) and adaptive HyperLogLog counters ($p \in \{14, 12, 10\}$) for larger graphs. |
| **Fast-Leiden Community Detection** | Traag, Waltman & van Eck (2019), *From Louvain to Leiden: Guaranteeing Well-Connected Communities* | [`CommunityDetector`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/community.rs#L1-L320) executes modularity-driven local moving, BFS connected-component refinement (preventing disconnected Louvain communities), and graph coarsening after stripping $\mu + 4\sigma$ utility hubs (`HubStripPolicy`). |
| **Kosaraju SCC Condensation & Bitset Reachability** | Kosaraju (1978); Nuutila & Soisalon-Soininen (1994) | [`BlastRadiusEngine`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/blast_radius.rs#L1-L240) collapses recursive dependency cycles into a Condensed DAG of Strongly Connected Components (SCCs) and computes transitive reachability using 64-bit word bitsets (for dense subgraphs) or flat CSR BFS with distance decay $e^{-\lambda \cdot d}$. |
| **Simple Fast Dominance (CHK) & Post-Dominance** | Cooper, Harvey & Kennedy (2001), *A Simple, Fast Dominance Algorithm*; Lengauer & Tarjan (1979) | [`DominanceAnalyzer`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/dominance.rs#L1-L260) computes immediate dominators (`idom`), dominance frontiers (`DF`), and reverse-CFG post-dominators (`pdom`) to derive control dependencies. |
| **Program Dependence Graph (PDG) & Slicing** | Ferrante, Ottenstein & Warren (1987), *The Program Dependence Graph*; Weiser (1981), *Program Slicing* | [`PdgBuilder`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/pdg.rs#L1-L220) combines post-dominance control edges with lexical `(name, scope_id)` reaching-definition dataflow ([`DataflowAnalyzer`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/dataflow.rs#L1-L180)); [`BackwardSlicer`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/slicing.rs#L1-L200) and [`InterproceduralCFG`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/interprocedural_slicing.rs#L1-L220) traverse intra- and inter-procedural slices. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Tier-2 CSR Projection & Memory Residency (`StructuralTopology` / `CodeGraphCsr` / `FlatGraphIndex`)

During macro analysis, `rgctl-analysis` never loads `ColdMetadataDb` (`NodeExtension` strings, parameters, properties). Instead, it projects the hot mmap into compact flat arrays:

| Structure | Source File | Per-Node / Per-Edge Cost | Role |
| :--- | :--- | :--- | :--- |
| [`StructuralTopology`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/columnar_snapshot.rs#L520-L590) | `crates/rgctl-graph/src/columnar_snapshot.rs` | `1 B` `NodeType` + `4 B` CSR offsets + `4 B` targets | Minimal topology projection extracted directly from `ColumnarGraphMmap` without decoding strings. |
| [`CodeGraphCsr`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/csr.rs#L1-L140) | `crates/rgctl-analysis/src/csr.rs` | `out_offsets: Vec<u32>`, `out_targets: Vec<u32>`, `in_offsets: Vec<u32>`, `in_sources: Vec<u32>`, `node_roles: Vec<NodeRole>` | Cache-friendly CSR projection used by `FastPageRank`, `CommunityDetector`, and `BlastRadiusEngine`. |
| [`FlatGraphIndex`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/flat_index.rs#L1-L150) | `crates/rgctl-analysis/src/flat_index.rs` | `adj_offsets: Vec<u32>`, `adj_targets: Vec<u32>`, `rev_offsets: Vec<u32>`, `rev_sources: Vec<u32>` | Contiguous `u32` forward/reverse index shared by `SampledBetweenness` and `HyperBallHarmonic`. |

### 3.2 Verified Centrality, HyperBall & Community Constants

All constants below are verified directly against [`crates/rgctl-analysis/src/centrality.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality.rs), [`crates/rgctl-analysis/src/centrality_approx.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality_approx.rs), and [`crates/rgctl-analysis/src/community.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/community.rs):

| Constant | Value | File | Behavior |
| :--- | :--- | :--- | :--- |
| `PARALLEL_THRESHOLD` | `4_096` | `centrality.rs`, `centrality_approx.rs` | Minimum node count $V$ before Rayon parallel iterators activate (`par_iter`). |
| `LARGE_GRAPH_PAGERANK_NODE_LIMIT` | `500_000` | `centrality.rs` | Above 500K nodes, `FastPageRank` adapts from `20` iterations / `1e-6` tolerance to `8` iterations / `1e-4` tolerance (`LARGE_GRAPH_PAGERANK_MAX_ITERATIONS = 8`). |
| `DEFAULT_EXACT_CENTRALITY_LIMIT` | `500` | `centrality_approx.rs` | Graphs with $V \le 500$ run exact $O(V \cdot E)$ Brandes betweenness; $V > 500$ switches to `SampledBetweenness`. |
| `DEFAULT_SAMPLE_PIVOTS` ($K$) | `512` | `centrality_approx.rs` | Default number of degree-stratified Brandes BFS pivot sources for `SampledBetweenness` (50% top total-degree hubs + 50% uniform hash stride). |
| `HYPERBALL_EXACT_THRESHOLD` | `8_192` | `centrality_approx.rs` | Graphs with $V \le 8,192$ run exact 64-bit word `BitSet` ball propagation (`hyperball_exact`) with zero HyperLogLog approximation error. |
| `HYPERLOGLOG_PRECISION` ($p$) | `14` (`12` / `10` adaptive) | `centrality_approx.rs` | HLL register precision: $p = 14$ ($m = 16,384$ registers) for $V \le 8,192$; $p = 12$ ($m = 4,096$) for $8,192 < V \le 100,000$; $p = 10$ ($m = 1,024$) for $V > 100,000$. |
| `DEFAULT_HYPERBALL_ROUNDS` | `16` (`8` when $V > 500,000$) | `centrality_approx.rs` | Maximum diameter rounds for `HyperBallHarmonic` (`LARGE_GRAPH_HYPERBALL_ROUNDS = 8` above `LARGE_GRAPH_HYPERBALL_NODE_LIMIT = 500_000`). |
| `HubStripPolicy::MeanPlusFourSigma` | $\mu + 4\sigma$ (`min_degree = 32`) | `community.rs` | Strips omnipresent utility hubs whose total degree exceeds $\max(32, \mu + 4\sigma)$ prior to Fast-Leiden community clustering, then re-attaches them by majority neighbor vote. |

### 3.3 Hybrid CPG Archive v3 Binary Layout (`.rgctl/analysis/cfg_pdg.archive.bin`)

Implemented in [`crates/rgctl-analysis/src/cfg_pdg_archive.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs). Supports both whole-repo discovery (`rgctl discover --with-cfg`) and lazy single-function append (`ensure_cfg_pdg_for_source`).

#### Header, Payloads, Sorted 32-Byte TOC & 16-Byte EOF Trailer

| Region | Size / Alignment | Fields & Layout | Description |
| :--- | :--- | :--- | :--- |
| **Header** | Variable (`8 B + 8 B + digest_len`) | `magic: [u8; 4]` (`b"RBCP"`), `version: u32` (`3`), `digest_len: u64`, `graph_digest: [u8; digest_len]` | Validates archive format (`ARCHIVE_VERSION = 3`) and matches the parent snapshot `content_digest`. |
| **Record Payloads** | Contiguous (`bincode`) | Sequence of serialized [`CfgPdgRecord`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs#L52-L75) (`function_id`, `code_hash`, `function_name`, `file_path`, `Arc<ControlFlowGraph>`, `Arc<ProgramDependenceGraph>`) | Streamed sequentially during `discover --with-cfg` or appended before the new TOC footer by `ensure_cfg_pdg_for_source`. |
| **Sorted TOC Array** | `toc_count × 32 B` (8-byte aligned) | `key_bytes: [u8; 16]` (UUID bytes, sorted ascending), `offset: u64` (LE), `len: u32` (LE), `flags: u32` (LE: `TOC_FLAG_PRIMARY = 0`, `TOC_FLAG_STABLE_ALIAS = 1`) | [`TocEntry`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs#L126-L157) (`TOC_ENTRY_SIZE = 32`); enables $O(1)$ mmap open and $O(\log F)$ binary search by either graph `NodeId` UUID or `stable_record_uuid(file, name)`. |
| **EOF Trailer** | `16 B` (`TOC_TRAILER_SIZE`) | `toc_offset: u64` (LE), `toc_count: u32` (LE), `footer_magic: u32` (`0x3354_4F43` = `b"TOC3"`) | Located at `file_len - 16` so `CfgPdgArchiveMmap::open` locates the TOC slice in $O(1)$ without scanning payloads. |

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 Analysis & CPG CLI Commands

Verified against `rgctl --help` and [`src/cli/args.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/args.rs):

```bash
# Discover with eager CFG/PDG, taint, or harmonic centrality precomputation
rgctl discover . --with-cfg --with-taint --with-harmonic

# Centrality & community metrics
rgctl metrics --pagerank --betweenness --communities -f json
rgctl communities list -f json
rgctl communities label -f json

# Blast radius & impact reachability (supports function AND container Class/Struct/Module seeds)
rgctl blast-radius <symbol> --max-depth 5 -f json

# Intra-procedural CFG, Dominance, and PDG inspection
rgctl inspect <symbol> cfg --prune -f json
rgctl inspect <symbol> pdg --edge-layer all --def-use -f json
rgctl inspect <symbol> dom --frontiers -f json

# Program slicing (backward, forward, taint, chop) — lazily populates v3 cfg_pdg.archive.bin
rgctl slice <file_or_symbol> --line 42 --variable x -f json
rgctl slice <file_or_symbol> --line 42 --taint -f json

# Hybrid Code Property Graph (CPG) queries
rgctl cpg status -f json
rgctl cpg calls --function <symbol> -f json
rgctl cpg mutations --field <field_name> -f json
rgctl cpg flows --file <path> --line 42 --variable x --direction forward -f json
```

### 4.2 Implementation Map

| Subsystem | Source File | Key Types & Entry Points |
| :--- | :--- | :--- |
| **CSR & Flat Index** | [`crates/rgctl-analysis/src/csr.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/csr.rs), [`flat_index.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/flat_index.rs) | `CodeGraphCsr`, `FlatGraphIndex`, `NodeRole` |
| **PageRank & Exact Centrality** | [`crates/rgctl-analysis/src/centrality.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality.rs) | `FastPageRank`, `PageRankConfig`, `CentralityCalculator` |
| **Approximate Centrality** | [`crates/rgctl-analysis/src/centrality_approx.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality_approx.rs) | `SampledBetweenness`, `HyperBallHarmonic`, `ApproximateCentrality` |
| **Fast-Leiden Communities** | [`crates/rgctl-analysis/src/community.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/community.rs), [`community_naming.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/community_naming.rs) | `CommunityDetector`, `CommunityConfig`, `HubStripPolicy`, `label_communities` |
| **Blast Radius & SCC DAG** | [`crates/rgctl-analysis/src/blast_radius.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/blast_radius.rs), [`reachability.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/reachability.rs) | `BlastRadiusEngine`, `CondensedDag`, `MacroCallIndex` |
| **CFG, Dominance & PDG** | [`cfg.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg.rs), [`dominance.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/dominance.rs), [`dataflow.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/dataflow.rs), [`pdg.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/pdg.rs) | `CfgAnalyzer`, `DominanceAnalyzer`, `DataflowAnalyzer`, `PdgBuilder` |
| **v3 CPG Archive & Lazy Builder** | [`crates/rgctl-analysis/src/cfg_pdg_archive.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs) | `CfgPdgArchiveMmap`, `CfgPdgRecord`, `TocEntry`, `ensure_cfg_pdg_for_source` |
| **Slicing & Taint** | [`slicing.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/slicing.rs), [`interprocedural_slicing.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/interprocedural_slicing.rs), [`taint.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/taint.rs) | `BackwardSlicer`, `ForwardSlicer`, `InterproceduralCFG`, `TaintAnalyzer` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Offload Heavy Analytics from Tokio:** Never invoke `FastPageRank`, `SampledBetweenness`, `CommunityDetector`, or `BlastRadiusEngine` directly on an async Tokio worker thread without `spawn_blocking`.
2. **Never Read `ColdMetadataDb` Inside CSR Loops:** Centrality, community detection, and reachability must operate purely on `CodeGraphCsr` / `FlatGraphIndex` dense `u32` arrays.
3. **Single-Parse AST Reuse:** When `discover --with-cfg` runs, `extract_with_cfg_pdg` builds the CFG and PDG from the same in-memory `tree_sitter::Tree` used for symbol extraction — never re-parse the file a second time.

### Verification Commands

```bash
# Run all rgctl-analysis unit and integration tests (with strict clippy)
cargo clippy -p rgctl-analysis -- -D warnings
cargo test -p rgctl-analysis
cargo test --test centrality_audit --test slicing --test taint_security

# Run Criterion benchmarks for centrality, communities, blast radius, and analysis
cargo bench -p rgctl-analysis --bench centrality_benchmarks
cargo bench -p rgctl-analysis --bench community_benchmarks
cargo bench -p rgctl-analysis --bench blast_radius_benchmarks
cargo bench -p rgctl-analysis --bench analysis_benchmarks
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Active-Pivot Sampling on Sparse Heterogeneous Graphs ([#187](https://github.com/rgctl/rgctl/issues/187)):** `SampledBetweenness` currently selects $K = 512$ pivots across all $0..V$ node indices; on sparse graphs with high ratios of 0-out-degree leaf/non-callable nodes (`File`, `Import`, `Field`), effective active pivot coverage is lower than $K$ (tracked in OpenSpec change `sampled-betweenness-active-pivots`).
2. **Intra-Procedural Reaching Definitions Are Lexical (`(name, scope_id)`):** `DataflowAnalyzer` tracks variable definitions and uses by lexical identifier and nested scope ID without full inter-procedural pointer aliasing (Andersen/Steensgaard). Field mutations are indexed separately via `FieldWriteIndex` (`rgctl cpg mutations`).
3. **Whole-Repo Sink Taint Requires `--with-cfg`:** While `rgctl slice <symbol> --taint` and `rgctl cpg flows` lazily build missing function CFG/PDG entries on demand via `ensure_cfg_pdg_for_source`, whole-repository sink scanning (`rgctl taint --sink <pattern>`) requires `rgctl discover --with-cfg` so all function PDGs are present in `cfg_pdg.bin`.
