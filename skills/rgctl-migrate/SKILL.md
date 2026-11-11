---
name: rgctl-migrate
description: >-
  Code-intelligence migration planning and lossless write-back execution with
  rgctl: A→B framework migrations (Spring Boot→Quarkus, JavaEE→Quarkus,
  javax→jakarta, FastAPI↔Flask, Express↔Fastify, Actix↔Axum), custom/LLM-planned
  library upgrades (JUnit 4→5, internal SDKs), API/package refactors, and
  Strangler-Fig architectural extraction. Prefer rgctl structural queries and
  lossless write-back first.
rgctl-managed: true
---

# rgctl-migrate (`/rgctl:migrate`)

Plan and execute code migrations with **rgctl code intelligence + lossless write-back** — not grep-first.

## Step 0: Acquire `.rgctl/` First (mandatory)

```bash
cd "${REPO_PATH:-.}"
rgctl -f json status
# For A→B framework, library, or symbol write-back migrations (Tracks A & B):
rgctl discover . --lossless
# For Strangler-Fig architectural decomposition (Track C):
rgctl discover . --export-migration-hints --with-harmonic
```

Prefer `rgctl -f json` (`writeback`, `inventory`, `find`, `relations`, `callers`, `blast-radius`, `slice`) **before** reading source files.

---

## Choose a Migration Track

| Track | Scenario | Primary Engine |
|-------|----------|----------------|
| **Track A1 — Catalog A→B Framework Migration** | Spring Boot → Quarkus, JavaEE → Quarkus, OpenLiberty / WebSphere → Quarkus, `javax` → `jakarta`, Spring Boot 2 → 3, FastAPI ↔ Flask, Express ↔ Fastify, Actix ↔ Axum | `rgctl writeback migrate --from <A> --to <B>` + `agent_worklist[]` |
| **Track A2 — Uncataloged / Custom A→B Library Upgrade** | JUnit 4 → JUnit 5, internal company SDK v1 → v2, custom annotation/namespace rewrites | Structural census → author `migration-plan.json` → `rgctl writeback migrate --plan migration-plan.json` |
| **Track B — API, Package & Symbol Refactoring** | Cross-repo symbol rename, package move, DTO/method signature evolution | `rgctl writeback patch --rename Old=New` (or `--plan`) + `callers` / `blast-radius` / `cpg mutations` |
| **Track C — Architectural / Strangler-Fig Extraction** | Carve a bounded context or microservice out of a monolith | `rgctl discover . --export-migration-hints --with-harmonic` → `.rgctl/migration_plan.json` `.steps[]` |

Optional note: if sources include COBOL/PL/I/JCL, add `-l cobol,pli,jcl` on `discover` — do **not** add mainframe flags for mainstream Java/Kotlin/TS/Python/Rust/Go work.

---

## Track A1 — Catalog A→B Framework Migration (e.g., Spring → Quarkus, JavaEE → Quarkus)

`rgctl` ships compiled bidirectional write-back profiles (`frameworks.bin`) across 10 languages (`java`, `kotlin`, `python`, `typescript`, `javascript`, `rust`, `csharp`, `go`, `php`, `ruby`). It deterministically executes **Waves 1–3** (imports/namespaces, class/field/method stereotypes/DI/routes, and parameter annotations) while preserving 100% of comments and formatting, and outputs a blast-radius-ordered `agent_worklist[]` for the remaining ~10–20% of behavioral code.

### 1. Discover & Inspect Available Framework Profiles

```bash
# 1) Build graph + lossless Zstd source store + Green-CST sidecars
rgctl discover . --lossless

# 2) List built-in write-back framework profiles (optional --lang filter)
#    Java/Kotlin catalog IDs: spring, javaee, openliberty, jakarta-ee, quarkus
#    (If a framework is not in this list, use Track A2 `--plan migration-plan.json`!)
rgctl -f json writeback frameworks --lang java

# 3) Run pre-migration structural census (imports, annotations, DI, routes)
rgctl -f json inventory --by import-prefix --limit 50
rgctl -f json find --annotation "@RestController,@Controller,@Service,@Repository,@Stateless,@Autowired,@EJB,@Inject,@Value,@PersistenceContext,@RequestMapping,@GetMapping,@PostMapping,@Path" --show-attributes --limit 50
rgctl -f json relations --edge injects --limit 50
rgctl -f json relations --edge exposesroute --limit 50
```

### 2. Scenario Command Recipes (Waves 1–3 Deterministic Splicing)

#### Scenario A1.1: Spring Boot → Quarkus (`spring` → `quarkus`)
Rewrites `org.springframework.*` / `javax.*` imports, `@RestController`/`@Service`/`@Repository` → `@ApplicationScoped`, `@Autowired` → `@Inject`, `@Value("${k:default}")` → `@ConfigProperty(name = "k", defaultValue = "default")`, `@RequestMapping`/`@GetMapping`/`@PostMapping` → `@Path`/`@GET`/`@POST`, `@PathVariable`/`@RequestParam`/`@RequestHeader` → `@PathParam`/`@QueryParam`/`@HeaderParam`, and flags `JpaRepository` / `SecurityFilterChain` in `agent_worklist[]`.

```bash
# Preview wave counts, Trust Ledger, and agent_worklist[] without touching disk:
rgctl -f json writeback migrate --from spring --to quarkus --dry-run

# Apply in-place (automatically refreshes .rgctl/ lossless sidecars after Wave 3):
rgctl -f json writeback migrate --from spring --to quarkus --in-place
```

#### Scenario A1.2: JavaEE (`javax.*` / EJB / JAX-RS) → Quarkus (`javaee` → `quarkus`)
Rewrites `javax.*` → `jakarta.*` across imports and qualified type references (never inside comments/strings), `@Stateless`/`@Singleton` → `@ApplicationScoped`, `@EJB`/`@PersistenceContext` → `@Inject`, `@TransactionAttribute(...)` → `@Transactional`, and flags `AbstractStatelessSessionBean` / `SessionContext.getCallerPrincipal()` in `agent_worklist[]`.

```bash
# Preview:
rgctl -f json writeback migrate --from javaee --to quarkus --dry-run

# Apply in-place:
rgctl -f json writeback migrate --from javaee --to quarkus --in-place
```

#### Scenario A1.3: JavaEE `javax.*` → Jakarta EE `jakarta.*` (`javaee` → `jakarta-ee`)
```bash
rgctl -f json writeback migrate --from javaee --to jakarta-ee --dry-run
rgctl -f json writeback migrate --from javaee --to jakarta-ee --in-place
```

#### Scenario A1.4: OpenLiberty / WebSphere → Quarkus (`openliberty` → `quarkus`)
Rewrites `javax.*` → `jakarta.*` across imports and qualified type references, `@Stateless`/`@Singleton` → `@ApplicationScoped`, `@EJB`/`@PersistenceContext` → `@Inject`, `@TransactionAttribute(...)` → `@Transactional`, preserves MicroProfile `@ConfigProperty`, and flags WebSphere/Liberty specifics (`WSSubject`, `InitialContext`, `HttpServlet`, `ServletContextListener`, `RestClientBuilder`) in `agent_worklist[]`.

```bash
rgctl -f json writeback migrate --from openliberty --to quarkus --dry-run
rgctl -f json writeback migrate --from openliberty --to quarkus --in-place
```

#### Scenario A1.5: Spring Boot 2 → Spring Boot 3 (`spring` → `spring`)
```bash
rgctl -f json writeback migrate --from spring --to spring --dry-run
rgctl -f json writeback migrate --from spring --to spring --in-place
```

#### Scenario A1.6: Staged Wave-by-Wave Execution (Optional)
```bash
rgctl -f json writeback migrate --from spring --to quarkus --wave 1 --in-place   # Wave 1: imports & namespaces
rgctl -f json writeback migrate --from spring --to quarkus --wave 2 --in-place   # Wave 2: class/field/method annotations
rgctl -f json writeback migrate --from spring --to quarkus --wave 3 --in-place   # Wave 3: parameter annotations
```

### 3. Resolve `agent_worklist[]` (Tier-D Behavioral Idioms & Build Files)

Parse `.agent_worklist[]` from the `writeback migrate` JSON output. Items are already ordered by `blast_radius_order` (`1..N`). For each item:

```bash
# 1) Inspect callers and blast radius of the worklist symbol
rgctl -f json callers "<symbol>" --depth 2
rgctl -f json blast-radius "<symbol>" --depth 2 --summary

# 2) Slice data flow around call triggers (e.g. SessionContext, SecurityContextHolder)
rgctl -f json slice "<file>" --line <start_line> --variable <var>

# 3) Apply targeted LLM edits to the behavioral item & build file (pom.xml / build.gradle)
```

### 4. Post-Migration Verification Gate

```bash
rgctl discover . --lossless
rgctl -f json writeback verify --strict
rgctl -f json find "import org.springframework*" --type import   # expect total == 0
rgctl -f json find "import javax*" --type import                 # expect total == 0
rgctl -f json review paths --synthetic-head worktree --max-symbols 25
```

---

## Track A2 — Uncataloged / Custom A→B Migration (`--plan migration-plan.json`)

When migrating a testing framework (e.g. **JUnit 4 → JUnit 5**), third-party library, or internal SDK not in `writeback frameworks`:

### 1. Run Structural Census Queries

```bash
rgctl discover . --lossless
rgctl -f json inventory --by import-prefix --limit 50
rgctl -f json find "import org.junit*" --type import --limit 50
rgctl -f json find --annotation "@Before,@After,@BeforeClass,@AfterClass,@Ignore,@Test,@RunWith,@Rule" --show-attributes --limit 50
```

### 2. Author `migration-plan.json`

```json
{
  "name": "junit4-to-junit5",
  "language": "java",
  "namespace_rewrites": {
    "org.junit.Assert": "org.junit.jupiter.api.Assertions"
  },
  "import_rewrites": {
    "org.junit.Test": "org.junit.jupiter.api.Test",
    "org.junit.Before": "org.junit.jupiter.api.BeforeEach",
    "org.junit.After": "org.junit.jupiter.api.AfterEach",
    "org.junit.BeforeClass": "org.junit.jupiter.api.BeforeAll",
    "org.junit.AfterClass": "org.junit.jupiter.api.AfterAll",
    "org.junit.Ignore": "org.junit.jupiter.api.Disabled"
  },
  "annotation_rewrites": [
    { "from": "Before", "to": "@BeforeEach", "imports": ["org.junit.jupiter.api.BeforeEach"] },
    { "from": "After", "to": "@AfterEach", "imports": ["org.junit.jupiter.api.AfterEach"] },
    { "from": "BeforeClass", "to": "@BeforeAll", "imports": ["org.junit.jupiter.api.BeforeAll"] },
    { "from": "AfterClass", "to": "@AfterAll", "imports": ["org.junit.jupiter.api.AfterAll"] },
    { "from": "Ignore", "to": "@Disabled", "imports": ["org.junit.jupiter.api.Disabled"] },
    { "from": "Test", "to": "@Test", "imports": ["org.junit.jupiter.api.Test"] }
  ],
  "worklist_triggers": {
    "annotations": ["RunWith", "Rule"],
    "supertypes": [],
    "calls": []
  }
}
```

### 3. Execute Plan & Verify

```bash
rgctl -f json writeback migrate --plan migration-plan.json --dry-run
rgctl -f json writeback migrate --plan migration-plan.json --in-place
# Resolve any @RunWith / @Rule items in agent_worklist[]
rgctl discover . --lossless
rgctl -f json writeback verify --strict
```

---

## Track B — API, Package & Symbol Refactoring (`writeback patch`)

```bash
rgctl discover . --lossless

# 1) Enumerate package members, callers, field mutations, and blast radius
rgctl -f json find --package <maven.or.npm.coords> --limit 40
rgctl -f json callers <TypeOrMethod> --depth 2
rgctl -f json blast-radius <TypeOrMethod> --depth 2 --summary
rgctl -f json inspect <TypeOrMethod> ast
rgctl -f json cpg mutations --type <DTO> --exclude-ctors   # needs discover --with-cfg
rgctl -f json slice <FILE> --line N --variable V

# 2) Lossless CST token-boundary symbol rename (never mutates comments or string literals)
rgctl -f json writeback patch --rename OldSymbol=NewSymbol --dry-run
rgctl -f json writeback patch --rename OldSymbol=NewSymbol --in-place

# Or apply explicit byte-span edits from a JSON plan:
rgctl -f json writeback patch --plan mutations.json --in-place

# 3) Verify
rgctl discover . --lossless
rgctl -f json writeback verify --strict
```

---

## Track C — Architectural / Strangler-Fig Service Decomposition

Use when extracting a bounded context or microservice from a monolith:

```bash
# 1) Generate dependency-scheduled migration plan with harmonic centrality
rgctl discover . --export-migration-hints --with-harmonic

# 2) Inspect ordered extraction phases (v2 .steps[] schema)
jq '[.steps[] | select(.label != "root")] | .[0:20]' .rgctl/migration_plan.json

# 3) Cross-check Fast-Leiden domain communities and boundary blast radius
rgctl -f json communities list | jq '[.communities[] | select(.member_count > 1)] | .[0:20]'
rgctl -f json blast-radius "<boundary_symbol>" --depth 3 --summary
rgctl -f json callers "<boundary_symbol>" --scope <pkg> --scope-mode outside
```

Presets (`--migration-preset`): `hybrid_default` | `foundational_first` | `dense_cluster` | `risk_mitigation`.
Order (`--migration-order`): `scheduled` | `priority`.

### Extraction Brief Template (Track C Deliverable)

```markdown
## Extraction Brief
## Ordered phases (from .rgctl/migration_plan.json .steps[])
## Coupling bottlenecks (communities + blast-radius)
## Shared boundaries / adapters
## Test handoffs
## Sources (.rgctl/migration_plan.json, communities, blast-radius)
```

---

## Gotchas

- Always pass `-f json` on query and write-back verbs; parse `schema_version` + payload from stdout; never use `2>/dev/null`.
- `migration_plan.json` (Track C Strangler-Fig) uses **`.steps[]`** (v2), not v1 `packages[]`.
- Inventory JSON uses **`.counts`**, not `.items`.
- Prefer `blast-radius --summary` and `inventory --limit` over dumping huge payloads into context.
