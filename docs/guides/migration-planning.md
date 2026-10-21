# Migration Planning

## Introduction

rgctl can generate a **dependency-aware migration roadmap** that tells you the optimal order for extracting modules from a monolith into separate services or packages. The roadmap is produced by combining community detection, blast-radius analysis, PageRank, and harmonic centrality into a prioritized, topologically-sorted extraction plan.

Migration planning answers the critical question: "In what order should we extract pieces of this monolith so that each step is safe and dependencies are respected?"

## Use Cases

- **Monolith decomposition.** Generate a step-by-step plan for breaking a monolith into microservices.
- **Risk-ordered extraction.** Start with low-risk, low-dependency modules and work toward the core.
- **Stakeholder communication.** Present a data-driven migration plan to decision-makers.
- **Sprint planning.** Map migration steps to development sprints based on priority scores.
- **Architecture evolution.** Track how the migration plan changes as modules are extracted.

## Example Project

This guide uses the **CoolStore** (`example/coolstore`) -- a Java EE e-commerce application that is a realistic migration candidate.

## Step-by-Step

### 1. Run Discovery with Migration Flags

Migration planning requires harmonic centrality and the migration hints export:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover -l java \
  --with-cfg \
  --with-harmonic \
  --export-migration-hints
```

**Output (text mode):**

```
[>] rgctl discover
[✓] Loaded 26 files from snapshot -> 608 nodes, 1709 edges

✓ Control flow analysis:
  Field writes indexed: 41
  CFG/PDG/Dominance: 150 functions analyzed
  Skipped: 2 functions (unsupported_language=0, missing_source=0, analysis_error=2)
[✓] rgctl discover finished in ~300ms
```

With `-f json`, **stdout is the migration plan itself** (`schema_version` 2, `preset`, `steps`, …) — not DiscoverResponse telemetry (`command` / `metrics`). The same document is also written to `.rgctl/migration_plan.json`.

**What happened:**

- `--with-harmonic` computed harmonic centrality for every node, measuring how "reachable" each module is from the rest of the graph. This feeds into the migration priority score.
- `--export-migration-hints` generated `.rgctl/migration_plan.json` with the full roadmap (and, with `-f json`, emitted that plan on stdout).
- `--with-cfg` provided deep analysis data for more accurate blast-radius scoring.

### 2. Read the Migration Plan

Read the on-disk plan (same JSON as `-f json` stdout when `--export-migration-hints` is set):

```bash
cat example/coolstore/.rgctl/migration_plan.json
# or re-run discover with -f json --export-migration-hints
```

**Output (truncated):**

```json
{
  "schema_version": 2,
  "preset": "hybrid_default",
  "preset_label": "Hybrid Default",
  "order_mode": "scheduled",
  "weights": {
    "alpha": 0.33,
    "beta": 0.33,
    "gamma": 0.34
  },
  "steps": [
    {
      "step": 1,
      "community_id": 4,
      "label": "com.redhat.coolstore.persistence",
      "priority_score": -0.17,
      "schedule_step": 1,
      "priority_rank": 5,
      "avg_pagerank": 0.001435598242096603,
      "avg_harmonic": 0.0,
      "max_blast": 0.0
    },
    {
      "step": 2,
      "community_id": 3,
      "label": "com.redhat.coolstore.utils",
      "priority_score": 0.4095343470271542,
      "schedule_step": 2,
      "priority_rank": 1,
      "avg_pagerank": 0.0021056537225376815,
      "avg_harmonic": 0.013742449227720499,
      "max_blast": 0.0
    },
    {
      "step": 3,
      "community_id": 2,
      "label": "com.redhat.coolstore.rest",
      "priority_score": 0.273298724649785,
      "schedule_step": 3,
      "priority_rank": 2,
      "avg_pagerank": 0.0016656480341528852,
      "avg_harmonic": 0.018173883872805163,
      "max_blast": 0.0
    },
    {
      "step": 4,
      "community_id": 1,
      "label": "com.redhat.coolstore.service",
      "priority_score": 0.2551460368258126,
      "schedule_step": 4,
      "priority_rank": 3,
      "avg_pagerank": 0.001971891468308647,
      "avg_harmonic": 0.008867945703540873,
      "max_blast": 0.0
    },
    {
      "step": 5,
      "community_id": 0,
      "label": "com.redhat.coolstore.model",
      "priority_score": -0.06700304475953148,
      "schedule_step": 5,
      "priority_rank": 4,
      "avg_pagerank": 0.0016304640094605611,
      "avg_harmonic": 0.00038695176607285886,
      "max_blast": 0.0
    }
  ]
}
```

**What this tells you:**

- **`preset: "hybrid_default"`** -- the default migration strategy, balancing PageRank, harmonic centrality, and blast radius with equal weights (0.33/0.33/0.34).
- **`order_mode: "scheduled"`** -- steps are topologically sorted so dependencies are extracted before dependents.
- **Step 1** -- extract `com.redhat.coolstore.persistence` first (lowest coupling / priority on this run).
- **Step 2** -- extract `com.redhat.coolstore.utils` next in the scheduled order.
- **`priority_score`** -- a combined score from the three metrics. Lower (more negative) scores come first in the scheduled order.
- **`max_blast: 0.0`** -- neither of the first two steps has any blast-radius impact, making them safe starting points.

### 3. Migration Presets

rgctl offers four migration strategy presets:

```bash
# Foundational-first: extract shared libraries and utilities first
rgctl -r example/coolstore discover -l java \
  --with-harmonic --export-migration-hints \
  --migration-preset foundational_first

# Dense cluster: extract tightly-coupled clusters together
rgctl -r example/coolstore discover -l java \
  --with-harmonic --export-migration-hints \
  --migration-preset dense_cluster

# Risk mitigation: prioritize low-risk extractions
rgctl -r example/coolstore discover -l java \
  --with-harmonic --export-migration-hints \
  --migration-preset risk_mitigation
```

| Preset | Strategy | Best For |
|--------|----------|----------|
| `hybrid_default` | Equal weight to PageRank, harmonic, blast radius | General purpose |
| `foundational_first` | Prioritize shared infrastructure | Libraries-first approach |
| `dense_cluster` | Group tightly-coupled modules | Cluster extraction |
| `risk_mitigation` | Prioritize low-risk, low-impact modules | Conservative migration |

### 4. Migration Order Modes

Control how steps are sorted:

```bash
# Scheduled: dependency-aware topological sort (default)
rgctl -r example/coolstore discover -l java \
  --with-harmonic --export-migration-hints \
  --migration-order scheduled

# Priority: sort by priority score regardless of dependencies
rgctl -r example/coolstore discover -l java \
  --with-harmonic --export-migration-hints \
  --migration-order priority
```

| Mode | Sorting | Use When |
|------|---------|----------|
| `scheduled` | Topological (respects dependencies) | You want a safe, step-by-step plan |
| `priority` | By priority score (highest risk reduction first) | You want to see which modules matter most |

### 5. Reading the Plan Fields

Each step in the migration plan contains:

| Field | Meaning |
|-------|---------|
| `step` | Extraction order (1 = first) |
| `community_id` | The community (module cluster) to extract |
| `label` | Human-readable name for the community |
| `priority_score` | Combined metric score |
| `schedule_step` | Position in the topological sort |
| `priority_rank` | Position in the pure priority ranking |
| `avg_pagerank` | Average PageRank of functions in this community |
| `avg_harmonic` | Average harmonic centrality of functions in this community |
| `max_blast` | Maximum blast-radius score of any function in this community |

### 6. Exploring Migration Steps

After reviewing the plan, use other rgctl commands to dig deeper:

```bash
# See what functions are in the community being extracted
rgctl -r example/coolstore -f json communities list | jq '.communities[:5]'
rgctl -r example/coolstore -f json find --type function --scope com.redhat.coolstore.persistence --limit 20

# Check blast radius of a specific function before extracting
rgctl -r example/coolstore blast-radius getShoppingCart --depth 3

# View mutations on the domain model being extracted
rgctl -r example/coolstore -f json cpg mutations \
  --type ShoppingCart --exclude-ctors
```

### 7. Dashboard Visualization

If you enabled `--with-dashboard`, the migration plan is also available in the browser dashboard:

```bash
rgctl -r example/coolstore serve --open
```

Navigate to the **Migration** tab to see the roadmap as an interactive timeline.

## The Migration Algorithm

The migration plan is generated by:

1. **Community detection** -- partition the graph into functional clusters (Louvain algorithm).
2. **Metric computation** -- compute PageRank, harmonic centrality, and blast radius for every function.
3. **Community scoring** -- aggregate metrics per community (average PageRank, average harmonic, max blast).
4. **Priority ranking** -- combine the three scores with configurable weights (alpha, beta, gamma).
5. **Topological sorting** -- order communities so dependencies come before dependents.

See [Migration Algorithms](../migration-algorithms.md) for the academic background.

## Benefits

- **Data-driven ordering.** Replace intuition with quantitative analysis for deciding what to extract first.
- **Dependency-safe.** The scheduled order ensures you never extract a module before its dependencies.
- **Configurable strategy.** Four presets and two ordering modes let you tailor the plan to your migration approach.
- **Full traceability.** Every step includes the metrics that justify its position in the plan.
- **Composable.** Use structured queries, blast-radius, and CPG to investigate each migration step in detail.

## Related Guides

- [Discovering and Indexing a Codebase](discovering-and-indexing.md) -- `discover` with migration flags
- [Graph Metrics](graph-metrics.md) -- the metrics that feed into migration scoring
- [Community Detection](community-detection.md) -- communities define the extraction units
- [Blast Radius Analysis](blast-radius-analysis.md) -- per-function impact analysis for migration steps
- [CI Policy Checks](ci-policy-checks.md) -- enforce architectural rules during migration
