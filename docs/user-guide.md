# rBuilder User Guide

End-to-end guide for installing rBuilder, indexing a Java example project, and querying the codebase from the **command line only**.

**New to code graphs?** Read **[Introduction](Introduction.md)** first — concepts, goals, and benefits for each feature, with links back here for commands.

For JSON field reference see [cli-output-schemas.md](cli-output-schemas.md) and [json-api.md](json-api.md).

---

## Table of contents

1. [Installation](#1-installation)
2. [Add rBuilder to your PATH](#2-add-rbuilder-to-your-path)
3. [Example project: coolstore](#3-example-project-coolstore)
4. [Index with `discover`](#4-index-with-discover)
5. [Global CLI flags](#5-global-cli-flags)
6. [Query the graph with GQL](#6-query-the-graph-with-gql)
7. [Blast radius (change impact)](#7-blast-radius-change-impact)
8. [Program slicing and taint](#8-program-slicing-and-taint)
9. [Inspect CFG / PDG / dominance](#9-inspect-cfg--pdg--dominance)
10. [Graph metrics](#10-graph-metrics)
11. [Export graph projections](#11-export-graph-projections)
12. [CI policy check](#12-ci-policy-check)
13. [Query daemon (`serve`)](#13-query-daemon-serve)
14. [Recommended workflow](#14-recommended-workflow)
15. [Command reference](#15-command-reference)
16. [Troubleshooting](#16-troubleshooting)

---

## 1. Installation

### Option A — GitHub release (recommended)

Pre-built binaries are published on the project **Releases** page:

**https://github.com/sshaaf/rBuilder/releases**

1. Open the latest release.
2. Download the archive for your platform:

   | Platform | Typical asset name |
   |----------|-------------------|
   | macOS (Apple Silicon) | `rbuilder-*-aarch64-apple-darwin.tar.gz` |
   | macOS (Intel) | `rbuilder-*-x86_64-apple-darwin.tar.gz` |
   | Linux (x86_64) | `rbuilder-*-x86_64-unknown-linux-gnu.tar.gz` |
   | Windows | `rbuilder-*-x86_64-pc-windows-msvc.zip` |

3. Extract the archive. You should get a single `rbuilder` executable (plus `rbuilder.exe` on Windows).

```bash
# macOS / Linux example
tar -xzf rbuilder-*-aarch64-apple-darwin.tar.gz
./rbuilder --version
```

```powershell
# Windows example (PowerShell)
Expand-Archive rbuilder-*-x86_64-pc-windows-msvc.zip -DestinationPath .
.\rbuilder.exe --version
```

If no release is published yet for your platform, use [Option B](#option-b--build-from-source).

### Option B — Build from source

Requires **Rust 1.70+** ([rustup.rs](https://rustup.rs/)).

```bash
git clone https://github.com/sshaaf/rBuilder.git
cd rBuilder
cargo build --release
./target/release/rbuilder --version
```

All six Tier 1 languages (Rust, Python, JavaScript, TypeScript, Go, Java) are always included in the binary.

---

## 2. Add rBuilder to your PATH

Pick one approach for your shell.

### macOS / Linux — user-local install

```bash
mkdir -p ~/.local/bin
cp /path/to/rbuilder ~/.local/bin/
chmod +x ~/.local/bin/rbuilder
```

Add to `~/.zshrc` or `~/.bashrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Reload and verify:

```bash
source ~/.zshrc   # or ~/.bashrc
rbuilder --version
```

### macOS / Linux — system-wide (optional)

```bash
sudo cp /path/to/rbuilder /usr/local/bin/
rbuilder --version
```

### Windows

1. Copy `rbuilder.exe` to a folder such as `C:\Tools\rbuilder\`.
2. Open **Settings → System → About → Advanced system settings → Environment Variables**.
3. Under **User variables**, edit `Path` and add `C:\Tools\rbuilder`.
4. Open a new terminal:

```powershell
rbuilder --version
```

### Per-project usage (no PATH change)

Pass the full path or use a repo-local alias:

```bash
alias rbuilder='/path/to/rbuilder'
```

---

## 3. Example project: coolstore

[Konveyor coolstore](https://github.com/konveyor-ecosystem/coolstore) is a Java e-commerce demo used in migration tooling walkthroughs. Use the **`quarkus`** branch for a modern Quarkus REST app (recommended). The **`main`** branch is the legacy Java EE / EAP monolith.

```bash
git clone https://github.com/konveyor-ecosystem/coolstore.git
cd coolstore
git checkout quarkus
export REPO="$PWD"
```

Layout on the `quarkus` branch (simplified):

```
coolstore/
├── pom.xml
├── src/main/java/com/redhat/coolstore/
│   ├── rest/           # CartEndpoint, OrderEndpoint, ProductEndpoint, …
│   ├── service/        # ShoppingCartService, OrderService, CatalogService, …
│   └── model/          # Order, Product, ShoppingCart, …
└── deploy/
```

All commands below assume `REPO` points at the project root, or run from inside the repo and use `.` instead of `"$REPO"`.

---

## 4. Index with `discover`

`discover` scans source files, builds the knowledge graph, runs analytics (complexity, communities, centrality, blast-radius scoring), and writes artifacts under `.rbuilder/`.

### Fast index (default)

```bash
cd "$REPO"
rbuilder discover .
```

Typical runtime: under a minute on a laptop for coolstore (`quarkus` branch).

**CI / automation** — structured metrics on stdout:

```bash
rbuilder -f json discover . | jq '.metrics'
```

### Language and path filters

```bash
# Java only, skip build output
rbuilder discover . -l java -e target,node_modules

# Multiple languages
rbuilder discover . -l java,typescript -e node_modules,dist
```

### Deeper analysis (slower)

| Flag | What it adds |
|------|----------------|
| `--security` | Secret scanning on config-like files |
| `--cfg` | Per-function CFG, PDG, dominance, and taint analysis |
| `--all` | `--security` + `--cfg` |

```bash
rbuilder discover . --cfg
rbuilder discover . --all
```

Use `--cfg` or `--all` when you need `inspect`, `slice` overlays, or taint flows. On large monorepos (100k+ functions) expect minutes to hours.

### Verbose logging

```bash
rbuilder discover . -v
```

### Legacy JSON graph (optional)

By default, rBuilder writes a **binary snapshot** (`graph.snapshot.bin`). Legacy `graph.db` / `graph.json` are only written when requested:

```bash
rbuilder discover . --write-json-graph
```

### What `discover` creates

After a successful run:

```
coolstore/.rbuilder/
├── graph.snapshot.bin          # Columnar mmap graph (primary cache for queries)
├── blast_engine.snapshot.bin   # Pre-built blast-radius engine
├── macro_call_index.db         # Fast symbol lookup (SQLite)
├── macro_call_index.bin        # Bin lookup companion
├── analysis_results.bin        # Columnar analysis properties
├── file_hashes.json            # Incremental file tracker
├── analysis/                   # Per-function CFG/PDG/taint (with --cfg or --all)
│   └── cfg_pdg.archive.bin     # CFG/PDG archive (with --cfg or --all)
└── dashboard/                  # Static HTML dashboard (if embedded in your build)
    ├── index.html
    ├── manifest.json
    └── graph_payload.bin
```

Query commands read `graph.snapshot.bin` when present. You do **not** need `graph.db` for normal CLI use.

Point every subsequent command at this repo:

```bash
export REPO="$PWD"
# or pass -r on each command:
rbuilder -r "$REPO" gql 'MATCH (n:Function) RETURN n LIMIT 5'
```

---

## 5. Global CLI flags

These apply to **every** subcommand:

| Flag | Purpose |
|------|---------|
| `-r, --repo PATH` | Repository root (default: current directory) |
| `-d, --db PATH` | Legacy graph JSON path (default: `.rbuilder/graph.db`) |
| `-f, --format FORMAT` | Output: `text`, `json`, `graphviz`, `mermaid` |
| `-o, --output FILE` | Write command output to a file instead of stdout |

Examples:

```bash
# JSON for scripting
rbuilder -r "$REPO" -f json gql 'MATCH (n:Class) RETURN n LIMIT 10'

# Mermaid diagram to a file
rbuilder -r "$REPO" -f mermaid -o cart-cfg.mmd inspect CartEndpoint cfg
```

---

## 6. Query the graph with GQL

`gql` runs the graph query language against the indexed graph. **Run `discover` first.**

### Count and list nodes

```bash
rbuilder -r "$REPO" gql 'MATCH (n:Function) RETURN n'
```

### Filter by name (wildcards)

```bash
rbuilder -r "$REPO" gql \
  "MATCH (n:Function) WHERE n.name LIKE '*Cart*' RETURN n"
```

Exact match:

```bash
rbuilder -r "$REPO" gql \
  "MATCH (n:Function) WHERE n.name = 'ShoppingCartService' RETURN n"
```

### Call relationships

One-hop calls:

```bash
rbuilder -r "$REPO" gql \
  'MATCH (a:Function)-[:CALLS*1..1]->(b:Function) RETURN a,b LIMIT 20'
```

Multi-hop chain (up to 3 hops):

```bash
rbuilder -r "$REPO" gql \
  'MATCH (a:Function)-[:CALLS*1..3]->(b:Function) RETURN a,b'
```

```bash
rbuilder -r "$REPO" gql \
  "MATCH (n:Function) WHERE n.name LIKE '*Endpoint' RETURN n"
```

### Named query macros

Built-in macros avoid typing long queries:

```bash
rbuilder -r "$REPO" gql --macro-name all_functions 'unused'
rbuilder -r "$REPO" gql --macro-name direct_calls 'unused'
rbuilder -r "$REPO" gql --macro-name call_chain 'unused'
```

The positional query string is ignored when `--macro-name` is set.

### Explain query plans

```bash
rbuilder -r "$REPO" gql --explain \
  "MATCH (n:Function) WHERE n.name = 'ShoppingCartService' RETURN n"
```

### JSON for automation

```bash
rbuilder -r "$REPO" -f json gql \
  "MATCH (n:Function) WHERE n.name LIKE '*Cart*' RETURN n" \
  | jq '.rows'
```

### Common node types

`Function`, `Class`, `Interface`, `Module`, `File`, `Import`, `ConfigKey`, …

### Common edge types

`CALLS`, `IMPORTS`, `CONTAINS`, `DEPENDS_ON`, `IMPLEMENTS`, …

---

## 7. Blast radius (change impact)

`blast-radius` answers: **“What breaks upstream if I change this symbol?”**

```bash
rbuilder -r "$REPO" blast-radius ShoppingCartService
```

Coolstore examples:

```bash
rbuilder -r "$REPO" blast-radius CatalogService
rbuilder -r "$REPO" blast-radius CartEndpoint
rbuilder -r "$REPO" blast-radius OrderService
```

### Symbol forms

| Form | Example |
|------|---------|
| Bare name | `process` (fails if ambiguous) |
| FQN | `ShoppingCartService::checkOutShoppingCart` |
| UUID | node id from GQL JSON output |

Disambiguate with:

```bash
rbuilder -r "$REPO" blast-radius process --class ShoppingCartService
rbuilder -r "$REPO" blast-radius process --file src/main/java/com/redhat/coolstore/service/ShoppingCartService.java
```

### Limit caller depth

```bash
# Direct callers only in impact zone
rbuilder -r "$REPO" blast-radius ShoppingCartService --depth 1

# Up to 5 incoming call hops
rbuilder -r "$REPO" blast-radius ShoppingCartService --depth 5
```

Omit `--depth` for full transitive upstream closure.

### Policy file (gatekeeping)

```bash
rbuilder -r "$REPO" blast-radius ShoppingCartService --policy-file policy.json
rbuilder -r "$REPO" blast-radius ShoppingCartService --no-policy
```

### Statement-level slice hand-offs (slow)

```bash
rbuilder -r "$REPO" blast-radius ShoppingCartService --with-slices
```

Requires `discover --cfg` for rich PDG context.

### JSON output

```bash
rbuilder -r "$REPO" -f json blast-radius ShoppingCartService \
  | jq '.metrics.score, .topology.direct_callers'
```

Schema: [cli-output-schemas.md](cli-output-schemas.md) §1 and [json-api.md](json-api.md) §6.

---

## 8. Program slicing and taint

`slice` performs **line-level** backward or forward slicing on a source file. It reads the file from disk; `discover --cfg` improves cross-function context elsewhere in the toolchain.

### Backward slice

“What code influences this variable at this line?”

```bash
rbuilder -r "$REPO" slice \
  src/main/java/com/redhat/coolstore/service/ShoppingCartService.java \
  --line 45 \
  --variable cart \
  --function ShoppingCartService
```

### Forward slice

```bash
rbuilder -r "$REPO" slice \
  src/main/java/com/redhat/coolstore/rest/CartEndpoint.java \
  --line 37 \
  --variable cartId \
  --function CartEndpoint \
  --direction forward
```

### Taint trace

```bash
rbuilder -r "$REPO" slice \
  src/main/java/com/redhat/coolstore/service/ShoppingCartService.java \
  --line 48 \
  --variable cart \
  --function ShoppingCartService \
  --taint
```

### View modes

| `--view` | Description |
|----------|-------------|
| `text` | Summary (default) |
| `cfg` | CFG overlay — use with `-f mermaid` or `-f graphviz` |
| `pdg` | PDG overlay |

```bash
rbuilder -r "$REPO" -f mermaid slice ... --view cfg
```

### Explicit language

If parsing fails, pass `--language java` and an exact `--function` class name.

---

## 9. Inspect CFG / PDG / dominance

`inspect` dumps semantic layers for an **indexed function symbol**. Run `discover --cfg` first for full CFG/PDG data.

```bash
# Control-flow graph summary
rbuilder -r "$REPO" inspect ShoppingCartService cfg

# Prune unreachable blocks
rbuilder -r "$REPO" inspect ShoppingCartService cfg --prune

# CFG as Mermaid
rbuilder -r "$REPO" -f mermaid inspect ShoppingCartService cfg

# Program dependence graph (data edges only)
rbuilder -r "$REPO" inspect ShoppingCartService pdg --edge-layer data

# PDG with def-use lists
rbuilder -r "$REPO" inspect ShoppingCartService pdg --def-use

# Dominator tree + frontiers
rbuilder -r "$REPO" inspect ShoppingCartService dom --frontiers
```

---

## 10. Graph metrics

`metrics` reports network analytics on the indexed call graph. Discover already computes many of these during indexing; use `metrics` for on-demand JSON output.

```bash
# All metrics (PageRank, betweenness, communities)
rbuilder -r "$REPO" metrics

# Individual reports
rbuilder -r "$REPO" metrics --pagerank
rbuilder -r "$REPO" metrics --betweenness
rbuilder -r "$REPO" metrics --communities

# Tune PageRank iterations
rbuilder -r "$REPO" -f json metrics --pagerank --iterations 50 | jq .
```

---

## 11. Export graph projections

`export` writes the graph or a GQL-selected subgraph to a file.

```bash
# Full graph as JSON
rbuilder -r "$REPO" export \
  --export-format json \
  --export-output coolstore-graph.json

# GraphML subgraph
rbuilder -r "$REPO" export \
  --export-format graphml \
  --export-output cart-subgraph.graphml \
  --query "MATCH (n:Function) WHERE n.name LIKE '*Cart*' RETURN n"

# DOT / Mermaid for external tools
rbuilder -r "$REPO" export --export-format graphviz --export-output calls.dot --query all
rbuilder -r "$REPO" export --export-format mermaid --export-output calls.mmd --query all
```

`--query all` exports the full graph; otherwise pass a GQL query string.

---

## 12. CI policy check

`check` evaluates blast-radius policy rules against functions changed in the current git working tree (or all functions if git is unavailable).

```bash
rbuilder -r "$REPO" check --policy-file policy.json
```

Exit code **1** when violations are found — suitable for CI pipelines.

---

## 13. Query daemon (`serve`)

For many blast-radius or GQL calls in one session, keep the graph and blast engine warm:

```bash
# Terminal 1
rbuilder -r "$REPO" serve

# Terminal 2 — auto-uses .rbuilder/query.sock when present
rbuilder -r "$REPO" -f json blast-radius ShoppingCartService
```

Options:

```bash
rbuilder -r "$REPO" serve --socket /tmp/rbuilder.sock --idle-secs 600
```

Disable auto-connect: `RBUILDER_NO_QUERY_DAEMON=1`.

---

## 14. Recommended workflow

```bash
# 1. Install and clone example
git clone https://github.com/konveyor-ecosystem/coolstore.git
cd coolstore
git checkout quarkus
export REPO="$PWD"

# 2. Index
rbuilder discover .

# 3. Explore structure
rbuilder -r "$REPO" gql --macro-name all_functions 'x' | head
rbuilder -r "$REPO" gql \
  'MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a,b LIMIT 15'

# 4. Change-impact before editing
rbuilder -r "$REPO" blast-radius CartEndpoint
rbuilder -r "$REPO" -f json blast-radius CartEndpoint --depth 3 | jq .

# 5. Find architectural hotspots
rbuilder -r "$REPO" -f json metrics --pagerank | jq .

# 6. Deep dive on a hot path (after discover --cfg)
rbuilder discover . --cfg
rbuilder -r "$REPO" inspect ShoppingCartService pdg --edge-layer data
rbuilder -r "$REPO" slice src/main/java/com/redhat/coolstore/service/ShoppingCartService.java \
  --line 45 --variable cart --function ShoppingCartService

# 7. Export for external graph tools
rbuilder -r "$REPO" export --export-format graphml \
  --export-output coolstore-calls.graphml --query all
```

---

## 15. Command reference

| Command | Purpose |
|---------|---------|
| `discover` | Index repo, build `.rbuilder/` artifacts |
| `gql` | Graph query language |
| `blast-radius` | Upstream call-graph impact for a symbol |
| `slice` | Line-level program slice or taint trace |
| `inspect` | CFG / PDG / dominance for a function |
| `metrics` | PageRank, betweenness, communities |
| `export` | Serialize graph (json, graphml, dot, mermaid) |
| `check` | CI policy gateway |
| `serve` | Unix-socket query daemon |

### `discover` flags

| Flag | Description |
|------|-------------|
| `-l, --languages` | Comma-separated filter (`java`, `typescript`, `rust`, …) |
| `-e, --exclude` | Comma-separated path exclude patterns |
| `-v, --verbose` | Debug logging |
| `--security` | Secret scanning |
| `--cfg` | CFG / PDG / taint analysis |
| `--all` | Security + CFG analysis |
| `--write-json-graph` | Also write legacy `graph.db` / `graph.json` |

---

## 16. Troubleshooting

### `Graph not found` / `run discover first`

Run indexing in the repository root:

```bash
rbuilder discover .
```

Or pass `-r` explicitly:

```bash
rbuilder -r /path/to/coolstore gql 'MATCH (n:Function) RETURN n LIMIT 1'
```

### Symbol not found (`blast-radius`, `inspect`)

Search for exact names:

```bash
rbuilder -r "$REPO" gql "MATCH (n:Function) WHERE n.name LIKE '*Cart*' RETURN n"
```

Use FQN or disambiguation flags:

```bash
rbuilder -r "$REPO" blast-radius addItem --class ShoppingCartService
```

### Slice parse errors

Pass explicit language and class:

```bash
rbuilder -r "$REPO" slice path/to/File.java \
  --line 10 --variable x --function MyClass --language java
```

### Slow `discover`

Start with the default mode. Add `--cfg` or `--all` only when you need inspect, slice overlays, or taint.

### `rbuilder: command not found`

Confirm PATH (see [§2](#2-add-rbuilder-to-your-path)) or invoke the binary by full path.

---

## Further reading

- [json-api.md](json-api.md) — programmatic JSON parsing (TypeScript shapes, jq, exit codes)
- [cli-getting-started.md](cli-getting-started.md) — extended coolstore examples
- [cli-output-schemas.md](cli-output-schemas.md) — JSON shapes for automation
- [performance-engineering.md](performance-engineering.md) — latency tiers and benchmarks
- [dashboard-design.md](dashboard-design.md) — optional HTML dashboard (not required for CLI)
