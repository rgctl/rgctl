# Graph Centrality & Network Metrics — Engineering Design

This document specifies the **Macro Graph Centrality, Network Metrics, and Git Churn Hotspot subsystem** in `rgctl` (`crates/rgctl-analysis/src/centrality.rs`, `centrality_approx.rs`, `git_churn.rs`, `results.rs`, and `src/cli/metrics.rs`). It covers pull-based CSR `FastPageRank`, exact and degree-stratified `SampledBetweenness` ($K = 512$), exact-bitset and HyperLogLog `HyperBallHarmonic` ($p \in \{14, 12, 10\}$), `NodeRole` filtering, and columnar `.rgctl/analysis_results.bin` persistence.

![Functions tab — centrality columns (gbuilder)](../images/design/graph-metrics/graph-metrics-functions-table.png)

*Figure 1: **Functions** tab — sortable PageRank (PR), betweenness (BC), harmonic (Harm), and blast columns over the function inventory.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. ZERO-COPY CSR PROJECTION (`FlatGraphIndex` / `CodeGraphCsr`)              │
│                                                                              │
│  `graph.snapshot.bin` (`RGCL` v2)                                            │
│    ├─► Call-only CSR (`EdgeType::Calls`)                                     │
│    ├─► Structural CSR (`Calls`, `Imports`, `Inherits`, `Implements`,         │
│    │     `References`, `Uses`)                                               │
│    └─► `NodeRole` table (`Production`, `Test`, `Generated`, `NonCallable`)   │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. PARALLEL CENTRALITY & CHURN PASSES (`crates/rgctl-analysis`)              │
│                                                                              │
│  • `FastPageRank`:                                                           │
│      - Pull-based CSR power iteration (`d = 0.85`, `PARALLEL_THRESHOLD=4096`)│
│      - `V <= 500,000`: 20 iterations, `tol = 1e-6`                           │
│      - `V >  500,000`:  8 iterations, `tol = 1e-4`                           │
│  • `Betweenness`:                                                            │
│      - `V <= 500` (`DEFAULT_EXACT_CENTRALITY_LIMIT`): exact Brandes O(V*E)   │
│      - `V >  500`: `SampledBetweenness` (`DEFAULT_SAMPLE_PIVOTS = 512`)      │
│  • `HyperBallHarmonic` (opt-in via `discover --with-harmonic`):              │
│      - `V <= 8,192` (`HYPERBALL_EXACT_THRESHOLD`): exact 64-bit word BitSet  │
│      - `V >  8,192`: parallel HyperLogLog (`p = 14 / 12 / 10`, 16/8 rounds)  │
│  • `GitChurn` (opt-in via `discover --with-git`):                            │
│      - Exponential recency decay + `CoChangesWith` edges + hotspot score     │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. COLUMNAR PERSISTENCE & CONSUMERS                                          │
│                                                                              │
│  • `.rgctl/analysis_results.bin` (`AnalysisResults::CentralityTable`)        │
│  • `.rgctl/analysis/git_churn.json` (`--with-git`)                           │
│  • CLI: `rgctl metrics --pagerank --betweenness --communities --churn`       │
│  • Dashboard: `function_metrics.json` (`FunctionsView.tsx`, `MigrationView`) │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Pull-Based CSR PageRank** | Page, Brin, Motwani & Winograd (1998); Gargiulo et al. (2015), *Componentwise PageRank* | [`FastPageRank::compute_flat`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality.rs#L170-L360) precomputes `inv_out_deg[u]` and pulls incoming neighbor contributions across contiguous CSR slices (`rev_offsets`, `rev_sources`), redistributing dangling-node mass uniformly each round and filtering hotspots by `NodeRole::Production` unless `--include-tests` is passed. |
| **Brandes Betweenness & Degree-Stratified Pivot Sampling** | Brandes (2001), *A Faster Algorithm for Betweenness Centrality*; Geisberger, Sanders & Schultes (2008), *Better Approximation of Betweenness Centrality* | [`CentralityCalculator`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality.rs#L520-L640) runs exact Brandes for $V \le 500$ (`DEFAULT_EXACT_CENTRALITY_LIMIT = 500`) and [`SampledBetweenness`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality_approx.rs#L65-L260) with $K = 512$ (`DEFAULT_SAMPLE_PIVOTS = 512`) degree-stratified pivots (50% highest total-degree hubs + 50% deterministic stride) scaled by $N/K$ for $V > 500$. |
| **HyperBall Harmonic Centrality** | Boldi & Vigna (2013), *In-Core Computation of Geometric Centralities with HyperBall: A Hundred Billion Nodes and Beyond* | [`HyperBallHarmonic`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality_approx.rs#L275-L520) evaluates $H(v) = \sum_{d=1}^{R} \frac{|B(v, d)| - |B(v, d-1)|}{d}$ using exact 64-bit word `BitSet` unions when $V \le 8,192$ (`HYPERBALL_EXACT_THRESHOLD`) and register-wise max-merge HyperLogLog counters ($p = 14$ for $V \le 8,192$, $p = 12$ for $V \le 100,000$, $p = 10$ for $V > 100,000$) over $R = 16$ rounds ($R = 8$ when $V > 500,000$). |
| **Temporal Change Coupling & Churn Hotspots** | Tornhill (2015), *Your Code as a Crime Scene*; Graves et al. (2000), *Predicting Fault Incidence Using Software Change History* | [`git_churn.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/git_churn.rs#L1-L220) streams `git log` over `--git-since` (default `90.days`), weights commit frequency by exponential recency decay $\times \log_2(1 + \text{cyclomatic\_complexity})$, and emits `EdgeType::CoChangesWith` edges between files modified together. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Verified Centrality Constants (`centrality.rs` & `centrality_approx.rs`)

| Constant | Value | Meaning |
| :--- | :--- | :--- |
| `PARALLEL_THRESHOLD` | `4_096` | Minimum graph size $V$ before Rayon parallel iterators activate (`par_iter`). |
| `LARGE_GRAPH_PAGERANK_NODE_LIMIT` | `500_000` | Threshold above which `FastPageRank` switches from `(max_iter = 20, tol = 1e-6)` to `(max_iter = 8, tol = 1e-4)`. |
| `DEFAULT_EXACT_CENTRALITY_LIMIT` | `500` | Maximum node count $V$ for exact $O(V \cdot E)$ Brandes betweenness before switching to `SampledBetweenness`. |
| `DEFAULT_SAMPLE_PIVOTS` ($K$) | `512` | Number of pivot sources sampled by `SampledBetweenness` when $V > 500$. |
| `HYPERBALL_EXACT_THRESHOLD` | `8_192` | Maximum node count $V$ for exact 64-bit word `BitSet` harmonic ball propagation (`hyperball_exact`). |
| `HYPERLOGLOG_PRECISION` ($p$) | `14` (`12` / `10` adaptive) | HyperLogLog register precision ($m = 2^p$ bytes per node) when $V > 8,192$. |
| `DEFAULT_HYPERBALL_ROUNDS` | `16` (`8` when $V > 500,000$) | Ball expansion rounds (`LARGE_GRAPH_HYPERBALL_ROUNDS = 8` above `LARGE_GRAPH_HYPERBALL_NODE_LIMIT = 500_000`). |

### 3.2 Columnar Persistence (`.rgctl/analysis_results.bin`)

Defined in [`crates/rgctl-analysis/src/results.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/results.rs):
- Stores flat `Vec<f64>` / `Vec<f32>` columns indexed by dense node index (`0..V`) inside `CentralityTable` (`pagerank`, `betweenness`, `harmonic`, `degree`) rather than `HashMap<Uuid, f64>`, eliminating per-node hash-table overhead on 1M+ node repositories.

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl metrics --help` and [`src/cli/metrics.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/metrics.rs):

```bash
# Discover with optional harmonic centrality and git churn / co-change edges
rgctl discover . --with-harmonic --with-git --git-since 90.days

# Query all core graph metrics (PageRank, betweenness, communities)
rgctl metrics -f json

# Query PageRank with custom iteration count or including test/generated symbols
rgctl metrics --pagerank --iterations 50 -f json
rgctl metrics --pagerank --include-tests -f json

# Query betweenness centrality or Fast-Leiden community modularity
rgctl metrics --betweenness -f json
rgctl metrics --communities -f json

# Query git churn hotspots and co-changing files (requires `discover --with-git`)
rgctl metrics --churn -f json
```

### 4.2 JSON Schema (`rgctl metrics --pagerank --betweenness -f json`)

```json
{
  "schema_version": "1.0",
  "pagerank": {
    "top_nodes": [
      {
        "node": "4f8a12c0-3b11-5e99-8100-aabbccddeeff",
        "name": "OrderRepository::save",
        "kind": "method",
        "qualified_name": "com.example.OrderRepository::save",
        "file_path": "src/OrderRepository.java",
        "pagerank": 0.014821
      }
    ]
  },
  "betweenness": {
    "top_nodes": [
      {
        "node": "9c2100a1-1122-5334-9900-112233445566",
        "name": "OrderService::processOrder",
        "kind": "method",
        "file_path": "src/OrderService.java",
        "betweenness": 142.5
      }
    ]
  }
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **PageRank & Exact Centrality** | [`crates/rgctl-analysis/src/centrality.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality.rs) | `FastPageRank`, `PageRankConfig`, `NodeRole`, `CentralityCalculator` |
| **Approximate Betweenness & Harmonic** | [`crates/rgctl-analysis/src/centrality_approx.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/centrality_approx.rs) | `SampledBetweenness`, `HyperBallHarmonic`, `ApproximateCentrality` |
| **Git Churn & Co-Change Analysis** | [`crates/rgctl-analysis/src/git_churn.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/git_churn.rs) | `GitChurnAnalyzer`, `FileChurnMetrics` |
| **Columnar Results Table** | [`crates/rgctl-analysis/src/results.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/results.rs) | `AnalysisResults`, `CentralityTable`, `CommunityTable` |
| **CLI Metrics Command** | [`src/cli/metrics.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/metrics.rs), [`src/cli/metrics_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/metrics_output.rs) | `MetricsArgs`, `run`, `collect_churn_hotspots` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Harmonic Centrality Is Opt-In (`--with-harmonic`):** Because `HyperBallHarmonic` allocates HLL register banks across $V$ vertices, it is off on default cold `discover` and enabled via `--with-harmonic` or `--full`.
2. **Large-Repo Dashboard Sparse Mode:** When $V \ge 50,000$, `function_metrics.json` writes `sparse_mode: "community_only"` so the dashboard loads metrics via WASM + package metagraph aggregates without writing multi-hundred-megabyte JSON arrays.

### Verification Commands

```bash
# Run centrality unit and audit integration tests
cargo test -p rgctl-analysis centrality
cargo test --test centrality_audit

# Run Criterion centrality microbenchmarks
cargo bench -p rgctl-analysis --bench centrality_benchmarks
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Active-Pivot Sampling on Heterogeneous Graphs ([#187](https://github.com/rgctl/rgctl/issues/187)):** `SampledBetweenness` currently draws $K = 512$ pivots from $0..V$ before filtering out 0-out-degree nodes (`File`, `Import`, `Field`, leaf functions); restricting the candidate pool to active $out\_deg > 0$ vertices is tracked in OpenSpec change `sampled-betweenness-active-pivots`.
2. **Git Churn Requires Local Git History:** `rgctl discover --with-git` requires a non-shallow `.git` commit history covering the `--git-since` window; shallow CI clones (`--depth 1`) yield single-commit churn scores.
