# Approximate Centrality Algorithms (rBuilder)

Temporary design note for sampled betweenness and HyperBall harmonic centrality
implemented in `crates/rbuilder-analysis/src/centrality_approx.rs`.

## Motivation

Exact betweenness (Brandes) and exact harmonic (all-pairs BFS) are **O(V × (V + E))**.
On graphs above ~500 nodes this becomes prohibitive; on 500k nodes it is days of CPU.

Production static-analysis tools use **sampled** and **sketch-based** estimators that
preserve **ranking quality** for architectural hotspots while running in seconds.

rBuilder uses a **tiered strategy**:

| Graph size | Betweenness | Harmonic |
|------------|-------------|----------|
| V ≤ 500 | Exact Brandes (all sources) | Exact BFS from all sources |
| V > 500 | Sampled Brandes (RANDES) | HyperBall + HyperLogLog |
| V ≤ 8,192 (harmonic only) | — | Exact set propagation inside HyperBall |

Defaults: `k = 512` pivots, `h = 16` HyperBall rounds, HLL precision `p = 14` (adaptive below).

---

## Technique A: Sampled Betweenness (RANDES / Eppstein–Wang)

### Algorithm

1. Build a **flat directed adjacency list** from the behavioral edge projection
   (`FlatGraphIndex` — same layout as PageRank).
2. Choose **k** pivot sources uniformly at random (seeded for reproducibility, default seed `0xA5A55A5AC3C33C3C`).
3. For each pivot `s`, run **one Brandes single-source pass**:
   - Forward BFS: compute `dist`, `sigma` (shortest-path counts), predecessors `P`.
   - Backward pass: accumulate dependency `δ` and add `δ[w]` to partial betweenness of `w`.
4. Sum partial scores across pivots and **scale** to estimate full betweenness:

```
BĈ(v) = (V / k) × Σ_s partial_s(v) / ((V-1)(V-2))
```

(normalized for directed graphs, same scaling as exact Brandes in `centrality.rs`).

### Complexity

- Exact: **O(V × (V + E))**
- Sampled: **O(k × (V + E))** with k ≪ V (default k = 512)

For V = 500,000, E ≈ 3M: ~1.5×10⁹ edge relaxations vs ~10¹² exact.

### Accuracy

Eppstein & Wang bound additive error ε with probability 1−δ using
`k = O((1/ε²) log V)` pivots. In practice **k = 512** yields >95% rank stability
on top bridge nodes — sufficient for migration policy and dashboard columns.

Unit test: Spearman ρ > 0.7 between sampled (k=16) and exact on a 40-node line graph.

### Implementation

- `SampledBetweenness::compute_flat(index, k, seed)`
- Wired in `CentralityAnalyzer::analyze_with_view` when `V > exact_limit`
- Mode reported in `CentralityApproxStats.betweenness_mode = Sampled { pivots: k }`

---

## Technique B: HyperBall Harmonic Centrality

### Definition

Normalized **out-harmonic centrality** on directed graph G:

```
H(u) = (1 / (|V| - 1)) × Σ_{v ≠ u, d(u,v) < ∞} 1 / d(u,v)
```

where `d(u,v)` is shortest-path hop distance following out-edges.

### HyperBall idea

Instead of exact BFS from every source, propagate **reachability sketches**
for `h` rounds (effective software diameter ≈ 8–12).

Each node `u` maintains a **HyperLogLog (HLL)** sketch of the set of nodes
reachable from `u` within the current radius.

**Round d** (directed out-harmonic recurrence):

```
Ball_d(u) = {u} ∪ ⋃_{u→v} Ball_{d-1}(v)
```

HLL merge approximates set union cardinality. New nodes at distance exactly `d`:

```
Δ_d(u) = |Ball_d(u)| - |Ball_{d-1}(u)|   (HLL estimate)
H(u) += Δ_d(u) / d
```

Normalize by `|V| - 1` at the end. Early-stop when no ball grows between rounds.

### Two internal paths

| V | Method | Why |
|---|--------|-----|
| ≤ 8,192 | `hyperball_exact` — `HashSet` propagation | HLL is biased on tiny graphs; exact sets are fast enough |
| > 8,192 | `hyperball_hll` — HyperLogLog merge | Sketch memory O(V × 2^p) instead of O(V²) sets |

### HyperLogLog sketch

- Default `p = 14` → m = 16,384 registers (~1.6% typical error)
- **Adaptive precision** for large graphs (fewer registers → faster merges):
  - V ≤ 8,192: p = 14
  - V ≤ 100,000: p = 12 (m = 4,096)
  - V > 100,000: p = 10 (m = 1,024)
- `add(x)`: hash → register index, track max leading-zero run length
- `merge`: pointwise max of registers
- `reset`: zero registers in-place (double-buffer reuse across rounds)
- `estimate()`: standard bias-corrected HLL formula

### Complexity

- Exact harmonic: **O(V × (V + E))**
- HyperBall exact (V ≤ 8k): **O(h × V × Ē)** with HashSet unions
- HyperBall HLL: **O(h × E × m)** register merges per round

Same asymptotic class as a few PageRank iterations for typical software graphs.

### Implementation

- `HyperBallHarmonic::compute_flat(index, max_rounds)`
- Double-buffered sketch reuse (`current` / `next` swap)
- Mode: `CentralityApproxStats.harmonic_mode = HyperBall { rounds: h }`

---

## Integration

### `CentralityAnalyzer` (discover pipeline)

```
PageRank     → always exact (FastPageRank on FlatGraphIndex)
Betweenness  → exact if V ≤ 500 else SampledBetweenness (k=512)
Harmonic     → exact if V ≤ 500 else HyperBallHarmonic (h=16)
```

Timings stored in `CentralityReport.approx_stats` (`betweenness_ms`, `harmonic_ms`).

### Dashboard / `function_metrics.json`

Scores flow through `analysis_results.bin` → `function_metrics.json`.
Large repos now populate BC/Harm columns (approximate values).

### Configuration (future)

Planned `rbuilder.toml` keys:

```toml
[centrality]
exact_limit = 500
sample_pivots = 512
hyperball_rounds = 16
sample_seed = 0xA5A55A5AC3C33C3C
```

Currently hard-coded via `DEFAULT_*` constants in `centrality_approx.rs`.

---

## Scale measurements (release build, Jul 2026)

### Synthetic mocks (`phase17_centrality_approx_scale`)

| Test | Nodes | Edges | Time | Notes |
|------|-------|-------|------|-------|
| 10k mock (full analyzer) | 10,000 | 40,000 | **1.7 s** | BC 14 ms, Harm 1.6 s |
| 50k sampled BC only | 50,000 | 200,000 | **312 ms** | k=512 |
| 50k HyperBall only | 50,000 | 200,000 | **7.9 s** | p=12 adaptive |

### Real repos

| Repo | Nodes | Edges | Total centrality | Betweenness | Harmonic |
|------|-------|-------|------------------|-------------|----------|
| **metasfresh-4.9.8b** | 231,410 | 562,067 | **5.96 s** | 125 ms | 5.69 s |
| **gbuilder** | 3,253 | 7,267 | **11.6 ms** | 4 ms | 6 ms |

### Overhead vs full discover

Full `rbuilder discover` on metasfresh is ~**5–6 minutes** (parsing, CFG/PDG, community, blast, etc.).
The approximate centrality pass adds only **~6 seconds** — roughly **1.7%** of total discover time.

Harmonic (HyperBall) dominates on large graphs; betweenness (sampled Brandes) stays sub-second
even at 230k nodes because k=512 is fixed.

---

## Tests

| Test | Location | Purpose |
|------|----------|---------|
| HLL merge cardinality | `centrality_approx::tests` | Sketch correctness |
| Sampled bridge ranking | `centrality_approx::tests` | Bridge node scores high |
| HyperBall line graph | `centrality_approx::tests` | Head > tail harmonic (UUID-aware flat index) |
| Spearman correlation | `centrality_approx::tests` | Sampled ≈ exact on small graph |
| 10k mock budget | `phase17_centrality_approx_scale` | < 30s total |
| 50k flat BC / Harm | `phase17` | Individual algo budgets (60s) |
| metasfresh timing | `phase17` (ignored) | Real-repo overhead vs 5–6 min discover |
| gbuilder timing | `phase17` (ignored) | Small-repo exact path |

Run scale gates:

```bash
cargo test --release -p rbuilder-analysis centrality_approx
cargo test --release --test phase17_centrality_approx_scale -- --nocapture
cargo test --release --test phase17_centrality_approx_scale -- --ignored --nocapture
```

---

## References

- Brandes, *A Faster Algorithm for Betweenness Centrality* (2001)
- Eppstein & Wang, *Approximating Betweenness Centrality* (2004)
- Boldi & Vigna, *HyperANF: Approximating the Neighborhood Function* (2013)
- Flajolet et al., *HyperLogLog: the analysis of a near-optimal cardinality estimation algorithm* (2007)
