# Discovering and Indexing a Codebase

## Introduction

The `discover` command is the foundation of every rgctl workflow. It parses your source code, builds a **code knowledge graph** of functions, classes, modules, and their relationships (calls, contains, imports), and runs configurable analytics on the result. Every other rgctl command — `find`, `blast-radius`, `metrics`, and the rest — reads from the graph that `discover` produces.

Think of `discover` as the indexing step: you run it once (or after significant code changes), and then query the graph as many times as you like without re-parsing.

## Use Cases

- **Onboarding to an unfamiliar codebase.** Run `discover` to build an inventory of every function, class, and call relationship so you can query the structure instead of reading files one by one.
- **Pre-migration analysis.** Enable `--with-cfg`, `--with-harmonic`, and `--export-migration-hints` to generate a dependency-aware migration roadmap before refactoring a monolith.
- **CI integration.** Run `discover` in CI so downstream commands like `check` can enforce architectural policies on every pull request.
- **Deep program analysis.** Enable `--with-cfg` to build control-flow graphs, program dependence graphs, and dominator trees for every function — required by `slice`, `inspect`, and `cpg` commands.

## Example Project

This guide uses the **CoolStore** — a Java EE e-commerce application. It lives in `example/coolstore` (local clone of [konveyor-ecosystem/coolstore](https://github.com/konveyor-ecosystem/coolstore); fetch scripts may place it at `example/coolstore-weblogic` — symlink or `-r` accordingly). Prefer **`-l java`** for the backend; the tree also ships a large Angular/bower frontend.

## Choosing what to index

`discover` takes an optional `PATH` argument. How it combines with `-r` / `--repo` matters:

| Pattern | Indexes | When to use |
|---------|---------|-------------|
| `cd example/coolstore && rgctl discover .` | Current directory | **Recommended** in tutorials and day-to-day use |
| `rgctl -r example/coolstore discover` | The `-r` path (no `PATH` arg) | Scripts, agents (`export REPO=…`) |
| `rgctl discover /abs/path/to/coolstore` | Absolute path | One-shot from any cwd |

**Pitfall:** `rgctl -r example/coolstore discover .` does **not** index `example/coolstore`. The positional `.` becomes the session root (usually your **shell cwd**), so `-r` is ignored. That can scan the wrong tree and fail on large parent directories.

**Artifacts:** `discover` writes snapshots under **`{repo}/.rgctl/`**. Add `.rgctl/` to `.gitignore`.

## Step-by-Step

### 1. Basic Discovery

From the repository root:

```bash
cd example/coolstore
rgctl discover . -l java
```

Or from anywhere, without a `PATH` argument:

```bash
rgctl -r example/coolstore discover -l java
```

### Discover profiles (recommended flag sets)

| Goal | Suggested flags |
|------|-----------------|
| **Structural** (query find/callers/…) | `discover .` (default) |
| **CFG / slice ready** | `--with-cfg` (+ `--with-ast-skeleton` if needed) |
| **Full staged pipeline** | `--full` (CFG + dashboard + harmonic + semantic; not taint/security) |
| **Migration roadmap** | `--export-migration-hints --with-harmonic` (+ preset/order) |
| **Kantra** | `--with-kantra` (+ `--kantra-target` …) |
| **Dirty tree refresh** | prefer `rgctl update` / `serve --watch` — not full rediscover |

`rgctl discover --help` groups flags under Paths & filters · Pipeline features · Migration · Session / limits.

### Full pipeline

```bash
rgctl -r example/coolstore discover -l java --full
```

This prints a plan, finishes a basic (queryable) index, then runs CFG + dashboard + harmonic centrality and a vocab semantic index. `--full` does not enable taint or secret scanning.

### Constrained / container discover

```bash
rgctl discover . --with-limits max-mem-mb=4096,threads=1
```

Or set `RGCTL_WITH_LIMITS=max-mem-mb=4096,threads=1`. Soft RSS budget (~95% abort), thread cap, and smaller spill buffers — default discover stays unconstrained.

**Output:**

```
[>] rgctl discover
[✓] Loaded 26 files from snapshot -> 608 nodes, 1709 edges
[✓] rgctl discover finished in ~300ms
```

**What happened:**

- With `-l java`, rgctl indexed CoolStore’s Java sources only (**26** files → **608** nodes / **1709** edges; inventory reports **152** functions on the current tree).
- The graph snapshot was written to `example/coolstore/.rgctl/graph.snapshot.bin`.
- Basic discover finishes in well under a second on this corpus.

### 2. Discovery with Deep Analysis

To enable control-flow graphs, PDGs, and dominance analysis for every function, add `--with-cfg`:

```bash
rgctl -r example/coolstore discover -l java --with-cfg
```

**Output:**

```
[>] rgctl discover
[✓] Loaded 26 files from snapshot -> 608 nodes, 1709 edges

✓ Control flow analysis:
  Field writes indexed: 41
  CFG/PDG/Dominance: 150 functions analyzed
  Skipped: 2 functions (unsupported_language=0, missing_source=0, analysis_error=2)
[✓] rgctl discover finished in ~300ms
```

**What happened:**

- In addition to the basic graph, rgctl built a CFG, PDG, and dominator tree for each analyzed Java function (**150** on this tree; inventory still lists **152** function symbols).
- It indexed **41** field-write sites (`cpg status.field_write_count`), enabling `cpg mutations`.
- The analysis archive was written to `.rgctl/analysis/cfg_pdg.archive.bin`.
- A small number of functions may be skipped for analysis errors.

### 3. Full Analysis with Dashboard and Migration

For the most complete analysis, combine all the deep-analysis flags:

```bash
rgctl -r example/coolstore discover -l java \
  --with-cfg \
  --with-dashboard \
  --with-harmonic \
  --export-migration-hints
```

This enables:

| Flag | Purpose |
|------|---------|
| `--with-cfg` | Build CFG, PDG, and dominator trees for every function |
| `--with-dashboard` | Export a static dashboard bundle to `.rgctl/dashboard/` |
| `--with-harmonic` | Compute harmonic centrality (needed for migration ranking) |
| `--export-migration-hints` | Write `.rgctl/migration_plan.json`; with `-f json`, stdout is the migration plan (`schema_version` 2, `steps`) instead of DiscoverResponse telemetry |

Without `--export-migration-hints`, `discover -f json` emits DiscoverResponse telemetry (`command: "discover"`, `metrics`, `schema_version: 2`) — not the migration plan.

### 4. Filtering by Language

CoolStore tutorials in these guides use `-l java` (same as `--languages java`) so the Angular/bower frontend is not indexed:

```bash
rgctl -r example/coolstore discover --languages java
```

Omit `-l` / `--languages` only when you intentionally want a multi-language index of the full tree (much larger; vendor JS dominates).

### 5. Excluding Directories

To skip vendor or generated code when indexing more than Java:

```bash
rgctl -r example/coolstore discover --exclude bower_components
```

### 6. Inspecting Artifacts

After discovery, artifacts live under **`{repo}/.rgctl/`**:

| Path | Content |
|------|---------|
| `.rgctl/graph.snapshot.bin` | Columnar graph snapshot |
| `.rgctl/analysis/cfg_pdg.archive.bin` | CFG/PDG/dominance archive (with `--with-cfg`) |
| `.rgctl/dashboard/manifest.json` | Dashboard metadata (with `--with-dashboard`) |
| `.rgctl/migration_plan.json` | Migration roadmap (with `--export-migration-hints`) |

## Discover Options Reference

| Option | Description |
|--------|-------------|
| `--with-cfg` | Per-function CFG, PDG, and dominator trees |
| `--with-taint` | Discover-time taint analysis (implies `--with-cfg`) |
| `--with-dfg-loops` | Classify loop-carried data dependencies on PDGs |
| `--with-ast-skeleton` | Write coarse AST skeleton archive |
| `--with-security` | Secret scanning |
| `--with-dashboard` | Export static dashboard bundle |
| `--with-harmonic` | Compute harmonic centrality for migration ranking |
| `--export-migration-hints` | Write migration roadmap JSON (and emit it on stdout with `-f json`) |
| `--write-json-graph` | Write legacy JSON graph files |
| `-l, --languages` | Restrict to specific languages |
| `-e, --exclude` | Exclude directories by name |
| `-v, --verbose` | Debug logging with stage profiling |
| `--full` | Staged pipeline: basic → CFG/dashboard/harmonic → semantic index |

## Benefits

- **Single command to index an entire codebase.** No build system integration, no compilation required — rgctl works directly on source files.
- **Incremental depth.** Start with basic discovery in under a second, then opt into deeper analysis (`--with-cfg`, `--with-taint`) only when you need it.
- **Multi-language support.** Nine Tier 1 languages (Java, Go, Rust, Python, JavaScript, TypeScript, C#, C, C++) plus markdown and config formats.
- **Foundation for all other commands.** Every query, metric, and analysis reads from the graph `discover` produces.

## Related Guides

- [Structured graph queries](structured-query.md) — query the graph that `discover` builds
- [Graph Metrics](graph-metrics.md) — run PageRank, betweenness, and community detection
- [Migration Planning](migration-planning.md) — use the `--export-migration-hints` output
- [HTTP Server and Dashboard](http-server-and-dashboard.md) — serve the `--with-dashboard` output in a browser
