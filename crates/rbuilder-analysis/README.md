# rbuilder-analysis

Graph analysis algorithms for [rBuilder](https://github.com/sshaaf/rBuilder): centrality, community detection, blast radius, CFG/PDG construction, slicing, and taint analysis.

See [docs/analysis-architecture.md](../../docs/analysis-architecture.md) for the three-tier graph model.

## Module index

| Module | Graph input | Algorithm | Complexity |
|--------|-------------|-----------|------------|
| `graph_utils` | Backend / snapshot | Topology projection | O(V+E) |
| `callgraph` | Backend | u32 adjacency build | O(V+E) |
| `centrality` | `PetGraphView` | PageRank, Brandes betweenness | O(k·E), O(V·E) |
| `centrality_approx` | `FlatGraphIndex` | Sampled Brandes, HyperBall | Approximate |
| `community` | `PetGraphView` | Label propagation + modularity | O(iters·E) |
| `blast_radius_scc` | Call DiGraph | Kosaraju + bitset reachability | O(1) query |
| `blast_radius` | `PetGraphView` | Reverse BFS | O(V+E) |
| `dependency` | `PetGraphView` | Kosaraju SCC, reverse BFS | O(V+E) |
| `complexity` | Backend properties | Aggregation | O(F) |
| `cfg_builder` | tree-sitter AST | CFG construction | O(stmts) |
| `dominance` | CFG | Cooper-Harvey-Kennedy idom | O(n²) worst |
| `dataflow` | CFG + PDG | Reaching definitions | O(n·d) |
| `pdg` | CFG | Data + control dependencies | O(n·d) |
| `slicing` | PDG | Backward BFS slice | O(V+E) |
| `taint` | PDG | Forward taint propagation | O(V+E) |
| `migration` | Community graph | Weighted topo sort | O(V+E) |
| `results` | — | Columnar metric storage | O(1) lookup |

## Running tests and benchmarks

```bash
cargo test -p rbuilder-analysis
cargo clippy -p rbuilder-analysis -- -D warnings
cargo bench --bench graph_benchmarks      # PetGraphView + blast radius
cargo bench --bench centrality_benchmarks   # PageRank, betweenness
cargo bench --bench community_benchmarks    # Label propagation
```

## Conventions

- Build `PetGraphView` once per analysis pass; pass references to analyzers.
- Use `TraversalConfig` (default depth 10) for bounded BFS traversals.
- Prefer `BlastRadiusEngine` over `BlastRadiusAnalyzer` for large graphs needing full transitive closure.
- No `unwrap()` in production paths; propagate errors with `rbuilder_error::Result`.
