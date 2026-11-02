---
name: rgctl-vuln
description: >-
  Triage an OSV/CVE with rgctl vuln (triage → deps check → analyze): package
  match, parked call-site reachability (.sink_taint), OpenVEX status, and a
  short CVE Reachability report. Use when asked about a CVE, OSV advisory,
  exploitability, or OpenVEX for this repo.
rgctl-managed: true
---

# rgctl-vuln (`/rgctl:vuln`)

End-to-end OSV/CVE reachability and OpenVEX triage via **`vuln triage`**, **`deps check`**, and **`vuln analyze`**. No forge posts; no `serve` / `--watch`.

Bound stdout on large repos: keep `--limit` / `--depth` small on follow-up probes; project JSON with `jq` (do not paste full analyze envelopes into chat).

## When to Use

- User asks whether the repo is affected by a CVE / OSV advisory.
- User wants OpenVEX status, exploitability, or concrete `file:line` call sites to a vulnerable sink.
- User has an OSV JSON path (file or directory) or a CVE id with a local advisory fixture.

## Prerequisites

- Work in the repo root (`REPO_PATH`, default `.`).
- OSV input: `--osv <path>` (file or directory of OSV JSON).
- CFG archive for reachability: `discover . --with-cfg` (add `--with-taint` when PDG-backed confidence is required).

Do **not** start `rgctl serve` or `rgctl serve --watch` for this skill.

## Steps

### Step 0: CFG readiness

```bash
cd "${REPO_PATH:-.}"
OSV_PATH="${osv_path:?set osv_path to OSV JSON file or directory}"
JARS_FLAG=""
# Add when lib/ or shaded JARs exist:
# JARS_FLAG="--include-jars lib"

rgctl -f json cpg status
# If CFG missing / cfg_available will be false on analyze:
# rgctl discover . --with-cfg
# Optional PDG confidence: rgctl discover . --with-cfg --with-taint
```

### Step 1: Normalize advisory (`vuln triage`)

```bash
rgctl -f json vuln triage --osv "${OSV_PATH}" \
  > "${REPO_PATH:-.}/.rgctl/vuln-triage.json"
```

Extract package coordinates, version ranges, and affected sink methods. Summarize — do not dump raw JSON.

### Step 2: Deps / shaded JAR match (`deps check`)

```bash
rgctl -f json deps check --osv "${OSV_PATH}" ${JARS_FLAG} \
  > "${REPO_PATH:-.}/.rgctl/vuln-deps.json"
```

Report `not_affected` vs `affected_candidate`. Zero Maven imports ≠ absent when `bundled_presence` / JAR match is true.

### Step 3: Reachability + OpenVEX (`vuln analyze`)

```bash
rgctl -f json vuln analyze --osv "${OSV_PATH}" ${JARS_FLAG} \
  > "${REPO_PATH:-.}/.rgctl/vuln-analyze.json"
```

Read (prefer `jq` projections):

| Field | Meaning |
|-------|---------|
| `.deps_verdict` | Deps-layer match |
| `.exploitability` | `not_affected` \| `not_exploitable` \| `exploitable` \| `under_investigation` |
| `.openvex.statements[0].status` | OpenVEX status |
| `.sink_taint.call_sites[]` | `{caller, target_method, file, line}` from graph + parked relations |
| `.sink_taint.paths[]` | `{from, to, hops}` upstream entry → sink |

`vuln analyze` already links parked external calls into `.sink_taint.call_sites` / `.paths` — do **not** re-run a full `find --package` / `callers --package` chain unless debugging empty sites.

### Step 4: Optional slice + report

On a hit call site (`file` + `line`), optional verification:

```bash
rgctl -f json slice "<file>" --line <line> --variable <var> --with-alias --limit 40
```

Then emit a **CVE Reachability & OpenVEX** report:

```markdown
## Verdict: <exploitability> — <one sentence>

## Advisory
## Deps match
## Call sites (file:line)
## Paths (entry → sink)
## OpenVEX
## Follow-ups
```

Cite exact `file:line` from `.sink_taint.call_sites[]`. Empty paths with `cfg_available=false` is **not** PDG proof of safe.

## Gotchas

- Always `-f json`; parse stdout; never `2>/dev/null`.
- Prefer `--with-cfg` before analyze for exploitability beyond deps-only.
- Unresolved sinks alone MUST NOT force `not_affected` without caller evidence.
- Scratch JSON under `{repo}/.rgctl/`; keep chat summaries short.
