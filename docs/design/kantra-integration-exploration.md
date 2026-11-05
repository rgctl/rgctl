# Kantra Integration — Design & Current State (`--with-kantra`)

Native evaluation of [Konveyor Kantra](https://github.com/konveyor/kantra) migration rules against the rgctl graph and source cache — **no external LSP, Kantra CLI, or container runtime**.

> **Status:** Shipped. Implemented in the [`rgctl-kantra`](../../crates/rgctl-kantra) crate and wired into `discover --with-kantra`. This document records the design rationale and the *current* evaluation coverage. For end-user instructions see the [user guide](../user-guide.md#kantra-migration-rules---with-kantra) and [`crates/rgctl-kantra/README.md`](../../crates/rgctl-kantra/README.md); for output schema see [json-api.md](../json-api.md#kantra_findingsjson).
>
> This doc describes behaviour as of the current tree. When coverage changes, the source of truth is `crates/rgctl-kantra/src/classify.rs` (`is_supported_provider` / `UNSUPPORTED_PROVIDERS`) and the `WhenClause` variants in `schema.rs` — update the tables in §5 to match.

---

## 1. Context

Kantra is the Konveyor CLI for static code analysis and transformation. It uses YAML-based rules with a `when` clause that dispatches to language providers (`java.referenced`, `go.referenced`, `builtin.filecontent`, etc.). Rules produce violations (issues) with severity, effort estimates, and remediation guidance.

rgctl already indexes a rich graph of symbols, imports, call edges, annotations, dependencies, and community structure. When `--with-kantra` runs it preloads the source text it needs into its own cache (§6.1). That makes it possible to evaluate the large majority of real-world Kantra rules directly against the graph and source cache — and, crucially, to *enrich* each violation with graph context (blast radius, community, centrality) that Kantra alone cannot provide.

The runtime embeds the Konveyor `stable/java` catalog (~2.6k rules) in release binaries, so `discover --with-kantra` works with no rule setup.

---

## 2. Relationship to rgctl's other rule/security systems

rgctl has four other rule and security systems. Kantra evaluation (`rgctl-kantra`) is independent of all of them and does not share their rule format:

| System | Crate | Trigger | Scope |
|--------|-------|---------|-------|
| Taint analysis | `rgctl-analysis` (`taint.rs`) | `--with-taint` | Hardcoded per-language source/sink/sanitizer `text.contains()` over PDG nodes |
| CWE/security patterns | `rgctl-security` | `--with-security` | 10 hardcoded CWE patterns over taint flows |
| Rule engine | `rgctl-rules` | (library) | JSON rulesets over graph nodes (`And/Or/Not` + leaf conditions, label/metadata actions) |
| Policy/check | `rgctl-service` | `rgctl check --policy-file` | Blast-radius guardrails as a CI gate |

Kantra findings are *not* currently fed into the `rgctl check` policy gate — see [§9 Boundaries & open questions](#9-boundaries--open-questions).

---

## 3. Kantra rule format (reference)

A Kantra rule is YAML with three parts: metadata, condition (`when`), and action (message/tag).

```yaml
- ruleID: crypto-change-00010
  description: "x/crypto/hkdf bypasses FIPS module"
  category: mandatory          # mandatory | optional | potential
  effort: 3                    # story points
  labels:
    - konveyor.io/source=go
    - konveyor.io/target=go
  message: "detailed remediation with ### Before / ### After"
  links:
    - url: https://pkg.go.dev/crypto/hkdf
      title: "Migration Documentation"
  when:
    go.referenced:
      pattern: golang.org/x/crypto/hkdf
```

### 3.1 Condition types

| Provider | Capability | Fields |
|----------|-----------|--------|
| `builtin` | `filecontent` | `pattern` (regex), `filePattern` (glob) |
| `builtin` | `file` | `pattern` (glob) |
| `builtin` | `xml` | `xpath`, `namespaces`, `filepaths` |
| `builtin` | `json` | `xpath`, `filepaths` |
| `builtin` | `hasTags` | (inline list) |
| `java` | `referenced` | `pattern`, `location`, `annotated` |
| `java` | `dependency` | `name`, `upperbound`, `lowerbound` |
| `go` | `referenced` | `pattern` |
| `go` | `dependency` | `name`, `upperbound`, `lowerbound` |

Conditions compose with `and:`, `or:`, `not:`.

### 3.2 Java `location` values

`IMPORT`, `PACKAGE`, `TYPE`, `INHERITANCE`, `ANNOTATION`, `IMPLEMENTS_TYPE`, `FIELD`, `METHOD`, `METHOD_CALL`, `CONSTRUCTOR_CALL`, `CLASS`, `RETURN_TYPE`, `VARIABLE_DECLARATION`, `ENUM_CONSTANT`.

### 3.3 Ruleset structure

Rules live in a directory with a `ruleset.yaml` golden file plus one or more `*.yaml` rule files:

```
my-rules/
  ruleset.yaml          # { name, description, labels }
  cloud-readiness.yaml  # list of rules
  crypto.yaml           # list of rules
```

---

## 4. Architecture

### 4.1 Chosen approach: native evaluation with graph enrichment

Kantra YAML is parsed natively (serde + yaml). Each rule's `when` clause is classified by evaluability; supported conditions are evaluated against the graph and source cache; unsupported conditions are skipped with a recorded reason; and surviving violations are enriched with graph metrics.

```
  ┌─────────────────────────────────────────────────────────┐
  │  1. Resolve catalog (embedded | --kantra-catalog tree |  │
  │     --kantra-rules single ruleset dir)                   │
  │  2. Classify each when clause → RuleSupport              │
  │       Supported | Partial | Unsupported                 │
  │  3. Evaluate supported leaves against graph + sources    │
  │       (compose And/Or/Not over per-leaf match sites)     │
  │  4. Enrich violations (community, pagerank, blast radius)│
  │  5. Emit .rgctl/kantra_findings.json                     │
  │       + KantraRule/KantraRuleset nodes + VIOLATES edges  │
  └─────────────────────────────────────────────────────────┘
```

Key modules in `rgctl-kantra`: `loader`/`catalog` (YAML + embedded blob), `classify`, `engine` + `eval/*` (per-provider matchers), `enrich`, `findings`, `index` (graph nodes), `violates` (edges), `cache` (incremental).

### 4.2 Alternatives considered

- **Translation layer** — translate Kantra YAML into the existing `rgctl-rules` `MatchCondition`. Rejected: lossy for source/file/dependency conditions, and the rule engine has no source-text or manifest access.
- **Kantra as peer runner** — shell out to the `kantra` CLI and ingest its `output.yaml`. Rejected as the default: 100% fidelity but requires the Kantra CLI + podman/docker and pays JDT LS / container startup. Still a viable complement for the conditions rgctl cannot evaluate (see [§9 Boundaries](#9-boundaries--open-questions)); native eval is positioned as a fast pre-check.

---

## 5. Current coverage

Coverage is driven by `classify.rs` and the `WhenClause` variants in `schema.rs`. A rule is `Supported` when all its providers are supported, `Partial` when it mixes supported and unsupported providers, `Unsupported` otherwise (or when it contains an invalid regex).

### 5.1 Per-condition coverage

| Kantra condition | rgctl data source | Status | Notes |
|------------------|-------------------|--------|-------|
| `builtin.filecontent` | `SourceCache` (preloaded source text) | **Full** | Regex per line; results cached in `.rgctl/kantra_cache/` |
| `builtin.file` | graph `File` nodes / file list | **Full** | Glob match on paths |
| `builtin.hasTags` | node `labels[]` | **Full** | |
| `go.referenced` | `Import` nodes (`qualified_name`) | **Full** | Pattern match on Go import path |
| `java.referenced` `IMPORT` | `Import` nodes | **Full** | Boundary-anchored Kantra glob→regex on import name |
| `java.referenced` `TYPE`/`CLASS` | `Class`/`Interface`/`Enum` nodes | **Full** | Match `qualified_name` or `name` |
| `java.referenced` `PACKAGE` | Imports, external types, call-site hints | **Full** | Graph/call-site only (no comment grep) |
| `java.referenced` `INHERITANCE`/`IMPLEMENTS_TYPE` | `EXTENDS`/`IMPLEMENTS` edges | **Full** | Uses import-FQN stubs (Phase 0 external-stub edges) |
| `java.referenced` `ANNOTATION` | `ANNOTATED_WITH` edges + `Annotation` nodes | **Full** | Match annotation name/qualified_name; `annotated.pattern` applied when present |
| `java.referenced` `annotated.pattern` | Annotation edges/nodes | **Partial** | Supported for `ANNOTATION`; `annotated.elements` remains unsupported |
| `java.referenced` `CONSTRUCTOR_CALL` / `METHOD_CALL` | `.rgctl/parked_relations.bin` + Instantiates/Calls edges | **Full** | Exact 1-based call-site line; `confidence: high` |
| `java.referenced` `METHOD` / `FIELD` / `RETURN_TYPE` / `THROWS` / `VARIABLE` | Function/Variable nodes + FQN properties | **Full** | Symbol metadata (`return_type_fqn`, `throws`, `param_type_fqn:*`, …) |
| `java.dependency` / `go.dependency` | `Dependency` nodes from manifest extraction | **Full** | Colon↔dot coordinate normalize + `lowerbound`/`upperbound`; static POM/manifest scope only (no remote Maven resolution) |
| `builtin.xml` (xpath) | on-demand XML reparse | **Partial** | `/a/b`, `//tag`, `|`, `prefix:local`, `[@attr]`, `local-name()`/`namespace-uri()`; unsupported XPath → `skipped_rules` |
| `builtin.json` (jsonpath) | on-demand JSON reparse | **Partial** | Minimal path subset |
| `and:` / `or:` / `not:` | composed over per-leaf match sites | **Full** | `eval_compose` in `engine.rs` |
| `java.referenced` with `annotated.elements` | — | **Unsupported** | Listed in `UNSUPPORTED_PROVIDERS`; no structured per-element decomposition (edge `metadata["arguments"]` is an opaque string) |
| Cross-condition chaining (`as:` / `from:`) | — | **Unsupported** | No inter-condition variable passing |
| Custom variable interpolation in messages | — | **Unsupported** | Formatting concern, low priority |

Note on confidence: graph/call-site hits set `confidence: "high"` and `matched_location`. Only residual unknown locations still use a lexical line-scan (`confidence: "low"`). Tag-only Konveyor rules (`tag:` without `category`/`effort`) are recorded in `skipped_rules` and never emit incident violations.

**JR1 boundary:** in-repo inheritance and import-resolved FQNs are covered. Transitive third-party JAR superclass hierarchies (Eclipse JDT LS classpath resolution) remain out of scope without `--scip` or bytecode enrichment.

### 5.2 Coverage of real-world rulesets

Rough distribution of condition types in [konveyor/rulesets](https://github.com/konveyor/rulesets). The percentages below are **approximate, hand-estimated guidance** (not a measured census) — treat them as order-of-magnitude only:

| Pattern | Approx. frequency | Status |
|---------|-------------------|--------|
| `*.referenced` with simple pattern | ~60% | Full |
| `builtin.filecontent` | ~25% | Full |
| `builtin.file` | ~5% | Full |
| Composite `and:`/`or:`/`not:` of the above | ~5% | Full |
| `*.dependency` version checks | ~3% | Full |
| `builtin.xml`/`builtin.json` | ~1% | Partial (minimal path subset) |
| `java.referenced` with `annotated.elements` | ~1% | Unsupported |

Rules that cannot be evaluated are not dropped silently — they are recorded in `skipped_rules` with a reason.

---

## 6. Graph data used for evaluation

### 6.1 Source text in memory

`run_kantra_stage` always builds its own `SourceCache` via `preload_discovered_sources()` over the discovered file list (parallel read); this is independent of the CFG pipeline's `FileSourceCache` and happens whether or not `--with-cfg` is set. `builtin.filecontent`, `builtin.xml`/`builtin.json`, and the `java.referenced` fallbacks regex-match against it.

### 6.2 Import nodes

Import statements are first-class graph nodes (`NodeType::Import`):

| Language | `name` | `qualified_name` |
|----------|--------|------------------|
| Java | Full import text (`import java.util.List;`) | `None` |
| Go | Short name (last path segment) | Full import path (`github.com/pkg/errors`) |
| C/C++ | `#include` path | — |
| C# | `using` directive | — |

### 6.3 Annotation nodes and edges

Java annotations are extracted during **initial graph indexing** (plain `discover`, no `--with-cfg`):

- Annotation type declarations (`@interface Foo`) → `NodeType::Annotation` nodes.
- Annotation usage (`@RequestMapping(path="/x")`) → `EdgeType::AnnotatedWith` edges from the annotated symbol to the annotation type.
- Annotation arguments → opaque raw string in edge `metadata["arguments"]` (e.g. `(path = "/x")`) — regex-matchable but not decomposed into key-value pairs (hence `annotated.elements` is unsupported).
- Unresolved annotations (`@Override`) → external stub `Annotation` nodes.

### 6.4 Dependency nodes

Manifest parsing (`rgctl-extraction/src/manifests/`) produces `NodeType::Dependency` nodes with `qualified_name` shaped as `ecosystem:coord` and an `ecosystem:` label. `java.dependency`/`go.dependency` match on ecosystem + name + version bounds (`java`/`maven`/`gradle` coords are treated compatibly).

### 6.5 Symbol metadata and edges

Nodes carry `qualified_name`, `signature`, `return_type`, `parameters`, `file_path`/`start_line`/`end_line`, an open `properties` bag, and `labels`. Edge types used by eval: `Calls`, `Uses`, `EXTENDS`, `IMPLEMENTS`, `ANNOTATED_WITH`, `DefinedIn`, `Contains`.

---

## 7. Pipeline integration

### 7.1 CLI flags

| Flag | Purpose |
|------|---------|
| `--with-kantra` | Run Kantra eval and index `KantraRule`/`KantraRuleset` nodes into the session graph |
| `--kantra-target NAME` | Evaluate only rules labeled `konveyor.io/target=NAME` |
| `--kantra-rules DIR` | Single ruleset directory (`ruleset.yaml` + `*.yaml`); overrides embedded catalog |
| `--kantra-catalog ROOT` | Rulesets tree (e.g. `stable/java`); overrides embedded catalog |
| `--kantra-index-only` | Skip eval; still index catalog rules into the graph |

`--with-kantra` does **not** imply `--with-cfg`, and Kantra evaluation does not use CFG/PDG/taint results today (enrichment is community/centrality/blast-radius only — see §8).

### 7.2 Where it hooks in

In `src/cli/discover_impl.rs`, Kantra runs in three split phases within `discover`:

1. **Eval** (`kantra_eval`) — `run_kantra_stage` runs after community/centrality (so those metrics exist) and before the CFG batch / `BlastRadiusEngine`. Builds the source cache, evaluates rules, writes initial `kantra_findings.json`.
2. **Enrich** (`kantra_enrich`) — `run_kantra_enrich` runs after `BlastRadiusEngine` is built, so `blast_radius_score` / `impact_zone_size` are available to fill in.
3. **Index + violates** (`kantra_index`, `kantra_violates`) — `run_kantra_index` and `run_kantra_violates` run at the very end, after `drop(cold)`, so rewriting `graph.snapshot.bin` (rule nodes + `VIOLATES` edges) doesn't mutate a still-open mmap.

Stage timings are recorded separately in the discover profile: `kantra_eval`, `kantra_enrich`, `kantra_index`, `kantra_violates` (see `src/cli/stage_profile.rs`).

### 7.3 Outputs

- **`.rgctl/kantra_findings.json`** — `schema_version`, `catalog_id`, `ruleset`, `target_filter`, `evaluated_rules`, `violations[]` (with per-violation `enrichment`), `skipped_rules[]`, and `cache_hits`/`cache_misses`. Schema: [json-api.md](../json-api.md#kantra_findingsjson).
- **Graph nodes** — `NodeType::KantraRuleset` and `NodeType::KantraRule`, linked by `Contains` edges; queryable via `rgctl -f json find --type kantrarule`.
- **`VIOLATES` edges** — after full eval, `EdgeType::Violates` links each `KantraRule` node to matched code nodes; `rgctl relations --edge violates --from-type kantrarule`.
- **Dashboard** — with `--with-universe`, exports `kantra_index.json` + per-file violation shards (Universe **Static rules** room).

### 7.4 Incremental caching

`builtin.filecontent` results are cached under `.rgctl/kantra_cache/`, keyed by file content hash and ruleset hash (`ruleset_hash`). Unchanged files against an unchanged ruleset are skipped (`cache_hits`).

---

## 8. The graph advantage

This is what differentiates native evaluation from running Kantra directly. Each violation is enriched (`enrich.rs` → `ViolationEnrichment`) with:

| Enrichment | What it adds |
|------------|--------------|
| **Blast radius** (`blast_radius_score`, `impact_zone_size`) | "This symbol is reached by N functions across the graph" |
| **Community** (`community_id`) | "All violations cluster in community 7 — your payment module" |
| **Centrality** (`pagerank`) | "This is a high-PageRank hotspot — fixing it has outsized impact" |

Kantra tells you *what* is wrong. The graph tells you *how much it matters*.

> **Future work:** correlating a violation with taint/CWE flow ("this match is also on a vulnerable taint path") is *not* implemented — `rgctl-kantra` does not consume CFG/PDG/taint results. It would require threading taint output into the enrich stage.

---

## 9. Boundaries & open questions

### Boundaries

1. **No structured annotation-element matching** — `annotated: { elements: [...] }` is unsupported; the raw `metadata["arguments"]` string can only be regex-grepped via a filecontent rule. The `annotated.pattern` form is parsed but currently ignored (see §5.1).
2. **Minimal XML/JSON path support** — `builtin.xml`/`builtin.json` handle a small XPath/JSON-path subset, not the full spec.
3. **JR1 classpath inheritance** — third-party JAR supertypes still require SCIP/bytecode enrichment; in-repo + import-resolved FQNs are covered via parked call sites.
4. **No cross-condition chaining** (`as:`/`from:`) and **no custom variable interpolation**.
5. **No taint/CWE correlation** — enrichment is community/centrality/blast-radius only; `rgctl-kantra` does not consume CFG/PDG/taint output (see §8 future work).

Items 2–5 are where the Kantra peer-runner (§4.2) remains strictly more capable; native eval is the fast, container-free path for the large majority (roughly ~90%, by the approximate estimate in §5.2) of rules it does cover.

### Open questions

1. **`check` integration** — fold mandatory Kantra violations into `rgctl check` so a single CI gate covers blast-radius policy *and* migration-rule severity.
2. **Kantra-compatible `output.yaml`** — emit alongside `kantra_findings.json` for Konveyor Hub ingestion?
3. **Rules scaffolding** — `rgctl rules init` to generate a Kantra-compatible ruleset skeleton.

---

## 10. Related docs

- [User guide — Kantra migration rules](../user-guide.md#kantra-migration-rules---with-kantra)
- [`crates/rgctl-kantra/README.md`](../../crates/rgctl-kantra/README.md)
- [JSON API — `kantra_findings.json`](../json-api.md#kantra_findingsjson)
- [Taint Analysis Design](taint-analysis-design.md)
- [CI Policy Checks Design](ci-policy-checks-design.md)
- [Blast Radius Design](blast-radius-design.md)
- [Kantra Rules Quickstart](https://github.com/konveyor/kantra/blob/main/docs/rules-quickstart.md)
- [Konveyor Rulesets](https://github.com/konveyor/rulesets)
- [analyzer-lsp Rules Reference](https://github.com/konveyor/analyzer-lsp/blob/main/docs/rules.md)
