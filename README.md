# rBuilder

**A code knowledge graph built for LLM agents — accurate answers, minimal tokens, maximum speed.**

AI coding agents default to reading files sequentially. That burns context, misses structure, and produces confident wrong answers about impact and dependencies. **rBuilder indexes the whole repository once** into a rich graph with pre-computed **reachability**, then serves **compact, deterministic query results** — so agents (and humans) get the right slice of the codebase without loading it into the prompt.

[![CI](https://github.com/sshaaf/rBuilder/actions/workflows/ci.yml/badge.svg)](https://github.com/sshaaf/rBuilder/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## What the **R** stands for

| **R** | Meaning |
|-------|---------|
| **Rust** | Memory-safe, predictable performance at scale — the foundation for parsing large monorepos without blowing the heap |
| **Reachability** | Pre-computed call reachability (sparse bitsets, not multi‑GB dense matrices) so “what breaks if I change this?” stays sub-second |
| **Rich** code graph | 30+ typed relations — CALLS, IMPORTS, CONTAINS, IMPLEMENTS, and more — not just files and folders |

Together: **rBuilder** is the **reachability builder** — it constructs the graph and the compressed reachability engine agents need for trustworthy structural reasoning.

---

## Built for agents

**Goal:** make LLM-assisted development **more accurate** while **using fewer tokens**.

| Without rBuilder | With rBuilder |
|------------------|---------------|
| Agent reads dozens of files to guess dependencies | Agent calls `blast-radius Symbol` → structured impact JSON |
| “What calls this?” requires search + inference | `gql` returns exact graph matches |
| Migration planning from partial context | **Migration planner** — package roadmap, dual ordering, tunable scores, interactive graph |
| Repeated file dumps every turn | One `discover`, then cache-backed queries via CLI or `-f json` |

rBuilder answers **reachability and relation questions deterministically** from the indexed graph. The LLM reasons on **summaries and facts**, not raw repo grep — fewer tokens, less hallucination, faster turns.

**Primary outputs for agents:** `-f json` on `discover`, `gql`, `blast-radius`, `metrics`, and `export`. See **[JSON API](docs/json-api.md)**.

---

## Where most tools stop

Most codebase tools stop at **text search**, **file trees**, or a **shallow call graph**. rBuilder goes further — compiler-grade structure and security analysis, pre-computed at index time, queryable in milliseconds. That is what makes agent answers trustworthy.

| Feature | What it gives you | Typical tools |
|---------|-------------------|---------------|
| **[Blast radius](docs/Introduction.md#blast-radius-change-impact)** | Pre-computed **reachability** over the call graph — upstream impact, scores, policy gates, sub-second on large repos | Text grep or manual “find references”; no compressed reachability engine |
| **[Program slicing](docs/Introduction.md#program-slicing)** | **Backward / forward slice** — only the statements that affect (or are affected by) a line and variable | Whole-file context dumps; no PDG-backed minimal slice |
| **[Taint analysis](docs/Introduction.md#taint-analysis)** | **Source → sink** flows (HTTP params → SQL, shell, render, …) with sanitizer awareness | Regex heuristics; no intra-procedural dataflow |
| **[CFG](docs/Introduction.md#cfg-pdg-and-dominance-deep-structure)** | **Control-flow graph** per function — branches, loops, executable paths | No control-flow layer |
| **[PDG](docs/Introduction.md#cfg-pdg-and-dominance-deep-structure)** | **Program dependence graph** — data and control deps between statements; foundation for slice and taint | No dependence graph |
| **[Dominance](docs/Introduction.md#cfg-pdg-and-dominance-deep-structure)** | **Dominator trees** and frontiers — the same structures compilers use for advanced analysis | Not exposed in developer tools |
| **[GQL](docs/Introduction.md#graph-queries-gql)** | **Graph query language** over 30+ relation types — inventory, call chains, patterns | SQL-on-files or ad-hoc AST scripts |
| **[Graph metrics](docs/Introduction.md#graph-metrics-architecture-hotspots)** | **PageRank**, **betweenness**, **communities** on the live call graph | Ad-hoc scripts; no unified hotspot pipeline |
| **[Migration planner](docs/migration-planner-design.md)** | **Package-level roadmap** — PageRank + harmonic centrality − blast radius; dependency-aware schedule and priority rank; ForceAtlas2 graph in the dashboard | Spreadsheets and guesswork; no unified package graph + ordering pipeline |
| **[CI policy checks](docs/Introduction.md#ci-policy-checks)** | **`check`** — fail builds when blast-radius rules are violated on touched symbols | No governance tied to impact analysis |

All of the above share one index: run [`discover`](docs/Introduction.md#indexing-the-repository-discover) once (use [`discover --cfg`](docs/Introduction.md#indexing-the-repository-discover) or `--all` for full CFG/PDG/taint archives and migration exports). Explore in the CLI, pipe **JSON** to agents, or open the **[dashboard](docs/Introduction.md#dashboard-visual-exploration)** (including the **Migration** tab).

**Deep dive on every feature → [Introduction](docs/Introduction.md)**

---

## Speed by design

rBuilder is **async and parallel by design** — discovery walks the tree, parses languages concurrently, and builds analytics on the graph in parallel (Rayon + Tokio throughout the pipeline).

- **Full discovery in seconds** on typical repos (not minutes of ad-hoc agent exploration)
- **Reachability compressed** — enterprise-scale call graphs stored in compact on-disk snapshots, not gigabytes in RAM
- **Query daemon (`serve`)** — keep mmap snapshots warm for hundreds of blast-radius calls in agent loops without cold-start cost

Index once → query many times. That is the agent workflow.

```text
  Agent / script / human
           │
           ▼
    rbuilder gql | blast-radius | metrics | export -f json
           │
           ▼
  .rbuilder/  ← graph snapshot + reachability engine + indexes
           ▲
           │
      discover .     ← async parallel index (seconds)
           ▲
           │
     Your repository
```

---

## Code understanding and migrations

Use the features above together for **migration and modernization** work:

- **Migration planner** — package-level graph, tunable scoring presets, dependency-aware schedule vs. priority rank
- **Blast radius** + **metrics** — see fan-in and architectural hotspots before moving a service or framework
- **GQL** + **export** — inventory symbols and ship subgraphs to downstream tools
- **Slice** + **taint** — validate data-flow assumptions agents often get wrong
- **`check`** — enforce blast-radius policy in CI while agents (or humans) land changes

### Migration planner

After `discover --all`, open the dashboard **Migration** tab or export a machine-readable plan for agents and downstream tools:

*Unified view: tune α/β/γ weights and presets, explore the package call graph (Louvain-colored, size = priority), and paginate through the ordered package table.*

- **Package macro graph** — aggregates functions into path-derived package labels (Java package paths, Rust/C `/src/` modules)
- **Dual ordering** — **scheduled step** (Kahn topological sort, callee before caller) and **priority rank** (score-only)
- **Scoring** — `Priority = α·PageRank + β·Harmonic − γ·Blast`; presets include Hybrid Default, Risk Mitigation, Hotspot First
- **CLI export** — `discover --export-migration-plan` writes `migration_graph.json` and `migration_plan.json` under `.rbuilder/`

```bash
rbuilder discover . --all --export-migration-plan
rbuilder serve   # http://127.0.0.1:8080/ → Migration tab
```

Design → **[Migration planner design](docs/migration-planner-design.md)** · Workflow → **[Building a migration plan](docs/building-migration-plan.md)**

Walkthrough on a real Java repo → **[coolstore example](docs/user-guide.md#3-example-project-coolstore)** (User Guide).

**Research map** — which papers rBuilder implements, which inspire the roadmap, and where to propose changes → **[Further reading](docs/further-reading.md#research-foundations-in-rbuilder)**.

---

## Quick start

**Install** from [GitHub Releases](https://github.com/sshaaf/rBuilder/releases) or build from source:

```bash
git clone https://github.com/sshaaf/rBuilder.git
cd rBuilder
cargo build --release
```

**Discover** (build the graph + reachability caches):

```bash
git clone https://github.com/konveyor-ecosystem/coolstore.git
cd coolstore
rbuilder discover .
# agent-friendly telemetry:
rbuilder -f json discover . | jq '.metrics'
```

**Query** (compact answers instead of file dumps):

```bash
# Graph inventory for the agent
rbuilder -f json gql 'MATCH (n:Function) RETURN n LIMIT 10'

# Impact — critical before the agent edits a symbol
rbuilder -f json blast-radius ShoppingCartService

# Hotspots — where migration/refactor pain concentrates
rbuilder -f json metrics --pagerank --communities

# Package migration roadmap (graph + plan JSON for agents)
rbuilder discover . --all --export-migration-plan
```

Concepts → **[Introduction](docs/Introduction.md)** · Commands → **[User Guide](docs/user-guide.md)**

Example deep-analysis commands (after `discover --cfg`):

```bash
rbuilder inspect MyClass#myMethod          # CFG / PDG / dominance
rbuilder slice src/Foo.java --line 42 --variable x
rbuilder slice src/Foo.java --line 10 --variable req --taint
```

---

## Command reference

Quick links into **[Introduction](docs/Introduction.md)** — see [Where most tools stop](#where-most-tools-stop) for the differentiators.

| Command | Introduction |
|---------|----------------|
| `discover` | [Indexing](docs/Introduction.md#indexing-the-repository-discover) |
| `gql` | [Graph queries](docs/Introduction.md#graph-queries-gql) |
| `blast-radius` | [Blast radius](docs/Introduction.md#blast-radius-change-impact) |
| `slice` | [Program slicing](docs/Introduction.md#program-slicing) · [Taint](docs/Introduction.md#taint-analysis) |
| `inspect` | [CFG, PDG, dominance](docs/Introduction.md#cfg-pdg-and-dominance-deep-structure) |
| `metrics` | [Graph metrics](docs/Introduction.md#graph-metrics-architecture-hotspots) |
| `export` | [Export](docs/Introduction.md#export-and-sharing) |
| `check` | [CI policy](docs/Introduction.md#ci-policy-checks) |
| `serve` | [Query daemon](docs/Introduction.md#query-daemon-repeated-analysis) |

**Dashboard** — visual exploration after `discover` (`.rbuilder/dashboard/`), including the **Migration** planner tab.  
**Migration export** — `discover --export-migration-plan` (optional `--migration-preset`, `--migration-order scheduled|priority`).  
**Languages** — 35+ (Rust, Python, Java, Go, TypeScript, IaC, CI YAML, …). See [LANGUAGE_GUIDE.md](LANGUAGE_GUIDE.md).

---

## Documentation

| Document | For |
|----------|-----|
| **[Introduction](docs/Introduction.md)** | Concepts — graph, reachability, each feature |
| **[User Guide](docs/user-guide.md)** | Install, coolstore, every command |
| **[JSON API](docs/json-api.md)** | **Agent integration** — parse `-f json` |
| **[Further reading](docs/further-reading.md)** | **Research implemented vs inspired** — papers, code map, contribution ideas |
| **[CLI output schemas](docs/cli-output-schemas.md)** | Exact field tables |
| **[Performance engineering](docs/performance-engineering.md)** | Latency tiers, cache layout |
| **[Dashboard design](docs/dashboard-design.md)** | Browser UI |
| **[Migration planner design](docs/migration-planner-design.md)** | Package graph, scoring, ordering, dashboard UI |
| **[Building a migration plan](docs/building-migration-plan.md)** | End-to-end migration workflow |
| **[Releasing](docs/releasing.md)** | Tags and release workflow |

---

## Development

```bash
./scripts/ci-local.sh              # CI parity
./scripts/release-local.sh --native  # pre-tag release smoke test
```

---

## License

MIT — see [LICENSE](LICENSE).
