# Unreleased (post v0.4.26)

<!-- Add bullets here during development; move to docs/releases/v0.4.x.md at tag time. -->

## Highlights

### Lossless Code Write-Back (`discover --lossless` & `rgctl writeback`)

`rgctl` can now act as a **bidirectional code engine**: in addition to extracting the code knowledge graph for analysis, you can opt into lossless source & Concrete Syntax Tree (CST) sidecars during discovery to verify byte-identical source reconstruction, export a working tree from `.rgctl/` alone, and apply surgical, syntax-validated byte-span patches (renames and structured mutation plans) that preserve every surrounding comment, whitespace byte, newline convention (`LF`/`CRLF`), and UTF-8 BOM.

```bash
# 1. Index the repository with lossless Zstd source + Green-CST sidecars
rgctl discover . --lossless

# 2. Verify 100% BLAKE3 byte identity and Green-CST leaf coverage
rgctl writeback verify -f json

# 3. Reconstruct the source tree into a directory solely from .rgctl/ sidecars
rgctl writeback export --output ./reconstructed -f json

# 4. Preview or apply a CST token-aware rename across the codebase
rgctl writeback patch --rename old_fn=new_fn --dry-run -f json
rgctl writeback patch --rename old_fn=new_fn --in-place -f json

# 5. Apply a structured multi-file byte-span edit plan
rgctl writeback patch --plan mutations.json --output ./patched -f json
```

- **Opt-in lossless sidecars (`rgctl discover --lossless`):**
  - Default `rgctl discover` remains unchanged with zero extra disk or wall-time overhead.
  - Passing `--lossless` (or `RGCTL_LOSSLESS=1`) writes two mmap-able sidecars alongside `.rgctl/graph.snapshot.bin`:
    - **`.rgctl/content_store.bin` (`RGCS` v2):** Zstd level-3 compressed source frames deduplicated by 32-byte BLAKE3 content hash, preserving exact original bytes (`LF`/`CRLF`, UTF-8 BOM, trailing whitespace) with $O(\log N)$ sorted path lookup.
    - **`.rgctl/cst.archive.bin` (`RGCT` v1):** Succinct Columnar Green-CST archive storing Balanced-Parentheses (`bp_bits`) tree topology, pre-order tree-sitter `kind_ids`, and relative `(lead_trivia_len, byte_width)` token widths without duplicating source text. Reuses the worker's already-parsed `tree_sitter::Tree` and precomputed BLAKE3 digest so `--lossless` never parses or hashes a file twice.
- **Red-Green byte spans on graph nodes (`rgctl -f json find` & `rgctl -f json inspect`):**
  - `NodeRow` packs `start_byte` (`u32`) and relative `name_rel_offset` (`u16`) into existing padding bytes, keeping the hot `NodeRow` column at exactly **64 bytes** (one CPU cache line).
  - `NodeExtension` appends a fixed 14-byte position-independent (Green) span tail (`decl_byte_len`, `name_span_overflow`, `name_token_len`) so `rgctl diff` / `stable_key` remains zero-copy and 100% invariant to line or byte shifts above a declaration.
  - `rgctl -f json find` and `rgctl -f json inspect` now expose `start_byte`, `end_byte`, `name_start_byte`, and `name_end_byte` on symbols across all Tier-1 languages.
- **Three `rgctl writeback` subcommands with `TrustLedgerReport` (`-f json`):**
  - **`rgctl writeback verify [--strict]`:** Verifies Zstd frame decompression, 32-byte BLAKE3 digests, and Green-CST leaf span monotonicity for every indexed file (plus live working-tree drift detection when source files are present on disk).
  - **`rgctl writeback export --output <dir> [--file <substr>]`:** Reconstructs source files directly from `.rgctl/content_store.bin`—even if the original working tree has been deleted.
  - **`rgctl writeback patch (--rename <old=new> | --plan <file.json>) [--dry-run | --in-place | --output <dir>] [--force]`:**
    - Applies non-overlapping byte-span splices in reverse byte order (`end_byte` descending) so earlier edits never invalidate later offsets in the same file, leaving all unedited byte ranges 100% untouched.
    - `--rename old=new` combines graph symbol `name_byte_span`s from `graph.snapshot.bin` with Green-CST leaf token spans from `cst.archive.bin`, ensuring comments (`// old`), string literals (`"old"`), and longer identifiers (`old_extended`) are never corrupted.
    - Re-parses every patched buffer with the language's tree-sitter grammar before writing to disk and rejects any file where an edit introduces `ERROR` or `MISSING` syntax nodes (`syntax_error_rejected`, overridable only with `--force`).
    - Emits a structured `TrustLedgerReport` JSON (`verified_identical`, `patched_verified`, `conflict_rejected`, `syntax_error_rejected`) with before/after BLAKE3 hashes per file.
  - Architecture & contributor reference: [Lossless Write-Back Architecture](../design/lossless-writeback-architecture.md).

### Shell completions

- `rgctl completions <bash|zsh|fish|elvish|powershell>` prints a shell completion script generated from the CLI definition; no repo or cache needed. Setup: [Installation → Shell completions](../installation.md#shell-completions).
