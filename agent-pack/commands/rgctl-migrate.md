---
name: /rgctl:migrate
id: rgctl-migrate
category: Workflow
description: >-
  Plan a Strangler-Fig extraction with rgctl discover --export-migration-hints,
  migration_plan.json steps, Fast-Leiden communities, and blast-radius on
  boundary symbols. Emits a short Extraction Brief. Session-only.
---

Run the **rgctl-migrate** skill protocol for this repository.

**Canonical skill:** `rgctl-migrate` (installed next to meta skill `rgctl`).

**Input:** Optional args after the command — `--migration-preset` / `--migration-order`, language filters (e.g. `-l cobol,pli,jcl`), or free-text describing what to extract or modernize.

Follow `skills/rgctl-migrate/SKILL.md` (or the host’s installed copy under the agent skills root):

1. `rgctl discover . --export-migration-hints --with-harmonic` (preset/order + legacy langs when needed)
2. Parse `.rgctl/migration_plan.json` v2 `.steps[]` where `label != "root"`
3. `communities list` (`member_count > 1`) + `blast-radius` on top boundary candidates
4. Emit Strangler-Fig Extraction Brief (ordered phases, coupling bottlenecks, test/adapter handoffs)

Do **not** post to GitHub/GitLab. Do **not** start `rgctl serve` / `--watch`.
