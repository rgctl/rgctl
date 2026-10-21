# Structured graph queries

## Introduction

After `discover`, use **structured query verbs** to explore the indexed knowledge graph: `find`, `callers`, `callees`, `relations`, `inventory`, and `status`. Query verbs use a stable JSON envelope (`schema_version` **2**) when you pass `-f json`; `status` uses `schema_version` **3**.

These commands are the **agent-facing** way to answer structural questions — symbol lookup, call neighborhoods, typed edges, and inventories — without loading whole files into context. Prefer them over ad-hoc graph languages in scripts and CI.

## Use cases

- **Inventory a codebase.** Count or list functions, classes, edge types, or communities.
- **Resolve symbols.** Find definitions by name pattern, type, file glob, or scope prefix.
- **Trace calls.** List direct or multi-hop callers and callees for a method.
- **Walk relations.** Follow `calls`, `contains`, `imports`, `references`, `violates`, and other edge types with explicit direction.
- **Session health.** `status` reports whether `.rgctl/graph.snapshot.bin` is present before heavier commands.

## Example project

This guide uses **CoolStore** (`example/coolstore`). Index first:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover -l java
```

## Step-by-step

### 1. Session status

```bash
rgctl -r example/coolstore -f json status | jq '{status, nodes, edges, schema_version}'
```

**Example** (after `discover -l java --with-cfg --with-harmonic`):

```json
{
  "status": "ok",
  "nodes": 608,
  "edges": 1709,
  "schema_version": 3
}
```

Returns `status: "ok"` when a snapshot exists, plus node/edge counts and optional Kantra findings path.

### 2. Function inventory

```bash
rgctl -r example/coolstore -f json inventory --by type | jq '.counts[] | select(.key=="function")'
```

**Example:** `{"count": 152, "key": "function"}`


Text mode prints `key<TAB>count` rows. `--by` also accepts `edge`, `lang`, `file`, `community`, and `import-prefix`.

Count all functions without listing rows:

```bash
rgctl -r example/coolstore -f json find --type function --count-only | jq '.total'
```

### 3. Find by name

```bash
rgctl -r example/coolstore -f json find priceShoppingCart --exact | jq '.entities[0]'
```

**Example:**

```json
{
  "file": "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
  "id": "28de0bfd-1b19-5c3c-bcbf-26947d067654",
  "line": 54,
  "name": "priceShoppingCart",
  "qualified_name": "com.redhat.coolstore.service.ShoppingCartService.priceShoppingCart",
  "type": "function"
}
```

Pattern search (default limit 50):

```bash
rgctl -r example/coolstore find '*Cart*' --type function
```

Disambiguate overloads with `--file`, `--class`, or `--line` (same idea as `blast-radius`).

### 4. Callers and callees

Who calls `priceShoppingCart`?

```bash
rgctl -r example/coolstore -f json callers priceShoppingCart | jq '.callers[:5]'
```

**Example:**

```json
[
  {
    "name": "checkOutShoppingCart",
    "qualified_name": "com.redhat.coolstore.service.ShoppingCartService.checkOutShoppingCart",
    "line": 43,
    "type": "function"
  },
  {
    "name": "delete",
    "qualified_name": "com.redhat.coolstore.rest.CartEndpoint.delete",
    "line": 98,
    "type": "function"
  },
  {
    "name": "set",
    "qualified_name": "com.redhat.coolstore.rest.CartEndpoint.set",
    "line": 74,
    "type": "function"
  },
  {
    "name": "add",
    "qualified_name": "com.redhat.coolstore.rest.CartEndpoint.add",
    "line": 47,
    "type": "function"
  }
]
```

Envelope keys: `callers`, `total`, `schema_version` (2). What does it call?

```bash
rgctl -r example/coolstore callees priceShoppingCart --depth 2
```

JSON uses `callers` / `callees` keys (not `neighbors`). Each row is an `EntityRow` with `id`, `name`, `type`, `file`, and optional `qualified_name`.

### 5. Relations (CALLS and more)

List call edges involving `priceShoppingCart`:

```bash
rgctl -r example/coolstore -f json relations priceShoppingCart --edge calls --direction both
```

Seedless scan (use filters to stay bounded):

```bash
rgctl -r example/coolstore -f json relations --edge calls --from-type function --to-type function --limit 20
```

Edge names are lowercase CLI tokens (`calls`, `contains`, `imports`, `references`, `violates`, …). Direction: `out`, `in`, or `both`.

### 6. Classes and modules

```bash
rgctl -r example/coolstore -f json find ShoppingCartService --type class | jq '.entities'
```

Limit to a package prefix:

```bash
rgctl -r example/coolstore find '*Service' --type class --scope com.redhat.coolstore.service
```

### 7. Communities

Community detection runs at `discover` and lives in `.rgctl/analysis_results.bin` (not as topology edges).

```bash
rgctl -r example/coolstore -f json communities list | jq '.communities[:5]'
rgctl -r example/coolstore -f json inventory --by community | jq '.counts[:5]'
```

**Example (communities):**

```json
[
  {
    "id": 177,
    "label": "Order",
    "member_count": 25
  },
  {
    "id": 397,
    "label": "coolstore.service::CatalogItemEntity",
    "member_count": 20
  },
  {
    "id": 143,
    "label": "ShoppingCart",
    "member_count": 15
  },
  {
    "id": 306,
    "label": "coolstore.model::shoppingCartToJson",
    "member_count": 14
  },
  {
    "id": 608,
    "label": "Infrastructure / Common Library",
    "member_count": 12
  }
]
```

Heuristic labels (for example `coolstore.model::shoppingCartToJson`) help you choose a `--scope` prefix for `find`. Refresh labels with `communities label --write`. See [Community detection](community-detection.md).

### 8. Kantra rules

After `discover -l java --with-kantra`, list indexed rules (CoolStore currently yields thousands of rule nodes and `violates` edges):

```bash
rgctl -r example/coolstore -f json find --type kantrarule --limit 10 | jq '.entities[].name'
```

Filter by migration target using discover’s `--kantra-target` (violations and skips land in `.rgctl/kantra_findings.json`). Rule-to-code matches:

```bash
rgctl -r example/coolstore -f json relations --edge violates --from-type kantrarule --limit 20
```

### 9. `query` alias

`rgctl query find …` is equivalent to `rgctl find …` (same for `callers`, `callees`, `relations`, `inventory`). Useful when wrapping a single `rgctl` subprocess.

## JSON shapes (summary)

| Verb | Primary keys |
|------|----------------|
| `find` | `entities`, `returned`, `total`, `schema_version` (2) |
| `callers` / `callees` | `callers` or `callees`, `total`, `schema_version` (2) |
| `relations` | `edges` (`source`, `edge`, `target`, `direction`, `hops`), `returned`, `total`, optional seed `target` |
| `inventory` | `by`, `counts` |
| `status` | `status`, `nodes`, `edges`, `digest`, `schema_version` (3) |

Full field catalog: [json-api.md](../json-api.md#5-structured-query-verbs).

## Common flags

| Flag | Verbs | Purpose |
|------|-------|---------|
| `-r REPO` | all | Repository root (same as `discover`) |
| `-f json` | all | Machine-readable stdout |
| `--type` | `find` | Node type (`function`, `class`, `module`, `kantrarule`, …) |
| `--file GLOB` | find, callers, callees, relations | Path filter |
| `--scope PREFIX` | find, callers, callees, relations | Qualified-name prefix filter |
| `--limit N` | find, callers, callees, relations | Cap rows (default 50 where applicable) |
| `--exact` | `find` | Exact name match |
| `--count-only` | `find` | Return counts only |

## Benefits

- **Stable contracts.** Fixed verbs with `schema_version` 2 envelopes (query verbs) and `schema_version` 3 for `status`.
- **Fast.** Queries mmap the session snapshot; no re-parse of source trees.
- **Composable.** Chain with `blast-radius`, `metrics`, `cpg`, and `semantic query` on the same `.rgctl/` session.

## Related guides

- [Discovering and indexing](discovering-and-indexing.md) — build `.rgctl/` first
- [Blast radius analysis](blast-radius-analysis.md) — weighted upstream impact from a symbol
- [Community detection](community-detection.md) — labels and `communities list`
- [Agent pack](agent-skill.md) — install the `rgctl` skill that calls these verbs
- [JSON API](../json-api.md) — exhaustive stdout schemas
