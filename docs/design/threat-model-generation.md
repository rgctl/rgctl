# Threat-Model Evidence Extraction & STRIDE Synthesis (`rgctl vuln threat-model`)

**Status:** Implemented (`schema_version: 1`, GitHub Issue [#229](https://github.com/rgctl/rgctl/issues/229))  
**Primary Crates:** `rgctl-security` (`threat_model.rs`), `rgctl` CLI (`src/cli/threat_model.rs`), `rgctl-vuln` skill (Mode C)

---

## 1. System Overview & End-to-End Data Flow

`rgctl` splits threat-model generation into **two strictly separated layers** so the deterministic Rust binary remains fast, reproducible, and offline-safe while downstream LLM agents (`/rgctl:vuln`) synthesize business context without hallucinating unsupported claims:

1. **Layer 1 — Deterministic Structural & CPG Evidence Engine (`rgctl vuln threat-model`, alias `rgctl vuln surface`):**  
   Operates 100% read-only on `.rgctl/graph.snapshot.bin` hot columnar arrays plus optional sidecars (`parked_relations.bin`, `analysis_results.bin`, `callable_resolution.json`, `.rgctl/osv/`, and bounded on-demand per-function CFG/PDG). Requires **zero** new `discover` passes and **zero** changes to `NodeRow` / `EdgeRow` layouts.
2. **Layer 2 — Contextual Agent Workflow (`/rgctl:vuln` Mode C):**  
   Consumes `rgctl -f json vuln threat-model --offline`, cross-references repository architecture docs (`README.md`, `SECURITY.md`, or `discover -l markdown,<lang>`) and PR call-path diffs (`rgctl review paths`), and refines `needs_review` candidates while preserving explicit `evidence_refs` and `assumptions`.

```text
  .rgctl/graph.snapshot.bin (mmap hot columns: NodeType, EdgeType, spans)
  ├── .rgctl/parked_relations.bin   (external calls + import/receiver hints)
  ├── .rgctl/analysis_results.bin   (Fast-Leiden subsystem communities)
  ├── .rgctl/callable_resolution.json (indirect / unresolved call-site count)
  └── .rgctl/osv/*.json | --osv     (offline OSV / lockfile / OpenVEX reachability)
                   │
                   ▼
     ┌──────────────────────────────────────────────────────────────┐
     │  rgctl_security::threat_model::build_threat_model (Layer 1)  │
     │  1. Entry Points: ExposesRoute, handler annotations, CLI main│
     │  2. Bounded BFS (max_depth=8): reachable call cones          │
     │  3. Lazy CFG/PDG (--max-pdg-functions, default 64)          │
     │  4. Assets: UsesConfig secrets/endpoints, Tables, Models     │
     │  5. Trust Boundaries: Ingress, Fast-Leiden bridges, Egress   │
     │  6. STRIDE Rules: graph_rule + needs_review + evidence_refs  │
     └──────────────────────────────────────────────────────────────┘
          │                       │                        │
   -f json (v1)             -f mermaid                -f text (GFM)
   Deterministic JSON       `flowchart LR` DFD        Human Markdown
          │
          ▼
     ┌──────────────────────────────────────────────────────────────┐
     │  /rgctl:vuln Skill — Mode C (Layer 2 Contextual Synthesis)   │
     │  • Cross-checks docs & out-of-repo gateway/IAM assumptions   │
     │  • Differential PR attack-surface review (`review paths`)    │
     │  • Preserves `graph_fact` vs `graph_rule` vs `agent_inference`│
     └──────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations & Capability / Gap Matrix

### 2.1 Threat-Modeling Foundations Mapped to `rgctl`

| Concept | Origin | Mapping in `rgctl` |
| :--- | :--- | :--- |
| **Data-Flow Diagrams (DFD) & Trust Boundaries** | Shostack (2014), DeMarco DFD | External actors → `ExposesRoute` / `@PostMapping` / `CLI main` (`ExternalIngress`), cross-cluster calls across Fast-Leiden partitions in `analysis_results.bin` (`SubsystemBridge`), and outbound `parked_relations.bin` I/O (`ExternalEgress`). |
| **STRIDE-per-Element / STRIDE-per-Interaction** | Kohnfelder & Garg (1999) | Deterministic graph rules (`StrideCategory`) triggered over `(EntryPoint, DataFlow, Sink, Asset, ReachableVuln)` tuples, always tagged `provenance: "graph_rule"` and `review_status: "needs_review"`. |
| **Intra-Procedural Dominance Taint** | Ferrante, Ottenstein & Warren (1987) PDG | `ensure_cfg_pdg_for_source` + `TaintAnalyzer` (`crates/rgctl-analysis/src/taint.rs`) distinguishes `proven_unsanitized_flow` (`cpg_taint`) from `sanitized_flow` and `no_local_source_flow` without upfront `discover --with-cfg`. |
| **Reachable Supply-Chain Advisories (VEX)** | CISA OpenVEX v0.2.0 / OSV | `evaluate_repo_vulns` joins lockfile/manifest dependency ranges with call-graph and lazy PDG reachability (`provenance: "osv_reachability"`). |

### 2.2 Capability & Gap Matrix (Research Questions #1–#5)

| Threat-Model Dimension | Directly Observable by `rgctl` (`graph_fact` / `cpg_taint` / `osv_reachability`) | Deterministic Rule Inference (`graph_rule` / `needs_review`) | Requires LLM / Human / Infrastructure Context (`agent_inference` / Out-of-Scope) |
| :--- | :--- | :--- | :--- |
| **Entry Points & Attack Surface** | HTTP routes (`ExposesRoute`), REST/RPC/queue annotations (`@PostMapping`, `@KafkaListener`, `@GrpcService`), CLI `main` functions, exact `file:line`. | Distinguishing public vs internal endpoints when no auth decorator/callee appears in the local handler cone. | API Gateway / Ingress routing, network exposure (VPC-internal vs internet-facing), CORS/TLS termination. |
| **AuthN / AuthZ & Input Controls** | Decorator/annotation presence (`@PreAuthorize`, `@RolesAllowed`), DI guard types (`Injects`), direct auth/validator calls (`require_auth`, `jwt.verify`, `shlex.quote`), PDG dominator sanitizers. | **Spoofing (`S`)** & **Elevation of Privilege (`E`)** candidates when external handlers lack observed auth guards before reaching privileged sinks. | Whether a custom middleware registered globally in startup/config enforces auth; RBAC/ABAC tenant-isolation correctness (IDOR / BOLA). |
| **Sensitive Assets** | `UsesConfig` edges (`STRIPE_SECRET_KEY`, `JWT_SECRET`, `DATABASE_URL`), SQL `Table` nodes, domain structs/classes (`User`, `Payment`, `Credential`) referenced by reachable handlers. | **Information Disclosure (`I`)** candidates on credential/secret config consumers and unescaped render/egress flows. | PII/PCI regulatory classification of arbitrary custom fields; encryption-at-rest / KMS / Vault policies. |
| **Sinks & Data Flows** | Multi-hop `Calls` chains (`EntryPoint -> ... -> Sink`), external `parked_relations.bin` calls, and intra-procedural PDG source-to-sink flows (`SqlQuery`, `ShellCommand`, `FileWrite`, `CodeEval`, `HtmlRender`, `NetworkOutput`). | **Tampering (`T`)**, **Repudiation (`R`)**, and **Denial of Service (`D`)** candidates when mutating/resource-heavy flows lack observed sanitizers, audit logs, or rate-limit guards. | Cross-service async message flows (unless merged via `rgctl merge`), reflective/eval string construction, ORM implicit parameterization nuances. |
| **Known CVEs** | Lockfile/SBOM version match + `parked_relations` / `dep_stubs.bin` call sites + OpenVEX PURLs (`OSV-*`). | **Elevation of Privilege (`E`)** candidates linking `exploitable` / `under_investigation` CVEs to reaching entry points. | Runtime feature flags outside `.rgctl/vuln-rules.d/`, container base-image OS packages without an SBOM. |

---

## 3. Data Structures, Epistemic Provenance & Memory Discipline

### 3.1 Five-Tier Epistemic Provenance Ladder

Every record in `ThreatModelReport` carries an explicit [`EvidenceProvenance`](../../crates/rgctl-security/src/threat_model.rs) enum value:

1. `graph_fact` — Directly extracted AST symbol or typed relation (`ExposesRoute`, `Calls`, `UsesConfig`, `AnnotatedWith`, `Injects`).
2. `cpg_taint` — Verified intra-procedural CFG/PDG data-flow or dominator-sanitizer status (`PdgSinkStatus`).
3. `osv_reachability` — Lockfile + call-graph/PDG OSV reachability verdict (`VulnAnalyzeResult`).
4. `graph_rule` — Deterministic STRIDE rule candidate (`ThreatCandidate`). Invariant: **always** `review_status: "needs_review"`, `>= 1` resolvable ID in `evidence_refs[]`, and non-empty `assumptions[]`.
5. `agent_inference` — Reserved for Layer-2 `/rgctl:vuln` contextual enrichment so LLM hypotheses can never masquerade as `graph_fact` or `cpg_taint`.

### 3.2 Memory & Hot-Path Discipline

- **Zero `discover` overhead:** `rgctl discover .` is completely untouched (0 bytes added to `Symbol`, `Relation`, `NodeRow`, or `EdgeRow`; Gate A cold baselines unaffected).
- **Streaming hot-column scan:** `build_threat_model` reads `node_type_at`, `node_name_str`, `node_qualified_name_str`, `node_file_path_str`, `node_lines_at`, and `for_each_edge` directly from `ColumnarGraphMmap` without touching `ColdMetadataDb` (`NodeExtension` blobs).
- **Bounded lazy CFG/PDG (`--max-pdg-functions`, default `DEFAULT_MAX_LAZY_PDG_FUNCTIONS = 64`):** Functions reachable from entry points are ordered by `(min_hops, file, start_line)` and evaluated lazily in memory (`persist: false`) up to the budget, while still scanning 100% of graph + parked sinks repo-wide.
- **Measured scale (linux, 1.81M nodes, release, M-series):** ~5.0 s wall (`load_nodes` 0.33 s, `edge_pass` 0.25 s, `lazy_pdg` 0.03 s), ~2.0 GiB peak memory footprint, byte-identical JSON across runs. Node metadata borrows strings zero-copy from `ColumnarGraphMmap` with an interned file table; sink classification is memoized per (callee, language); lazy PDG groups functions per file on Rayon using snapshot `decl_byte_span` directly; and parked calls are classified on Rayon then applied in record order. Enable `RUST_LOG=profile=info` for per-stage `[profile] threat-model …` timings on `stderr`.
- **No silent caps:** every bound that can hide evidence is reported in `limitations_and_gaps` with counts — `CALL_DEPTH_LIMITED` (BFS frontier hit `--max-depth` with unexplored callees) and `PDG_BUDGET_EXHAUSTED` (reachable functions beyond `--max-pdg-functions`, or all of them under `--no-lazy-cfg`). The Mermaid DFD caps nodes for legibility but always emits an `OMITTED` node with counts; JSON is never truncated.
- **Sink lines:** PDG-verified and parked (external) sinks carry the exact statement line. Graph-only sinks (from a `Calls` edge, no PDG coverage) carry the **caller's start line**, because hot edge columns store no call-site line.
- **Offline-by-default determinism:** Zero outbound HTTP requests occur unless `--online` is explicitly passed. All collections use canonical `(file, line, symbol)` / `BTreeMap` ordering so consecutive runs produce byte-identical JSON, Markdown, and Mermaid outputs.

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 Reproducible CLI Commands

```bash
# 1. Index repository (plain `discover .` suffices — no `--with-cfg` needed)
cd /path/to/repo && rgctl discover .

# 2. Emit versioned deterministic JSON threat-model pack (offline by default)
rgctl -f json vuln threat-model --offline > .rgctl/threat-model.json
# Alias:
rgctl -f json vuln surface --offline

# 3. Join local OSV advisory or scanner report (zero network calls)
rgctl -f json vuln threat-model --osv path/to/advisories/
rgctl -f json vuln threat-model --from-report trivy.json --sbom bom.cdx.json

# 4. Emit standalone Mermaid Data-Flow Diagram or Markdown report
rgctl -f mermaid vuln threat-model --offline -o .rgctl/threat-model.mmd
rgctl vuln threat-model --offline -o THREAT_MODEL.md
```

### 4.2 JSON Schema (`schema_version: 1`)

```json
{
  "schema_version": 1,
  "command": "vuln threat-model",
  "summary": {
    "entry_points": 2,
    "assets": 2,
    "sinks": 1,
    "trust_boundaries": 2,
    "data_flows": 1,
    "reachable_vulnerabilities": 0,
    "threat_candidates": 5,
    "stride_counts": {
      "denial_of_service": 1,
      "elevation_of_privilege": 1,
      "information_disclosure": 1,
      "repudiation": 1,
      "spoofing": 1,
      "tampering": 1
    },
    "unresolved_or_indirect_calls": 0
  },
  "entry_points": [
    {
      "id": "EP-001",
      "kind": "http_route",
      "name": "POST /exec",
      "handler_symbol": "app.run_diag",
      "http_method": "POST",
      "route_path": "/exec",
      "file": "app.py",
      "line": 19,
      "observed_auth_controls": [],
      "observed_validation_controls": [],
      "provenance": "graph_fact"
    }
  ],
  "assets": [
    {
      "id": "AST-001",
      "kind": "secret_or_credential_config",
      "name": "STRIPE_SECRET_KEY",
      "consumers": ["app.py"],
      "provenance": "graph_fact"
    }
  ],
  "sinks": [
    {
      "id": "SNK-001",
      "sink_category": "shell_command",
      "callee": "subprocess.run",
      "caller": "app.run_diag",
      "file": "app.py",
      "line": 21,
      "is_external_call": true,
      "pdg_taint": { "status": "proven_unsanitized_flow", "source_line": 20 },
      "provenance": "cpg_taint"
    }
  ],
  "trust_boundaries": [
    {
      "id": "TB-001",
      "kind": "external_ingress",
      "from_zone": "External Caller",
      "to_zone": "Application",
      "crossing_count": 2,
      "sample_crossings": ["POST /exec -> app.run_diag"],
      "provenance": "graph_fact"
    }
  ],
  "data_flows": [
    {
      "id": "DF-001",
      "entry_point_id": "EP-001",
      "sink_id": "SNK-001",
      "sink_category": "shell_command",
      "call_path": ["app.run_diag"],
      "hops": 0,
      "crossed_communities": [],
      "has_observed_auth": false,
      "has_observed_sanitizer": false,
      "has_observed_audit_log": false,
      "pdg_taint": { "status": "proven_unsanitized_flow", "source_line": 20 },
      "provenance": "cpg_taint"
    }
  ],
  "reachable_vulnerabilities": [],
  "threat_candidates": [
    {
      "id": "THR-002",
      "stride": "tampering",
      "title": "Potential integrity / injection path from `POST /exec` to `subprocess.run` (`shell_command`)",
      "description": "...",
      "affected_component": "app.run_diag",
      "provenance": "graph_rule",
      "review_status": "needs_review",
      "confidence": "high",
      "evidence_refs": ["EP-001", "SNK-001", "DF-001"],
      "assumptions": ["Caller-supplied inputs reach sink arguments without custom wrapper parameterization not listed in built-in taint rules."],
      "suggested_mitigations": ["Use parameterized queries / prepared statements, strict allowlists, or typed schema validation before invoking the sink."]
    }
  ],
  "evidence_index": {
    "EP-001": { "id": "EP-001", "provenance": "graph_fact", "kind": "entry_point", "symbol_or_target": "app.run_diag", "file": "app.py", "line": 19, "summary": "..." }
  },
  "limitations_and_gaps": [
    { "code": "OUT_OF_REPO_CONTROLS", "category": "infrastructure_visibility", "detail": "..." },
    { "code": "PDG_BUDGET_EXHAUSTED", "category": "data_flow_completeness", "detail": "..." }
  ]
}
```

### 4.3 Implementation Map

| File | Responsibility |
| :--- | :--- |
| [`crates/rgctl-security/src/threat_model.rs`](../../crates/rgctl-security/src/threat_model.rs) | `THREAT_MODEL_SCHEMA_VERSION = 1` types, hot-column `build_threat_model`, STRIDE rules, Mermaid DFD & Markdown renderers, unit tests. |
| [`src/cli/threat_model.rs`](../../src/cli/threat_model.rs) | CLI handler, sidecar & offline OSV wiring, parallel per-file cached lazy CFG/PDG `TaintAnalyzer` callback (`inspect_functions_pdg_parallel`). |
| [`src/cli/mod.rs`](../../src/cli/mod.rs) & [`src/cli/vuln_deps.rs`](../../src/cli/vuln_deps.rs) | `VulnCommands::ThreatModel` (`visible_alias = "surface"`) and shared `evaluate_repo_vulns` batch pipeline. |
| [`skills/rgctl-vuln/SKILL.md`](../../skills/rgctl-vuln/SKILL.md) & [`skills/rgctl/SKILL.md`](../../skills/rgctl/SKILL.md) | Layer-2 Mode C agent workflow, epistemic & privacy rules, and natural-language routing. |
| [`tests/vuln_threat_model.rs`](../../tests/vuln_threat_model.rs) | End-to-end integration tests across Web/FastAPI, Java/Maven+OSV, and Rust CLI/batch archetypes + determinism & epistemic invariants. |
| [`tests/threat_model_cli.rs`](../../tests/threat_model_cli.rs) | Fixture-scale gate on `rgctl-tests/ecommerce-{java,python,rust}`: contract, auth suppression, CLI `main`, byte-identical output, and reported depth / PDG-budget caps. |

---

## 5. Representative Fixture Evaluation & Manual Baseline Comparison

### 5.1 Three Representative Application Archetypes

Two layers of evidence: the dogfood multi-service fixtures (`rgctl-tests/ecommerce-*`, gated by [`tests/threat_model_cli.rs`](../../tests/threat_model_cli.rs)) and small controlled archetypes written inline in [`tests/vuln_threat_model.rs`](../../tests/vuln_threat_model.rs) that exercise sinks, taint, and OSV joins the fixtures do not contain. Fixture numbers below are from a cold `discover .` + `vuln threat-model --offline` (no OSV cache).

**Dogfood fixtures (`rgctl-tests/ecommerce-*`):**

| Fixture | Entry Points | Assets | Sinks / Flows | Trust Boundaries | STRIDE Candidates | Limitations reported | Notes |
| :--- | ---: | ---: | ---: | ---: | :--- | :--- | :--- |
| `ecommerce-python` (FastAPI) | 27 HTTP routes (13 with observed auth) | 0 | 0 / 0 | 19 | 14 `Spoofing` (auth-guarded routes suppressed) | `OUT_OF_REPO_CONTROLS`, `DYNAMIC_OR_INDIRECT_CALLS`, `OSV_OFFLINE_EMPTY` | Persistence is SQLAlchemy ORM (parameterized), so no injection sinks. Pydantic `BaseSettings` fields (`jwt_secret`, `database_url`) are **not** extracted as config nodes, so no assets: an extraction gap, not a threat-model rule. |
| `ecommerce-java` (Spring) | 31 (30 REST routes, 5 with observed auth, + 1 `main`) | 8 (`spring.datasource.*`, `app.jwt.*`, `User`, `Order`, …) | 0 / 0 | 22 | 25 `Spoofing`, 2 `InfoDisclosure` | + `PDG_BUDGET_EXHAUSTED` (reachable set exceeds the default 64) | Global `SecurityFilterChain` auth is invisible per-route, so most `Spoofing` candidates are `needs_review` framing, not findings. JPA repositories are parameterized, so no injection sinks. |
| `ecommerce-rust` (CLI/batch) | 1 (`CLI main`) | 3 (`JWT_SECRET`, `DATABASE_URL`, `BIND_ADDR`) | 27 (26 `sqlx` queries, 1 log) / 5 | 8 | 1 `InfoDisclosure` (`JWT_SECRET`) | `OUT_OF_REPO_CONTROLS`, `DYNAMIC_OR_INDIRECT_CALLS`, `OSV_OFFLINE_EMPTY` | No HTTP `Spoofing` noise on a local CLI `main`. `sqlx` queries take literal SQL, so no `Tampering`. |

**Controlled archetypes (`tests/vuln_threat_model.rs`):**

| Archetype | What it proves |
| :--- | :--- |
| `archetype_1_python_web_app` | `request.query_params` → `subprocess.run` is `proven_unsanitized_flow` without `--with-cfg`; high-confidence `Tampering` + `EoP` on unauthenticated `POST /exec`; `require_auth` route suppresses `Spoofing`; env secrets become assets. |
| `archetype_2_java_enterprise` (`--osv` jackson CVE-2022-42004) | Maven `pom.xml` + `jackson-databind@2.13.4` reachability joined to the exposing `@PostMapping("/api/orders")` controller as an `EoP` candidate linked to `OSV-001`. |
| `archetype_3_rust_cli` | `main -> deploy_bundle -> Command::new` `Tampering`; callback indirection (`hook(target)`) surfaced as `DYNAMIC_OR_INDIRECT_CALLS`. |
| `multi_route_flows_keep_evidence_index_consistent…` | Byte-identical output across runs; every `DF-*` evidence record describes its own flow; `get_author` / `invalidate_cache` are not auth / validation controls. |
| `scheduled_jobs_are_not_http_ingress` | `@Scheduled` handlers are `scheduled_job` entry points and never yield `Spoofing`. |
| `unrelated_numeric_cast_and_ratelimit_never_suppress_proven_unsanitized_taint` | Unrelated numeric casts (`int(...)`) and rate-limit helpers (`rate_limit()`) never mask `proven_unsanitized_flow` `Tampering` candidates, while `rate_limit()` still suppresses `DenialOfService`. |
| `overloaded_methods_report_proven_unsanitized_flow_without_cache_collision` | Overloaded Java methods (`query(int)` vs `query(HttpServletRequest)`) resolve by `Uuid` + `decl_byte_span` without colliding in per-file PDG caches. |

### 5.2 Manual Threat-Model Baseline Comparison (Signal vs Noise)

| Finding Class | Outcome vs Manual Baseline | Analysis |
| :--- | :--- | :--- |
| **True Positives (High Signal)** | Unauthenticated routes (`POST /checkout/{cart_id}`, `POST /exec`), command/SQL sinks reachable without sanitizers (`THR` `confidence: high`), plaintext secret env keys (`STRIPE_SECRET_KEY`, `JWT_SECRET`), and reachable dependency CVEs (`CVE-2022-42004`). | Directly grounded in `file:line` graph/PDG/OSV evidence; zero hallucinated symbols or paths. |
| **False Positives & Framing** | Intentionally public read-only routes (`GET /`, `GET /api/categories`) or routes protected by Spring Security `SecurityFilterChain` / external API gateway trigger `Spoofing` (`confidence: medium`). | Framed honestly as `review_status: "needs_review"` with explicit `assumptions` citing out-of-repo gateways and global runtime filters rather than false vulnerability alerts. |
| **False Negatives / Unsupported** | Business-logic abuse (e.g. negative cart quantity, horizontal BOLA/IDOR when `@PreAuthorize` checks role but not `cart.owner_id == user.id`), reflective/macro-generated routes (e.g. builder-chained Axum `Router::route` without `ExposesRoute` edges), and dynamic function-pointer targets. | Documented in `limitations_and_gaps` (`OUT_OF_REPO_CONTROLS`, `DYNAMIC_OR_INDIRECT_CALLS`) and delegated to Layer-2 `/rgctl:vuln` human/agent review. |

---

## 6. Known Limitations, Privacy Guidance & Productization Architecture

1. **Why Subcommand + Skill (`rgctl vuln threat-model` + `/rgctl:vuln`), Not a New Top-Level Verb:**  
   Keeping deterministic extraction under `rgctl vuln threat-model` (alias `surface`) reuses `SharedVulnGraphResources`, `parked_relations.bin`, `dep_stubs.bin`, and `ensure_cfg_pdg_for_source` while preserving the `<= 3` high-ceremony slash-command budget (`/rgctl:review`, `/rgctl:vuln`, `/rgctl:migrate`).
2. **Dynamic Dispatch & Reflection Honesty:**  
   Calls through function parameters, trait objects without static receiver bounds, or runtime reflection are counted in `summary.unresolved_or_indirect_calls` (from `.rgctl/callable_resolution.json`) and surfaced as `DYNAMIC_OR_INDIRECT_CALLS` rather than guessed.
3. **LLM Privacy & Data-Handling Boundary:**  
   Layer 1 (`rgctl vuln threat-model --offline`) runs 100% locally with zero network egress and emits compact structural metadata (`symbol`, `file:line`, edge kinds) rather than source bodies. When using Layer 2 (`/rgctl:vuln`) against proprietary repositories, agents should pass only `.rgctl/threat-model.json` projections and narrow `rgctl slice` excerpts in accordance with organizational LLM policies.
