# `rgctl` Engineering Design Documents

**Audience:** Contributors, systems engineers, and maintainers working on `rgctl`. For the top-level documentation hub, start at [**`docs/README.md`**](../README.md).

Every engineering design document in this directory follows the **6-section `rgctl` architecture standard** established by [`lossless-writeback-architecture.md`](lossless-writeback-architecture.md):
1. **System Overview & End-to-End Data Flow** (ASCII architecture pipeline)
2. **Theoretical Foundations Mapped to `rgctl`** (Academic / compiler / graph theory origins mapped to exact Rust modules)
3. **Binary Layout, Data Structures & Memory Discipline** (Exact byte layouts, `.rgctl/` sidecars, complexity, and `content_digest` invalidation rules)
4. **CLI Contract, JSON Schema & Implementation Map** (Verified CLI invocations, JSON schemas, and source file tables)
5. **Performance Guardrails & Verification Checklist** (Ingest hot-path invariants, unit/integration tests, Criterion benchmarks, and Cold Profile Gates)
6. **Known Limitations & By-Design Boundaries** (Explicit soundness and scope boundaries)

---

## Core Architecture Specifications

| Architecture Document | Scope |
| :--- | :--- |
| [**Graph Storage & Extraction Architecture**](../graph-storage-architecture.md) | `RGCL` v2 (`136B` header, `64B NodeRow`, `40B EdgeRow`), `SegmentedSpill` external sort, `ColdMetadataDb` (`NodeExtension` + 14B Green span tail), `RBPK` parked relations, `RGCS` v2 / `RGCT` v1 lossless sidecars, and `DeltaSegment` compaction. |
| [**Graph Analysis, CPG & Reachability Architecture**](../analysis-architecture.md) | 3-tier memory model, `StructuralTopology` / `CodeGraphCsr` / `FlatGraphIndex`, verified centrality & HyperBall constants, Fast-Leiden communities, SCC blast radius, and the v3 `cfg_pdg.archive.bin` archive. |
| [**Workspace Code Structure**](../Code_structure.md) | Crate dependency graph, layer responsibilities, CLI command dispatch map, and `.rgctl/` artifact inventory. |

---

## Feature & Subsystem Design Documents (`docs/design/`)

### 1. Lossless Storage, Diff & CI Governance

| Feature | Design Document | CLI Entrypoints |
| :--- | :--- | :--- |
| **Lossless Source Reconstruction & Surgical Write-Back** | [**`lossless-writeback-architecture.md`**](lossless-writeback-architecture.md) | `rgctl discover --lossless`, `rgctl writeback verify\|export\|patch` |
| **Columnar Graph Snapshot Diff** | [**`graph-diff-design.md`**](graph-diff-design.md) | `rgctl diff`, `rgctl review paths\|check` |
| **CI Policy Checks & Temporal PR Review** | [**`ci-policy-checks-design.md`**](ci-policy-checks-design.md) | `rgctl check`, `rgctl review paths\|check\|export` |

### 2. Intra-Procedural Compiler IR, Hybrid CPG & Security

| Feature | Design Document | CLI Entrypoints |
| :--- | :--- | :--- |
| **Control-Flow Graph (CFG) & v3 Archive** | [**`cfg-design.md`**](cfg-design.md) | `rgctl discover --with-cfg`, `rgctl inspect <symbol> cfg` |
| **Dominance & Post-Dominance Trees** | [**`dominance-design.md`**](dominance-design.md) | `rgctl inspect <symbol> dom [--frontiers]` |
| **Program Dependence Graph (PDG) & Dataflow** | [**`pdg-design.md`**](pdg-design.md) | `rgctl inspect <symbol> pdg`, `rgctl discover --with-dfg-loops` |
| **Intra- & Inter-Procedural Program Slicing** | [**`program-slicing-design.md`**](program-slicing-design.md) | `rgctl slice <file> --line <N> --variable <V> [--with-alias]` |
| **Hybrid Code Property Graph (CPG)** | [**`hybrid-cpg-plan.md`**](hybrid-cpg-plan.md) | `rgctl cpg status\|function\|calls\|mutations\|flows\|pdg\|slice\|ast\|export` |
| **Taint Analysis & OSV/OpenVEX Reachability** | [**`taint-analysis-design.md`**](taint-analysis-design.md) | `rgctl taint --sink`, `rgctl slice --taint`, `rgctl vuln triage\|analyze`, `rgctl deps check` |

### 3. Macro Graph Analytics, Migration, Clones, Search & UI

| Feature | Design Document | CLI Entrypoints |
| :--- | :--- | :--- |
| **Blast Radius & Kosaraju SCC Reachability** | [**`blast-radius-design.md`**](blast-radius-design.md) | `rgctl blast-radius <symbol> [--depth N] [--summary] [--with-slices]` |
| **Graph Centrality & Network Metrics** | [**`graph-metrics-design.md`**](graph-metrics-design.md) | `rgctl metrics [--pagerank] [--betweenness] [--communities] [--churn]` |
| **Fast-Leiden Community Detection & Naming** | [**`community-query-and-naming-plan.md`**](community-query-and-naming-plan.md) | `rgctl communities list\|label`, `rgctl inventory --by community` |
| **Software Migration Planner & Census** | [**`migration-planner-design.md`**](migration-planner-design.md) | `rgctl discover --with-harmonic --export-migration-hints`, `rgctl inventory --by import-prefix`, `rgctl find --annotation --show-attributes` |
| **Semantic Search & Multi-Signal Late Fusion** | [**`semantic-search-design.md`**](semantic-search-design.md) | `rgctl semantic index\|query\|distill` |
| **Clone Detection (Type-1 Exact, Type-2 Bloom, Type-3/4 SESE+1-WL)** | [**`clone-detection-design.md`**](clone-detection-design.md) | `rgctl clones [--mode exact\|bloom\|fragment]` |
| **Universe Dashboard v2 & Local HTTP Server** | [**`dashboard-v2.md`**](dashboard-v2.md) | `rgctl discover --with-universe`, `rgctl serve [--open] [--watch]` |

---

## Regenerating Design Screenshots

PNG assets live under [`docs/images/design/`](../images/design/) per feature subdirectory. To regenerate after UI changes:

```bash
./scripts/build-dashboard.sh
cargo build --release
rgctl -r /path/to/repo discover --full
rgctl -r /path/to/repo serve --port 8080

DASHBOARD_URL=http://127.0.0.1:8080/ node dashboard/scripts/capture-design-screenshots.mjs
DASHBOARD_URL=http://127.0.0.1:8080/ node dashboard/scripts/capture-migration-screenshots.mjs
```

---

## Companion Documentation

| Audience | Document |
| :--- | :--- |
| **Overview & Feature Tour** | [`docs/Introduction.md`](../Introduction.md) |
| **CLI User Guide** | [`docs/user-guide.md`](../user-guide.md) |
| **JSON Output Schemas** | [`docs/json-api.md`](../json-api.md) |
| **Academic & Research Foundations** | [`docs/further-reading.md`](../further-reading.md) |
| **Cold Profile Gates & Baselines** | [`docs/internal/profile.md`](../internal/profile.md) |
