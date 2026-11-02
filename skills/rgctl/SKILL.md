---
name: rgctl
description: >-
  Answer structural questions about a codebase using the rgctl CLI graph
  (architecture, communities, call relationships, blast radius, data-flow
  slices, CPG, semantic search, migration roadmap, CI gates). Use when the
  user asks how code is connected, what calls what, impact of changing a
  symbol, where data flows, migration planning/roadmap, repo
  structure/hotspots, or when `.rgctl/` exists — treat natural-language
  codebase questions as rgctl queries first.
rgctl-managed: true
---

# rgctl

Answer **structural** questions from a pre-built code knowledge graph instead of reading whole files into context. Spawn **`rgctl -f json`** subprocesses (or use foreground **`rgctl serve`** for repeated HTTP queries). Artifacts live at **`{repo}/.rgctl/`** after `discover`.

## When to Use This Skill

Use rgctl when the user asks:
- **Architecture questions** — "What calls X?", "Where is the checkout flow?", "What communities exist?"
- **Impact analysis** — "What breaks if I change this function?"
- **Data flow** — "Where does this variable flow?", "Trace this tainted input"
- **Migration planning** — "Generate a migration roadmap" / "What should we extract first?"
- **Hotspots** — "What are the most central/risky functions?"
- **Subsystem mapping** — "Which module owns feature X?"

If `.rgctl/` exists, prefer rgctl queries over reading files.

## CLI for agents

```bash
export REPO=/path/to/repo
cd "$REPO" && rgctl discover .          # or: rgctl -r "$REPO" discover
rgctl -r "$REPO" -f json <command> …
```

**Critical:** Parse `schema_version` + payload from **stdout**. **Never use `2>/dev/null`** — it swallows rgctl errors.

For interactive exploration, optional: `rgctl serve --open` (dashboard). Agents should still spawn CLI structured verbs (`find` / `callers` / `relations` / `inventory` / …).

Legacy daemon cache under `~/.rgctl/cache/` is obsolete; run `rgctl discover .` in the repo to build `{repo}/.rgctl/`.

## Agent Loop

```text
1. USER PROMPT     → natural language (not a CLI string)
2. SUBPROCESS      → rgctl -f json <structured command>
3. GRAPH FACTS     → parse schema_version + payload
4. LLM REASONING   → summarize using "what to report" guidelines
5. ACTION          → edit / plan / check — re-query if graph may be stale
```

**Prerequisite (once per repo):** `discover` (and `semantic index` when using search). `rgctl slice` / `cpg flows` build single-function CFG/PDG lazily (no upfront `--with-cfg`). Reserve `discover --with-cfg` for `taint --sink`, `cpg mutations`, and batch `inspect`.

## What to Do When Invoked

1. **Help-only** — If user only wants help/command list → print workflow table below and **stop** (no discover, no queries)
2. **Fast path (existing index)** — If `.rgctl/` exists **and** request is a structural question (not rebuild) → **do not re-run full discover**. Route via workflow table; use CLI `-f json`.
   - **Staleness exception:** run `rgctl -f json status`. If `index_current` is `false`, run **`rgctl update`** once — works with or without `serve --watch` (under watch, update enqueues for the sole writer; do not stop serve). Do **not** loop full `discover` for dirty working trees.
   - **Exception — vuln / OSV / OpenVEX:** if reachability, sink-first taint, or `vuln analyze` is in scope and CFG was not built (`cpg status` / missing CFG archive), re-run `discover . --with-cfg` (add `--with-taint` for PDG-backed confidence) before P4–P6
3. **No index** — Run `cd "$REPO" && rgctl discover .` or `rgctl -r "$REPO" discover` (do **not** use `-r REPO discover .` — the `.` ignores `-r`). Add flags only when needed; for vuln scans use `--with-cfg` (see vuln workflow)
4. **Natural-language routing** — Map utterance with workflow table. Do not ask user to rephrase into CLI unless disambiguation required
5. **Summarize** — Report key facts, not raw JSON dumps
6. **Stop conditions** — Pure code-edit/debug with no structural need → do not force rgctl

**Relationship questions** (e.g., "relationship between X and Y"): resolve symbols → bounded `CALLS`/`DEPENDSON` traversal → answer in plain language (hops, shared neighbors, files). If no direct path but asymmetric dependency, fall back to `blast-radius` on each.

## Workflow Families

### 1. Discovery & Indexing

**When:** First use, or after major changes

| User Intent | CLI Command |
|-------------|-------------|
| Build graph index | `cd repo && discover .` or `rgctl -r PATH discover` |
| SCIP compiler overlay | `discover . --scip index.scip` |
| Git churn / co-change | `discover . --with-git [--git-since 90.days]` → `metrics --churn` / `relations -e cochangeswith` |
| Multi-repo merge | `merge repoA repoB --output-dir OUT` |
| Patch graph after edits | `rgctl update` (or `update --files path1,path2`) — structural only |
| Keep graph fresh while serving | `serve --watch` (debounced FS updates); still run `update` to enqueue if needed |
| Prefer canonical verbs | `find`/`callers`/`slice`/`vuln analyze` (see encyclopedia map); `query`/`cpg`/`security` are façades |
| Build semantic index | `semantic index` |
| Check CFG readiness | `cpg status` |
| Full staged pipeline | `discover . --full` |
| Markdown + code context graph | `discover . -l markdown,<lang>` then `find '<Heading>*' --type module` → `relations <MODULE_ID_OR_HEADING> --edge references --direction out` |

**Common flags:**
- `--with-cfg` — Pre-build whole-repo CFG/PDG archive (taint/mutations/batch inspect; not required for lazy `slice`)
- `--scip PATH` — Compiler-exact SCIP call overlay
- `--with-git` — Git churn + `CoChangesWith` edges
- `--with-dashboard` — Build dashboard bundle
- `--export-migration-hints` — Write `.rgctl/migration_plan.json` (primary migration deliverable)
- `--with-security --with-taint` — Security scanning
- `--with-harmonic` — Harmonic centrality (used with migration ranking)
- `-l cobol,pli,jcl,apex,pascal,perl,ada,matlab` — legacy/mainframe migration languages when those sources are present

**See:** Discovering and Indexing Guide (product docs; not bundled in this skill pack)

### 1b. Migration roadmap (`--export-migration-hints`)

**Primary deliverable for migration questions:** `.rgctl/migration_plan.json` — not ad-hoc rule dumps. Prefer slash command **`/rgctl:migrate`** (skill `rgctl-migrate`) for the full Strangler-Fig brief.

| User Intent | CLI Command |
|-------------|-------------|
| **Migration roadmap / extraction brief** | **`/rgctl:migrate`** → skill `rgctl-migrate` |
| Generate migration plan (CLI) | `discover . --export-migration-hints` (add `--with-harmonic` for ranking) |
| Preset / order | `--migration-preset hybrid_default\|foundational_first\|dense_cluster\|risk_mitigation` · `--migration-order scheduled\|priority` |
| Read plan | `.rgctl/migration_plan.json` v2 `.steps[]` (`label != "root"`); plain `discover -f json` without the flag is DiscoverResponse telemetry |
| Legacy / mainframe sources | add `-l cobol,pli,jcl` (also `apex,pascal,perl,ada,matlab` as needed) |
| Dashboard view | `discover . --with-dashboard --export-migration-hints` then `serve --open` |

**Supporting probes** (optional): `communities list` (`member_count > 1`) → `blast-radius` on boundary candidates. Bound `blast-radius` with `--depth 2`/`3` and filter `communities list` with `jq` (neither accepts `--limit`).

**See:** Migration Planning Guide (product docs; not bundled in this skill pack), migrate workflow in [references/workflows.md](references/workflows.md)

### 2. Query & Search

**Prefer structured verbs** (mmap; no Cypher). Parse `-f json` from **stdout** (`schema_version`); never `2>/dev/null`.

| User Intent | CLI Command |
|-------------|-------------|
| Session / index freshness | `status` (`index_current`, dirty counts) |
| Schema / counts (incl. zeros) | `inventory --by type` or `inventory --by edge` |
| Import prefix census | `inventory --by import-prefix` |
| Count functions | `find --type function --count-only` |
| Find by name/type | `find "User*" --type class --limit 50` |
| Suffix scan (MDB / Remote) | `find '*MDB*' --type class` |
| Classes with annotation | `find --annotation @MessageDriven --type class` |
| javax import worklist | `find "import javax*" --type import --scope <pkg>` |
| Annotation pairs (seedless) | `relations --edge annotatedwith --from-type function --to-type annotation --scope <pkg>` |
| Find callers/callees | `callers <Symbol> --depth 1` / `callees <Symbol>` |
| Outside callers of a module | `callers <Symbol> --scope <pkg> --scope-mode outside` |
| EXTENDS / IMPLEMENTS inventory | `relations --edge extends --from-type class` (omit SYMBOL) |
| MRO overrides | `relations <SYM> --edge overrides --direction both` |
| Git co-change files | `relations <FILE> --edge cochangeswith` (needs `discover --with-git`) |
| Framework routes / DI | `relations --edge exposesroute` / `relations --edge injects` |
| Natural-language search | `semantic query "checkout flow"` |
| List communities | `communities list` (filter `member_count > 1`; skip unlabeled `"Community N"` singletons) |
| Subsystem ownership | `semantic query "X" --scope community` (prefer named clusters) |
| Refresh community labels | `communities label --write` |
| Community census | `inventory --by community` |
| Markdown heading → code | `find 'Checkout*' --type module`; `relations <MODULE_ID_OR_HEADING> --edge references --direction out` (use `.entities[0].id` from `find` when link nodes share the heading name) |

**Migration:** prefer **`/rgctl:migrate`** or `--export-migration-hints` → `migration_plan.json` v2 `.steps[]`. Optional probes: `communities list` (filter via `jq`) / `blast-radius` (keep `--depth` small).

**Complexity honesty:** exact name = hash index; prefix/`*mid*`/`--scope` may scan keys/columns until better indexes land. Module re-index is still a strong speed lever. Annotation **arguments** (e.g. `@Path("/x")`) need `--show-attributes` when `annotation_args.json` is present.
**See:** [Command Encyclopedia](references/command-encyclopedia.md) (find/callers/relations/inventory/status), Semantic Search Guide (product docs; not bundled in this skill pack)

### 3. Impact & Safety

| User Intent | CLI Command |
|-------------|-------------|
| Blast radius | `blast-radius <Symbol> --depth N` (prefer `--depth 2`/`3`; bound output with `jq`) |
| **Review a PR / branch** (report + verdict) | **`/rgctl:review`** → skill `rgctl-review` (writes `REVIEW_REPORT.md`) |
| Review uncommitted local edits | `review paths --synthetic-head worktree` (via **`/rgctl:review`** Step 0a) |
| Call paths before/after (CLI only) | `review paths --base-ref ORIGIN --head-ref HEAD` |
| Temporal PR policy / CI gate | `review check --policy-file policy.json` (alias: `pr-check`) |
| Policy check (full codebase) | `check --policy-file policy.json` |
| Policy check (one symbol) | `blast-radius <Symbol> --policy-file policy.json` |
| **OSV / CVE / OpenVEX triage** | **`/rgctl:vuln`** → skill `rgctl-vuln` |
| OSV CLI-only | `discover . --with-cfg` then `vuln triage` → `deps check` → `vuln analyze` |

**High-ceremony slash commands** (keep palette ≤3): **`/rgctl:review`**, **`/rgctl:vuln`**, **`/rgctl:migrate`**. Atomic lookups (`find`, `callers`, `blast-radius`, `slice`, …) stay in this meta-skill — never as slash commands.

**PR / branch / worktree review:** prefer **`/rgctl:review`**. Uncommitted dirty tree + warm `.rgctl/` → `--synthetic-head worktree` (no `.rgctl-base/`). Branch compare → base artifact + `--base-ref` / `--head-ref`.

**CLI-only PR family:** `review paths` for call-path JSON; `review check` / `pr-check` for policy gates. Do not treat paths JSON as pass/fail.

**Vuln scans:** prefer **`/rgctl:vuln`**. Index with `--with-cfg` before analyze; read `.sink_taint.call_sites[]` / `.paths[]` from `vuln analyze` (no redundant manual caller chain unless debugging).

**See:** Blast Radius Guide / CI Policy Guide (product docs), vuln workflow in [references/workflows.md](references/workflows.md)

### 4. Metrics & Analysis

| User Intent | CLI Command |
|-------------|-------------|
| PageRank hotspots (names + paths in JSON) | `metrics --pagerank -f json` |
| Betweenness bridges | `metrics --betweenness -f json` |
| Community stats (Fast-Leiden) | `metrics --communities` |
| Git churn hotspots | `metrics --churn` (after `discover --with-git`) |
| Fragment / exact / bloom clones | `clones --mode fragment --seed <SYM\|FILE:LINES>` (or `exact` / `bloom`) |

`top[]` entries include `name`, `qualified_name`, `kind`, `file_path` alongside `node` (UUID) — no follow-up `cpg function` needed.

### 5. Code Analysis (CFG/PDG/Slicing)

| User Intent | CLI Command |
|-------------|-------------|
| CFG/PDG status | `cpg status` |
| Field mutations | `cpg mutations --type Foo` (needs `discover --with-cfg`) |
| Data flow trace | `cpg flows FILE --line N --variable V` (optional `--function`, `--with-alias`; lazy CFG) |
| Program slice | `slice FILE --line N --variable V` (optional `--function`, `--with-alias`; lazy CFG) |
| CFG inspection | `inspect <Symbol> cfg [--file PATH] [--class C] [--line N]` |
| PDG inspection | `inspect <Symbol> pdg --edge-layer data` |

`slice` / `cpg flows` do **not** need upfront `--with-cfg`. Prefer `discover --with-cfg` for `cpg mutations`, sink-first `taint`, and batch `inspect`. Disambiguate overloaded symbols with `--file` / `--class` / `--line` on `inspect`, `cpg function|calls|ast|pdg`, and `blast-radius`.

### 5b. Diff & Konveyor rules

| User Intent | CLI Command |
|-------------|-------------|
| Snapshot / graph diff | `diff --base <A> --head <B>` (or prepared refs; see encyclopedia) |
| Multi-repo merge | `merge <REPOS>... --output-dir <DIR>` |
| Kantra / migration rules during discover | `discover . --with-kantra` |
| List / query installed rules | `rules` (and related subcommands) |

### 6. Export & Visualization

| User Intent | CLI Command |
|-------------|-------------|
| Export graph | `export --export-format graphviz --export-output OUT` |
| CPG export | `cpg export --format graphson --output cpg.json` |
| HTTP dashboard | `serve --open` |

## Natural Language → Command Routing

| User Says | Command |
|-----------|---------|
| "Generate migration plan" / "What should we extract first?" | **`/rgctl:migrate`** (or `discover . --export-migration-hints` → `.steps[]`) |
| "Bottlenecks / hotspots" | `metrics --pagerank` |
| "Where is checkout flow?" | `semantic query "checkout flow" --limit 10` |
| "How does this doc map to code?" | `discover . -l markdown,<lang>` → `find 'Heading*' --type module` → `relations … --edge references --direction out` |
| "Impact if I change X" | `blast-radius X --depth 2` |
| "Are we affected by this CVE / OSV?" | **`/rgctl:vuln`** (or vuln workflow in references) |
| "Validate against policy" | `check --policy-file policy.json` |
| "Review my uncommitted changes" | **`/rgctl:review`** → `review paths --synthetic-head worktree` |
| "How did call paths change on this PR?" | **`/rgctl:review`** or `review paths --base-ref origin/main --head-ref HEAD` |
| "PR policy gate / new violations vs main" | `review check --policy-file …` (alias `pr-check`) |
| "Who calls X" | `callers X --depth 2` (impact → `blast-radius X`) |
| "javax imports / annotations" | `find "import javax*" --type import`; `relations --edge annotatedwith --from-type function --to-type annotation` |
| "Where is X mutated?" | `cpg mutations --type X --exclude-ctors` (needs `--with-cfg`) |
| "Slice / data flow for this line" | `slice FILE --line N --variable V` (lazy; optional `--with-alias`) |
| "Duplicate code / copy-pasted loop or snippet" | `clones --mode fragment --seed <fn\|FILE:LINES>` (or `--mode exact` / `--mode bloom`) |
| "What co-changes with this file?" | `discover --with-git` then `relations FILE --edge cochangeswith` / `metrics --churn` |
| "Override hierarchy for this method" | `relations <SYM> --edge overrides --direction both` |

## Failure Playbook

| Symptom | Fix |
|---------|-----|
| No `.rgctl/` in repo | Run `cd repo && rgctl discover .` |
| `status` shows `index_current=false` | Run `rgctl update` once (works under `serve --watch` via queue); do **not** full rediscover |
| blast-radius / callers empty for a file that exists on disk | `rgctl update --files <path>` once, retry query; then fall back to reading source |
| `slice` / `cpg flows` fails | Confirm `--line` is inside a function body; retry (lazy append). Reserve rediscover `--with-cfg` for mutations/taint/inspect |
| inspect / `cpg mutations` / sink-first taint fails | Re-discover with `--with-cfg` |
| vuln analyze / sink-first taint weak (`cfg_available=false`) | Re-discover with `--with-cfg` (add `--with-taint` for PDG confidence) |
| semantic query fails | `semantic index` |
| Ambiguous symbol | Add `--file` / `--class` / `--line` (inspect/cpg/blast-radius/find/callers); read `candidates` + `recovery_command` under `-f json` |
| `check` exit 1 | Report violations (JSON still on stdout) |
| find/relations empty | Run `inventory --by type` / `--by edge` (zeros mean unpopulated schema); check `--scope` |
| Name glob returns 0 | Try `semantic query` / `communities list` / broader `find '*X*'` types |

## Artifacts

All paths under **`{repo}/.rgctl/`**:

| Path | Content |
|------|---------|
| `graph.snapshot.bin` | Main graph snapshot |
| `semantic_index.bin` | Semantic index |
| `migration_plan.json` | Migration roadmap (`--export-migration-hints`) |
| `dashboard/` | Dashboard bundle |
| `analysis/` | CFG/PDG archives |
| `clones*.json` | Clone group sidecar caches (`clones.json`, `clones.bloom.json`, `clones.fragment.json`) |

## Usage Globals

```bash
export REPO=/path/to/repo
rgctl -r "$REPO" discover .
rgctl -r "$REPO" discover          # same target as discover .
rgctl -r "$REPO" -f json <command> …
# RGCTL_FORMAT=json also selects JSON without -f
```

**Globals:** `-f json` / `RGCTL_FORMAT` (agents), `-r` / `--repo` (positional `.` resolves against `-r`), `-o` output file, `-d` / `--db` (custom graph cache path)

## Reference Files

- **[Command Encyclopedia](references/command-encyclopedia.md)** — Full command reference
- **[Workflows](references/workflows.md)** — Worked scenarios
- **[Communities & Policy](references/communities-and-policy.md)** — CI policy checks

## External Documentation

- User Guide (product docs; not bundled in this skill pack) — Complete CLI tutorial
- JSON API (product docs; not bundled in this skill pack) — Schema specifications
- Agent Recipes (product docs; not bundled in this skill pack) — Copy-paste recipes
- USER_AGENTS_TEMPLATE.md (product docs; not bundled in this skill pack) — paste into consumer repos
- Contributor `AGENTS.md` (rgctl source tree only; not shipped to consumer installs)
- Policy Format (product docs; not bundled in this skill pack) — CI policy schema

## Installation

```bash
rgctl install --skill --tools cursor,claude,codex,antigravity,agents
```

Installs the single skill `rgctl` (with `references/`) for each selected adapter. Omitting `--tools` installs **cursor, claude, codex, agents, antigravity**; use `--tools all` for the full registry. See https://rgctl.io/docs/guides/agent-skill/ (product docs; not bundled in this skill pack). Scenario prose lives in `references/workflows.md`.
