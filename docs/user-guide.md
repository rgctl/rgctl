# rgctl User Guide

End-to-end guide for installing rgctl, indexing an in-tree example, and querying a codebase from the **command line**. Sample outputs target **`rgctl-tests/ecommerce-java`**. Runnable examples are backed by scenarios under [`user-guide/scenarios/`](user-guide/scenarios/) (see change `docs-agent-first-diataxis`).

**Concepts:** [Introduction](Introduction.md). **Use with agents:** [agent-skill](guides/agent-skill.md) · [USER_AGENTS_TEMPLATE](agents/USER_AGENTS_TEMPLATE.md). **Contribute:** [AGENTS.md](../AGENTS.md). **JSON fields:** [json-api.md](json-api.md).

### How this guide is organized

| Zone | Sections | Role |
|------|----------|------|
| **Tutorial spine** | §1–4, §16 | Install → fixture → discover → recommended workflow |
| **How-to** | §5–14 | Flags and feature commands with sample output |
| **Optional UI** | §15 | `serve` / dashboard — nice-to-have, not required for agents |
| **Reference** | §17–18 | Command cheat sheet + troubleshooting |

---

## Table of contents

1. [Installation](#1-installation)
2. [Add rgctl to your PATH](#2-add-rgctl-to-your-path)
3. [Example project: ecommerce-java](#3-example-project-ecommerce-java)
4. [Index with `discover`](#4-index-with-discover)
5. [Global CLI flags](#5-global-cli-flags)
6. [Structured graph queries](#6-structured-graph-queries)
7. [Blast radius (change impact)](#7-blast-radius-change-impact)
8. [Program slicing and taint](#8-program-slicing-and-taint)
9. [Inspect CFG / PDG / dominance](#9-inspect-cfg--pdg--dominance)
10. [Hybrid CPG (`cpg`)](#10-hybrid-cpg-cpg)
11. [Graph metrics](#11-graph-metrics)
12. [Semantic search](#12-semantic-search)
13. [Export graph projections](#13-export-graph-projections)
14. [CI policy check](#14-ci-policy-check)
15. [HTTP server (`serve`) — optional](#15-http-server-serve--optional)
16. [Recommended workflow](#16-recommended-workflow)
17. [Command reference](#17-command-reference)
18. [Troubleshooting](#18-troubleshooting)

---

## 1. Installation

> **Standalone guide:** [Installation](installation.md) covers prerequisites, CLI and HTTP modes, agent pack setup, upgrading, and troubleshooting.

### Option A — GitHub release (recommended)

Pre-built binaries are published on the project **Releases** page:

**https://github.com/sshaaf/rgctl/releases**

1. Open the latest release.
2. Download the archive for your platform:

   | Platform | Typical asset name |
   |----------|-------------------|
   | macOS (Apple Silicon) | `rgctl-*-aarch64-apple-darwin.tar.gz` |
   | macOS (Intel) | `rgctl-*-x86_64-apple-darwin.tar.gz` |
   | Linux (x86_64) | `rgctl-*-x86_64-unknown-linux-gnu.tar.gz` |
   | Windows | `rgctl-*-x86_64-pc-windows-msvc.zip` |

3. Extract the archive. You should get a single `rgctl` executable (plus `rgctl.exe` on Windows).

```bash
# macOS / Linux example
tar -xzf rgctl-*-aarch64-apple-darwin.tar.gz
./rgctl --version
```

```powershell
# Windows example (PowerShell)
Expand-Archive rgctl-*-x86_64-pc-windows-msvc.zip -DestinationPath .
.\rgctl.exe --version
```

If no release is published yet for your platform, use [Option B](#option-b--build-from-source).

### Option B — Build from source

Requires **Rust 1.99+** (Edition 2024; [rustup.rs](https://rustup.rs/)).

```bash
git clone https://github.com/sshaaf/rgctl.git
cd rgctl
# Optional: code-daemon ONNX weights (~206 MB via Git LFS) if you use
# `semantic index --embedder code-daemon`. Skip for the default vocab embedder.
git lfs pull
cargo build --release --bin rgctl
./target/release/rgctl --version
```

All **Tier 1** languages registered in [`languages.toml`](../languages.toml) (including Rust, Python, Ruby, PHP, JavaScript, TypeScript, Go, Java, C#, C, C++) are included in the release binary.

### Install the agent pack

After `rgctl` is on your `PATH`, install the **agent pack** into the **target repository** (the same root you pass to `discover` via `-r` / `--repo`, or the current directory):

```bash
rgctl install --skill
rgctl -r /path/to/repo install --skill
rgctl install --list-agents
```

That writes **one skill** named `rgctl` (plus `references/` with command encyclopedia and workflow playbooks). Older packs that installed separate `rgctl-discover` / `rgctl-impact` / … folders are obsolete — delete those directories if present.

**Default (no `--tools`):** `cursor`, `claude`, `codex`, `agents`, `antigravity`. Use **`--tools all`** for the full registry.

Full flag reference: [Installation](installation.md). Walkthrough: [Agent pack](guides/agent-skill.md). Add `--with-policy` for a Cursor structural-rules snippet. Use `-g` for a global install. Exit code **1** if a managed file differs unless you pass `--force`. Install does not run `discover`.

---

## 2. Add rgctl to your PATH

Pick one approach for your shell.

### macOS / Linux — user-local install

```bash
mkdir -p ~/.local/bin
cp /path/to/rgctl ~/.local/bin/
chmod +x ~/.local/bin/rgctl
```

Add to `~/.zshrc` or `~/.bashrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Reload and verify:

```bash
source ~/.zshrc   # or ~/.bashrc
rgctl --version
```

### macOS / Linux — system-wide (optional)

```bash
sudo cp /path/to/rgctl /usr/local/bin/
rgctl --version
```

### Windows

1. Copy `rgctl.exe` to a folder such as `C:\Tools\rgctl\`.
2. Open **Settings → System → About → Advanced system settings → Environment Variables**.
3. Under **User variables**, edit `Path` and add `C:\Tools\rgctl`.
4. Open a new terminal:

```powershell
rgctl --version
```

### Per-project usage (no PATH change)

Pass the full path or use a repo-local alias:

```bash
alias rgctl='/path/to/rgctl'
```

---

## 3. Example project: ecommerce-java

This guide uses the in-tree Spring Boot fixture shipped with rgctl:

**[`rgctl-tests/ecommerce-java`](../rgctl-tests/ecommerce-java)**

It implements the same e-commerce domain as the other `ecommerce-*` fixtures (cart, orders, products, auth), plus a **CoolStore-compatible dual API** under `/services/*` (additive next to `/api/*`). No separate clone is required when you have the rgctl repo.

```bash
# From the rgctl repository root
export REPO="$PWD/rgctl-tests/ecommerce-java"
cd "$REPO"
```

Layout (simplified):

```
ecommerce-java/
├── pom.xml
└── src/main/java/com/example/ecommerce/
    ├── controller/     # /api/* — CartController, OrderController, ProductController, …
    ├── service/        # CartService, OrderService, ProductService, …
    ├── entity/         # Cart, Order, Product, User, …
    ├── repository/     # Spring Data JPA repos
    ├── security/       # JWT filter / token provider
    └── coolstore/      # /services/* — CoolStore cart pricing + orders (in-memory)
        ├── rest/       # ProductEndpoint, CartEndpoint, OrderEndpoint
        ├── service/    # ShoppingCartService, PromoService, ShippingService, …
        └── model/      # ShoppingCart, ShoppingCartItem, CatalogProduct, …
```

**Dual REST surface** (same contract on every `ecommerce-*` language):

| Surface | Role |
|---------|------|
| `/api/*` | JWT e-commerce API (auth, categories, cart ownership, reviews, …) |
| `/services/*` | CoolStore-style products / cart / checkout / orders (`cartId` session carts) |

`ShoppingCartService.priceShoppingCart` mutates cart totals (promo + shipping) — the Layer F target for `cpg mutations --type ShoppingCart`. Full route table: [`rgctl-tests/README.md`](../rgctl-tests/README.md).

Sibling fixtures (`ecommerce-python`, `ecommerce-rust`, `ecommerce-c`, …) share both REST shapes.

All commands below assume `REPO` points at `ecommerce-java`, or that you run from inside that directory and use `.` instead of `"$REPO"`.

**Sample outputs** in this guide were captured on a laptop with a release build; absolute paths are shortened to `…/ecommerce-java/…` for readability. Counts may differ slightly across versions.

---

## 4. Index with `discover`

`discover` scans source files, builds the knowledge graph, runs analytics (complexity, communities, centrality, blast-radius scoring), and writes artifacts under **`{repo}/.rgctl/`**.

**Choosing the discover target** (common pitfall):

| Command | Indexes |
|---------|---------|
| `cd repo && rgctl discover .` | **Recommended** — current directory |
| `rgctl -r /path/to/repo discover` | Repo at `/path/to/repo` |
| `rgctl discover /path/to/repo` | Same (absolute path positional) |
| `rgctl -r /path/to/repo discover .` | **Shell cwd only** — `-r` is ignored when `.` is present |

Full table: [Discovering and indexing](guides/discovering-and-indexing.md).

Built-in registry includes **markdown** (`rgctl-lang-markdown`): `.md` and `.mdx` are indexed by default (headings, links, code blocks, frontmatter). See [markdown-context.md](markdown-context.md). Use `-l markdown` or `-l markdown,java` to limit languages.

### Full pipeline (`--full`)

`rgctl discover PATH --full` prints an execution plan, runs a **basic** discover (queryable snapshot), reports that the initial discover is complete, then continues in the same process with `--with-cfg --with-dashboard --with-harmonic`, then `semantic index`. Other terminals can run `find` / `inventory` after stage 1. Does **not** imply taint or secret scanning.

```bash
rgctl discover . --full
```

Status is written to `.rgctl/pipeline_status.json`. A second `--full` on unchanged sources skips fresh stages.

### Constrained discover (`--with-limits`)

Opt-in for containers / small machines. Default discover is unchanged.

```bash
rgctl discover . --with-limits max-mem-mb=4096,threads=1
# or: RGCTL_WITH_LIMITS=max-mem-mb=4096,threads=1 rgctl discover .
```

Caps pipeline/CFG Rayon workers, shrinks the extract stream channel and spill sort-run buffers from `max-mem-mb`, and aborts with a clear error if peak RSS approaches ~95% of the budget (warn at ~90%).

### Fast index (default)

```bash
cd "$REPO"
rgctl discover . -l java -e target
```

Example output:

```text
==> Analyzing: …/ecommerce-java/.
[✓] Indexed 51 files -> 518 nodes, 1122 edges (0.0s)
[✓] Detected 443 communities (modularity: 0.47)
[✓] Analyzed 187 functions (avg complexity: 1.0, 0 high, 0 medium)
[*] Top hotspot: findAll (PageRank: 0.0177)
[!] Found 48 circular dependencies
[✓] Analysis complete
[✓] Saved to .rgctl/ (0.1 MB total)
[✓] Completed in 0.0s (peak memory: 21 MB)

[i] Next steps:
   rgctl find --type function --count-only   # Query the graph
   rgctl slice <file> --line <N> --variable <VAR>
   rgctl serve --open   # Dashboard + query API at http://127.0.0.1:8080
```

Typical runtime on this fixture: **well under a second**.

**CI / automation** — structured metrics on stdout:

```bash
rgctl -f json discover . -l java -e target | jq .
```

<!-- ug-scenario:04-discover-json -->
Example:

```json
{
  "command": "discover",
  "metrics": {
    "duration_ms": "…",
    "edges_generated": 2637,
    "files_discovered": 65,
    "files_indexed": 65,
    "files_skipped": 0,
    "nodes_generated": 943
  },
  "schema_version": 2
}
```
<!-- /ug-scenario:04-discover-json -->

### Language and path filters

```bash
# Java only, skip Maven output
rgctl discover . -l java -e target

# Multiple languages (polyglot monorepo)
rgctl discover . -l java,typescript -e target,node_modules,dist
```

### Default pipeline (always on)

Bare `discover` (no `--with-*`) always runs: index/extract → topology → community → complexity → PageRank/betweenness → dependency cycles → blast engine → persist analysis + snapshot.

Harmonic, dashboard, migration export, security, CFG/PDG, and discover-time taint are **opt-in** via the flags below.

### Deeper analysis (opt-in)

| Flag | What it adds |
|------|----------------|
| `--with-security` | Secret scanning |
| `--with-cfg` | Per-function CFG, dominators, PDG (archive under `.rgctl/analysis/`) |
| `--with-taint` | Discover-time taint into archive (implies CFG/PDG pass) |
| `--taint-rules PATH` | Extra YAML pack file or directory (after built-ins + `.rgctl/taint-rules.d/`) |
| `--with-harmonic` | Harmonic centrality (migration ranking) |
| `--with-dashboard` | Static dashboard bundle under `.rgctl/dashboard/` |
| `--export-migration-hints` | Migration roadmap JSON (alias: `--export-migration-plan`) |
| `--with-kantra` | Konveyor Kantra rule evaluation + rules graph index (see below) |

```bash
# CFG so inspect / slice have rich PDG context
rgctl discover . -l java -e target --with-cfg

# Full walkthrough set used for the samples below
rgctl discover . -l java -e target \
  --with-cfg --with-dashboard --with-harmonic --export-migration-hints
```

Example lines from that richer run:

```text
[!] Deep analysis enabled (--with-cfg / --with-taint).
✓ Control flow analysis:
  CFG/PDG/Dominance: 178 functions analyzed
  Skipped: 9 functions (unsupported language or parse error)
[✓] Migration plan (Hybrid Default): 9 steps → …/ecommerce-java/./.rgctl/migration_plan.json
[✓] Dashboard: …/ecommerce-java/./.rgctl/dashboard/index.html
```

Use `--with-cfg` when you need `inspect` / slice overlays; add `--with-taint` for discover-time taint flows. On large monorepos (100k+ functions) expect minutes to hours.

### Kantra migration rules (`--with-kantra`)

Evaluate [Konveyor Kantra](https://github.com/konveyor/kantra) rules natively against the indexed graph and source cache — no Kantra CLI, LSP, or containers. Release binaries embed the Konveyor `stable/java` catalog (~2.6k rules); `discover --with-kantra` uses it by default.

```bash
# Default: embedded catalog + eval + index rules into graph.snapshot.bin
rgctl discover . -l java -e target --with-kantra

# Migration target filter (konveyor.io/target=<NAME> labels)
rgctl discover . -l java --with-kantra --kantra-target quarkus

# CI / fixtures: override with a local ruleset directory
rgctl discover . --with-kantra --kantra-rules tests/fixtures/kantra-rules

# Dev: local konveyor/rulesets checkout instead of embedded blob
rgctl discover . --with-kantra --kantra-catalog /path/to/rulesets/stable/java

# Index rules into graph only (skip eval)
rgctl discover . --with-kantra --kantra-index-only
```

| Flag | Purpose |
|------|---------|
| `--with-kantra` | Run Kantra eval and index `KantraRule` / `KantraRuleset` nodes into the session graph |
| `--kantra-target NAME` | Evaluate only rules labeled `konveyor.io/target=NAME` |
| `--kantra-rules DIR` | Single ruleset directory (`ruleset.yaml` + `*.yaml`); overrides embedded catalog |
| `--kantra-catalog ROOT` | Rulesets tree (e.g. `stable/java`); overrides embedded catalog |
| `--kantra-index-only` | Skip eval; still index catalog rules into the graph |

**Artifacts:** `.rgctl/kantra_findings.json` (violations + skipped rules). Rule nodes are queryable after index:

```bash
rgctl -f json find --type kantrarule --limit 10 | jq '.returned'
# Target filter at discover time:
rgctl discover . --with-kantra --kantra-target quarkus
```

Konveyor labels (`konveyor.io/target`, `konveyor.io/source`, …) are stored on rule nodes; filter eval with `--kantra-target` and inspect violations in `kantra_findings.json`.

**Source builds:** full embedded catalog requires the git submodule — `git submodule update --init crates/rgctl-kantra/assets/rulesets` or `./scripts/init-kantra-rulesets.sh`. Without it, the build falls back to `tests/fixtures/kantra-rules/`. See [`crates/rgctl-kantra/README.md`](../crates/rgctl-kantra/README.md).

**Further reading:** [Kantra integration exploration](design/kantra-integration-exploration.md) · [Architecture options (embedded catalog + rules graph)](../KANTRA_ARCHITECTURE_OPTIONS.md) · [JSON schema](json-api.md#kantra_findingsjson)

Does **not** require `--with-cfg`. Unsupported rule providers and invalid regex patterns are skipped with reasons in `skipped_rules`. After full eval, `violates` edges link each `KantraRule` to matched code nodes (`rgctl relations --edge violates --from-type kantrarule`).

**Dashboard:** `discover --with-kantra --with-dashboard` exports `kantra_index.json` and per-file violation shards. Open **Migration Rules** in the dashboard (category + Konveyor target filters, syntax-highlighted snippets). See [Dashboard user guide — Migration Rules](dashboard-user-guide.md#migration-rules-kantra).

### Verbose logging and stage profiling

```bash
rgctl discover . -v
```

With `-v`, discover emits a **`[profile] discover summary`** line (wall time, peak RSS, node count) and per-stage timings.

Extraction internals also emit `populate_graph` timing buckets in profile logs. These buckets are intentionally distinct (`symbol_processing_secs`, `config_key_secs`, `relation_resolution_secs`, `config_usage_secs`) and are measured independently to avoid double-counting when one bucket is empty.

```bash
RUST_LOG=info,profile=info rgctl discover . --with-cfg -v -l java -e target 2>&1 \
  | tee discover-profile.log
grep '\[profile\]' discover-profile.log
```

Example profile lines (ecommerce-java, `--with-cfg`):

```text
[profile] discover summary wall_secs=0.14 index_secs=0.01 post_index_secs=0.09 \
  peak_rss_mb=27.0 functions=187 nodes=518 cfg=true security=false
[profile] stage stage="cfg_total" secs=0.030 pct_wall=21.0
[profile] stage stage="save_dashboard" secs=0.028 pct_wall=19.6
[profile] stage stage="index_extract" secs=0.012 pct_wall=8.1
```

Harmonic centrality is **off by default** — pass `--with-harmonic` when you need it for migration ranking. On kernel-scale graphs it adds ~30s wall and multi‑GB peak RSS.

See [analysis-architecture.md](analysis-architecture.md) and [internal/profile.md](internal/profile.md) for large-graph adaptive gating and cold-profile runs.

### Legacy JSON graph (optional)

By default, rgctl writes a **binary snapshot** (`graph.snapshot.bin`). Legacy `graph.db` / `graph.json` are only written when requested:

```bash
rgctl discover . --write-json-graph
```

### What `discover` creates

After a successful run, the layout below appears under **`{repo}/.rgctl/`**:

```
{repo}/.rgctl/
├── graph.snapshot.bin          # Columnar mmap graph (primary cache for queries)
├── content_store.bin           # Large markdown bodies / files (body_ref / blob_ref; Obsidian + doc semantic)
├── blast_engine.snapshot.bin   # Pre-built blast-radius engine
├── macro_call_index.db         # Blast-radius lookup cache (SQLite; not the graph)
├── macro_call_index.bin        # Same index in bincode (companion to .db)
├── analysis_results.bin        # Columnar analysis properties
├── kantra_findings.json        # With --with-kantra (violations + skipped rules)
├── file_hashes.json            # Incremental file tracker
├── migration_plan.json         # With --export-migration-hints
├── analysis/                   # Per-function CFG/PDG/taint (with --with-cfg / --with-taint)
│   └── cfg_pdg.archive.bin
└── dashboard/                  # Only with --with-dashboard
    ├── index.html
    ├── manifest.json
    ├── migration_plan.json
    └── graph_payload.bin
```

Query commands read `graph.snapshot.bin` when present. You do **not** need `graph.db` for normal CLI use.

Point every subsequent command at the **same repo root** you indexed:

```bash
export REPO="$PWD"   # after cd into the repo
# or pass -r on each command:
rgctl -r "$REPO" -f json status
```

---

## 5. Global CLI flags

These apply to **every** subcommand:

| Flag | Purpose |
|------|---------|
| `-r, --repo PATH` | Repository root (default: current directory) |
| `-d, --db PATH` | Legacy graph JSON path (default: `.rgctl/graph.db`) |
| `-f, --format FORMAT` | Output: `text`, `json`, `graphviz`, `mermaid` |
| `-o, --output FILE` | Write command output to a file instead of stdout |

Examples:

```bash
# JSON for scripting
rgctl -r "$REPO" -f json find --type class --limit 10 | jq '.entities[].name'

# Mermaid diagram to a file
rgctl -r "$REPO" -f mermaid -o checkout-cfg.mmd inspect checkout cfg
```

---

## 6. Structured graph queries

Use **`find`**, **`callers`**, **`callees`**, **`relations`**, **`inventory`**, and **`status`** against the indexed graph. **Run `discover` first.** Step-by-step guide: [Structured graph queries](guides/structured-query.md). JSON shapes: [json-api.md §5](json-api.md#5-structured-query-verbs).

### Session check

```bash
rgctl -r "$REPO" -f json status | jq '{status, nodes, edges}'
```

### Function inventory

```bash
rgctl -r "$REPO" -f json find --type function --count-only | jq '.total'
```

<!-- ug-scenario:06-find-function-count -->
```text
317
```
<!-- /ug-scenario:06-find-function-count -->

Text mode lists names; JSON returns `entities[]` with `id`, `name`, `type`, and `file`.

### Exact name match

```bash
rgctl -r "$REPO" find clearCart --type function --exact
```

```text
clearCart
clearCart
```

(There are two `clearCart` methods — service and controller.)

```bash
rgctl -r "$REPO" -f json find clearCart --type function --exact | jq '.entities'
```

```json
[
  {
    "id": "…",
    "name": "clearCart",
    "type": "function",
    "file": "…/service/CartService.java"
  },
  {
    "id": "…",
    "name": "clearCart",
    "type": "function",
    "file": "…/controller/CartController.java"
  }
]
```

### Classes

```bash
rgctl -r "$REPO" -f json find CartService --type class --exact | jq '.entities[0]'
```

### Call relationships

Who calls `clearCart`?

```bash
rgctl -r "$REPO" callers clearCart
```

```text
checkout
clearCart
```

Or enumerate `calls` edges explicitly:

```bash
rgctl -r "$REPO" -f json relations clearCart --edge calls --direction in | jq '.edges[].source.name'
```

### Common node / edge types

- Node types for `--type`: `function`, `class`, `interface`, `module`, `file`, `kantrarule`, …
- Edge types for `--edge`: `calls`, `imports`, `contains`, `depends_on`, `implements`, `references`, `violates`, …

### Named communities (analysis overlay)

`discover` runs label-propagation community detection and stores assignments in
`.rgctl/analysis_results.bin` — **not** as edges in the topology graph.

Community detection uses behavioral edges (`Calls`, `Uses`, `References`) by default.  
On mixed code + markdown repos, doc `REFERENCES` participate in the same community pass as code edges; this is expected behavior.

```bash
rgctl -r "$REPO" -f json communities list | jq '.communities[:3]'
```

```json
{
  "communities": [
    {
      "id": 385,
      "label": "ecommerce.service::checkout",
      "member_count": 19
    }
  ],
  "schema_version": 1
}
```

```bash
# Bucket sizes by community id
rgctl -r "$REPO" -f json inventory --by community | jq '.counts[:5]'

# Refresh heuristic labels (--write updates analysis_results.bin)
rgctl -r "$REPO" communities list
rgctl -r "$REPO" communities label --write
```

Use each community **label** (often `package::symbol`) as a `--scope` prefix on `find` to explore members. Labels are **heuristic** — not ground-truth domain names. See
[community query & naming plan](design/community-query-and-naming-plan.md).

---

## 7. Blast radius (change impact)

`blast-radius` answers: **“What breaks upstream if I change this symbol?”**

Bare names are often ambiguous. Prefer **FQN** (`Class::method`):

```bash
rgctl -r "$REPO" blast-radius 'CartService::clearCart'
```

```text
Blast radius for 'CartService::clearCart'
  Score: 25.1/100
  Direct callers: 1
  Impact zone: 1
  Callers: OrderService.checkout
  Impact: OrderService.checkout
```

Ambiguous bare name shows remediation:

```bash
rgctl -r "$REPO" blast-radius clearCart
```

```text
Error: Symbol 'clearCart' is ambiguous. Found 2 matches.
UUID                                   | Class Context  | Source File Path
…                                      | CartService    | …/CartService.java
…                                      | CartController | …/CartController.java

Remediation: Refine your search query using a fully qualified namespace syntax:
  rgctl blast-radius "ClassName::clearCart"
  rgctl blast-radius "path/to/file.java::clearCart"
```

### Symbol forms

| Form | Example |
|------|---------|
| Bare name | `checkout` (fails if ambiguous) |
| FQN | `CartService::clearCart` |
| UUID | node id from `find` / blast JSON |

Disambiguate with filters:

```bash
rgctl -r "$REPO" blast-radius clearCart --class CartService
rgctl -r "$REPO" blast-radius clearCart \
  --file src/main/java/com/example/ecommerce/service/CartService.java
```

### Limit caller depth

```bash
rgctl -r "$REPO" blast-radius 'CartService::clearCart' --depth 1
rgctl -r "$REPO" blast-radius 'CartService::clearCart' --depth 5
```

Omit `--depth` for full transitive upstream closure.

### JSON output

```bash
rgctl -r "$REPO" -f json blast-radius 'CartService::clearCart' \
  | jq '{score: .metrics.score, callers: .topology.direct_callers}'
```

<!-- ug-scenario:07-blast-clearCart -->
```json
{
  "score": 25.05,
  "callers": [
    {
      "file_path": "…/OrderService.java",
      "fqn": "com.example.ecommerce.service.OrderService.checkout",
      "id": "…"
    }
  ]
}
```
<!-- /ug-scenario:07-blast-clearCart -->

Schema: [json-api.md](json-api.md) (blast-radius + field catalogs).

### Statement-level slice hand-offs (slow)

```bash
rgctl -r "$REPO" blast-radius 'CartService::clearCart' --with-slices
```

Requires `discover --with-cfg` for rich PDG context.

---

## 8. Program slicing and taint

`slice` performs **line-level** backward or forward slicing on a source file. Paths may be absolute, cwd-relative, or relative to `--repo`. Run `discover --with-cfg` first so PDG data is available.

### Backward slice

“What code influences this variable at this line?” — in `OrderService.checkout`, `cart` is assigned on line 52:

```bash
rgctl -r "$REPO" slice \
  src/main/java/com/example/ecommerce/service/OrderService.java \
  --line 52 \
  --variable cart \
  --function checkout
```

```text
Backward slice for src/main/java/com/example/ecommerce/service/OrderService.java:52 (variable: cart)
Reduction: 92.3%
  52
```

A denser example from `CartService.addItem` (line 53, local `item`):

```bash
rgctl -r "$REPO" slice \
  src/main/java/com/example/ecommerce/service/CartService.java \
  --line 53 \
  --variable item \
  --function addItem
```

```text
Backward slice for src/main/java/com/example/ecommerce/service/CartService.java:53 (variable: item)
Reduction: 92.9%
  53
```

### Forward slice

```bash
rgctl -r "$REPO" slice \
  src/main/java/com/example/ecommerce/service/CartService.java \
  --line 38 \
  --variable cart \
  --function addItem \
  --direction forward
```

### Taint trace

```bash
rgctl -r "$REPO" slice \
  src/main/java/com/example/ecommerce/service/OrderService.java \
  --line 83 \
  --variable cartService \
  --function checkout \
  --taint
```

### View modes

| `--view` | Description |
|----------|-------------|
| `text` | Summary (default) |
| `cfg` | CFG overlay — use with `-f mermaid` or `-f graphviz` |
| `pdg` | PDG overlay |

```bash
rgctl -r "$REPO" -f mermaid slice \
  src/main/java/com/example/ecommerce/service/CartService.java \
  --line 53 --variable item --function addItem --view cfg
```

### `--function` names

`--function` must be the **method/function name** in the source file (as parsed by tree-sitter), not the enclosing class name:

```bash
rgctl -r "$REPO" find checkout --type function --exact
```

---

## 9. Inspect CFG / PDG / dominance

`inspect` dumps semantic layers for an **indexed function symbol**. Prefer a unique name, or the same
`path/to/file.ts::symbol` / `Class::method` form as `blast-radius`. Run `discover --with-cfg` first.

```bash
rgctl -r "$REPO" inspect checkout cfg
rgctl -r "$REPO" inspect "fixtures/caseB_one.ts::dup" cfg
```

```text
CFG for checkout: 5 blocks, 5 edges
```

```bash
rgctl -r "$REPO" -f json inspect checkout cfg | jq '{layer, blocks: (.nodes|length), edges: (.edges|length)}'
```

```json
{
  "layer": "cfg",
  "blocks": 5,
  "edges": 5
}
```

Mermaid CFG:

```bash
rgctl -r "$REPO" -f mermaid inspect checkout cfg
```

```text
flowchart TD
  462c1054-… --> 14712608-…
  462c1054-… --> ae5a5a76-…
  14712608-… --> 897883b6-…
  ae5a5a76-… --> 897883b6-…
  897883b6-… --> 4165ce10-…
```

Other layers:

```bash
# Prune unreachable blocks
rgctl -r "$REPO" inspect checkout cfg --prune

# Program dependence graph (data edges)
rgctl -r "$REPO" inspect checkout pdg --edge-layer data
# → PDG for checkout: 13 nodes, 22 data deps, 0 control deps

rgctl -r "$REPO" inspect checkout pdg --def-use
rgctl -r "$REPO" inspect checkout dom --frontiers
```

---

## 10. Hybrid CPG (`cpg`)

The `cpg` façade bridges the **repo call graph** (L_repo) with the **per-function CFG/PDG archive** (L_proc) built by `discover --with-cfg`. Use it for typed field mutations, data flows, and Joern-style handoffs without stitching several CLI tools yourself.

Requires a prior `discover … --with-cfg` (the ecommerce walkthrough already uses that flag).

### Status and CALL neighborhood

```bash
rgctl -r "$REPO" cpg status
# → CPG L_proc: ready (… functions) at …/cfg_pdg.archive.bin
# → CPG field writes: N indexed (cpg mutations)

rgctl -r "$REPO" cpg function priceShoppingCart
rgctl -r "$REPO" cpg calls 'ShoppingCartService::priceShoppingCart'
```

### Field mutations (CoolStore `ShoppingCart`)

Find non-constructor writes to a type — useful before converting a mutable DTO/cart model to an immutable record, or to prove pricing still mutates totals:

```bash
rgctl -r "$REPO" cpg mutations --type ShoppingCart --exclude-ctors
```

Example (paths shortened):

```text
Mutations of ShoppingCart [excl. ctors] (7 hits):
  …/coolstore/model/ShoppingCart.java:61  this.cartTotal = cartTotal
  …/coolstore/model/ShoppingCart.java:45  this.cartItemTotal = cartItemTotal
  …
```

Pair with blast-radius on the CoolStore pricing entrypoint:

```bash
rgctl -r "$REPO" blast-radius 'ShoppingCartService::priceShoppingCart'
# → Callers include CartEndpoint.add / delete / checkout and checkOutShoppingCart
```

**Dashboard:** after `discover --with-cfg --with-dashboard`, the **Dataflow** tab includes a **Field mutations (CPG)** panel (same filters). Click a hit to open that function’s PDG and highlight the write line. See [Dashboard user guide](dashboard-user-guide.md#dataflow).

JSON for agents:

```bash
rgctl -r "$REPO" -f json cpg mutations --type ShoppingCart --exclude-ctors
```

Empty result means no **typed** non-ctor writes were recovered (receivers without a resolved type are omitted unless `--include-unresolved`). On C fixtures, query the struct typedef name (e.g. `shopping_cart_t`). See [agent-recipes.md](agent-recipes.md) Recipe 11 and [hybrid-cpg-plan.md](design/hybrid-cpg-plan.md).

### Flows, AST, export

```bash
# Forward flows from a variable at a line (wraps slice; optional --with-alias)
rgctl -r "$REPO" -f json cpg flows \
  src/main/java/com/example/ecommerce/coolstore/service/ShoppingCartService.java \
  --line 75 --variable sc --function priceShoppingCart --direction forward

# Optional: discover --with-ast-skeleton then:
rgctl -r "$REPO" -f json cpg ast priceShoppingCart

rgctl -r "$REPO" cpg export --format graphson --output /tmp/ecommerce-cpg.json \
  --path-contains coolstore/
```

---

## 11. Graph metrics

`metrics` reports network analytics on the indexed call graph. Prefer **JSON** for scripting (text mode prints debug-style structs).

```bash
rgctl -r "$REPO" -f json metrics --communities | jq .
```

```json
{
  "communities": {
    "assignments": 518,
    "count": 442,
    "modularity": 0.49
  },
  "schema_version": 1
}
```

That summary is counts only. For **named** communities and membership, use `communities list`
([§6](#6-structured-graph-queries)) or `.rgctl/dashboard/communities.json` after `--with-dashboard`.

```bash
rgctl -r "$REPO" -f json metrics --pagerank | jq '.pagerank | {iterations, converged, top: .top[:3]}'
```

```json
{
  "iterations": 20,
  "converged": false,
  "top": [
    { "node": "…uuid…", "pagerank": 0.0027 },
    { "node": "…uuid…", "pagerank": 0.0015 },
    { "node": "…uuid…", "pagerank": 0.0015 }
  ]
}
```

```bash
rgctl -r "$REPO" metrics --betweenness
rgctl -r "$REPO" -f json metrics --pagerank --iterations 50 | jq .
```

---

## 12. Semantic search

Semantic search is **opt-in** — it does not run during `discover`. Build a separate Hamming index, then query by natural language or keywords.

**Default index scope:** `:Function` symbols. **Doc headings:** `semantic index --scope docs` (or `--scope all` for functions + docs). See [markdown-context.md](markdown-context.md#semantic-search-doc-sections).

**Prerequisites:** `discover` completed. Default embedder is **vocab** (compiled token table, no ONNX). Quality extra: `--embedder code-daemon` (needs `git lfs pull` for bundled ONNX). CI smoke: `--embedder hash`.

```bash
# Build semantic index (default: vocab, 256-d, declaration metadata only)
rgctl -r "$REPO" semantic index

# Incremental rebuild — reuse rows when body hash unchanged
rgctl -r "$REPO" semantic index --incremental

# Query (JSON for agents). Late fusion is ON by default.
rgctl -r "$REPO" -f json semantic query "shopping cart checkout" --limit 10
rgctl -r "$REPO" -f json semantic query "OrderService" --keyword-and
# Pure Hamming (disable fusion):
rgctl -r "$REPO" -f json semantic query "OrderService" --no-fusion --limit 10

# Community-scoped search — pool member embeddings (needs discover analysis + semantic index)
rgctl -r "$REPO" -f json semantic query "shopping cart" --scope community --limit 5

# Doc section search (markdown headings; needs discover -l markdown)
rgctl -r "$REPO" semantic index --scope docs --embedder hash
rgctl -r "$REPO" -f json semantic query "checkout flow" --scope docs --limit 10

# Hash embedder (no ONNX) — e.g. CI
rgctl -r "$REPO" semantic index --embedder hash

# Vocab is the default; optional call-graph diffusion
rgctl -r "$REPO" semantic index --diffuse \
  --diffuse-alpha 0.25 --diffuse-iters 2

# Re-read function bodies into the vector (off by default; fusion still uses discover token-blooms)
rgctl -r "$REPO" semantic index --embed-bodies

# Distill our token list through a teacher (rebuild after copying to assets/vocab_matrix.bin)
rgctl -r "$REPO" semantic distill --matrix crates/rgctl-analysis/assets/vocab_matrix.bin --embedder code-daemon

# Neural code retriever (ONNX)
rgctl -r "$REPO" semantic index --embedder code-daemon
```

Passing `--diffuse` recomputes dense vectors and mixes call-graph neighbors **before** sign quantization (even when `--incremental` would otherwise reuse bits). Query does not re-diffuse — restart is not required for CLI query; for the dashboard, restart `serve` after rebuilding the index.

**Doc semantic index:** `--scope docs` on `semantic index` embeds `:Module` sections (`kind=heading` and `kind=code_block`). Query `--scope docs` does **not** filter hits — only `semantic index --scope community` changes query behavior. Build a doc-scoped index before querying doc sections. Large bodies use `content_store.bin` when `body_ref` is set. CLI success text may still say `Indexed N functions` (entry count, not always functions).

| Flag | Purpose |
|------|---------|
| **`semantic index --scope`** `function\|docs\|all` | Which symbols to embed (default: functions only) |
| **`semantic query --scope`** `function\|community\|docs\|all` | `community` = pooled community search; other scopes do not filter hits (index content determines results) |
| `--no-fusion` | Disable late fusion (default is fusion **on**: blast, PageRank, name, token-bloom, community, package, callees) |
| `--keyword-and` | Every query token must match metadata or body sketch |
| `--candidate-pool <N>` | Hamming pool size before fusion [default: 256] |
| `--expand neighbors\|blast\|all` | Hybrid expansion after top hits |
| `--embedder hash\|vocab\|onnx\|code-daemon` | Embedding backend [default: `vocab`] |
| `--embed-bodies` | Append identifier tokens from function source (off by default) |
| `--dimensions <N>` | Float width before quantize; multiple of 8 [default: 256] |
| `semantic distill --matrix <PATH>` | Write RBVK from our token list via a teacher (`code-daemon` default; not `vocab`) |
| `--diffuse` / `--no-diffuse` | Jacobi call-graph mix on dense floats before quantize (index only; off by default) |
| `--diffuse-alpha` / `--diffuse-iters` | Diffusion blend weight and iterations [defaults: 0.25, 2] |
| `--diffuse-bidirectional` | Include callers as well as callees |

**Dashboard:** `rgctl serve --open` → **Search** tab uses the same index via `/api/semantic/*`. The UI does not choose the embedder — build the index with CLI first, then restart `serve`. Status shows `model_id` (e.g. `vocab-accumulate-v1`).

**Perf note (linux-scale):** time queries with a **release** binary (`cargo build --release`). Debug builds can be ~100× slower on Hamming scan. Index load of `.rgctl/semantic_index.bin` is bincode into owned strings (~tens of seconds at ~1.8M functions); query itself is ~few ms in release.

Design → **[Semantic search design](design/semantic-search-design.md)** · timing tests → `cargo test --test semantic_query_timing -- --nocapture`

---

## 13. Export graph projections

`export` writes the graph or a **filter-selected** subgraph to a file or directory. The `--query` flag uses **filter syntax** (`name:Foo`, `type:Function`, …) — not structured `find` patterns (JSON/graph formats honor the filter; Obsidian/OKF use `--query all`):

| `--export-format` | Output | `--query` |
|-------------------|--------|-----------|
| `json` | Graph snapshot JSON | Filter or `all` |
| `graphml` | GraphML XML | Filter or `all` |
| `graphviz` | DOT | Filter or `all` |
| `mermaid` | Mermaid flowchart | Filter or `all` |
| `obsidian` | Obsidian vault **directory** | `all` (heading modules) |
| `okf` | OKF JSON entity bundle | `all` |

| Query | Meaning |
|-------|---------|
| `all` | Entire graph |
| `name:clearCart` | Nodes with exact name |
| `type:Function` | All functions |
| `functions` | Shortcut for function nodes |

```bash
rgctl -r "$REPO" export \
  --export-format mermaid \
  --export-output cart-clear.mmd \
  --query "name:clearCart"
```

```text
Exported 2 nodes, 1 edges -> cart-clear.mmd
```

```bash
# Full graph as JSON / GraphML / DOT
rgctl -r "$REPO" export --export-format json --export-output ecommerce-graph.json --query all
rgctl -r "$REPO" export --export-format graphml --export-output ecommerce.graphml --query all
rgctl -r "$REPO" export --export-format graphviz --export-output calls.dot --query all
```

### Markdown → Obsidian vault

For documentation repos (or `-l markdown` discover), export heading sections as an Obsidian vault. Requires prior `discover` (creates `.rgctl/` and `content_store.bin` for large bodies).

```bash
export REPO=/path/to/docs-repo
rgctl -r "$REPO" discover -l markdown
rgctl -r "$REPO" export \
  --export-format obsidian \
  --export-output "$REPO/vault" \
  --query all
```

Open `$REPO/vault` in Obsidian (**Open folder as vault**). Each note is one heading section; YAML `qualified_name` maps back to graph `module` nodes (`find` / `relations`). Re-run `discover` + `export` after doc changes.

Other doc export formats: `--export-format okf` (JSON entity bundle). Full walkthrough: [markdown-context.md](markdown-context.md#obsidian-vault-export).

For structural lookups on docs, use `find` / `relations` — see [markdown context](markdown-context.md). For browsing, use `serve` + the dashboard ([HTTP Server and Dashboard](guides/http-server-and-dashboard.md)).

---

## 14. CI policy check

`check` evaluates blast-radius policy rules against functions changed in the current git working tree (or all functions if git is unavailable).

Example policy files: [docs/examples/policy-strict.json](examples/policy-strict.json). Format: [policy-format.md](policy-format.md).

```bash
rgctl -r "$REPO" check --policy-file policy.json
```

Exit code **1** when violations are found — suitable for CI pipelines.

The fixture also ships a shared policy at [`rgctl-tests/rgctl-policy.json`](../rgctl-tests/rgctl-policy.json).

---

## 15. HTTP server (`serve`) — optional

`serve` binds HTTP immediately and, unless `--no-pipeline`, starts the same staged full pipeline as `discover --full`. The dashboard at `/` shows a preparing page until the bundle exists. Prefer CLI `-f json` for agents and CI.

```bash
# Starts indexing if needed; preparing page until dashboard exists
rgctl -r "$REPO" serve --port 8080

# Old fail-fast (require existing artifacts)
rgctl -r "$REPO" serve --no-pipeline --query-only
```

| Endpoint | Purpose |
|----------|---------|
| `/` | Dashboard UI or preparing page |
| `GET /api/status` | Full-pipeline status JSON |
| `POST /api/semantic/query` | Semantic search (primary HTTP query for agents) |
| `GET /api/semantic/status` | Semantic index availability |
| `POST /api/semantic/query` | Semantic search (JSON body) |
| `/api/health` | Health check |

```bash
curl -sS http://127.0.0.1:8080/api/status | jq .
curl -sS -X POST http://127.0.0.1:8080/api/semantic/query \
  -H 'Content-Type: application/json' \
  -d '{"query":"checkout cart","limit":5}' | jq '.hits[:3]'
```

Full reference: [HTTP Server and Dashboard](guides/http-server-and-dashboard.md). CoolStore walkthrough: [HTTP Server and Dashboard](guides/http-server-and-dashboard.md).

---

## 16. Recommended workflow

```bash
# 1. Point at the in-tree fixture
cd /path/to/rgctl
export REPO="$PWD/rgctl-tests/ecommerce-java"
cd "$REPO"

# 2. Index (add CFG + dashboard + Kantra for the rest of this walkthrough)
rgctl discover . -l java -e target \
  --with-cfg --with-dashboard --with-harmonic --with-kantra --export-migration-hints

# 3. Explore structure
rgctl -r "$REPO" -f json find --type function --count-only | jq '.total'
rgctl -r "$REPO" -f json communities list | jq '.communities[:5]'
rgctl -r "$REPO" communities list | head -15
rgctl -r "$REPO" callers clearCart

# 4. Change-impact before editing
rgctl -r "$REPO" blast-radius 'CartService::clearCart'
rgctl -r "$REPO" -f json blast-radius 'CartService::clearCart' | jq '.metrics'

# 5. CoolStore dual API + hybrid CPG (field mutations)
rgctl -r "$REPO" cpg status
rgctl -r "$REPO" cpg mutations --type ShoppingCart --exclude-ctors
rgctl -r "$REPO" blast-radius 'ShoppingCartService::priceShoppingCart'

# 5b. Konveyor Kantra migration rules (optional target filter)
rgctl -r "$REPO" -f json find --type kantrarule --limit 5 | jq '.returned'
rgctl -r "$REPO" -f json relations --edge violates --from-type kantrarule --limit 10 | jq '.returned'
jq '{violations: (.violations|length), evaluated_rules, target_filter}' \
  "$REPO/.rgctl/kantra_findings.json"

# 6. Architectural hotspots
rgctl -r "$REPO" -f json metrics --communities | jq .
rgctl -r "$REPO" -f json metrics --pagerank | jq '.pagerank.top[:5]'

# 7. Deep dive on checkout
rgctl -r "$REPO" inspect checkout cfg
rgctl -r "$REPO" slice \
  src/main/java/com/example/ecommerce/service/CartService.java \
  --line 53 --variable item --function addItem

# 8. Export / dashboard
rgctl -r "$REPO" export --export-format mermaid \
  --export-output clearCart.mmd --query 'name:clearCart'
rgctl -r "$REPO" serve --open
```

Migration hints (with `--export-migration-hints`) land under `.rgctl/migration_plan.json` and `.rgctl/dashboard/migration_plan.json` — package-level steps such as `com.example.ecommerce.service`, `…repository`, `…controller`, and CoolStore `…coolstore.*`.

---

## 17. Command reference

| Command | Purpose |
|---------|---------|
| `discover` | Index repo, build `.rgctl/` artifacts (`--full` = staged CFG/dashboard/harmonic + semantic) |
| `find` / `callers` / `callees` / `relations` / `inventory` / `status` | Structured graph queries ([§6](#6-structured-graph-queries)) |
| `communities` | List / refresh heuristic community labels |
| `blast-radius` | Upstream call-graph impact for a symbol |
| `slice` | Line-level program slice or taint trace |
| `inspect` | CFG / PDG / dominance for a function |
| `cpg` | Hybrid CPG: status, mutations, flows, calls, export (needs `--with-cfg`) |
| `metrics` | PageRank, betweenness, communities summary |
| `export` | Serialize graph (json, graphml, dot, mermaid, obsidian vault, okf) |
| `check` | CI policy gateway |
| `install` | Copy the bundled agent pack (single skill `rgctl`) into adapter dirs |
| `semantic` | Opt-in semantic index + query (`--scope community`, `docs`, `all`) |
| `serve` | HTTP dashboard + `/api/query` + `/api/status` (auto full pipeline); `--no-pipeline` fail-fast |

### `discover` flags

| Flag | Description |
|------|-------------|
| `-l, --languages` | Comma-separated filter (`java`, `typescript`, `rust`, …) |
| `-e, --exclude` | Comma-separated path exclude patterns |
| `-v, --verbose` | Debug logging + stage profile lines |
| `--with-security` | Secret scanning |
| `--with-cfg` | CFG / PDG (not taint); alias `--cfg` |
| `--with-taint` | Discover-time taint (implies CFG pass) |
| `--taint-rules PATH` | Extra taint YAML pack (file or dir; after built-ins / `.rgctl/taint-rules.d/`) |
| `--with-dfg-loops` | Tag loop-carried `DataDependency` edges in PDG (with `--with-cfg`) |
| `--with-ast-skeleton` | Build AST skeleton archive for `cpg ast` |
| `--with-harmonic` | Harmonic centrality (default off; needed for migration ranking) |
| `--with-dashboard` | Static dashboard bundle (default off) |
| `--full` | Staged pipeline: basic discover, then CFG+dashboard+harmonic, then semantic index |
| `--export-migration-hints` | Migration roadmap JSON (alias `--export-migration-plan`) |
| `--migration-preset` | Preset for migration hints (`hybrid`, `foundational`, …) |
| `--migration-order` | `scheduled` (topological) or `priority` |
| `--write-json-graph` | Also write legacy `graph.db` / `graph.json` |
| `--with-kantra` | Konveyor Kantra eval + rules graph index (embedded catalog by default) |
| `--kantra-target NAME` | Filter eval to `konveyor.io/target=NAME` |
| `--kantra-rules DIR` | Override embedded catalog with one ruleset directory |
| `--kantra-catalog ROOT` | Override embedded catalog with a rulesets tree |
| `--kantra-index-only` | Index rules into graph; skip eval |

There is no umbrella `--all` flag — combine `--with-cfg --with-security --with-taint` explicitly when you want the former deep pass.

---

## 18. Troubleshooting

### Indexed the wrong repository

If you ran `rgctl -r /path/to/repo discover .` from another directory, artifacts reflect **shell cwd**, not `-r`. Fix:

```bash
cd /path/to/repo && rgctl discover .
# or: rgctl -r /path/to/repo discover    # no trailing .
```

### `Graph not found` / `run discover first`

```bash
rgctl discover . -l java -e target
# or
rgctl -r "$REPO" -f json status
```

### Symbol not found / ambiguous (`blast-radius`, `inspect`)

List exact names, then use FQN:

```bash
rgctl -r "$REPO" find clearCart --type function --exact
rgctl -r "$REPO" blast-radius 'CartService::clearCart'
rgctl -r "$REPO" blast-radius clearCart --class CartService
```

`inspect` takes a **function** name (`checkout`, `addItem`), not a class name (`CartService`).

### Slice parse / PDG errors

Ensure you ran `discover --with-cfg`, then pass the method name and a variable that exists on that line:

```bash
rgctl -r "$REPO" slice \
  src/main/java/com/example/ecommerce/service/CartService.java \
  --line 53 --variable item --function addItem --language java
```

### Empty `cpg mutations`

Confirm `cpg status` shows a field-write index, then match the **resolved type name** (Java/C#/…: `ShoppingCart`; C: `shopping_cart_t`). Setters count as mutation sites; unresolved receivers are omitted unless `--include-unresolved`. Re-run `discover --with-cfg` after adding CoolStore sources.

### Slow `discover`

Start with the default mode. Add `--with-cfg` or `--with-taint` only when you need inspect, slice overlays, or taint. Keep `--with-harmonic` / `--with-dashboard` off unless you need migration ranking or the static UI.

On **very large repos** (500k+ graph nodes), discover automatically:

- Caps PageRank iterations and relaxes convergence tolerance
- Caps HyperBall harmonic rounds (when `--with-harmonic`) and parallelizes propagation
- Skips per-function rows in `function_metrics.json` (community/metagraph view instead)
- Uses on-demand blast reachability for flat call graphs (no eager multi-hundred-GB bitsets)

Profile a cold run (delete in-repo artifacts first):

```bash
rm -rf .rgctl
rgctl discover . -v   # from repo root
# or:
RUST_LOG=info,profile=info rgctl discover . -v 2>&1 | grep '\[profile\]'
```

### Further reading

- [Introduction](Introduction.md) — concepts and feature goals
- [cli-getting-started.md](cli-getting-started.md) — deprecated stub (use this User Guide)
- [HTTP Server and Dashboard](guides/http-server-and-dashboard.md) — dashboard HTTP API
- [json-api.md](json-api.md) — machine-readable output + field catalogs
- [USER_AGENTS_TEMPLATE](agents/USER_AGENTS_TEMPLATE.md) — paste into consumer repos’ `AGENTS.md`
- [AGENTS.md](../AGENTS.md) — contributor agent README for this repository
- [agent-skill](guides/agent-skill.md) — skill install
- [`rgctl-tests/README.md`](../rgctl-tests/README.md) — all language fixtures + correctness suite
