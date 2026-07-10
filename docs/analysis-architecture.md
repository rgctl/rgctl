# rbuilder-analysis Architecture

The `rbuilder-analysis` crate bridges the code knowledge graph (`rbuilder-graph`) with static-analysis algorithms. It organizes graph usage into three tiers.

## Tier 1: Repository graph (source of truth)

- **`MemoryBackend`** — in-memory node/edge store with typed topology
- **Mmap snapshots** — `PreparedGraphSnapshot`, `MmappedGraphSnapshot` for zero-copy reads

Nodes represent symbols (functions, types, modules). Edges carry `EdgeType` semantics (`Calls`, `Uses`, `Contains`, `References`, etc.).

## Tier 2: Projected views (repository-level analysis)

| Type | Module | Purpose |
|------|--------|---------|
| `PetGraphView` | `graph_utils` | `petgraph` DiGraph + UnGraph with UUID bi-maps; edge weights are `EdgeType` |
| `CallGraph` | `callgraph` | u32-indexed call-only adjacency for fast traversal |
| `FlatGraphIndex` | `centrality` | Contiguous `usize` edge list for numeric algorithms |

**Convention:** Build `PetGraphView` once per analysis pass and pass `&PetGraphView` to centrality, community, dependency, and blast-radius analyzers.

### Repository-level analyses

| Analysis | Primary graph | Algorithm |
|----------|---------------|-----------|
| Centrality | `PetGraphView` → `FlatGraphIndex` | PageRank, Brandes betweenness, HyperBall harmonic |
| Community | `PetGraphView` (undirected filter) | Label propagation + modularity |
| Blast radius (engine) | Call-only DiGraph | Kosaraju SCC → bitset reachability |
| Blast radius (analyzer) | `PetGraphView` Calls filter | Reverse BFS |
| Dependencies | `PetGraphView` directed | Kosaraju SCC, reverse BFS impact |
| Complexity | Backend node properties | Aggregation |
| Migration | Community graph | Weighted topological sort |

## Tier 3: Intra-procedural graphs

| Type | Module | Purpose |
|------|--------|---------|
| `ControlFlowGraph` | `cfg`, `cfg_builder` | Per-function CFG via tree-sitter |
| `ProgramDependenceGraph` | `pdg` | Data + control dependencies |
| `DominatorTree` | `dominance` | Immediate dominators and frontiers |

### Pipeline

```
tree-sitter AST → CFG → DominatorTree + ReachingDefs → PDG → Slicing / Taint
```

`InterproceduralCFG` stitches per-function CFGs with `CallGraph` for cross-function slicing.

## Traversal depth

Graph BFS traversals (blast radius analyzer, dependency impact) share `TraversalConfig` with default depth **10** (`DEFAULT_TRAVERSAL_DEPTH` in `graph_utils`). Use `TraversalConfig::unlimited()` for full transitive closure; prefer `BlastRadiusEngine` for large graphs.

## Caching and persistence

- **`FlowCache`** / **`CfgPdgArchive`** — per-function CFG/PDG cache
- **`BlastEngineSnapshot`** — persisted SCC reachability bitsets
- **`AnalysisResults`** — columnar metrics decoupled from graph topology

## Further reading

- Crate README: `crates/rbuilder-analysis/README.md`
- OpenSpec design: `openspec/changes/review-rbuilder-analysis/design.md`
- Benchmarks: `cargo bench --bench graph_benchmarks` (workspace root)
