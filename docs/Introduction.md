# Introduction to rBuilder

This document explains **what rBuilder is**, how a **code knowledge graph** works, and what each capability is for — before you run commands. For install steps and copy-paste examples, use the **[User Guide](user-guide.md)**.

**Audience:** architects, team leads, security engineers, and developers who want the concepts first.  
**Hands-on next step:** [User Guide → coolstore example](user-guide.md#3-example-project-coolstore)

---

## Table of contents

1. [What problem does rBuilder solve?](#what-problem-does-rbuilder-solve)
2. [What is a code knowledge graph?](#what-is-a-code-knowledge-graph)
3. [How rBuilder fits together](#how-rbuilder-fits-together)
4. [Indexing the repository (`discover`)](#indexing-the-repository-discover)
5. [Graph queries (GQL)](#graph-queries-gql)
6. [Blast radius (change impact)](#blast-radius-change-impact)
7. [Program slicing](#program-slicing)
8. [Taint analysis](#taint-analysis)
9. [CFG, PDG, and dominance (deep structure)](#cfg-pdg-and-dominance-deep-structure)
10. [Graph metrics (architecture hotspots)](#graph-metrics-architecture-hotspots)
11. [Migration planner (package roadmap)](#migration-planner-package-roadmap)
12. [Export and sharing](#export-and-sharing)
13. [CI policy checks](#ci-policy-checks)
14. [Query daemon (repeated analysis)](#query-daemon-repeated-analysis)
15. [Dashboard (visual exploration)](#dashboard-visual-exploration)
16. [Where to go next](#where-to-go-next)

---

## What problem does rBuilder solve?

Modern codebases are too large to hold in your head. When you change a function, you need to know:

- Who calls it upstream?
- Which services or modules depend on it?
- Could this change affect security-sensitive paths?
- Where is complexity concentrated?

Reading files one by one is slow and error-prone. **rBuilder turns your repository into a structured graph** — functions, classes, calls, imports, and more — so you can **ask structural questions** and get answers in seconds instead of hours.

The tool is built in **Rust** for speed and predictable memory use: large enterprise repos (hundreds of thousands of nodes) can be indexed in one pass, with analysis results stored in compact on-disk caches rather than loading everything into RAM.

---

## What is a code knowledge graph?

Think of your codebase as a **map**, not a pile of files.

| Everyday idea | In rBuilder |
|---------------|-------------|
| Places on the map | **Nodes** — functions, classes, files, modules, config keys, … |
| Roads between places | **Edges** — typed **relations** between nodes |
| A travel guide | The **graph** stored under `.rbuilder/` after indexing |

### Relations (edges)

rBuilder records many relation types, for example:

- **CALLS** — one function invokes another  
- **CONTAINS** — a class or file holds a member  
- **IMPORTS** — dependency between compilation units  
- **IMPLEMENTS** / **DEPENDS_ON** — structural coupling  

Together, these form a **rich relation matrix**: you see not only “what exists,” but **how parts connect**.

### Reachability

Many questions are really reachability questions: *“If I change X, what else can be affected along call paths?”*

rBuilder pre-computes **reachability** over the call graph (who can reach whom upstream) and stores it in a **compressed snapshot** (sparse bitsets instead of a multi-gigabyte dense matrix). That is what makes **blast radius** queries fast on large graphs — the “R” in rBuilder aligns with **reachability** and **relations** as first-class ideas.

You do not need graph theory to use the CLI; it helps to know that **indexing builds the map**, and **commands query the map**.

---

## How rBuilder fits together

```text
  Your repo (source files)
           │
           ▼
      discover          ← scan, parse, build graph + caches
           │
           ▼
  .rbuilder/            ← graph snapshot, blast engine, indexes
           │
     ┌─────┴─────┬─────────────┬──────────────┐
     ▼           ▼             ▼              ▼
   gql      blast-radius    slice/inspect   metrics/export
                                              migration plan
```

1. **Once per repo (or after big changes):** run `discover` to build `.rbuilder/`.  
2. **Many times:** run query commands (`gql`, `blast-radius`, …) against that cache.  
3. **Optional:** open the **dashboard** for interactive exploration, or use **`-f json`** for automation ([JSON API](json-api.md)).

---

## Indexing the repository (`discover`)

### Goal

Turn a folder of source code into a **persistent, queryable graph** plus pre-computed analysis (complexity, communities, blast-radius scores, optional CFG/PDG).

### Description

`discover` walks the repository, uses language-aware parsers to extract symbols and relationships, and writes artifacts to `.rbuilder/`. This includes a binary graph snapshot (`graph.snapshot.bin`), a blast-radius engine snapshot, SQLite lookup tables, and optionally per-function control-flow and taint data when you enable deeper modes (`--cfg` or `--all`).

Default discover is tuned for speed. Deeper modes trade time for semantic detail (slicing, taint, inspect overlays).

### Key benefits

- **One command** to prepare the whole repo for all other features  
- **Incremental-friendly** file tracking for faster re-runs after small changes  
- **CI-friendly** telemetry with `-f json` (file counts, nodes, edges, duration)  
- **Optional security scan** (`--security`) and **optional CFG/PDG/taint** (`--cfg`, `--all`)
- **Optional migration roadmap** (`--export-migration-plan`, with `--migration-preset` and `--migration-order scheduled|priority`)

### How to run it

→ [User Guide §4 — Index with `discover`](user-guide.md#4-index-with-discover)

---

## Graph queries (GQL)

### Goal

**Explore and inventory** the codebase using a small graph query language — like SQL for structure, not for table rows.

### Description

**GQL** (graph query language) matches patterns in the graph: find all functions whose name contains `Cart`, list call chains between functions, or count nodes by type. Results can be human-readable text or **JSON** for scripts.

Named **macros** (`all_functions`, `direct_calls`, `call_chain`) bundle common patterns so you do not rewrite long queries.

### Key benefits

- **Fast orientation** in unfamiliar repos (“how many functions?”, “who calls whom?”)  
- **Repeatable audits** — same query on every release  
- **Automation** — pipe JSON to `jq` or your own tools  
- **No LLM required** — deterministic answers from the indexed graph

### How to run it

→ [User Guide §6 — Query the graph with GQL](user-guide.md#6-query-the-graph-with-gql)

---

## Blast radius (change impact)

### Goal

Answer: **“If I change this function or method, what breaks upstream?”** — before you merge the change.

### Description

**Blast radius** walks the **incoming call graph** (callers and transitive callers) from a chosen symbol. It returns an impact **score**, lists of **direct callers** and the wider **impact zone**, and (with JSON) stable **UUIDs** and **canonical names** for automation.

You can cap how far upstream to look (`--depth`), attach **policy files** for governance (e.g. “this change must not cross domain boundaries”), and optionally request **slice hand-offs** for line-level follow-up.

Pre-computed reachability at discover time is what keeps this sub-second on large graphs.

### Key benefits

- **Change-risk triage** before code review or release  
- **Refactoring safety** — see fan-in before renaming or deleting APIs  
- **Policy gates** — fail CI when impact crosses forbidden boundaries  
- **Structured JSON** for tickets, bots, and agent workflows

### How to run it

→ [User Guide §7 — Blast radius (change impact)](user-guide.md#7-blast-radius-change-impact)

---

## Program slicing

### Goal

Answer: **“Which lines of this function actually affect this variable at this line?”** — backward or forward through data and control dependencies.

### Description

**Slicing** is a precision tool for debugging and review. You point at a **file**, **line**, and **variable**; rBuilder computes the **slice** — the minimal set of statements that influence (or are influenced by) that point. This uses control-flow and program-dependence structure inside the function.

Slicing reads source from disk; richer cross-function context is available when the repo was indexed with `discover --cfg`.

### Key benefits

- **Narrow focus** during incident response (“what fed this value?”)  
- **Review efficiency** — less noise than reading the whole file  
- **Exportable views** — text summary, or CFG/PDG overlays with Mermaid/Graphviz

### How to run it

→ [User Guide §8 — Program slicing and taint](user-guide.md#8-program-slicing-and-taint) (slice sections)

---

## Taint analysis

### Goal

Find **unsafe flows** where untrusted input (sources) may reach dangerous operations (sinks) — e.g. HTTP parameters into SQL, or user input into shell commands.

### Description

**Taint analysis** tracks how data of interest propagates from **sources** (request parameters, files, environment variables, …) to **sinks** (SQL execution, shell, HTML render, …). Flows may be **sanitized** on the path; vulnerable flows are those with no effective sanitizer.

At CLI level, `slice --taint` gives a quick per-function check. Full-repo taint summaries are produced when you run `discover --cfg` or `--all` and appear in the dashboard **taint** tab and exported JSON indexes.

### Key benefits

- **Security review** without manual path tracing on every endpoint  
- **Severity hints** from source/sink pairing  
- **Integration** with discover pipeline for batch reporting across functions

### How to run it

→ [User Guide §8 — Program slicing and taint](user-guide.md#8-program-slicing-and-taint) (taint sections)  
→ Deeper index: `discover . --cfg` or `discover . --all` ([User Guide §4](user-guide.md#4-index-with-discover))

---

## CFG, PDG, and dominance (deep structure)

### Goal

Inspect **how code executes inside a single function** — branches, loops, data dependencies between statements, and dominance structure used by compilers and advanced analyses.

### Description

| Concept | Meaning |
|---------|---------|
| **CFG** (control-flow graph) | Blocks and branches: what can run after what |
| **PDG** (program dependence graph) | Data and control edges between statements |
| **Dominance** | Which blocks must execute before others; dominance frontiers for SSA-style reasoning |

The **`inspect`** command dumps these layers for a named function. **`discover --cfg`** must have run so the archive contains CFG/PDG data for indexed symbols.

### Key benefits

- **Compiler-minded debugging** without leaving the repo tool chain  
- **Foundation** for slice, taint, and dataflow features  
- **Diagram export** (Mermaid, Graphviz) for docs and reviews

### How to run it

→ [User Guide §9 — Inspect CFG / PDG / dominance](user-guide.md#9-inspect-cfg--pdg--dominance)

---

## Graph metrics (architecture hotspots)

### Goal

Find **structural hotspots** in the architecture — functions that are central, bridge modules, or form natural communities.

### Description

**Metrics** runs graph algorithms on the indexed call graph:

- **PageRank** — influential nodes (many important callers/callees)  
- **Betweenness** — bridge nodes on many paths  
- **Communities** — densely connected clusters (often packages or subsystems)

Discover already computes many analytics during indexing; `metrics` exposes them on demand as JSON or text.

### Key benefits

- **Prioritize refactors** where coupling is highest  
- **Onboarding** — “start reading here” for new engineers  
- **Architecture reviews** with quantitative backing

### How to run it

→ [User Guide §10 — Graph metrics](user-guide.md#10-graph-metrics)

---

## Migration planner (package roadmap)

### Goal

Answer: **“In what order should we migrate or extract packages, given centrality, blast risk, and call dependencies?”** — a concrete roadmap, not just a hotspot list.

### Description

The **migration planner** aggregates per-function metrics into **package-level macro nodes** (Java-style package paths, Rust/C `/src/` module paths), builds a **call graph between packages**, and ranks packages with a tunable score:

`Priority = α·PageRank + β·Harmonic − γ·Blast`

Two orderings are available:

- **Scheduled step** — Kahn topological sort so callees appear before callers (dependency-aware)  
- **Priority rank** — score-only ordering without dependency constraints  

Strategy **presets** (Hybrid Default, Risk Mitigation, Hotspot First) adjust α/β/γ. The dashboard **Migration** tab lets you tune weights live and explore a ForceAtlas2 layout (cluster color from Louvain communities, node size from priority). CLI export writes `migration_graph.json` and `migration_plan.json` under `.rbuilder/` when you run `discover` with `--export-migration-plan` (typically with `--all` so harmonic and blast metrics are available).

### Key benefits

- **Actionable batches** — migrate by package, not anonymous community ids  
- **Risk-aware ordering** — balance architectural importance against blast impact  
- **Dual views** — strict dependency schedule vs. pure priority for planning debates  
- **Agent-ready JSON** — same plan the dashboard shows, exportable at discover time

### How to run it

```bash
rbuilder discover . --all --export-migration-plan
rbuilder serve   # optional: warm daemon; open dashboard → Migration tab
```

→ Engineering detail: **[Migration planner design](migration-planner-design.md)**  

---

## Export and sharing

### Goal

**Take the graph (or a subgraph) out of rBuilder** for other tools — spreadsheets, GraphML viewers, documentation, or custom pipelines.

### Description

**Export** writes files in common formats: JSON (full graph), GraphML, Graphviz DOT, or Mermaid. You can export everything or restrict to a GQL-selected subgraph (e.g. only functions matching `*Cart*`).

### Key benefits

- **Interop** with existing visualization and graph tools  
- **Snapshots** for compliance or architecture baselines  
- **Custom analytics** in Python/R/Excel on exported JSON

### How to run it

→ [User Guide §11 — Export graph projections](user-guide.md#11-export-graph-projections)

---

## CI policy checks

### Goal

**Block merges** when changed code violates blast-radius or governance rules defined in a policy file.

### Description

**`check`** compares functions touched in the current git working tree (or the whole graph if git is unavailable) against a **policy file**. Violations are listed in JSON; exit code `1` signals failure for CI pipelines.

This pairs with blast-radius semantics: policies can encode scale limits, forbidden cross-domain impact, cascade hazards on high-betweenness nodes, and related rules.

### Key benefits

- **Automated governance** in pull-request pipelines  
- **Consistent enforcement** of architecture rules  
- **Machine-readable violations** for bots and dashboards

### How to run it

→ [User Guide §12 — CI policy check](user-guide.md#12-ci-policy-check)

---

## Query daemon (repeated analysis)

### Goal

Keep the graph and blast engine **loaded in memory** when you run many queries in a row (scripts, IDE integrations, agents).

### Description

**`serve`** starts a lightweight Unix-socket daemon that holds mmap snapshots warm. Subsequent `blast-radius` (and related) commands auto-connect to `.rbuilder/query.sock` unless disabled via environment variable.

Useful for interactive sessions or batch jobs that fire hundreds of impact queries; not required for occasional one-off CLI use.

### Key benefits

- **Lower latency** on repeated blast-radius and graph-backed queries  
- **Same JSON shapes** as the normal CLI  
- **Optional** — no daemon needed for first-time exploration

### How to run it

→ [User Guide §13 — Query daemon (`serve`)](user-guide.md#13-query-daemon-serve)

---

## Dashboard (visual exploration)

### Goal

**Explore** the graph interactively in a browser — package overview, drill-down, CFG, slice, blast radius, dataflow, taint, and **Migration** (package roadmap) — without memorizing CLI syntax.

### Description

After `discover`, rBuilder writes a static bundle under **`.rbuilder/dashboard/`** (`index.html`, `manifest.json`, graph payload, metagraph, migration indexes when exported, and per-feature indexes). Serve that folder over HTTP (WASM graph engine requires a real server, not `file://`).

The dashboard complements the CLI: same underlying graph and analysis artifacts. The **Migration** tab mirrors the Rust planner in TypeScript for live preset and weight changes.

### Key benefits

- **Visual navigation** for large monorepos (package metagraph, zoom, inspector)  
- **Demos and onboarding** for non-CLI users  
- **Phase-gated features** described in [dashboard-design.md](dashboard-design.md)

### How to run it

```bash
rbuilder discover .          # produces .rbuilder/dashboard/
cd .rbuilder/dashboard && python3 -m http.server 8765
# open http://localhost:8765
```

→ Install and repo setup: [User Guide §1–3](user-guide.md#1-installation)

---

## Where to go next

| If you want to… | Read |
|-----------------|------|
| Install, PATH, coolstore walkthrough, every command | **[User Guide](user-guide.md)** |
| Parse `-f json` in scripts or CI | **[JSON API](json-api.md)** |
| Exact JSON field tables | **[CLI output schemas](cli-output-schemas.md)** |
| Plan a package-by-package migration roadmap | **[Migration planner design](migration-planner-design.md)** · **[Building a migration plan](building-migration-plan.md)** |
| Dashboard architecture and phases | **[Dashboard design](dashboard-design.md)** |
| Performance tiers and benchmarks | **[Performance engineering](performance-engineering.md)** |
| Papers implemented, inspired, and contribution ideas | **[Further reading](further-reading.md#research-foundations-in-rbuilder)** |

**Suggested first hour**

1. Read this introduction (you are here).  
2. Follow [User Guide §1–4](user-guide.md#1-installation) — install, clone [coolstore](https://github.com/konveyor-ecosystem/coolstore), run `discover`.  
3. Run one **GQL** query and one **blast-radius** on a function you recognize.  
4. Optionally open the **dashboard** (try the **Migration** tab after `discover --all --export-migration-plan`) or try `-f json` with [JSON API](json-api.md).

---

*Background on naming and design philosophy: [what-is-rBuilder-thoughts.md](what-is-rBuilder-thoughts.md)*
