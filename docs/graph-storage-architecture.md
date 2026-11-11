# `rgctl` Graph Storage & Extraction Architecture

This document specifies the **storage, extraction, and compaction architecture** of `rgctl` (`crates/rgctl-graph`, `crates/rgctl-extraction`, and `crates/rgctl-core`). It details how source repositories are parsed in parallel, streamed through bounded disk spill segments, committed into the zero-copy `RGCL` v2 columnar mmap snapshot and companion `.rgctl/` binary sidecars, and queried or mutated with deterministic `content_digest` cache invalidation.

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. PARALLEL EXTRACTION (`crates/rgctl-extraction`, Rayon worker pool)        │
│                                                                              │
│  Source File (&[u8])                                                         │
│    ├─► Thread-local tree-sitter Parser (`rgctl_plugin_helpers::parse_source`)│
│    ├─► Single-pass LanguageExtractor → `FileExtraction`                      │
│    │     • `Symbol` (spans, signature, `Vec<Parameter>`, visibility, etc.)   │
│    │     • `Relation` (`Calls`, `Imports`, `Inherits`, `AnnotatedWith`, ...) │
│    │     • Optional AST reuse (`extract_with_cfg_pdg` → v3 `cfg_pdg.bin`)    │
│    ├─► Worker-side `SymbolPass1Prep`:                                        │
│    │     • Line offsets, BLAKE3 `code_hash`, 256-bit `token_bloom` (`[u64;4]`)│
│    └─► Optional `--lossless` sidecars:                                       │
│          • `ContentStoreWriter` → `.rgctl/content_store.bin` (`RGCS` v2)     │
│          • `CstSidecarWriter`   → `.rgctl/cst.archive.bin`   (`RGCT` v1)     │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │ bounded `mpsc::sync_channel`
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. STREAMING PASS-1 & DISK SPILL (`GraphBuilder` + `SegmentedSpill`)         │
│                                                                              │
│  Sequential Pass-1 Merge Thread:                                             │
│    ├─► `begin_file_batch` → `add_symbol_with_prep` → `end_file_batch`        │
│    │     • Assigns dense 0..V `NodeId` (`u32`) and populates symbol tables   │
│    │     • Spills cold `NodeExtension` records to `.rgctl/spill/`            │
│    ├─► Resolves cross-file relations (Pass-2) or parks unresolved targets    │
│    │     into `.rgctl/parked_relations.bin` (`RBPK` v1)                      │
│    └─► External k-way merge sort (`DEFAULT_SORT_RUN_BYTES = 256 MiB`,        │
│        or `64 MiB` when constrained via `--with-limits max-mem-mb=...`)      │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. PERSISTENT `.rgctl/` ARTIFACT SUITE (`crates/rgctl-graph`)                │
│                                                                              │
│  • `graph.snapshot.bin`  (`RGCL` v2, 64-byte aligned zero-copy mmap)         │
│      ├─ Header (136 B, `content_digest: [u8; 32]`)                           │
│      ├─ `NodeRow` table (`V × 64 B`: `NodeId`, `NodeType`, spans, interned)  │
│      ├─ `EdgeRow` table (`E × 40 B`: `source`, `target`, `EdgeType`, spans)  │
│      ├─ Forward CSR (`(V + 1) × 8 B` offsets + `E × 4 B` edge indices)       │
│      ├─ Reverse CSR (`(V + 1) × 8 B` offsets + `E × 4 B` edge indices)       │
│      └─ Zstd dictionary + block-compressed string pool (64 KiB blocks)       │
│  • `metadata.db/`        (`ColdMetadataDb` / `NodeExtension` + 14B tail)     │
│  • `parked_relations.bin`(`RBPK` v1 unresolved external edges + call spans)  │
│  • `annotation_args.json`(sidecar `HashMap<String, String>` for annotations) │
│  • `content_store.bin`   (`RGCS` v2 Zstd content-addressed source blobs)     │
│  • `cst.archive.bin`     (`RGCT` v1 succinct balanced-parentheses CSTs)      │
│  • `deltas/delta-*.bin`  (`DeltaSegment` incremental append overlay)         │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Core Storage Principles

1. **Zero-Copy Hot Topology (`RGCL` v2):** Traversal, CSR neighborhood iteration, and symbol table lookups operate directly over OS virtual memory (`memmap2::Mmap`) without deserializing heap objects.
2. **Hot/Cold Vertical Partitioning:** Fixed-width 64-byte [`NodeRow`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/columnar_snapshot.rs#L65-L96) and 40-byte [`EdgeRow`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/columnar_snapshot.rs#L101-L119) records stay in the hot mmap; variable-width metadata (`qualified_name`, `return_type`, `parameters`, `token_bloom`, `properties`, `labels`, and lossless span overflow) lives in `ColdMetadataDb` (`.rgctl/metadata.db/`).
3. **Bounded-Memory Streaming Ingest:** Extraction workers stream per-file batches into [`SegmentedSpill`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/spill.rs#L1-L120), bounding heap residency to fixed sort runs (`256 MiB` default; `64 MiB` under `--with-limits`).
4. **Deterministic Content Digest:** Every `graph.snapshot.bin` embeds a 32-byte BLAKE3 `content_digest` in its header (`offset 72..104`). All downstream caches (`analysis.snapshot.bin`, `blast_engine.snapshot.bin`, `macro_call_index.db`, `cfg_pdg.bin`) validate this digest before reuse.

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Compressed Sparse Row (CSR) Dual Adjacency** | Buluç et al. (2009), *Parallel Sparse Matrix-Vector and Matrix-Transpose-Vector Multiplication Using Compressed Sparse Blocks* | [`ColumnarGraphMmap`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/columnar_snapshot.rs#L240-L310) stores both forward (`out_offsets`, `out_edges`) and transpose (`in_offsets`, `in_edges`) CSR arrays at 64-byte page-aligned file offsets for $O(\text{deg}(v))$ traversal with zero allocations. |
| **Vertical Partitioning (Hot/Cold Column Store)** | Abadi, Boncz & Harizopoulos (2009), *Column-Stores vs. Row-Stores: How Different Are They Really?* | Hot graph topology fits in 64B `NodeRow` (1 CPU cache line) and 40B `EdgeRow`; cold variable-length attributes are fetched lazily on demand from `ColdMetadataDb` (`NodeExtension`). |
| **External Memory $k$-Way Merge Sort** | Vitter (2001), *External Memory Algorithms and Data Structures: Dealing with Massive Data* | [`SegmentedSpill`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/spill.rs#L1-L120) buffers serialized node/edge records up to `DEFAULT_SORT_RUN_BYTES` (`256 MiB`), sorts in-memory by key, flushes run files to `.rgctl/spill/`, and streams a `BinaryHeap` k-way merge into the final snapshot writer. |
| **Log-Structured Merge (LSM) Delta Compaction** | O'Neil et al. (1996), *The Log-Structured Merge-Tree (LSM-Tree)* | Incremental updates (`rgctl discover --incremental`) write modified files as [`DeltaSegment`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/delta_segment.rs#L1-L90) overlays with a `RoaringBitmap` tombstone mask over base `NodeId`s, compacting via [`GraphCompactor`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/compactor.rs#L1-L95) when tombstone ratio exceeds 20% or delta count exceeds 8. |
| **Dictionary-Trained Block Compression** | Collet & Kucherawy (RFC 8878), *Zstandard Compression and the `application/zstd` Media Type* | `StringTableSection` trains a 32 KiB Zstd dictionary (`zstd::dict::from_continuous`) over sampled symbol/path strings and compresses 64 KiB string blocks (`BlockCompressedStringPool`) with an explicit `u32` offset index + 64-slot thread-local LRU block cache. |
| **Red-Green Syntax / Structural Identity Split** | Lippert & Roslyn Team (2012), *Persistence, Facades and Roslyn's Red-Green Trees* | [`NodeRow`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/columnar_snapshot.rs#L65-L96) and [`NodeExtension`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/node_extension.rs#L37-L115) separate structural identity (`StableNodeKey`, `extension_digest_excluding_spans`) from physical byte spans (`start_byte`, `name_rel_offset`, and the 14-byte Green span tail). |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 `RGCL` v2 Columnar Snapshot (`.rgctl/graph.snapshot.bin`)

Implemented in [`crates/rgctl-graph/src/columnar_snapshot.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/columnar_snapshot.rs). All sections are padded to **64-byte cache-line boundaries** (`ALIGNMENT = 64`).

#### Header Layout (`136 bytes`, `#[repr(C)]`)

| Byte Offset | Field | Type | Description |
| :--- | :--- | :--- | :--- |
| `0..4` | `magic` | `[u8; 4]` | Always `b"RGCL"` |
| `4..8` | `version` | `u32` (LE) | Format version (`2`) |
| `8..16` | `node_count` | `u64` (LE) | Total number of `NodeRow` entries ($V$) |
| `16..24` | `edge_count` | `u64` (LE) | Total number of `EdgeRow` entries ($E$) |
| `24..32` | `node_table_offset` | `u64` (LE) | 64-byte aligned start of `[NodeRow; V]` |
| `32..40` | `edge_table_offset` | `u64` (LE) | 64-byte aligned start of `[EdgeRow; E]` |
| `40..48` | `out_offsets_offset` | `u64` (LE) | Forward CSR offsets (`[u64; V + 1]`) |
| `48..56` | `out_edges_offset` | `u64` (LE) | Forward CSR edge indices (`[u32; E]`) |
| `56..64` | `in_offsets_offset` | `u64` (LE) | Reverse CSR offsets (`[u64; V + 1]`) |
| `64..72` | `in_edges_offset` | `u64` (LE) | Reverse CSR edge indices (`[u32; E]`) |
| `72..104` | `content_digest` | `[u8; 32]` | BLAKE3 digest of all preceding topology & string sections |
| `104..112` | `string_offsets_offset` | `u64` (LE) | String table metadata & block index offset |
| `112..120` | `string_data_offset` | `u64` (LE) | Zstd dictionary + compressed 64 KiB string blocks |
| `120..128` | `string_count` | `u64` (LE) | Number of interned strings ($S$) |
| `128..136` | `_reserved` | `[u8; 8]` | Reserved zero bytes |

#### `NodeRow` (`64 bytes`, `#[repr(C)]`, 1 CPU cache line)

Defined in [`columnar_snapshot.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/columnar_snapshot.rs#L65-L96):

| Byte Offset | Field | Type | Description |
| :--- | :--- | :--- | :--- |
| `0..16` | `id_bytes` | `[u8; 16]` | Deterministic UUIDv5 / stable 128-bit `NodeId` |
| `16..20` | `name_off` | `u32` | Interned string ID for simple symbol name |
| `20..24` | `file_off` | `u32` | Interned string ID for normalized repository-relative path |
| `24..28` | `lang_off` | `u32` | Interned string ID for language (`rust`, `java`, `c`, etc.) |
| `28..32` | `line_start` | `u32` | 1-based start line |
| `32..36` | `line_end` | `u32` | 1-based end line |
| `36..40` | `col_start` | `u32` | 0-based start column |
| `40..44` | `col_end` | `u32` | 0-based end column |
| `44..48` | `signature_off` | `u32` | Interned string ID for full signature (`u32::MAX` if none) |
| `48..52` | `visibility_off` | `u32` | Interned string ID for visibility (`public`, `private`, etc.) |
| `52` | `node_type` | `u8` | Compact [`NodeType`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-core/src/types.rs#L118-L165) discriminant (`Function=0`, `Method=1`, `Class=2`, ...) |
| `53` | `flags` | `u8` | Bitflags (`FLAG_IS_ASYNC=1<<0`, `FLAG_IS_STATIC=1<<1`, `FLAG_IS_ABSTRACT=1<<2`, `FLAG_IS_EXPORTED=1<<3`, `FLAG_HAS_EXTENSION=1<<4`) |
| `54..56` | `name_rel_offset` | `u16` | Relative byte offset of identifier token from `start_byte` (`0xFFFF` = overflow in `NodeExtension`) |
| `56..60` | `start_byte` | `u32` | File-relative UTF-8 start byte of declaration (`u32::MAX` if unknown) |
| `60..64` | `complexity` | `u32` | Cyclomatic complexity precomputed during extraction |

#### `EdgeRow` (`40 bytes`, `#[repr(C)]`)

Defined in [`columnar_snapshot.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/columnar_snapshot.rs#L101-L119):

| Byte Offset | Field | Type | Description |
| :--- | :--- | :--- | :--- |
| `0..4` | `source_idx` | `u32` | Dense source node index (`0..V`) |
| `4..8` | `target_idx` | `u32` | Dense target node index (`0..V`) |
| `8..12` | `edge_type` | `u32` | Compact [`EdgeType`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-core/src/types.rs#L220-L280) discriminant (`Calls`, `Imports`, `Inherits`, `Implements`, `Overrides`, `AnnotatedWith`, etc.) |
| `12..16` | `weight_bits` | `u32` | IEEE-754 `f32::to_bits(weight)` |
| `16..20` | `line` | `u32` | Call-site / reference line number (`u32::MAX` if none) |
| `20..24` | `column` | `u32` | Call-site / reference column (`u32::MAX` if none) |
| `24..28` | `context_off` | `u32` | Interned string ID for call-site expression/context (`u32::MAX` if none) |
| `28..32` | `call_site_start_byte` | `u32` | File-relative start byte of call/import token (`u32::MAX` if unknown) |
| `32..34` | `call_site_byte_len` | `u16` | Byte length of call/import token (`0` if unknown) |
| `34..40` | `_pad` | `[u8; 6]` | Explicit zero alignment padding |

---

### 3.2 Cold Metadata (`NodeExtension`) & Method Overload Representation

Defined in [`crates/rgctl-graph/src/node_extension.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/node_extension.rs#L37-L115) and persisted in `.rgctl/metadata.db/` (`ColdMetadataDb`). Only nodes with `FLAG_HAS_EXTENSION` set in `NodeRow.flags` carry an entry.

```rust
pub struct GraphParameter {
    pub name: String,
    pub param_type: Option<String>,
    pub default_value: Option<String>,
}

pub struct NodeExtension {
    pub qualified_name: Option<String>,
    pub return_type: Option<String>,
    pub code_hash: Option<String>,
    pub token_bloom: Option<[u64; 4]>,
    pub parameters: Vec<GraphParameter>,
    pub properties: LazyStringMap,
    pub labels: Vec<String>,
    // Appended as a fixed 14-byte Green span tail (`SPAN_EXTENSION_TAIL_BYTES`):
    pub decl_byte_len: u32,
    pub name_span_overflow: (u32, u32),
    pub name_token_len: u16,
}
```

- **14-Byte Green Span Tail (`SPAN_EXTENSION_TAIL_BYTES = 14`):** `NodeExtension::to_bytes()` serializes the first 7 fields via `bincode`, then appends a fixed 14-byte little-endian tail (`decl_byte_len: u32`, `name_span_overflow.0: u32`, `name_span_overflow.1: u32`, `name_token_len: u16`).
- **Span-Excluding Structural Digest:** [`NodeExtension::extension_digest_excluding_spans`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/node_extension.rs#L149-L179) hashes only the `bincode` payload (excluding the 14-byte span tail) so whitespace shifts in a file never trigger false-positive semantic modifications during `rgctl diff`.
- **Parameters & Signatures Across Layers:**
  - Language plugins populate `Symbol.signature: Option<String>` and `Symbol.parameters: Vec<Parameter>` (`name`, `param_type`, `default_value`, `is_variadic`).
  - `GraphBuilder::add_symbol_with_prep` interns `signature` into `NodeRow.signature_off` and stores `parameters: Vec<GraphParameter>` in `NodeExtension`.
  - Note on symbol keying: `GraphBuilder::symbol_key` currently keys symbols by `(normalized_file_path, symbol.qualified_name_or_name(), symbol.kind)`. See Section 6 for current overload disambiguation boundaries ([#183](https://github.com/rgctl/rgctl/issues/183)).

---

### 3.3 Companion `.rgctl/` Sidecars & Incremental Deltas

| Artifact Path | Magic / Version | Purpose & Key Types |
| :--- | :--- | :--- |
| `.rgctl/parked_relations.bin` | `RBPK` v1 | [`ParkedRelationRecord`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/parked_relations.rs#L22-L60): stores edges whose target symbols live outside the repository (stdlib, third-party packages, Maven/Cargo/npm deps), preserving `source_file`, `source_symbol`, `target_symbol`, `relation_kind`, `line`, `column`, `call_site_start_byte`, `call_site_byte_len`, and `metadata`. Used by `rgctl inventory --by import-prefix`, `rgctl vuln`, and `rgctl writeback`. |
| `.rgctl/annotation_args.json` | JSON v1 | Sidecar map `HashMap<String, String>` keyed by `"{source_uuid}|{target_uuid}|{line}"` storing raw annotation/decorator arguments (`@RequestMapping("/api")` → `"/api"`) extracted from `Relation.metadata["arguments"]` on `EdgeType::AnnotatedWith` edges. Consumed by `rgctl find --annotation <name> --show-attributes`. |
| `.rgctl/content_store.bin` | `RGCS` v2 | [`ContentStoreReader`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/content_store.rs#L1-L120): content-addressed `[u8; 32]` BLAKE3 index + independent Zstd level-3 frames (`RGCS_FLAG_ZSTD = 0x01`) storing exact original file bytes (including BOM and CRLF/LF) when `rgctl discover --lossless` is enabled. |
| `.rgctl/cst.archive.bin` | `RGCT` v1 | [`CstSidecarReader`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/cst_sidecar.rs#L1-L130): per-file succinct tree-sitter CST topology encoded via a balanced-parentheses bitvector (2 bits/node) + `kind_ids: Vec<u16>` + relative `(lead_trivia_len, byte_width)` arrays. |
| `.rgctl/deltas/delta-*.bin` | `RGDL` v1 | [`DeltaSegment`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/delta_segment.rs#L1-L95): incremental overlay containing a `RoaringBitmap` of tombstoned base `NodeId`s plus replacement `Node` and `Edge` lists. Merged lazily via `OverlayGraph` or compacted via [`GraphCompactor`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/compactor.rs#L1-L95). |
| `.rgctl/file_hashes.json` | JSON v1 | `FileTracker` state mapping normalized relative paths to BLAKE3 file hashes and node ID ranges for $O(1)$ incremental invalidation. |

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 Discovery & Compaction CLI Commands

Verified against `rgctl --help` and [`src/cli/args.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/args.rs):

```bash
# Cold full-repository discovery (default columnar RGCL v2 snapshot)
rgctl discover . -v

# Language-filtered discovery with exclusions
rgctl discover . -l rust,python -e vendor,third_party -f json

# Lossless discovery (populates RGCS v2 content_store.bin + RGCT v1 cst.archive.bin)
rgctl discover . --lossless -f json

# Constrained container/cgroup discovery (caps memory, shrinks sort runs to 64 MiB)
rgctl discover . --with-limits max-mem-mb=4096,threads=2

# Incremental discovery + manual LSM delta compaction
rgctl discover . --incremental
rgctl compact -f json

# Merge multiple repository snapshots into a unified multi-repo graph
rgctl merge --inputs repo-a/.rgctl,repo-b/.rgctl -o merged/.rgctl -f json
```

### 4.2 Implementation Map

| Component | Source File | Key Types & Entry Points |
| :--- | :--- | :--- |
| **Columnar Mmap Snapshot (`RGCL` v2)** | [`crates/rgctl-graph/src/columnar_snapshot.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/columnar_snapshot.rs) | `ColumnarSnapshotHeader`, `NodeRow`, `EdgeRow`, `ColumnarGraphMmap`, `write_columnar_snapshot` |
| **Cold Metadata Store** | [`crates/rgctl-graph/src/node_extension.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/node_extension.rs), [`cold_metadata.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/cold_metadata.rs) | `NodeExtension`, `GraphParameter`, `ColdMetadataDb`, `extension_digest_excluding_spans` |
| **Disk Spill & External Merge** | [`crates/rgctl-graph/src/spill.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/spill.rs) | `SegmentedSpill`, `DEFAULT_SORT_RUN_BYTES` (`256 MiB`), `CONSTRAINED_SORT_RUN_BYTES` (`64 MiB`) |
| **Graph Builder & Pass-1/2 Commit** | [`crates/rgctl-extraction/src/builder.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-extraction/src/builder.rs) | `GraphBuilder`, `SymbolPass1Prep`, `add_symbol_with_prep`, `emit_mro_overrides` |
| **Parked External Relations (`RBPK`)** | [`crates/rgctl-graph/src/parked_relations.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/parked_relations.rs) | `ParkedRelationRecord`, `write_parked_relations`, `read_parked_relations` |
| **Lossless Sidecars (`RGCS` / `RGCT`)** | [`crates/rgctl-graph/src/content_store.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/content_store.rs), [`cst_sidecar.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/cst_sidecar.rs) | `ContentStoreWriter`, `ContentStoreReader`, `CstSidecarWriter`, `CstSidecarReader` |
| **Incremental Delta & Compactor** | [`crates/rgctl-graph/src/delta_segment.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/delta_segment.rs), [`compactor.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/compactor.rs) | `DeltaSegment`, `OverlayGraph`, `GraphCompactor`, `CompactionConfig` |

---

## 5. Performance Guardrails & Verification Checklist

### Ingest Hot-Path Rules (`AGENTS.md`)

1. **No Per-Symbol Heap Strings on Merge Thread:** Pass `&str` slices and worker-precomputed [`SymbolPass1Prep`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-extraction/src/builder.rs#L95-L140) (line offsets, BLAKE3 `code_hash`, 256-bit `token_bloom`) into `add_symbol_with_prep`.
2. **Thread-Local Tree-Sitter Parsers:** Always parse via `rgctl_plugin_helpers::parse_source` (1 `Parser` per Rayon worker thread), never `Parser::new()` per file.
3. **Hash & Normalize Once per File:** Compute `FileExtraction.file_hash` on the extraction worker from bytes already in memory; bracket file symbol commits with `begin_file_batch` / `end_file_batch`.
4. **Spill Buffer Reuse:** Reuse `SegmentedSpill`'s scratch `Vec<u8>` with `bincode::serialize_into`; keep sort runs at `DEFAULT_SORT_RUN_BYTES` (`256 MiB`) unless `--with-limits` is active.

### Verification Commands

```bash
# Unit & property tests for columnar mmap, spill, delta segments, and sidecars
cargo test -p rgctl-graph -p rgctl-extraction

# Snapshot diff & columnar traversal microbenchmarks
cargo bench -p rgctl-graph --bench graph_benchmarks
cargo bench -p rgctl-graph --bench snapshot_diff

# Gate A: Linux kernel cold discover regression gate (<= 75s baseline + 10% on ref M3 Pro)
cargo build --release --bin rgctl
cargo test --release --test cold_profile_gates linux_cold_discover_within_baseline -- --ignored --nocapture
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Same-Class Method Overloads Share Pass-1 Symbol Key ([#183](https://github.com/rgctl/rgctl/issues/183)):** While `NodeRow.signature_off` and `NodeExtension.parameters` store full parameter lists, `GraphBuilder::symbol_key` and `StableNodeKey::from_node` currently key symbols by `(file, qualified_name, kind)` without a parameter-type tuple discriminator. Overloaded methods in the same class merge or overwrite during Pass-1 symbol resolution until optional parameter-tuple disambiguation lands.
2. **Parked External Targets Require Downstream Resolution:** Calls/imports targeting symbols not defined in the analyzed repository are stored in `.rgctl/parked_relations.bin` (`RBPK`) rather than creating phantom `NodeRow` entries in `graph.snapshot.bin`, keeping vertex count $V$ strictly proportional to repository source symbols.
3. **Lossless Sidecars Are Opt-In (`--lossless`):** Default `rgctl discover` records Red-Green byte spans (`start_byte`, `name_rel_offset`, `decl_byte_len`, `call_site_start_byte`) in `RGCL` v2 and `ColdMetadataDb`, but only writes `.rgctl/content_store.bin` (`RGCS` v2) and `.rgctl/cst.archive.bin` (`RGCT` v1) when `--lossless` is passed.
