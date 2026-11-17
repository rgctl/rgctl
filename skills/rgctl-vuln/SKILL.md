---
name: rgctl-vuln
description: >-
  Triage an OSV/CVE or scan an entire repo with rgctl vuln (fetch / scan /
  triage → deps check → analyze): OSV network feed + offline/report overrides,
  lockfile version matching, parked & transitive call-site reachability,
  OpenVEX PURLs, SARIF 2.1.0, and a concise CVE Reachability report. Use when
  asked about a CVE, OSV advisory, scanner report (Trivy/Grype/OSV-Scanner),
  exploitability, or OpenVEX/SARIF for this repo.
rgctl-managed: true
---

# rgctl-vuln (`/rgctl:vuln`)

End-to-end OSV/CVE reachability, whole-repo vulnerability scanning, and OpenVEX/SARIF generation via **`vuln fetch`**, **`vuln scan`**, **`vuln triage`**, **`deps check`**, and **`vuln analyze`**. No forge posts; no `serve` / `--watch`.

Bound stdout on large repos: keep `--depth` small on follow-up `blast-radius` probes and `--limit` on `find`/`callers`; project JSON with `jq` (do not paste full analyze/scan envelopes into chat).

## When to Use

- User asks whether the repo is affected by a CVE / OSV advisory or wants a full repo vulnerability scan (`rgctl vuln scan`).
- User wants to enrich a Trivy, Grype, or OSV-Scanner JSON report (`--from-report`) or CycloneDX/SPDX SBOM (`--sbom`) with reachability proofs.
- User wants OpenVEX v0.2.0 (`pkg:` PURLs), SARIF 2.1.0 (`-f sarif`), or concrete `file:line` call sites to a vulnerable sink.
- User wants a fail-closed CI gate (`--strict-reachability`) or air-gapped evaluation (`--offline`, `--osv <file|dir>`).

## Prerequisites

- Index the repo once with `rgctl discover . --full` (recommended staged pipeline: graph + CFG/PDG + taint + semantic; or `rgctl -r "$REPO" discover --full` without positional `.`) if `.rgctl/graph.snapshot.bin` is missing. Plain `rgctl discover .` also works in fast CI — `vuln analyze` and `vuln scan` lazily build single-function CFG/PDG per discovered call site on demand.
- Add `--with-dep-stubs` on `vuln scan` or `vuln analyze` when wrapper JARs or `node_modules`/`vendor` packages sit between first-party code and vulnerable sinks (builds `.rgctl/dep_stubs.bin` on demand).
- Lockfiles across all Tier-1 ecosystems (`Cargo.lock`, `go.sum`, `gradle.lockfile`, `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `uv.lock`, `poetry.lock`, `Pipfile.lock`, `requirements.txt`, `packages.lock.json`, `composer.lock`, `Gemfile.lock`, `pubspec.lock`, `mix.lock`, `Package.resolved`, `Puppetfile.lock`, `vcpkg.json`, `conan.lock`, `*.rockspec`) are scanned automatically.
- Local override flags (`--osv <file|dir>`, `--from-report <path>`, `--offline`) guarantee **zero outbound network requests**.

Do **not** start `rgctl serve` or `rgctl serve --watch` for this skill.

## Workflows

### Mode A: Whole-Repo Scan or Scanner Report Enrichment (`vuln fetch` / `vuln scan`)

```bash
cd "${REPO_PATH:-.}"
test -f .rgctl/graph.snapshot.bin || rgctl -f json discover .

# 1a. Live OSV network scan (queries api.osv.dev/v1/querybatch & caches .rgctl/osv/*.json):
rgctl -f json vuln scan > .rgctl/vuln-scan.json

# 1b. Two-step fetch then air-gapped offline scan (+ optional JAR bytecode call-stub stitching):
rgctl -f json vuln fetch
rgctl -f json vuln scan --offline --include-jars lib --with-dep-stubs > .rgctl/vuln-scan.json

# 1c. Enrich Trivy / Grype / OSV-Scanner JSON + optional CycloneDX/SPDX SBOM (zero network):
rgctl -f json vuln scan --from-report trivy.json --sbom bom.cdx.json > .rgctl/vuln-scan.json

# 1d. Emit SARIF 2.1.0 (with codeFlows/threadFlows) for GitHub Code Scanning:
rgctl -f sarif vuln scan --offline -o .rgctl/findings.sarif
```

Summarize counts (`jq '.counts' .rgctl/vuln-scan.json`) and inspect `exploitable` / `under_investigation` items (`jq '.results[] | select(.exploitability == "exploitable" or .exploitability == "under_investigation")'`).

---

### Mode B: Single-Advisory Deep Triage (`vuln triage` → `deps check` → `vuln analyze`)

#### Step 1: Normalize advisory (`vuln triage`)

```bash
OSV_PATH="${osv_path:?set osv_path to OSV JSON file}"
rgctl -f json vuln triage --osv "${OSV_PATH}" > .rgctl/vuln-triage.json
```

Inspect `packages[].ranges[]` (`fixed` exclusive vs `last_affected` inclusive bounds) and `packages[].symbol_coverage` (`{"coverage":"explicit","symbols":[...]}` vs `{"coverage":"unspecified"}`).

#### Step 2: Lockfile, manifest & bundled JAR match (`deps check`)

```bash
JARS_FLAG="" # e.g. "--include-jars lib" when bundled JARs/WARs exist
rgctl -f json deps check --osv "${OSV_PATH}" ${JARS_FLAG} > .rgctl/vuln-deps.json
```

Report `not_affected` vs `affected_candidate` and `matched[]` (`in_range`, `is_direct`, `provenance`).

#### Step 3: Reachability + OpenVEX / SARIF (`vuln analyze`)

```bash
# Default reachability (add --with-dep-stubs to stitch App -> WrapperJar -> VulnerableJar;
# add --strict-reachability for fail-closed CI gating):
rgctl -f json vuln analyze --osv "${OSV_PATH}" ${JARS_FLAG} > .rgctl/vuln-analyze.json
```

Key JSON fields (`jq` projections):

| Field | Meaning |
|-------|---------|
| `.deps_verdict` | `not_affected` \| `affected_candidate` |
| `.exploitability` | `not_affected` \| `not_exploitable` \| `exploitable` \| `under_investigation` |
| `.openvex.statements[0].status` | `not_affected` \| `affected` \| `under_investigation` |
| `.openvex.statements[0].justification` | `component_not_present` \| `vulnerable_code_not_in_execute_path` \| `inline_mitigations_already_exist` |
| `.openvex.statements[0].products[]` | Canonical PURL (`pkg:maven/group/art@ver`, `pkg:npm/...`, `pkg:cargo/...`, etc.) |
| `.sink_taint.call_sites // []` | `{caller, target_method, file, line}` from graph + `parked_relations.bin` + optional `dep_stubs.bin` |
| `.sink_taint.paths[]` | Proven untrusted→sink taint paths (including stitched wrapper chains) |
| `.sink_taint.caller_chains // []` | Structural upstream caller / `BlastRadiusEngine` chains |

**Epistemic Verdict Ladder:**
- `SymbolCoverage::Unspecified` + package imported → `under_investigation` (never false-negative `not_exploitable`; escalates to `exploitable` under `--strict-reachability`).
- `SymbolCoverage::Unspecified` + zero imports → `not_exploitable`.
- `SymbolCoverage::Explicit` + 0 callers → `not_exploitable (vulnerable_code_not_in_execute_path)`.
- `SymbolCoverage::Explicit` + callers + proven PDG taint → `exploitable`.
- `SymbolCoverage::Explicit` + callers + constant literals / dominating sanitizers / unsatisfied `.rgctl/vuln-rules.d/*.yaml` trigger rule → `not_exploitable (inline_mitigations_already_exist)`.
- `SymbolCoverage::Explicit` + callers + inconclusive dataflow → `under_investigation` (or `exploitable` with `--strict-reachability`).

#### Step 4: Optional slice + CVE Reachability Report

For a hit call site (`file` + `line`), optionally inspect statement dataflow:

```bash
rgctl -f json slice "<file>" --line <line> --variable <var> --with-alias
```

Emit a concise **CVE Reachability & OpenVEX** report:

```markdown
## Verdict: <exploitability> — <one sentence>

## Advisory
- OSV ID, affected package, `symbol_coverage` (explicit/unspecified), version intervals

## Deps match
- verdict, direct vs transitive (`is_direct`), lockfile/SBOM provenance

## Call sites (file:line)
- from `.sink_taint.call_sites // []`

## Paths (entry → sink)
- from `.sink_taint.paths[]` and `.sink_taint.caller_chains // []`

## OpenVEX / SARIF
- status, justification, canonical `pkg:` PURL products

## Follow-ups
```

## Gotchas

- Always pass `-f json` (or `-f sarif`) and parse stdout; never scrape stderr.
- Never use deprecated `rgctl gql`; use structured verbs (`vuln`, `deps`, `slice`, `blast-radius`, `find`, `callers`).
- Local overrides (`--osv`, `--from-report`, `--offline`) strictly prevent outbound HTTP calls.
- Use `--with-dep-stubs` when third-party wrapper JARs (including Gradle/shaded JARs without `META-INF/maven/pom.xml`) or `node_modules`/`vendor` packages sit between first-party code and the vulnerable sink.
- Conditional CVEs (e.g. feature-flag-gated vulnerabilities) can be codified in `{repo}/.rgctl/vuln-rules.d/*.yaml`.
