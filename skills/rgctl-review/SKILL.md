---
name: rgctl-review
description: >-
  Review a PR or branch with rgctl review (paths / check): call-path deltas,
  optional blast radius, architecture-doc soft pass, and REVIEW_REPORT.md plus a
  short four-section verdict. Session-only — no GitHub/GitLab posting. Use when
  asked to review a PR, branch diff, or call-path impact vs a base ref.
---

# rgctl-review (`/rgctl:review`)

Local structural PR/branch review via **`rgctl review paths`** and **`rgctl review check`**. No `rgctl serve`, no `--watch`, no forge API posts.

Default mode synthesizes a delta head from the base graph artifact + git name-status between `--base-ref` and `--head-ref`.

## When to Use

- User asks to review a PR, MR, branch, or code diff.
- User asks for blast radius / call-path impact of changes vs a base.
- User wants architecture or policy findings before merge (session report).

## Prerequisites

- Work in the repo root (`REPO_PATH`, default `.`). `cd` there before git/rgctl.
- `base_ref` and `head_ref` fetchable (working tree at head is fine). Default base: `origin/main`.
- A **base graph artifact** for `base_ref`. Without it, review fails with `base graph snapshot not configured`.

Base artifact resolution (root must contain `.rgctl/graph.snapshot.bin`):

1. `--base-artifact` / session `artifact_path`
2. `$RGCTL_BASE_ARTIFACT`
3. `{repo}/.rgctl-base/`

Expected layout: `{base_root}/.rgctl/graph.snapshot.bin`.

Do **not** start `rgctl serve` or `rgctl serve --watch` for this skill.

## Steps

### Step 0: Resolve / create base artifact

```bash
cd "${REPO_PATH:-.}"
BASE_REF="${base_ref:-origin/main}"
HEAD_REF="${head_ref:-HEAD}"
BASE_ROOT="${artifact_path:-${RGCTL_BASE_ARTIFACT:-.rgctl-base}}"
LIMITS_SPEC="${RGCTL_WITH_LIMITS:-max-mem-mb=4096,threads=1}"

if [ -f "${BASE_ROOT}/.rgctl/graph.snapshot.bin" ]; then
  echo "using base artifact at ${BASE_ROOT}"
else
  git fetch origin "$(echo "$BASE_REF" | sed 's#^origin/##')" 2>/dev/null || true
  WT="$(mktemp -d "${TMPDIR:-/tmp}/rgctl-base-wt.XXXXXX")"
  cleanup_wt() { git worktree remove --force "$WT" 2>/dev/null || true; rm -rf "$WT"; }
  trap cleanup_wt EXIT
  git worktree add --detach "$WT" "$BASE_REF"
  (
    cd "$WT"
    # Structural snapshot only — review paths/check do not need CFG/PDG.
    rgctl discover . --with-limits "${LIMITS_SPEC}"
  )
  mkdir -p .rgctl-base
  rm -rf .rgctl-base/.rgctl
  cp -a "$WT/.rgctl" .rgctl-base/
  cleanup_wt
  trap - EXIT
  BASE_ROOT=".rgctl-base"
fi

test -f "${BASE_ROOT}/.rgctl/graph.snapshot.bin" \
  || { echo "ERROR: base graph artifact missing for ${BASE_REF}"; exit 1; }

export RGCTL_BASE_ARTIFACT="$(cd "${BASE_ROOT}" && pwd)"
```

Notes: seed with `cp -a .rgctl .rgctl-base/` after discovering **at `base_ref`**. Do **not** copy the PR-branch `.rgctl/` into `.rgctl-base/`. Prefer a pre-seeded base on large repos — worktree discover can be expensive.

### Step 1: Call-path delta

```bash
mkdir -p "${REPO_PATH:-.}/.rgctl"
rgctl -f json review paths \
  --base-artifact "${RGCTL_BASE_ARTIFACT}" \
  --base-ref "${BASE_REF}" \
  --head-ref "${HEAD_REF}" \
  --upstream-depth "${upstream_depth:-2}" \
  --downstream-depth "${downstream_depth:-1}" \
  --fanout "${fanout:-5}" \
  --max-symbols "${max_symbols:-25}" \
  > "${REPO_PATH:-.}/.rgctl/review-paths.json"
```

Parse JSON (`schema_version: 1`). Extract `path_delta` (`added` / `removed` / `retargeted`), top symbols, truncation flags. Exit **2** → disambiguate with `--symbol`.

### Step 2: Blast radius (optional, top-N)

After Step 1, head snapshot is under `{repo}/.rgctl/`. For top impacted symbols:

```bash
rgctl -f json blast-radius --classify-boundary <symbol>
# If no policy will be used in Step 3: add --no-policy
```

### Step 3: Policy gate (if policy available)

Resolve first of: session `policy_json` → `.rgctl/policy.json` → copy root `policy.json` → `.rgctl/policy.json`.

```bash
POLICY_FILE="${REPO_PATH:-.}/.rgctl/policy.json"
if [ ! -f "${POLICY_FILE}" ] && [ -f "${REPO_PATH:-.}/policy.json" ]; then
  cp -f "${REPO_PATH:-.}/policy.json" "${POLICY_FILE}"
fi

if [ -f "${POLICY_FILE}" ]; then
  rgctl -f json review check \
    --policy-file "${POLICY_FILE}" \
    --bisect \
    --base-artifact "${RGCTL_BASE_ARTIFACT}" \
    --base-ref "${BASE_REF}" \
    --head-ref "${HEAD_REF}" \
    > "${REPO_PATH:-.}/.rgctl/review-check.json"
else
  echo "No policy configured" > "${REPO_PATH:-.}/.rgctl/review-check.json"
fi
```

### Step 3b: Architecture markdown (soft, advisory)

After path delta: read at most **5** of `ARCHITECTURE.md`, `docs/architecture.md`, `docs/architecture/**`, `docs/adr/**`, `ADR/**`, `**/ARCHITECTURE.md`, plus `README.md` only under dirs overlapping changed paths. Exclude `target/`, `node_modules/`, vendored trees. Cite constraints in §2 / Guidelines; **MUST NOT** alone set Verdict RED.

### Step 4: Write REVIEW_REPORT.md, then four-section summary

**Always** write `{REPO}/REVIEW_REPORT.md` (or session override path), then emit the short review in chat. No forge posts.

#### 4a. REVIEW_REPORT.md (fixed H2 order)

```markdown
# Review Report

## Overview
## Verdict
### Concerns
## Impact Diagram
## Change Set
## Changes
## Blast Radius
## Communities
## Policy Findings
## Test Handoff
## Sources
```

Under **Impact Diagram**: exactly one fenced `mermaid` block (`flowchart LR`). Nodes/edges from Steps 1–2 only; cap ~25 nodes; if no delta, minimal `base → head` with label `no path delta`.

#### 4b. Traffic light + four-section chat summary

Ladder (first match):

1. **RED** — policy **new** / **regression**; security/auth/PII/payment integrity; clear break without mitigation.
2. **YELLOW** — hot-path behavior change without tests; medium+ blast with weak coverage; evidence-linked arch-doc Concerns.
3. **GREEN** — nits / small blast / no policy new/regression.

Emit:

```markdown
## Verdict: 🟡 YELLOW — <one sentence why>

## 1. High-level Summary
## 2. Call-Path & Architecture Impact
## 3. Test Coverage & Regressions
## 4. Line-by-Line Feedback

## Guidelines
### Concerns
### Blockers
### Open questions
### What's fine
```

Section 4 bullets MUST start with `` `path:line` `` or `` `path:start-end` `` (new-file lines). For RED, Blockers non-empty.

## Gotchas

- Base artifact is mandatory — `--base-ref` alone is not enough.
- Layout: `cp -a .rgctl .rgctl-base/` (keep nested `.rgctl/`).
- No serve/watch/update for this workflow.
- `review paths` schema_version **1** (int); `review check` schema_version **"2"** (string).
- Always constrain discover: `--with-limits`.
- Prefer scratch JSON under `{repo}/.rgctl/`.
