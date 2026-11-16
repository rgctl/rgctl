# Taint Analysis & Vulnerability Reachability — Engineering Design

This document specifies the **Taint Analysis, Sink-First Reachability, and OSV/OpenVEX Vulnerability Triage subsystem** in `rgctl` (`crates/rgctl-analysis/src/taint.rs`, `crates/rgctl-security/`, `src/cli/slice.rs`, and `src/cli/vuln_deps.rs`). It covers intra- and inter-procedural source-to-sink taint propagation, sanitizer neutralization, custom YAML rule packs, `.sink_taint` parked call-site reachability, and end-to-end CVE/OSV analysis (`rgctl taint`, `rgctl vuln`, `rgctl deps`).

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
│  PATH B: Sink-First & OSV Reachability (`rgctl taint --sink` / `rgctl vuln`) │
│    ├─► Requires `rgctl discover --with-cfg` (or `--with-taint`)              │
│    ├─► Resolves sink call sites in `graph.snapshot.bin` AND external library │
│    │   calls in `.rgctl/parked_relations.bin` (`RBPK` v1)                    │
│    └─► Traces backward across call graph (`--depth 8`) + per-function PDGs   │
│        to verify whether external/untrusted inputs reach the sink            │
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
| `rgctl deps check --osv <PATH>` | Repository manifests (`Cargo.toml`, `pom.xml`, `package.json`, `go.mod`, etc.; optional `--include-jars` / `--include-node-modules`) | Matches OSV package coordinates and semantic version ranges against lockfiles/manifests. |
| `rgctl vuln analyze --osv <PATH>` | `graph.snapshot.bin` + `parked_relations.bin` (+ `cfg_pdg.archive.bin` for `.sink_taint` flow verification) | Short-circuits with `not_affected` if `deps check` fails; otherwise joins package imports, parked call sites, and sink taint into an OpenVEX report. |

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

# 4. OSV / CVE vulnerability triage, dependency check, and OpenVEX reachability pipeline
rgctl vuln triage --osv advisories/GHSA-xxxx.json -f json
rgctl deps check --osv advisories/GHSA-xxxx.json --include-jars lib/ -f json
rgctl vuln analyze --osv advisories/GHSA-xxxx.json --include-jars lib/ -f json
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
| **OSV, Deps & Sink-First Reachability** | [`crates/rgctl-security/src/lib.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-security/src/lib.rs) | `triage_osv_path`, `deps_check`, `vuln_analyze`, `build_sink_first_result_with_sites` |
| **CLI Taint, Vuln & Deps Commands** | [`src/cli/vuln_deps.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/vuln_deps.rs), [`src/cli/slice.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/slice.rs) | `run_vuln_triage`, `run_deps_check`, `run_vuln_analyze`, `taint_slice_json` |
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

1. **Whole-Repo Sink Taint Requires `--with-cfg`:** If `rgctl taint --sink` or `rgctl vuln analyze` is run without `.rgctl/analysis/cfg_pdg.archive.bin`, call-site reachability (`affected_method_callers` and `call_sites`) is still reported from `graph.snapshot.bin` + `parked_relations.bin`, while `sink_taint.cfg_available` is set to `false` with a diagnostic prompting `rgctl discover . --with-cfg`.
2. **Pattern-Based Sanitizer Matching:** Built-in and YAML sanitizer rules match by function/method pattern (e.g., `PreparedStatement`, `html_escape`, `filepath.Clean`). Custom domain validators that sanitize without matching known sanitizer patterns should be declared in `.rgctl/taint-rules.d/*.yaml` or passed via `--taint-rules`.
