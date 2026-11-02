---
name: rgctl-migrate
description: >-
  Plan a Strangler-Fig extraction with rgctl discover --export-migration-hints,
  migration_plan.json v2 steps, Fast-Leiden communities, and blast-radius on
  boundary symbols. Use when asked for a migration roadmap, extraction order,
  or service-boundary audit.
rgctl-managed: true
---

# rgctl-migrate (`/rgctl:migrate`)

Dependency-ordered Strangler-Fig migration planning via **`--export-migration-hints`**, **`communities list`**, and **`blast-radius`**. No forge posts; no `serve` / `--watch` required (optional dashboard is out of scope for this skill).

Bound stdout: filter plan steps and communities with `jq '.[0:20]'`; use `--depth 2` or `--depth 3` on `blast-radius` probes (neither `blast-radius` nor `communities list` accepts `--limit`).

## When to Use

- User wants a migration / extraction roadmap or “what should we move first?”
- User is modernizing legacy/mainframe sources (`cobol`, `pli`, `jcl`, …) or carving a service boundary.
- User asks for coupling bottlenecks or test/adapter handoffs for a Strangler-Fig cut.

## Prerequisites

- Work in the repo root (`REPO_PATH`, default `.`).
- Choose preset/order to match intent (defaults below are fine for a first pass).

Do **not** start `rgctl serve` solely for this skill.

## Steps

### Step 1: Discover + migration plan

```bash
cd "${REPO_PATH:-.}"
PRESET="${migration_preset:-hybrid_default}"
ORDER="${migration_order:-scheduled}"
# When legacy sources exist, add e.g.: -l cobol,pli,jcl
# LANG_FLAGS="-l cobol,pli,jcl"
LANG_FLAGS="${LANG_FLAGS:-}"

rgctl discover . --export-migration-hints --with-harmonic \
  --migration-preset "${PRESET}" --migration-order "${ORDER}" \
  ${LANG_FLAGS}
```

Other presets: `foundational_first`, `dense_cluster`, `risk_mitigation`. Order: `scheduled` (dependency-safe) or `priority` (score rank).

### Step 2: Parse `migration_plan.json` (schema v2)

```bash
jq '.schema_version, [.steps[] | select(.label != "root") | {
  step, label, schedule_step, priority_rank, avg_pagerank, avg_harmonic
}] | .[0:20]' "${REPO_PATH:-.}/.rgctl/migration_plan.json"
```

Ignore the synthetic `"root"` step. Summarize ordered waves — do not paste the full plan into chat.

### Step 3: Communities + blast radius on boundaries

```bash
rgctl -f json communities list \
  | jq '[.communities[] | select(.member_count > 1)] | .[0:20]'

# For top extraction candidates / shared boundary symbols:
rgctl -f json blast-radius "<boundary_symbol>" --depth 3
```

Prefer multi-member clusters (`member_count > 1`). Skip unlabeled singletons when labeling ownership (`select(.name | startswith("Community ") | not)` on named overlays).

### Step 4: Strangler-Fig Extraction Brief

```markdown
## Extraction Brief

## Ordered phases (from steps[])
## Coupling bottlenecks (communities + blast-radius)
## Shared boundaries / adapters to introduce
## Test handoffs
## Sources (.rgctl/migration_plan.json, communities, blast-radius)
```

## Gotchas

- Always `-f json` on query verbs; parse stdout; never `2>/dev/null`.
- `migration_plan.json` uses **`.steps[]`** (v2), not v1 `packages[]`.
- PageRank / harmonic names are already on plan steps and `metrics --pagerank` JSON — no `cpg function` UUID lookup.
- Constrain heavy discovers with `--with-limits` in containers when needed.
