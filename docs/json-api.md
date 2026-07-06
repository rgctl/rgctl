# rBuilder JSON API

Programmatic reference for parsing rBuilder output. Every structured CLI command emits **one JSON document on stdout** when invoked with `-f json` / `--format json`.

**Companion docs:** field-by-field catalogs live in [cli-output-schemas.md](cli-output-schemas.md).

**Source of truth (Rust types):** `src/cli/*_output.rs`

---

## Table of contents

1. [Invocation](#1-invocation)
2. [Schema versioning](#2-schema-versioning)
3. [Command index](#3-command-index)
4. [`discover`](#4-discover)
5. [`gql`](#5-gql)
6. [`blast-radius`](#6-blast-radius)
7. [`metrics`](#7-metrics)
8. [`check`](#8-check)
9. [`slice`](#9-slice)
10. [`inspect`](#10-inspect)
11. [`export` (file formats)](#11-export-file-formats)
12. [On-disk JSON after `discover`](#12-on-disk-json-after-discover)
13. [Exit codes](#13-exit-codes)
14. [Parsing recipes](#14-parsing-recipes)

---

## 1. Invocation

### Global flags

| Flag | Effect |
|------|--------|
| `-f json` | Emit structured JSON on **stdout** |
| `-r PATH` | Repository root (default: cwd) |
| `-o FILE` | Write stdout payload to a file instead of the terminal |

```bash
export REPO=/path/to/coolstore
rbuilder -r "$REPO" -f json gql 'MATCH (n:Function) RETURN n LIMIT 5' | jq .
rbuilder -r "$REPO" -f json blast-radius ShoppingCartService -o /tmp/blast.json
```

### Stdout vs stderr

| Mode | stdout | stderr |
|------|--------|--------|
| `-f json discover` | Single JSON telemetry object | Quiet (errors only) unless `-v` |
| `-f json` (other commands) | Single JSON result | Errors / warnings |
| Default text | Human-readable tables | Progress / info logs |

**Rule:** parse **stdout only** for JSON. Do not scrape stderr.

### Prerequisites

All query commands require a prior successful `discover` (creates `.rbuilder/graph.snapshot.bin` and related caches). See [user-guide.md](user-guide.md).

---

## 2. Schema versioning

Every JSON payload includes a top-level **`schema_version`** integer. Check it before parsing nested fields.

```javascript
const doc = JSON.parse(stdout);
if (doc.schema_version !== 2) {
  throw new Error(`unsupported blast-radius schema ${doc.schema_version}`);
}
```

| Command | Current `schema_version` | Breaking changes |
|---------|------------------------:|------------------|
| `discover` | **2** | v2 introduced structured `metrics` block |
| `blast-radius` | **2** | v2 added `target.language`, `target.canonical_fqn`, `metrics.caller_depth_limit` |
| `gql` | **1** | — |
| `metrics` | **1** | — |
| `check` | **1** | — |
| `slice` | **1** | — |
| `inspect` | **1** | — |

**Omitted vs null:** optional fields are **absent** when unset (not `null`), unless noted otherwise. Empty collections are usually `[]`, not omitted.

**Graph topology reuse:** `slice` (text view) and `inspect` (cfg/pdg) share node/edge shapes documented in [cli-output-schemas.md](cli-output-schemas.md) §6–7.

---

## 3. Command index

| Command | `-f json` | Primary keys | Typical use |
|---------|:---------:|--------------|-------------|
| `discover` | ✅ | `metrics` | CI ingestion gates, timing |
| `gql` | ✅ | `rows`, `count` | Graph queries, inventory |
| `blast-radius` | ✅ | `target`, `metrics`, `topology` | Change-impact automation |
| `metrics` | ✅ | `pagerank`, `betweenness`, `communities` | Hotspot ranking |
| `check` | ✅ | `passed`, `violations` | CI policy gate |
| `slice` | ✅ | `lines` / `nodes` / `edges` / `taint` | Line-level analysis |
| `inspect` | ✅ | `layer`, `nodes`, `edges` | CFG/PDG/dominance dumps |
| `export` | ❌ (file) | — | Full-graph serialization |
| `serve` | ❌ | — | Unix socket daemon (NDJSON RPC) |

---

## 4. `discover`

```bash
rbuilder -f json discover PATH [-l LANGS] [-e PATTERNS] [--cfg] [--all]
```

### TypeScript shape

```typescript
interface DiscoverResponse {
  schema_version: 2;
  command: "discover";
  metrics: {
    files_discovered: number;
    files_indexed: number;
    files_skipped: number;
    nodes_generated: number;
    edges_generated: number;
    duration_ms: number;
  };
}
```

### Example

```json
{
  "schema_version": 2,
  "command": "discover",
  "metrics": {
    "files_discovered": 120,
    "files_indexed": 118,
    "files_skipped": 2,
    "nodes_generated": 1842,
    "edges_generated": 4103,
    "duration_ms": 12500
  }
}
```

### jq

```bash
rbuilder -f json discover . | jq '.metrics | {nodes: .nodes_generated, ms: .duration_ms}'
```

---

## 5. `gql`

```bash
rbuilder -f json gql "<QUERY>" [--macro-name NAME] [--explain]
```

### TypeScript shape

```typescript
interface GqlResponse {
  schema_version: 1;
  rows: GqlRow[];       // one entry per MATCH result row
  count: number;        // always rows.length
  explain: boolean;     // mirrors --explain (plan is text-only)
}

interface GqlRow {
  binding: string;      // variable name from MATCH (e.g. "n", "a")
  node: string;         // bare symbol name
  type: string;         // node type label, e.g. "Function"
  file: string | null;  // source path when indexed
}
```

Each `rows[i]` is an **array** of bindings (one object per variable in the `RETURN` clause).

### Example

```json
{
  "schema_version": 1,
  "rows": [
    [
      {
        "binding": "n",
        "node": "ShoppingCartService",
        "type": "Function",
        "file": "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java"
      }
    ]
  ],
  "count": 1,
  "explain": false
}
```

### jq

```bash
# All function names
rbuilder -f json gql 'MATCH (n:Function) RETURN n' \
  | jq -r '.rows[][].node'

# Multi-binding row (a,b) from a CALLS query
rbuilder -f json gql 'MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a,b LIMIT 5' \
  | jq '.rows[] | map({binding, node, file})'
```

### Macros

When `--macro-name` is set, the positional query string is ignored:

```bash
rbuilder -f json gql --macro-name all_functions 'unused'
# Macros: all_functions | direct_calls | call_chain
```

---

## 6. `blast-radius`

```bash
rbuilder -f json blast-radius SYMBOL [--depth N] [--policy-file PATH] [--with-slices]
```

### TypeScript shape

```typescript
interface BlastRadiusResponse {
  schema_version: 2;
  target: {
    id: string;              // UUID
    symbol: string;
    class_context: string | null;
    file_path: string;
    language: string;        // "java" | "rust" | "python" | "unknown"
    signature?: string;      // omitted when unknown
    canonical_fqn: string;   // prefer for routing: "Class::method"
  };
  metrics: {
    score: number;           // 0–100
    direct_callers_count: number;
    impact_zone_size: number;
    caller_depth_limit?: number;  // present when --depth N
  };
  topology: {
    scc_component_id: number | null;
    direct_callers: SymbolContext[];
    impact_zone: SymbolContext[];
  };
  gatekeeping: {
    policy_status: "SKIPPED" | "PASS" | "VIOLATED";
    violations: PolicyViolation[];
    handoffs: SliceHandoff[];  // [] unless --with-slices
  };
}

interface SymbolContext {
  id: string;       // UUID — stable join key
  fqn: string;      // display name (language-native)
  file_path: string;
}

interface SliceHandoff {
  callee: string;
  param: string;
  index: number;
}
```

### Policy violations (`gatekeeping.violations`)

Tagged union — discriminant field is **`kind`**:

| `kind` | Fields |
|--------|--------|
| `domain_isolation` | `source_domain`, `reached_domain`, `node` |
| `scale_failure` | `count`, `max` |
| `cascade_hazard` | `node`, `betweenness`, `threshold` |
| `sanitization_bypass` | `sink_line`, `path_trace`, `sanitizer_node` |

### jq

```bash
# Impact score and caller UUIDs
rbuilder -f json blast-radius ShoppingCartService \
  | jq '{score: .metrics.score, callers: [.topology.direct_callers[].id]}'

# Depth-capped impact zone
rbuilder -f json blast-radius CartEndpoint --depth 3 \
  | jq '.metrics.caller_depth_limit, .topology.impact_zone | length'

# Policy gate
rbuilder -f json blast-radius OrderService --policy-file policy.json \
  | jq '.gatekeeping.policy_status, .gatekeeping.violations'
```

**Routing rule:** use `target.canonical_fqn` and `topology.*.id` (UUID). Treat `topology.*.fqn` as display text only.

### Migration from legacy flat JSON

Older rBuilder emitted a flat object (`symbol`, `score`, `direct_callers[]`, `impact_zone[]` at the root). Current output is **nested** with `schema_version: 2`. See [cli-output-schemas.md](cli-output-schemas.md) §1 for the full field catalog and jq path mapping.

```bash
# Was: jq '.score'  →  Now:
jq '.metrics.score'

# Was: jq '.direct_callers[]'  →  Now:
jq '.topology.direct_callers[].fqn'

# Prefer for automation (v2):
jq '.target.canonical_fqn'
```

---

## 7. `metrics`

```bash
rbuilder -f json metrics [--pagerank] [--betweenness] [--communities] [--iterations N]
```

Default (no section flags) includes **all three** sections. Requesting a single flag omits the others entirely.

### TypeScript shape

```typescript
interface MetricsResponse {
  schema_version: 1;
  pagerank?: {
    top: { node: string; pagerank: number }[];  // max 20
    converged: boolean;
    iterations: number;
    max_delta: number;
  };
  betweenness?: { node: string; score: number }[];  // max 20, top-level array
  communities?: {
    count: number;
    modularity: number;
    assignments: number;
  };
}
```

### jq

```bash
rbuilder -f json metrics --pagerank | jq '.pagerank.top[:5]'
rbuilder -f json metrics | jq '.communities.modularity'
```

---

## 8. `check`

```bash
rbuilder -f json check --policy-file policy.json
```

Evaluates policy rules against **git-changed** functions (or all functions if git is unavailable).

### TypeScript shape

```typescript
interface CheckResponse {
  schema_version: 1;
  policy: string;           // path passed to --policy-file
  passed: boolean;
  violations: {
    symbol: string;
    error?: string;         // engine error (mutually exclusive with violation)
    violation?: string;     // human-readable policy text
  }[];
}
```

### jq

```bash
rbuilder -f json check --policy-file policy.json | jq '{passed, count: (.violations | length)}'
```

---

## 9. `slice`

```bash
rbuilder -f json slice FILE --line N --variable VAR [--function NAME] \
  [--view text|cfg|pdg] [--direction backward|forward] [--taint]
```

Response shape depends on **`--view`** and **`--taint`**.

### `--view text` (default)

```typescript
interface SliceTextResponse {
  schema_version: 1;
  file: string;
  criterion: { line: number; variable: string };
  direction: "backward" | "forward";
  reduction_percent: number;
  lines: number[];              // source lines in the slice
  nodes: PdgNode[];             // PDG subgraph
  edges: PdgEdge[];
}
```

### `--view cfg`

```typescript
interface SliceCfgResponse {
  schema_version: 1;
  file: string;
  function: string;
  view: "cfg";
  nodes: CfgBlockNode[];
  edges: CfgEdgeNode[];
}
```

### `--view pdg`

Same topology as inspect PDG (`view: "pdg"`).

### `--taint`

Flat summary (no graph topology):

```typescript
interface SliceTaintResponse {
  schema_version: 1;
  file: string;
  function: string;
  line: number;
  variable: string;
  taint: true;
  flows: number;
  vulnerable: number;
}
```

### Shared graph primitives

```typescript
interface PdgNode {
  id: string;       // "node_0", …
  line: number;
  label: string;
  kind: string;
  defined?: string[];
  used?: string[];
}

interface PdgEdge {
  source: string;
  target: string;
  kind: "data" | "control" | string;
  variable?: string;  // data deps only
}

interface CfgBlockNode {
  id: string;       // "block_0", …
  block_index: number;
  start_line: number;
  end_line: number;
  statements: { line: number; kind: string; text: string }[];
}

interface CfgEdgeNode {
  source: string;
  target: string;
  kind: string;   // "next", "iftrue", "iffalse", …
}
```

### jq

```bash
# Lines touched by backward slice
rbuilder -f json slice src/.../Foo.java --line 42 --variable x --function Foo \
  | jq '.lines'

# Taint counts only
rbuilder -f json slice src/.../Foo.java --line 10 --variable input --function Foo --taint \
  | jq '{flows, vulnerable}'
```

---

## 10. `inspect`

```bash
rbuilder -f json inspect SYMBOL cfg|pdg|dom [layer options]
```

Requires `discover --cfg` for richest PDG/CFG data from the analysis archive.

### CFG layer

```typescript
interface InspectCfgResponse {
  schema_version: 1;
  symbol: string;
  layer: "cfg";
  pruned: boolean;
  nodes: CfgBlockNode[];
  edges: CfgEdgeNode[];
}
```

### PDG layer

```typescript
interface InspectPdgResponse {
  schema_version: 1;
  symbol: string;
  layer: "pdg";
  nodes: PdgNode[];
  edges: PdgEdge[];
  data_deps: number;
  control_deps: number;
}
```

### Dominance layer

```typescript
interface InspectDomResponse {
  schema_version: 1;
  symbol: string;
  layer: "dom";
  nodes: { block_index: number; start_line: number; end_line: number }[];
  idom: { block: number; immediate_dominator: number }[];
  frontiers?: { block: number; frontier_blocks: number[] }[];  // with --frontiers
}
```

Block references use integer **`block_index`** (sorted by `start_line`), not string ids.

### jq

```bash
rbuilder -f json inspect ShoppingCartService pdg --edge-layer data \
  | jq '{data: .data_deps, nodes: [.nodes[] | {line, label}]}'
```

**Diagram formats:** `-f mermaid` and `-f graphviz` emit diagram **text** (not JSON) for cfg/dom layers.

---

## 11. `export` (file formats)

`export` writes to **`--export-output`**; stdout is a one-line summary (unless global `-o` redirects).

```bash
rbuilder export --export-format json --export-output graph.json [--query "GQL…"]
```

| `--export-format` | File content |
|-------------------|--------------|
| `json` | Full `CodeGraph` JSON (same family as legacy `graph.db`) |
| `graphml` | GraphML XML |
| `graphviz` | DOT |
| `mermaid` | Mermaid flowchart |

`--query all` exports the full graph; otherwise pass a GQL query string to select a subgraph.

---

## 12. On-disk JSON after `discover`

These files are written under `.rbuilder/` (and copied into `.rbuilder/dashboard/` for the UI). They are **not** emitted on stdout but are stable inputs for custom tooling.

| Path | `schema_version` | Purpose |
|------|------------------:|---------|
| `dashboard/manifest.json` | 1 | Bundle metadata, phase flags, metric summary |
| `dashboard/metagraph.json` | 2 | Package-level graph for LOD UI |
| `dashboard/cfg_index.json` | 1 | CFG function catalog |
| `dashboard/slice_index.json` | 1 | Slice/PDG function catalog |
| `dashboard/dataflow_index.json` | 1 | Dataflow function catalog |
| `dashboard/taint_index.json` | 1 | Taint summary (`discover --cfg`) |
| `dashboard/taint/{uuid}.json` | 1 | Per-function taint flows |
| `dashboard/slice/{uuid}.json` | 1 | Per-function source + PDG bundle |
| `dashboard/cfg/{uuid}.json` | 1 | Per-function CFG preview |
| `file_hashes.json` | — | Incremental discover state |

### `manifest.json` (excerpt)

```json
{
  "schema_version": 1,
  "phases": { "0": "complete", "4": "complete", "8": "pending" },
  "graph": {
    "payload_path": "graph_payload.bin",
    "payload_format": "columnar_v2",
    "node_count": 1842,
    "edge_count": 4103
  },
  "analysis": {
    "cfg_available": true,
    "taint_available": true,
    "taint_flow_count": 12,
    "taint_vulnerable_count": 3
  },
  "metrics": {
    "function_count": 412,
    "avg_complexity": 1.2
  }
}
```

### `taint_index.json`

```json
{
  "schema_version": 1,
  "available": true,
  "detail_dir": "taint",
  "function_count": 8,
  "total_flows": 12,
  "vulnerable_flows": 3,
  "functions": [
    {
      "function_id": "uuid",
      "name": "ShoppingCartService",
      "file_path": "src/main/java/.../ShoppingCartService.java",
      "flow_count": 2,
      "vulnerable_count": 1
    }
  ]
}
```

### `taint/{uuid}.json` flow entry

```json
{
  "id": 0,
  "variable": "userInput",
  "source_type": "HttpParameter",
  "sink_type": "SqlQuery",
  "severity": 10,
  "vulnerable": true,
  "sanitizers": [],
  "source_line": 42,
  "sink_line": 88,
  "source_text": "...",
  "sink_text": "...",
  "path_lines": [42, 55, 88],
  "path_statements": ["...", "...", "..."]
}
```

Binary artifacts (`graph.snapshot.bin`, `graph_payload.bin`, `blast_engine.snapshot.bin`) use internal columnar formats — use CLI JSON or `export --export-format json` for portable graph access.

---

## 13. Exit codes

| Command | `0` | `1` |
|---------|-----|-----|
| `discover` | Success | Failure |
| `gql` | Success | Query/IO error |
| `blast-radius` | Success, or policy skipped | `--policy-file` + `policy_status == "VIOLATED"` (JSON still on stdout) |
| `check` | `passed == true` | `passed == false` |
| `slice` / `inspect` / `metrics` / `export` | Success | Error |

**CI pattern:** capture stdout first, then check `$?`.

```bash
out=$(rbuilder -f json blast-radius Foo --policy-file policy.json) || ec=$?
echo "$out" | jq .
exit "${ec:-0}"
```

---

## 14. Parsing recipes

### Python

```python
import json, subprocess

def rbuilder_json(repo: str, *args: str) -> dict:
    cmd = ["rbuilder", "-r", repo, "-f", "json", *args]
    out = subprocess.check_output(cmd, text=True)
    return json.loads(out)

doc = rbuilder_json("/path/to/coolstore", "blast-radius", "CartEndpoint")
assert doc["schema_version"] == 2
for caller in doc["topology"]["direct_callers"]:
    print(caller["id"], caller["fqn"])
```

### Node.js

```javascript
import { execFileSync } from "node:child_process";

function rbuilderJson(repo, ...args) {
  const out = execFileSync("rbuilder", ["-r", repo, "-f", "json", ...args], {
    encoding: "utf8",
  });
  return JSON.parse(out);
}

const gql = rbuilderJson(process.env.REPO, "gql", "MATCH (n:Function) RETURN n");
const names = gql.rows.flat().map((b) => b.node);
```

### CI ingestion gate

```bash
metrics=$(rbuilder -f json discover .)
nodes=$(echo "$metrics" | jq '.metrics.nodes_generated')
test "$nodes" -gt 100
```

### Chaining discover → query

```bash
rbuilder -f json discover . | tee discover.json
rbuilder -f json gql --macro-name all_functions x | jq '.count'
```

---

## Verification

Schema fixtures are tested in CI:

```bash
cargo test --test cli_output --test subprocess_golden_path --test all_commands_sanity
```

See [cli-io-sanity-qe.md](cli-io-sanity-qe.md) for the full coverage matrix.

---

## Related

- [user-guide.md](user-guide.md) — install, coolstore walkthrough, CLI examples
- [cli-output-schemas.md](cli-output-schemas.md) — exhaustive field tables per command
- [performance-engineering.md](performance-engineering.md) — latency tiers and `serve` daemon notes
