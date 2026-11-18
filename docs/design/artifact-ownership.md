# Artifact Ownership & Consumers — Engineering Design

This document maps every persistent `.rgctl/` artifact to its **producer**, **owner**, and **readers**: the CLI, `rgctl serve`/HTTP APIs, and the v2 dashboard bundle.

The goal is to answer one question before changing a file:

> If I move, delete, compress, or serve this artifact differently, who breaks?

The short answer is: **canonical analysis artifacts belong to the CLI and server; the dashboard bundle is a derived transport/cache and must never become the source of truth.**

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. CANONICAL ANALYSIS PLANE — `.rgctl/`                                     │
│                                                                              │
│  discover / analysis writes:                                                 │
│    graph.snapshot.bin          analysis_results.bin                          │
│    analysis/analysis.pack*     analysis/cfg_pdg.archive.bin                 │
│    blast_engine.snapshot.bin   macro_call_index.{bin,db}                    │
│    semantic_index.bin          content_store.bin                            │
│    field_write.index.bin       parked_relations.bin                         │
│    file_hashes.json            callable_resolution.json                     │
│    annotation_args.json        workflows/review.json                        │
│                                                                              │
│  Owner: extraction / analysis / graph crates                                 │
│  Primary readers: `rgctl` CLI, `rgctl serve`                                 │
└───────────────────────────────┬──────────────────────────────────────────────┘
                                │
        ┌───────────────────────┴────────────────────────┐
        │                                                │
        ▼                                                ▼
┌───────────────────────────────┐        ┌─────────────────────────────────────┐
│ 2. CLI QUERY PLANE            │        │ 3. SERVE / API PLANE                │
│                               │        │                                     │
│ find / callers / callees      │        │ graph.snapshot → /api/query         │
│ relations / inventory         │        │ graph.snapshot → /api/functions     │
│ blast-radius / inspect        │        │ semantic_index → /api/semantic/*    │
│ slice / cpg / taint / vuln    │        │ analysis_results → semantic context │
│ semantic / review / export    │        │                                     │
│ merge / writeback / diff      │        │ No dashboard bundle is canonical.   │
└───────────────────────────────┘        └──────────────────┬──────────────────┘
                                                            │
                                                            ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 4. DASHBOARD BUNDLE PLANE — `.rgctl/dashboard/`                             │
│                                                                              │
│  Built by `crates/rgctl-dashboard` from canonical artifacts.                 │
│  Served as static files; embedded shell comes from `dashboard/dist-v2/`.     │
│                                                                              │
│  presentation: manifest, metagraph, communities, migration_graph            │
│  browser WASM: graph_payload.bin, metanode_members.bin                      │
│  CFG: cfg_meta, cfg_index (fallback), cfg/<id>, cfg record packs            │
│  PDG: slice_meta, slice_index (fallback), slice/<id>, slice record packs    │
│  Taint: taint_index, taint/<id>, dataflow_index, blast_index                │
│  Source: sources_index, sources/<id>.txt                                    │
│  Unused/legacy: mutations_index, migration_plan, dashboard cfg archive copy │
│                                                                              │
│  Owner: `rgctl-dashboard` exporter. Browser reads it; CLI writes/derives it. │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Ownership rule

| Plane | Owner | Source of truth? | Safe to delete? |
|---|---|---|---|
| `.rgctl/` canonical artifacts | extraction / analysis | **Yes** | No — CLI breaks |
| `.rgctl/dashboard/` bundle | `rgctl-dashboard` | No — derived | Yes, if re-exported |
| HTTP API responses | `serve` | No — computed | Yes |
| `dashboard/dist-v2/` | dashboard build | No — UI shell | Rebuild |

---

## 2. Ownership & Invalidation Model

This section replaces the usual “theoretical foundations” section: artifact ownership is an engineering contract, not an algorithm.

### 2.1 Ownership classes

| Class | Examples | Rule |
|---|---|---|
| **Canonical graph** | `graph.snapshot.bin` | Mmap-owned by the CLI/server. Never copied into the browser except as a derived `graph_payload.bin`. |
| **Canonical analysis** | `analysis_results.bin`, `analysis.pack*`, `cfg_pdg.archive.bin`, `field_write.index.bin` | Persisted by analysis; read by CLI. Dashboard may export derived copies, but must not replace the analysis artifact. |
| **Canonical lookup/cache** | `semantic_index.bin`, `blast_engine.snapshot.bin`, `macro_call_index.*` | Derived, digest-bound caches. CLI may rebuild; dashboard should use them through `serve` rather than reimplementing. |
| **Source/history** | `content_store.bin`, `cst.archive.bin`, `file_hashes.json` | Source or incremental state. Not presentation data. |
| **Cross-repo diagnostics** | `parked_relations.bin`, `callable_resolution.json` | Extraction diagnostics used by merge/vuln/review. API summaries are fine; full binary is server-side. |
| **Dashboard presentation** | `metagraph.json`, `communities.json`, `migration_graph.json` | Small enough to ship; derived from canonical analysis. |
| **Dashboard transport** | `cfg_meta`, `slice_meta`, record packs, `sources_index` | Browser-facing exports of canonical data. Prefer API/Range over full JSON. |
| **Dead/legacy bundle output** | `mutations_index.json`, `migration_plan.json`, dashboard `cfg_pdg.archive.bin` | Verify consumers before removal; no current v2 reader for some. |

### 2.2 Invalidation keys

| Key | Binds |
|---|---|
| `graph.digest` | `graph.snapshot`, `graph_payload`, metagraph, communities, CFG/PDG record packs, taint/DFG/blast indices |
| `export_fingerprint` | Dashboard re-export reuse |
| `content_digest` | Blast engine snapshot / graph-bound caches |
| `semantic` model + index metadata | `semantic_index.bin` |
| file hashes | Incremental `discover`/`update` |
| Git/workflow state | `workflows/review.json` |

A dashboard artifact whose digest does not match `manifest.graph.digest` is stale and must be reported as absent/stale, not rendered as current.

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Canonical `.rgctl/` artifacts

| Artifact | Approx size (metasfresh) | Producer | CLI readers | Serve/API readers | Browser readers |
|---|---:|---|---|---|---|
| `graph.snapshot.bin` | 538 MB | `discover` | **All query commands**: `find`, `callers`, `callees`, `relations`, `inventory`, `blast-radius`, `inspect`, `slice`, `cpg`, `review`, `semantic`, `vuln` | `/api/query`, `/api/functions` | — |
| `analysis_results.bin` | 129 MB | analysis | `communities`, `blast-radius`, `semantic`, `vuln`, scoped policy | semantic/community context | — |
| `analysis/analysis.pack.bin` + `analysis.pack_index.bin` | 1.9 GB + 53 MB | `discover --with-cfg` | `AnalysisStorage` consumers (`vuln` analysis; CFG persistence) | — | — |
| `analysis/cfg_pdg.archive.bin` | 1.9 GB | `discover --with-cfg` | `cpg inspect/pdg/flows/slice`, field writes, clones, interprocedural slicing | — | — |
| `blast_engine.snapshot.bin` | 101 MB | analysis | `blast-radius`, `semantic`, `vuln` | future impact API | — |
| `semantic_index.bin` | 137 MB | `semantic index` | `semantic`, `serve` | semantic API | — |
| `content_store.bin` | 120 MB | `discover --lossless` | `export` (Obsidian/OKF), `writeback`, semantic source resolution | future source API | — |
| `field_write.index.bin` | 29 MB | `discover --with-cfg` | `cpg mutations`, `cpg status` | future mutations API | — |
| `parked_relations.bin` | 370 MB | `discover` | `merge`, `vuln deps` | future external-edge API | — |
| `file_hashes.json` | 61 MB | `discover` | `discover`/`update`, `status` | — | — |
| `callable_resolution.json` | 0.8 MB | extraction | discover/pipeline freshness | — | — |
| `annotation_args.json` | 6.7 MB | extraction | structured `find --annotation` | — | — |
| `macro_call_index.bin` / `.db` | MB | analysis | `blast-radius`, service impact | — | — |
| `workflows/review.json` | varies | `review export` | review/PR tooling | future Review API | — |
| `deltas/`, `cst.archive.bin` | varies | incremental/lossless | diff/writeback | — | — |

### 3.2 Dashboard bundle `.rgctl/dashboard/`

| Artifact | Approx size | Producer | CLI reads? | Browser reads? | Notes |
|---|---:|---|---|---|---|
| `manifest.json` | 2.5 KB | dashboard exporter | `serve`/bundle selection | shell/digest | Inline copy also injected into `index.html` |
| `graph_payload.bin` | 538 MB | exporter copy of snapshot | — | WASM Architecture/Blast | CLI uses `graph.snapshot.bin` instead |
| `metagraph.json` | 377 KB | `metagraph.rs` | — | Architecture | |
| `communities.json` | 6.8 KB | `communities.rs` | — | Architecture | |
| `metanode_members.bin` | 1.7 MB | `metagraph.rs` | — | Architecture | Membership sidecar |
| `migration_graph.json` | 197 KB | `migration_export.rs` | — | Architecture/Migration | |
| `migration_plan.json` | 62 KB historical | `migration_export.rs` | pipeline/status only | — | **Not exported for v2**: plan is computed locally |
| `cfg_meta.json` | 219 B | `cfg_export.rs` | — | CFG worker | Preferred over index |
| `cfg_index.json` | 67 MB historical | CFG export | — | fallback header only | V2 writes no function array; static symbol fallback is unavailable |
| `cfg/<id>.json` | per function | CFG export | — | CFG per-file | Not used in archive-only bundles |
| `cfg_pdg.record_index.bin` | 6.4 MB | CFG export | — | CFG worker | Range index |
| `cfg_pdg.record_data.bin` | 265 MB | CFG export | — | CFG worker | Range data |
| `cfg_pdg.archive.bin` (bundle) | 1.9 GB | exporter copy | — | — | **No longer copied**: v2 uses record packs; CLI uses `.rgctl/analysis/cfg_pdg.archive.bin` |
| `slice_meta.json` | 178 B | slice export | — | PDG worker | Preferred over index |
| `slice_index.json` | 66 MB historical | slice export | — | fallback header only | V2 writes no function array |
| `slice/<id>.json` | per function | slice export | — | PDG per-file | |
| `slice.record_index.bin` | 6.4 MB | slice export | — | PDG worker | Range index |
| `slice.record_data.bin` | 1.25 GB | slice export | — | PDG worker | Range data |
| `taint_index.json` | 142 KB | taint export | pipeline skip check | Taint | |
| `taint/<id>.json` | per function | taint export | — | Taint | |
| `dataflow_index.json` | 69 MB historical | dataflow export | — | — | **Not exported for v2**: Taint counts come from PDG detail |
| `blast_index.json` | 82 B | blast export | — | Taint | Scores only |
| `sources_index.json` | 5.5 MB | source catalog | — | coverage/source pane | |
| `sources/<id>.txt` | source files | source catalog | — | source pane | |
| `mutations_index.json` | 24 MB historical | mutations export | — | — | **Not exported for v2**: no mutations view |
| `function_index.json` | varies | light-bundle export | — | fallback | Only when no CFG/slice indexes |
| `assets/`, `index.html` | UI build | dashboard build | `serve` static | shell | No data semantics |

### 3.3 HTTP API JSON

| Endpoint | Backing artifact | v2 consumer |
|---|---|---|
| `GET /api/functions` | `graph.snapshot.bin` | `useFocus`, Palette search, Focus resolution |
| `GET /api/semantic/status` | `semantic_index.bin` metadata | Ask availability |
| `POST /api/semantic/query` | `semantic_index.bin` + `analysis_results.bin` | Ask retrieval |
| `POST /api/query` | `graph.snapshot.bin` | not used by v2; CLI/service consumers |
| `POST /api/cpg` | documented in `dashboard-v2.md` | **not routed in `http_serve.rs`** — documentation drift |

---

## 4. CLI Contract & Implementation Map

### 4.1 `rgctl serve` is special

`serve` is a CLI command, but it is not an analysis consumer. It serves the dashboard bundle and API. It may read:

- `manifest.json` to choose/refresh the static shell
- dashboard files to serve them
- `graph.snapshot.bin` for graph APIs
- `semantic_index.bin` / `analysis_results.bin` for semantic APIs

It does **not** make the dashboard bundle canonical.

### 4.2 Same-name traps

| Name | Canonical path | Dashboard path | Who reads which |
|---|---|---|---|
| `graph_payload.bin` | — (copy of `graph.snapshot.bin`) | `.rgctl/dashboard/graph_payload.bin` | Browser WASM only; CLI uses snapshot |
| `cfg_pdg.archive.bin` | `.rgctl/analysis/cfg_pdg.archive.bin` | `.rgctl/dashboard/cfg_pdg.archive.bin` | CLI uses analysis copy; v2 uses neither (record packs) |
| `analysis_results.bin` | `.rgctl/analysis_results.bin` | — | CLI/serve only |
| `cfg_index.json` | — | `.rgctl/dashboard/cfg_index.json` | Browser fallback only; CLI does not read |
| `slice_index.json` | — | `.rgctl/dashboard/slice_index.json` | Browser fallback only |
| `field_write.index.bin` | `.rgctl/analysis/field_write.index.bin` | `mutations_index.json` is derived | CLI reads binary; browser has no current view |
| `content_store.bin` | `.rgctl/content_store.bin` | `sources_index.json` + `sources/` are derived | CLI reads store; browser reads source files |
| `taint_index.json` | — | `.rgctl/dashboard/taint_index.json` | Browser Taint; pipeline only checks existence |
| `migration_plan.json` | `.rgctl/migration_plan.json` | `.rgctl/dashboard/migration_plan.json` | CLI writes; v2 does not read |

### 4.3 Implementation map

| Component | Source |
|---|---|
| Bundle export orchestration | `crates/rgctl-dashboard/src/lib.rs` |
| Manifest / paths | `crates/rgctl-dashboard/src/manifest.rs` |
| Graph payload / static shell | `crates/rgctl-dashboard/src/bundle.rs` |
| CFG / slice export | `crates/rgctl-dashboard/src/cfg_export.rs`, `slice_export.rs`, `analysis_stream_export.rs` |
| Taint / dataflow / blast | `taint_export.rs`, `dataflow_export.rs`, `blast_export.rs` |
| Source catalog | `source_catalog.rs` |
| HTTP server / APIs | `src/cli/http_serve.rs` |
| v2 data loaders | `dashboard/src/v2/data.ts`, `data/loadTaint.ts`, `cfgArchive.ts` |
| v2 workers | `dashboard/src/v2/expandEngine.ts`, `blastWorker.ts` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Do not make the browser bundle canonical.** If the CLI needs it, it belongs under `.rgctl/`, not `.rgctl/dashboard/`.
2. **Do not copy a canonical archive into the dashboard unless a browser reader uses it.** The dashboard `cfg_pdg.archive.bin` copy is the prime example.
3. **Do not ship a large JSON if an API/Range read can answer the question.**
4. **Do not use dashboard `cfg_index.json` / `slice_index.json` as CLI inputs.** They are browser fallbacks.
5. **Keep `graph_payload.bin` a copy of the snapshot, never the reverse.**
6. **Bind every derived artifact to `graph.digest`; reject stale data explicitly.**
7. **When removing a bundle file, prove no v1/v2/browser reader remains and no `serve` path checks it.**
8. **When removing a canonical file, assume every CLI command breaks until proven otherwise.**

Verification commands before an artifact change:

```bash
# CLI still works
rgctl -f json find --type function --limit 5
rgctl -f json blast-radius <symbol>
rgctl -f json cpg status
rgctl -f json cpg mutations --type <Type>
rgctl semantic query "checkout"

# Dashboard still works
rgctl serve --open
# verify: /architecture, ⌘K, Understand CFG/PDG/Blast, Taint
```

Automated checks to keep in place:

- v2 tests (`npm run test:v2`)
- dashboard / bundle Rust tests (`cargo test -p rgctl-dashboard`)
- `preflight-pr.sh --quick`
- a future artifact-consumer lint that scans for hardcoded dashboard filenames in CLI code and canonical filenames in browser code

---

## 6. Known Limitations & By-Design Boundaries

1. **The dashboard bundle is a projection, not a database.** It may omit data the CLI has.
2. **Static hosting is a second-class path.** The API/Range path is the fast path; static fallbacks still exist for `cfg_index` / `slice_index` / `function_index`.
3. **V2 cleanups are landing.** The dashboard `cfg_pdg.archive.bin` copy, `dataflow_index.json`, `mutations_index.json`, and `migration_plan.json` are no longer written for v2. `cfg_index.json` / `slice_index.json` keep only their headers for v2; function search requires `/api/functions`. The v1 bundle retains its existing behavior.
4. **Some CLI paths are library-only.** `analysis.pack*` is not the primary `cpg`/`slice` path; those read `analysis/cfg_pdg.archive.bin`.
5. **Manifest path fields are not authoritative in the browser.** Much of the v2 client still hardcodes filenames (`cfg_meta.json`, `slice_meta.json`, etc.), which is why the artifact map and the code can drift.
6. **`/api/cpg` is documented but not routed.** `dashboard-v2.md` lists it, but `http_serve.rs` routes only `/api/query`, `/api/functions`, and `/api/semantic/*`.
7. **Key schemes are inconsistent.** Metrics are keyed by columnar index; Focus/taint/details use `function_id`; source uses file path. Any cleanup must preserve or explicitly translate these keys.
8. **A rename breaks Focus.** `function_id` is deterministic from path + name + type; a renamed symbol produces a new UUID. This is a UX boundary, not an artifact bug.
