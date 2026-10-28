# Command Encyclopedia

Detailed reference for all rgctl commands with full JSON samples and field specifications.

Samples below are truncated where noted. Field names match live CLI / `docs/json-api.md`. Fixture: `rgctl-tests/ecommerce-java` unless noted **illustrative** (schema-faithful shape).

## Canonical command map

Prefer the **Canonical** column in examples and agent workflows. Aliases/façades remain supported.

| Intent | Canonical | Aliases / façades |
|--------|-----------|-------------------|
| Index / cold start | `discover` (`--scip`, `--with-git`) | — |
| Multi-repo merge | `merge … --output-dir` | — |
| Session freshness | `status` → `update` | `serve --watch` / `watch` (+ queue) |
| Symbol lookup | `find` | `query find` |
| Callers / callees | `callers` / `callees` | `query …`, `cpg calls` |
| Typed edges | `relations` (`overrides`, `cochangeswith`, …) | `query relations` |
| Counts | `inventory` | `query inventory` |
| Line slice / flow | `slice` (lazy CFG; optional `--function`, `--with-alias`) | `cpg slice`, `cpg flows` |
| Raw CFG / PDG | `inspect` | `cpg pdg` |
| Impact | `blast-radius` | — |
| Hotspots / churn | `metrics` (`--pagerank`, `--churn`, …) | — |
| Exact / bloom / fragment clones | `clones` (`--mode fragment --seed`) | — (not `semantic query`) |
| OSV → OpenVEX | `vuln analyze` | `security vuln analyze` |
| Dep match | `deps check` | `security deps check` |
| Sink taint CLI | `taint` | `security taint` |
| CI policy | `check` | — |
| PR call-path review | `review paths` | — |
| PR policy gate | `review check` | `pr-check` (alias) |
| Composed review artifact | `review export` | — |
| Kantra rules | `rules run` | — |
| Experimental GQL | `gql` | prefer Query verbs above |

`rgctl --help` lists commands under visual groups: Lifecycle · Query · Analysis · Security · Policy · Meta (flat verbs unchanged: `rgctl discover`, not nested).

## Table of Contents

- [discover](#discover)
- [update](#update)
- [merge](#merge)
- [find / callers / callees / relations / inventory](#find--callers--callees--relations--inventory)
- [blast-radius](#blast-radius)
- [slice](#slice)
- [inspect](#inspect)
- [metrics](#metrics)
- [semantic](#semantic)
- [communities](#communities)
- [clones](#clones)
- [cpg](#cpg)
- [check](#check)
- [review](#review)
- [export](#export)
- [serve](#serve)

---

## discover

**Command:** `rgctl [-f json] discover [PATH] [-l/--languages CSV] [-e/--exclude GLOB] [-v/--verbose] [--scip PATH.scip] [--with-git] [--git-since DUR] [--with-cfg] [--with-security] [--with-taint] [--with-universe] [--with-harmonic] [--export-migration-hints] [--migration-preset NAME] [--migration-order NAME] [--with-ast-skeleton] [--with-dfg-loops] [--write-json-graph] …`

**Purpose:** Index the repo once (or after large changes). Writes the default **`RGCL` v2** columnar mmap snapshot (`.rgctl/graph.snapshot.bin`) that agents query.

**Prerequisites:** None (this creates `.rgctl/`).

**Repo targeting:** `rgctl -r /repo discover .` and `rgctl -r /repo discover` resolve to `/repo` (positional `.` does not override `-r` with process cwd). Relative PATH joins `-r`.

**Other flags:** `--languages java,go` restricts the language set; `--exclude` filters paths (glob); `--verbose` prints per-file progress (noisy — skip unless debugging a stuck/slow discover); `--with-kantra` writes Kantra findings; `--write-json-graph` also writes legacy `graph.db`/`graph.json` (rarely needed — snapshot-only is the default).

**SCIP overlay:** `--scip <PATH.scip>` applies a compiler-exact `.scip` protobuf index (`apply_scip_overlay`, `resolution = "scip"`) to upgrade ambiguous call edges.

**Git churn / co-change:** `--with-git` (optional `--git-since 90.days`) streams `git log --numstat`, writes `.rgctl/analysis/git_churn.json`, and emits `EdgeType::CoChangesWith` for file pairs with Jaccard $J \ge 0.25$. Query later via `metrics --churn` and `relations --edge cochangeswith`.

**CFG/PDG:** `--with-cfg` pre-builds the whole-repo v3 `cfg_pdg.bin` archive (needed for `taint --sink`, `cpg mutations`, batch `inspect`). Single-function `rgctl slice` / `cpg flows` **lazily** build and append one function's CFG/PDG without requiring `--with-cfg` upfront.

**Migration flags:** `--export-migration-hints` writes `.rgctl/migration_plan.json` **and**, with `-f json`, emits the migration plan JSON on stdout (`preset`, `order_mode`, `steps[]`, `schema_version: 2`). Plain `discover -f json` without the flag emits DiscoverResponse telemetry (`command: "discover"`, `metrics`). Pair with `--with-harmonic` for ranking. `--migration-preset` / `--migration-order` control strategy and step sort.

**Examples:**
```bash
rgctl discover .
rgctl discover . --scip index.scip
rgctl discover . --with-git --git-since 90.days
rgctl discover . --with-cfg --with-taint   # whole-repo CFG archive (taint / mutations)
```

**Sample** (`-f json`, ecommerce-java):

```json
{
  "schema_version": 2,
  "command": "discover",
  "metrics": {
    "files_discovered": 66,
    "files_indexed": 66,
    "files_skipped": 0,
    "nodes_generated": 843,
    "edges_generated": 1793,
    "duration_ms": 306
  }
}
```

**Pitfalls:** Do not re-run full discover on every question if `.rgctl/` exists — use `status` + `update` when sources are dirty. Prefer lazy `slice` over rediscovering with `--with-cfg` for one-off line slices; reserve `--with-cfg` for `taint --sink`, `cpg mutations`, and batch `inspect`. `--with-taint` is discover-time taint (on-demand: `slice --taint`). Semantic search needs a separate `semantic index`. Migration roadmaps need `--export-migration-hints` (plan is on stdout with `-f json` and in `migration_plan.json`).

**Agent should report:** files indexed, nodes/edges, duration; note which feature flags were used (`--scip`, `--with-git`, `--with-cfg`); for migration, path to `migration_plan.json` + top steps.

---

## update

**Command:** `rgctl [-f json] update [PATH] [--files PATH,...] [--since REF] [--cascade-depth N] [--force] [--no-wait] [--wait-timeout SECS] [-l/--languages CSV] [-e/--exclude GLOB]`

**Purpose:** Incremental **structural** graph patch for changed sources. Not a full discover — does not rebuild communities, Kantra, CFG/PDG, or migration plans. Prefer this (or `serve --watch`) over rediscover when the working tree drifts.

**Prerequisites:** Existing `.rgctl/` snapshot (`rgctl discover` once).

**Examples:**
```bash
rgctl update                         # hash-diff vs file_hashes.json
rgctl update --files src/Foo.java    # explicit paths
rgctl update --since HEAD~1          # git-changed files
rgctl -f json update                 # schema_version + UpdateResult fields
rgctl update --no-wait               # enqueue only when serve --watch is live
```

**Compatible alias:** `rgctl discover --files path1,path2` (same incremental updater / queue handoff).

**Watch handoff:** While `serve --watch` holds `.rgctl/watch.lock`, `update` **enqueues** for the watcher (sole writer) and waits by default (`source: "watch_queue"` in JSON). Do **not** stop serve to refresh. `--force` is rejected under a live watcher. Second `serve --watch` still exits.

**Pitfalls:** After update, analysis sidecars keyed on graph digest may be invalidated — re-run `discover --with-cfg` / `semantic index` only when those features are needed. Empty change set exits 0 with “already current”.

**Agent should report:** files_affected, nodes/edges deltas; if zero, say index already current; if queued via watch, note `source: watch_queue`.

---

## merge

**Command:** `rgctl merge <REPOS>... --output-dir <OUTPUT_DIR>`

**Purpose:** Multi-repo **park-and-resolve** merge. Combines $N$ discovered repo snapshots into one columnar graph under `--output-dir`, resolving cross-repo calls/imports/routes parked in each repo's `.rgctl/parked_relations.bin` (`RBPK`) via 4-tier matching (`ExactFqn` → `ImportScopedSuffix` → `UniqueBareName` → `AmbiguousStub`) and rewiring external stubs to real target UUIDs.

**Prerequisites:** Each input path must already have been discovered (`rgctl discover <REPO>`).

**Examples:**
```bash
rgctl discover services/auth
rgctl discover services/billing
rgctl merge services/auth services/billing --output-dir .rgctl-merged
rgctl -r .rgctl-merged -f json callers validateToken
```

**Pitfalls:** `--output-dir` is required. Merged artifacts live under the output dir's `.rgctl/` — use `-r <OUTPUT_DIR>` (or `cd` there) for subsequent queries. Re-run discover on inputs after large source changes before merging again.

**Agent should report:** output dir, that parked relations were resolved, and a sample cross-repo `callers` / `relations` check.

---

## migration_plan (on-disk)

**Path:** `.rgctl/migration_plan.json` (after `discover --export-migration-hints`)

**Purpose:** Community/centrality-based migration roadmap for agents (packages, steps, ordering).

**Prerequisites:** `discover … --export-migration-hints` (typically with `--with-harmonic` for ranking).

**Agent should report:** preset/order, top packages/steps by priority — summarize, do not paste the full plan JSON into context.

**See:** Migration Planning Guide (product docs; not bundled in this skill pack), migrate workflow in [workflows.md](workflows.md)

---

## find / callers / callees / relations / inventory

**Commands:**

```bash
rgctl -f json find [PATTERN] --type function --scope pkg --limit 50
rgctl -f json find --type function --count-only
rgctl -f json find --annotation @MessageDriven --type class
rgctl -f json find --annotation @Stateful,@Stateless,@Singleton --type class
rgctl -f json find '*MDB*' --type class --limit 50          # bare-name suffix scan
rgctl -f json callers <SYMBOL> --depth 1 --file PATH --class NAME --line N
rgctl -f json callees <SYMBOL> --depth 1
rgctl -f json relations [SYMBOL] --edge annotatedwith --from-type function --to-type annotation --scope pkg
rgctl -f json relations --edge extends --from-type class   # seedless
rgctl -f json relations <SYM> --edge overrides --direction both   # cross-language MRO
rgctl -f json relations "src/checkout.rs" --edge cochangeswith    # needs discover --with-git
rgctl -f json relations --edge exposesroute --limit 100           # framework routes
rgctl -f json relations --edge injects --limit 100                # DI wiring
rgctl -f json inventory --by type   # includes zero-count kinds
rgctl -f json inventory --by edge
rgctl -f json inventory --by import-prefix   # javax.ejb / javax.jms / org.eclipse …
rgctl -f json status                # snapshot + index_current / dirty_files
rgctl update                        # patch structural graph when status is dirty
rgctl discover . --find '*coolstore*'   # locate candidate project roots (no index)
rgctl -f json find --annotation @Resource --show-attributes  # needs annotation_args.json from discover
rgctl -f json query find …          # alias namespace
```

**Purpose:** Deterministic mmap structured query (no Cypher, no `MemoryBackend` hydrate). **Agents must use these verbs** — do not invent MATCH strings. Relations `total` is distinct `(source,target,edge)`; duplicates collapse with `occurrences` (`schema_version` ≥ 2). `inventory --by edge` uses the same rule: `count` = distinct, `occurrences` = raw stored edges.

**Notable edges:** `overrides` (cross-language MRO / polymorphic dispatch), `cochangeswith` (git temporal co-change from `discover --with-git`), `exposesroute` / `injects` (framework routes & DI from compiled `frameworks.bin`).

**Migration (agents):** primary path is `discover --export-migration-hints` → `.rgctl/migration_plan.json`. Optional supporting probes (keep `--limit` small):
1. `status` — is `.rgctl/` present and `index_current`?
2. If not current → `rgctl update` (not full discover)
3. `inventory --by import-prefix --limit 40` — import surface census
4. `find --annotation …` / suffix globs — blockers without package guess
5. `callers` / `blast-radius` on plan candidates

**Prerequisites:** `discover` done (columnar `graph.snapshot.bin`). `status` does not rediscover or update.

**Flags:** `--annotation` inverts `AnnotatedWith` (OR list; `@` optional). `--show-attributes` needs annotation-arg indexing (errors honestly until indexed). `--scope` + `--scope-mode inside|outside|crossing` (or `--exclude-scope`). `--file` / `--class` / `--line` disambiguate. Edge rows use keyed `source`/`target` (never positional). Omit `SYMBOL` on `relations` for set-wide typed-edge scans.

**Pitfalls:** Exact name is O(1) hash; prefix/contains/`--scope` may scan. Ambiguous symbols emit candidates (`error: ambiguous_symbol` JSON under `-f json`). Annotation argument values are not in the graph yet. Warm caches invalidate wall-time claims — label cold vs warm. Do not scrape stderr; parse `schema_version` on stdout.

**Agent should report:** counts, lean names/files, keyed edge pairs — not full node dumps.

**See:** OpenSpec `add-migration-search-primitives` (+ `add-structured-query-cli`).

---

## blast-radius

**Command:** `rgctl -f json blast-radius '<Symbol>' [--depth N] [--class C] [--file P] [--with-slices] [--policy-file PATH] [--no-policy]`

**Purpose:** Upstream change impact — who breaks if this symbol changes.

**Prerequisites:** `discover` done.

**Sample** (schema v2 shape; ecommerce names — field set matches live CLI):

```bash
rgctl -f json blast-radius 'checkout' --class OrderService --depth 3
```

```json
{
  "schema_version": 2,
  "target": {
    "id": "424d403b-1b2c-4a3d-8e9f-0c1b2a3f4e5d",
    "symbol": "checkout",
    "class_context": "OrderService",
    "file_path": "…/service/OrderService.java",
    "language": "java",
    "signature": "public OrderDto checkout() {",
    "canonical_fqn": "OrderService::checkout"
  },
  "metrics": {
    "score": 25.05,
    "direct_callers_count": 1,
    "impact_zone_size": 3,
    "caller_depth_limit": 3
  },
  "topology": {
    "scc_component_id": null,
    "direct_callers": [
      {
        "id": "8b2c4a3d-0c1b-4e5d-8e9f-424d403b1b2c",
        "fqn": "OrderController.checkout",
        "file_path": "…/OrderController.java"
      }
    ],
    "impact_zone": [
      {
        "id": "…",
        "fqn": "…",
        "file_path": "…"
      }
    ]
  },
  "gatekeeping": { "policy_status": "SKIPPED", "violations": [], "handoffs": [] }
}
```

**Pitfalls:** Ambiguous names need `--class` / `--file`. `--with-slices` is slow. Exit `1` when policy `VIOLATED` (JSON still emitted first). `--no-policy` skips policy evaluation entirely (gatekeeping reports `SKIPPED`) — use for pure impact analysis when the user isn't asking about CI gates. **Interface / dynamic dispatch:** blast-radius and CALLS edges track static call sites only — receiver methods, virtual calls, and trait/interface impls may return score=0 / 0 callers even when widely used. If blast-radius returns 0 for a method that clearly has callers, fall back to `grep` for call sites in source.

**Agent should report:** score, direct callers (`fqn` / `file_path`), impact_zone_size, policy status — not full topology arrays. Ignore or pass through extra v2 fields (`id`, `language`, `signature`, `scc_component_id`) as needed.

---

## slice

**Command:** `rgctl -f json slice <FILE> --line N --variable V [--function METHOD] [--direction backward|forward] [--with-alias] [--taint] [--view text|cfg|pdg]`

**Purpose:** Line-level data dependence (what affects V / where V flows). `--taint` for source→sink security on the criterion. `--with-alias` expands may-alias names / field bases.

**Prerequisites:** Plain `discover` is enough. **No upfront `discover --with-cfg` required** — if `.rgctl/analysis/cfg_pdg.bin` lacks the target function, `slice` lazily parses `<FILE>`, builds that function's CFG/PDG, and atomically appends a `TocEntry`. When `--function` is omitted, the innermost enclosing function at `--line` is auto-resolved. If provided, `--function` is the **method/function name**, not the class.

**Sample** (ecommerce-java `CartService.addItem`):

```bash
# Lazy CFG: omit --function; optional --with-alias
rgctl -f json slice src/main/java/com/example/ecommerce/service/CartService.java \
  --line 38 --variable cart --direction backward --with-alias

rgctl -f json slice src/main/java/com/example/ecommerce/service/CartService.java \
  --line 38 --variable cart --function addItem --direction backward
```

```json
{
  "schema_version": 1,
  "file": "src/main/java/com/example/ecommerce/service/CartService.java",
  "direction": "backward",
  "criterion": { "line": 38, "variable": "cart" },
  "lines": [38],
  "reduction_percent": 92.86,
  "nodes": [
    {
      "id": "node_0",
      "kind": "Expression",
      "label": "Cart cart = getUserCart();",
      "line": 38
    }
  ],
  "edges": []
}
```

**Pitfalls:** Wrong `--function` (class vs method) is a common failure. Prefer omitting `--function` when `file:line` uniquely identifies the span. Whole-repo `taint --sink` still needs `discover --with-cfg`; single-criterion `slice --taint` does not.

**Agent should report:** criterion, direction, `nodes[].label` / lines, reduction — not the full edge list unless asked.

---

## inspect

**Command:** `rgctl -f json inspect <SYMBOL> cfg [--prune] | pdg [--edge-layer all|data|control] [--def-use] | dom [--frontiers]`

**Purpose:** Raw CFG / PDG / dominator view for one function.

**Prerequisites:** Prefer `discover --with-cfg` for a warm whole-repo archive (batch L_proc). Symbol only — **no** `--class` (disambiguate via `find` / `blast-radius` / `callers` first).

**Layer flags:** `cfg --prune` drops unreachable blocks before display. `pdg --edge-layer data|control` filters to one dependence type (default `all`); `--def-use` adds def-use variable lists per node. `dom --frontiers` prints dominance frontiers instead of just the tree.

**Sample:**

```json
{
  "schema_version": 1,
  "symbol": "addItem",
  "layer": "cfg",
  "pruned": false,
  "nodes": [
    {
      "id": "block_0",
      "block_index": 0,
      "start_line": 0,
      "end_line": 0,
      "statements": []
    },
    {
      "id": "block_1",
      "block_index": 1,
      "start_line": 24,
      "end_line": 24,
      "statements": [
        { "kind": "Return", "line": 24, "text": "return cartService.addItem(…);" }
      ]
    }
  ],
  "edges": [
    { "kind": "return", "source": "block_1", "target": "block_0" }
  ]
}
```

There are **no** `nodes_count` / `edges_count` fields — use `len(nodes)` / `len(edges)`.

**Pitfalls:** Ambiguous symbols fail; resolve FQN/name carefully. CFG may be **partial** on complex methods (covering only the first branch/entry block) — supplement with source reading if the block count seems low for the method's complexity.

**Agent should report:** layer, `len(nodes)` / `len(edges)`, notable `statements[].text` — not every node. Note if the CFG appears incomplete.

---

## metrics

**Command:** `rgctl -f json metrics [--pagerank] [--betweenness] [--communities] [--churn] [--include-tests] [--iterations N]`

**Purpose:** Hotspots (pull-based CSR `FastPageRank`), bridges (`SampledBetweenness`), Fast-Leiden community stats, and git churn (`--churn` after `discover --with-git`).

**Prerequisites:** `discover` done. `--churn` needs prior `discover --with-git` (reads `.rgctl/analysis/git_churn.json`). Default (no flags) computes centrality/community sections. PageRank filters non-production `NodeRole`s by default — pass `--include-tests` to include test/generated helpers.

**Examples:**
```bash
rgctl -f json metrics --pagerank --betweenness --communities
rgctl -f json metrics --churn
```

**Sample** (`--pagerank`; `top[]` includes resolved metadata — **one call**):

```json
{
  "schema_version": 1,
  "pagerank": {
    "top": [{
      "node": "<uuid>",
      "pagerank": 0.0117,
      "name": "processOrder",
      "qualified_name": "OrderService::processOrder",
      "kind": "function",
      "file_path": "src/OrderService.java"
    }],
    "converged": true,
    "iterations": 20,
    "max_delta": 0.0
  }
}
```

**Agent should report:** top hotspot symbols from `name` / `file_path` directly (no N+1 `cpg function` loop); for `--churn`, top files/functions by `git_hotspot_score`.

---

## semantic

**Command:**

```bash
rgctl semantic index [--embedder vocab|hash|onnx|code-daemon] [--embed-bodies] [--model PATH] [--tokenizer PATH] \
  [--dimensions N] [--incremental] [--diffuse] [--diffuse-alpha F] [--diffuse-iters N] [--diffuse-bidirectional]
rgctl semantic distill --matrix PATH [--embedder code-daemon|hash|onnx] [--tokens PATH] [--dimensions N]
rgctl -f json semantic query "…" [--limit N] [--scope function|community] \
  [--expand neighbors|blast|all] [--expand-depth N] [--no-fusion] [--candidate-pool N] [--keyword-and]
```

**Purpose:** Natural-language / keyword find of functions (and community-scoped search), with optional one-shot expansion into graph context.

**Prerequisites:** `discover`, then **`semantic index`** (separate artifact). Default **vocab** (no ONNX). `--embedder onnx` needs `--model` (+ optional `--tokenizer` for SentencePiece). `--embedder code-daemon` needs ONNX weights (`git lfs pull`). `--embed-bodies` re-reads function source (off by default).

**Index tuning:** `--dimensions` (default 256, multiple of 8) trades index size for precision. `--incremental` (default true) reuses embeddings for unchanged `code_hash`. `--diffuse` blends each embedding toward its call-graph neighbors' mean (Jacobi iterations via `--diffuse-alpha`/`--diffuse-iters`; `--diffuse-bidirectional` includes callers, not just callees) — useful when bare-name/docstring signal is weak and callers/callees disambiguate intent; `--no-diffuse` forces it off.

**Query expansion:** `--expand neighbors` pulls CALLS neighbors of top hits, `--expand blast` runs blast-radius on top hits, `--expand all` combines those — use when the user's NL query implies "and show me what's connected," so you skip a manual follow-up call. `--expand-depth` controls hop depth for `neighbors` expansion (default 1). Prefer follow-up `callers` / `blast-radius` over any Cypher expand mode. `--no-fusion` returns pure Hamming top-k (skip late-fusion re-ranking — rarely needed). `--candidate-pool` widens/narrows the pre-fusion candidate set (default 256). `--keyword-and` requires all query keywords to match entry metadata (stricter than default OR).

**Sample** (default vocab, query `checkout cart`):

```json
{
  "schema_version": 3,
  "query": "checkout cart",
  "model_id": "vocab-accumulate-v1",
  "dimensions": 256,
  "index_schema_version": 1,
  "hits": [
    {
      "name": "getCart",
      "qualified_name": "CartController.getCart",
      "node_id": "94823a58-9efd-4de4-95fb-aa082c2012c3",
      "score": 0.50,
      "fused_score": 0.50,
      "distance": 40,
      "ranking": "fusion",
      "file_path": "…/CartController.java"
    }
  ]
}
```

**Pitfalls:** Query without index fails. Restart `serve` after rebuilding index for dashboard search. **Large repos (100K+ nodes):** `--scope community` can still be noisy when many small communities remain after Fast-Leiden + hub stripping. For subsystem ownership on large repos, prefer `communities list` + grep labels over `--scope community`.

**Agent should report:** top hit names, files, scores (`score` / `fused_score`); keep `node_id` for follow-up `callers` / `blast-radius` — not every hit.

---

## communities

**Command:** `rgctl -f json communities list` | `rgctl communities label [--write]`

**Purpose:** Named community overlay (subsystems) from **Fast-Leiden** (local moving + BFS connected-subgraph refinement + coarsening) with $\mu + 4\sigma$ `HubStripPolicy` isolating ubiquitous utilities into `"Shared / Infrastructure"` (`infrastructure_community_id`). `label` recomputes heuristic labels (e.g. after renames shift what a cluster "is about") and persists them into `analysis_results.bin` (`--write` defaults to `true`; response's `written` field confirms). Migration macro nodes use `community_id` (schema v3; older `louvain_community_id` still accepted on read).

**Prerequisites:** `discover` (community detection during analysis).

**Sample:**

```json
{
  "schema_version": 1,
  "modularity": 0.45,
  "written": false,
  "communities": [
    { "id": 462, "label": "ecommerce.service::findByEmail", "member_count": 19 }
  ]
}
```

**Agent should report:** top labels + sizes; use `inventory --by community` for census; explore ownership via `semantic query --scope community` / `blast-radius` (responses may include `community_id`).

---

## clones

**Command:**

```bash
rgctl -f json clones --mode exact [--min-loc N] [--exclude GLOB] [--lang ID] [--no-write] [--no-cache]
rgctl -f json clones --mode bloom [--threshold 0.85] [--min-loc N] [--exclude GLOB]
rgctl -f json clones --mode fragment [--seed SYMBOL|FILE:LINES] [--lines START-END] [--min-statements N] [--max-statements N] [--threshold F]
rgctl -f json clones SYMBOL --file PATH [--class C] [--line N]
```

**Purpose:** **Clone groups** — `exact` = same `code_hash` (Type-1); `bloom` = high `token_bloom` Jaccard **candidates** (`candidates: true`, not Type-1); `fragment` = Type-3/4 two-stage sub-function detector (Stage 1 bitwise Bloom popcount pre-filter + Stage 2 SESE hammock decomposition, $3 \le |S| \le 15$, with 2-iteration 1-WL canonical color refinement). Seed with `--seed <SYM_OR_FILE:LINES>` (optional `--lines START-END`). Answers “where else is this implementation or code snippet?” as pairs/groups. **Not** an alias of `semantic query` (NL/embedding nearest neighbors).

**Prerequisites:** `discover` (Function `code_hash` / `token_bloom`). Fragment mode does **not** require whole-repo `discover --with-cfg`; CFGs load from `.rgctl/analysis/cfg_pdg.bin` when present, otherwise on-demand tree-sitter parse of Stage-1 candidates. Sidecars `.rgctl/clones.json` / `.rgctl/clones.bloom.json` / `.rgctl/clones.fragment.json` (invalidated by `graph_digest`). Does **not** write clone edges into `graph.snapshot.bin`.

**Sample** (fixture `rgctl-tests/clone-exact`):

```json
{
  "schema_version": 1,
  "mode": "exact",
  "graph_digest": "<blake3>",
  "filters": { "min_loc": 5, "exclude": ["test"] },
  "group_count": 1,
  "groups": [
    {
      "mode": "exact",
      "hash": "9eec51b8…",
      "size": 2,
      "confidence": 1.0,
      "members": [
        { "id": "…", "name": "normalizePayload", "file": "…/CloneA.java", "start_line": 5, "loc": 17 },
        { "id": "…", "name": "normalizePayload", "file": "…/CloneB.java", "start_line": 5, "loc": 17 }
      ]
    }
  ]
}
```

**Fragment Sample** (`--mode fragment` emits `schema_version: 2`):

```json
{
  "schema_version": 2,
  "mode": "fragment",
  "graph_digest": "<blake3>",
  "filters": { "min_statements": 3, "max_statements": 15, "threshold": 1.0, "exclude": [] },
  "seed": {
    "file": "src/orders.rs",
    "start_line": 24,
    "end_line": 32,
    "enclosing_function": "process_orders",
    "structural_hash": "f9cbad17…"
  },
  "group_count": 1,
  "groups": [
    {
      "structural_hash": "f9cbad17…",
      "size": 2,
      "score": 1.0,
      "members": [
        { "id": "…", "name": "process_orders", "file": "src/orders.rs", "start_line": 24, "end_line": 32, "enclosing_function": "process_orders", "statement_count": 5 },
        { "id": "…", "name": "audit_orders", "file": "src/audit.rs", "start_line": 40, "end_line": 48, "enclosing_function": "audit_orders", "statement_count": 5 }
      ]
    }
  ]
}
```

**Pitfalls:** Bare `--exclude test` matches a path **component** named `test` (not substring of `rgctl-tests`). Ambiguous symbols need `--file` / `--class` / `--line`. Bloom is noisy — treat as candidates; prefer `exact` for Type-1 or `fragment` for sub-function logic. Default `min_loc` is 5; bloom default `--threshold` is 0.85; fragment defaults are `min_statements = 3`, `max_statements = 15`.

**Agent should report:** group sizes, member names/files, start/end lines, enclosing functions; for fragment mode, highlight duplicated snippet coordinates and structural hash. Do not conflate with `semantic query` hits.

**See:** clone-detection-design.md (product docs; not bundled in this skill pack), Clone Detection Guide (product docs; not bundled in this skill pack), json-api §16b

---

## cpg

**Command:** `rgctl -f json cpg <subcommand> …`

**Purpose:** Hybrid CPG façade (repo topology + CFG/PDG archive).

**Prerequisites:** `discover`. `cpg flows` / `cpg slice` share lazy single-function CFG/PDG with `rgctl slice` (no upfront `--with-cfg`). Prefer whole-repo `discover --with-cfg` for `cpg mutations`, batch `cpg pdg` / `inspect`, and `taint --sink`. `--with-ast-skeleton` for `ast`.

### cpg status

```bash
rgctl -f json cpg status
```

**Purpose:** Is the L_proc / CFG–PDG archive ready (whole-repo or partially lazy-filled)?

**Agent should report:** ready/not ready; for mutations/taint, whether to re-run `discover --with-cfg`.

### cpg function / cpg calls

```bash
rgctl -f json cpg function '<Symbol>'
rgctl -f json cpg calls '<Symbol>'
```

**Purpose:** Resolve a function in L_repo and whether L_proc exists; CALL neighborhood.

**Agent should report:** resolved identity + direct call neighbors.

### cpg pdg / cpg slice / cpg flows

```bash
rgctl -f json cpg pdg '<Symbol>' [--edge-layer all|data|control] [--def-use]
rgctl -f json cpg slice …   # wraps slice; see slice flags
rgctl -f json cpg flows FILE --line N --variable V [--function F] \
  [--direction forward|backward] [--with-alias]
```

**Purpose:** Dependence / data-flow overlays (prefer these when already in a CPG workflow). `--function` is optional — auto-resolves innermost enclosing function from `file:line` when omitted (same as `slice`).

**Pitfalls:** `cpg flows` / `slice` append lazily; only whole-repo archive gaps for `mutations` / sink-first `taint` need re-discover `--with-cfg`. `--with-alias` expands may-alias names. `--line` must point to a line **inside a function body** — struct definitions, import blocks, or other non-function lines will fail or return empty results.

**Agent should report:** key dependent statements / flow direction — not full graphs.

### cpg mutations

```bash
rgctl -f json cpg mutations --type ShoppingCart [--exclude-ctors] [--member fieldName] [--include-unresolved]
```

**Purpose:** Field mutations on a type (cart / DTO safety).

**Prerequisites:** `discover --with-cfg` (needs `field_write.bin` / whole-repo archive).

**Other flags:** `--member` narrows to one field (e.g. "who writes `items`?" instead of the whole type). `--include-unresolved` also reports writes whose receiver type couldn't be statically resolved (dynamic dispatch) — noisier but catches mutations `blast-radius`/CALLS would miss.

**Agent should report:** which fields are written, by which functions.

### cpg ast

```bash
rgctl -f json cpg ast '<Symbol>'
```

**Purpose:** Coarse AST skeleton for a function.

**Prerequisites:** `discover --with-ast-skeleton`.

**Agent should report:** skeleton summary / notable nodes.

### cpg export

```bash
rgctl cpg export --format graphson --output cpg.json [--path-contains src/] \
  [--include-l-proc] [--include-field-writes]
```

**Purpose:** Export hybrid CPG view (GraphML / GraphSON).

**Other flags:** `--include-l-proc` and `--include-field-writes` both default to **on** (merging PDG DATA_FLOW edges and mutation-index field-write sites respectively) — there's no CLI switch to turn them off; they're primarily documentation of what a plain `cpg export` already includes.

**Agent should report:** output path + format.

**General cpg pitfalls:** Archive IO errors mean re-run `discover --with-cfg` (and ensure write permissions under `.rgctl/analysis/`).

---

## check

**Command:** `rgctl -f json check --policy-file policy.json`

**Purpose:** CI gate — fail when blast-radius policy rules are violated.

**Prerequisites:** `discover`; valid policy file (`docs/policy-format.md`).

**Sample:**

```json
{
  "schema_version": 1,
  "passed": true,
  "policy": "rgctl-tests/rgctl-policy.json",
  "violations": []
}
```

**Pitfalls:** Exit code `1` on failure — still parse JSON for violations.

**Agent should report:** passed/failed + violation summaries.

---

## review

**Family:** temporal PR analysis under `rgctl review …`.

### review paths

**Command:** `rgctl -f json review paths [--base-ref REF] [--head-ref REF] [--full-snapshots] [--upstream-depth N] [--downstream-depth N] [--symbol NAME]`

**Purpose:** Before/after call-path report for changed symbols (spines + `path_delta`). Not a policy gate.

**Prerequisites:** Base/head graph snapshots (same prep as `review check` / `pr-check`).

**Sample (shape):**

```json
{
  "schema_version": 1,
  "command": "review paths",
  "change_summary": {
    "changed_symbols": 1,
    "call_edges": { "added": 0, "removed": 0, "retargeted": 1, "unchanged": 0 },
    "files_in_scope": 1,
    "unscored_files": 0
  },
  "truncation": { "symbols": false, "fanout": false, "depth": false },
  "symbols": [
    {
      "name": "submitOrder",
      "path_before": ["CheckoutController.handle", "submitOrder", "chargeCard"],
      "path_after": ["CheckoutController.handle", "submitOrder", "authorizeThenCapture"],
      "path_delta": [
        {
          "kind": "retargeted",
          "from": "submitOrder",
          "to_before": "chargeCard",
          "to_after": "authorizeThenCapture"
        }
      ]
    }
  ],
  "unscored_files": [],
  "ambiguous": []
}
```

**Pitfalls:** Truncation flags do not fail the command. Ambiguous `--symbol` lists `ambiguous` and exits non-zero. Do not dump raw JSON to the user; present spines then summary then unscored.

**Agent should report:** representative before/after paths + delta kinds; note truncation / unscored files.

### review check

**Command:** `rgctl -f json review check --policy-file PATH [--base-ref REF] [--head-ref REF] …`

**Purpose:** Temporal PR policy gate (new/existing/resolved/regression). Prefer this name; **`pr-check`** is a compatibility alias with identical JSON and exit codes.

**See:** CI Policy Checks (product docs; not bundled in this skill pack), json-api § pr-check / review check (product docs; not bundled in this skill pack).

### review export

**Command:** `rgctl review export [--base-ref REF] [--head-ref REF] [--policy-file PATH] [--stdout] [--full-snapshots]`

**Purpose:** Compose `review paths` + optional `review check` + the violation timeline into **one** versioned artifact at `.rgctl/workflows/review.json`, so a review needs a single fetch instead of three commands. Use `--stdout` to pipe it.

**Prerequisites:** Base/head graph snapshots (same prep as `review paths`).

**Pitfalls (critical):**
- `policy` is **`null` when not evaluated** (no `--policy-file`), which is *not* the same as "no violations". Never report a null `policy` as a clean result.
- `review export` is **read-only**: unlike `review check`, it does **not** write `.rgctl/violation_ledger.jsonl`. Persisting observations stays `review check`'s job.
- Output target precedence: `-o PATH` → `--stdout` → `.rgctl/workflows/review.json`.

**Agent should report:** the artifact path, `review_paths.change_summary`, whether `policy` was evaluated, and any `provenance.unavailable` reasons.

**See:** json-api § review export (product docs; not bundled in this skill pack).

---

## export

**Command:** `rgctl export --export-format mermaid|graphviz|… --export-output OUT [--query FILTER]`

**Purpose:** Export graph / neighborhood diagrams.

**Prerequisites:** `discover` done.

**Pitfalls (critical):** `--query` uses **filter** syntax — `name:Foo`, `type:Function`, `all` — **not** Cypher `MATCH … RETURN`. Agents must not pass MATCH strings to `--query`.

**Agent should report:** output path + format; confirm filter used.

---

## serve

**Command:** `rgctl serve [--open] [--host H] [--port N] [--dashboard-dir DIR] [--query-only]`

**Purpose:** Local HTTP dashboard + `POST /api/query` (and semantic routes) for **one repository**. Serve-only — does **not** run discover; index first with `rgctl discover . --with-universe`.

**Prerequisites:** `discover` (universe dashboard bundle with `--with-universe` for full UI).

**Pitfalls:** Foreground only — binds until Ctrl+C. Not a multi-repo catalog. Agents usually prefer CLI `-f json` subprocesses over HTTP.

**Agent should report:** URL/port for HTTP; confirm `discover --with-universe` (or equivalent) already produced `.rgctl/dashboard/`.

---

## diff

**Command:** `rgctl -f json diff --base <REF_OR_ARTIFACT> --head <REF_OR_ARTIFACT> …`

**Purpose:** Compare two graph snapshots / refs (structural delta for agents and review workflows).

**Prerequisites:** Discoverable trees or prepared `.rgctl` artifacts for base/head.

**Agent should report:** high-level added/removed/changed symbol counts; avoid dumping full edge lists.

---

## rules / kantra

**Command:** `rgctl discover . --with-kantra` then `rgctl rules …` (list / query Konveyor-style findings)

**Purpose:** Run Kantra/rules analysis during discover; query findings afterward. Structural policy workflows that mention `kantra` should use this path (not ad-hoc scrapes).

**Prerequisites:** `discover --with-kantra` writes findings under `.rgctl/`.

**Agent should report:** findings path + top rule hits; pair with `migration_plan.json` when migrating.

---

## See Also

- JSON API Reference (product docs; not bundled in this skill pack) - Complete field specifications
- Agent Recipes (product docs; not bundled in this skill pack) - Copy-paste command examples
- User Guide (product docs; not bundled in this skill pack) - Full CLI reference
