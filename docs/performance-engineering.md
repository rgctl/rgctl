# Performance Engineering — Blast Radius & Graph Caches

This document captures blast-radius **query latency tiers**, the **benchmark landscape**, optimization roadmap, and how performance work relates to the CLI I/O contract.

**Companion doc:** [cli-io-sanity-qe.md](cli-io-sanity-qe.md) — JSON schemas, exit codes, and subprocess correctness (orthogonal to wall-clock gates).

Last updated: 2026-07-03 (T3 complete: ICFG from CFG archive + br.slice.total_ms gate).

---

## Executive summary

| Track | Status | What it proves |
|-------|--------|----------------|
| **CLI I/O contract** | ✅ Strong | `cli_output`, `subprocess_golden_path`, `all_commands_sanity` — CI in [blast-radius-perf.yml](../.github/workflows/blast-radius-perf.yml) |
| **Blast-radius perf infra** | ✅ Landed | `blast_radius_perf` (8 CI gates + 3 ignored 150k) |
| **Production latency (T0–T3)** | ⚠️ Partial | metasfresh T1 ~200 ms post-Sprint A; soft gates when cache present |

**Bottom line:** We can regress **algorithm + cache micro-paths** at mock scale and **SQLite lookup** on synthetic rows. We **cannot yet** automatically regress end-to-end CLI latency on a real monorepo cache.

---

## Current latency budget (metasfresh reference)

Reference repo: `example/metasfresh-4.9.8b` (~128k functions, ~700k edges).

| Tier | Path | Typical latency | Dominant cost | Automated gate? |
|------|------|-----------------|---------------|-----------------|
| **T0** | SQLite macro index (`macro_call_index.db`) | ~280 ms | DB open + bincode BLOB read (Sprint A) | Micro + subprocess (`br.query.fast_path_ms`) |
| **T1** | Lite path (mmap graph + engine v2 snapshot) | ~200 ms (post-Sprint A; was ~5.8 s) | Lazy `ReachabilityStore` load + single-row expand at analyze | Soft gate when metasfresh cache present |
| **T2** | Full graph hydrate (`MemoryBackend`) | ~26 s+ | Legacy JSON / full backend rebuild | No |
| **T3** | `--with-slices` | seconds (tiny + `--cfg` archive); minutes without | ICFG from archive + PDG preload; ICFG still rebuilt if no archive | ✅ subprocess `br.slice.total_ms` (tiny fixture) |

These T0–T3 numbers are **manual** (`/usr/bin/time`, ignored rebuild test). They are not enforced in CI.

### Query-tier routing (CLI)

`src/cli/blast_radius.rs` dispatches in order:

1. **T0 fast path** — `try_fast_cached_lookup()` → `MacroCallLookupDb::lookup_resolved()` + `SnapshotNodeStore` (no full hydrate when cache hits).
2. **T1 lite path** — mmap snapshot + pre-built `BlastEngineSnapshot` + `try_load_engine()`.
3. **T2 full path** — `ctx.load_graph()` → `MemoryBackend` + live `BlastRadiusEngine::build()`.
4. **T3 slices** — full graph + `resolve_handoff_seeds()` (handoffs in JSON even when ICFG/PDG trace fails) + optional `trace_blast_to_slices_with_blast()`.

Policy evaluation and `--with-slices` force T2+ (full graph required today).

---

## Benchmark landscape

### What exists today

| Asset | Location | Scale | Gate | Runs in CI? |
|-------|----------|-------|------|-------------|
| **Blast analyze (warm)** | `tests/blast_radius_perf.rs` | 5k / 25k edges mock | **< 1 ms** | Yes (`cargo test --release --test blast_radius_perf`) |
| **SQLite unique lookup** | `blast_radius_perf` | Synthetic row (100 callers, 500 impact names) | **< 15 ms** | Yes (release test) |
| **SQLite FQN resolved** | `blast_radius_perf` | Single candidate row | **< 50 ms** | Yes (release test) |
| **PetGraph from prepared** | `blast_radius_perf` + `blast_radius_benchmarks` | 150k / 700k mock | **< 30 s** | No (`#[ignore]`, needs `--ignored`) |
| **PetGraph from columnar store** | `blast_radius_perf` | 5k mock | **< 500 ms** | Yes (release test) |
| **Columnar open vs v1** | `blast_radius_perf` | 5k mock | v2 faster than v1 bincode | Yes (release test) |
| **SnapshotNodeStore open** | `blast_radius_perf` + `blast_radius_benchmarks` | 150k mock | **< 15 s** | No (`#[ignore]`) |
| **Engine snapshot load** | `blast_radius_perf` + `blast_radius_benchmarks` | 150k mock | **< 60 s** | No (`#[ignore]`) |
| **Blast analyze (small)** | `benches/blast_radius_benchmarks.rs` | 5k mock | **< 5 ms** (bench assert) | No |
| PageRank | `centrality_benchmarks` + `centrality_audit` | 150k / 700k mock | **< 20 ms** | No (ignored / bench) |
| Community | `community_benchmarks` + `community_audit` | 150k / 700k mock | **< 150 ms** | No (ignored / bench) |
| PetGraphView (generic) | `benches/graph_benchmarks.rs` | 5k–25k; 150k/1M with `RBUILDER_BENCH_LARGE=1` | Criterion only | No |
| Blast engine (small) | `graph_benchmarks` | ≤1000 nodes | Criterion only | No |
| Dominance | `benches/analysis_benchmarks.rs` | 1000-block CFG | **< 15 ms** | Yes (`scripts/semantic-verification.sh`) |
| Semantic smoke | `tests/analysis_perf.rs` | Small fixtures | < 2–5 s generous | Yes |
| Blast correctness | `tests/blast_radius.rs` | Small chains | Functional | Yes |
| **CLI I/O sanity** | `tests/cli_output/all_commands_sanity.rs` | Tiny polyglot fixture | Schema / exit code | Yes |
| Cache rebuild | `tests/rebuild_macro_index.rs` | metasfresh (ignored) | `eprintln!` only | No |
| Full repo indexing | `benches/full_analysis.rs` | kafka example | Criterion | **Orphan** (not in `Cargo.toml`) |

### Standard benchmark suite (implemented)

| Component | Path | Status |
|-----------|------|--------|
| Criterion benches | `benches/blast_radius_benchmarks.rs` | ✅ Wired in `Cargo.toml` |
| Release gate tests | `tests/blast_radius_perf.rs` | ✅ CI gates + ignored 150k gates |

### Fixture tiers

| ID | Purpose | Source |
|----|---------|--------|
| **S** | Algorithm smoke | 5k-node mock in blast_radius_perf / blast_radius_benchmarks |
| **M** | Scale gates (centrality/community parity) | `build_monorepo_mock(150_000, 700_000)` |
| **R** | Realistic Java monorepo | `example/metasfresh-4.9.8b/.rbuilder/*` if present |
| **R'** | Polyglot smoke | `example/kafka` if present |
| **T** | CLI contract (not perf) | `tests/fixtures/tiny_polyglot_repo` |

Environment variables:

```bash
RBUILDER_BENCH_LARGE=1              # enable 150k mock Criterion groups
RBUILDER_BENCH_REPO=/path/to/repo     # real-repo soft gates in blast_radius_perf (skips if cache missing)
RBUILDER_BENCH_SYMBOL=saveError       # optional symbol for bench_repo_lite_analyze_under_3s
```

### Pattern that works

Centrality, community, and blast-radius now share:

1. Synthetic monorepo mock at 150k nodes / 700k edges.
2. Criterion bench for trend tracking.
3. Ignored integration test with hard wall-time asserts (`cargo test --release --test … -- --ignored`).
4. Optional real-repo test when checkout exists (metasfresh: manual only today).

---

## Metric registry

Each metric should eventually have Criterion samples **and** a release gate where marked.

### Query path (user-facing)

| Metric ID | Description | Target (metasfresh) | Gate status |
|-----------|-------------|---------------------|-------------|
| `br.query.analyze_ms` | Warm `BlastRadiusEngine::analyze` | < 1 ms (5k mock) | ✅ `blast_radius_perf` |
| `br.query.sqlite_unique_ms` | `MacroCallLookupDb::lookup()` | < 15 ms | ✅ `blast_radius_perf` (synthetic DB) |
| `br.query.sqlite_fqn_ms` | `lookup_resolved()` with class filter | < 50 ms | ✅ `blast_radius_perf` (synthetic DB) |
| `br.query.fast_path_ms` | `try_fast_cached_lookup` end-to-end CLI | < 150 ms | ✅ `subprocess_golden_path` |
| `br.query.lite_total_ms` | Full T1 CLI (snapshot + engine, no SQLite hit) | < 3000 ms | ✅ soft `blast_radius_perf` when metasfresh cache present |
| `br.query.full_hydrate_ms` | T2 `load_graph` + analyze | < 15000 ms | ❌ Soft gate only (manual) |

### Load / deserialize (implementation-facing)

| Metric ID | Description | Gate (150k mock) | Gate status |
|-----------|-------------|------------------|-------------|
| `br.load.petgraph_from_prepared_ms` | `PetGraphView::from_prepared` | < 30 s | ✅ ignored `blast_radius_perf` |
| `br.load.petgraph_from_snapshot_store_ms` | `PetGraphView::from_snapshot_store` (columnar v2) | < 500 ms (5k mock) | ✅ `blast_radius_perf` |
| `br.load.columnar_open_ms` | Columnar v2 open vs v1 bincode | v2 faster than v1 (5k mock) | ✅ `blast_radius_perf` |
| `br.load.snapshot_node_store_ms` | `SnapshotNodeStore::open` | < 15 s | ✅ ignored `blast_radius_perf` |
| `br.load.graph_snapshot_ms` | `MmappedGraphSnapshot::open` | < 15 s | ✅ `blast_radius_benchmarks` assert |
| `br.load.engine_snapshot_ms` | Load + `from_engine_snapshot` | < 5 s (150k mock + metasfresh soft) | ✅ `blast_radius_perf` |
| `br.load.engine_snapshot_rss_mb` | RSS delta after engine load | < 512 MB (5k mock) | ✅ `blast_radius_perf` |
| `br.load.backend_hydrate_ms` | `hydrate_prepared` vs batch re-index | hydrate ≤ batch | ✅ `blast_radius_perf` (5k mock) |

### Discover / write path

| Metric ID | Description | Gate status |
|-----------|-------------|-------------|
| `br.discover.engine_build_ms` | `BlastRadiusEngine::build` | ❌ Manual only |
| `br.discover.analyze_all_ms` | Parallel loop over all functions at discover | < 2 s (5k mock) | ✅ `blast_radius_perf` |
| `br.discover.snapshot_write_ms` | `PreparedGraphSnapshot::write_to_path` | ❌ |
| `br.discover.engine_snapshot_write_ms` | v2 sparse+zstd write | ❌ |
| `br.discover.macro_index_write_ms` | SQLite + macro index | ❌ |
| Discover telemetry | `discover -f json` → `metrics.duration_ms` | ✅ I/O contract only ([cli-io-sanity-qe](cli-io-sanity-qe.md)) |

### Slice path (`--with-slices`)

| Metric ID | Description | Target | Gate status |
|-----------|-------------|--------|-------------|
| `br.slice.icfg_build_ms` | `InterproceduralCFG::build` | — | Skipped when archive present |
| `br.slice.per_seed_pdg_ms` | PDG per handoff seed | — | Preloaded from archive when present |
| `br.slice.total_ms` | `blast-radius --with-slices` CLI | < 30 s (tiny + `--cfg`) | ✅ `subprocess_golden_path` |
| CFG/PDG archive | `discover --cfg` → `cfg_pdg.archive.bin`; ICFG + PDG on slice | — | ✅ Done |
| Handoffs JSON | `resolve_handoff_seeds` → `gatekeeping.handoffs` | — | ✅ I/O contract |

### Gate failure format

```
br.query.sqlite_unique_ms regression: 42ms >= 15ms
```

### Proposed CI trend output (not implemented)

```json
{"metric":"br.query.lite_total_ms","fixture":"metasfresh","value_ms":200,"commit":"…","date":"2026-07-03"}
```

---

## Remaining gaps

| Gap | Impact |
|-----|--------|
| No **end-to-end CLI** T1 subprocess gate on metasfresh | In-process soft gates only; T0 covered via `fast_path_ms` |
| **T3** slice path without prior `discover --cfg` | Still rebuilds ICFG/PDG from source on large repos |
| **metasfresh** not required in CI | Soft gates skip when checkout/cache absent |
| **P2 columnar graph / query daemon** | **Done** — columnar v2 + `rbuilder serve` query daemon |
| `semantic-verification.sh` | Dominance only; no blast-radius |
| **JSON lines** trend file | Not built |

---

## Optimization roadmap

Prioritized by impact on T0–T3. Status as of 2026-07-03.

### P0 — Query path (days)

| # | Item | Metric | Status |
|---|------|--------|--------|
| 1 | **SQLite fast path** — unique bare-name `lookup` before `lookup_resolved`; UUID columns + bincode BLOBs for caller/impact payloads | `br.query.sqlite_*` | **Done** — `try_fast_cached_lookup` in `blast_radius.rs`; `MacroCallLookupDb` bincode columns with JSON fallback (`macro_call_lookup.rs`). |
| 2 | **Single snapshot session** — one mmap open per CLI invocation | `br.load.graph_snapshot_ms` | **Done** — `CliContext.snapshot_session()` caches `SnapshotSession` (store + digest); fast/lite paths reuse it. |
| 3 | **Lazy in-memory reachability** — query-time bitset expand, not load-time full matrix | `br.load.engine_snapshot_ms`, RSS | **Done** — `ReachabilityStore` in `blast_engine_snapshot.rs`; v2 snapshot load keeps sparse+zstd rows; `row_bitset(scc_id)` expands on demand. Re-benchmark T1 on metasfresh to quantify win. |

### P1 — Discover & hydrate (days–week)

| # | Item | Metric | Status |
|---|------|--------|--------|
| 4 | Prepared indexes on hydrate | `br.load.backend_hydrate_ms` | **Done** — `MemoryBackend::hydrate_prepared` applies snapshot `PreparedIndexes`; labels/properties indexed once |
| 5 | Discover deduplication — one `PreparedGraphSnapshot`, `build_from_view` | `br.discover.*` | **Done** — single `prepared` in `discover_impl.rs`; `PetGraphView::from_prepared` + `BlastRadiusEngine::build_from_view` |
| 6 | Parallel analyze loop at discover | `br.discover.analyze_all_ms` | **Done** — `rayon` parallel `engine.analyze` over functions; lazy cache uses `Mutex` for `Sync` |
| 7 | Snapshot canonical; JSON `graph.db` opt-in | — | **Done** — discover writes `graph.snapshot.bin` by default; `--write-json-graph` for legacy JSON |

### P2 — Architecture (weeks)

| # | Item | Status |
|---|------|--------|
| 8 | Columnar mmap graph — true zero-copy open | **Done** — v2 columnar `graph.snapshot.bin`; `ColumnarGraphMmap` + `SnapshotNodeStore` read mmap columns; v1 bincode still supported on read |
| 9 | CFG/PDG mmap archive for `--with-slices` | **Done** — `CfgPdgArchive`; `to_interprocedural_cfg` + PDG preload; run `discover --cfg` first on large repos |
| 10 | Ephemeral query daemon (amortize cold start) | **Done** — `rbuilder serve` on `.rbuilder/query.sock`; lite `blast-radius` auto-connects when socket present (`RBUILDER_NO_QUERY_DAEMON` to disable) |

### P3 — Cleanup

| # | Item | Status |
|---|------|--------|
| 11 | Implement `--depth` hop-limited impact zone | **Done** — limits incoming call hops on `impact_zone`; emits `metrics.caller_depth_limit`; score recomputed when capped |
| 12 | Scope policy centrality to impact subgraph only | Open |
| 13 | Wire or delete `benches/full_analysis.rs` | **Done** — removed orphaned bench |

---

## How to run benchmarks

```bash
# ── Blast-radius perf (release) ──
cargo test --release --test blast_radius_perf
cargo test --release --test blast_radius_perf -- --ignored   # 150k mock gates

cargo bench --bench blast_radius_benchmarks
RBUILDER_BENCH_LARGE=1 cargo bench --bench blast_radius_benchmarks   # 150k groups

# ── Peer scale gates ──
cargo bench --bench centrality_benchmarks
cargo bench --bench community_benchmarks
cargo test --release --test centrality_audit -- --ignored
cargo test --release --test community_audit -- --ignored

# ── Correctness (CI-friendly) ──
cargo test --release --test blast_radius
cargo test --test cli_output --test subprocess_golden_path --test all_commands_sanity

# ── Semantic pipeline (dominance perf only) ──
bash scripts/semantic-verification.sh

# ── Query daemon (amortize engine load) ──
rbuilder serve -r example/metasfresh-4.9.8b &
rbuilder blast-radius saveError -r example/metasfresh-4.9.8b   # auto-uses socket when present
RBUILDER_NO_QUERY_DAEMON=1 rbuilder blast-radius saveError -r …  # force cold CLI path

# ── Manual real-repo ──
cargo test --release rebuild_metasfresh_caches -- --ignored --nocapture
/usr/bin/time -p target/release/rbuilder blast-radius saveError -r example/metasfresh-4.9.8b
```

---

## Recommended next steps

1. **Metasfresh T3 gate** — optional soft gate for `--with-slices` when fixture cache present.

2. **Nightly large-scale** — `RBUILDER_BENCH_LARGE=1 cargo bench --bench blast_radius_benchmarks` + `phase16 --ignored`.

---

## Related files

| File | Role |
|------|------|
| `benches/blast_radius_benchmarks.rs` | Blast-radius Criterion + small-scale asserts |
| `tests/blast_radius_perf.rs` | Release perf gates (SQLite, analyze, hydrate, RSS, 150k load) |
| `.github/workflows/blast-radius-perf.yml` | CI: `blast_radius_perf` + CLI I/O tests on PR |
| `crates/rbuilder-analysis/src/cfg_pdg_archive.rs` | CFG/PDG archive for `--with-slices` (T3) |
| `benches/centrality_benchmarks.rs` | PageRank 150k template |
| `benches/community_benchmarks.rs` | Community 150k template |
| `tests/centrality_audit.rs` | Ignored PageRank regression |
| `tests/community_audit.rs` | Ignored community regression |
| `tests/rebuild_macro_index.rs` | Manual metasfresh cache rebuild |
| `tests/cli_output/subprocess_golden_path.rs` | Golden paths + `br.query.fast_path_ms` gate |
| `tests/cli_output/all_commands_sanity.rs` | CLI I/O contract (not wall-clock) |
| `docs/cli-io-sanity-qe.md` | I/O contract matrix |
| `scripts/semantic-verification.sh` | CI semantic + dominance perf |
| `src/cli/context.rs` | Snapshot session cache (`SnapshotSession`) |
| `src/cli/query_daemon.rs` | Ephemeral Unix-socket query daemon (`rbuilder serve`) |
| `src/cli/blast_radius.rs` | T0/T1/T2/T3 orchestration + fast-path lookup |
| `crates/rbuilder-analysis/src/blast_radius_scc.rs` | Engine + `ReachabilityStore` integration |
| `src/cli/discover_impl.rs` | Discover pipeline (prepared dedup, parallel blast analyze) |
| `crates/rbuilder-graph/src/backend/memory.rs` | `hydrate_prepared` with snapshot indexes |
| `src/cli/discover_output.rs` | Discover JSON telemetry (`duration_ms`) |
| `crates/rbuilder-analysis/src/macro_call_lookup.rs` | SQLite macro index (T0) |
| `crates/rbuilder-analysis/src/blast_engine_snapshot.rs` | Engine v2 sparse+zstd |
| `crates/rbuilder-graph/src/snapshot.rs` | Graph snapshot + `SnapshotNodeStore` |
| `crates/rbuilder-graph/src/columnar_snapshot.rs` | Columnar v2 mmap layout + writer |
