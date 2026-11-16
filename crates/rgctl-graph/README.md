# `rgctl-graph`

Zero-copy columnar graph storage (`RGCL` v2), bounded external-memory spill (`SegmentedSpill`), cold metadata sidecars (`ColdMetadataDb` / `NodeExtension`), parked external relations (`RBPK`), lossless source/CST archives (`RGCS` v2 / `RGCT` v1), and LSM-style incremental delta compaction (`DeltaSegment` / `GraphCompactor`) for `rgctl`.

For the full 6-section architecture specification, binary byte layouts, and ingest invariants, see [**`docs/graph-storage-architecture.md`**](../../docs/graph-storage-architecture.md) and [**`docs/design/lossless-writeback-architecture.md`**](../../docs/design/lossless-writeback-architecture.md).

---

## Key Modules & Binary Artifacts

| Module | Artifact in `.rgctl/` | Description |
| :--- | :--- | :--- |
| [`columnar_snapshot.rs`](src/columnar_snapshot.rs) | `graph.snapshot.bin` (`RGCL` v2) | 136-byte header (with 32-byte BLAKE3 `content_digest`), 64-byte cache-line-aligned `NodeRow`, 40-byte `EdgeRow`, dual forward/reverse CSR arrays, and Zstd dictionary-compressed 64 KiB string blocks (`ColumnarGraphMmap`). |
| [`columnar_snapshot.rs`](src/columnar_snapshot.rs), [`schema.rs`](src/schema.rs) | `metadata.db/` | Cold variable-width node attributes (`qualified_name`, `return_type`, `code_hash`, 256-bit `token_bloom`, `parameters: Vec<GraphParameter>`, `properties`, `labels`) plus the fixed 14-byte Green span tail (`decl_byte_len`, `name_span_overflow`, `name_token_len`). |
| [`segmented_spill.rs`](src/segmented_spill.rs) | `spill/` (ephemeral) | `SegmentedSpill` external $k$-way merge sort with `DEFAULT_SORT_RUN_BYTES = 256 MiB` (`64 MiB` under `--with-limits`) and scratch buffer reuse. |
| [`columnar_snapshot.rs`](src/columnar_snapshot.rs) | `parked_relations.bin` (`RBPK` v1) | Unresolved external edges (stdlib, third-party packages) with source/target symbols, `EdgeType`, line/column, and call-site byte spans. |
| [`content_store.rs`](src/content_store.rs), [`lossless_content.rs`](src/lossless_content.rs) | `content_store.bin` (`RGCS` v2) | Content-addressed BLAKE3 `[u8; 32]` source file store with per-blob Zstd level-3 compression (`--lossless`). |
| [`cst_archive.rs`](src/cst_archive.rs) | `cst.archive.bin` (`RGCT` v1) | Succinct balanced-parentheses CST bitvector (2 bits/node) + `kind_ids` + relative `(lead_trivia_len, byte_width)` streams (`--lossless`). |
| [`graph_compactor.rs`](src/graph_compactor.rs) | `deltas/delta-*.bin` (`RGDL` v1) | `RoaringBitmap` tombstones + replacement nodes/edges (`OverlayGraph`) and background `GraphCompactor`. |
| [`snapshot_diff.rs`](src/snapshot_diff.rs) | `.rgctl-diff/` | Two-tier structural graph diffing (`StableNodeKey`, `extension_digest_excluding_spans`, sorted CSR set-difference). |

---

## Testing & Benchmarks

```bash
# Run all unit and property tests
cargo test -p rgctl-graph

# Run columnar traversal and snapshot diff benchmarks
cargo bench -p rgctl-graph --bench graph_benchmarks
cargo bench -p rgctl-graph --bench snapshot_diff
```
