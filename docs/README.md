# rgctl documentation

Agent-first docs: index once, query with `-f json`, deepen in the User Guide when a human needs the full walkthrough.

> **Feature guides** (`docs/guides/` historically) now live in **[rgctl-website](https://github.com/rgctl/rgctl-website)** (`docs/guides/`) and are published at [shaaf.dev/rgctl/docs/guides/](https://rgctl.io/docs/guides/). This repo keeps a [pointer stub](guides/README.md).

## Primary (start here)

| Goal | Canon |
|------|--------|
| Install rgctl + choose operating mode | **[Installation](installation.md)** |
| Step-by-step feature how-tos (CoolStore) | **[Guides](https://rgctl.io/docs/guides/)** |
| Full CLI taxonomy (canonical vs aliases) | **[CLI command reference](https://rgctl.io/docs/guides/cli-command-reference/)** |
| Per-language AST coverage (website from JSON) | **[Languages](languages/README.md)** · `*-ast-coverage.json` |
| Contribute to rgctl (agent README) | [AGENTS.md](../AGENTS.md) — rules, cold profiles, tests/benches |
| Use rgctl with LLM IDEs | [Agent pack](https://rgctl.io/docs/guides/agent-skill/) · [USER_AGENTS_TEMPLATE](agents/USER_AGENTS_TEMPLATE.md) |
| JSON shapes (`schema_version`, fields) | [JSON API](json-api.md) |
| CLI walkthrough (ecommerce-java) | [User Guide](user-guide.md) |
| Concepts (what / why) | [Introduction](Introduction.md) |

**Use rgctl (consumer agent loop):** install the agent pack (`--skill`) → `discover` once → `find` / `callers` / `blast-radius` / `cpg` with `-f json` ([agent-skill](https://rgctl.io/docs/guides/agent-skill/)).  
**Contribute to this repo:** [AGENTS.md](../AGENTS.md).  
**First hour (human):** [Install](installation.md) → User Guide §1–4 on [ecommerce-java](user-guide.md#3-example-project-ecommerce-java), then a [Guide](https://rgctl.io/docs/guides/) for the feature you need.  
**Latest release:** [v0.4.21 release notes](releases/v0.4.21.md).

**Upgrading from v0.4.9:** no breaking changes — PHP is additive (`discover -l php`). Ruby is additive (`discover -l ruby`).

**Upgrading from v0.4.8:** [v0.4.9 release notes](releases/v0.4.9.md) (daemon/MCP removed; Kantra; in-repo `.rgctl/` only).

## Secondary

| Goal | Doc |
|------|-----|
| Supported languages | [Languages](languages/README.md) (SSOT: coverage JSON) |
| Dependency Injection & Frameworks | [Dependency Injection and Frameworks](https://rgctl.io/docs/guides/dependency-injection-and-frameworks/) — routes (`ExposesRoute`), DI (`Injects`), reflection, semantic tags, and `frameworks.bin` |
| Markdown / doc context graph | [Guide](https://rgctl.io/docs/guides/markdown-context-graph/) (step-by-step) — `.md` / `.mdx`, structured queries, Obsidian export, doc semantic index |
| HTTP server + dashboard | [HTTP Server and Dashboard](https://rgctl.io/docs/guides/http-server-and-dashboard/) |
| CI blast-radius policy | [Policy format](policy-format.md) |
| Monolith migration (how-to) | [Migration planning](https://rgctl.io/docs/guides/migration-planning/) |
| Research map | [Further reading](further-reading.md) |

Optional browser UI (nice-to-have, not required for agents): [HTTP server and dashboard](https://rgctl.io/docs/guides/http-server-and-dashboard/) after `discover --with-universe`.

## For contributors

Internals and contribution bars — not the default agent reading path.

| Document | Topic |
|----------|--------|
| [Contributor checklist](contributor-checklist.md) | End-to-end workflow: add language, update feature, tests, PR |
| [Language testing guide](language-testing-guide.md) | Where to add language tests, what to assert, Tier 1 matrix |
| [Feature designs](design/README.md) | Per-capability engineering notes |
| [Tier 1 language support](tier-1-language-support.md) | Layer A–F bar for Tier 1 languages |
| [Code structure](Code_structure.md) | Crate layout |
| [Analysis architecture](analysis-architecture.md) | CFG / PDG / taint |
| [Graph storage architecture](graph-storage-architecture.md) | Snapshots, blast cache |
| [Integration test matrix](internal/integration-tests.md) | Tier A/B/C — in-repo artifacts, HTTP serve, CLI subprocess · golden CLI: `tests/cli_output/` |
| [HTTP server and dashboard](https://rgctl.io/docs/guides/http-server-and-dashboard/) | Serve / dashboard UI |
| [Migration planner design](design/migration-planner-design.md) · [Migration algorithms](migration-algorithms.md) | Migration internals |
| [Releasing](releasing.md) | Versioned binaries |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | Dev setup |

## Terminology

| Term | Meaning |
|------|---------|
| Tier-1 deep-analysis languages | Custom plugins with `*-ast-coverage.json` + CFG/PDG (single-pass `ScopeStack` + `KindDispatchTable`); distinct from the ~60 language/config plugins in [`languages.toml`](../languages.toml) |
| Default graph artifact | `.rgctl/graph.snapshot.bin` (`RGCL` v2 columnar mmap); `graph.db` / `graph.json` only with `--write-json-graph` |
| `--with-cfg` | Whole-repo CFG/PDG archive (needed for `taint --sink`, `cpg mutations`); `slice` / `cpg flows` lazy-build one function without it |
| Communities | **Fast-Leiden** (local moving + BFS refinement + coarsening) with $\mu+4\sigma$ `HubStripPolicy` |
| Canonical query / policy CLI | `find` / `callers` / `callees` / `relations` / `inventory`; `review check` (`query` / `pr-check` are compatibility aliases) |
| Dashboard / migration JSON | Opt-in (`--with-universe` / `--export-migration-hints`; `--with-dashboard` is deprecated) |
| First-hour fixture | In-tree **ecommerce-java** |

## Redirects (historical names)

- `cli-getting-started.md` → [Installation](installation.md) / [User Guide](user-guide.md)
- `cli-output-schemas.md` → [JSON API](json-api.md)
- `LANGUAGE_GUIDE.md` → [languages/README.md](languages/README.md)

Docs match the CLI in this repository — verify with `rgctl --version`.

Maintainer scratch: [`internal/`](internal/) (not public).
