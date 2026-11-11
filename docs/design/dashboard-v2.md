# Universe Dashboard v2 & HTTP Server — Engineering Design

This document specifies the **Universe Dashboard v2, Static Artifact Bundle (`rgctl-dashboard`), and Local HTTP Query Server (`rgctl serve`)** in `rgctl` (`crates/rgctl-dashboard/`, `dashboard/`, and `src/cli/http_serve.rs`). It details the end-to-end data flow from `rgctl discover --with-universe` to the React 19 + WASM + Three.js / Sigma.js browser client, the ambient `StableNodeKey` Focus mechanism, the 7-Room + 6-Lens information architecture, and the `/api/query`, `/api/cpg`, and `/api/semantic` HTTP endpoints.

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. BUNDLE GENERATION (`rgctl discover . --with-universe`)                    │
│                                                                              │
│  `crates/rgctl-dashboard` writes `.rgctl/dashboard/`:                        │
│    ├─► `manifest.json` (`graph_digest`, coverage flags, mode = `"universe"`) │
│    ├─► `graph_payload.bin` (compact binary topology for browser WASM engine) │
│    ├─► `metagraph.json` & `communities.json` (L0 namespaces, L1 packages,    │
│    │     Fast-Leiden labels, coupling web)                                   │
│    ├─► `migration_graph.json` (v3) & `migration_plan.json` (v2)              │
│    ├─► `function_metrics.json` (or `sparse_mode: "community_only"` >= 50k)   │
│    └─► Optional sidecars: `cfg_index.json`, `slice_index.json`,              │
│        `taint_index.json`, `blast_index.json`, `.rgctl/workflows/review.json`│
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. LOCAL HTTP SERVER (`rgctl serve [--open] [--watch]`, `http_serve.rs`)     │
│                                                                              │
│  • Serves static `.rgctl/dashboard/` + embedded `dashboard/dist` assets      │
│  • `POST /api/query` (`/graphql`) → Structured & GQL graph queries           │
│  • `POST /api/cpg`                → Hybrid CPG (`status`, `mutations`, etc.) │
│  • `GET  /api/semantic/status` & `POST /api/semantic/query`                  │
│  • Optional `--watch`: 25ms debounced `notify` watcher → `IncrementalUpdater`│
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. BROWSER CLIENT (`dashboard/src/v2/`, React 19 + Web Worker WASM)          │
│                                                                              │
│  • Persistent Instrument Bar: Coverage · Digest · `⌘K` Omnibox · Focus Bar   │
│  • Portable URL Focus (`?focus=<path\x1fname>&room=<room>&lens=<lens>`)      │
│  • Orientation: `Repo` ("Where do I start?")                                 │
│  • 7 Rooms: `Architecture` (landing) · `Review` · `Taint` · `Migration` ·    │
│             `Understand` · `Agent` · `Proof`                                 │
│  • 6 Lenses: `CFG` · `Dataflow` · `Slice/Trace` · `Blast` · `Metrics` · `Time`│
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust & TypeScript |
| :--- | :--- | :--- |
| **Hierarchical Software Landscape Metaphor (L0/L1/L2 Universe)** | Knight & Munro (2000), *Virtual But Visible Software*; Wettel & Lanza (2007), *Visualizing Software Systems as Cities* | Instead of dumping 100,000 raw nodes onto a 2D hairball canvas, the **Architecture** room renders a 3-tier spatial hierarchy in imperative `three.js`: **L0** namespace volumes, **L1** package districts (`metagraph.json`), and **L2** coupling arcs, drilling down to symbol-level lenses only on selection. |
| **Location-Independent Ambient Focus State** | Portable entity addressing across views (`StableNodeKey`) | [`dashboard/src/v2/focus.ts`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/dashboard/src/v2/focus.ts) encodes the active symbol selection as a stable `(path, name, type)` key in the URL search parameters (`?focus=...&room=...&lens=...`) rather than a volatile array index, surviving re-indexing across commits. |
| **Digest-Keyed Immutable Client Caching** | Content-addressable state invalidation (`@tanstack/react-query`) | Every query key in the browser client is prefixed with `manifest.graph_digest` and configured with `staleTime: Infinity`. Bundle artifacts are never refetched while the snapshot digest is unchanged, and automatically invalidate the instant `rgctl serve --watch` publishes a new digest. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 `.rgctl/dashboard/` Artifact Manifest (`manifest.json`)

Generated by [`crates/rgctl-dashboard/src/manifest.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-dashboard/src/manifest.rs):

| Field / File | Purpose & Scale Guardrail |
| :--- | :--- |
| `manifest.json` | Records `graph_digest`, node/edge counts, `mode: "universe"`, and boolean capability flags (`cfg_available`, `taint_available`, `migration_available`, `semantic_available`). |
| `graph_payload.bin` | Compact binary node/edge topology loaded into the browser Web Worker (`wasm-bindgen`) for sub-millisecond client-side BFS, blast-radius, and neighborhood queries. |
| `metagraph.json` & `communities.json` | Package-level macro nodes, inter-package call weights, and Fast-Leiden community labels (`member_indices` omitted above 50k nodes to bound JSON size). |
| `function_metrics.json` | Per-function PageRank, betweenness, harmonic, and blast scores; switches to `sparse_mode: "community_only"` when $V \ge 50,000$. |
| `cfg_index.json` | Per-function CFG/PDG index; switches to `detail_mode: "archive_only"` on large repositories so per-function records are fetched lazily on demand. |

### 3.2 Seven Core UI Engineering Rules

1. **Answer First, Graph Second:** Every room opens on a ranked conclusion and its evidence table; the graph canvas is an inspection lens, never an unlabeled landing hairball.
2. **Every Number Carries a Receipt:** Every score, count, or verdict exposes its exact inputs and the reproducible `rgctl` CLI command.
3. **Lenses, Not Tabs:** `CFG`, `Dataflow`, `Slice/Trace`, `Blast`, `Metrics`, and `Time` inspect the current `Focus` subject without resetting room context.
4. **One Focus, Many Views:** Selecting a symbol in `Review` carries into `Taint`, `Architecture`, and `Understand` via the URL search parameters.
5. **Explicit Scope Boundaries:** When an analysis is intra-procedural or when `--with-cfg` / `--with-taint` was not run during `discover`, the UI states the exact boundary and the CLI command to enable it.
6. **Zero Cloud / Zero Telemetry:** All rendering, WASM graph traversal, and queries run 100% locally against `127.0.0.1`.
7. **Imperative Three.js Lifecycle:** 3D scenes in `Architecture` mount imperatively with explicit WebGL geometry/material/renderer disposal on unmount and `requestAnimationFrame` demand rendering (`needsRender` dirty flag).

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands & HTTP Endpoints

Verified against `rgctl discover --help` and `rgctl serve --help` ([`src/cli/mod.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/mod.rs#L932-L970)):

```bash
# 1. Discover and export the Universe Dashboard bundle (.rgctl/dashboard/)
rgctl discover . --with-universe

# 2. Full discovery (CFG/PDG, taint, harmonic centrality, and Universe Dashboard)
rgctl discover . --full

# 3. Serve the dashboard and HTTP query API locally (opens browser at http://127.0.0.1:8080)
rgctl serve --open

# 4. Serve with live file-watching incremental updates (25ms debounce)
rgctl serve --open --watch

# 5. Serve only the HTTP query APIs (no static dashboard UI required)
rgctl serve --query-only --host 127.0.0.1 --port 8080
```

#### Local HTTP Endpoints (`src/cli/http_serve.rs`)

| Endpoint | Method | Purpose |
| :--- | :--- | :--- |
| `/` | `GET` | Serves the Universe Dashboard SPA (`.rgctl/dashboard/` + embedded bundle). |
| `/api/query` (alias `/graphql`) | `POST` | Executes structured queries or GQL queries against the live snapshot. |
| `/api/cpg` | `POST` | Executes Hybrid CPG operations (`status`, `function`, `calls`, `mutations`, `flows`, `pdg`, `slice`). |
| `/api/semantic/status` | `GET` | Reports `.rgctl/semantic_index.bin` readiness, model ID, and row count. |
| `/api/semantic/query` | `POST` | Runs two-stage Hamming + late-fusion semantic search. |

### 4.2 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Rust Dashboard Bundle Exporter** | [`crates/rgctl-dashboard/src/lib.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-dashboard/src/lib.rs), [`manifest.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-dashboard/src/manifest.rs), [`metagraph.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-dashboard/src/metagraph.rs) | `export_dashboard_bundle`, `DashboardManifest`, `build_metagraph` |
| **HTTP Server & Live Watcher** | [`src/cli/http_serve.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/http_serve.rs), [`src/cli/file_watch.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/file_watch.rs) | `serve`, `handle_api_query`, `handle_api_cpg` |
| **Dashboard v2 Shell & Focus** | [`dashboard/src/v2/`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/dashboard/src/v2/) | `focus.ts`, `DEFAULT_ROOM`, `ArchitectureRoom`, `ReviewRoom` |
| **Legacy & Lens Views** | [`dashboard/src/`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/dashboard/src/) | `App.tsx`, `BlastView.tsx`, `CfgView.tsx`, `DataflowView.tsx`, `SliceView.tsx`, `TaintView.tsx`, `MigrationView.tsx`, `SearchView.tsx` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Serve Never Auto-Runs Full Discover:** `rgctl serve` is strictly a fast read/query server over `.rgctl/` (plus optional incremental `--watch`). It never blocks startup with an implicit whole-repo `discover`.
2. **In-Memory `DashboardExportContext` Reuse:** When `rgctl discover --with-universe` runs, `export_dashboard_bundle` consumes the in-memory `AnalysisResults` and `CodeGraph` directly via `DashboardExportContext` without reloading snapshots per export stage.

### Verification Commands

```bash
# Build frontend assets and verify Rust dashboard bundle integration tests
./scripts/build-dashboard.sh
cargo test -p rgctl-dashboard
cargo test --test dashboard_harness

# Run frontend unit tests
cd dashboard && npm run test:unit
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Focus Keys Across Symbol Renames:** A URL Focus key (`path\x1fname`) survives re-indexing when line numbers or node indices shift, but if a function is renamed, old URL links render an explicit `"Focus does not resolve against this bundle"` banner rather than guessing.
2. **Static File Hosting vs. `rgctl serve`:** While static bundle JSON files (`metagraph.json`, `migration_graph.json`) can be inspected offline, interactive `/api/query`, `/api/cpg`, and `/api/semantic/query` endpoints require running `rgctl serve`.
