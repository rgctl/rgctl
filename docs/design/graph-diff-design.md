# Graph Snapshot Diff — Engineering Design

This document specifies the **Columnar Graph Snapshot Diff and Temporal PR Delta subsystem** in `rgctl` (`crates/rgctl-graph/src/snapshot_diff.rs`, `stable_key.rs`, `crates/rgctl-incremental/`, and `src/cli/diff.rs`). It covers cross-snapshot `StableNodeKey` identity, span-excluding structural extension digests (`extension_digest_excluding_spans`), parallel 16-shard node indexing, sorted CSR edge merge-joins, reverse-call cascade invalidation, and the Linux kernel cold diff gate (`linux_cold_diff_within_baseline`).

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. PARALLEL SNAPSHOT OPEN & DIGEST SHORT-CIRCUIT (`SnapshotPair::open`)      │
│                                                                              │
│  Base Snapshot (`graph.snapshot.bin`)  Head Snapshot (`graph.snapshot.bin`)  │
│    └──────────────┬───────────────────────────────────┬──────────────────────┘
│                   ▼                                   ▼                      │
│          `rayon::join(SnapshotNodeStore::open(base), open(head))`            │
│                   │                                                          │
│                   ├─► Compare 32-byte header `content_digest` (`offset 72`)  │
│                   └─► If equal: `digest_equal = true` → O(1) immediate return│
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │ If digests differ
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. PARALLEL NODE INDEXING & STRUCTURAL COMPARISON (`snapshot_diff.rs`)       │
│                                                                              │
│  • Compute 64-bit `StableNodeKey` = BLAKE3(norm_path, name, node_type, qn)   │
│    directly over zero-copy mmap string pool slices (`string_at`)             │
│  • Partition Head nodes across `NODE_INDEX_SHARDS = 16` Rayon shards         │
│  • Scan Base nodes against Head index:                                       │
│      - In Head only  → `NodeDeltaKind::Added`                                │
│      - In Base only  → `NodeDeltaKind::Removed`                              │
│      - In Both       → Compare `(node_type, extension_digest_excluding_spans)`│
│                        (ignores byte/line shifts!) → `NodeDeltaKind::Changed`│
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. SORTED STABLE-KEY EDGE MERGE-JOIN & PR CASCADE                            │
│                                                                              │
│  • Map `EdgeRow (source_idx, target_idx, edge_type)` →                       │
│    `(StableNodeKey(from), StableNodeKey(to), EdgeType)`                      │
│  • Parallel sort + linear two-pointer merge-join → `EdgeDeltaKind::{Added,   │
│    Removed}`                                                                 │
│  • Consumers:                                                                │
│      - `rgctl diff --base <PATH> --head <PATH>`                              │
│      - `rgctl review paths` / `rgctl review check` (`pr-check`)              │
│      - `rgctl check --temporal`                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Content-Addressable Merkle Short-Circuit** | Merkle (1987), *A Digital Signature Based on a Conventional Encryption Function* | [`SnapshotPair::digest_equal`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/snapshot_diff.rs#L37-L39) compares the 32-byte BLAKE3 `content_digest` stored in the 136-byte `RGCL` v2 header (`offset 72..104`) in $O(1)$ time before touching node or edge tables. |
| **Location-Independent Structural Node Identity** | Lippert & Roslyn Team (2012), *Red-Green Syntax Trees* (separating syntax identity from physical offsets) | [`stable_key_from_facets`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/stable_key.rs#L91-L115) hashes `(normalized_file_path, name, node_type, qualified_name)` into a 64-bit `StableNodeKey` (and deterministic UUIDv5 via `NAMESPACE_RGCTL`), intentionally omitting `start_line` and `start_byte` so inserting lines at the top of a file never causes false symbol deletions/additions. |
| **Span-Excluding Cold Extension Digest** | Differential dataflow / semantic hash invariance | [`extension_digest_excluding_spans`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/node_extension.rs#L149-L179) strips the 14-byte Green span tail (`SPAN_EXTENSION_TAIL_BYTES = 14`: `decl_byte_len`, `name_span_overflow`, `name_token_len`) before hashing `NodeExtension` bytes, so pure formatting/offset shifts do not mark unchanged symbols as `Changed`. |
| **Sort-Merge Join over CSR Edge Streams** | Graefe (1993), *Query Evaluation Techniques for Large Databases* | Instead of building multi-gigabyte `HashSet<(Uuid, Uuid, EdgeType)>` tables, `diff_snapshots` projects base and head edges into flat `(StableNodeKey, StableNodeKey, EdgeType)` vectors, sorts them in parallel (`par_sort_unstable`), and runs a cache-linear two-pointer merge-join. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 `StableNodeKey` & `NodeRowRef` (`crates/rgctl-graph/src/stable_key.rs`)

```rust
/// Cross-snapshot stable node identity (first 8 bytes of BLAKE3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StableNodeKey(u64);

pub struct NodeRowRef {
    pub idx: usize,
    pub id: Uuid,
    /// BLAKE3 digest of cold NodeExtension excluding the 14-byte Green span tail.
    pub extension_digest: u64,
    pub node_type: NodeType,
    pub start_line: u32,
    pub end_line: u32,
}
```

#### Facet Hashing Wire Format (`stable_key_from_facets`)

```text
BLAKE3(
    normalize_path_str(file_path)
    || 0xFF
    || name.as_bytes()
    || 0xFF
    || (node_type as u16).to_le_bytes()
    || 0xFF
    || qualified_name.unwrap_or("")
)[0..8] -> u64 (LE)
```

### 3.2 Reverse-Dependency Cascade (`--cascade-depth`)

When synthesizing a head snapshot incrementally (`rgctl update`, `rgctl review paths`, `rgctl review check`):
1. `ChangeSet::invalidation_paths()` identifies modified, deleted, and renamed files.
2. `incoming_callers_files_depth(base_mmap, seeds, cascade_depth)` walks reverse `Calls` CSR edges up to `cascade_depth` hops (default `1`) to include untouched caller files whose outgoing call edges must be re-bound to updated symbol keys.

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl diff --help`, `rgctl review --help`, and [`src/cli/diff.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/diff.rs):

```bash
# Direct structural diff between two snapshots or .rgctl directories
rgctl diff --base /path/to/base/.rgctl --head /path/to/head/.rgctl -f json

# PR call-path spine comparison (uses snapshot diff + delta head synthesis)
rgctl review paths --base-ref origin/main --head-ref HEAD -f json

# Temporal PR policy check (classifies violations as NEW, EXISTING, or RESOLVED)
rgctl review check --policy-file policy.json --base-ref origin/main --head-ref HEAD -f json

# Compose full review report into .rgctl/workflows/review.json (or --stdout)
rgctl review export --base-ref origin/main --head-ref HEAD --stdout -f json
```

### 4.2 JSON Schema (`rgctl diff --base <B> --head <H> -f json`)

```json
{
  "schema_version": 1,
  "base": "/repo/.rgctl-diff/base/graph.snapshot.bin",
  "head": "/repo/.rgctl-diff/head/graph.snapshot.bin",
  "digest_equal": false,
  "nodes_added": 42,
  "nodes_removed": 11,
  "nodes_changed": 85,
  "edges_added": 194,
  "edges_removed": 63,
  "timings": {
    "wall_secs": 0.412,
    "open_secs": 0.008,
    "digest_secs": 0.0001,
    "diff_secs": 0.4039
  }
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Snapshot Pair & Structural Diff** | [`crates/rgctl-graph/src/snapshot_diff.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/snapshot_diff.rs) | `SnapshotPair`, `diff_snapshots`, `DiffSink`, `DiffStats`, `NodeDeltaEvent`, `EdgeDeltaEvent` |
| **Cross-Snapshot Stable Key** | [`crates/rgctl-graph/src/stable_key.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/stable_key.rs) | `StableNodeKey`, `NodeRowRef`, `stable_key_from_facets`, `stable_key_from_row`, `extension_digest` |
| **Span-Excluding Extension Digest** | [`crates/rgctl-graph/src/node_extension.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/node_extension.rs) | `extension_digest_excluding_spans`, `SPAN_EXTENSION_TAIL_BYTES` (`14`) |
| **Incremental PR Scope & Cascade** | [`crates/rgctl-incremental/src/lib.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-incremental/src/lib.rs) | `IncrementalUpdater`, `incoming_callers_files_depth` |
| **CLI Diff & Review Commands** | [`src/cli/diff.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/diff.rs), [`src/cli/review.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/review.rs), [`src/cli/pr_check.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/pr_check.rs) | `DiffArgs`, `ReviewCommands`, `run` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Zero Heap String Allocation on Key Hashing:** `stable_key_from_row` reads symbol names, paths, and qualified names as borrowed `&str` slices from the `ColumnarGraphMmap` string pool (`string_at`), never allocating `String` per node.
2. **16-Shard Parallel Hash Index (`NODE_INDEX_SHARDS = 16`):** Head node indexing partitions rows by `(idx % 16)` across Rayon workers into disjoint `HashMap<StableNodeKey, NodeRowRef>` shards to eliminate lock contention.
3. **Scoped PR Policy Evaluation:** `rgctl review check` hydrates only the upstream call closure of modified symbols (`BlastRadiusEngine::build_scoped`) rather than dual-hydrating two full `MemoryBackend` graphs.

### Verification Commands

```bash
# Run snapshot diff unit tests & microbenchmarks
cargo test -p rgctl-graph snapshot_diff
cargo bench -p rgctl-graph --bench snapshot_diff

# Linux two-ref cold diff profile gate (prepares v7.1 <-> HEAD under example/linux/.rgctl-diff/)
./scripts/prepare-linux-diff-snapshots.sh
cargo test --release --test cold_profile_gates linux_cold_diff_within_baseline -- --ignored --nocapture
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Cross-File Symbol Moves Appear as `Removed` + `Added`:** Because `StableNodeKey` includes the normalized repository-relative file path (`file_path`), moving a symbol to a different file emits one `NodeDeltaKind::Removed` event at the old path and one `NodeDeltaKind::Added` event at the new path.
2. **Same-Class Overload Collisions ([#183](https://github.com/rgctl/rgctl/issues/183)):** Until optional parameter-tuple disambiguation is enabled in `stable_key_from_facets`, overloaded methods in the same class with identical `qualified_name` share a `StableNodeKey`.
