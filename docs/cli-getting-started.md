# rBuilder CLI Getting Started

This guide walks through indexing [coolstore-quarkus](https://github.com/sshaaf/coolstore-quarkus) — a Quarkus microservices demo with cart, catalog, inventory, order, and payment services — and querying it with the rBuilder CLI.

## Prerequisites

Build rBuilder from source:

```bash
git clone https://github.com/sshaaf/rBuilder.git
cd rBuilder
cargo build --release
export PATH="$PWD/target/release:$PATH"
rbuilder --version
```

Clone the example repository:

```bash
git clone https://github.com/sshaaf/coolstore-quarkus.git
cd coolstore-quarkus
```

Coolstore layout (simplified):

```
coolstore-quarkus/
├── cart-service/        # CartService, CartResourceV1, …
├── catalog-service/     # CatalogService, CatalogResource, …
├── inventory-service/   # InventoryResource, …
├── order-service/       # OrderResource, …
├── payment-service/     # PaymentResource, …
└── coolstore-fe/        # frontend
```

---

## Step 1: Discover the codebase

`discover` scans the repository, builds the knowledge graph, runs graph analytics, writes artifacts under `.rbuilder/`, and **generates the interactive HTML dashboard** at `.rbuilder/dashboard.html`.

### Fast index (default)

Indexes source files and runs standard graph analysis (complexity, communities, centrality, blast-radius scoring):

```bash
rbuilder discover .
```

Typical runtime: ~30 seconds on a laptop for a repo this size.

**CI / automation:** use `-f json` to emit structured ingestion metrics on stdout (no progress bar):

```bash
rbuilder -f json discover . | jq '.metrics'
```

See [cli-output-schemas.md](cli-output-schemas.md) §2 for the full `discover` JSON shape.

Open the dashboard when indexing finishes:

```bash
open .rbuilder/dashboard.html   # macOS
# xdg-open .rbuilder/dashboard.html   # Linux
```

### Optional analysis modes

```bash
# Add secret scanning on config-like files
rbuilder discover . --security

# Add CFG / PDG / taint analysis (much slower)
rbuilder discover . --cfg

# Everything: security + CFG/PDG/taint
rbuilder discover . --all
```

Filter languages or paths:

```bash
rbuilder discover . -l java,typescript -e node_modules,target
```

Verbose logging:

```bash
rbuilder discover . -v
```

### What discover creates

After a successful run you should see:

```
coolstore-quarkus/.rbuilder/
├── graph.db              # Graph cache (used by all query commands)
├── graph.json            # Legacy mirror of graph topology
├── analysis_results.bin  # Columnar analysis snapshot
├── dashboard.html        # Interactive HTML dashboard (always written by discover)
├── file_hashes.json      # Incremental file tracker
└── analysis/             # Per-function CFG/PDG overlays (discover --cfg or --all only)
```

Point rBuilder at this repo for every subsequent command:

```bash
export REPO="$PWD"   # coolstore-quarkus root
```

Or pass `-r` on each invocation:

```bash
rbuilder -r "$REPO" gql 'MATCH (n:Function) RETURN n LIMIT 5'
```

---

## Global flags

These work on every command:

| Flag | Purpose |
|------|---------|
| `-r, --repo PATH` | Repository root (default: current directory) |
| `-d, --db PATH` | Graph cache file (default: `.rbuilder/graph.db`) |
| `-f, --format FORMAT` | Output format: `text`, `json`, `graphviz`, `mermaid` |
| `-o, --output FILE` | Write command output to a file instead of stdout |

Examples:

```bash
# JSON output for scripting
rbuilder -r "$REPO" -f json gql 'MATCH (n:Class) RETURN n LIMIT 10'

# Discover telemetry for CI (files indexed, nodes, duration_ms)
rbuilder -r "$REPO" -f json discover .

# Mermaid diagram on stdout (or -o file.mmd)
rbuilder -r "$REPO" -f mermaid inspect CartResourceV1 cfg
```

> The HTML dashboard is **not** a global format flag. It is written automatically by `discover` to `.rbuilder/dashboard.html`. Re-run `discover` to refresh it after large changes.

---

## Step 2: Query with GQL

`gql` runs the graph query language against the indexed graph. You must run `discover` first.

### Count functions

```bash
rbuilder -r "$REPO" gql 'MATCH (n:Function) RETURN n'
```

### Find cart-service REST endpoints

```bash
rbuilder -r "$REPO" gql \
  "MATCH (n:Function) WHERE n.name LIKE '*Resource*' RETURN n"
```

### Trace call relationships

One-hop calls between functions:

```bash
rbuilder -r "$REPO" gql \
  'MATCH (a:Function)-[:CALLS*1..1]->(b:Function) RETURN a,b LIMIT 20'
```

Multi-hop call chain (up to 3 hops):

```bash
rbuilder -r "$REPO" gql \
  'MATCH (a:Function)-[:CALLS*1..3]->(b:Function) RETURN a,b'
```

### Named query macros

Built-in macros avoid typing long queries:

```bash
# All functions
rbuilder -r "$REPO" gql --macro-name all_functions 'unused'

# Direct call edges
rbuilder -r "$REPO" gql --macro-name direct_calls 'unused'

# Call chains up to 3 hops
rbuilder -r "$REPO" gql --macro-name call_chain 'unused'
```

> The positional query argument is ignored when `--macro-name` is set; use any placeholder string.

### Explain query plans

```bash
rbuilder -r "$REPO" gql --explain \
  "MATCH (n:Function) WHERE n.name = 'CartService' RETURN n"
```

### JSON for automation

```bash
rbuilder -r "$REPO" -f json gql \
  "MATCH (n:Function) WHERE n.name LIKE '*Cart*' RETURN n" \
  | jq '.rows'
```

---

## Step 3: Impact analysis (blast radius)

`blast-radius` answers “what breaks if I change this symbol?” using SCC-aware macro impact analysis.

```bash
rbuilder -r "$REPO" blast-radius CartService
```

Other useful targets in coolstore:

```bash
rbuilder -r "$REPO" blast-radius CatalogService
rbuilder -r "$REPO" blast-radius CartResourceV1
rbuilder -r "$REPO" blast-radius InventoryResource
```

Limit transitive caller depth:

```bash
rbuilder -r "$REPO" blast-radius CartService --depth 5
```

When `--depth` is set, `topology.impact_zone` includes only upstream callers within that many incoming call hops (hop 1 = direct callers). JSON output includes `metrics.caller_depth_limit` with the applied cap; omit `--depth` for the full transitive closure.

With a policy file (enables cascade-hazard checks against centrality thresholds):

```bash
rbuilder -r "$REPO" blast-radius CartService --policy-file policy.json
```

JSON output includes direct callers, impact zone, and interprocedural slice hand-offs:

JSON output (schema v1 — nested `target` / `metrics` / `topology` / `gatekeeping`):

```bash
rbuilder -r "$REPO" -f json blast-radius CartService | jq '.metrics.score, .topology.direct_callers'
```

Breaking change from the old flat JSON shape; see [blast-radius-json-schema-v1.md](blast-radius-json-schema-v1.md) and the full catalog [cli-output-schemas.md](cli-output-schemas.md).

### Optional: query daemon (repeated queries)

For many blast-radius calls in one session, keep the graph and blast engine warm:

```bash
# Terminal 1
rbuilder -r "$REPO" serve

# Terminal 2 — auto-uses .rbuilder/query.sock when present
rbuilder -r "$REPO" -f json blast-radius CartService
```

Disable auto-connect with `RBUILDER_NO_QUERY_DAEMON=1`. Not required for one-off or agent queries.

---

## Step 4: Program slicing

`slice` performs line-level backward or forward slicing (and optional taint checks) on a single source file. It reads the file from disk and does not require the symbol to be indexed first, but `discover --cfg` improves cross-function context elsewhere.

Backward slice — “what code influences this variable at this line?”:

```bash
rbuilder -r "$REPO" slice \
  cart-service/src/main/java/org/coolstore/cart/service/CartServiceImpl.java \
  --line 42 \
  --variable cart \
  --function CartServiceImpl
```

Forward slice:

```bash
rbuilder -r "$REPO" slice \
  cart-service/src/main/java/org/coolstore/cart/resource/CartResourceV1.java \
  --line 30 \
  --variable request \
  --function CartResourceV1 \
  --direction forward
```

Taint policy check (requires patterns in the function):

```bash
rbuilder -r "$REPO" slice \
  cart-service/src/main/java/org/coolstore/cart/service/CartServiceImpl.java \
  --line 50 \
  --variable input \
  --function CartServiceImpl \
  --taint
```

View formats:

```bash
# Text summary (default)
rbuilder -r "$REPO" slice ... --view text

# CFG or PDG overlay (use with -f mermaid or -f graphviz)
rbuilder -r "$REPO" -f mermaid slice ... --view cfg
```

---

## Step 5: Inspect CFG / PDG / dominance

`inspect` dumps semantic layers for an indexed function symbol. Run `discover --cfg` first for richest results.

```bash
# Control-flow graph summary
rbuilder -r "$REPO" inspect CartServiceImpl cfg

# CFG as Mermaid diagram
rbuilder -r "$REPO" -f mermaid inspect CartServiceImpl cfg

# Program dependence graph
rbuilder -r "$REPO" inspect CartServiceImpl pdg --edge-layer data

# Dominator tree with frontiers
rbuilder -r "$REPO" inspect CartServiceImpl dom --frontiers
```

---

## Step 6: Graph metrics

`metrics` reports network analytics on the indexed call graph.

Run all metrics (PageRank, betweenness, communities):

```bash
rbuilder -r "$REPO" metrics
```

Individual reports:

```bash
rbuilder -r "$REPO" metrics --pagerank
rbuilder -r "$REPO" metrics --betweenness
rbuilder -r "$REPO" metrics --communities
```

Tune PageRank iterations:

```bash
rbuilder -r "$REPO" -f json metrics --pagerank --iterations 50 | jq .
```

> Discover already computes complexity, communities, and centrality during indexing. Use `metrics` when you want on-demand JSON output without re-running the full pipeline.

---

## Step 7: Export the graph

`export` serializes the graph or a query projection to a file.

```bash
# Full graph as JSON
rbuilder -r "$REPO" export --export-format json --export-output coolstore-graph.json

# GraphML subgraph (GQL query selects nodes/edges)
rbuilder -r "$REPO" export \
  --export-format graphml \
  --export-output cart-subgraph.graphml \
  --query "MATCH (n:Function) WHERE n.name LIKE '*Cart*' RETURN n"

# DOT / Mermaid for visualization tools
rbuilder -r "$REPO" export --export-format graphviz --export-output calls.dot --query all
rbuilder -r "$REPO" export --export-format mermaid --export-output calls.mmd --query all
```

---

## Step 8: CI policy check (optional)

`check` evaluates blast-radius policy rules against functions changed in the current git working tree (or all functions if git is unavailable).

```bash
rbuilder -r "$REPO" check --policy-file policy.json
```

Exit code `1` when violations are found — suitable for CI pipelines.

---

## Recommended workflow

```bash
# 1. Index and open dashboard
cd coolstore-quarkus
rbuilder discover .
open .rbuilder/dashboard.html

# 2. Explore structure
rbuilder -r . gql --macro-name all_functions 'x' | head
rbuilder -r . gql 'MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a,b LIMIT 15'

# 3. Change-impact before editing
rbuilder -r . blast-radius CartResourceV1

# 4. Deep dive on hot paths
rbuilder -r . metrics --pagerank

# 5. Line-level debugging on a specific change
rbuilder -r . slice cart-service/.../CartServiceImpl.java \
  --line 42 --variable cart --function CartServiceImpl
```

---

## Command reference

| Command | Purpose |
|---------|---------|
| `discover` | Index repo, build `.rbuilder/graph.db`, and write `dashboard.html` |
| `gql` | Graph query language |
| `blast-radius` | SCC macro impact / caller analysis |
| `slice` | Line-level program slice or taint trace |
| `inspect` | CFG / PDG / dominance for a function |
| `metrics` | PageRank, betweenness, communities |
| `export` | Serialize graph (json, graphml, dot, mermaid) |
| `check` | CI policy gateway |

### Discover flags

| Flag | Description |
|------|-------------|
| `-l, --languages` | Comma-separated language filter (`java`, `typescript`, …) |
| `-e, --exclude` | Comma-separated path exclude patterns |
| `-v, --verbose` | Debug logging |
| `--security` | Secret scanning |
| `--cfg` | CFG / PDG / taint analysis |
| `--all` | Security + CFG analysis |

### GQL WHERE clauses

Exact match:

```bash
WHERE n.name = 'CartService'
```

Wildcard match (`*` = any substring):

```bash
WHERE n.name LIKE '*Cart*'
```

### GQL node types (common)

`Function`, `Class`, `Interface`, `Module`, `File`, `Import`, `ConfigKey`, …

### GQL edge types (common)

`CALLS`, `IMPORTS`, `CONTAINS`, `DEPENDS_ON`, `IMPLEMENTS`, …

---

## Troubleshooting

**`Graph not found at .rbuilder/graph.db`**

Run `rbuilder discover .` in the repository root (or pass `-r` / `-d` explicitly).

**Symbol not found in blast-radius / inspect**

Check exact names with GQL:

```bash
rbuilder -r "$REPO" gql "MATCH (n:Function) WHERE n.name LIKE '*Cart*' RETURN n"
```

**Slice fails to parse a file**

Pass `--language java` and `--function <ExactClassName>` explicitly.

**Slow discover**

Use the default mode first. Add `--cfg` or `--all` only when you need slicing, taint, or inspect overlays.

---

## Next steps

- Open `.rbuilder/dashboard.html` after `discover` for interactive exploration (GQL, blast radius, slice overlays).
- Re-run `discover` after major refactors to refresh the graph and dashboard.
- Use `-f json` on query commands and pipe to `jq` for scripting and CI integration.
