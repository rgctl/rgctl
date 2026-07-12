# Performance baselines

Wall-clock reference timings for golden repositories. Update after `discover . --all` or analysis changes.

Run validation:

```bash
./scripts/validate-golden-repos.sh
```

Soft regression gate: **+10%** vs last recorded value (`tests/discover_perf_baselines.rs`).

## Reference timings

| Repo | Nodes | Edges | `discover --all` | Centrality only | Notes |
|------|-------|-------|------------------|-----------------|-------|
| gbuilder | 2,208 | 5,505 | **5.5 s** | **6.3 ms** | `/Users/sshaaf/git/java/gbuilder`, `--languages java` |
| metasfresh | 229,866 | 561,246 | **531 s** (~8.9 min) | **5.9 s** | `example/metasfresh-4.9.8b` |

Measured **2026-07-10** at git **`ccd0c73`** (release build, local machine).

### Discover `--all` breakdown (metasfresh)

| Phase | Wall time |
|-------|-----------|
| Index + graph | ~16 s |
| CFG/PDG + taint + dashboard export | ~515 s |
| **Total** | **531 s** |

## Criterion micro-benches (release, sample-size 10)

| Bench | Result | Command |
|-------|--------|---------|
| PetGraphView 5k/20k | ~1.41 ms | `cargo bench --bench graph_benchmarks petgraph_view_build/5000` |
| PetGraphView 10k/50k | ~3.47 ms | `cargo bench --bench graph_benchmarks petgraph_view_build/10000` |
| PetGraphView 25k/100k | ~7.90 ms | `cargo bench --bench graph_benchmarks petgraph_view_build/25000` |
| Blast `deep_chain_1000` | ~sub-ms | `cargo bench --bench graph_benchmarks analyze_with_policy` |

## Record format (append after runs)

```
| date | git sha | repo | discover_all_s | centrality_s | notes |
```

<!-- Baseline runs appended below by validate-golden-repos.sh or manual tests -->

| 2026-07-10 | ccd0c73 | gbuilder | 5.5 | 0.006 | discover --all + centrality_approx_scale |
| 2026-07-10 | ccd0c73 | metasfresh | 531 | 5.9 | discover --all + centrality_approx_scale |
