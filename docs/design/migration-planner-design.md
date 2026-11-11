# Software Migration Planner & Census — Engineering Design

This document specifies the **Software Migration Planner, Package Macro-Graph Scheduling, and Tier-1 Structured Query Census subsystem** in `rgctl` (`crates/rgctl-analysis/src/migration.rs`, `crates/rgctl-graph/src/structured_query.rs`, `crates/rgctl-dashboard/`, and `src/cli/`). It covers how `rgctl` combines PageRank, Harmonic Centrality, Blast Radius, and Fast-Leiden community votes into a dependency-aware package extraction roadmap (`migration_graph.json` v3 and `migration_plan.json` v2), alongside the structured query census primitives (`inventory --by import-prefix`, `find --annotation --show-attributes`, and seedless `relations`) used for framework/API migrations.

![Migration tab — unified roadmap (gbuilder)](../images/design/migration-planner/migration-unified-roadmap.png)

*Figure 1: Unified Migration view — multi-objective α/β/γ weight sliders, package dependency graph, and paginated extraction schedule.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. STRUCTURAL CENSUS & METRICS INPUTS (`.rgctl/`)                            │
│                                                                              │
│  A. Macro Graph Metrics (`analysis_results.bin` + `blast_engine.snapshot`):  │
│     • `FastPageRank` (α), `HyperBallHarmonic` (β, `--with-harmonic`),        │
│       `BlastRadiusEngine` max impact (γ), `Fast-Leiden` `community_id`       │
│  B. Tier-1 Framework/API Census (`graph.snapshot.bin` + sidecars):           │
│     • `parked_relations.bin` (`RBPK`) → `rgctl inventory --by import-prefix` │
│     • `annotation_args.json` → `rgctl find --annotation --show-attributes`   │
│     • Seedless `rgctl relations --edge <TYPE> --from-type <A> --to-type <B>` │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. PACKAGE MACRO-GRAPH & MULTI-OBJECTIVE SCHEDULER (`migration.rs`)          │
│                                                                              │
│  • Group functions by `package_label(file_path)` (capped at top 255 + other) │
│  • Aggregate inter-package `Calls` edges (`caller_pkg -> callee_pkg`)        │
│  • Compute normalized priority score per package `p`:                        │
│      `Priority(p) = α·norm(PR_p) + β·norm(Harm_p) - γ·norm(MaxBlast_p)`      │
│  • Schedule order:                                                           │
│      - `scheduled` (default): dependency-aware topological sort (callees     │
│        before callers, breaking cycles by lowest blast risk)                 │
│      - `priority`: pure descending `Priority(p)` rank                        │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. PERSISTENT ARTIFACTS & INTERACTIVE CONSUMERS                              │
│                                                                              │
│  • `.rgctl/migration_plan.json` (`schema_version = 2`, `--export-migration-hints`)│
│  • `.rgctl/dashboard/migration_graph.json` (`schema_version = 3`)            │
│  • Dashboard `MigrationView.tsx` (TypeScript mirror for live slider tuning)  │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Multi-Objective Centrality & Risk Ranking** | Panyala et al. (2017); Gargiulo et al. (2015), *Componentwise PageRank*; Boldi & Vigna (2013), *HyperBall* | [`compute_migration_plan`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/migration.rs#L220-L420) min-max normalizes average PageRank ($\alpha$), average Harmonic centrality ($\beta$), and maximum Blast Radius ($\gamma$) across package macro-nodes to balance architectural foundation value against blast risk. |
| **Strangler-Fig Topological Wave Scheduling** | Fowler (2004), *StranglerFigApplication*; Kahn's topological sort with cycle breaking | In `scheduled` mode, `compute_migration_plan` schedules leaf dependency targets (zero or satisfied outgoing package dependencies) in earlier waves so foundational shared modules are migrated before upstream callers. |
| **Prefix-Aggregated External Dependency Census** | Software supply-chain & namespace migration analysis (e.g., `javax.*` → `jakarta.*`) | [`StructuredQuery::inventory_import_prefix`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/structured_query.rs#L1-L260) aggregates both `NodeType::Import` nodes and unresolved `.rgctl/parked_relations.bin` (`RBPK`) targets by 2-segment namespace prefix (with `--prefix` filtering) so agents can quantify framework footprints in one command. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Package Macro-Graph (`migration_graph.json` v3) & Plan (`migration_plan.json` v2)

Defined in [`crates/rgctl-analysis/src/migration.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/migration.rs):

```rust
pub struct MigrationMacroNode {
    pub id: usize,
    pub label: String,              // package_label(file_path), e.g. "com.example.order"
    pub member_count: usize,
    pub avg_pagerank: f64,
    pub avg_harmonic: f64,
    pub avg_betweenness: f64,
    pub max_blast_radius: f64,
    pub community_id: Option<usize>, // Majority-vote Fast-Leiden community ID
}

pub struct MigrationWeights {
    pub alpha: f64, // PageRank weight
    pub beta: f64,  // Harmonic centrality weight
    pub gamma: f64, // Blast radius penalty weight
}
```

#### Strategy Presets (`--migration-preset`)

| Preset Name | $\alpha$ (PageRank) | $\beta$ (Harmonic) | $\gamma$ (Blast Penalty) | Target Use Case |
| :--- | :--- | :--- | :--- | :--- |
| `hybrid_default` | `0.33` | `0.33` | `0.34` | Balanced architectural extraction (default). |
| `foundational_first` | `0.60` | `0.25` | `0.15` | Bottom-up shared library / core schema modernization. |
| `dense_cluster` | `0.25` | `0.60` | `0.15` | Cohesive domain/microservice extraction (`--with-harmonic`). |
| `risk_mitigation` | `0.20` | `0.20` | `0.60` | Conservative Strangler-Fig extraction (lowest blast radius first). |

### 3.2 Tier-1 Structured Query Census Sidecars

1. **Import Prefix Census (`inventory --by import-prefix`):** Scans `NodeType::Import` nodes in `graph.snapshot.bin` plus `ParkedRelationRecord` entries in `.rgctl/parked_relations.bin` (`RBPK` v1), grouping external imports into 2-segment namespace buckets (`javax.persistence`, `org.springframework`, `tokio::sync`, etc.).
2. **Annotation Attribute Census (`find --annotation <NAME> --show-attributes`):** Reads `.rgctl/annotation_args.json` (`HashMap<"{source_uuid}|{target_uuid}|{line}", String>`) to return raw annotation/decorator arguments (`@RequestMapping("/api/orders")`, `#[serde(rename_all = "camelCase")]`) alongside annotated symbols.

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl discover --help`, `rgctl inventory --help`, `rgctl find --help`, and `rgctl relations --help`:

```bash
# 1. Generate migration roadmap JSON (.rgctl/migration_plan.json) + dashboard macro graph
rgctl discover . --with-harmonic --export-migration-hints \
  --migration-preset risk_mitigation --migration-order scheduled -f json

# 2. Framework / namespace import census (e.g. javax -> jakarta or Spring Boot upgrade)
rgctl inventory --by import-prefix --prefix javax. --limit 50 -f json
rgctl find --package javax.persistence -f json

# 3. Multi-annotation census with raw annotation attribute arguments
rgctl find --annotation Stateless,Singleton,Inject --show-attributes -f json

# 4. Seedless typed relation scan across node types (e.g. all class inheritance or DI injections)
rgctl relations --edge inherits --from-type class --to-type class -f json
rgctl relations --edge overrides -f json

# 5. Container-level callers & blast-radius for an entire Class/Interface/Struct/Module
rgctl callers OrderService --depth 2 -f json
rgctl blast-radius OrderService -f json
```

### 4.2 JSON Schema (`.rgctl/migration_plan.json` v2)

```json
{
  "schema_version": 2,
  "preset": "risk_mitigation",
  "order": "scheduled",
  "weights": {
    "alpha": 0.2,
    "beta": 0.2,
    "gamma": 0.6
  },
  "steps": [
    {
      "step": 1,
      "package_id": 4,
      "label": "com.example.catalog.dto",
      "member_count": 18,
      "priority_score": 0.742,
      "avg_pagerank": 0.0041,
      "avg_harmonic": 0.312,
      "max_blast_radius": 8.5,
      "community_id": 2,
      "depends_on_packages": []
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Migration Macro-Graph & Scheduler** | [`crates/rgctl-analysis/src/migration.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/migration.rs) | `MigrationGraph`, `MigrationMacroNode`, `MigrationWeights`, `build_migration_graph`, `compute_migration_plan`, `package_label` |
| **Structured Query Census Engine** | [`crates/rgctl-graph/src/structured_query.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/structured_query.rs), [`src/cli/structured_query.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/structured_query.rs) | `StructuredQuery`, `inventory_import_prefix`, `find_by_annotation`, `collect_member_functions_of_type` |
| **Dashboard Export & TypeScript Mirror** | [`crates/rgctl-dashboard/src/migration_export.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-dashboard/src/migration_export.rs), [`dashboard/src/MigrationView.tsx`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/dashboard/src/MigrationView.tsx) | `export_migration_artifacts`, `MigrationView` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Bounded 256-Node Macro Graph:** `build_migration_graph` caps package macro nodes at `256` (top `255` packages by `member_count` plus `(other)`) so topological scheduling and browser ForceAtlas2 rendering execute in $< 10\text{ ms}$ even on 1M-node repositories.
2. **Rust ↔ TypeScript Scoring Parity:** Any change to `Priority(p)` normalization or topological tie-breaking in `crates/rgctl-analysis/src/migration.rs` must be mirrored in `dashboard/src/` and verified by `tests/dashboard_harness.rs`.

### Verification Commands

```bash
# Run migration planner & structured query unit/integration tests
cargo test -p rgctl-analysis migration
cargo test -p rgctl-graph structured_query
cargo test --test dashboard_harness
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Harmonic Term Requires `--with-harmonic`:** If `rgctl discover` is run without `--with-harmonic` (or `--full`), `avg_harmonic` is `0.0` for all packages and the $\beta$ term contributes `0.5` (uniform min-max fallback).
2. **Directory/Namespace Heuristic for `package_label`:** For Java/Kotlin, `package_label` extracts the canonical package path after `src/main/java/`; for Rust, C, C++, and Python, it derives dotted module/directory labels from the repository-relative path rather than parsing build-system link targets (`Cargo.toml` / `CMakeLists.txt`).
