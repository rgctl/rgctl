# Taint Analysis & Vulnerability Reachability — Engineering Design

This document specifies the **Taint Analysis, Sink-First Reachability, and OSV/OpenVEX Vulnerability Triage subsystem** in `rgctl` (`crates/rgctl-analysis/src/taint.rs`, `crates/rgctl-security/`, `src/cli/slice.rs`, and `src/cli/vuln_deps.rs`). It covers intra- and inter-procedural source-to-sink taint propagation, sanitizer neutralization, custom YAML rule packs, `.sink_taint` parked call-site reachability, and end-to-end CVE/OSV analysis (`rgctl taint`, `rgctl vuln`, `rgctl deps`).

> **Updated (vuln-reachability-tier1-overhaul):** This document has been updated to reflect the following changes:
> - **Multi-convention OSV symbol extraction** (`SymbolCoverage::Explicit` / `Unspecified`) across Go `imports[].symbols`, RustSec `affects.functions`, and JVM/GHSA `affected_methods`/`affected_classes`.
> - **Multi-interval version ranges** (`Vec<VersionInterval>` with exclusive `Fixed` and inclusive `LastAffected` bounds) replacing single `(introduced, fixed)` overwrite.
> - **Epistemic verdict ladder** (`decide_verdict` 4-state matrix): `SymbolCoverage::Unspecified` with import hits → `under_investigation` (never false-negative `not_exploitable`); `--strict-reachability` escalates to `exploitable`.
> - **Lazy sink CPG hydration** (`ensure_cfg_pdg_for_source` + `TaintAnalyzer`) integrated into `vuln analyze` without requiring upfront `discover --with-cfg`.
> - **BlastRadiusEngine** reverse reachability wired into call-site path enrichment.
> - **Canonical PURL** (`pkg:<type>/<namespace>/<name>@<version>`) in OpenVEX `products`/`subcomponents`.
> - **SARIF 2.1.0** (`-f sarif`) with `codeFlows`/`threadFlows` file+line traces.
> - **Phased Tier-1 lockfile expansion**: `Cargo.lock`, `go.sum`, `gradle.lockfile`, `Package.resolved`, `vcpkg.json` (Wave A); `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `Pipfile.lock`, `uv.lock`, `poetry.lock`, `requirements.txt`, `packages.lock.json`, `composer.lock`, `Gemfile.lock`, `pubspec.lock`, `mix.lock`, `Puppetfile.lock`, `*.rockspec` (Wave B).
> - `VersionEngine::in_any_vulnerable_range` added with multi-range and `listed_versions` fallback across all 14 Tier-1 package ecosystems.
> - **Network OSV feed fetching & strict local overrides** (`rgctl vuln fetch`, `rgctl vuln scan`, `--osv <file|dir>`, `--from-report <trivy|grype|osv-scanner.json>`, `--sbom <cdx|spdx.json>`, `--offline`, `RGCTL_OSV_API_URL` / `--osv-api-url`).
> - **Single-mmap Rayon batch evaluation** (`rgctl vuln scan` and directory `--osv` on `vuln analyze`) opening `graph.snapshot.bin`, `parked_relations.bin`, and `blast_engine.snapshot.bin` once.
> - **Opt-in transitive call-stub stitching & conditional CVE rules**: `--with-dep-stubs` (`.rgctl/dep_stubs.bin` zero-decompile JVM `.class` constant-pool/bytecode scanner + vendor source stubs) and `.rgctl/vuln-rules.d/*.yaml` declarative trigger-condition rules.

![Taint Analysis tab — flows table (gbuilder)](../images/design/taint-analysis/taint-flows.png)

*Figure 1: **Taint Analysis** tab — per-function flow list with CWE severity badges, sanitizer status, and source→sink paths.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. TAINT SOURCES, SINKS & SANITIZER CATALOGS (`taint.rs` + `rgctl-security`) │
│                                                                              │
│  • Built-in multi-language CWE rule packs (SQLi CWE-89, CmdInj CWE-78,       │
│    XSS CWE-79, Path Traversal CWE-22, SSRF CWE-918, Deserialization CWE-502) │
│  • Project overrides: `.rgctl/taint-rules.d/*.yaml` + `--taint-rules <PATH>` │
│  • OSV advisory affected methods (`affected[].ecosystem_specific.imports`)   │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. TWO EXECUTION PATHS (`crates/rgctl-analysis/src/taint.rs`)                │
│                                                                              │
│  PATH A: Criterion-First Taint Slice (`rgctl slice <file> --line N --taint`) │
│    ├─► Lazily builds single-function CFG/PDG via `ensure_cfg_pdg_for_source` │
│    └─► Forward taint propagation from `(line, variable)` across PDG data     │
│        edges (`(name, scope_id)` + field sensitivity `base.field`),          │
│        neutralizing paths that pass through matching sanitizers              │
│                                                                              │
│  PATH B: Sink-First & OSV Reachability (`rgctl vuln` / `rgctl taint --sink`) │
│    ├─► `rgctl vuln analyze|scan` hydrates per-call-site CFG/PDG lazily on    │
│    │   demand (no upfront `discover --with-cfg` required; `taint --sink`     │
│    │   uses the prebuilt archive when scanning the whole repo)               │
│    ├─► Resolves sink call sites in `graph.snapshot.bin`, external library    │
│    │   calls in `.rgctl/parked_relations.bin` (`RBPK` v1), and optional      │
│    │   transitive wrapper chains in `.rgctl/dep_stubs.bin`                   │
│    └─► Traces backward across `BlastRadiusEngine` + per-function PDGs and    │
│        declarative `.rgctl/vuln-rules.d/*.yaml` trigger predicates           │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. OUTPUT ARTIFACTS & VERDICTS                                               │
│                                                                              │
│  • CLI JSON: `rgctl taint --sink`, `rgctl vuln triage|analyze`, `rgctl deps` │
│  • OpenVEX status (`affected`, `not_affected` + justification,               │
│    `under_investigation`) with `.sink_taint` call-site evidence              │
│  • Dashboard: `.rgctl/dashboard/taint_index.json` + `taint/{uuid}.json`      │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Monotone Information-Flow & Taint Lattice** | Denning & Denning (1977), *Certification of Programs for Secure Information Flow*; Livshits & Lam (2005) | [`TaintAnalyzer`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/taint.rs#L1-L320) propagates taint labels from matched `TaintSource` statements along PDG `DataDependency` edges (`(name, scope_id)`), tracking field-access paths (`req.body.user`) and computing a path vulnerability verdict based on whether a `TaintSanitizer` dominates the sink along the def-use chain. |
| **Demand-Driven Backward Sink Reachability** | Arzt et al. (2014), *FlowDroid: Precise Context, Flow, Field, Object-Sensitive and Lifecycle-Aware Taint Analysis* | For `rgctl taint --sink <SYMBOL>` and `rgctl vuln analyze --osv <FILE>`, [`rgctl-security`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-security/src/lib.rs#L1-L180) starts at discovered call sites of the vulnerable sink (including third-party calls recorded in `parked_relations.bin`), walks backward up the call graph up to `--depth`, and checks intra-procedural PDG reachability from external entry/parameter sources. |
| **VEX (Vulnerability Exploitability eXchange) Justification Proofs** | CISA / OpenVEX Specification v0.2.0 | `rgctl vuln analyze` combines manifest dependency matching (`deps check`), package import census, call-site discovery (`parked_relations.bin`), and `.sink_taint` PDG verification to emit machine-readable OpenVEX verdicts (`component_not_present`, `vulnerable_code_not_in_execute_path`, or `affected`). |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Core Taint & Vulnerability Types

```rust
pub struct TaintFlow {
    pub source: TaintEndpoint,
    pub sink: TaintEndpoint,
    pub path: Vec<TaintStep>,
    pub cwe: Option<String>,
    pub severity: f32,        // 0.0 .. 10.0
    pub is_vulnerable: bool,  // false when neutralized by a sanitizer on path
    pub sanitizers_applied: Vec<String>,
}

pub struct SinkCallSite {
    pub caller_symbol: String,
    pub file_path: String,
    pub line: usize,
    pub sink_target: String,
}
```

### 3.2 Artifact Dependencies in `.rgctl/`

| Command / Mode | Required `.rgctl/` Artifacts | Why |
| :--- | :--- | :--- |
| `rgctl slice <file> --line N --variable V --taint` | `graph.snapshot.bin` (lazily creates/appends `.rgctl/analysis/cfg_pdg.archive.bin`) | Single-function criterion taint slice builds the enclosing function's CFG/PDG on demand via `ensure_cfg_pdg_for_source`. |
| `rgctl taint --sink <SYMBOL>` | `graph.snapshot.bin`, `parked_relations.bin`, `.rgctl/analysis/cfg_pdg.archive.bin` (`discover --with-cfg`) | Whole-repo sink scan inspects all callers of `<SYMBOL>` and traverses their precomputed PDGs in `cfg_pdg.archive.bin`. |
| `rgctl vuln triage --osv <PATH>` | None (pure OSV JSON parser) | Normalizes OSV package ranges and affected symbols without touching `.rgctl/`. |
| `rgctl vuln fetch` | Writes `.rgctl/osv/{id}.json` (never runs on `discover`) | Queries `POST {osv_api_url}/v1/querybatch` (batches $\le 1,000$) and hydrates matching OSV JSON documents locally. |
| `rgctl deps check --osv <PATH>` | Repository manifests + 16 Tier-1 lockfile formats (optional `--include-jars` / `--include-node-modules`) | Single-pass pruned dependency inventory + ecosystem-specific `VersionEngine` range membership. |
| `rgctl vuln analyze` / `rgctl vuln scan` | `graph.snapshot.bin` + `parked_relations.bin` + `blast_engine.snapshot.bin` (lazily hydrates CFG/PDG per call site; optional `.rgctl/dep_stubs.bin` via `--with-dep-stubs` and `.rgctl/vuln-rules.d/*.yaml`) | Short-circuits with `not_affected` if `deps check` misses; opens graph sidecars once via mmap (Rayon-parallel across advisories in batch mode) and emits OpenVEX v0.2.0 (canonical PURLs) or SARIF 2.1.0 (`-f sarif`). |

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl --help` and [`src/cli/mod.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/mod.rs#L701-L715) / [`src/cli/vuln_deps.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/vuln_deps.rs):

```bash
# 1. Discover with CFG/PDG and discover-time taint (plus optional custom YAML rules)
rgctl discover . --with-cfg --with-taint
rgctl discover . --with-cfg --with-taint --taint-rules ./custom-taint-rules.yaml

# 2. Sink-first taint reachability across the repo (requires `discover --with-cfg`)
rgctl taint --sink ObjectMapper.readValue --source external --depth 8 -f json

# 3. Single-function criterion taint slice (works WITHOUT prior `--with-cfg` via lazy append)
rgctl slice src/OrderController.java --line 42 --variable userInput --taint -f json

# 4. OSV / CVE fetch, local overrides, batch scan, OpenVEX & SARIF 2.1.0
rgctl -f json vuln fetch
rgctl -f json vuln scan
rgctl -f json vuln scan --offline --include-jars lib/ --with-dep-stubs
rgctl -f json vuln scan --from-report trivy-results.json --sbom bom.cdx.json
rgctl -f json vuln triage --osv advisories/GHSA-xxxx.json
rgctl -f json deps check --osv advisories/GHSA-xxxx.json --include-jars lib/
rgctl -f json vuln analyze --osv advisories/GHSA-xxxx.json --include-jars lib/ --strict-reachability
rgctl -f sarif vuln scan --offline -o results.sarif
```

*(Note: `rgctl security taint`, `rgctl security vuln`, and `rgctl security deps` are supported aliases for the top-level `taint`, `vuln`, and `deps` commands.)*

### 4.2 JSON Schema (`rgctl vuln analyze --osv <PATH> -f json`)

```json
{
  "osv_id": "GHSA-rgctl-test-0001",
  "deps_verdict": "affected_candidate",
  "import_hits": 3,
  "affected_method_callers": ["OrderService::parsePayload"],
  "sink_taint": {
    "sink": "ObjectMapper.readValue",
    "cfg_available": true,
    "reachable_flows": 1,
    "call_sites": [
      {
        "caller_symbol": "OrderService::parsePayload",
        "file_path": "src/OrderService.java",
        "line": 84
      }
    ]
  },
  "openvex": {
    "status": "affected",
    "justification": null
  }
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Intra/Inter-Procedural Taint Engine** | [`crates/rgctl-analysis/src/taint.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/taint.rs) | `TaintAnalyzer`, `TaintFlow`, `TaintSource`, `TaintSink`, `TaintSanitizer`, `TaintRulePack` |
| **OSV, Fetch, Stubs, Rules & SARIF** | [`crates/rgctl-security/src/lib.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-security/src/lib.rs) | `fetch_osv_for_repo`, `resolve_advisories_and_inventory`, `triage_osv_path`, `deps_check`, `vuln_analyze`, `load_or_build_dep_stubs`, `load_vuln_trigger_rules`, `analyze_results_to_sarif` |
| **CLI Taint, Vuln & Deps Commands** | [`src/cli/vuln_deps.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/vuln_deps.rs), [`src/cli/slice.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/slice.rs) | `run_vuln_fetch`, `run_vuln_scan`, `run_vuln_analyze`, `run_deps_check`, `run_vuln_triage`, `taint_slice_json` |
| **Dashboard Taint Export & UI** | [`crates/rgctl-dashboard/src/taint_export.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-dashboard/src/taint_export.rs), [`dashboard/src/TaintView.tsx`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/dashboard/src/TaintView.tsx) | `export_taint_index`, `TaintView` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Fast Deps Short-Circuit in `vuln analyze`:** `run_vuln_analyze` executes `deps_check` before opening `graph.snapshot.bin` or `cfg_pdg.archive.bin`. When the vulnerable package/version is absent (`DepsVerdict::NotAffected`), it immediately emits an OpenVEX `not_affected` verdict in milliseconds without loading graph artifacts.
2. **Parked Relations for Third-Party Sinks:** Third-party library sinks (such as `ObjectMapper.readValue` or `jndi.InitialContext.lookup`) are resolved by scanning both internal graph edges and `.rgctl/parked_relations.bin` (`RBPK` v1).

### Verification Commands

```bash
# Run taint & CWE security test suites
cargo test -p rgctl-analysis taint
cargo test -p rgctl-security
cargo test --test taint_security
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Lazy Per-Call-Site CPG vs Whole-Repo `rgctl taint --sink`:** `rgctl vuln analyze` and `rgctl vuln scan` hydrate single-function CFG/PDG records lazily on demand via `ensure_cfg_pdg_for_source` — upfront `discover --with-cfg` is **not** mandatory (though pre-building `--with-cfg` speeds repeated multi-site queries). Standalone `rgctl taint --sink` scans all precomputed PDGs in `.rgctl/analysis/cfg_pdg.archive.bin` and therefore still requires `discover --with-cfg`.
2. **Strict Local Override Zero-Network Guarantee:** Supplying `--osv <file|dir>`, `--from-report <path>`, or `--offline` strictly bypasses all HTTP calls to `api.osv.dev`.
3. **Pattern & Trigger-Rule Boundaries:** Built-in sanitizer packs and `.rgctl/vuln-rules.d/*.yaml` trigger predicates match static call/identifier patterns; dynamic reflection, runtime dependency injection containers outside source/stubs, and structural-only non-package languages (`bash`, `sql`, `hcl`, `markdown`) are reported with explicit epistemic honesty (`under_investigation` unless escalated by `--strict-reachability`).
