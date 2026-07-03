# How to Run Blast Radius Analysis

Developer-oriented guide for indexing a codebase and running **blast radius** — upstream call-graph impact for a function or method.

For JSON field reference see [cli-output-schemas.md](cli-output-schemas.md). For perf tiers see [performance-engineering.md](performance-engineering.md).

---

## Prerequisites

- **Git**
- **Rust 1.70+** — [rustup.rs](https://rustup.rs/)
- A target repository to analyze

---

## Step 1: Build rBuilder

```bash
git clone https://github.com/sshaaf/rBuilder.git
cd rBuilder
cargo build --release
export PATH="$PWD/target/release:$PATH"
rbuilder --version
```

---

## Step 2: Index with `discover`

`discover` parses sources, builds the call graph, pre-computes blast analysis, and writes artifacts under **`.rbuilder/`**:

| Artifact | Purpose |
|----------|---------|
| `graph.snapshot.bin` | Columnar v2 mmap graph (**default**; required for fast paths) |
| `blast_engine.snapshot.bin` | Pre-built SCC blast engine (lite query path) |
| `macro_call_index.db` / `.bin` | SQLite + bin lookup (T0 fast path) |
| `cfg_pdg.archive.bin` | Optional; `discover --cfg` for slice hand-offs |

Legacy `graph.db` / `graph.json` are **not** written unless you pass `--write-json-graph`.

```bash
cd /path/to/your-project
rbuilder discover . --languages java,rust --exclude target,node_modules
```

With JSON telemetry on stdout:

```bash
rbuilder -f json discover .
```

---

## Step 3: Run blast radius

```bash
rbuilder -r /path/to/your-project blast-radius OrderService::process
```

### Symbol forms

| Form | Example |
|------|---------|
| Bare name | `process` (fails if ambiguous) |
| FQN | `OrderService::process` |
| UUID | `424d403b-1b2c-4a3d-8e9f-0c1b2a3f4e5d` |

Disambiguation flags: `--class OrderService`, `--file java/com/example/OrderService.java`

### Limit upstream depth (`--depth`)

Cap **`topology.impact_zone`** to **N incoming call hops** (hop 1 = direct callers). **`direct_callers`** is always immediate callers only.

```bash
# Direct callers only in impact_zone
rbuilder -f json blast-radius OrderService::process --depth 1

# Up to 5 hops upstream
rbuilder -f json blast-radius OrderService::process --depth 5

# Full transitive closure (default)
rbuilder -f json blast-radius OrderService::process
```

When `--depth` is set, JSON includes `metrics.caller_depth_limit: N` and `metrics.score` reflects the filtered zone.

### JSON output (schema v2)

```bash
rbuilder -f json blast-radius OrderService::process | jq '.metrics'
rbuilder -f json blast-radius OrderService::process | jq '.target.canonical_fqn'
rbuilder -f json blast-radius OrderService::process --depth 3 | jq '.metrics.caller_depth_limit'
```

Top-level shape: `target`, `metrics`, `topology`, `gatekeeping` — see [blast-radius-json-schema-v2.md](blast-radius-json-schema-v2.md).

### Policy and slices

```bash
rbuilder -f json blast-radius publishEvent --policy-file policy.json
rbuilder -f json blast-radius publishEvent --with-slices
```

These force the **full graph path** (slower). Policy evaluates the depth-filtered impact zone when `--depth` is also set.

---

## Step 4: Optional query daemon (repeated queries)

**Not required** for one-off or agent use. Helpful when running many symbols in one session.

```bash
# Terminal 1 — keeps mmap graph + engine in memory
rbuilder -r /path/to/your-project serve

# Terminal 2 — auto-connects to .rbuilder/query.sock
rbuilder -r /path/to/your-project -f json blast-radius saveError
```

- Disable auto-connect: `RBUILDER_NO_QUERY_DAEMON=1`
- Restart `serve` after `discover` (digest mismatch otherwise)

---

## Query path overview (for debugging)

```
blast-radius SYMBOL
  ├─ T0  macro_call_index / SQLite cache hit
  ├─ T1  query daemon (if serve running) OR lite engine (mmap + engine snapshot)
  └─ full hydrated graph (--with-slices, --policy-file centrality, cache miss)
```

---

## Read text output

```
Blast radius for 'process'
  Score: 42.3/100
  Direct callers: 1
  Impact zone: 3
  ...
```

| Field | Meaning |
|-------|---------|
| **Score** | 0–100 impact score (recomputed when `--depth` caps the zone) |
| **Direct callers** | Immediate callers (one hop) |
| **Impact zone** | Transitive upstream functions in scope (filtered by `--depth` when set) |

Only **Calls** edges are traversed. Non-function nodes are filtered from the displayed zone.

---

## Troubleshooting

### Graph not found

```
Graph not found at ... (run `rbuilder discover` first)
```

Run `discover` on that repo path. Ensure `.rbuilder/graph.snapshot.bin` exists.

### Symbol not found / ambiguous

```bash
rbuilder gql "MATCH (n:Function) WHERE n.name = 'process' RETURN n"
rbuilder blast-radius OrderService::process
```

### Stale cache after code changes

```bash
rbuilder discover .
# restart `serve` if running
rbuilder blast-radius ...
```

### Depth seems ignored

- Confirm `-f json` and check `metrics.caller_depth_limit`
- Full closure is default when `--depth` is omitted
- `--with-slices` / `--policy-file` still honor `--depth` on the impact zone for policy; slices use full engine result internally

---

## Related commands

| Command | Use |
|---------|-----|
| `discover` | Build / refresh `.rbuilder/*` snapshots |
| `serve` | Warm daemon for repeated blast-radius |
| `gql` | Explore functions and structure |
| `check` | CI policy gateway |
| `slice` | Line-level backward/forward slice |

---

## Quick reference

```bash
cargo build --release
cd /path/to/project && rbuilder discover .
rbuilder blast-radius MyClass::myMethod
rbuilder -f json blast-radius MyClass::myMethod --depth 5
rbuilder serve -r /path/to/project &   # optional
```
