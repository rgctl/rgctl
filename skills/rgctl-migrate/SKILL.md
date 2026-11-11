---
name: rgctl-migrate
description: >-
  Code-intelligence migration planning with rgctl: framework/runtime/version
  upgrades, API/package/library refactors, and Strangler-Fig architectural
  extraction. Prefer rgctl structural queries first. Use when asked for a
  migration roadmap, modernization plan, javax→jakarta / Quarkus / Spring Boot
  upgrade, package move, or service-boundary cut.
rgctl-managed: true
---

# rgctl-migrate (`/rgctl:migrate`)

Plan and execute migrations with **rgctl code intelligence** — not grep-first.

## Step 0: Acquire `.rgctl/` First (mandatory)

```bash
cd "${REPO_PATH:-.}"
rgctl -f json status   # if graph.snapshot.bin missing →
rgctl discover .
```

Prefer `rgctl -f json` (`inventory`, `find`, `relations`, `callers`, `blast-radius`, `slice`) **before** reading source files. For multi-wave work, start `rgctl serve --watch` or run `rgctl update` between waves.

## Choose a track

| Track | When | Lead with |
|-------|------|-----------|
| **A — Framework / Runtime / Version** | EE→Quarkus, Spring Boot 2→3, Jakarta namespace, JDK bumps | import census + annotations + DI/routes |
| **B — API / Package / Library** | Move packages, rename types, swap libraries | `find --package`, callers, CPG mutations, blast-radius |
| **C — Architectural / Strangler-Fig** | Extract a service / bounded context | `--export-migration-hints`, communities, boundary blast-radius |

Optional note: if sources are COBOL/PL/I/JCL, add `-l cobol,pli,jcl` on discover — do **not** lead with mainframe flags for mainstream Java/TS/Go work.

## Track A — Framework, Runtime & Version Migration

```bash
rgctl -f json inventory --by import-prefix --limit 50
# optional focus: --prefix javax.  or --prefix org.springframework.

rgctl -f json find "import <legacy_prefix>*" --type import --limit 50
rgctl -f json find --annotation "@PersistenceContext,@Inject,@ConfigProperty,@Autowired,@Path,@RequestMapping" --show-attributes --limit 40
rgctl -f json relations --edge injects --limit 40
rgctl -f json relations --edge exposesroute --limit 40
rgctl -f json callers <entry_sym> --depth 2
rgctl -f json blast-radius <entry_sym> --summary
```

Order edit waves by callers / blast-radius. After each wave: `rgctl update` → re-check `find "import <legacy>*" --type import` (`total == 0`).

## Track B — API, Package & Library Refactoring

```bash
rgctl -f json find --package <maven.or.npm.coords> --limit 40
rgctl -f json callers <TypeOrMethod> --depth 2
rgctl -f json cpg mutations --type <DTO> --exclude-ctors   # needs discover --with-cfg
rgctl -f json slice <FILE> --line N --variable V
rgctl -f json blast-radius <TypeOrMethod> --summary
```

## Track C — Architectural / Strangler-Fig Decomposition

```bash
rgctl discover . --export-migration-hints --with-harmonic
jq '[.steps[] | select(.label != "root")] | .[0:20]' .rgctl/migration_plan.json
rgctl -f json communities list | jq '[.communities[] | select(.member_count > 1)] | .[0:20]'
rgctl -f json blast-radius "<boundary_symbol>" --depth 3 --summary
```

Presets: `hybrid_default` | `foundational_first` | `dense_cluster` | `risk_mitigation`. Order: `scheduled` | `priority`.

### Extraction brief template

```markdown
## Extraction Brief
## Ordered phases (from steps[])
## Coupling bottlenecks (communities + blast-radius)
## Shared boundaries / adapters
## Test handoffs
## Sources (.rgctl/migration_plan.json, communities, blast-radius)
```

## Post-edit verification

```bash
rgctl update
rgctl -f json find "import <legacy>*" --type import   # expect total == 0
rgctl -f json review paths --synthetic-head worktree --max-symbols 25
```

## Gotchas

- Always `-f json` on query verbs; parse stdout; never `2>/dev/null`.
- `migration_plan.json` uses **`.steps[]`** (v2), not v1 `packages[]`.
- Inventory JSON uses **`.counts`**, not `.items`.
- Prefer `blast-radius --summary` / `inventory --limit` over dumping huge payloads.
