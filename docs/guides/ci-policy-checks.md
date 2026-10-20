# CI Policy Checks

rgctl can enforce architecture rules in CI and before merge: impact-zone limits, centrality alerts, and domain isolation. Policy files are JSON; commands exit **0** on pass and **1** on failure.

This guide focuses on **which command to use**, how to wire **pull-request gates**, and copy-paste **CI recipes**. For the full policy schema see [policy-format.md](../policy-format.md). For engineering internals see [ci-policy-checks-design.md](../design/ci-policy-checks-design.md).

---

## Which command?

| Goal | Command | Graphs needed | Blocks on |
|------|---------|---------------|-----------|
| Pre-commit / dirty working tree | `check` | One (`discover` → `.rgctl/`) | Any violation in git scope |
| PR gate: only **new** breakage vs `main` | **`pr-check`** (default) | Base cache + delta head | `new` (+ `regression` by default) |
| Same as `pr-check` from `check` | `check --temporal` | Base cache + delta head | Same as `pr-check` |
| Preview uncommitted edits temporally | `pr-check --synthetic-head worktree` | Base cache + HEAD snapshot + worktree | Same as `pr-check` |
| One-off symbol review | `blast-radius SYMBOL --policy-file` | One | That symbol only |

**Rule of thumb**

- **`check`** — “Did my local edits touch functions that violate policy?” (single snapshot, git-scoped symbols).
- **`pr-check`** — “Did this PR introduce **new** policy violations compared to `main`?” (base vs head, temporal classes, graph diff).

Most teams want **`pr-check`** on pull requests with `scope.new_violations_only: true` so legacy debt on `main` does not block every PR.

---

## Quick start: PR gate on `main`

**1. Index the repo once (per machine / cache refresh):**

```bash
rgctl -r "$REPO" discover .
```

**2. Save a base artifact from `main`:**

```bash
git checkout main
rgctl -r "$REPO" discover .
mkdir -p "$REPO/.rgctl-base" && cp -a "$REPO/.rgctl" "$REPO/.rgctl-base/"
git checkout -   # back to your branch
```

**3. Run the temporal gate:**

```bash
rgctl -r "$REPO" -f json pr-check \
  --policy-file rgctl-tests/rgctl-pr-policy.json \
  --base-ref origin/main \
  --head-ref HEAD \
  --strict
```

By default **`pr-check` does not require a second full `discover` on the PR branch**. It copies the base snapshot into `.rgctl/`, applies the git name-status delta (with optional caller cascade), and evaluates policy on scoped entities only.

Exit **1** when `passed` is false. With the sample PR policy, only **`new`** and **`regression`** violations fail the gate.

---

## One-time setup

### Graph artifacts

After `discover`, artifacts live under `{repo}/.rgctl/`:

```text
.rgctl/
  graph.snapshot.bin       # required
  analysis_results.bin     # optional; speeds centrality reuse
  violation_ledger.jsonl   # appended by pr-check (violation timeline)
```

For PR gates, cache **`main`** (or merge-base) separately:

```text
.rgctl-base/.rgctl/graph.snapshot.bin     # local default for --base-artifact
# or
$RGCTL_BASE_ARTIFACT/.rgctl/graph.snapshot.bin
# or
.rgctl-cache/<sha>/.rgctl/                # CI cache per commit (see below)
```

Resolution order for the base snapshot: `--base-artifact` → `$RGCTL_BASE_ARTIFACT` → `{repo}/.rgctl-base/`.

### Deterministic node IDs (migration)

Node UUIDs are now stable across re-indexing when `file_path` + name are known. **Upgrade once** after pulling a release that includes this change:

```bash
rm -rf .rgctl .rgctl-base
rgctl discover .
```

Then rebuild `.rgctl-base/` from `main` as above. See [release notes](../releases/unreleased.md#graph--ci-policy).

---

## Policy files

Example PR policy shipped with rgctl: [rgctl-tests/rgctl-pr-policy.json](../../rgctl-tests/rgctl-pr-policy.json).

```json
{
  "max_impact_nodes": 50,
  "centrality_alert_threshold": 0.15,
  "scope": {
    "new_violations_only": true,
    "fail_on_regression": true
  },
  "size_limits": {
    "max_changed_files": 500,
    "max_scoped_entities": 5000
  }
}
```

| Field | Role in PR CI |
|-------|----------------|
| `max_impact_nodes` | Blast impact zone cap per scoped function |
| `centrality_alert_threshold` | Cascade hazard when high-betweenness nodes are reached |
| `scope.new_violations_only` | **`true`** → only `new` / `regression` fail the gate |
| `scope.fail_on_regression` | Fail when a resolved violation reappears (ledger-backed) |
| `size_limits.*` | Abort if the PR scope is too large (runaway PR protection) |
| `temporal.*` | Grace periods, SLA aging, sunset dates (optional) |

Stricter smoke-test policies: [examples/policy-strict.json](../examples/policy-strict.json), [examples/policy-permissive.json](../examples/policy-permissive.json).

Full schema: [policy-format.md](../policy-format.md).

---

## `rgctl check`

Evaluates policy on functions touched in a **git diff**, using **one** graph snapshot (`.rgctl/`).

```bash
# Working tree vs last commit (default scope)
rgctl -r "$REPO" -f json check --policy-file policy.json

# Commits on current branch
rgctl -r "$REPO" -f json check \
  --policy-file policy.json \
  --base-ref origin/main \
  --head-ref HEAD \
  --strict
```

| Flag | Effect |
|------|--------|
| *(none)* | Scope = `git diff --name-only HEAD` (uncommitted + staged vs `HEAD`) |
| `--base-ref` + `--head-ref` | Scope = paths changed between those commits |
| `--strict` | Fail if git scope is empty (no “check everything” fallback) |
| `--temporal` | Run the same pipeline as `pr-check` (base cache + delta head) |

Policy field `scope.strict_diff: true` also enables strict mode for `check`.

**When to use:** local pre-commit hooks, nightly jobs on a single snapshot, or quick gates without maintaining a `main` cache. For merge gates that compare against `main`, prefer **`pr-check`** (or `check --temporal`).

### Example: CoolStore

```bash
rgctl -r example/coolstore discover . -l java
rgctl -r example/coolstore -f json check \
  --policy-file example/coolstore/policy.json
```

On CoolStore Java-only, `example/coolstore/policy.json` (`max_impact_nodes: 15`) fails `check` on symbols such as `toProduct` when the impact zone is 19. That illustrates why PR workflows use **`new_violations_only`** instead of failing on all existing debt.

---

## `rgctl pr-check`

Temporal PR gate: compare **base** (`main`) vs **head** (PR), classify each violation, and optionally report graph diff stats.

### How it works (default: delta head)

```text
  .rgctl-base/          git diff              PR working tree
  (main snapshot)   base_ref..head_ref      (on-disk files)
        │                    │                      │
        └──── compact ───────┴── into ──────────────┘
                              │
                   .rgctl/synth-head/  (synthesized head)
                              │
                    scoped policy eval → JSON + exit code
```

1. Open base snapshot from cache (`.rgctl-base/`, `$RGCTL_BASE_ARTIFACT`, or `--base-artifact`).
2. Compact base + changed paths from `git diff --name-status` into `{repo}/.rgctl/synth-head/` (does **not** overwrite live `.rgctl/graph.snapshot.bin`).
3. Optionally expand scope to **caller files** (`--cascade-depth`, default `1`).
4. Evaluate blast-radius policy only on entities in the git scope.
5. Classify each violation temporally; apply calendar rules; append to `violation_ledger.jsonl`.

**Worktree mode:** `--synthetic-head worktree` uses live `.rgctl/graph.snapshot.bin` as base when `.rgctl-base/` is absent, and still writes the head under `.rgctl/synth-head/`.

**Caveat:** delta extract reads **working-tree files on disk**, not `git show <head_ref>:path`. Check out the target ref (or use `--synthetic-head worktree`). A mismatch emits a stderr warning. `--bisect` inherits the same limitation until blob extract lands.

**Legacy mode:** pass `--full-snapshots` and provide both `--base-artifact` and `--head-artifact` (or pre-built `.rgctl/` on the PR branch). Use when you already run two full `discover` jobs in CI.

### Temporal classes

| Class | Meaning | Fails default PR gate? |
|-------|---------|------------------------|
| `new` | Violation on head, not on base | **Yes** |
| `existing` | Violation on both snapshots | No (`new_violations_only`) |
| `resolved` | Fixed on head (debt paid down) | No — reported as progress |
| `regression` | Reappeared after ledger marked it resolved | Yes (`fail_on_regression`) |

### CLI flags

| Flag | Default | Purpose |
|------|---------|---------|
| `--policy-file` | *(required)* | JSON policy |
| `--base-ref` | `origin/main` | Left side of git diff |
| `--head-ref` | `HEAD` | Right side of git diff |
| `--base-artifact` | `.rgctl-base/` or `$RGCTL_BASE_ARTIFACT` | Base graph cache root |
| `--head-artifact` | *(omit for delta mode)* | Pre-built head; skips synthesis |
| `--full-snapshots` | off | Require pre-built head; no delta synthesis |
| `--strict` | off | Fail when git reports zero changed files |
| `--cascade-depth` | `1` | Re-index caller files when callees change (`0` = off) |
| `--bisect` | off | Add `introduced_in_commit` per new/regression violation |
| `--synthetic-head worktree` | off | Scope + head from uncommitted changes vs `HEAD` |
| `--strict-calendar` | off | Treat calendar `warn` as failure (grace / sunset windows) |

### What passes?

With the sample PR policy (`new_violations_only: true`, `fail_on_regression: true`):

- **Pass:** no violations, or only `existing` / `resolved`, or calendar warnings during grace (unless `--strict-calendar`).
- **Fail:** any `new` or `regression`, or calendar-forced failures (post-grace existing, SLA breach, sunset).

### JSON output (schema v2)

```bash
rgctl -r . -f json pr-check --policy-file rgctl-tests/rgctl-pr-policy.json \
  --base-ref origin/main --head-ref HEAD --strict \
  | jq '{passed, violations_summary, scope, graph_diff}'
```

```json
{
  "schema_version": "2",
  "passed": true,
  "violations": [],
  "violations_summary": {
    "new": 0,
    "existing": 0,
    "resolved": 0,
    "regression": 0
  },
  "graph_diff": {
    "nodes_added": 0,
    "nodes_removed": 0,
    "nodes_changed": 0,
    "edges_added": 0,
    "edges_removed": 0
  },
  "scope": { "files": 3, "entities": 2 }
}
```

Each violation includes `symbol`, `classification`, `stable_key`, `violation` (tagged union), optional `introduced_in_commit` (with `--bisect`), and optional `severity` (`warn` | `fail`) from calendar rules.

Shape reference: [json-api.md § pr-check](../json-api.md#8b-pr-check).

### Violation ledger

Each `pr-check` run appends to `.rgctl/violation_ledger.jsonl` keyed by `(stable_key, rule_id)`. The ledger powers:

- **`regression`** — violation was previously `resolved` in the ledger
- **SLA enforcement** — `temporal.enforce_sla` + `violation_sla_days` vs ledger `first_seen`

### Calendar policies (optional)

Add a `temporal` block to defer hard failures during rollout:

```json
{
  "max_impact_nodes": 50,
  "scope": { "new_violations_only": true },
  "temporal": {
    "effective_from": "2026-09-01",
    "grace_period_days": 30,
    "severity_during_grace": "warn",
    "fail_existing_after_grace": true,
    "violation_sla_days": 30,
    "enforce_sla": true
  }
}
```

During grace, violations emit `severity: warn` and exit **0** unless you pass **`--strict-calendar`**. After grace, `existing` violations can fail when `fail_existing_after_grace` is set.

---

## CI on GitHub Actions

Canonical example workflow in this repo: [.github/workflows/rgctl-pr-check.yml](../../.github/workflows/rgctl-pr-check.yml).

It:

1. Caches `.rgctl-cache/<base-sha>/` per merge-base commit.
2. Runs `discover` on cache miss only.
3. Sets `RGCTL_BASE_ARTIFACT` and runs **delta** `pr-check` (no PR-branch `discover`).

**Trigger:** `workflow_dispatch` or PR label `rgctl-pr-check`.

### Minimal workflow (copy-paste)

```yaml
name: Architecture PR gate

on:
  pull_request:
    branches: [main]

jobs:
  pr-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install rgctl
        run: cargo build --release --bin rgctl   # or download a release binary

      - name: Cache base graph
        id: base-cache
        uses: actions/cache@v4
        with:
          path: .rgctl-cache/${{ github.event.pull_request.base.sha }}
          key: rgctl-base-${{ github.event.pull_request.base.sha }}

      - name: Build base graph (cache miss)
        if: steps.base-cache.outputs.cache-hit != 'true'
        run: |
          mkdir -p ".rgctl-cache/${{ github.event.pull_request.base.sha }}"
          git checkout "${{ github.event.pull_request.base.sha }}"
          ./target/release/rgctl -r . discover .
          cp -a .rgctl ".rgctl-cache/${{ github.event.pull_request.base.sha }}/"
          git checkout -

      - name: Temporal policy gate
        env:
          RGCTL_BASE_ARTIFACT: .rgctl-cache/${{ github.event.pull_request.base.sha }}
        run: |
          ./target/release/rgctl -r . -f json pr-check \
            --policy-file rgctl-tests/rgctl-pr-policy.json \
            --base-ref origin/${{ github.base_ref }} \
            --head-ref HEAD \
            --strict
```

### Two-job pattern (main always fresh)

| Job | Branch | Action |
|-----|--------|--------|
| `index-main` | `main` | `discover` → upload `.rgctl/` artifact |
| `pr-check` | PR | download artifact → `RGCTL_BASE_ARTIFACT` → `pr-check` |

Use when you do not want per-SHA cache logic on the runner.

### CI cache layout

```text
.rgctl-cache/
  <merge-base-sha>/
    .rgctl/
      graph.snapshot.bin
      file_hashes.json
```

Point `RGCTL_BASE_ARTIFACT` at the directory that **contains** `.rgctl/` (not the snapshot file itself).

---

## Recipes

### Local pre-commit (`check`)

```bash
rgctl -r . discover .    # if sources changed materially
rgctl -r . check --policy-file policy.json
```

### Uncommitted preview (temporal)

**Option A — worktree synthetic head (if `.rgctl/` reflects `HEAD`):**

```bash
rgctl -r . -f json pr-check \
  --policy-file rgctl-tests/rgctl-pr-policy.json \
  --synthetic-head worktree
```

**Option B — `check --temporal` (same evaluator, commit refs):**

```bash
rgctl -r . -f json check --temporal \
  --policy-file rgctl-tests/rgctl-pr-policy.json \
  --base-ref origin/main --head-ref HEAD
```

### PR against `main` (recommended CI)

```bash
export RGCTL_BASE_ARTIFACT="$PWD/.rgctl-base"   # or CI cache path
rgctl -r . -f json pr-check \
  --policy-file rgctl-tests/rgctl-pr-policy.json \
  --base-ref origin/main \
  --head-ref HEAD \
  --strict
```

No `discover` on the PR branch required in delta mode.

### Compare two release tags (dual snapshots)

When you need exact graphs from two full indexes (not delta synthesis):

```bash
git checkout v1.0 && rgctl -r . discover .
cp -a .rgctl /tmp/snapshots/v1.0-rgctl

git checkout v2.0 && rgctl -r . discover .

rgctl -r . -f json pr-check \
  --policy-file rgctl-tests/rgctl-pr-policy.json \
  --base-artifact /tmp/snapshots/v1.0-rgctl \
  --head-artifact . \
  --base-ref v1.0 --head-ref v2.0 \
  --full-snapshots --strict
```

### Find the introducing commit (`--bisect`)

```bash
rgctl -r . -f json pr-check \
  --policy-file rgctl-tests/rgctl-pr-policy.json \
  --base-ref origin/main --head-ref HEAD \
  --bisect \
  | jq '.violations[] | {symbol, classification, introduced_in_commit}'
```

### Save a report artifact

```bash
rgctl -r . -f json pr-check \
  --policy-file rgctl-tests/rgctl-pr-policy.json \
  --base-ref origin/main --head-ref HEAD --strict \
  > "reports/pr-check-$(git rev-parse --short HEAD).json"
```

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| `Graph not found` | No `.rgctl/` | Run `discover` (or ensure base cache exists for `pr-check`) |
| `strict diff scope: no changed files` | Empty git diff with `--strict` | Remove `--strict` or change refs |
| All violations `existing`, gate passes, but you expected failure | `new_violations_only: true` | Intentional — only **new** debt fails |
| Gate fails on legacy code | `new_violations_only: false` | Set `scope.new_violations_only: true` for PR CI |
| `worktree head synthesis requires...` | No `.rgctl/graph.snapshot.bin` | `discover` on `HEAD` before `--synthetic-head worktree` |
| Cross-file edges wrong after partial re-index | Stale pre-deterministic IDs | `rm -rf .rgctl .rgctl-base && discover` |
| `changed file count exceeds...` | Large PR | Raise `size_limits.max_changed_files` or split PR |

---

## Related

| Doc | Content |
|-----|---------|
| [policy-format.md](../policy-format.md) | Full JSON schema |
| [json-api.md](../json-api.md) | `check` / `pr-check` response shapes |
| [ci-policy-checks-design.md](../design/ci-policy-checks-design.md) | Architecture diagram, Rust module map |
| [graph-diff-design.md](../design/graph-diff-design.md) | Snapshot diff + cascade internals |
| [discovering-and-indexing.md](discovering-and-indexing.md) | `discover` prerequisites |
| [blast-radius-analysis.md](blast-radius-analysis.md) | Per-symbol policy via `blast-radius --policy-file` |
