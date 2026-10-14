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

**Prerequisite (once per repo):** `discover` (and `semantic index` when using search). Deep analysis (`cpg`, `inspect`, slice/taint) needs `discover --with-cfg`.

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
| Patch graph after edits | `rgctl update` (or `update --files path1,path2`) — structural only |
| Keep graph fresh while serving | `serve --watch` (debounced FS updates); still run `update` to enqueue if needed |
| Prefer canonical verbs | `find`/`callers`/`slice`/`vuln analyze` (see encyclopedia map); `query`/`cpg`/`security` are façades |
| Build semantic index | `semantic index` |
| Check CFG readiness | `cpg status` |
| Full staged pipeline | `discover . --full` |

**Common flags:**
- `--with-cfg` — Enable CFG/PDG (for slice, inspect, cpg)
- `--with-dashboard` — Build dashboard bundle
- `--export-migration-hints` — Write `.rgctl/migration_plan.json` (primary migration deliverable)
- `--with-security --with-taint` — Security scanning
- `--with-harmonic` — Harmonic centrality (used with migration ranking)

**See:** [Discovering and Indexing Guide](../../docs/guides/discovering-and-indexing.md)

### 1b. Migration roadmap (`--export-migration-hints`)

**Primary deliverable for migration questions:** `.rgctl/migration_plan.json` — not ad-hoc rule dumps.

| User Intent | CLI Command |
|-------------|-------------|
| Generate migration plan | `discover . --export-migration-hints` (add `--with-harmonic` for ranking) |
| Preset / order | `--migration-preset hybrid_default\|foundational_first\|dense_cluster\|risk_mitigation` · `--migration-order scheduled\|priority` |
| Read plan | `.rgctl/migration_plan.json` (discover `-f json` stdout is telemetry only) |
| Dashboard view | `discover . --with-dashboard --export-migration-hints` then `serve --open` |

**Supporting probes** (optional context before/after the plan): `status` → `inventory --by import-prefix --limit 40` → `find --annotation …` / suffix globs → `callers` / `blast-radius` on candidates.

**See:** [Migration Planning Guide](../../docs/guides/migration-planning.md), migrate workflow in [references/workflows.md](references/workflows.md)

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
| Natural-language search | `semantic query "checkout flow"` |
| List communities | `communities list` |
| Subsystem ownership | `semantic query "X" --scope community` |
| Refresh community labels | `communities label --write` |
| Community census | `inventory --by community` |

**Migration:** prefer `--export-migration-hints` → `migration_plan.json`. Optional probes: `status` → `inventory --by import-prefix --limit 40` → `find --annotation …` / suffix globs → `callers` / `blast-radius`.

**Complexity honesty:** exact name = hash index; prefix/`*mid*`/`--scope` may scan keys/columns until better indexes land. Module re-index is still a strong speed lever. Annotation **arguments** (e.g. `@Path("/x")`) need `--show-attributes` when `annotation_args.json` is present.
**See:** [Command Encyclopedia](references/command-encyclopedia.md) (find/callers/relations/inventory/status), [Semantic Search Guide](../../docs/guides/semantic-search.md)

### 3. Impact & Safety

| User Intent | CLI Command |
|-------------|-------------|
| Blast radius | `blast-radius <Symbol> --depth N` |
| **Review a PR / branch** (report + verdict) | **`/rgctl:review`** → skill `rgctl-review` (writes `REVIEW_REPORT.md`) |
| Call paths before/after (CLI only) | `review paths --base-ref ORIGIN --head-ref HEAD` |
| Temporal PR policy / CI gate | `review check --policy-file policy.json` (alias: `pr-check`) |
| Policy check (full codebase) | `check --policy-file policy.json` |
| Policy check (one symbol) | `blast-radius <Symbol> --policy-file policy.json` |
| OSV / CVE / OpenVEX | `discover . --with-cfg` then `vuln triage` → `deps check` → `vuln analyze` (see vuln workflow) |

**PR / branch review:** when the user asks to **review a PR**, branch, or call-path impact vs a base, prefer slash command **`/rgctl:review`** (skill `rgctl-review`) — base artifact, `review paths` / optional `review check`, session `REVIEW_REPORT.md` + four-section verdict. No forge posting.

**CLI-only PR family:** use **`review paths`** when the user only wants call-path JSON. Use **`review check`** / `pr-check` for policy/merge gates. Do **not** run `review check` solely to explain paths; do **not** treat paths JSON as pass/fail.

**Presenting `review paths`:** (1) call paths per symbol (before → after + deltas), (2) short prose summary, (3) unscored files. Unchanged path → one line. No raw JSON dump; no follow-up `callers`/`callees` when paths already returned.

**Vuln scans:** index with `--with-cfg` before sink-first taint / blast classify / exploitability VEX; add `--with-taint` when PDG-backed confidence is required. Plain discover is enough only for triage + deps-only early exit.

**See:** [Blast Radius Guide](../../docs/guides/blast-radius-analysis.md), [CI Policy Guide](../../docs/guides/ci-policy-checks.md), vuln workflow in [references/workflows.md](references/workflows.md)

### 4. Metrics & Analysis

| User Intent | CLI Command |
|-------------|-------------|
| PageRank hotspots | `metrics --pagerank` |
| Betweenness bridges | `metrics --betweenness` |
| Community stats | `metrics --communities` |

### 5. Code Analysis (CFG/PDG/Slicing)

| User Intent | CLI Command |
|-------------|-------------|
| CFG/PDG status | `cpg status` |
| Field mutations | `cpg mutations --type Foo` |
| Data flow trace | `cpg flows FILE --line N --variable V --function F` |
| Program slice | `slice FILE --line N --variable V --function F` |
| CFG inspection | `inspect <Symbol> cfg` |
| PDG inspection | `inspect <Symbol> pdg --edge-layer data` |

Needs `discover --with-cfg`. `--function` is method name, not class.

### 6. Export & Visualization

| User Intent | CLI Command |
|-------------|-------------|
| Export graph | `export --export-format graphviz --export-output OUT` |
| CPG export | `cpg export --format graphson --output cpg.json` |
| HTTP dashboard | `serve --open` |

## Natural Language → Command Routing

| User Says | Command |
|-----------|---------|
| "Generate migration plan" / "What should we extract first?" | `discover . --export-migration-hints` → `.rgctl/migration_plan.json` |
| "Bottlenecks / hotspots" | `metrics --pagerank` |
| "Where is checkout flow?" | `semantic query "checkout flow" --limit 10` |
| "Impact if I change X" | `blast-radius X --depth 2` |
| "Are we affected by this CVE / OSV?" | `discover . --with-cfg` → `vuln triage` / `deps check` / `vuln analyze` (vuln workflow) |
| "Validate against policy" | `check --policy-file policy.json` |
| "How did call paths change on this PR?" | `review paths --base-ref origin/main --head-ref HEAD` |
| "PR policy gate / new violations vs main" | `review check --policy-file …` (alias `pr-check`) |
| "Who calls X" | `callers X --depth 2` (impact → `blast-radius X`) |
| "javax imports / annotations" | `find "import javax*" --type import`; `relations --edge annotatedwith --from-type function --to-type annotation` |
| "Where is X mutated?" | `cpg mutations --type X --exclude-ctors` |
| "Duplicate code / copy-pasted loop or snippet" | `clones --mode fragment --seed <fn> --lines <start-end>` (or `--mode exact` / `--mode bloom`) |

## Failure Playbook

| Symptom | Fix |
|---------|-----|
| No `.rgctl/` in repo | Run `cd repo && rgctl discover .` |
| `status` shows `index_current=false` | Run `rgctl update` once (works under `serve --watch` via queue); do **not** full rediscover |
| blast-radius / callers empty for a file that exists on disk | `rgctl update --files <path>` once, retry query; then fall back to reading source |
| slice/inspect/cpg fails | Re-discover with `--with-cfg` |
| vuln analyze / sink-first taint weak (`cfg_available=false`) | Re-discover with `--with-cfg` (add `--with-taint` for PDG confidence) |
| semantic query fails | `semantic index` |
| Ambiguous symbol | Add `--class` or `--file` on callers/find |
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
rgctl -r "$REPO" discover
rgctl -r "$REPO" -f json <command> …
```

**Globals:** `-f json` (agents), `-r` / `--repo`, `-o` output file, `-d` / `--db` (custom graph cache path)

## Reference Files

- **[Command Encyclopedia](references/command-encyclopedia.md)** — Full command reference
- **[Workflows](references/workflows.md)** — Worked scenarios
- **[Communities & Policy](references/communities-and-policy.md)** — CI policy checks

## External Documentation

- [User Guide](../../docs/user-guide.md) — Complete CLI tutorial
- [JSON API](../../docs/json-api.md) — Schema specifications
- [Agent Recipes](../../docs/agent-recipes.md) — Copy-paste recipes
- [USER_AGENTS_TEMPLATE.md](../../docs/agents/USER_AGENTS_TEMPLATE.md) — paste into consumer repos
- [AGENTS.md](../../AGENTS.md) — contributor agent README (rgctl source tree)
- [Policy Format](../../docs/policy-format.md) — CI policy schema

## Installation

```bash
rgctl install --skill --tools cursor,claude,codex,antigravity,agents
```

Installs the single skill `rgctl` (with `references/`) for each selected adapter. Omitting `--tools` installs **cursor, claude, codex, agents, antigravity**; use `--tools all` for the full registry. See [docs/guides/agent-skill.md](../../docs/guides/agent-skill.md). Scenario prose lives in `references/workflows.md`.
