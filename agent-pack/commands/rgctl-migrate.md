---
name: /rgctl:migrate
id: rgctl-migrate
category: Workflow
description: >-
  Execute A→B framework migrations (Spring Boot→Quarkus, JavaEE→Quarkus,
  javax→jakarta), custom/LLM-planned library upgrades (--plan), API/symbol
  refactors, or Strangler-Fig architectural extraction with rgctl. Session-only.
---

Run the **rgctl-migrate** skill protocol for this repository.

**Canonical skill:** `rgctl-migrate` (installed next to meta skill `rgctl`).

**Input:** Optional args after the command — `--from <src> --to <dst>`, `--plan <file.json>`, `--migration-preset` / `--migration-order`, language filters (e.g. `-l java` or `-l cobol,pli,jcl`), or free-text describing what to migrate or extract.

Follow `skills/rgctl-migrate/SKILL.md` (or the host’s installed copy under the agent skills root).

## Track A1 — Catalog A→B Framework Migration (Spring→Quarkus, JavaEE→Quarkus, `javax`→`jakarta`)

1. `rgctl discover . --lossless`
2. `rgctl -f json writeback frameworks --lang java`
3. Preview & apply Waves 1–3:
   - Spring → Quarkus: `rgctl -f json writeback migrate --from spring --to quarkus --dry-run` → `--in-place`
   - JavaEE → Quarkus: `rgctl -f json writeback migrate --from javaee --to quarkus --dry-run` → `--in-place`
   - `javax` → `jakarta`: `rgctl -f json writeback migrate --from javaee --to jakarta-ee --in-place`
   - Spring Boot 2 → 3: `rgctl -f json writeback migrate --from spring --to spring --in-place`
4. Resolve `agent_worklist[]` in `blast_radius_order` (`callers`, `blast-radius --summary`, `slice`) + update `pom.xml` / `build.gradle`
5. `rgctl discover . --lossless` → `rgctl -f json writeback verify --strict` → `rgctl -f json find "import <legacy>*" --type import` (`total == 0`)

## Track A2 — Custom / Uncataloged Library Migration (`--plan`)

1. `rgctl discover . --lossless`
2. Census: `rgctl -f json inventory --by import-prefix --limit 50` + `rgctl -f json find --annotation "@Before,@After,@Test" --show-attributes`
3. Author `migration-plan.json` (`namespace_rewrites`, `import_rewrites`, `annotation_rewrites`, `worklist_triggers`)
4. `rgctl -f json writeback migrate --plan migration-plan.json --dry-run` → `--in-place`
5. Resolve `agent_worklist[]` → `rgctl discover . --lossless` → `rgctl -f json writeback verify --strict`

## Track B — API, Package & Symbol Refactoring (`writeback patch`)

1. `rgctl discover . --lossless`
2. `rgctl -f json callers <Sym> --depth 2` + `rgctl -f json blast-radius <Sym> --summary`
3. `rgctl -f json writeback patch --rename OldName=NewName --in-place` (or `--plan mutations.json`)
4. `rgctl discover . --lossless` → `rgctl -f json writeback verify --strict`

## Track C — Strangler-Fig Architectural Extraction

1. `rgctl discover . --export-migration-hints --with-harmonic`
2. Parse `.rgctl/migration_plan.json` v2 `.steps[]` where `label != "root"`
3. `rgctl -f json communities list` (`member_count > 1`) + `rgctl -f json blast-radius <boundary> --depth 3 --summary`
4. Emit Strangler-Fig Extraction Brief (ordered phases, coupling bottlenecks, test/adapter handoffs)

Do **not** post to GitHub/GitLab. Do **not** start `rgctl serve` / `--watch`.
