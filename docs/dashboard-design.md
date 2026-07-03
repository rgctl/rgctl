# rBuilder static dashboard — engineering design (living document)

**Status:** Phase 6 complete (WASM blast radius + depth slider)  
**Last updated:** 2026-07-03  
**Owner:** rBuilder core / export pipeline  

This document is the **running source of truth** for the WASM + WebGL dashboard replacement. Update the [Implementation status](#implementation-status) section at the end of every phase PR.

Related: [performance-engineering.md](performance-engineering.md), [Code_structure.md](Code_structure.md), [cli-output-schemas.md](cli-output-schemas.md).

---

## Goals

| Goal | Target |
|------|--------|
| Scale | 300k+ nodes (community view); full exploration at smaller sizes |
| Distribution | Self-contained `.rbuilder/dashboard/` — no CDN, works offline |
| Data | Reuse **columnar v2** `graph.snapshot.bin` as `graph_payload.bin` (no third graph format) |
| Analysis | Rust/WASM worker; UI thread stays at 60 FPS |
| Legacy | **Delete** monolithic `html.rs` — no parallel stacks |

---

## Repository layout

```
dashboard/                      # Vite + Preact + Bootstrap 5 (bundled, offline)
  src/styles.css                # Bootstrap import + graph layout tweaks
  src/App.tsx                   # Tab shell, stat toolbar
  src/worker.ts                 # Loads WASM + graph_payload.bin
  dist/                         # Built assets (npm run build) — embedded at compile time

crates/rbuilder-dashboard/      # Rust: export bundle on discover
  src/lib.rs                    # export_dashboard_bundle()
  src/manifest.rs               # manifest.json schema
  src/bundle.rs                 # Copy/extract static assets

crates/rbuilder-wasm/           # WASM engine (Phase 1+)
  src/lib.rs                    # Columnar v2 header parse, future graph ops

.rbuilder/dashboard/            # Runtime output (per repo, gitignored)
  index.html
  assets/                       # Hashed JS/CSS from Vite
  manifest.json                 # Counts, paths, phase flags
  graph_payload.bin             # Copy of graph.snapshot.bin (columnar v2)
  wasm/                         # rbuilder_engine.wasm + JS glue
```

---

## Phase map (with mandatory removal)

| Phase | Build | Remove |
|-------|-------|--------|
| **0** | Preact shell, bundle export, manifest | `crates/rbuilder-export/src/html.rs`, CDN D3 dashboard |
| **1** | `graph_payload.bin`, WASM header parse, worker boot | JSON-in-HTML graph embed (already gone with html.rs) |
| **2** | Sigma.js metanode view @ 50k+ | Placeholder graph renderer |
| **3** | LOD drill-down, bitmask filters, function table | Placeholder function tab text |
| **4** | CFG/dominance from `cfg_pdg.archive.bin` | ~~`all_analyses.json`~~ (removed) |
| **5** | Slice + CodeMirror | Client `computeBackwardSlice` |
| **6** | Blast radius + depth slider in WASM | Client blast BFS |
| **7** | Taint from archive | Embedded taint JSON |
| **8** | Decommission audit | Any remaining legacy grep hits |

---

## Phase 0 — Shell + bundle export

### What it does

1. `discover` writes `.rbuilder/dashboard/` instead of monolithic `dashboard.html`.
2. UI: responsive tab bar matching legacy areas (graph, functions, CFG, slice, blast, guide).
3. Stat cards read **`manifest.json`** (no inline graph JSON).
4. Static assets bundled at **compile time** via `include_dir` from `dashboard/dist/`.

### How to build the UI

```bash
cd dashboard && npm ci && npm run build
cargo build --release   # rbuilder-dashboard embeds dashboard/dist
```

### How to open after discover

```bash
rbuilder discover .
# Option A — local static server (recommended for WASM fetch)
cd .rbuilder/dashboard && python3 -m http.server 8765
# Option B — open index.html directly (manifest injected at export; WASM fetch may need server)
open http://localhost:8765
```

### Lookup

| Question | Where |
|----------|-------|
| Export entrypoint | `rbuilder_dashboard::export_dashboard_bundle` |
| Discover hook | `src/cli/discover_impl.rs` |
| Manifest schema | `crates/rbuilder-dashboard/src/manifest.rs` |
| Embedded UI files | `crates/rbuilder-dashboard/src/bundle.rs` |

---

## Phase 1 — Binary payload + WASM loader

### What it does

1. **`graph_payload.bin`** — byte copy of `.rbuilder/graph.snapshot.bin` (columnar v2).
2. **`manifest.json`** — `payload_format: "columnar_v2"`, node/edge counts, digest.
3. **Web worker** fetches payload + instantiates WASM.
4. **WASM** parses columnar header (magic, version, counts) — no full graph hydrate.

### Worker message protocol (v1)

| Direction | Message | Payload |
|-----------|---------|---------|
| UI → worker | `{ type: "init" }` | — |
| worker → UI | `{ type: "ready", nodeCount, edgeCount, schemaVersion, wasm: true }` | — |
| worker → UI | `{ type: "error", message }` | — |

Future phases add `filter`, `blast_radius`, `compute_slice`, etc.

### Columnar v2 header (first 136 bytes)

See `crates/rbuilder-graph/src/columnar_snapshot.rs`:

- `[0..4]` magic `RBGR`
- `[4..8]` format version `2`
- `[8..12]` schema version
- `[12..20]` node count (u64 LE)
- `[20..28]` edge count (u64 LE)
- `[28..92]` digest (64-byte UTF-8, null padded)

WASM duplicates minimal parse in `crates/rbuilder-wasm/src/lib.rs` (no full graph crate in WASM yet).

### Lookup

| Question | Where |
|----------|-------|
| WASM API | `EngineContext::from_bytes` in `crates/rbuilder-wasm` |
| Build WASM | `scripts/build-dashboard.sh` or `wasm-pack build` in `crates/rbuilder-wasm` |
| Worker | `dashboard/src/worker.ts` |

---

## manifest.json (schema v1)

```json
{
  "schema_version": 1,
  "dashboard_version": "0.1.0",
  "phases": { "0": "complete", "1": "complete" },
  "graph": {
    "payload_path": "graph_payload.bin",
    "payload_format": "columnar_v2",
    "node_count": 798,
    "edge_count": 1506,
    "digest": "sha256:..."
  },
  "metrics": {
    "function_count": 120,
    "class_count": 45,
    "calls_count": 890,
    "avg_complexity": 2.4,
    "high_blast_radius_count": 3
  },
  "generated_at": "2026-07-03T20:00:00Z"
}
```

---

## Sanity checks

**Golden repo (run after every phase):** `/Users/sshaaf/git/java/gbuilder`  
Override: `RBUILDER_DASHBOARD_GOLDEN_REPO=/path/to/repo`

```bash
# One-shot phase gate (build UI + WASM + both tests)
./scripts/test-dashboard-golden.sh

# Or manually:
./scripts/build-dashboard.sh
cargo build --release
cargo test --release --test dashboard_bundle --test dashboard_gbuilder -- --nocapture

# Preview gbuilder dashboard
cd /Users/sshaaf/git/java/gbuilder/.rbuilder/dashboard && python3 -m http.server 8765
```

### Test targets

| Test | Repo | Purpose |
|------|------|---------|
| `dashboard_bundle` | `tests/fixtures/tiny_polyglot_repo` (temp copy) | Fast CI / minimal graph |
| `dashboard_gbuilder` | `/Users/sshaaf/git/java/gbuilder` | Real Java graph (~2k nodes) |

Shared assertions: `tests/dashboard_harness.rs` → `assert_dashboard_bundle_with_meta()`.

### Phase 2 artifacts

| File | Role |
|------|------|
| `metagraph.json` | Package-level metanodes + aggregated call edges |
| `manifest.view` | Metagraph path, counts, `mode`, `community_only` flag |

UI loads `./metagraph.json` in the Graph tab (Sigma.js). At ≥50k source nodes, `community_only` is set; Phase 3 enables WASM drill-down into package members via `member_indices`.

### Phase 3 — LOD + filters

| Feature | Where |
|---------|-------|
| `member_indices` on metanodes | `metagraph.json` schema v2 |
| WASM columnar expand / list | `EngineContext::expandIndices`, `listNodes` |
| Worker messages | `expand`, `list_nodes` in `dashboard/src/worker.ts` |
| Graph drill-down | Double-click metanode or **Drill down** in inspector |
| Type bitmask filter | `NodeTypeFilter` — Function, Class, Struct, … |
| Function table | `FunctionsView` — virtual scroll via WASM pagination |

Worker protocol (v2):

| Direction | Message | Payload |
|-----------|---------|---------|
| UI → worker | `{ type: "expand", indices, typeMask }` | columnar row indices |
| worker → UI | `{ type: "subgraph", payload }` | nodes + internal call edges |
| UI → worker | `{ type: "list_nodes", typeMask, offset, limit }` | paginated scan |
| worker → UI | `{ type: "node_list", payload }` | `{ total, offset, items }` |

---

## Implementation status

_Update this table when a phase lands._

| Component | Phase | Status | Notes |
|-----------|-------|--------|-------|
| `docs/dashboard-design.md` | 0 | **done** | This document |
| `dashboard/` Preact shell | 0 | **done** | Tabs + stat cards from manifest |
| `rbuilder-dashboard` crate | 0 | **done** | `export_dashboard_bundle`, embed dist |
| Discover → bundle (not html) | 0 | **done** | Replaces `export_html_dashboard` |
| Delete `html.rs` | 0 | **done** | Removed from rbuilder-export |
| `graph_payload.bin` copy | 1 | **done** | From `graph.snapshot.bin` |
| `rbuilder-wasm` header parse | 1 | **done** | Counts from columnar v2 |
| Worker + WASM boot in UI | 1 | **done** | Status bar shows engine stats |
| `tests/dashboard_bundle.rs` | 0+1 | **done** | Tiny fixture subprocess |
| `tests/dashboard_gbuilder.rs` | 0+1 | **done** | **gbuilder** golden repo gate |
| `scripts/test-dashboard-golden.sh` | 0+1 | **done** | Phase gate script |
| Sigma.js graph | 2 | **done** | `GraphView.tsx` — package metagraph WebGL |
| Community metanodes | 2 | **done** | `metagraph.json` export + inspector |
| `tests/dashboard_harness.rs` | 2 | **done** | Asserts `metagraph.json`, `manifest.view`, phase 2 |
| WASM columnar LOD | 3 | **done** | `expandIndices` / `listNodes` |
| Graph drill-down | 3 | **done** | Sigma subgraph + breadcrumb |
| Bitmask type filters | 3 | **done** | Graph + Functions tabs |
| Functions virtual table | 3 | **done** | WASM paginated list |
| `tests/dashboard_harness.rs` | 3 | **done** | phase 3 + `member_indices` |
| Bootstrap UI restore | 3+ | **done** | Full-height tabs, worker URL fix |
| CFG index + detail export | 4 | **done** | `cfg_index.json`, `cfg/*.json`, archive copy |
| CFG / dominance tab | 4 | **done** | `CfgView.tsx` Sigma CFG + idom table |
| Remove `all_analyses.json` | 4 | **done** | Discover no longer writes consolidated JSON |
| Slice index + PDG export | 5 | **done** | `slice_index.json`, `slice/*.json` with source + PDG |
| CodeMirror slice tab | 5 | **done** | `SliceView.tsx`, worker `compute_slice` on exported PDG |
| `tests/dashboard_harness.rs` | 5 | **done** | phase 5 + `slice_index.json` |
| WASM `blastRadius` API | 6 | **done** | Reverse call-graph BFS with depth limit |
| Blast radius tab | 6 | **done** | `BlastView.tsx` + depth slider |
| `blast_index.json` export | 6 | **done** | Optional snapshot copy |
| `tests/dashboard_harness.rs` | 6 | **done** | phase 6 + `blast_index.json` |

### Removed (Phase 0)

- `crates/rbuilder-export/src/html.rs` (~2700 lines)
- `export_html_dashboard` public API
- Monolithic `.rbuilder/dashboard.html` default output

### Removed (Phase 4)

- `all_analyses.json` writer in discover (`discover --cfg` still writes per-function storage + archive)

### Not yet removed (later phases)

- Discover-time blast radius string properties on nodes (Phase 6, dashboard only)

---

## PR checklist (every phase)

- [ ] **Build** — new behavior documented in this file
- [ ] **Remove** — deleted code listed in Implementation status
- [ ] **Sanity** — `./scripts/test-dashboard-golden.sh` passes (gbuilder + tiny fixture)
- [ ] **Docs** — cli-getting-started path updated if user-visible
