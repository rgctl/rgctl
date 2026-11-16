# Blast Radius & Reachability — Engineering Design

This document specifies the **Blast Radius, Kosaraju SCC Condensation, and Multi-Tier Reachability subsystem** in `rgctl` (`crates/rgctl-analysis/src/blast_radius_scc.rs`, `blast_engine_snapshot.rs`, `macro_call_index.rs`, `macro_call_lookup.rs`, and `src/cli/blast_radius.rs`). It details how `rgctl` computes upstream change impact across cyclic call graphs, supports both callable (`Function`, `Method`) and container (`Class`, `Interface`, `Struct`, `Enum`, `Module`) seeds, and serves sub-second queries via the 3-tier lookup architecture.

![Blast Radius tab — impact metrics and caller table (gbuilder)](../images/design/blast-radius/blast-radius-impact.png)

*Figure 1: Dashboard **Blast Radius** tab — depth slider, impact score cards, and transitive caller table for a selected symbol.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. DISCOVER-TIME SCC CONDENSATION & INDEXING (`rgctl-analysis`)              │
│                                                                              │
│  `graph.snapshot.bin` (`RGCL` v2 CSR)                                        │
│    ├─► Kosaraju Two-Pass SCC Condensation (`blast_radius_scc.rs`):           │
│    │     • Collapses mutually recursive `Calls` cycles into supernodes       │
│    │     • Builds topological Condensed DAG + 64-bit word reachability       │
│    ├─► Writes `.rgctl/blast_engine.snapshot.bin` (`BlastRadiusEngine` mmap)  │
│    └─► Writes `.rgctl/macro_call_index.db` / `.bin` (`MacroCallLookupDb`)    │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. QUERY-TIME 3-TIER RESOLUTION (`src/cli/blast_radius.rs`)                  │
│                                                                              │
│  Input: `rgctl blast-radius <SYMBOL> [--depth N] [--class C] [--file F]`     │
│    ├─► Container Seed Check (`Class`/`Interface`/`Struct`/`Enum`/`Module`):  │
│    │     • Collects member methods (`Contains` / `Defines` / FQN prefix)     │
│    │     • Aggregates external callers across all member methods             │
│    ├─► TIER 0 (`MacroCallLookupDb` SQLite / bincode fast path):              │
│    │     • O(1) indexed lookup when no policy or slice handoff is requested  │
│    ├─► TIER 1 (`blast_engine.snapshot.bin` + `SnapshotNodeStore` mmap):      │
│    │     • In-process SCC DAG reachability + `--depth N` BFS filter          │
│    └─► TIER 2 (Full Hydrate + Policy / PDG Slice Handoff):                   │
│          • Activated for `--policy-file` or `--with-slices`                  │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. OUTPUT & GOVERNANCE                                                       │
│                                                                              │
│  • JSON v2 envelope (`target`, `metrics`, `topology`, `gatekeeping`,         │
│    optional `boundary_hits` and `slice_handoffs`)                            │
│  • Dashboard `BlastView.tsx` (WASM interactive BFS + `blast_index.json`)     │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Kosaraju-Sharir Strongly Connected Components (SCC)** | Kosaraju (1978); Sharir (1981), *A Strong-Connectivity Algorithm and Its Applications in Data Flow Analysis* | [`BlastRadiusEngine`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/blast_radius_scc.rs#L1-L260) decomposes the call graph into Strongly Connected Components ($C_1, \dots, C_k$), collapsing mutual recursion into a cycle-free Condensed DAG where topological reachability is well-defined and terminating. |
| **Bitset-Accelerated Transitive Closure on Condensed DAGs** | Nuutila & Soisalon-Soininen (1994), *On Finding the Strongly Connected Components in a Directed Graph* | Bottom-up dynamic programming over the reverse topological order of the Condensed DAG merges 64-bit word bitsets (`reach[u] |= reach[v]`) to compute exact transitive upstream caller counts in $O(V \cdot E_{\text{dag}} / 64)$ operations. |
| **Log-Normalized Architectural Impact Scoring** | Software change impact & coupling metrics (Briand, Daly & Wüst, 1999) | `impact_score_from_counts(direct_callers, transitive_callers)` combines direct fan-in and transitive upstream reach into a bounded $[0, 100]$ score so leaf utilities and core architectural bottlenecks are immediately distinguishable. |
| **Container-Level Member Aggregation** | Object-oriented coupling (Chidamber & Kemerer CBO/RFC metrics, 1994) | When `rgctl blast-radius` targets a container type (`Class`, `Interface`, `Struct`, `Enum`, `Trait`, `Module`), [`resolve_type_container_blast`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/blast_radius.rs#L400-L560) unions the upstream caller cones of all member callables while excluding internal self-calls within the container. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 `.rgctl/` Blast Radius Artifacts

| Artifact Path | Purpose | Key Rust Types |
| :--- | :--- | :--- |
| `.rgctl/blast_engine.snapshot.bin` | Serialized SCC condensation map (`node_to_scc`), SCC member lists, condensed DAG adjacency, and precomputed per-node `BlastRadiusResult` scores, bound to parent `graph.snapshot.bin` `content_digest`. | `BlastRadiusEngine`, `BlastRadiusResult` ([`blast_engine_snapshot.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/blast_engine_snapshot.rs)) |
| `.rgctl/macro_call_index.db` / `.bin` | Tier-0 indexed lookup table mapping function names, FQNs, and UUIDs to `MacroIndexEntry` (`direct_caller_ids`, `impact_zone_ids`, `score`) for sub-10ms CLI queries. | `MacroCallLookupDb`, `MacroCallIndex`, `MacroIndexEntry` ([`macro_call_lookup.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/macro_call_lookup.rs)) |

### 3.2 Depth Filtering & Boundary Classification

- **Hop-Bounded Impact (`--depth N`):** [`filter_impact_by_caller_depth`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/graph_utils.rs#L1-L140) runs a reverse `Calls` BFS from the target symbol over `PetGraphView` up to `N` hops, intersecting the hop-bounded set with the engine's impact zone and recomputing `score = impact_score_from_counts(direct, filtered_impact)`.
- **Boundary Classification (`--classify-boundary` / `--boundary KIND`):** Labels upstream callers that cross external system boundaries (`REST_ENDPOINT`, `GRAPHQL`, `GRPC`, `CLI`, `MESSAGE_CONSUMER`, `SCHEDULED_JOB`).

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl blast-radius --help` and [`src/cli/mod.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/mod.rs#L655-L699):

```bash
# Full transitive blast radius for a function, method, or container class/struct
rgctl blast-radius OrderService::processOrder -f json
rgctl blast-radius ShoppingCartService -f json

# Limit upstream callers to N hops and disambiguate by class/file/line
rgctl blast-radius process --class OrderService --file src/OrderService.java --line 42 --depth 3 -f json

# Compact summary (score, distance buckets, top affected files) for large cones
rgctl blast-radius Logger::info --summary -f json

# Classify external entry-point boundaries reached in the upstream cone
rgctl blast-radius OrderService::save --classify-boundary --boundary REST_ENDPOINT -f json

# Enforce CI policy gatekeeping (exits 1 if policy is VIOLATED)
rgctl blast-radius OrderService::save --policy-file policy.json -f json

# Statement-level PDG slice handoff across caller call sites
rgctl blast-radius OrderService::save --with-slices -f json
```

### 4.2 JSON Schema (`rgctl blast-radius <SYMBOL> -f json`)

Implemented in [`src/cli/blast_radius_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/blast_radius_output.rs):

```json
{
  "schema_version": "2.0",
  "target": {
    "id": "2c8b91f0-7c12-5a01-9e11-112233445566",
    "name": "processOrder",
    "canonical_fqn": "com.example.OrderService::processOrder",
    "node_type": "Method",
    "file_path": "src/OrderService.java",
    "start_line": 42
  },
  "metrics": {
    "impact_score": 38.5,
    "direct_callers_count": 2,
    "impact_zone_count": 7,
    "caller_depth_limit": 3
  },
  "topology": {
    "direct_callers": [
      {
        "id": "8a112233-4455-5667-8899-aabbccddeeff",
        "name": "checkout",
        "file_path": "src/CheckoutController.java",
        "start_line": 29
      }
    ],
    "impact_zone": []
  },
  "gatekeeping": {
    "status": "SKIPPED",
    "violations": []
  }
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Kosaraju SCC & Reachability Engine** | [`crates/rgctl-analysis/src/blast_radius_scc.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/blast_radius_scc.rs) | `BlastRadiusEngine`, `BlastRadiusResult`, `impact_score_from_counts` |
| **Engine Snapshot Persistence** | [`crates/rgctl-analysis/src/blast_engine_snapshot.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/blast_engine_snapshot.rs) | `save_engine`, `try_load_engine` |
| **Tier-0 Macro Call Lookup Cache** | [`crates/rgctl-analysis/src/macro_call_lookup.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/macro_call_lookup.rs), [`macro_call_index.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/macro_call_index.rs) | `MacroCallLookupDb`, `MacroCallIndex`, `MacroIndexEntry` |
| **Blast-to-Slice PDG Handoff** | [`crates/rgctl-analysis/src/blast_slice_handoff.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/blast_slice_handoff.rs) | `resolve_handoff_seeds`, `trace_blast_to_slices_with_blast` |
| **CLI Orchestration & JSON Output** | [`src/cli/blast_radius.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/blast_radius.rs), [`src/cli/blast_radius_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/blast_radius_output.rs) | `BlastRadiusArgs`, `try_fast_cached_lookup`, `BlastRadiusResponse` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Tier-0 Fast Path First:** Default `rgctl blast-radius <symbol>` queries without `--policy-file` or `--with-slices` resolve directly through `MacroCallLookupDb` + `SnapshotNodeStore` mmap without hydrating a full `MemoryBackend`.
2. **Digest-Validated Cache Reuse:** Both `MacroCallLookupDb::is_valid_for_repo` and `try_load_engine` verify the parent `graph.snapshot.bin` `content_digest` so stale reachability cones are never served after `rgctl update` or `rgctl discover`.

### Verification Commands

```bash
# Run blast radius unit, perf, and CLI golden tests
cargo test -p rgctl-analysis blast_radius
cargo test --release --test blast_radius_perf
cargo test --test subprocess_golden_path

# Run Criterion blast-radius benchmarks
cargo bench -p rgctl-analysis --bench blast_radius_benchmarks
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Static Call Graph Scope:** Reachability follows static `Calls` edges (plus `Overrides` and optional `--scip` overlay edges). Dynamic reflection (`Method.invoke`) or runtime-configured message bus topics without static annotations are not connected.
2. **Retired Daemon Mode (`query.sock`):** All `blast-radius` queries run in-process against `.rgctl/` mmap artifacts; the legacy background Unix-socket daemon (`query.sock`) was retired in v0.4.7.
