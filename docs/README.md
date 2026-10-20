# rgctl documentation

Agent-first docs: index once, query with `-f json`, deepen in the User Guide when a human needs the full walkthrough.

## Primary (start here)

| Goal | Canon |
|------|--------|
| Install rgctl + choose operating mode | **[Installation](installation.md)** |
| Step-by-step feature how-tos (CoolStore) | **[Guides](guides/README.md)** |
| Per-language AST coverage (website from JSON) | **[Languages](languages/README.md)** · `*-ast-coverage.json` |
| Contribute to rgctl (agent README) | [AGENTS.md](../AGENTS.md) — rules, cold profiles, tests/benches |
| Use rgctl with LLM IDEs | [Agent pack](guides/agent-skill.md) · [USER_AGENTS_TEMPLATE](agents/USER_AGENTS_TEMPLATE.md) · [Agent recipes](agent-recipes.md) |
| JSON shapes (`schema_version`, fields) | [JSON API](json-api.md) |
| CLI walkthrough (ecommerce-java) | [User Guide](user-guide.md) |
| Concepts (what / why) | [Introduction](Introduction.md) |

**Use rgctl (consumer agent loop):** install the agent pack (`--skill`) → `discover` once → `find` / `callers` / `blast-radius` / `cpg` with `-f json` ([agent-skill](guides/agent-skill.md)).  
**Contribute to this repo:** [AGENTS.md](../AGENTS.md).  
**First hour (human):** [Install](installation.md) → User Guide §1–4 on [ecommerce-java](user-guide.md#3-example-project-ecommerce-java), then a [Guide](guides/README.md) for the feature you need.  
**Latest release:** [v0.4.17 release notes](releases/v0.4.17.md) (Kotlin/Groovy/Puppet Tier 1, AST coverage SSOT, TS/JS named arrows).

**Upgrading from v0.4.9:** no breaking changes — PHP is additive (`discover -l php`). Ruby is additive (`discover -l ruby`).

**Upgrading from v0.4.8:** [v0.4.9 release notes](releases/v0.4.9.md) (daemon/MCP removed; Kantra; in-repo `.rgctl/` only).

## Secondary

| Goal | Doc |
|------|-----|
| Supported languages | [Languages](languages/README.md) (SSOT: coverage JSON) |
| Markdown / doc context graph | [Guide](guides/markdown-context-graph.md) (step-by-step) — `.md` / `.mdx`, structured queries, Obsidian export, doc semantic index |
| HTTP server + dashboard | [HTTP Server and Dashboard](guides/http-server-and-dashboard.md) |
| CI blast-radius policy | [Policy format](policy-format.md) |
| Monolith migration (how-to) | [Building a migration plan](building-migration-plan.md) |
| Research map | [Further reading](further-reading.md) |

Optional browser UI (nice-to-have, not required for agents): [Dashboard user guide](dashboard-user-guide.md) after `discover --with-dashboard`.

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
| [CLI I/O sanity QE](cli-io-sanity-qe.md) | Golden-path test contract |
| [Integration test matrix](internal/integration-tests.md) | Tier A/B/C — in-repo artifacts, HTTP serve, CLI subprocess |
| [Dashboard design](dashboard-design.md) | WASM export pipeline |
| [Migration planner design](design/migration-planner-design.md) · [Migration algorithms](migration-algorithms.md) · [Harmonic centrality](harmonic-centrality.md) | Migration internals |
| [Releasing](releasing.md) | Versioned binaries |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | Dev setup |

## Terminology

| Term | Meaning |
|------|---------|
| Tier 1 languages | Custom plugins; matrix from `*-ast-coverage.json` ([languages/README.md](languages/README.md)) |
| `--with-cfg` | CFG/PDG archive (prefer over legacy `--cfg`) |
| Communities | Label propagation (Raghavan 2007); `louvain_community_id` is historical |
| Dashboard / migration JSON | Opt-in (`--with-dashboard` / `--export-migration-hints`) |
| First-hour fixture | In-tree **ecommerce-java** |

## Redirects

- [cli-getting-started.md](cli-getting-started.md) → [Installation](installation.md) / User Guide  
- [cli-output-schemas.md](cli-output-schemas.md) → [JSON API](json-api.md)  
- [LANGUAGE_GUIDE.md](LANGUAGE_GUIDE.md) → [languages/README.md](languages/README.md)

Docs match the CLI in this repository — verify with `rgctl --version`.

Maintainer scratch: [`internal/`](internal/) (not public).
