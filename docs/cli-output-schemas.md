# CLI output schemas

Reference for JSON (and related) output shapes emitted by rBuilder CLI commands.

**Global flag:** `-f json` / `--format json`

**Architecture:** Each JSON command has a typed serializer in `src/cli/*_output.rs`. Commands build domain results in workspace crates (`rbuilder-analysis`, `rbuilder-pipeline`, …) and serialize in the CLI layer only — see [Code_structure.md](Code_structure.md) §2 (CLI is thin).

---

## Conventions matrix

| Convention | blast-radius | discover | gql | metrics | check | slice | inspect |
|------------|:------------:|:--------:|:---:|:-------:|:-----:|:-----:|:-------:|
| `schema_version` | ✅ v2 | ✅ v2 | ✅ v1 | ✅ v1 | ✅ v1 | ✅ v1 | ✅ v1 |
| Typed `*_output.rs` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Explicit empty arrays | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ |
| Omitted optional keys | — | — | — | ✅ | ✅ | ✅ | ✅ |
| Composable graph topology | ✅ | — | — | — | — | ✅ | ✅ |
| Stable node UUIDs (no nil) | ✅ | — | — | — | — | — | — |

**Tests:** See [cli-io-sanity-audit.md](cli-io-sanity-audit.md) for the full coverage matrix, harness design, and extension guide.

| Layer | Cargo target | Path | Covers |
|-------|--------------|------|--------|
| 1 — Unit schema | `cli_output` | `tests/cli_output/*.rs` | Typed `*_output.rs` fixtures, serde shapes |
| 2 — Golden path | `subprocess_golden_path` | `tests/cli_output/subprocess_golden_path.rs` | Discover + blast-radius pipelines, exit 1 |
| 3 — Full sanity | `all_commands_sanity` | `tests/cli_output/all_commands_sanity.rs` | All JSON commands, sandbox `-d`, platform rules |
| Fixture | — | `tests/fixtures/tiny_polyglot_repo/` | Java + Rust polyglot subprocess input |

```bash
cargo test --test cli_output --test subprocess_golden_path --test all_commands_sanity
```

Related: [blast-radius-json-schema-v1.md](blast-radius-json-schema-v1.md) (v1 break), [blast-radius-json-schema-v2.md](blast-radius-json-schema-v2.md) (v2 target metadata).

---

## 1. `blast-radius` — schema v2

**Command:**

```bash
rbuilder -f json blast-radius <SYMBOL> [--depth N] [--policy-file PATH] [--with-slices] [--class CLASS] [--file PATH]
```

**Flags:**

| Flag | Description |
|------|-------------|
| `--depth N` | Cap `topology.impact_zone` to upstream callers within **N incoming call hops** (hop 1 = direct callers). Omits `metrics.caller_depth_limit` when unset (full closure). Score is recomputed when capped. |
| `--policy-file` | Run policy guardrails on the (possibly depth-filtered) impact zone |
| `--with-slices` | Populate `gatekeeping.handoffs` (requires full graph path) |
| `--class` / `--file` | Disambiguate overloads |

**Optional warm path:** start `rbuilder serve -r REPO` to keep graph + engine loaded; lite queries auto-connect to `.rbuilder/query.sock` unless `RBUILDER_NO_QUERY_DAEMON=1`.

**Source:** `src/cli/blast_radius_output.rs`  
**Cache enrichment:** `crates/rbuilder-analysis/src/macro_call_index.rs`, `macro_call_lookup.rs`

### Top-level

```json
{
  "schema_version": 2,
  "target": { },
  "metrics": { },
  "topology": { },
  "gatekeeping": { }
}
```

### `target` — identification metadata (v2)

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID string | Resolved graph node id |
| `symbol` | string | Bare function/method name |
| `class_context` | string \| null | Containing class or namespace when known |
| `file_path` | string | Project-relative source path (empty if unknown) |
| `language` | string | `"java"`, `"rust"`, `"python"`, or `"unknown"` |
| `signature` | string \| omitted | Method signature when known (overload disambiguation) |
| `canonical_fqn` | string | Uniform `Class::method` (e.g. `OrderService::process`) |

- `language` comes from graph `properties.language` (set by language plugins at extract time) or file-extension fallback.
- `signature` comes from graph `Node.signature` (tree-sitter during discover).
- `canonical_fqn` normalizes Java dot notation to double-colon form; Rust `module::fn` passes through.

### `metrics` — quantitative impact

| Field | Type | Description |
|-------|------|-------------|
| `score` | number | Impact score 0–100 |
| `direct_callers_count` | integer | Immediate caller count |
| `impact_zone_size` | integer | Transitive caller count (functions only); reflects `--depth` cap when set |
| `caller_depth_limit` | integer \| omitted | Present when `--depth N` was passed; echoes the hop cap applied to `impact_zone` |

### `topology` — graph layout

| Field | Type | Description |
|-------|------|-------------|
| `scc_component_id` | integer \| null | SCC index from engine; `null` on macro-index/SQLite fast path |
| `direct_callers` | `SymbolContext[]` | Immediate callers |
| `impact_zone` | `SymbolContext[]` | Transitive upstream callers (filtered by `--depth` when set) |

**`SymbolContext`:**

```json
{
  "id": "UUID",
  "fqn": "string",
  "file_path": "string"
}
```

- `fqn` is language-native display text from graph `qualified_name` or bare `name`.
- **Route on `target.canonical_fqn` + UUIDs**, not parsed `topology.fqn`.
- **Nil UUID policy:** entries without a resolvable graph UUID are **omitted** from `topology` (never `00000000-…`).
- After `discover`, SQLite/bin caches store `direct_caller_ids`, `impact_zone_ids`, and target metadata for composable chaining.

### `gatekeeping` — policy and slice tracing

| Field | Type | Description |
|-------|------|-------------|
| `policy_status` | string | `"SKIPPED"` (default), `"PASS"`, or `"VIOLATED"` |
| `violations` | array | Structured policy violations (always present; `[]` when none) |
| `handoffs` | array | Slice seeds (always present; `[]` without `--with-slices`) |

See v1 doc for `SliceHandoff` and `PolicyViolation` tag shapes.

### Example (Java, cache path)

```json
{
  "schema_version": 2,
  "target": {
    "id": "424d403b-1b2c-4a3d-8e9f-0c1b2a3f4e5d",
    "symbol": "process",
    "class_context": "OrderService",
    "file_path": "java/com/example/OrderService.java",
    "language": "java",
    "signature": "public void process(String orderId) {",
    "canonical_fqn": "OrderService::process"
  },
  "metrics": {
    "score": 25.05,
    "direct_callers_count": 1,
    "impact_zone_size": 3
  },
  "topology": {
    "scc_component_id": null,
    "direct_callers": [
      {
        "id": "8b2c4a3d-0c1b-4e5d-8e9f-424d403b1b2c",
        "fqn": "com.example.OrderController.checkout",
        "file_path": "java/com/example/OrderController.java"
      }
    ],
    "impact_zone": []
  },
  "gatekeeping": {
    "policy_status": "SKIPPED",
    "violations": [],
    "handoffs": []
  }
}
```

### Exit codes

- `0` — success
- `1` — `policy_status == "VIOLATED"` when `--policy-file` is set (JSON still emitted to stdout first)

---

## 1b. `serve` — query daemon (no JSON stdout)

**Command:**

```bash
rbuilder serve -r REPO [--socket PATH] [--idle-secs SECS]
```

**Defaults:** socket `{repo}/.rbuilder/query.sock`, idle exit 300s.

**Role:** Loads mmap graph + blast engine once; answers NDJSON RPC (`ping`, `blast_radius`) over a Unix socket. Lite `blast-radius` (no `--with-slices`, no `--policy-file`) auto-connects when the socket exists.

**Environment:** `RBUILDER_NO_QUERY_DAEMON=1` disables client auto-connect.

**Requires:** prior `discover` producing `graph.snapshot.bin` and `blast_engine.snapshot.bin`.

---

## 2. `discover` — schema v2 (stdout JSON)

**Command:**

```bash
rbuilder -f json discover PATH [--languages LANGS] [--exclude PATTERNS] [--security] [--cfg] [--all] [--write-json-graph]
```

**Source:** `src/cli/discover_output.rs`, `src/cli/discover_impl.rs`

With `-f json`, discover **suppresses** progress bars and human status lines on stderr (logging quiet unless `-v`). **Stdout** receives a single telemetry object after ingestion completes. Artifacts under `.rbuilder/` are still written.

```json
{
  "schema_version": 2,
  "command": "discover",
  "metrics": {
    "files_discovered": 10921,
    "files_indexed": 10784,
    "files_skipped": 137,
    "nodes_generated": 231410,
    "edges_generated": 562067,
    "duration_ms": 18200
  }
}
```

| Field | Source |
|-------|--------|
| `files_discovered` | `PipelineStats.files_discovered` |
| `files_indexed` | `PipelineStats.files_processed` |
| `files_skipped` | `PipelineStats.files_failed` |
| `nodes_generated` | `PipelineStats.nodes_created` |
| `edges_generated` | `PipelineStats.edges_created` |
| `duration_ms` | Full discover wall-clock (includes analysis + persist) |

Without `-f json`, discover remains human-readable text progress (unchanged).

### Artifacts on disk

| Path | When | Format |
|------|------|--------|
| `.rbuilder/graph.snapshot.bin` | Always (default canonical graph) | Binary graph snapshot |
| `.rbuilder/blast_engine.snapshot.bin` | Always | Binary blast engine snapshot |
| `.rbuilder/macro_call_index.db` | Always | SQLite blast-radius cache (+ UUID + v2 target columns) |
| `.rbuilder/macro_call_index.bin` | Always | Bincode macro index |
| `.rbuilder/analysis_results.bin` | Always | Columnar analysis tables |
| `.rbuilder/dashboard.html` | When export succeeds | HTML dashboard |
| `.rbuilder/graph.db` / `.rbuilder/graph.json` | `--write-json-graph` only | Legacy full graph JSON |
| `.rbuilder/analysis/cfg_pdg.archive.bin` | `--cfg` or `--all` | CFG + PDG for `--with-slices` (ICFG assembled from archived CFGs + live call graph) |
| `.rbuilder/analysis/all_analyses.json` | `--cfg` / `--all` (verbose path) | Per-function CFG/taint JSON export |

---

## 3. `gql` — schema v1

**Command:**

```bash
rbuilder -f json gql "<QUERY>" [--explain] [--macro NAME]
```

**Source:** `src/cli/gql_output.rs`

```json
{
  "schema_version": 1,
  "rows": [
    [
      {
        "binding": "string",
        "node": "string",
        "type": "string",
        "file": "string | null"
      }
    ]
  ],
  "count": 0,
  "explain": false
}
```

| Field | Type | Description |
|-------|------|-------------|
| `rows` | array | One element per result row; each row is an array of bindings |
| `count` | integer | Always equals `rows.length` |
| `explain` | boolean | Mirrors `--explain` flag |
| `binding` | string | Variable name from the `MATCH` pattern |
| `node` | string | Matched node bare name |
| `type` | string | Debug-formatted `NodeType` (e.g. `"Function"`) |
| `file` | string \| null | Source path when present on the node |

**Note:** The explain **plan** is not included in JSON; it prints to text mode only.

---

## 4. `metrics` — schema v1

**Command:**

```bash
rbuilder -f json metrics [--pagerank] [--betweenness] [--communities] [--iterations N]
```

**Source:** `src/cli/metrics_output.rs`, `src/cli/metrics.rs`

Default (no section flags) computes **all three** sections.

```json
{
  "schema_version": 1,
  "pagerank": {
    "top": [
      { "node": "UUID string", "pagerank": 0.0 }
    ],
    "converged": true,
    "iterations": 20,
    "max_delta": 0.0
  },
  "betweenness": [
    { "node": "UUID string", "score": 0.0 }
  ],
  "communities": {
    "count": 0,
    "modularity": 0.0,
    "assignments": 0
  }
}
```

| Section | When present | Notes |
|---------|--------------|-------|
| `pagerank` | `--pagerank` or default (all) | `top` capped at 20 nodes |
| `betweenness` | `--betweenness` or default | Top-level **array**, top 20 |
| `communities` | `--communities` or default | `assignments` = number of labeled nodes |

Omitted keys: sections not requested are **absent** (not `null`, not `[]`). Serialization uses `Option` + `#[serde(skip_serializing_if = "Option::is_none")]` via `MetricsJsonResponse`.

---

## 5. `check` — schema v1

**Command:**

```bash
rbuilder -f json check --policy-file PATH
```

**Source:** `src/cli/check_output.rs`

```json
{
  "schema_version": 1,
  "policy": "path/to/policy.json",
  "violations": [
    {
      "symbol": "string",
      "error": "string",
      "violation": "string"
    }
  ],
  "passed": true
}
```

| Field | Type | Description |
|-------|------|-------------|
| `policy` | string | Path passed to `--policy-file` |
| `violations` | array | Always present; empty when passing |
| `passed` | boolean | `true` iff `violations` is empty |

**Violation entry** (one of `error` or `violation`; the other is omitted):

```json
{ "symbol": "foo", "error": "engine or policy error text" }
```

```json
{ "symbol": "foo", "violation": "cascade hazard: node … betweenness …" }
```

### Exit codes

- `0` — `passed == true`
- `1` — `passed == false`

---

## 6. `slice` — schema v1

**Command:**

```bash
rbuilder -f json slice FILE --line N --variable VAR [--view cfg|pdg|text] [--direction backward|forward] [--taint]
```

**Source:** `src/cli/slice_output.rs`

### CFG view (`--view cfg`)

```json
{
  "schema_version": 1,
  "file": "string",
  "function": "string",
  "view": "cfg",
  "nodes": [
    {
      "id": "block_0",
      "block_index": 0,
      "start_line": 1,
      "end_line": 5,
      "statements": [
        { "line": 1, "kind": "Expression", "text": "let x = 1;" }
      ]
    }
  ],
  "edges": [
    { "source": "block_0", "target": "block_1", "kind": "next" }
  ]
}
```

### PDG view (`--view pdg`)

```json
{
  "schema_version": 1,
  "file": "string",
  "function": "string",
  "view": "pdg",
  "nodes": [
    { "id": "node_0", "line": 42, "label": "let tmp = ctx;", "kind": "Expression" }
  ],
  "edges": [
    { "source": "node_1", "target": "node_0", "kind": "data", "variable": "ctx" }
  ]
}
```

### Text slice view (default `--view text`)

Includes line list **and** PDG subgraph topology for the slice:

```json
{
  "schema_version": 1,
  "file": "string",
  "criterion": { "line": 42, "variable": "ctx" },
  "direction": "backward",
  "reduction_percent": 65.0,
  "lines": [40, 42],
  "nodes": [ { "id": "node_0", "line": 42, "label": "...", "kind": "..." } ],
  "edges": [ { "source": "node_1", "target": "node_0", "kind": "data", "variable": "ctx" } ]
}
```

### Taint mode (`--taint`)

```json
{
  "schema_version": 1,
  "file": "string",
  "function": "string",
  "line": 0,
  "variable": "string",
  "taint": true,
  "flows": 0,
  "vulnerable": 0
}
```

---

## 7. `inspect` — schema v1

**Command:**

```bash
rbuilder -f json inspect SYMBOL --layer cfg|pdg|dom [layer options]
```

**Source:** `src/cli/inspect_output.rs`

### CFG layer

```json
{
  "schema_version": 1,
  "symbol": "string",
  "layer": "cfg",
  "pruned": false,
  "nodes": [ { "id": "block_0", "block_index": 0, "start_line": 1, "end_line": 5, "statements": [] } ],
  "edges": [ { "source": "block_0", "target": "block_1", "kind": "next" } ]
}
```

### PDG layer

```json
{
  "schema_version": 1,
  "symbol": "string",
  "layer": "pdg",
  "nodes": [
    { "id": "node_0", "line": 1, "label": "...", "kind": "...", "defined": ["x"], "used": ["y"] }
  ],
  "edges": [ { "source": "node_0", "target": "node_1", "kind": "control" } ],
  "data_deps": 0,
  "control_deps": 0
}
```

`defined` / `used` appear when `--def-use` is set.

### Dominance layer

```json
{
  "schema_version": 1,
  "symbol": "string",
  "layer": "dom",
  "nodes": [ { "block_index": 0, "start_line": 10, "end_line": 15 } ],
  "idom": [ { "block": 1, "immediate_dominator": 0 } ],
  "frontiers": [ { "block": 0, "frontier_blocks": [2, 3] } ]
}
```

Block references use stable **`block_index`** integers (sorted by `start_line`), not debug strings.

**Other formats:** `--format mermaid` and `--format graphviz` emit diagram text for CFG/dom layers (not JSON).

---

## 8. `export` — file output (not stdout JSON)

**Command:**

```bash
rbuilder export --format json|graphml|graphviz|mermaid -o PATH [--query "…"]
```

Writes to `-o`; stdout is a one-line summary unless output is redirected via global `-o`.

| `--format` | File content |
|------------|--------------|
| `json` | `CodeGraph::export_json()` (same family as `graph.db`) |
| `graphml` | GraphML XML |
| `graphviz` | DOT |
| `mermaid` | Mermaid flowchart |

---

## Verification

```bash
# Typed schema sanity (unit fixtures per command)
cargo test --test cli_output

# Subprocess golden path (discover + blast-radius)
cargo test --test subprocess_golden_path

# Full platform I/O audit (all structured commands, sandbox -d)
cargo test --test all_commands_sanity

# Combined CI gate
cargo test --test cli_output --test subprocess_golden_path --test all_commands_sanity
```

---

## Remaining gaps

- **HTML dashboard:** still uses discover-time node properties, not CLI JSON shapes
- **Rust plugin:** does not set `properties.language` on graph nodes yet (v2 falls back to `.rs` extension)
- **Re-run `discover`** on repos indexed before P2 to populate SQLite UUID + v2 target columns
