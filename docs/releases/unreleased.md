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

### Two-Layer Framework Migration Write-Back (`rgctl writeback migrate` & `rgctl writeback frameworks`)

Building on lossless sidecars, `rgctl` now includes a **two-layer A→B framework migration engine** (`java-framework-migration-writeback`) that deterministically splices 80–90% of structural boilerplate across **Waves 1–3** while emitting a blast-radius-ordered `agent_worklist[]` for the remaining ~10–20% of behavioral code and build manifests.

```bash
# 1. Inspect built-in bidirectional framework write-back profiles
rgctl -f json writeback frameworks --lang java

# 2. Layer 1 — Catalog A→B migration (e.g., Spring Boot → Quarkus, JavaEE → Quarkus, OpenLiberty → Quarkus)
rgctl -f json writeback migrate --from spring --to quarkus --dry-run
rgctl -f json writeback migrate --from javaee --to quarkus --in-place
rgctl -f json writeback migrate --from openliberty --to quarkus --in-place

# 3. Layer 2 — Uncataloged / custom library migration via declarative JSON plan (e.g., JUnit 4 → JUnit 5)
rgctl -f json writeback migrate --plan junit4-to-junit5.json --in-place
```

- **Layer 1 — Bidirectional Framework Catalog (`frameworks/*.toml` → `frameworks.bin`):**
  - Ships compiled `[[frameworks]]` write-back profiles across 10 languages (`java`, `kotlin`, `python`, `typescript`, `javascript`, `rust`, `csharp`, `go`, `php`, `ruby`), including `spring`, `javaee`, `openliberty`, `jakarta-ee`, and `quarkus` for Java/Kotlin.
  - Supports repo-local overrides in `.rgctl/frameworks.d/*.toml` (cached with BLAKE3 in `.rgctl/cache/frameworks.bin`) and normalizes common framework aliases (`springboot`/`spring-boot` → `spring`, `javax`/`java-ee` → `javaee`, `websphere`/`liberty` → `openliberty`, `jakarta` → `jakarta-ee`).
  - Self-correcting CLI diagnostics: passing an uncataloged framework to `--from` or `--to` lists all valid catalog IDs for that language and points the agent to `--plan <migration-plan.json>`.
- **Layer 2 — Declarative Migration Plans (`--plan <file.json>`) & Structural Census (`inventory --by import-prefix`):**
  - Added `rgctl -f json inventory --by import-prefix` to group `Import` nodes by 2–3 segment package prefixes so LLM agents can census any uncataloged library and author a `MigrationPlan` JSON (`namespace_rewrites`, `import_rewrites`, `annotation_rewrites`, `worklist_triggers`).
- **3-Wave AST-Aware Splicing + Graph-Seeded `agent_worklist[]`:**
  - **Wave 1 (Imports & Namespaces):** Rewrites `import` declarations, strips replaced source-framework annotation imports (including wildcard imports), rewrites qualified namespace prefixes (`javax.` → `jakarta.`) outside comments/strings, and inserts sorted target imports only for concepts actually matched in each file.
  - **Wave 2 (Class, Field & Method Annotations):** Single-pass tree-sitter AST matching rewrites stereotypes (`@RestController`/`@Service`/`@Repository`/`@Stateless` → `@ApplicationScoped`), DI (`@Autowired`/`@EJB`/`@PersistenceContext` → `@Inject`), config (`@Value("${k:default}")` → `@ConfigProperty(name = "k", defaultValue = "default")`), transactions (`@TransactionAttribute` → `@Transactional`), and HTTP routes (`@RequestMapping`/`@GetMapping`/`@PostMapping` → `@Path`/`@GET`/`@POST`, including `method = RequestMethod.POST` and multi-attribute `value =` / `path =`).
  - **Wave 3 (Parameter Annotations):** Rewrites `@PathVariable`/`@RequestParam`/`@RequestHeader` ↔ `@PathParam`/`@QueryParam`/`@HeaderParam` on formal parameters.
  - **Blast-Radius-Ordered `agent_worklist[]`:** Combines `.rgctl/graph.snapshot.bin` `Extends`/`Implements`/`Calls`/`AnnotatedWith` edges and incoming `Calls` counts with AST fallback triggers (e.g., `JpaRepository`, `AbstractStatelessSessionBean`, `SecurityContextHolder`, `WSSubject`, `getCallerPrincipal`, plus `pom.xml` / `build.gradle`) sorted by `caller_count` descending.
  - **Automatic `--in-place` refresh:** Running `rgctl writeback migrate --in-place` automatically runs `rgctl discover . --lossless` after Wave 3 so `.rgctl/` snapshots and lossless sidecars stay in sync.

### Shell completions

- `rgctl completions <bash|zsh|fish|elvish|powershell>` prints a shell completion script generated from the CLI definition; no repo or cache needed. Setup: [Installation → Shell completions](../installation.md#shell-completions).

## Breaking changes

| Changed | Migration |
|---------|-----------|
| Removed Konveyor Kantra integration (`rgctl rules`, `discover --with-kantra` / `--kantra-*`, `.rgctl/kantra_findings.json`, `.rgctl/dashboard/kantra_index.json`, `NodeType::KantraRule` / `KantraRuleset`, `EdgeType::Violates`, and the Dashboard Static Rules tab/room) ([#177](https://github.com/rgctl/rgctl/issues/177)) | Use `rgctl find --annotation`, `rgctl relations`, `rgctl discover --export-migration-hints` (`migration_plan.json`), and `rgctl writeback` for framework migration discovery and execution |
