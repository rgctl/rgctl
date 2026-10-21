# Community Detection & CI Policy Checks

Detailed guide for understanding implicit architecture (communities) and enforcing architectural rules (policy checks).

## Table of Contents

- [Community Detection](#community-detection)
- [CI Policy Checks](#ci-policy-checks)
- [Common Workflows](#common-workflows)

---

## Community Detection

### Overview

The `communities` command reveals **implicit architecture** — functional clusters that rgctl automatically detects during `discover`. The Louvain algorithm partitions the call graph into groups of tightly connected functions, even if they span multiple files or packages.

**Key use cases:**
- Understand architecture beyond package/directory structure
- Identify cohesive modules and coupling problems
- Plan microservice extraction (communities → service boundaries)
- Navigate unfamiliar code (high-level functional map)
- Find ownership of features ("which subsystem owns checkout?")

### Commands

#### List Communities

**CLI:**
```bash
rgctl -f json communities list
```


**Sample output:**
```json
{
  "schema_version": 1,
  "modularity": 0.45,
  "communities": [
    {
      "id": 4391,
      "label": "bower_components.lodash (26)",
      "member_count": 120
    },
    {
      "id": 14763,
      "label": "Infrastructure / Common Library",
      "member_count": 60
    },
    {
      "id": 12715,
      "label": "coolstore.model::length",
      "member_count": 34
    }
  ]
}
```

**What to report:**
- Top communities by size
- Notable labels (e.g., "Infrastructure", domain-specific)
- Modularity score (>0.4 indicates good clustering)

#### Query Community Members

Once you have a community ID from `communities list`, explore ownership and members via structured verbs:

**CLI:**
```bash
rgctl -f json inventory --by community
rgctl -f json semantic query "<label or concept>" --scope community --limit 10
# pick a seed symbol from hits, then:
rgctl -f json blast-radius '<Symbol>' --depth 2   # may include community_id
rgctl -f json callers '<Symbol>' --depth 2
```


#### Refresh Community Labels

If labels are stale (e.g., after major renames):

**CLI only:**
```bash
rgctl communities label --write
```

This recomputes heuristic labels and persists them into `analysis_results.bin`. Check `written: true` in response.


### Community-Scoped Semantic Search

Find which subsystem owns a feature:

**CLI:**
```bash
rgctl -f json semantic query "checkout flow" --scope community --limit 10
```


Returns hits pooled by community rather than individual functions.

**Note:** On large repos (100K+ nodes), label-propagation may produce very granular clusters. For subsystem ownership, prefer `communities list` + grep labels over `--scope community`.

### Interpreting Communities

| Pattern | Interpretation |
|---------|----------------|
| Large community with clear label | Well-defined functional domain |
| Unexpectedly large community | Tight coupling between what should be separate |
| Many small singleton communities | Low cohesion, scattered dependencies |
| Cross-package communities | Logical grouping transcends directory structure |

### Common Questions

**"What communities exist?"**
```bash
rgctl -f json communities list
```

**"Which subsystem owns checkout?"**
```bash
rgctl -f json semantic query "checkout" --scope community
```

**"List functions related to community 12"**
```bash
rgctl -f json communities list | jq '.communities[] | select(.id == 12)'
rgctl -f json semantic query "<that community label>" --scope community
rgctl -f json inventory --by community
```

**"Why did a name glob return 0 for 'gateway'?"**

Concepts often live in package/directory paths or community labels, not function names:
1. Try `communities list` and grep for "gateway" in labels
2. Try `semantic query "gateway"`
3. Broaden: `find '*Gateway*' --type class` / `find '*gateway*' --type module`

---

## CI Policy Checks

### Overview

The `check` command is a **CI policy gateway** that evaluates your codebase against architectural rules defined in a JSON policy file. It scans every function, runs blast-radius analysis, and reports violations. Exit code 1 on failure makes it suitable for CI gates.

**Key use cases:**
- CI pipeline integration (block merges on violations)
- Blast-radius guardrails (prevent high-impact changes)
- Centrality alerts (flag risky high-centrality functions)
- Domain isolation (enforce module boundaries)
- Continuous architecture monitoring

### Policy File Schema

Example `policy.json`:

```json
{
  "max_impact_nodes": 15,
  "centrality_alert_threshold": 0.8,
  "forbidden_crossings": [
    {
      "from_pattern": ".*controller.*",
      "to_pattern": ".*database.*",
      "reason": "Controllers must not directly access database"
    }
  ]
}
```

**Rules:**

| Field | Type | Meaning |
|-------|------|---------|
| `max_impact_nodes` | number | Max allowed blast-radius impact zone size |
| `centrality_alert_threshold` | number (0-1) | Centrality score above which to alert |
| `forbidden_crossings` | array | Disallowed call patterns between modules |
| `forbidden_crossings[].from_pattern` | regex | Caller pattern |
| `forbidden_crossings[].to_pattern` | regex | Callee pattern |
| `forbidden_crossings[].reason` | string | Human explanation |

**See:** Policy Format Guide (product docs; not bundled in this skill pack) for full schema

### Running Policy Checks

**CLI:**
```bash
rgctl -f json check --policy-file policy.json
```


**Sample output (violations):**

```json
{
  "schema_version": 1,
  "passed": false,
  "policy": "policy.json",
  "violations": [
    {
      "symbol": "indexOf",
      "error": "Graph error: scale failure: impact zone size 648 exceeds max 15"
    },
    {
      "symbol": "baseIsEqual",
      "error": "Graph error: scale failure: impact zone size 114 exceeds max 15"
    }
  ]
}
```

**What to report:**
- `passed` status (true/false)
- Count of violations
- Top violators by severity
- Specific symbols and their violations

### Exit Codes

- `0` — All rules pass
- `1` — Policy violation(s) found

JSON output still emitted on exit 1 (parse stdout for violations).

### CI Integration

**GitHub Actions example:**

```yaml
- name: Policy Check
  run: |
    rgctl -r . discover
    rgctl -r . check --policy-file .rgctl/policy.json
```

**GitLab CI example:**

```yaml
policy-check:
  script:
    - rgctl -r . discover
    - rgctl -r . check --policy-file .rgctl/policy.json
  only:
    - merge_requests
```

### Per-Symbol Policy (Blast-Radius)

You can also check policy on individual symbols without full codebase scan:

**CLI:**
```bash
rgctl -f json blast-radius updateQuantity --policy-file policy.json
```


The `gatekeeping` field in blast-radius response shows per-symbol policy status:

```json
{
  "gatekeeping": {
    "policy_status": "VIOLATED",
    "violations": ["impact zone size 42 exceeds max 15"],
    "handoffs": []
  }
}
```

**Policy status values:**
- `PASSED` — Symbol passes all rules
- `VIOLATED` — Symbol violates at least one rule
- `SKIPPED` — No policy file or `--no-policy` flag

### Crafting Good Policies

**Start permissive, tighten gradually:**
1. Run `metrics --pagerank` to find top hotspots
2. Set `max_impact_nodes` to 90th percentile of current impact zones
3. Monitor violations over several PRs
4. Gradually lower threshold as architecture improves

**Centrality threshold:**
- 0.5-0.7: Moderate centrality (informational)
- 0.7-0.9: High centrality (review required)
- 0.9+: Critical path (extra scrutiny)

**Forbidden crossings:**
- Use specific patterns: `.*controller.*` not `.*`
- Provide clear `reason` fields for developer guidance
- Test name patterns with `find` / `callers` before adding to policy

### Common Violations

| Violation | Cause | Fix |
|-----------|-------|-----|
| Impact zone too large | Utility function called everywhere | Extract interface, inject dependency |
| High centrality | Central hub in call graph | Split into smaller functions, reduce coupling |
| Forbidden crossing | Controller → Database direct call | Introduce service layer |

---

## Common Workflows

### Workflow 1: Understand Implicit Architecture

**Goal:** Map the codebase's functional structure

```bash
# 1. List communities (sorted by size)
rgctl -f json communities list

# 2. Identify largest/notable communities
# Look for labels like "Infrastructure", domain names

# 3. Explore a community via semantic + impact
rgctl -f json inventory --by community
rgctl -f json semantic query "<community label>" --scope community --limit 20

# 4. Probe coupling with callers / blast-radius on seed symbols from hits
rgctl -f json callers '<KeyFunction>' --depth 2
rgctl -f json blast-radius '<KeyFunction>' --depth 2
```

**Report:**
- Top 5 communities by size
- Notable labels (infrastructure, domain-specific)
- Cross-community coupling patterns

### Workflow 2: Plan Microservice Extraction

**Goal:** Identify service boundaries from communities

```bash
# 1. Find large cohesive communities
rgctl -f json communities list

# 2. For candidate community, probe external impact from seed symbols
rgctl -f json semantic query "<community label>" --scope community --limit 10
rgctl -f json callers '<KeyFunction>' --depth 2 --scope-mode outside
rgctl -f json blast-radius '<KeyFunction>' --depth 3
```

**Report:**
- Candidate communities (size 20-100 functions, clear domain)
- External dependencies (API surface)
- Blast-radius of extraction (migration risk)

### Workflow 3: Enforce Architectural Rules

**Goal:** Set up CI gate to prevent violations

```bash
# 1. Analyze current state
rgctl -f json metrics --pagerank
# Find 90th percentile impact zone size

# 2. Create policy.json
cat > policy.json <<EOF
{
  "max_impact_nodes": 25,
  "centrality_alert_threshold": 0.8,
  "forbidden_crossings": [
    {
      "from_pattern": ".*controller.*",
      "to_pattern": ".*repository.*",
      "reason": "Use service layer between controllers and repositories"
    }
  ]
}
EOF

# 3. Test policy locally
rgctl -f json check --policy-file policy.json

# 4. Add to CI (see CI Integration above)
```

**Report:**
- Current violations (with context)
- Threshold rationale (based on metrics)
- Migration plan for existing violations

### Workflow 4: Track Architectural Drift

**Goal:** Monitor policy compliance over time

```bash
# In CI, save policy results
rgctl -f json check --policy-file policy.json > policy-results.json

# Track violations count in your dashboard
jq '.violations | length' policy-results.json

# Alert on new violations
diff <(jq -r '.violations[].symbol' main-policy.json | sort) \
     <(jq -r '.violations[].symbol' pr-policy.json | sort)
```

### Workflow 5: Find Feature Ownership

**Goal:** "Which subsystem owns the checkout flow?"

```bash
# Option 1: Semantic search scoped to communities
rgctl -f json semantic query "checkout flow" --scope community

# Option 2: Find function + check its community
rgctl -f json semantic query "checkout" --limit 5
# Note the node_id from top hit
rgctl -f json blast-radius <node_id>
# Response includes community_id

# Option 3: List communities and grep labels
rgctl -f json communities list | jq -r '.communities[] | "\(.id): \(.label)"' | grep -i checkout
```

---

## Troubleshooting

### Communities

| Issue | Fix |
|-------|-----|
| No communities detected | Run `discover` (detection happens during analysis) |
| Labels are stale/missing | Run `communities label --write` |
| `--scope community` returns 0 | Large repos produce granular clusters; use `communities list` + grep instead |
| Community ID unknown | Run `communities list` first; ID only exists if analysis ran |

### Policy Checks

| Issue | Fix |
|-------|-----|
| `check` exits 1 | Expected on violations; parse JSON for details |
| Policy file not found | Use absolute path or relative to repo root |
| All functions violate threshold | Threshold too strict; use `metrics --pagerank` to calibrate |
| Forbidden crossing not triggering | Test regex with `find` / `callers` first; ensure pattern matches actual function names |

---

## See Also

- Community Detection Guide (product docs; not bundled in this skill pack) — In-depth tutorial
- CI Policy Checks Guide (product docs; not bundled in this skill pack) — Full policy schema
- Policy Format Reference (product docs; not bundled in this skill pack) — JSON schema specification
- Graph Metrics Guide (product docs; not bundled in this skill pack) — Centrality, PageRank
- Blast Radius Guide (product docs; not bundled in this skill pack) — Impact analysis
