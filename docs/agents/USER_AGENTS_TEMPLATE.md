# rgctl for AI agents (USER TEMPLATE)

> **Copy this file into your own repository as root `AGENTS.md`** if you want agents working *on that codebase* to prefer rgctl for structural questions.
>
> **Preferred (automated):** install the agent pack instead of (or in addition to) pasting this file:
>
> ```bash
> rgctl -r "$REPO" install --skill --tools cursor,claude,codex,antigravity,agents
> ```
>
> See [Agent pack](../guides/agent-skill.md). This template is a fallback / complementary channel — skills remain the canonical runtime guidance.
>
> **Not for contributing to rgctl itself.** Contributors: see the repository root [AGENTS.md](../../AGENTS.md).

---

rgctl is designed so agents answer **structural questions** from a pre-built graph instead of reading whole files into context.

**Installation:** [installation.md](../installation.md)  
**Agent pack install:** [agent-skill.md](../guides/agent-skill.md)  
**Full JSON reference:** [json-api.md](../json-api.md) · [site](https://sshaaf.github.io/rgctl/docs/json-api/)  
**Copy-paste recipes:** [agent-recipes.md](../agent-recipes.md)  
**Human walkthrough:** [user-guide.md](../user-guide.md)  
**Docs hub:** [docs/README.md](../README.md) · [site docs](https://sshaaf.github.io/rgctl/docs/)

Default for agents: spawn **`rgctl -f json`** subprocesses (or use foreground **`rgctl serve`** for repeated HTTP queries). Do **not** open the browser dashboard unless the user asks for a visual UI.

Install the agent pack once (limit `--tools` to the IDEs you use; default is all registry adapters):

```bash
rgctl -r "$REPO" install --skill --tools cursor,claude,codex,antigravity,agents
```

---

## Agent workflow

```text
1. rgctl -r "$REPO" discover .     # or: cd "$REPO" && rgctl discover .  (`.` with `-r` targets the repo)
2. rgctl -f json <command>            # compact facts on stdout
3. Parse schema_version + payload     # never scrape stderr for JSON
```

Artifacts live at **`{repo}/.rgctl/`**. Set `REPO` to the repository root:

```bash
export REPO=/path/to/repo
rgctl -r "$REPO" -f json find --type function --limit 20
```

Upgrading from an old daemon install: copy `~/.rgctl/cache/{name}/.rgctl/` into the repo as `.rgctl/` manually, or re-run `rgctl discover .` (see [installation.md](../installation.md)).

---

## High-value commands (low token cost)

| Intent | Command |
|--------|---------|
| Full session (graph + CFG + dashboard + semantic) | `rgctl discover PATH --full` (queryable after stage 1; status in `.rgctl/pipeline_status.json`) |
| HTTP session (auto-pipeline) | `rgctl serve` — `GET /api/status`; `--no-pipeline` restores fail-fast |
| Schema / counts (zeros included) | `rgctl -f json inventory --by type` or `--by edge` |
| Inventory / count functions | `rgctl -f json find --type function --count-only` |
| Find symbol by pattern | `rgctl -f json find "*Service*" --type function --limit 20` |
| Find by package scope | `rgctl -f json find --type class --scope com.example` |
| Callers / callees | `rgctl -f json callers <Symbol> --depth 2` / `callees <Symbol>` |
| Typed edges (seedless OK) | `rgctl -f json relations --edge annotatedwith --from-type function --to-type annotation` |
| List communities | `rgctl -f json communities list` |
| Natural-language function search | `rgctl semantic index` then `rgctl -f json semantic query "checkout flow" --limit 10` |
| Community semantic search | `rgctl -f json semantic query "checkout" --scope community --limit 10` |
| Impact before editing | `rgctl -f json blast-radius <Symbol> [--depth N]` |
| Architectural hotspots | `rgctl -f json metrics --pagerank` |
| Doc headings / cross-links | `discover` indexes `.md` / `.mdx` by default; use `find` / `relations` on Module/REFERENCES — see [markdown-context.md](../markdown-context.md) |
| Obsidian vault from docs | `rgctl -r "$REPO" discover -l markdown` then `export --export-format obsidian --export-output "$REPO/vault" --query all` — see [markdown-context.md](../markdown-context.md#obsidian-vault-export) |
| Doc section semantic search | `rgctl semantic index --scope docs --embedder hash` then `rgctl -f json semantic query "checkout flow" --scope docs --limit 10` (query scope does not filter — index must be doc-scoped) |
| Hybrid CPG status / CALL / PDG / slice | `rgctl -f json cpg status` then `cpg function\|calls\|pdg\|slice` (needs `discover --with-cfg` for PDG/slice) |
| Field mutations (cart / DTO safety) | `rgctl -f json cpg mutations --type ShoppingCart --exclude-ctors` (needs `--with-cfg`) |
| Data flows / slice (CPG) | `rgctl -f json cpg flows FILE --line N --variable V --function F [--direction forward\|backward] [--with-alias]` |
| Loop-carried DFG tags | `rgctl discover . --with-cfg --with-dfg-loops` |
| AST skeleton | `rgctl discover --with-ast-skeleton` then `rgctl -f json cpg ast <Symbol>` |
| CPG export | `rgctl cpg export --format graphson --output cpg.json [--path-contains src/]` |
| Migration plan | `rgctl -f json discover . --with-cfg --with-security --with-taint --with-dashboard --with-harmonic --export-migration-hints` (stdout = migration plan JSON; also writes `.rgctl/migration_plan.json`) |
| CI gate on changes | `rgctl -f json check --policy-file policy.json` (exit 1 = violations) |
| Temporal PR gate | `rgctl -f json pr-check --policy-file rgctl-pr-policy.json --base-artifact .rgctl-base --base-ref origin/main --head-ref HEAD --strict` |
| Check temporal bridge | `rgctl -f json check --temporal --policy-file policy.json --base-ref origin/main --head-ref HEAD` |
| Incremental file index | `rgctl update` or `rgctl update --files src/foo.rs` (requires existing `.rgctl/`; alias: `discover --files`) |
| Live refresh while serving | `rgctl serve --watch` |

---

## Repeated queries in one session

**Option A — CLI subprocess (default for agents):**

```bash
export REPO=/path/to/repo
rgctl -r "$REPO" -f json find --type function --limit 5
rgctl -r "$REPO" -f json callers ShoppingCartService --depth 2
rgctl -r "$REPO" -f json blast-radius ShoppingCartService
```

**Option B — HTTP (one long-lived process):**

```bash
rgctl -r "$REPO" serve --open
# rgctl -f json find --type function --limit 5
```

See [HTTP Server and Dashboard](../guides/http-server-and-dashboard.md).

---

## Rules of thumb

0. **Artifacts** — always `{repo}/.rgctl/` after `discover`. Add `.rgctl/` to `.gitignore`.
1. **Index first** — `find`/`callers`/`relations`/`inventory`, `blast-radius`, `metrics` fail without `discover`.
2. **Discover target** — `rgctl -r PATH discover .` (or `cd PATH && rgctl discover .`); relative PATH joins `-r`.
3. **Use `-f json`** — stable `schema_version` fields; see [json-api.md](../json-api.md). Errors under JSON emit `error_code` / `recovery_command` on stdout.
4. **Disambiguate symbols** — `inspect` / `cpg` / `blast-radius` accept `--file` / `--class` / `--line`.
5. **`slice --function`** accepts bare method or `Class::method` / `Class.method`.
6. **`export --query`** uses filter syntax (`name:Foo`, `type:Function`, `all`) — not Cypher `MATCH`.
7. **Deep analysis** needs `discover --with-cfg` (and `--with-taint` for discover-time taint).
8. **Semantic search** needs `semantic index` (separate from discover). Default embedder is **vocab**.
9. **Dashboard is optional** — only when a human wants a UI.
10. **Markdown docs** — indexed on `discover`; see [markdown-context.md](../markdown-context.md).

---

## On-disk artifacts

After `discover`, under **`{repo}/.rgctl/`**: `graph.snapshot.bin`, optional `semantic_index.bin`, dashboard payloads when enabled.

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Policy violation or command error |

## See also

- [Introduction](../Introduction.md) · [User Guide](../user-guide.md) · [Agent recipes](../agent-recipes.md)
