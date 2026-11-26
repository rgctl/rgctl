# `rgctl` Graph Storage & Extraction Architecture

This document specifies the **storage, extraction, and compaction architecture** of `rgctl` (`crates/rgctl-graph`, `crates/rgctl-extraction`, and `crates/rgctl-core`). It details how source repositories are parsed in parallel, streamed through bounded disk spill segments, committed into the zero-copy `RBGR` v5 columnar mmap snapshot and companion `.rgctl/` binary sidecars, and queried or mutated with deterministic `content_digest` cache invalidation. For the dedicated specification of `rgctl`'s bidirectional CSR layout and CSR-backed graph algorithms, see [**CSR Topology & Zero-Copy Graph Architecture**](design/csr-topology-architecture.md).

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
│    │     • Optional AST reuse (`extract_with_cfg_pdg` → `cfg_pdg.archive.bin`)│
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
│  • `graph.snapshot.bin`  (`RBGR` v5, 8-byte aligned zero-copy mmap)          │
│      ├─ Header (136 B, `SNAPSHOT_MAGIC = b"RBGR"`, 64 B hex `content_digest`)│
│      ├─ Embedded `name_index` & `type_index` sections (lazy-parsed)          │
│      ├─ `NodeRow` table (`V × 64 B`: sorted UUID, packed `NodeType`, spans)  │
│      ├─ Bidirectional CSR (`row_ptr`, `in_row_ptr`, `targets`, `in_targets`, │
│      │    `edge_types`, `in_edge_types`) + `SparseEdgePad` table             │
│      ├─ Deduplicated UTF-8 `StringPool`                                      │
│      └─ Cold `NodeExtension` blob column (14 B Green span tail)              │
│  • `parked_relations.bin`(`RBPK` v1 unresolved external edges + call spans)  │
│  • `annotation_args.json`(sidecar `HashMap<String, String>` for annotations) │
│  • `content_store.bin`   (`RGCS` v2 Zstd content-addressed source blobs)     │
│  • `cst.archive.bin`     (`RGCT` v1 succinct balanced-parentheses CSTs)      │
│  • `deltas/delta-*.bin`  (`DeltaSegment` incremental append overlay)         │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Core Storage Principles

1. **Zero-Copy Hot Topology (`RGCL` v2):** Traversal, CSR neighborhood iteration, and symbol table lookups operate directly over OS virtual memory (`memmap2::Mmap`) without deserializing heap objects.
2. **Hot/Cold Vertical Partitioning:** Fixed-width 64-byte [`NodeRow`](../crates/rgctl-graph/src/columnar_snapshot.rs#L65-L96) and 40-byte [`EdgeRow`](../crates/rgctl-graph/src/columnar_snapshot.rs#L101-L119) records stay in the hot mmap; variable-width metadata (`qualified_name`, `return_type`, `parameters`, `token_bloom`, `properties`, `labels`, and lossless span overflow) lives in `ColdMetadataDb` (`.rgctl/metadata.db/`).
3. **Bounded-Memory Streaming Ingest:** Extraction workers stream per-file batches into [`SegmentedSpill`](../crates/rgctl-graph/src/segmented_spill.rs#L1-L120), bounding heap residency to fixed sort runs (`256 MiB` default; `64 MiB` under `--with-limits`).
4. **Deterministic Content Digest:** Every `graph.snapshot.bin` embeds a 32-byte BLAKE3 `content_digest` in its header (`offset 72..104`). All downstream caches (`analysis.snapshot.bin`, `blast_engine.snapshot.bin`, `macro_call_index.db`, `cfg_pdg.archive.bin`) validate this digest before reuse.

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Compressed Sparse Row (CSR) Dual Adjacency** | Buluç et al. (2009), *Parallel Sparse Matrix-Vector and Matrix-Transpose-Vector Multiplication Using Compressed Sparse Blocks* | [`ColumnarGraphMmap`](../crates/rgctl-graph/src/columnar_snapshot.rs#L240-L310) stores both forward (`out_offsets`, `out_edges`) and transpose (`in_offsets`, `in_edges`) CSR arrays at 64-byte page-aligned file offsets for $O(\text{deg}(v))$ traversal with zero allocations. |
| **Vertical Partitioning (Hot/Cold Column Store)** | Abadi, Boncz & Harizopoulos (2009), *Column-Stores vs. Row-Stores: How Different Are They Really?* | Hot graph topology fits in 64B `NodeRow` (1 CPU cache line) and 40B `EdgeRow`; cold variable-length attributes are fetched lazily on demand from `ColdMetadataDb` (`NodeExtension`). |
| **External Memory $k$-Way Merge Sort** | Vitter (2001), *External Memory Algorithms and Data Structures: Dealing with Massive Data* | [`SegmentedSpill`](../crates/rgctl-graph/src/segmented_spill.rs#L1-L120) buffers serialized node/edge records up to `DEFAULT_SORT_RUN_BYTES` (`256 MiB`), sorts in-memory by key, flushes run files to `.rgctl/spill/`, and streams a `BinaryHeap` k-way merge into the final snapshot writer. |
| **Log-Structured Merge (LSM) Delta Compaction** | O'Neil et al. (1996), *The Log-Structured Merge-Tree (LSM-Tree)* | Incremental updates (`rgctl discover --incremental`) write modified files as [`DeltaSegment`](../crates/rgctl-graph/src/graph_compactor.rs#L1-L90) overlays with a `RoaringBitmap` tombstone mask over base `NodeId`s, compacting via [`GraphCompactor`](../crates/rgctl-graph/src/graph_compactor.rs#L1-L95) when tombstone ratio exceeds 20% or delta count exceeds 8. |
| **Dictionary-Trained Block Compression** | Collet & Kucherawy (RFC 8878), *Zstandard Compression and the `application/zstd` Media Type* | `StringTableSection` trains a 32 KiB Zstd dictionary (`zstd::dict::from_continuous`) over sampled symbol/path strings and compresses 64 KiB string blocks (`BlockCompressedStringPool`) with an explicit `u32` offset index + 64-slot thread-local LRU block cache. |
| **Red-Green Syntax / Structural Identity Split** | Lippert & Roslyn Team (2012), *Persistence, Facades and Roslyn's Red-Green Trees* | [`NodeRow`](../crates/rgctl-graph/src/columnar_snapshot.rs#L65-L96) and [`NodeExtension`](../crates/rgctl-graph/src/columnar_snapshot.rs#L37-L115) separate structural identity (`StableNodeKey`, `extension_digest_excluding_spans`) from physical byte spans (`start_byte`, `name_rel_offset`, and the 14-byte Green span tail). |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 `RBGR` v5 Columnar Snapshot (`.rgctl/graph.snapshot.bin`)

Implemented in [`crates/rgctl-graph/src/columnar_snapshot.rs`](../crates/rgctl-graph/src/columnar_snapshot.rs) (`SNAPSHOT_MAGIC = *b"RBGR"`, `COLUMNAR_SNAPSHOT_VERSION = 5`). Readers remain backward-compatible with v2, v3, and v4 files (`2..=COLUMNAR_SNAPSHOT_VERSION`).

#### Header Layout (`136 bytes`)

| Byte Offset | Field | Type | Description |
| :--- | :--- | :--- | :--- |
| `0..4` | `magic` | `[u8; 4]` | Always `b"RBGR"` (`SNAPSHOT_MAGIC`) |
| `4..8` | `format_version` | `u32` (LE) | Columnar format version (`5`; readers accept `2..=5`) |
| `8..12` | `schema_version` | `u32` (LE) | `GRAPH_SCHEMA_VERSION` |
| `12..20` | `node_count` | `u64` (LE) | Total number of sorted `NodeRow` entries ($V$) |
| `20..28` | `edge_count` | `u64` (LE) | Total number of directed edges ($E$) |
| `28..92` | `content_digest` | `[u8; 64]` | Zero-padded UTF-8 BLAKE3 hex digest of canonical nodes + edges |
| `92..100` | `offset_nodes` | `u64` (LE) | 8-byte aligned start of `[NodeRow; V]` |
| `100..108` | `offset_edges` | `u64` (LE) | 8-byte aligned start of v5 bidirectional CSR section (or legacy `[EdgeRow; E]` on v2–v4) |
| `108..116` | `offset_strings` | `u64` (LE) | Start of deduplicated UTF-8 `StringPool` |
| `116..124` | `offset_strings_len` | `u64` (LE) | Byte length of `StringPool` |
| `124..128` | `_reserved` | `[u8; 4]` | Reserved zero bytes |
| `128..136` | `offset_extensions` | `u64` (LE) | Start of cold `NodeExtension` bincode + 14B Green span tail column |

#### `NodeRow` (`64 bytes`, `#[repr(C)]`, 1 CPU cache line)

Defined in [`columnar_snapshot.rs`](../crates/rgctl-graph/src/columnar_snapshot.rs#L480-L498) and strictly sorted by `id` (`INV-1`):

| Byte Offset | Field | Type | Description |
| :--- | :--- | :--- | :--- |
| `0..16` | `id` | `[u8; 16]` | Deterministic 128-bit UUID |
| `16..18` | `node_type` | `u16` (LE) | Low 12 bits (`0x0FFF`): `NodeType`; bit 12: `NODE_DECLARATION_BIT`; bits 14–15: `Visibility` |
| `18..20` | `name_rel_offset` | `u16` (LE) | Identifier byte offset relative to `start_byte` (`u16::MAX` = none/overflow) |
| `20..24` | `name_off` | `u32` (LE) | `StringPool` byte offset for symbol name |
| `24..28` | `name_len` | `u32` (LE) | `StringPool` byte length for symbol name |
| `28..32` | `file_path_off` | `u32` (LE) | `StringPool` byte offset for file path |
| `32..36` | `file_path_len` | `u32` (LE) | `StringPool` byte length for file path (`0` = none) |
| `36..40` | `signature_off` | `u32` (LE) | `StringPool` byte offset for signature |
| `40..44` | `signature_len` | `u32` (LE) | `StringPool` byte length for signature (`0` = none) |
| `44..48` | `start_line` | `u32` (LE) | 1-based start line (`0` = unknown) |
| `48..52` | `end_line` | `u32` (LE) | 1-based end line (`0` = unknown) |
| `52..56` | `extension_off` | `u32` (LE) | Offset into `offset_extensions` blob |
| `56..60` | `extension_len` | `u32` (LE) | Byte length of cold `NodeExtension` blob |
| `60..64` | `start_byte` | `u32` (LE) | 0-indexed declaration start byte (`u32::MAX` = unknown) |

#### Bidirectional CSR & Sparse Edge-Pad Section (v5, at `offset_edges`)

In v5 snapshots, the legacy 40-byte UUID `EdgeRow` array is replaced by 8-byte-aligned zero-copy CSR arrays (`row_ptr: [u32; V + 1]`, `in_row_ptr: [u32; V + 1]`, `targets: [u32; E]`, `in_targets: [u32; E]`, `edge_types: [u8; E]`, `in_edge_types: [u8; E]`) sorted per vertex by `(edge_type_code, neighbor_u32)`, followed by `pad_count: u32` and `pad_count` 12-byte `SparseEdgePad` entries for edges carrying optional `CoChangesWith`, `cross_repo`, or `call_site_line` metadata (`10 B/edge + 8 B/node`, ~3.45x smaller than v4 `EdgeRow`). Full layout, invariants (`INV-2`), and query complexity tables are specified in [**CSR Topology & Zero-Copy Graph Architecture**](design/csr-topology-architecture.md).

---

### 3.2 Cold Metadata (`NodeExtension`) & Method Overload Representation

Defined in [`crates/rgctl-graph/src/columnar_snapshot.rs`](../crates/rgctl-graph/src/columnar_snapshot.rs#L37-L115) and persisted in `.rgctl/metadata.db/` (`ColdMetadataDb`). Only nodes with `FLAG_HAS_EXTENSION` set in `NodeRow.flags` carry an entry.

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
- **Span-Excluding Structural Digest:** [`NodeExtension::extension_digest_excluding_spans`](../crates/rgctl-graph/src/columnar_snapshot.rs#L149-L179) hashes only the `bincode` payload (excluding the 14-byte span tail) so whitespace shifts in a file never trigger false-positive semantic modifications during `rgctl diff`.
- **Parameters & Signatures Across Layers:**
  - Language plugins populate `Symbol.signature: Option<String>` and `Symbol.parameters: Vec<Parameter>` (`name`, `param_type`, `default_value`, `is_variadic`).
  - `GraphBuilder::add_symbol_with_prep` interns `signature` into `NodeRow.signature_off` and stores `parameters: Vec<GraphParameter>` in `NodeExtension`.
  - Note on symbol keying: `GraphBuilder::symbol_key` currently keys symbols by `(normalized_file_path, symbol.qualified_name_or_name(), symbol.kind)`. See Section 6 for current overload disambiguation boundaries ([#183](https://github.com/rgctl/rgctl/issues/183)).

---

### 3.3 Companion `.rgctl/` Sidecars & Incremental Deltas

| Artifact Path | Magic / Version | Purpose & Key Types |
| :--- | :--- | :--- |
| `.rgctl/parked_relations.bin` | `RBPK` v1 | [`ParkedRelationRecord`](../crates/rgctl-extraction/src/parked_relations.rs#L22-L60): stores edges whose target symbols live outside the repository (stdlib, third-party packages, Maven/Cargo/npm deps), preserving `source_file`, `source_symbol`, `target_symbol`, `relation_kind`, `line`, `column`, `call_site_start_byte`, `call_site_byte_len`, and `metadata`. Used by `rgctl inventory --by import-prefix`, `rgctl vuln`, and `rgctl writeback`. |
| `.rgctl/annotation_args.json` | JSON v1 | Sidecar map `HashMap<String, String>` keyed by `"{source_uuid}|{target_uuid}|{line}"` storing raw annotation/decorator arguments (`@RequestMapping("/api")` → `"/api"`) extracted from `Relation.metadata["arguments"]` on `EdgeType::AnnotatedWith` edges. Consumed by `rgctl find --annotation <name> --show-attributes`. |
| `.rgctl/content_store.bin` | `RGCS` v2 | [`ContentStoreReader`](../crates/rgctl-graph/src/content_store.rs#L1-L120): content-addressed `[u8; 32]` BLAKE3 index + independent Zstd level-3 frames (`RGCS_FLAG_ZSTD = 0x01`) storing exact original file bytes (including BOM and CRLF/LF) when `rgctl discover --lossless` is enabled. |
| `.rgctl/cst.archive.bin` | `RGCT` v1 | [`CstSidecarReader`](../crates/rgctl-graph/src/cst_archive.rs#L1-L130): per-file succinct tree-sitter CST topology encoded via a balanced-parentheses bitvector (2 bits/node) + `kind_ids: Vec<u16>` + relative `(lead_trivia_len, byte_width)` arrays. |
| `.rgctl/deltas/delta-*.bin` | `RGDL` v1 | [`DeltaSegment`](../crates/rgctl-graph/src/graph_compactor.rs#L1-L95): incremental overlay containing a `RoaringBitmap` of tombstoned base `NodeId`s plus replacement `Node` and `Edge` lists. Merged lazily via `OverlayGraph` or compacted via [`GraphCompactor`](../crates/rgctl-graph/src/graph_compactor.rs#L1-L95). |
| `.rgctl/file_hashes.json` | JSON v1 | `FileTracker` state mapping normalized relative paths to BLAKE3 file hashes and node ID ranges for $O(1)$ incremental invalidation. |

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 Discovery & Compaction CLI Commands

Verified against `rgctl --help` and [`src/cli/args.rs`](../src/cli/args.rs):

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
| **Columnar Mmap Snapshot (`RGCL` v2)** | [`crates/rgctl-graph/src/columnar_snapshot.rs`](../crates/rgctl-graph/src/columnar_snapshot.rs) | `ColumnarSnapshotHeader`, `NodeRow`, `EdgeRow`, `ColumnarGraphMmap`, `write_columnar_snapshot` |
| **Cold Metadata Store** | [`crates/rgctl-graph/src/columnar_snapshot.rs`](../crates/rgctl-graph/src/columnar_snapshot.rs), [`cold_metadata.rs`](../crates/rgctl-analysis/src/cold_metadata.rs) | `NodeExtension`, `GraphParameter`, `ColdMetadataDb`, `extension_digest_excluding_spans` |
| **Disk Spill & External Merge** | [`crates/rgctl-graph/src/segmented_spill.rs`](../crates/rgctl-graph/src/segmented_spill.rs) | `SegmentedSpill`, `DEFAULT_SORT_RUN_BYTES` (`256 MiB`), `CONSTRAINED_SORT_RUN_BYTES` (`64 MiB`) |
| **Graph Builder & Pass-1/2 Commit** | [`crates/rgctl-extraction/src/graph_builder.rs`](../crates/rgctl-extraction/src/graph_builder.rs) | `GraphBuilder`, `SymbolPass1Prep`, `add_symbol_with_prep`, `emit_mro_overrides` |
| **Parked External Relations (`RBPK`)** | [`crates/rgctl-extraction/src/parked_relations.rs`](../crates/rgctl-extraction/src/parked_relations.rs) | `ParkedRelationRecord`, `write_parked_relations`, `read_parked_relations` |
| **Lossless Sidecars (`RGCS` / `RGCT`)** | [`crates/rgctl-graph/src/content_store.rs`](../crates/rgctl-graph/src/content_store.rs), [`cst_archive.rs`](../crates/rgctl-graph/src/cst_archive.rs) | `ContentStoreWriter`, `ContentStoreReader`, `CstSidecarWriter`, `CstSidecarReader` |
| **Incremental Delta & Compactor** | [`crates/rgctl-graph/src/graph_compactor.rs`](../crates/rgctl-graph/src/graph_compactor.rs), [`graph_compactor.rs`](../crates/rgctl-graph/src/graph_compactor.rs) | `DeltaSegment`, `OverlayGraph`, `GraphCompactor`, `CompactionConfig` |

---

## 5. Performance Guardrails & Verification Checklist

### Ingest Hot-Path Rules (`AGENTS.md`)

1. **No Per-Symbol Heap Strings on Merge Thread:** Pass `&str` slices and worker-precomputed [`SymbolPass1Prep`](../crates/rgctl-extraction/src/graph_builder.rs#L95-L140) (line offsets, BLAKE3 `code_hash`, 256-bit `token_bloom`) into `add_symbol_with_prep`.
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
