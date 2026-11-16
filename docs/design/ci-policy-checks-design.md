# CI Policy Checks & Temporal PR Review — Engineering Design

This document specifies the **CI Architectural Policy Gate, Temporal PR Review (`rgctl review`), and Violation Ledger subsystem** in `rgctl` (`src/cli/check.rs`, `src/cli/review.rs`, `src/cli/pr_check.rs`, `src/cli/policy_file.rs`, and `crates/rgctl-analysis/src/policy.rs`). It covers declarative JSON policy rules, working-tree vs. base/head temporal snapshot evaluation, delta head synthesis with reverse-call cascade, git bisect violation attribution, and calendar grace/sunset enforcement.

![Blast scores that feed policy thresholds (gbuilder)](../images/design/ci-policy-checks/policy-blast-scores.png)

*Figure 1: **Blast Radius** tab — impact scores and caller fan-in that calibrate CI policy thresholds.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. INPUT ARTIFACTS & GIT SCOPE                                               │
│                                                                              │
│  • Policy Config: `policy.json` (`max_blast_score`, `max_impact_zone_size`,  │
│    `centrality_alert_threshold`, forbidden layer rules, calendar SLAs)       │
│  • Base Snapshot: `.rgctl-base/` or `$RGCTL_BASE_ARTIFACT`                   │
│  • Git Delta: `git diff --name-status <base_ref> <head_ref>` + hunk lines    │
│  • Optional Ledger: `.rgctl/violation_ledger.jsonl`                          │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. DELTA HEAD SYNTHESIS & SNAPSHOT DIFF (`rgctl review` / `check --temporal`)│
│                                                                              │
│  • If `--full-snapshots` not set:                                            │
│      - Seeds head from `.rgctl-base/`                                        │
│      - Runs `IncrementalUpdater` on changed files + `--cascade-depth 1`      │
│        incoming caller files (avoids a second full cold `discover`!)         │
│  • Opens `SnapshotPair::open(base, head)` and runs `diff_snapshots`          │
│  • Intersects `NodeDeltaEvent` with git diff hunks (`EntityScope`)           │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. SCOPED POLICY EVALUATION & CALL-PATH SPINES                               │
│                                                                              │
│  A. `rgctl review check` / `rgctl check --temporal`:                         │
│     • Builds scoped reachability (`BlastRadiusEngine::build_scoped`)         │
│     • Classifies violations: `NEW`, `REGRESSION`, `EXISTING`, `RESOLVED`     │
│     • Evaluates calendar grace/sunset windows (`--strict-calendar`)          │
│     • Optional `--bisect`: binary-searches commit range for introducing SHA  │
│                                                                              │
│  B. `rgctl review paths`:                                                    │
│     • Extracts before/after representative call spines (`--upstream-depth 2`,│
│       `--downstream-depth 1`, `--fanout 10`) for modified symbols            │
│                                                                              │
│  C. `rgctl review export`:                                                   │
│     • Composes diff + call spines + policy into `.rgctl/workflows/review.json`│
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Differential Static Analysis (Ratcheting / Baseline Suppression)** | Tricorder / ErrorProne (Sadowski et al., ICSE 2015); Differential Dataflow | Legacy codebases rarely pass strict architectural rules on day one. [`pr_check.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/pr_check.rs#L1-L260) diffs violations between `base` and `head` by `StableNodeKey`, failing CI only on **`NEW`** or **`REGRESSION`** violations (`scope.new_violations_only = true`) while tracking **`EXISTING`** technical debt in `violation_ledger.jsonl` and celebrating **`RESOLVED`** fixes. |
| **Scoped Upstream Closure Hydration** | Incremental reachability over Condensed SCC DAGs | Rather than hydrating two full `MemoryBackend` graphs in CI, `BlastRadiusEngine::build_scoped` walks reverse `Calls` CSR edges in `ColumnarGraphMmap` from the modified symbol set and hydrates only their upstream caller closure. |
| **Automated Regression Bisection** | Zeller (1999), *Yesterday, My Program Worked. Today, It Does Not. Why?* (*ESEC/FSE*) | When `--bisect` is passed to `rgctl review check`, `rgctl` binary-searches the git commit range `base_ref..head_ref` ($O(\log C)$ steps) to pinpoint the exact commit SHA that introduced each new policy violation. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Temporal Violation Classification Matrix

| Violation Class | Definition | Fails CI (Default Ratchet) | Fails CI (`--strict`) |
| :--- | :--- | :--- | :--- |
| **`NEW`** | Symbol or violation absent on `base`, present on `head` | **Yes** (Exit `1`) | **Yes** (Exit `1`) |
| **`REGRESSION`** | Violation existed on `base`, but severity/metric worsened beyond tolerance on `head` | **Yes** (Exit `1`) | **Yes** (Exit `1`) |
| **`EXISTING`** | Violation unchanged between `base` and `head` (within calendar grace/SLA window) | No (Exit `0`) | **Yes** (Exit `1`) |
| **`RESOLVED`** | Violation present on `base`, eliminated on `head` | No (Exit `0`) | No (Exit `0`) |

### 3.2 Policy File Configuration (`policy.json`)

See [`docs/policy-format.md`](../policy-format.md) and [`docs/examples/policy-strict.json`](../examples/policy-strict.json):

```json
{
  "max_blast_score": 65.0,
  "max_impact_zone_size": 150,
  "centrality_alert_threshold": 0.015,
  "scope": {
    "new_violations_only": true
  },
  "calendar": {
    "grace_days": 14,
    "sunset_date": "2027-01-01"
  }
}
```

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl check --help`, `rgctl review --help`, and [`src/cli/mod.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/mod.rs#L844-L918):

```bash
# 1. Working-tree CI policy check on git-modified functions
rgctl check --policy-file policy.json --base-ref origin/main --head-ref HEAD -f json

# 2. Temporal PR policy gate (synthesizes delta head from .rgctl-base/ + cascade-depth 1)
rgctl review check --policy-file policy.json --base-ref origin/main --head-ref HEAD -f json
rgctl review check --policy-file policy.json --base-ref origin/main --bisect --strict-calendar -f json

# 3. PR before/after call-path spine report (upstream & downstream call deltas)
rgctl review paths --base-ref origin/main --head-ref HEAD --upstream-depth 2 --downstream-depth 1 -f json
rgctl review paths --synthetic-head worktree --symbol OrderService::process -f json

# 4. Export unified PR review artifact (.rgctl/workflows/review.json or --stdout)
rgctl review export --base-ref origin/main --head-ref HEAD --policy-file policy.json --stdout -f json
```

*(Note: `rgctl pr-check` and `rgctl check --temporal` are supported aliases for `rgctl review check`.)*

### 4.2 JSON Schema (`rgctl review check --policy-file policy.json -f json`)

```json
{
  "schema_version": 2,
  "passed": false,
  "base_ref": "origin/main",
  "head_ref": "HEAD",
  "graph_diff": {
    "nodes_added": 3,
    "nodes_removed": 0,
    "nodes_changed": 5,
    "edges_added": 12,
    "edges_removed": 2
  },
  "violations_summary": {
    "new": 1,
    "regression": 0,
    "existing": 2,
    "resolved": 1
  },
  "violations": [
    {
      "classification": "NEW",
      "symbol": "OrderService::processOrder",
      "file": "src/OrderService.java",
      "rule": "max_blast_score",
      "actual": 74.2,
      "threshold": 65.0
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Working-Tree Policy Check** | [`src/cli/check.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/check.rs), [`src/cli/check_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/check_output.rs) | `CheckArgs`, `run`, `changed_function_symbols` |
| **Temporal PR Check & Bisect** | [`src/cli/pr_check.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/pr_check.rs), [`src/cli/pr_check_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/pr_check_output.rs), [`src/cli/temporal_prep.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/temporal_prep.rs) | `PrCheckArgs`, `seed_head_artifact_from_base`, `evaluate_temporal` |
| **PR Call-Path Spines & Export** | [`src/cli/review.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/review.rs), [`src/cli/review_export.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/review_export.rs) | `ReviewCommands`, `run_paths`, `run_export` |
| **Policy Schema & Evaluator** | [`src/cli/policy_file.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/policy_file.rs), [`crates/rgctl-analysis/src/policy.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/policy.rs) | `PolicyFile`, `PolicyRegistry`, `CalendarPolicy` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Delta Head Synthesis Over Full Rediscover:** By default, `rgctl review check` and `rgctl review paths` copy `.rgctl-base/` and run `IncrementalUpdater` on the git diff + `--cascade-depth 1` callers in milliseconds rather than running a second full repository `discover`.
2. **Null vs. Clean Policy Distinction in `review export`:** When `--policy-file` is omitted from `rgctl review export`, the exported `policy` field is serialized as `null` (not evaluated), never a misleading `"passed": true`.

### Verification Commands

```bash
# Run policy, review paths, and pr-check integration tests
cargo test --test subprocess_golden_path
cargo test -p rgctl-analysis policy
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Base Snapshot Availability:** If neither `--base-artifact`, `$RGCTL_BASE_ARTIFACT`, nor `.rgctl-base/` is present, `review` commands synthesize the base snapshot from `git worktree` at `--base-ref`, which requires `--base-ref` to be fetched in shallow CI checkouts (`git fetch origin main --depth=1`).
2. **Uncommitted Worktree Mode (`--synthetic-head worktree`):** Evaluating uncommitted working-tree edits against `HEAD` uses `--synthetic-head worktree` to overlay dirty files onto the `HEAD` snapshot.
