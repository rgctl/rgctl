# Compressed Sparse Row (CSR) Topology & Zero-Copy Graph Architecture

This document specifies the **Compressed Sparse Row (CSR) storage, zero-copy mmap topology, and CSR-backed graph algorithm architecture** across `rgctl` (`crates/rgctl-graph`, `crates/rgctl-analysis`, and `crates/rgctl-wasm`). It details how directed and undirected code-graph topologies are laid out in `.rgctl/graph.snapshot.bin` (`COLUMNAR_SNAPSHOT_VERSION = 5`), borrowed directly from OS virtual memory (`memmap2::Mmap`), partitioned per vertex by `(edge_type, neighbor)` for `O(log deg(u))` typed sub-slice access, and consumed across CLI queries, macro analytics, and browser WASM views.

Related specifications:
- [**Graph Storage & Extraction Architecture**](../graph-storage-architecture.md) — full `.rgctl/` artifact suite, `NodeRow`, `NodeExtension`, `SegmentedSpill`, and `GraphCompactor`.
- [**Graph Analysis, CPG & Reachability Architecture**](../analysis-architecture.md) — 3-tier memory model, CPG archive, and macro analytics overview.
- [**Graph Centrality & Network Metrics**](graph-metrics-design.md) — `FastPageRank`, `SampledBetweenness`, `HyperBallHarmonic`, and `NodeRole` filtering.
- [**Blast Radius & Reachability**](blast-radius-design.md) — Kosaraju SCC condensation and 3-tier impact queries.

---

## 1. System Overview & End-to-End Data Flow

```text
+------------------------------------------------------------------------------+
| 1. BOUNDED STREAMING COMPILE (`rgctl-extraction` -> `rgctl-graph`)           |
|                                                                              |
|  `SegmentedSpill` / `write_columnar_from_*` / `GraphCompactor`               |
|    +-> Sort & dedup `Node` rows by 128-bit UUID -> dense `0..N` `u32` IDs    |
|    +-> Resolve `(from, to, EdgeType)` to `(src: u32, dst: u32, code: u8)`    |
|    +-> `CodeGraphCsr::from_indexed_edges_with_pads`:                         |
|    |     * Two-pass counting sort into forward & reverse CSR buffers         |
|    |     * Sort each vertex slice by `(edge_type_code, neighbor_u32)`        |
|    |     * Extract non-zero 7B edge metadata into `SparseEdgePad` table      |
|    +-> Write `.rgctl/graph.snapshot.bin` (`RBGR` v5, 8-byte aligned CSR)     |
+--------------------------------------+---------------------------------------+
                                       | `memmap2::Mmap` (`O(1)` open)
                                       v
+------------------------------------------------------------------------------+
| 2. ZERO-COPY MMAP CSR (`ColumnarGraphMmap` & `CodeGraphCsr`)                 |
|                                                                              |
|  `CsrStorage::Mmap` borrows `&[u32]` and `&[u8]` slices directly from mmap:  |
|    * Forward: `row_ptr[N+1]`, `targets[E]`, `edge_types[E]`                  |
|    * Reverse: `in_row_ptr[N+1]`, `in_targets[E]`, `in_edge_types[E]`         |
|    * Contiguous typed slice lookup (`out_neighbors_of_type` /                |
|      `in_neighbors_of_type`) via binary search in `O(log deg(u))`            |
+------------------+-------------------+-------------------+-------------------+
                   |                   |                   |
                   v                   v                   v
+------------------------+ +-----------------------+ +-------------------------+
| 3A. CLI & WASM QUERIES | | 3B. SHARED TOPOLOGY   | | 3C. SPECIALIZED CSRs    |
|                        | |                       | |                         |
| `StructuredQuery`:     | | `StructuralTopology`  | | * `CsrOutAdj`:          |
| * `callers` / `callees`| | (`PetGraphView`):     | |   Brandes, RANDES,      |
| * `relations <seed>`   | | * `Arc::clone` CSR    | |   HyperBall, Beamer BFS |
| * `find --annotation`  | | * Binary search over  | | * `undirected_csr` ->   |
| * `inventory --by edge`| |   sorted `Vec<Uuid>`  | |   `WeightedCsr`:        |
|                        | |   (zero `HashMap`s)   | |   multi-level           |
| `rgctl-wasm`:          | | * `kosaraju_scc`      | |   Fast-Leiden           |
| * `v5_out_calls` /     | | * `FastPageRank` pull | | * `scoped_policy`:      |
|   `v5_in_calls`        | |   on `Calls` slices   | |   PR upstream closure   |
+------------------------+ +-----------------------+ +-------------------------+
```

### Why CSR Over Other Graph Representations

At Linux-kernel scale (~1.81M nodes, ~8M–10M edges), code knowledge graphs are **immutable after ingest**, **sparse** (mean degree ~4–5 with power-law hubs), and **queried repeatedly** across forward and reverse edge subsets:

1. **vs. Intrusive Linked Arena (`petgraph::DiGraph`):** Linked edge lists store ~20–24 B/edge and chase `next_edge` indices scattered across the edge arena (incurring L2/L3 cache misses on every hop). Bidirectional CSR packs all neighbors of vertex `u` contiguously in `10 B/edge + 8 B/node` total across both directions, enabling hardware stream prefetching.
2. **vs. Nested Heap Vectors (`Vec<Vec<u32>>` / `HashMap<Uuid, Vec<Uuid>>`):** `Vec<Vec<u32>>` requires `N` separate heap allocations (`~43 MB` of `Vec` headers alone at `N = 1.8M`, plus capacity slack, allocator fragmentation, and pointer indirection). `HashMap<Uuid, _>` adds 16-byte keys, hash probing on every hop, and multi-hundred-millisecond `Drop` teardown. CSR uses 6 flat arrays total (0 heap allocations when mmap-backed).
3. **vs. Coordinate / Edge List (`Vec<(u32, u32)>` / COO):** COO requires `O(E)` full-edge scans or `O(log E)` binary searches per vertex visit, making single-seed CLI queries (`callers`, `callees`, `relations`) and frontier algorithms (Brandes, Kosaraju, Fast-Leiden) orders of magnitude slower than `O(1)` CSR row-pointer indexing.
4. **vs. Dense Bit Matrices:** `O(V^2)` bits at `V = 1.8M` requires ~405 GB. `rgctl` therefore runs Kosaraju SCC condensation on CSR first and restricts bitset propagation to the condensed `Calls` DAG (`BlastRadiusEngine`).

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Dual Forward/Transpose CSR (CSB / Bi-CSR)** | Buluc et al. (2009), *Parallel Sparse Matrix-Vector and Matrix-Transpose-Vector Multiplication Using Compressed Sparse Blocks* | `CodeGraphCsr` (`crates/rgctl-graph/src/csr.rs`) materializes both outgoing (`row_ptr`, `targets`, `edge_types`) and incoming (`in_row_ptr`, `in_targets`, `in_edge_types`) arrays at write time so forward (`callees`, Brandes) and reverse (`callers`, PageRank pull, blast-radius) traversals both run in `O(deg(u))`. |
| **Type-Clustered Neighborhoods** | Sorted-key CSR / dictionary-coded adjacency | Each vertex's neighbor slice is sorted by `(edge_type_code: u8, neighbor: u32)`. `CodeGraphCsr::out_neighbors_of_type` and `in_neighbors_of_type` locate the contiguous `&[u32]` sub-slice for a single `EdgeType` (such as `Calls`) via two `partition_point` binary searches (`O(log deg(u))`) without scanning `Contains` or `References` edges. |
| **Pull-Based Parallel CSR PageRank** | Page, Brin, Motwani & Winograd (1998); Gargiulo et al. (2015), *Componentwise PageRank*; Whang et al. (2015) | `FastPageRank::compute_pull_impl` (`crates/rgctl-analysis/src/centrality.rs`) precomputes `node_contrib[src] = d * rank[src] / out_deg[src]` and updates `next_ranks` via `par_iter_mut` over destination vertices, pulling `in_neighbors(dst)` with zero atomic contention, zero per-worker `O(N)` scratch buffers, and fused `max_delta` reduction. |
| **Direction-Optimizing BFS (Top-Down / Bottom-Up)** | Beamer, Asanovic & Patterson (SC 2012), *Direction-Optimizing Breadth-First Search* | `HarmonicCentrality::compute_with_bi_adj` (`crates/rgctl-analysis/src/centrality.rs`) switches a BFS level from top-down (`out_adj`) to bottom-up (`in_adj` scan of unvisited vertices against `in_frontier`) whenever active frontier outgoing edges exceed `max(total_edges / 20, 64)`, returning to top-down as the frontier contracts. |
| **Iterative Kosaraju-Sharir SCC over CSR** | Kosaraju (1978); Sharir (1981) | `StructuralTopology::kosaraju_scc_filtered` (`crates/rgctl-analysis/src/structural_topology.rs`) runs non-recursive finishing-time DFS on `in_neighbors_of_type` followed by component DFS on `out_neighbors_of_type`, avoiding `petgraph::DiGraph` materialization for the full graph. |
| **Multi-Level Fast-Leiden on Weighted CSR** | Traag, Waltman & van Eck (2019), *From Louvain to Leiden: Guaranteeing Well-Connected Communities* | `StructuralTopology::undirected_csr_filtered` builds a flat deduplicated undirected CSR in two counting-sort passes, feeding directly into `WeightedCsr` (`crates/rgctl-analysis/src/community.rs`) across local-moving, BFS refinement, and coarsening levels with zero `Vec<Vec<usize>>` allocations. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Columnar Snapshot v5 CSR Wire Format (`.rgctl/graph.snapshot.bin`)

Implemented in `crates/rgctl-graph/src/columnar_snapshot.rs` (`SNAPSHOT_MAGIC = *b"RBGR"`, `COLUMNAR_SNAPSHOT_VERSION = 5`, `HEADER_SIZE = 136`). Readers remain backward-compatible with v2, v3, and v4 files (`2..=COLUMNAR_SNAPSHOT_VERSION`).

#### Snapshot File Layout (v5)

```text
Offset 0                  136               offset_nodes           offset_edges
+--------------------------+-----------------+----------------------+--------------------+
| Header (136 B, `RBGR` v5)| Name & Type     | NodeRow Table        | Bidirectional CSR  |
| - node_count (u64)       | Indexes         | (N * 64 B, sorted    | + SparseEdgePads   |
| - edge_count (u64)       | + 0..7 B pad    |  strictly by UUID)   | (8-byte aligned)   |
| - digest_hex (64 B)      | to 8B boundary  |                      |                    |
| - section offsets        |                 |                      |                    |
+--------------------------+-----------------+----------------------+--------------------+
                                                                    |
       +------------------------------------------------------------+
       v
offset_edges (8-byte aligned; OS `Mmap` base is page-aligned -> all `u32` arrays 4B-aligned)
+------------------------+------------------------+--------------------+--------------------+
| row_ptr                | in_row_ptr             | targets            | in_targets         |
| [u32; N + 1] (LE)      | [u32; N + 1] (LE)      | [u32; E] (LE)      | [u32; E] (LE)      |
+------------------------+------------------------+--------------------+--------------------+
| edge_types             | in_edge_types          | 0..3 B align4 pad  | pad_count: u32 (LE)|
| [u8; E]                | [u8; E]                |                    | SparseEdgePad[K]   |
+------------------------+------------------------+--------------------+--------------------+
```

#### CSR Section Byte Accounting

| Sub-section | Type | Byte Length | Alignment | Notes |
| :--- | :--- | :--- | :--- | :--- |
| `row_ptr` | `[u32; N + 1]` | `4 * (N + 1)` | 8 B | Outgoing slice bounds; `row_ptr[0] == 0`, `row_ptr[N] == E`. |
| `in_row_ptr` | `[u32; N + 1]` | `4 * (N + 1)` | 4 B | Incoming slice bounds; `in_row_ptr[0] == 0`, `in_row_ptr[N] == E`. |
| `targets` | `[u32; E]` | `4 * E` | 4 B | Dense `NodeRow` indices (`0..N`) for outgoing edges. |
| `in_targets` | `[u32; E]` | `4 * E` | 4 B | Dense `NodeRow` indices (`0..N`) for incoming edges. |
| `edge_types` | `[u8; E]` | `E` | 1 B | Stable 1-byte `edge_type_to_u8` codes parallel to `targets`. |
| `in_edge_types` | `[u8; E]` | `E` | 1 B | Stable 1-byte `edge_type_to_u8` codes parallel to `in_targets`. |
| `csr_pad` | `[u8; 0..3]` | `(4 - (2*E % 4)) % 4` | 1 B | Zero padding to restore 4-byte alignment. |
| `pad_count` | `u32` | `4` | 4 B | Count `K` of edges carrying non-zero compact metadata (`0` on default `discover`). |
| `sparse_pads` | `[SparseEdgePad; K]` | `12 * K` | 4 B | `(out_edge_idx: u32, pad: [u8; 7], _reserved: u8)` sorted by `out_edge_idx`. |

Compared to the legacy v2–v4 `EdgeRow` table (`40 B * E` storing two 16-byte UUIDs per edge), v5 CSR requires `10 B * E + 8 B * N + 4 B` (~**3.45x smaller** edge storage on disk — e.g., `~93 MB` vs `320 MB` at `N = 1.6M, E = 8M`) while providing **both** forward and reverse adjacency pre-built on disk.

#### Sparse Edge-Pad Table (`SparseEdgePad`, 12 bytes)

Most edges carry no per-edge properties (`pad == [0u8; 7]`). When optional overlays (`--with-git` `CoChangesWith` `cochange_count`/`jaccard`, `rgctl merge` `cross_repo`, or `call_site_line`) attach compact metadata, `CodeGraphCsr::from_indexed_edges_with_pads` tracks each edge's final sorted position `out_edge_idx` in `targets` and emits a 12-byte entry only for non-zero pads. During `for_each_edge_with_pad` / `to_prepared`, a single forward cursor merges sparse pads in `O(E)` with zero overhead when `pad_count == 0`.

---

### 3.2 In-Memory CSR Representations Across Crates

`rgctl` uses three CSR types, each specialized to its workload:

| Type | Crate & Module | Backing & Direction | Edge Payload | Primary Consumers |
| :--- | :--- | :--- | :--- | :--- |
| **`CodeGraphCsr`** | `rgctl-graph::csr` | `CsrStorage::Mmap` (zero-copy `Arc<Mmap>`) or `CsrStorage::Owned`; **bidirectional** | `u8` `EdgeType` code (slices sorted by `(code, neighbor)`) | `ColumnarGraphMmap`, `StructuredQuery`, `StructuralTopology` / `PetGraphView`, `BlastRadiusEngine` (Kosaraju SCC), `FastPageRank::compute_view_single_type`, `scoped_policy`, `invariants`, `snapshot_diff` |
| **`CsrOutAdj`** | `rgctl-analysis::centrality` | Owned `offsets: Vec<u32>`, `targets: Vec<u32>`; **unidirectional** (forward via `build_out_csr`, reverse via `build_in_csr`) | Unweighted, pre-filtered to `allowed_types` | `BetweennessCentrality` (exact Brandes), `SampledBetweenness` (RANDES), `HyperBallHarmonic` (bitset & HLL), `HarmonicCentrality` (Beamer BFS), standalone `FastPageRank::compute_flat` |
| **`WeightedCsr`** | `rgctl-analysis::community` | Owned `offsets: Vec<u32>`, `targets: Vec<u32>`, `weights: Vec<f64>`, `node_weight: Vec<f64>`; **undirected** | `f64` modularity edge weight (`1.0` at level 0, aggregated at coarsened levels) | Multi-level `Fast-Leiden` (`leiden_local_moving`, `leiden_refine`, `aggregate_communities_weighted`) |

In addition, `rgctl-wasm::columnar::ColumnarView` implements a standalone dependency-free v5 CSR reader (`v5_out_calls`, `v5_in_calls`) over `&[u8]` for browser WASM bundles.

---

### 3.3 `StructuralTopology` & `PetGraphView` Memory Discipline

`StructuralTopology` (`crates/rgctl-analysis/src/structural_topology.rs`) and its compatibility facade `PetGraphView` (`crates/rgctl-analysis/src/graph_utils.rs`) enforce three zero-duplication rules:

1. **`O(1)` CSR Share from Mmap:** `StructuralTopology::from_snapshot_store` clones `CodeGraphCsr` via `Arc::clone` (`0` heap bytes for CSR arrays, `0` edge scans).
2. **Zero `HashMap<Uuid, u32>` on Sorted Snapshots:** Because `INV-1` guarantees `NodeRow` is strictly sorted by `Uuid`, `StructuralTopology` sets `ids_sorted = true`, leaves `uuid_to_index` empty, and resolves `get_index(uuid)` via `index_to_uuid.binary_search(&uuid)` in `O(log N)` (~20 ns, saving ~80–100 MB of hash-table heap at 1.8M nodes).
3. **Zero Wrapper Duplication in `PetGraphView`:** `PetGraphView` wraps `StructuralTopology` as a single field (`size_of::<PetGraphView>() == size_of::<StructuralTopology>()`), converting `u32 <-> petgraph::graph::NodeIndex` inline without cloning `index_to_uuid` or building a second `HashMap`.

---

### 3.4 Structural & Digest Invariants (`crates/rgctl-graph/src/invariants.rs`)

- **INV-1 (Sorted Unique Nodes):** `NodeRow` IDs are strictly ascending (`row[i].id < row[i + 1].id`), establishing a canonical bijection between UUID order and dense `0..N` `u32` CSR indices.
- **INV-2 (Sorted Unique Bidirectional CSR):**
  - `row_ptr` and `in_row_ptr` have length `N + 1`, start at `0`, end at `E`, and are monotonically non-decreasing.
  - Every target/source index satisfies `< N` (no dangling endpoints).
  - Within every vertex `u`, outgoing `(edge_types[i], targets[i])` and incoming `(in_edge_types[i], in_targets[i])` pairs are strictly ascending (guaranteeing uniqueness and valid `partition_point` sub-slices).
  - Forward and reverse CSR tables are exact transposes: `(u, v, ty)` is in `out_neighbors(u)` iff `(v, u, ty)` is in `in_neighbors(v)`.
- **INV-6 (Format-Independent `content_digest`):** Snapshot writers hash sorted `Node`s followed by canonical `Edge`s sorted by `(from_uuid, to_uuid, edge_type_u8)`. The 64-hex-char BLAKE3 `content_digest` is therefore invariant to CSR internal grouping and matches `MemoryBackend` / `SegmentedSpill` digests bit-for-bit.

---

## 4. CLI Contract, Query Complexity & Implementation Map

### 4.1 Query Complexity Before vs. After Zero-Copy v5 CSR

| Operation / CLI Command | Pre-v5 (`EdgeRow` Scan) | v5 Zero-Copy CSR | Cold `NodeExtension` Decodes |
| :--- | :--- | :--- | :--- |
| `ColumnarGraphMmap::open` | `O(1)` | `O(1)` (header + 4 endpoint `u32` checks) | `0` |
| `discover` `topology` stage | `O(N + E)` (`HashMap` + `2 * E` scan) | `O(N)` `NodeRow.id` copy + `O(1)` `Arc::clone` (`10.7 ms` on Linux) | `0` |
| `rgctl callers <sym> [--depth D]` | `O(E)` (`build_typed_adjacency`) | `O(log N + \|visited_calls\|)` via `in_neighbors_of_type` | `0` (unless `--class` filter) |
| `rgctl callees <sym> [--depth D]` | `O(E)` (`build_typed_adjacency`) | `O(log N + \|visited_calls\|)` via `out_neighbors_of_type` | `0` (unless `--class` filter) |
| `rgctl relations <sym> --edge <T>` | `O(E)` (`build_typed_adjacency`) | `O(log N + \|visited_T\|)` | `0` (unless `--class` filter) |
| `rgctl find --annotation @A` | `O(E)` edge scan + per-edge `get_node` | `O(N + \|annotated\|)` via `in_neighbors_of_type(AnnotatedWith)` | `0` (unless `--show-attributes`) |
| `rgctl inventory --by edge` | `O(E)` UUID `HashSet` inserts | `O(E)` byte-slice histogram over `csr.edge_types()` (`49 ms` on Linux) | `0` |
| `BlastRadiusEngine::build_scoped` | `O(diam * E)` repeated scans | `O(\|upstream_closure\|)` via `in_neighbors_of_type(Calls)` | `0` |
| `diff_snapshots` (`rgctl diff`) | `O(E)` `HashMap<Uuid, StableNodeKey>` | `O(E)` indexed CSR walk over `Vec<Option<StableNodeKey>>` | `0` |

### 4.2 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Core Bidirectional CSR** | [`crates/rgctl-graph/src/csr.rs`](../../crates/rgctl-graph/src/csr.rs) | `CodeGraphCsr`, `CsrStorage` (`Owned` / `Mmap`), `out_neighbors_of_type`, `in_neighbors_of_type`, `from_indexed_edges_with_pads` |
| **Columnar v5 Snapshot Mmap** | [`crates/rgctl-graph/src/columnar_snapshot.rs`](../../crates/rgctl-graph/src/columnar_snapshot.rs) | `COLUMNAR_SNAPSHOT_VERSION` (`5`), `ColumnarGraphMmap::open`, `csr`, `for_each_edge_indexed`, `write_columnar_assembled_csr` |
| **Streaming Spill & Compaction** | [`crates/rgctl-graph/src/segmented_spill.rs`](../../crates/rgctl-graph/src/segmented_spill.rs), [`graph_compactor.rs`](../../crates/rgctl-graph/src/graph_compactor.rs) | `compile_spill_to_columnar`, `GraphCompactor::compact_to_path` |
| **Snapshot Invariants & Diff** | [`crates/rgctl-graph/src/invariants.rs`](../../crates/rgctl-graph/src/invariants.rs), [`snapshot_diff.rs`](../../crates/rgctl-graph/src/snapshot_diff.rs) | `check_inv2_sorted_unique_edges`, `check_inv3_endpoint_legality`, `diff_edges` |
| **Zero-Copy Structured Queries** | [`crates/rgctl-graph/src/structured_query.rs`](../../crates/rgctl-graph/src/structured_query.rs) | `StructuredQuery::call_neighbors`, `relations_seeded`, `find_by_annotation`, `inventory` (`InventoryBy::Edge`) |
| **Shared Analysis Topology** | [`crates/rgctl-analysis/src/structural_topology.rs`](../../crates/rgctl-analysis/src/structural_topology.rs), [`graph_utils.rs`](../../crates/rgctl-analysis/src/graph_utils.rs) | `StructuralTopology`, `PetGraphView`, `undirected_csr_filtered`, `kosaraju_scc_filtered` |
| **Pull-Based PageRank & Beamer BFS** | [`crates/rgctl-analysis/src/centrality.rs`](../../crates/rgctl-analysis/src/centrality.rs), [`centrality_approx.rs`](../../crates/rgctl-analysis/src/centrality_approx.rs) | `FastPageRank::compute_view_single_type`, `compute_with_in_csr`, `HarmonicCentrality::compute_with_bi_adj`, `CsrOutAdj` |
| **Zero-`Vec<Vec>` Fast-Leiden** | [`crates/rgctl-analysis/src/community.rs`](../../crates/rgctl-analysis/src/community.rs) | `build_community_undirected_csr`, `fast_leiden_csr`, `WeightedCsr` |
| **PR Scoped Closure** | [`crates/rgctl-analysis/src/scoped_policy.rs`](../../crates/rgctl-analysis/src/scoped_policy.rs) | `collect_upstream_call_closure`, `hydrate_subset_hot` |
| **Browser WASM v5 Reader** | [`crates/rgctl-wasm/src/columnar.rs`](../../crates/rgctl-wasm/src/columnar.rs) | `ColumnarView::v5_out_calls`, `v5_in_calls` |

---

## 5. Performance Guardrails & Verification Checklist

### Coding Guardrails for CSR & Topology Paths

1. **Never Scan All Edges for Seeded Neighborhoods:** Any query or analysis seeded on `k` symbols must traverse `csr.out_neighbors_of_type` / `csr.in_neighbors_of_type` (or `out_neighbors` / `in_neighbors`), never `store.for_each_edge` in a BFS loop.
2. **Preserve `(edge_type, neighbor)` Slice Ordering:** Any constructor producing `CodeGraphCsr` must sort per-vertex slices by `(edge_type_code, neighbor_u32)` so binary-searched `out_neighbors_of_type` / `in_neighbors_of_type` sub-slices remain sound.
3. **8-Byte Alignment Before CSR Write:** Always pad `offset_nodes` to an 8-byte boundary (`align8_u64`) before writing `NodeRow` (`64 B`), guaranteeing `offset_edges` is 8-byte aligned for zero-copy `&[u32]` slice casts on little-endian `Mmap`.
4. **Zero Cold Decodes on Topology Paths:** Keep `cold_ext_deserialization_count() == 0` across `StructuralTopology::from_snapshot_store`, `FastPageRank`, `CommunityDetector`, `DeadCodeAnalyzer`, `diff_snapshots`, and unattributed `StructuredQuery` neighborhood walks (`tests/columnar_zero_cold_touch_gate.rs`).
5. **No Duplicate Index Maps:** Do not reintroduce secondary `HashMap<Uuid, _>` caches onto `PetGraphView` or sorted `StructuralTopology` instances.

### Reference Measurements (Release Binary, Apple M3 Pro)

- **Apache Kafka (`example/kafka`, cold `discover`):** `topology = 1.2 ms`, `centrality = 75.8 ms`, `community = 91.7 ms`, `graph.snapshot.bin = 123 MB`.
- **Linux Kernel (`example/linux`, `1,807,217` nodes, `948,922` functions, cold `discover`):**
  - `wall_secs = 45.68 s` (Gate A budget `48 s`), peak OS memory footprint `6.45 GiB`.
  - `topology = 10.7 ms`, `centrality = 217.8 ms` (`pagerank = 37 ms`, `flat_index = 15 ms`, `betweenness = 154 ms`), `community = 1.14 s`, `post_index_secs = 5.78 s`.
  - Cold CLI subprocess queries: `rgctl -f json inventory --by edge` in `49.3 ms`; `rgctl -f json callees schedule --file kernel/sched/core.c` in `270.1 ms`; `rgctl -f json callers schedule --file kernel/sched/core.c` in `342.7 ms`.

### Verification Commands

```bash
# CSR, columnar v4/v5 round-trip, invariants, diff, and structured query tests
cargo test -p rgctl-graph -p rgctl-analysis -p rgctl-wasm

# Zero cold NodeExtension touch gate (topology, PageRank, Leiden, diff, structured queries)
cargo test --test columnar_zero_cold_touch_gate

# End-to-end equivalence, determinism, and centrality audit suites
cargo test --test e2e_workflow_equivalence --test graph_determinism_qe --test centrality_audit

# Pre-flight lint & typecheck gate
./scripts/preflight-pr.sh --quick
```

---

## 6. Known Limitations & By-Design Boundaries

1. **4 Billion Node / Edge Ceiling (`u32` Indexing):** CSR offsets (`row_ptr`, `in_row_ptr`) and vertex IDs (`targets`, `in_targets`) use `u32`, capping a single snapshot at $2^{32} - 1 \approx 4.29 \times 10^9$ nodes and directed edges (roughly three orders of magnitude above the Linux kernel).
2. **Seedless Typed-Edge Scans (`rgctl relations --edge <T>` without Symbol):** Because CSR partitions edges by `(vertex, edge_type)` rather than storing a global per-`EdgeType` inverted edge list, seedless `relations` walks `0..N` calling `out_neighbors_of_type(u, T)` (`O(N + |E_T|)`). Unscoped `inventory --by edge` avoids even `O(N)` by histogramming `csr.edge_types()` directly.
3. **Immutable CSR Backing:** `CodeGraphCsr` is immutable once written. Incremental edits (`rgctl update` / `rgctl watch`) stream kept base CSR edges and delta edges through `GraphCompactor` into an atomically renamed v5 snapshot rather than mutating CSR slices in place.
4. **Multiplicity Collapsing (`INV-2`):** Columnar snapshots store a simple typed directed multigraph (at most one `(from, to, EdgeType)` edge per triple). Multiple call sites from `f` to `g` collapse to one `Calls` CSR entry; intra-procedural call-site multiplicity is preserved in per-function CFG/PDG (`cfg_pdg.archive.bin`) and external call sites in `parked_relations.bin`.
