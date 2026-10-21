# Unreleased (post v0.4.19)

<!-- Add bullets here during development; move to docs/releases/v0.4.x.md at tag time. -->

## Agent & human CLI UX (`agent-and-human-cli-ux`)

Agents and interactive users share a cleaner CLI surface without changing discover hot paths:

- **`--repo` + PATH:** `rgctl -r <REPO> discover .` (and `update` / `serve`) resolve against `-r`, not process cwd; relative PATH joins `-r`.
- **JSON errors:** `-f json` / `RGCTL_FORMAT=json` emit a stdout envelope (`ok: false`, `error_code`, optional `candidates` / `suggestions` / `recovery_command`); quiet stderr banners unless `-v`.
- **Symbol targeting:** `--file` / `--class` / `--line` on `inspect`, `cpg function|calls|ast|pdg`, and `blast-radius`; query `--package` / `--methods` parity; `slice` / `cpg flows` accept `Class::method` / `Class.method`.
- **Status capabilities:** `status` reports structural / CFG-CPG / semantic (inferred scope) / agent-skills readiness with unlock commands.
- **Human TTY:** structured query rows show `kind  name  file:line` when stdout is a terminal; piped output stays 1-column. `inspect` and `metrics` print readable summaries (not count-only / `Debug`).
- **Metrics JSON:** `pagerank` / `betweenness` `top[]` include `name`, `qualified_name`, `kind`, `file_path` (no N+1 `cpg function`).
- **Skills:** `skills/rgctl` self-contained (no broken `../../docs/` links); `diff` + `kantra`/`rules` coverage; `rgctl-review` Step 0 uses a temp worktree without `--with-cfg`; `.agents/skills/` synced with parity tests.

## Faster `--with-cfg` / `--full` discover (`accelerate-cfg-discover-pipeline`)

Cold discover with CFG/CPG is cheaper without changing CLI flags or on-disk archive magic:

- **Single-pass per file:** one tree-sitter parse builds CFG/PDG, local types, field writes, and optional AST skeletons (overload-safe span lookup; no archive re-open for field-write indexing).
- **Worker-side bincode:** Rayon workers serialize `FunctionAnalysis` / `CfgPdgRecord`, drop live CFG/PDG Arcs, and write buffered pack + `cfg_pdg.archive.bin` via pre-encoded merge.
- **`discover --full` Stage 2 reuse:** when Stage 1 artifacts match `graph_digest`, skip recomputing complexity / communities / centrality / blast SCC / cold file-tracker sort; run harmonic (if needed), CFG, and dashboard only.
- CFG/PDG internals: deterministic PDG node UUIDs, `FxHash` scratch maps, denser dataflow lookups, faster empty-block contraction.
- Reference cold walls (2026-10-21): node `--with-cfg` **~4.8 s** (was ~6.2 s); metasfresh `--full` **~25.7 s** (was ~30 s). Gate A (linux default) unchanged. Details: [profile.md](../internal/profile.md).

## CFG empty-block contraction ([#121](https://github.com/sshaaf/rgctl/issues/121))

- Lazy `if`/`unless` block allocation: reuse empty current blocks for conditions; wire `IfFalse` directly to merge when there is no `else`.
- Post-prune `ControlFlowGraph::contract_empty_blocks` removes non-exit empty `Next` trampolines.
- Restores PDG control dependencies across early-return guard clauses (ecommerce `correctnessRoot` `exact_control_deps` 1→2).

## Docs: language testing guide

- New contributor guide [language-testing-guide.md](../language-testing-guide.md): where to add language tests, Tier 1 matrix (langfeatures / CFG / taint / ecommerce expected-facts / dashboard / Gate B / stable-key QE), what to assert, and a copy-paste PR checklist.
- Linked from [contributor-checklist.md](../contributor-checklist.md), [tier-1-language-support.md](../tier-1-language-support.md), [docs/README.md](../README.md), and [AGENTS.md](../../AGENTS.md).

## Build / lockfile ([#123](https://github.com/sshaaf/rgctl/issues/123))

- Track `Cargo.lock` (removed from `.gitignore`) so fresh clones agree on dependency versions.
- Pin `ort` to `=2.0.0-rc.13` (rc.14 changed `Session::builder` to require `&Environment`).
- CI always runs `cargo check --locked --bin rgctl` on pull requests; full suite remains behind the `ci` label.

## Review / pr-check delta head ([#124](https://github.com/sshaaf/rgctl/issues/124))

Default `review paths` / `review check` / `pr-check` head synthesis is **non-destructive**:

- Synthesized head lands under `.rgctl/synth-head/` (not the live `graph.snapshot.bin`).
- Compaction drops base edges whose **source** file was invalidated (call deletions no longer resurrect / flip to additions-only).
- `--synthetic-head worktree` can use the live session snapshot as base without `.rgctl-base/`.
- `review export --policy-file` reuses the prepared base/head snapshots (no second synth / dropped `--base-artifact`).
- Stderr warning when `--head-ref` ≠ `HEAD` or scoped files are dirty in non-worktree delta mode.

## Discover graph determinism ([#125](https://github.com/sshaaf/rgctl/issues/125))

Cold `discover` edge identity is deterministic under parallel extract:

- Pass-2 applies extraction tails in **path-sorted** order (not Rayon completion order).
- `Calls` resolve only to `Function` nodes (never Class field stubs via fuzzy suffix).
- Bare / field-like `References` do not create Class external stubs; typed References (`throws`, `class_literal`, explicit type hints such as `puppetresource`) still stub. External stubs are not registered in `symbols_by_suffix`, and distinct `to_qualified_hint` values do not coalesce by simple name.
- Deterministic node IDs / `StableNodeKey` include **qualified name** when present (fixes per-owner `$lambda$0` collisions). **`start_line` is not hashed** (avoids cross-commit identity churn when a line is inserted above a no-QN symbol). Diff/review hot paths read QN via a zero-alloc columnar prefix helper (no full `NodeExtension` materialize per row).

**Re-discover required** after upgrade: node UUIDs and edge sets can change vs pre-fix indexes.

## Agent pack: `/rgctl:review` (OpenSpec dual)

- New skill **`rgctl-review`** + slash command **`/rgctl:review`**: local PR/branch review protocol (`review paths` / `review check`, optional blast, soft architecture-md) writing **`REVIEW_REPORT.md`** + four-section session verdict — **no** GitHub/GitLab posting.
- `rgctl install --skill` installs **skills and commands together** (codegen emits per-adapter command files from `agent-pack/manifest.yaml` `commands:`).
- Meta skill routes “review this PR” intents to `/rgctl:review`.

## Clone detection ([#37](https://github.com/sshaaf/rgctl/issues/37))

First-class `rgctl clones` — whole-function and sub-function duplicate detection, distinct from `semantic query`. Query-time / sidecars only (no default discover cost; Gate A unchanged). Does **not** write clone edges into `graph.snapshot.bin`.

```bash
rgctl discover .                      # exact / bloom
rgctl discover . --with-cfg           # faster fragment path (CFG archive)

rgctl -f json clones --mode exact --min-loc 5 --exclude test
rgctl -f json clones --mode bloom --threshold 0.85 --exclude test
rgctl -f json clones --mode fragment --seed src/Foo.java:40-52
rgctl -f json clones --mode fragment --seed PaymentService::processRefund --lines 45-55
```

### Modes

| Mode | Unit | Signal | Honesty |
|------|------|--------|---------|
| **`exact`** (default) | Whole function | Equal non-empty `code_hash` (Type-1) | Exact body match |
| **`bloom`** | Whole function | `token_bloom` Jaccard (LSH bands; default threshold **0.85**) | `candidates: true` — noisy near-dupes |
| **`fragment`** | Sub-function (3–15 stmts) | CFG **SESE** hammocks + **1-WL** structural hash | Schema **v2**; structural match (rename-tolerant kinds) |

Reserved (error): `semantic` / `structural` as clone modes.

### Exact & bloom

- Filters: `--min-loc` (default 5), `--exclude` (path **component** match — `test` ≠ `rgctl-tests`), `--lang`.
- Symbol-scoped: `rgctl clones SYMBOL [--file|--class|--line]` (same QE as callers).
- Sidecars: `.rgctl/clones.json`, `.rgctl/clones.bloom.json` (digest-keyed; `--no-write` / `--no-cache`).

### Fragment (SESE + 1-WL)

- Requires useful CFG: prefer `discover --with-cfg` (archive fast path); otherwise on-demand parse for bloom-pruned candidates.
- Seed: `--seed <SYMBOL|file:start-end>` and/or `--lines START-END`; bounds `--min-statements` / `--max-statements` (defaults 3 / 15).
- Two-stage retrieval: function `token_bloom` coarse filter → SESE + 1-WL on candidates.
- Sidecar: `.rgctl/clones.fragment.json`.
- Analysis crates: `sese`, `wl_hash`, `fragment_clones` (+ post-dominator helpers).

### Docs & agents

- Guide: [Clone detection](../guides/clone-detection.md)
- Design: [clone-detection-design.md](../design/clone-detection-design.md)
- JSON API §16b; skill encyclopedia + workflows recipes
- Fixture / tests: `rgctl-tests/clone-exact`, `tests/clone_exact_integration.rs`, `tests/clones_integration.rs`
