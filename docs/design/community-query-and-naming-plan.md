# Fast-Leiden Community Detection, Naming & Query — Engineering Design

This document specifies the **Fast-Leiden Community Detection, $\mu + 4\sigma$ Hub Stripping, Heuristic Naming, and Community Query subsystem** in `rgctl` (`crates/rgctl-analysis/src/community.rs`, `results.rs`, `crates/rgctl-dashboard/src/communities.rs`, and `src/cli/communities.rs`). It details how `rgctl` partitions the call and reference graph into guaranteed well-connected communities, labels each cluster without mutating `graph.snapshot.bin`, and exposes community inventories to LLM agents and the dashboard.

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. CSR TOPOLOGY & HUB STRIPPING (`crates/rgctl-analysis/src/community.rs`)   │
│                                                                              │
│  `FlatGraphIndex` (`Calls` + `Uses` + `References`)                          │
│    ├─► Compute degree distribution `mu` (mean) and `sigma` (std dev)         │
│    └─► `HubStripPolicy::MeanPlusFourSigma`:                                  │
│          • Isolates ubiquitous utility hubs with `deg > max(32, mu + 4*sigma)`│
│            into `"Shared / Infrastructure"` (`infrastructure_community_id`)  │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. FAST-LEIDEN THREE-STAGE CLUSTERING (`CommunityDetector`)                  │
│                                                                              │
│  Repeat until modularity `Q` converges:                                      │
│    1. Queue-Based Local Moving:                                              │
│         Move nodes to neighbor communities maximizing Newman-Girvan `\Delta Q`│
│    2. BFS Connected-Subgraph Refinement (`refine_partition`):                │
│         Split any disconnected community components (guarantees 0 bridges)   │
│    3. Super-Node Coarsening:                                                 │
│         Aggregate refined communities into super-nodes for next pass         │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. HEURISTIC LABELING & READ-ONLY OVERLAY QUERIES                            │
│                                                                              │
│  • `fill_community_labels`: infers human-readable domain labels from         │
│    dominant package/path prefixes and symbol token frequencies               │
│  • Persisted in `.rgctl/analysis_results.bin` (`CommunityTable` + labels)    │
│  • CLI: `rgctl communities list`, `rgctl communities label [--write]`,       │
│         `rgctl inventory --by community`, `rgctl metrics --communities`      │
│  • Dashboard: `.rgctl/dashboard/communities.json` + `metagraph.json`         │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Leiden Algorithm (Well-Connected Communities)** | Traag, Waltman & van Eck (2019), *From Louvain to Leiden: Guaranteeing Well-Connected Communities* (*Scientific Reports*, 9:5233) | Classic Louvain can produce internally disconnected communities when a bridge node moves out. [`CommunityDetector`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/community.rs#L1-L340) implements **Fast-Leiden**: queue-driven local modularity moving followed by explicit BFS connected-component refinement (`refine_partition`) and super-node coarsening. |
| **Newman-Girvan Modularity ($Q$)** | Newman & Girvan (2004), *Finding and Evaluating Community Structure in Networks* | Optimizes $Q = \frac{1}{2m} \sum_{i,j} \left[ A_{ij} - \gamma \frac{k_i k_j}{2m} \right] \delta(c_i, c_j)$ with configurable resolution $\gamma$ (`CommunityConfig::resolution = 1.0`). |
| **Scale-Free Utility Hub Stripping ($\mu + 4\sigma$)** | Power-law hub suppression in software dependency networks | Ubiquitous logging, error, and string utilities otherwise pull unrelated business domains into one giant hairball cluster. `HubStripPolicy::MeanPlusFourSigma` strips vertices with degree $> \max(32, \mu + 4\sigma)$ prior to clustering and places them in a dedicated `"Shared / Infrastructure"` partition. |
| **Path-Prefix & Token-Frequency Cluster Labeling** | TF-IDF cluster summarization (Manning, Raghavan & Schütze, 2008) | `infer_label_from_names` and `fill_community_labels` derive concise human-readable names (e.g., `orders / checkout` or package prefix `com.example.billing`) from member paths and camelCase/snake_case identifier tokens, deduplicating collisions (`auth`, `auth (2)`). |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Core Rust Structures (`crates/rgctl-analysis/src/community.rs` & `results.rs`)

```rust
pub enum HubStripPolicy {
    None,
    MeanPlusFourSigma { min_degree: usize }, // default min_degree = 32
}

pub struct CommunityTable {
    /// Dense node_idx -> community_id assignment
    pub node_communities: Vec<usize>,
    /// Global Newman-Girvan modularity Q in [-0.5, 1.0]
    pub modularity: f64,
    /// Optional ID reserved for stripped infrastructure hubs
    pub infrastructure_community_id: Option<usize>,
    /// Heuristic domain labels keyed by community_id
    pub labels: HashMap<usize, String>,
}
```

### 3.2 Separation of Topology (`L_repo`) and Derived Partition (`analysis_results.bin`)

- **Strict Snapshot Invariance:** Community assignments and labels are **never** written into `.rgctl/graph.snapshot.bin`. Refreshing labels via `rgctl communities label --write` updates only `.rgctl/analysis_results.bin`, leaving the `RGCL` v2 `content_digest` untouched.

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl communities --help`, `rgctl inventory --help`, and [`src/cli/communities.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/communities.rs):

```bash
# List all detected Fast-Leiden communities with heuristic labels and member counts
rgctl communities list -f json

# Recompute heuristic labels and persist them into .rgctl/analysis_results.bin
rgctl communities label --write -f json

# Aggregate symbol counts by community ID via structured query inventory
rgctl inventory --by community --limit 20 -f json

# Inspect modularity and top communities via metrics
rgctl metrics --communities -f json
```

### 4.2 JSON Schema (`rgctl communities list -f json`)

```json
{
  "schema_version": 1,
  "modularity": 0.642,
  "written": false,
  "communities": [
    {
      "id": 0,
      "label": "Shared / Infrastructure",
      "member_count": 48
    },
    {
      "id": 1,
      "label": "com.example.order / checkout",
      "member_count": 112
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Fast-Leiden Detector & Hub Policy** | [`crates/rgctl-analysis/src/community.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/community.rs) | `CommunityDetector`, `CommunityConfig`, `HubStripPolicy`, `refine_partition`, `fill_community_labels`, `CommunityQueryContext` |
| **Persisted Community Table** | [`crates/rgctl-analysis/src/results.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/results.rs) | `AnalysisResults`, `CommunityTable` |
| **CLI Communities Command** | [`src/cli/communities.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/communities.rs) | `CommunitiesLabelArgs`, `run_list`, `run_label` |
| **Structured Query Inventory** | [`crates/rgctl-graph/src/structured_query.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/structured_query.rs) | `InventoryBy::Community` |
| **Dashboard Export** | [`crates/rgctl-dashboard/src/communities.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-dashboard/src/communities.rs) | `summarize_communities` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Deterministic Tie-Breaking:** Queue-based local moving breaks modularity gain ties deterministically by community ID so repeated runs on identical graphs produce reproducible partitions.
2. **Linear-Space Coarsening:** Super-node coarsening operates over compact `u32` CSR vectors (`FlatGraphIndex`) rather than allocating recursive `petgraph` instances per hierarchy level.

### Verification Commands

```bash
# Run Fast-Leiden community detection and naming unit tests
cargo test -p rgctl-analysis community

# Run Criterion community detection benchmarks
cargo bench -p rgctl-analysis --bench community_benchmarks
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Heuristic Domain Labels (No LLM Required):** Default community labels are derived deterministically from file paths and symbol tokens without calling an external LLM. Repositories with flat single-directory layouts fall back to dominant symbol token stems or `Community <id>` when token entropy is high.
2. **Migration Planner Uses Package Macro Nodes:** While Fast-Leiden `community_id` drives cluster coloring and ForceAtlas2 layout attraction in the dashboard, the Migration Planner (`migration_graph.json` v3) schedules concrete package/module macro nodes (`package_label(file_path)`) annotated with their majority-vote `community_id`.
