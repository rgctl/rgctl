# PR Review & Coding Practices Checklist

Mandatory engineering practices for **contributors**, **AI coding agents**, and **PR reviewers** working on `rgctl`. Every item below corresponds to a real scale, memory, crash, or correctness regression caught on multi-hundred-thousand- to multi-million-node corpora (`example/linux`, `example/kafka`, `example/metasfresh-4.9.8b`, `example/discourse`).

When **writing code** or **reviewing a PR**, audit the diff against all six sections below before approving or merging.

---

## Quick Diff Red-Flag Scan (Run During Every PR Review)

When reviewing a PR (`gh pr diff <N>` or `git diff main...HEAD`), treat any of the following in hot paths (`rgctl-plugin-*`, `rgctl-extraction`, `rgctl-graph`, `rgctl-analysis`, `rgctl-pipeline`, `src/cli/discover*`, `src/cli/vuln*`, `src/cli/metrics*`) as a **blocking review item** until justified and cold-profiled:

| Red flag in diff | Why it regresses at scale | Required replacement |
|---|---|---|
| `symbol.metadata[...]` / `relation.metadata[...]` / `serde_json::json!` per symbol or call | Allocates heap `BTreeMap` + owned `String` keys on millions of symbols/edges and stalls sequential Pass-1 merge | Add compact typed fields (`Option<Box<str>>`, `Option<Cow<'static, str>>`, `u8`, `bool`, bitflags) on `Symbol` / `Relation`; bump `facts_cache::FORMAT_VERSION` when layout changes |
| `symbols.iter().filter(...)` or linear AST/scope ancestor walks per call site | Turns per-file call extraction into `O(calls × symbols)` (`>60 s` parse CPU on Java/PHP/C) | Pre-index once per file with `CallFileIndex` (`push_call_relation_from_indexed`) and `O(1)` `ScopeStack` owner lookup |
| `.children(&mut cursor).collect::<Vec<_>>()` inside per-symbol / per-parameter walkers | Millions of short-lived `Vec<Node>` heap allocations during CST traversal | Walk `node.children(&mut cursor)` directly in a streaming loop |
| `HashMap<Uuid, String>` for file paths or cloning `(String, NodeType)` keys into multiple maps in `GraphBuilder` | Multi-GB duplicate heap strings (`1.8M–2.8M` nodes) + multi-second synchronous `drop` stall | Intern `file_id: u32` per `begin_file_batch`; key secondary maps by `Uuid`; use `FxHashMap`; drop massive tables on a background thread after snapshot compile |
| Allocating `edge.properties` maps during spilled discover | `SegmentedSpill::append_edge` (`for_columnar_digest`) discards `edge.properties` (`~30M` wasted `String` allocs on Linux) | Guard property insertion with `if self.spill.is_none()` unless a hot column explicitly stores it |
| `collect_nodes_by_type` / `materialize_node` loops during default `discover` or before `--with-cfg` | Deserializes `950k–1.9M` cold `NodeExtension` bincode blobs (`+1.2 s` wall, `+3.5–4.6 GB` RSS) | Emit hot summaries (`HotFunctionSummary`) from Pass 1, or read hot columns (`node_type_at`, `node_file_path_str`, `node_ids_for_type`) and materialize cold nodes per-file on workers |
| Whole-repo `preload_file_sources`, `.par_iter().map(...).collect::<Vec<_>>()` of encoded artifacts, or double `bincode::serialize` | Multiplies peak RAM (`28+ GiB` RSS and `+8 GiB` swap on `linux --with-cfg`) | Read source per `FileWorkGroup` on the worker, serialize `(CFG, PDG)` once, and stream over a bounded `sync_channel` into `AnalysisStorage` / `StreamingCfgPdgArchiveWriter` |
| Unbounded recursive `fn visit_*` / `fn walk_*` over tree-sitter `Node`s | Overflows worker stacks (`SIGABRT`) on deep method chains or `2,500`-arm `elsif` ladders (`example/discourse`) | Use an explicit iterative `Vec<(Node, ...)>` stack capped at `MAX_AST_WALK_DEPTH = 2048` (skipping nested `reset_kinds` subtrees) and cap statement/condition recursion at `MAX_CFG_STATEMENT_DEPTH = 1024` |
| `limit: Some(N)` on internal `StructuredQuery` / graph scans used for correctness (`vuln`, `dead-code`, `review`, `check`) | Silently drops imports/callers past `N` on medium/large repos (e.g. `rgctl` itself has `14,800+` `Import` nodes) | Never cap internal correctness scans; stream hot columnar rows (`node_type_at`, `node_name_str`, `node_file_path_str`) with zero `limit` |
| Exact `BetweennessCentrality::compute_unbounded` or `MemoryBackend` hydration on CLI query paths | Single-threaded `O(V·E)` stalls and unnecessary multi-GB hydration when mmap snapshot exists | Use `FlatGraphIndex::from_snapshot_store` + Rayon-parallel active-pivot `SampledBetweenness` (and honor `--iterations` in both human and `-f json` modes) |
| New `discover` work outside `[profile]` timers | Creates "dark-matter" wall-time gaps where `index_secs + post_index_secs << wall_secs` | Wrap every top-level phase in `DiscoverStageReport` so `\|wall_secs - accounted_secs\| < 0.5 s` (`tests/profile_stage_accounting.rs`) |

---

## 1. Ingest, Extraction & Pass-1 / Pass-2 Scale

- **Typed hot structs:** Keep `rgctl_plugin_api::Symbol` and `Relation` `metadata` as `serde_json::Value::Null` unless rare language-specific properties exist. Callable discriminators, roles, parameter identities, and complexity belong in typed struct fields.
- **Language-aware discriminator work:** Only format parameter-type tuples where overload disambiguation requires them (skip parameter CST formatting on non-overloaded C definitions when a body is present; preserve full declaration tuples on prototypes/headers).
- **Prefer implementations over prototypes:** Scoped and cross-file callable resolution (`resolve_scoped_callable` / `c_preferred_impl`) must prefer concrete definitions (`!declaration_ids.contains(id)`) when both `.h` declarations and `.c` implementations exist.
- **Parallel Pass-2 determinism (`#125`):** Read-only endpoint resolution may run in parallel across path-sorted `ExtractionTail` chunks (`Pass2WorkerScratch` local caches), but external stub materialization (`ensure_external_stub`), parked-relation recording, and `SegmentedSpill` edge commits **must** execute sequentially in deterministic `(normalized_file_path, relation_index)` order. Verify with single-thread vs multi-thread snapshot digest equality (`parallel_pass2_matches_single_thread_snapshot_digest_bit_for_bit`).

---

## 2. Streaming CFG/PDG & Memory Discipline (`--with-cfg` / `--full`)

- **Per-file streaming residency:** Never preload all repository source files or materialize all `NodeType::Function` cold extensions up front. Group work by file using hot `ColumnarGraphMmap` columns (`node_type_at`, `node_file_path_str`), read source text once per `FileWorkGroup` on the Rayon worker, materialize only that file's `Node`s, and drop the file `String`, `Tree`, and `Node`s before starting the next file.
- **Skip bodiless declarations early:** Functions marked `callable_role == "declaration"` (C prototypes, Java interface/abstract methods, TypeScript ambient signatures, Rust trait method declarations without bodies) or returning `"Function has no body"` must record `CfgSkipReason::BodilessDeclaration` — **not** `analysis_error` — and pure prototype/interface files must skip disk reads and tree-sitter parsing altogether. Ensure `try_full_incremental_shortcut` ignores bodiless declarations so warm `--with-cfg` runs still hit the fast shortcut.
- **Single-encode & bounded channel persistence:** Serialize `ControlFlowGraph` and `ProgramDependenceGraph` **once** per function (verify byte-for-byte identity against `FunctionAnalysis` and `CfgPdgRecord` in unit tests), compute `stable_record_uuid` on the worker (never `bincode::deserialize` payloads on the writer just to read header fields), and stream batches over a bounded `std::sync::mpsc::sync_channel` directly into `AnalysisStorage` and `StreamingCfgPdgArchiveWriter`.
- **Single directory walk in `--full`:** Thread `discovered_files: Arc<[PathBuf]>` from Stage 1 through Stage 2 (deep) and Stage 3 (`semantic_index`) instead of walking the filesystem three times.

---

## 3. Bounded AST & CFG Recursion

- **Iterative tree-sitter walks:** Tree-sitter CSTs on generated or DSL-heavy code (`example/discourse`, `example/llvm-project`) routinely exceed 2,000 levels of left-recursive call chains or `if/elsif` ladders. Use explicit heap stacks (`Vec<(Node, ...)>`) capped at `MAX_AST_WALK_DEPTH = 2048`.
- **No `O(D × N)` nested-function re-walks:** When collecting per-function locals across a file (`field_write_locals`), `continue` past child `reset_kinds` function nodes while inside an enclosing function because the outer file walk visits each nested function independently.
- **CFG recursion guards:** Lower `if / elsif / else if / elif` chains iteratively in `CfgBuilder::visit_if` and guard nested statement/logical-condition recursion at `MAX_CFG_STATEMENT_DEPTH = 1024` (emit a one-time `tracing::warn!` per function and a flat `StatementKind::Expression` / `StatementKind::Branch` fallback rather than overflowing worker stacks). Keep deep-AST regression tests running on a constrained **2 MiB** thread stack.

---

## 4. Query, CLI & Documentation Correctness

- **Path filter consistency:** Repo-relative `--file <dir/file.ext>` filters must match on `/` path-segment boundaries (`norm_path == pat || norm_path.ends_with(&format!("/{pat}"))`) identically across `find`, `callers`, `callees`, `relations`, and `blast-radius`.
- **CLI flag parity across output formats:** Flags like `--iterations`, `--limit`, `--file`, and `--class` must behave consistently in both human output and `-f json` (`rgctl-service`).
- **Pre-index sidecars once per command:** Multi-advisory or multi-seed commands (`vuln scan`, `review`, `check`) must open `ColumnarGraphSnapshot` and `parked_relations.bin` once (`SharedVulnGraphResources`) and pre-index lookup keys (`FxHashMap`) rather than linearly scanning per item.
- **Verified release notes & docs:** Every CLI command in `docs/releases/*.md` and `skills/` must use valid current flags (e.g. positional pattern `rgctl -f json find --type function <name>`, never non-existent `--name`) and accurate cache/semantics version numbers (`facts_cache::FORMAT_VERSION`, `SEMANTICS_VERSION`). Never introduce `rgctl gql` in new tests or docs.

---

## 5. `[profile]` Stage Accounting & Cold Profile Gates

1. **Zero dark-matter wall time:** Every top-level phase in `discover` (`file_discover`, `detect_changes`, `index_extract`, `index_graph_build`, `parked_save`, `cold_open`, `topology`, `community`, `community_labels`, `complexity`, `centrality`, `dependency`, `security`, `cfg_total`, `cfg_archive`, `field_write`, `blast_build`, `blast_snapshot`, `macro_index`, `structural_sketch`, `save_analysis`, `save_tracker`, `save_dashboard`, `migration_plan`, and `--full` `semantic_index`) must be non-overlapping and sum to within `< 0.5 s` of `wall_secs` (`cargo test --test profile_stage_accounting`).
2. **Cold profile verification (mandatory when touching extraction, graph build, CFG, or analytics):**
   - Build release first: `cargo build --release --bin rgctl`
   - Delete `.rgctl/` before **every** measurement (`rm -rf example/<corpus>/.rgctl`) and `cd` into the corpus.
   - Verify against reference M3 Pro baselines (`tests/cold_profile_gates.rs` / [profile.md](profile.md)):
     - `example/linux` default: **`<= 48.0 s`** (`~40.9 s` measured, `< 9.5 GB` RSS)
     - `example/kafka` default: **`<= 9.5 s`** (`~8.7 s` measured)
     - `example/metasfresh-4.9.8b --full`: **`<= 38.0 s`** process elapsed (`~32.3–35.7 s` measured)
     - `example/discourse -l ruby --with-cfg`: **`<= 7.0 s`** (`~4.6 s` measured, zero stack overflow)
     - `example/linux --with-cfg`: zero swap (`0 MB`), `< 3 GiB` RSS delta over ingest.

---

## 6. Pre-Flight Gate Before Concluding Any Task or PR

Always run and pass:

```bash
./scripts/preflight-pr.sh
```

And if `skills/` changed in a local dogfood checkout where `.agents/skills/` exists, sync `.agents/skills/` (`rgctl install --skill --tools agents`) so `rgctl-agent-pack-codegen` (`dogfood_agents_skills_match_skills_tree`) stays drift-free.
