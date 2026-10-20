# Unreleased (post v0.4.19)

<!-- Add bullets here during development; move to docs/releases/v0.4.x.md at tag time. -->

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
