# Lossless write-back architecture


This section serves as the engineering reference for contributors working on Tier-1 lossless extraction, columnar snapshot layouts, and the piece-table write-back engine (and will be published in-tree at `docs/design/lossless-writeback-architecture.md`).

### 1. System Overview & End-to-End Data Flow

`rgctl`'s lossless architecture separates **semantic graph queries** (fast 64-byte `NodeRow` / 16-byte `EdgeRow` scans in `.rgctl/graph.snapshot.bin`) from **syntactic & lexical reconstruction** (`.rgctl/cst.archive.bin` + `.rgctl/content_store.bin`).

```
                          ┌────────────────────────────────────────┐
                          │   Source Tree (16 Tier-1 Languages)    │
                          └───────────────────┬────────────────────┘
                                              │
                         Rayon Worker Pool (`rgctl-extraction`)
                   Single-Pass Tree-Sitter Walk + BLAKE3 `file_hash`
                                              │
         ┌────────────────────────────────────┼────────────────────────────────────┐
         │ (Always-on, 0 extra bytes/row)     │ (`--lossless` / `RGCTL_LOSSLESS=1`)│
         ▼                                    ▼                                    ▼
┌─────────────────────────────┐  ┌─────────────────────────────┐  ┌─────────────────────────────┐
│  .rgctl/graph.snapshot.bin  │  │    .rgctl/cst.archive.bin   │  │  .rgctl/content_store.bin   │
│         (RGCL v2)           │  │          (RGCT v1)          │  │          (RGCS v2)          │
├─────────────────────────────┤  ├─────────────────────────────┤  ├─────────────────────────────┤
│ • 64B NodeRow:              │  │ • Succinct BP bitvector     │  │ • Zstd-compressed (lvl 3)   │
│   - start_byte: u32 (@60)   │  │   (2 bits/CST node)         │  │   raw file frames           │
│   - name_rel_offset: u16(@18│  │ • Columnar kind_id: [u16]   │  │ • Deduplicated by 32-byte   │
│ • 16B EdgeRow + Call Spans  │  │ • Packed (lead_trivia: u16, │  │   BLAKE3 file_hash          │
│ • String Table (`offset_`)  │  │   byte_width: u16)          │  │ • Preserves BOM, CRLF/LF,   │
│ • Digest-invariant          │  │ • Sparse (cst_idx, NodeId)  │  │   comments & macros 100%    │
│   NodeExtension             │  │   anchor table              │  │                             │
└──────────────┬──────────────┘  └──────────────┬──────────────┘  └──────────────┬──────────────┘
               │                                │                                │
               └────────────────────────────────┼────────────────────────────────┘
                                                │
                                       Zero-Copy `mmap` Open
                                                │
                                                ▼
                          ┌────────────────────────────────────────┐
                          │   Piece-Table Write-Back & Splicer     │
                          │   (`crates/rgctl-graph/src/writeback`) │
                          ├────────────────────────────────────────┤
                          │ 1. Resolve target NodeId / EdgeId      │
                          │ 2. Project Red-tree byte spans         │
                          │ 3. Verify disjoint ascending SpanEdits │
                          │ 4. Splice dirty spans + stream         │
                          │    unmodified slices verbatim          │
                          │ 5. Re-parse patched buffer via         │
                          │    tree-sitter & emit TrustLedger      │
                          └───────────┬───────────────┬────────────┘
                                      │               │
                        ┌─────────────┘               └─────────────┐
                        ▼                                           ▼
         ┌─────────────────────────────┐             ┌─────────────────────────────┐
         │ `rgctl writeback verify` /  │             │   `rgctl writeback patch`   │
         │ `rgctl writeback export`    │             │  (Surgical Rename / Patch   │
         │ (100% BLAKE3 Byte Identity) │             │   + Tree-Sitter Validation) │
         └─────────────────────────────┘             └─────────────────────────────┘
```

---

### 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Origin | How `rgctl` Implements It |
| :--- | :--- | :--- |
| **Red-Green Syntax Trees** | Roslyn / `rust-analyzer` (`rowan`) / OpenRewrite LST | **Green data** is position-independent (`name_rel_offset: u16` relative to symbol `start_byte`, and relative `(lead_trivia_len, byte_width)` in `cst.archive.bin`). **Red coordinates** (`name_start_byte = start_byte + name_rel_offset`) are computed on the fly during queries/splicing. |
| **Merkle Content Addressing** | HyperAST / Git Packfiles | Extract workers already compute a 32-byte BLAKE3 `file_hash` and per-symbol `code_hash`. `content_store.bin` v2 deduplicates identical files across the repository using `file_hash`. |
| **Succinct Tree Topology** | Munro & Raman Balanced Parentheses (`BP`) | Instead of pointer-heavy `Vec<AstSkeletonNode>` (~40+ bytes/node), `cst.archive.bin` encodes CST topology in `2N` bits (`1` = open, `0` = close) alongside flat `u16` columns (~4.25 bytes/node). |
| **Piece-Table Span Splicing & Trust Ledger** | `StructFix` / `CT-Repair` / CPG Trust Ledgers | Write-back never invokes a full-file pretty-printer (which destroys comments, formatting, and preprocessor macros). Unmodified byte ranges are copied verbatim from `content_store.bin`; only mutated spans are spliced and re-validated with tree-sitter. |

---

### 3. Binary Layout & Memory Discipline

#### 3.1 `NodeRow` (64-Byte Cache-Line Invariant)
Located in `crates/rgctl-graph/src/columnar_snapshot.rs`. `NodeRow` MUST remain exactly 64 bytes (`#[repr(C)]`, `Pod`, `Zeroable`):

```
Byte Offset:  0       1       2..4    4..8          8..12         12..16       16..20
            ┌───────┬───────┬───────┬─────────────┬─────────────┬────────────┬────────────┐
Word 0..19: │ kind  │ lang  │ flags │ name_offset │ file_offset │ line_start │  line_end  │
            └───────┴───────┴───────┴─────────────┴─────────────┴────────────┴────────────┘
Byte Offset: 20..22            22..26       26..30          30..34       34..38       38..42
            ┌─────────────────┬────────────┬───────────────┬────────────┬────────────┬────────────┐
Word 20..41:│ name_rel_offset │ sig_offset │ qname_offset  │ doc_offset │ parent_id  │community_id│
            │ (was _pad: u16) │   (u32)    │     (u32)     │   (u32)    │   (u32)    │   (u32)    │
            └─────────────────┴────────────┴───────────────┴────────────┴────────────┴────────────┘
Byte Offset: 42..46      46..50    50..54         54..58  58..60 60..64
            ┌───────────┬─────────┬──────────────┬───────┬──────┬──────────────────────┐
Word 42..63:│ pagerank  │ ext_id  │ code_hash_id │  ...  │      │     start_byte       │
            │   (f32)   │  (u32)  │    (u32)     │       │      │ (was _pad_end: u32)  │
            └───────────┴─────────┴──────────────┴───────┴──────┴──────────────────────┘
```
*(Note: exact field offsets follow the existing `#[repr(C)]` layout in `columnar_snapshot.rs:122-137` where `_pad: u16` after `line_end` becomes `name_rel_offset: u16` and `_pad_end: u32` at offset 60 becomes `start_byte: u32`.)*

* **Sentinels:**
  * `start_byte == u32::MAX` → symbol has no source byte span (e.g., synthetic file/module node).
  * `name_rel_offset == u16::MAX` → either symbol has no identifier token or the identifier begins $\ge 65,535$ bytes after `start_byte` (in which case `NodeExtension.name_span_overflow: Option<(u32, u32)>` holds the exact relative start and byte length).
* **Red-Tree Span Formula:**
  ```rust
  pub fn name_byte_span(&self, unqualified_ident_len: u32, ext: Option<&NodeExtension>) -> Option<(u32, u32)> {
      if self.start_byte == u32::MAX {
          return None;
      }
      if self.name_rel_offset != u16::MAX {
          let start = self.start_byte.checked_add(u32::from(self.name_rel_offset))?;
          let len = ext.and_then(|e| e.name_token_len.map(u32::from)).unwrap_or(unqualified_ident_len);
          Some((start, start.checked_add(len)?))
      } else {
          let (rel_start, len) = ext?.name_span_overflow?;
          let start = self.start_byte.checked_add(rel_start)?;
          Some((start, start.checked_add(len)?))
      }
  }
  ```

#### 3.2 `rgctl diff` Digest Invariance Rule
In `crates/rgctl-graph/src/stable_key.rs`, `extension_digest` hashes semantic metadata (`return_type`, `visibility`, `modifiers`, `is_async`, `is_static`, `is_abstract`, `is_Exported`, `is_test`, `cyclomatic_complexity`, `generic_params`, `annotations`, `section_level`, `parameters`).
* **Rule:** Never hash `end_byte`, `name_span_overflow`, or any line/column/byte coordinate in `extension_digest`.
* **Why:** Inserting lines above a symbol shifts `start_byte` and `end_byte` by $+\Delta$, but leaves `name_rel_offset` and `extension_digest` unchanged, guaranteeing zero false-positive `NodeDeltaKind::Changed` entries in `rgctl diff`.

---

### 4. Contributor Workflow: Adding or Auditing a Tier-1 Language for Write-Back

All 16 Tier-1 languages (`c`, `cpp`, `csharp`, `go`, `groovy`, `java`, `javascript`, `kotlin`, `markdown`, `php`, `puppet`, `erb`, `python`, `ruby`, `rust`, `typescript`) follow a uniform 3-step contract in their `crates/rgctl-lang-<lang>/src/single_pass.rs` (or `plugin.rs`) walker:

1. **Record Declaration & Identifier Spans on Every Symbol:**
   Call `rgctl_plugin_helpers::record_symbol_spans(&mut symbol, decl_node, name_node)` when constructing any `Symbol`.
   * Pass the full declaration `tree_sitter::Node` (`function_declaration`, `method_declaration`, `class_declaration`, `field_declaration`, etc.) as `decl_node`.
   * Pass the exact identifier child node (`node.child_by_field_name("name")` or language equivalent) as `name_node`.
   * If `symbol.name` is synthesized (e.g., C++ `ClassName::methodName`, Rust `impl Trait for Type`, or overloaded signature suffixes), `record_symbol_spans` automatically sets `name_token_len` to `name_node.byte_range().len() as u16` so `name_end_byte` slices only the identifier token in source.

2. **Record Call-Site & Import Spans on Relations:**
   When emitting `EdgeType::Calls`, `EdgeType::Imports`, or `EdgeType::References`, populate `relation.call_site_start_byte` and `relation.call_site_byte_len` from the target callee identifier / import path `tree_sitter::Node`.

3. **Feed `GreenCstFileBuilder` When Lossless Mode Is Enabled:**
   In the iterative DFS pre-order push/pop loop:
   * On node enter: push `1` to `bp_bits`, push `node.kind_id()` to `kind_ids`, and push packed `(lead_trivia_len, byte_width)` to `rel_delta_and_width`. If the CST node created a `Symbol`, record `(cst_node_idx, symbol_local_idx)` in `graph_anchors`.
   * On node exit: push `0` to `bp_bits`.

---

### 5. Performance Guardrails Checklist (Mandatory for Contributors)

Before opening or merging any change touching lossless extraction or write-back, verify:

1. **`sizeof(NodeRow) == 64`**: Confirmed by compile-time `assert!`.
2. **No Merge-Thread Allocations**: `add_symbol_with_prep` in `crates/rgctl-extraction/src/graph_builder.rs` receives pre-computed scalar `u32`/`u16` fields in `SymbolPass1Prep`.
3. **Worker-Side Zstd & BLAKE3 Reuse**: Never call `std::fs::read` or re-compute BLAKE3 in `save_tracker` or during archive commit; reuse `FileExtraction.file_hash` and worker-compressed Zstd frames.
4. **Thread-Local Tree-Sitter Re-Parse**: `rgctl writeback patch` must validate spliced buffers using `rgctl_plugin_helpers::parse_source` (thread-local `Parser`), never `Parser::new()` per file.
5. **Pre-Flight & Cold Gates**:
   * Run `./scripts/preflight-pr.sh --quick` (`RGCTL_AST_COVERAGE_STRICT=1`, `fmt`, `clippy -D warnings`, all test targets).
   * Run `cargo test --release --test writeback_roundtrip_tier1` and `cargo test --release --test writeback_patch_integration`.
   * Run `cargo test --release --test cold_profile_gates linux_cold_discover_within_baseline -- --ignored --nocapture` to confirm Gate A stays within the 75s baseline.


---

### 6. Known Limitations & By-Design Boundaries

To keep `rgctl` fast, deterministic, and free of per-language code formatters, the write-back architecture intentionally enforces the following 7 boundaries:

1. **No "De Novo" Code Synthesis from Pure Abstract Graph Nodes/Edges:**
   * *Boundary:* You cannot insert a brand-new `NodeRow` (e.g., `kind = Method`) and `EdgeRow`s (`Calls`) into the graph without a concrete source byte template and ask `rgctl writeback` to invent the method body (`if`/`for`/`try` control flow, local variable names, and indentation).
   * *Why:* A semantic call graph / PDG is a many-to-one projection; infinitely many concrete programs share the same graph edges. Write-back requires either an existing source frame in `content_store.bin` or an explicit replacement byte slice in `SpanEdit`.
2. **No Cross-Language Transpilation (e.g., Discover Java → Write Back as Kotlin/Rust):**
   * *Boundary:* `rgctl writeback` reconstructs and patches files strictly in their original Tier-1 source language.
   * *Why:* `.rgctl/cst.archive.bin` stores raw tree-sitter `kind_id: u16` values bound to the source language's grammar (`tree-sitter-java`, `tree-sitter-rust`, etc.), not a language-independent Universal AST (UAST).
3. **No Guaranteed Dynamic-Dispatch / Reflection Refactoring Without `--scip`:**
   * *Boundary:* Tree-sitter-only extraction cannot resolve runtime duck-typing (`obj.process()` in Python/Ruby/JS/PHP), reflection (`Class.forName`), or complex overload shadowing that requires full compiler type inference.
   * *Why:* `rgctl writeback patch --rename` rewrites the edges present in the graph; for 100% semantic soundness on overloaded or polymorphic symbols, discovery should be run with `--scip`.
4. **No Refactoring Inside Unexpanded Preprocessor Macros or Proc-Macros:**
   * *Boundary:* While `rgctl writeback verify` and `export` reproduce `#define`, `#ifdef`, and `macro_rules!` blocks 100% byte-for-byte, `rgctl writeback patch` will not rewrite symbols synthesized via C token-pasting (`FOO_##BAR`) or Rust proc-macros.
   * *Why:* Tree-sitter parses unexpanded surface syntax. Any mutation touching a macro-ambiguous or `ERROR` span is aborted and logged as `ConflictRejected` or `ParseWarningPassthrough` in the `TrustLedgerReport`.
5. **No Automatic Code Re-Formatting or Line Wrapping (`rustfmt` / `prettier` / `gofmt`):**
   * *Boundary:* Splicing a 40-character identifier in place of a 3-character identifier replaces those exact bytes without re-wrapping lines at 80/100 columns or re-sorting imports.
   * *Why:* Embedding 16 language formatters would destroy original file formatting outside the edited span and introduce noisy diffs. Users can run their project formatter after `rgctl writeback patch` if desired.
6. **No Overlapping or Nested Structural Edits in a Single Pass:**
   * *Boundary:* If a mutation plan contains two overlapping byte spans in the same file (`edit[i - 1].end_byte > edit[i].start_byte`, such as replacing an entire call expression while simultaneously renaming an argument inside it), the Piece-Table Splicer rejects the file patch with `ConflictRejected`.
   * *Why:* Piece tables operate on flat byte coordinates of a single snapshot. Nested rewrites must either be coalesced into a single outer `SpanEdit` or applied across sequential passes (`patch` → `update` → `patch`).
7. **No Tier-2 (`tags.scm`) Languages or Unindexed Binary Assets:**
   * *Boundary:* The ~45 Tier-2 query-only languages and non-code binary files (`.jar`, `.class`, images, lockfiles) are excluded from `.rgctl/content_store.bin` and `.rgctl/cst.archive.bin` (`SkippedNonTier1` in the `TrustLedgerReport`).

| Operation | Supported? | Mechanism / Reason |
| :--- | :---: | :--- |
| **100% byte-identical round-trip of Tier-1 files (`verify` / `export`)** | **Yes** | Zstd `content_store.bin` + Succinct Green-CST (`cst.archive.bin`) |
| **Surgical identifier / call-site / import / span replacement (`patch`)** | **Yes** | Red-Green `NodeRow` spans + Piece-Table Splicer + tree-sitter check |
| **Preserving comments, CRLF/LF, BOMs, and macros on unmodified code** | **Yes** | Zero-copy slice streaming from `content_store.bin` |
| **Generating a brand-new function/file purely from abstract graph nodes** | **No (By Design)** | Call/PDG graph is lossy without an underlying source/CST template |
| **Cross-language transpilation (e.g., Java graph → Rust code)** | **No (By Design)** | Green-CST `kind_id`s are bound to the original language's tree-sitter grammar |
| **Rewriting inside C `#define` token-pasting or Rust proc-macros** | **No (By Design)** | Tree-sitter parses unexpanded source; guarded by `TrustLedgerReport` |
| **Auto-wrapping lines / running `rustfmt` or `prettier` on spliced spans** | **No (By Design)** | Splicer is byte-exact and formatter-agnostic |
| **Composing overlapping/nested byte edits in a single pass** | **No (By Design)** | Piece-Table Splicer enforces disjoint spans (`ConflictRejected`) |
| **Tier-2 languages (~45 `tags.scm` languages) & binary assets** | **No (By Design)** | Scoped strictly to the 16 Tier-1 `bundled_specs()` languages |
