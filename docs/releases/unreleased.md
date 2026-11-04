# Unreleased (post v0.4.25)

<!-- Add bullets here during development; move to docs/releases/v0.4.x.md at tag time. -->

## Highlights

### Agent skills & migration workflows (#170, #104, #105)

Overhauls `rgctl` and `rgctl-migrate` skills (`.agent/skills/*`, `skills/*`, and `docs/agents/USER_AGENTS_TEMPLATE.md`) following the ScarfBench evaluation ([#170](https://github.com/rgctl/rgctl/issues/170)):

- **Mandatory Step 0 (`.rgctl/` acquisition first):** Agents now verify `.rgctl/graph.snapshot.bin` (`rgctl -f json status`) and run `rgctl discover .` if absent before reading source files or falling back to `grep`/`rg`/`find`.
- **First-line code intelligence for all tasks:** Removed the "skip rgctl for code-edit tasks" escape hatch; positioned `rgctl -f json` (`find`, `relations`, `callers`, `callees`, `inventory`, `blast-radius`, `slice`) as the primary engine for questions, investigations, refactorings, and migrations.
- **Continuous sync for long-running migrations:** Documented keeping `.rgctl/` fresh across multi-wave edits using `rgctl serve --watch` (debounced background file watcher) or `rgctl update` between edit waves.
- **Three-track `rgctl-migrate` skill:** Broadened `rgctl-migrate` beyond Strangler-Fig decomposition into three tracks:
  - **Track A — Framework, Runtime & Version Migration** (`inventory --by import-prefix`, `find "import <prefix>*" --type import`, `find --annotation ... --show-attributes`, `relations --edge injects/exposesroute`, `callers` / `blast-radius --summary` wave ordering).
  - **Track B — API, Package & Library Refactoring** (`find --package`, `callers`, `cpg mutations`, `slice`, `blast-radius`).
  - **Track C — Architectural / Strangler-Fig Decomposition** (`discover . --export-migration-hints --with-harmonic`, `.rgctl/migration_plan.json` `.steps[]`, `communities list`, boundary `blast-radius`). Mainframe language flags (`-l cobol,pli,jcl`) are no longer the lead recipe.
- **Explicit Kantra target matrix:** Documented Kantra strictly as an optional target-specific overlay for its embedded ruleset catalog (`openjdk11+`, `openjdk17+`, `openjdk21+`, `jakarta-ee9+`, `quarkus`, `quarkus3+`, `spring-boot3+`, `spring6+`, `hibernate6+`, `camel3+`, `camel4+`, `eap7`, `eap8`, `openliberty`, `cloud-readiness`, `angular`, `patternfly-v6`, `dotnet-core`).
- **Workflows & build tracking:** Fixed the inventory JSON key in `skills/rgctl/references/workflows.md` (`.items` → `.counts`, [#104](https://github.com/rgctl/rgctl/issues/104)) and added `cargo:rerun-if-changed` for `skills/rgctl-migrate` and `skills/rgctl-vuln` in `build.rs`.

### Kantra target metadata, embedded `rules run`, and token-compact `rules findings` (#170)

Enables running `rgctl discover . --with-kantra` (all targets in one pass) or post-discover `rgctl rules run`, then querying only the target(s) needed:

- **Target/source/effort metadata on violations & rule nodes:**
  - `KantraViolation` in `.rgctl/kantra_findings.json` now persists `targets: Vec<String>`, `sources: Vec<String>`, and `effort: Option<u32>` from the matched rule's `konveyor.io/target` and `konveyor.io/source` labels.
  - Hydrated `KantraRule` graph nodes preserve comma-separated `targets` and `sources` properties alongside `konveyor.io/target` / `konveyor.io/source` so multi-target rules retain all target labels.
- **`rgctl rules run [DIR] [--target <TARGET>] [--catalog <PATH>]`:**
  - `<DIR>` is now optional; omitting `<DIR>` and `--catalog` evaluates the compiled-in `KantraCatalog::embedded()` against an existing `.rgctl/graph.snapshot.bin` without re-running `discover`.
  - `-f json` output is token-compacted: emits `summary` (`violations`, `unique_rules`, `unique_files`), `skipped_rules_count` (omitting raw `skipped_rules[]`), deduplicated `per_rule[]` metadata, and up to 50 compact `violations`.
- **`rgctl rules findings [--target <CSV>] [--source <CSV>] [--category <CAT>] [--file <SUBSTR>] [--limit <N>]`:**
  - Filters `.rgctl/kantra_findings.json` by one or more comma-separated targets/sources, category, or file substring.
  - Computes `total_matched`, `summary`, and `per_rule[]` over the full matched set **before** applying `--limit` truncation.
  - Emits rule `message`, `category`, `effort`, and `targets` once per unique rule in `per_rule[]` and omits duplicate `message` text on individual `violations[]` entries to minimize LLM token usage.
- **`rgctl rules targets`:**
  - Lists all `konveyor.io/target` and `konveyor.io/source` values in the embedded catalog with rule counts, plus violation hit counts per target/source when `.rgctl/kantra_findings.json` is present.

### Java field-level annotations & DI provenance (#170)

- **Single-pass field `Variable` extraction (`rgctl-lang-java`):**
  - `extract_all_single_pass` now emits `SymbolType::Variable` nodes for fields carrying framework/DI/JPA/configuration annotations directly during the single-pass AST walk (ignoring Lombok/nullability/compiler noise markers such as `@NonNull`, `@Nullable`, `@Getter`, `@Setter`, `@SuppressWarnings`, `@Deprecated`, `@Override`, `@Serial`, `@Builder`).
  - `rgctl -f json find --annotation "@PersistenceContext,@ConfigProperty,@Inject" --show-attributes` now resolves annotated fields and their parsed annotation arguments (`unitName`, `name`, etc.) without requiring `--with-cfg`.
  - `GraphBuilder::add_symbol_with_prep` labels field `Variable` nodes with `"field"` so `index_symbol_resolution` skips multi-segment package suffix expansion in `symbols_by_suffix`.
- **`@ConfigProperty` & `Injects` annotation provenance (`rgctl-plugin-helpers`, `rgctl-extraction`):**
  - Added `ConfigProperty` to `spring.jakarta.field_di` and `jakarta.ee.di` in `frameworks/java.toml`.
  - `EdgeType::Injects` relations and graph edges now record the triggering annotation (e.g. `@Inject`, `@PersistenceContext`, `@EJB`, `@ConfigProperty`) in `call_context`.

### Incremental update hygiene & query CLI ergonomics (#104, #105, #170)

- **Snapshot-only `rgctl update` & `serve --watch` (`rgctl-incremental`):**
  - Removed legacy `graph.save_to_repo(repo_root)` calls from `IncrementalUpdater` so incremental updates only write `.rgctl/graph.snapshot.bin` and never emit multi-MB `.rgctl/graph.json` files.
- **Automatic `.git/info/exclude` registration (`rgctl-graph`):**
  - `ensure_rgctl_git_exclude` automatically appends `.rgctl/` to `<repo>/.git/info/exclude` during `discover` and `update` when `.git/info` exists, keeping `git status` clean without modifying tracked `.gitignore` files.
- **`rgctl inventory --limit <N> --prefix <STR>` (#104):**
  - Added native prefix filtering and result-count capping to `rgctl inventory` (e.g. `rgctl -f json inventory --by import-prefix --prefix javax. --limit 20`).
- **`rgctl blast-radius <Symbol> --summary` (#105):**
  - Added compact `--summary` output returning `target`, `score`, `distance_buckets` (`direct`, `transitive`, `impact_zone_size`), `caller_depth_limit` (when `--depth` is set), up to 10 `direct_callers` (`fqn`, `file_path`), and top 10 impacted files (`top_files`).
