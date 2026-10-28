# Introduction to rgctl

**What rgctl is** and how a **code knowledge graph** works — concepts before commands.

**Hands-on:** [Installation](installation.md) · [Guides](https://rgctl.io/docs/guides/) (CoolStore) · [User Guide](user-guide.md) (ecommerce-java).  
**Agents:** [agent-skill](https://rgctl.io/docs/guides/agent-skill/) · [USER_AGENTS_TEMPLATE](agents/USER_AGENTS_TEMPLATE.md) · `rgctl install --skill`.  
**Contribute to rgctl:** [AGENTS.md](../AGENTS.md). **JSON:** [json-api.md](json-api.md).

---

## What problem does rgctl solve?

Modern codebases are too large to hold in your head — or in an LLM context window. Changing a function raises reachability questions: who calls it, what depends on it, are security-sensitive paths involved, where is complexity concentrated?

**rgctl turns the repository into a structured graph** — functions, types, calls, imports, docs, and more — so you ask structural questions and get deterministic answers instead of grepping and guessing. Built in **Rust** for speed and predictable memory on large repos. Primary consumer output is compact **`-f json`** for agents and scripts.

---

## What is a code knowledge graph?

| Everyday idea | In rgctl |
|---------------|--------------|
| Places on the map | **Nodes** — functions, classes, files, modules, headings, … |
| Roads | **Edges** — typed relations (`CALLS`, `CONTAINS`, `IMPORTS`, `VIOLATES`, …) |
| The map file | Artifacts under **`{repo}/.rgctl/`** after `discover` |

**Reachability** (who can reach whom along call paths) is pre-computed and stored compactly — that is why **blast-radius** stays fast on large graphs.

You do not need graph theory to use the CLI: **indexing builds the map; commands query the map.**

---

## How the pieces fit

```text
  Your repo (source)
      │
      │  discover  (cd repo && rgctl discover .   — or  rgctl -r PATH discover)
      ▼
  artifact root ({repo}/.rgctl/)
      │  primary: graph.snapshot.bin (RGCL v2 columnar mmap)
      │
      ├── find / callers / callees / relations / inventory / blast-radius / metrics / communities / clones / cpg / slice / inspect
      ├── check / review check / review paths / diff   (CI + temporal PR gate + snapshot compare)
      ├── update / watch / merge                       (incremental + multi-repo park-and-resolve)
      ├── semantic index + query                       (opt-in embedder)
      ├── export                                       (JSON, GraphML, Mermaid, Obsidian, …)
      └── serve                                        (optional HTTP dashboard + semantic API)
```

1. **Once** (or after large changes): `discover` from the repo you mean to index — see [Discovering and indexing](https://rgctl.io/docs/guides/discovering-and-indexing/) for `-r` vs `.` pitfalls.  
2. **Many times:** query commands read `{repo}/.rgctl/`. Prefer **`-f json`** and never scrape stderr ([JSON API](json-api.md)).  
3. **Agents:** install the pack (`rgctl install --skill --tools …`) — single skill `rgctl` ([agent-skill](https://rgctl.io/docs/guides/agent-skill/)).  
4. **Dashboard:** optional UI after `discover --with-universe` + `serve` — not required for structural answers.

Capability designs for contributors: [design/](design/README.md).

### Discover depth (common flags)

| Flags | Use |
|-------|-----|
| (default) | Fast graph + metrics caches → `.rgctl/graph.snapshot.bin` (`RGCL` v2) |
| `--with-cfg` | Whole-repo CFG/PDG archive (v3 `cfg_pdg.bin`); needed for `taint --sink`, `cpg mutations`, richest `inspect`. **Not** required for `slice` / `cpg flows` (lazy single-function build) |
| `--with-taint` | Discover-time taint (with CFG) |
| `--with-git` | Git churn hotspots + `CoChangesWith` edges (`metrics --churn`) |
| `--scip PATH` | SCIP overlay — upgrades ambiguous calls (`resolution = "scip"`) |
| `--with-kantra` | Konveyor Kantra rule eval (Java-oriented; `VIOLATES` edges) |
| `--full` | Full pipeline (large corpora / Gate B style runs) |
| `--with-universe` / `--export-migration-hints` | Opt-in universe UI bundle / migration JSON (`--with-dashboard` is a deprecated alias) |

```bash
rgctl discover . -l java,kotlin,python --with-cfg
rgctl discover . --with-git --git-since 90.days
rgctl discover . --scip index.scip
rgctl discover . -e node_modules,target,.git,vendor
```

---

## Capability map (concepts only)

Step-by-step how-tos: **[Guides](https://rgctl.io/docs/guides/)**. Full CLI walkthrough: **[User Guide](user-guide.md)**.

| Capability | Intent |
|------------|--------|
| **discover** | Index repo → `RGCL` v2 graph + analytics caches under `.rgctl/` |
| **find / callers / callees / relations / inventory** | Structured graph queries (agent-facing; `query …` is a compatibility alias) |
| **blast-radius** | Upstream impact / reachability for a symbol |
| **slice / taint** | Statement-level data/control dependence; source→sink (`slice` lazy-builds CFG/PDG; `--with-alias`) |
| **inspect** | CFG / PDG / dominance for one function |
| **cpg** | Hybrid CALL + CFG/PDG façade (`flows` lazy; `mutations` needs `--with-cfg`) |
| **clones** | Exact / bloom / fragment (SESE hammock + 2-iter 1-WL) with `--seed` |
| **metrics / communities** | `FastPageRank`, `SampledBetweenness`, `HyperBallHarmonic`; **Fast-Leiden** communities ($\mu+4\sigma$ hub strip) |
| **merge / update / watch** | Multi-repo park-and-resolve merge; incremental / live update |
| **semantic** | Opt-in natural-language / keyword search over functions |
| **export** | Subgraph / projection export (JSON, GraphML, Mermaid, Obsidian, …) |
| **check / review check** | CI blast-radius policy; temporal PR gate (`pr-check` = alias of `review check`) |
| **diff** | Compare two columnar snapshots (digest + `diff_snapshots`) |
| **migration hints** | Package roadmap JSON (`--export-migration-hints`) |
| **Kantra** | Migration-rule findings during discover (`--with-kantra`) |
| **MRO / Overrides** | Cross-language method resolution → `Overrides` edges; polymorphic call retarget |
| **install** | Bundle agent skills / optional policy into IDEs |
| **serve** | Foreground HTTP dashboard + semantic HTTP API for one repository |

**Markdown / docs:** `discover` indexes `.md` / `.mdx` by default (headings, links, frontmatter). Use `find` / `relations` on `module` nodes and `references` edges; function semantic search stays separate. See [guide](https://rgctl.io/docs/guides/markdown-context-graph/).

---

## Languages

**Tier-1 deep-analysis** languages use custom tree-sitter plugins with grammar-verified `crates/rgctl-lang-*/{id}-ast-coverage.json` manifests, single-pass `extract_all` (`ScopeStack` + `KindDispatchTable` + `parse_with_pool`), and CFG/PDG support. Set `RGCTL_AST_COVERAGE_STRICT=1` to fail the build on grammar/manifest drift.

Separately, [`languages.toml`](../languages.toml) registers **~60** language and config/IaC plugins (Tier 1 + generic tree-sitter + regex + formats). Do not equate “Tier 1 deep analysis” with the full plugin catalog count.

The website **`/docs/languages/`** pages are generated from those JSON files at build time — do not maintain parallel hand-written coverage tables. Pointers: [languages/README.md](languages/README.md) · [tier-1-language-support.md](tier-1-language-support.md) · [CLI command reference](https://rgctl.io/docs/guides/cli-command-reference/).

Current Tier-1 deep-analysis ids include C, C++, C#, Go, Groovy, Java, JavaScript, Kotlin, PHP, Puppet, Python, Ruby, Rust, TypeScript (plus markdown as a doc plugin; ERB and others with coverage JSON as they land).

---

## Where to go next

| You want… | Go to |
|-----------|--------|
| Install / verify the binary | [Installation](installation.md) |
| Feature how-tos on CoolStore | [Guides](https://rgctl.io/docs/guides/) |
| Full CLI + ecommerce-java | [User Guide](user-guide.md) |
| Agent pack (skills) | [agent-skill](https://rgctl.io/docs/guides/agent-skill/) |
| Paste into *another* repo | [USER_AGENTS_TEMPLATE](agents/USER_AGENTS_TEMPLATE.md) |
| Language support matrix | [languages/README.md](languages/README.md) (JSON SSOT → website) |
| JSON fields / `schema_version` | [json-api.md](json-api.md) |
| HTTP `serve` API | [HTTP Server and Dashboard](https://rgctl.io/docs/guides/http-server-and-dashboard/) |
| Latest release notes | [v0.4.17](releases/v0.4.17.md) |
| Contribute / cold profiles | [AGENTS.md](../AGENTS.md) · [docs hub — For contributors](README.md#for-contributors) |
| Research map | [further-reading.md](further-reading.md) |
