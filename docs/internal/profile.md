# Cold discover profile (maintainer)

How to run **accurate** discover timings on large local checkouts under `example/`. This doc replaces the old scratch file `temp.md`.

**Agent entry (summarized policy + Gate A/B + benches):** repository root [`AGENTS.md`](../../AGENTS.md) — *Starting context & performance* and *Profiles, tests, and benches*. This file is the **maintainer deep dive** (stage logs, reference machine, baseline tables).

**Scope:** developer-machine smoke tests — **not** isolated CI. Numbers vary with CPU governor, background apps, disk cache, and thermal throttling. Use gates for regression bounds; use manual runs here to inspect `[profile]` stages.

---

## Reference machine (Aug 2026)

Hardware and OS where the **latest numbers below** were recorded. This is a **developer laptop**, not a dedicated benchmark host — treat timings as directional, not reproducible across machines.

| Field | Value |
|-------|--------|
| Model | MacBook Pro (Apple Silicon) |
| CPU | **Apple M3 Pro** — 12 cores (6 performance + 6 efficiency) |
| RAM | **36 GB** |
| OS | macOS **darwin 25.6.0** (Sequoia family) |
| Binary | `target/release/rgctl` (release build immediately before each run) |
| Mode | In-repo artifacts in `{corpus}/.rgctl/` |
| Isolation | **None** — desktop apps, thermal limits, and APFS cache affect wall time |

Re-profile on **your** hardware before changing baselines in `tests/cold_profile_gates.rs`.

---

## Primary corpora

Large trees live under **`example/`** (gitignored). Fetch with `./scripts/fetch-profile-repos.sh` or clone manually. Override paths with `RGCTL_LINUX_REPO` / `METASFRESH_REPO`.

| Corpus | Path | Typical use |
|--------|------|-------------|
| **Linux kernel** | `example/linux/` | Default discover (no CFG / dashboard / harmonic) — scale ingest + analysis |
| **metasfresh** | `example/metasfresh-4.9.8b/` | Deep / full pipeline — CFG, dashboard export, optional semantic |

See also `example/README.md` (kafka, k8s-website markdown gates).

---

## Cold profile policy

1. **Release binary only:** `cargo build --release --bin rgctl`
2. **Delete artifacts:** `rm -rf example/<corpus>/.rgctl/`
3. **Run from the corpus directory** (see pitfall below)
4. **Logging:** `RUST_LOG=info,profile=info` and `discover … -v`

Warm or partial `.rgctl/` caches **invalidate** wall times (often seconds instead of minutes).

**Edge / node identity:** parallel cold `discover` is deterministic (path-sorted pass-2, callable-only `Calls`, QN-aware node IDs). See [#125](https://github.com/rgctl/rgctl/issues/125) and `docs/releases/unreleased.md` — re-discover after upgrading past that fix.

---

## Commands

### Automated gates (preferred for regression)

```bash
cargo build --release --bin rgctl
cargo test --release --test cold_profile_gates -- --ignored --nocapture --test-threads=1
```

| Gate | Corpus | Discover flags | Baseline (wall, +10%) |
|------|--------|----------------|------------------------|
| `linux_cold_discover_within_baseline` | `example/linux` | default | **75 s** (2026-10-21; 3-run mean ~71 s) |
| `metasfresh_cold_discover_within_baseline` | `example/metasfresh-4.9.8b` | `--full` | **50 s** (2026-10-21; process elapsed ~45 s, final summary ~25 s) |
| `kafka_cold_discover_within_baseline` | `example/kafka` | default | env `RGCTL_KAFKA_COLD_BASELINE_SECS` (default **12 s**; 2026-10-21 ~9.6 s) |
| `k8s_website_markdown_cold_discover_within_baseline` | `example/k8s-website` | `-l markdown` | **9 s** (2026-10-21; ~7.7 s) |
| `ecommerce_java_inheritance_cold_discover_within_baseline` | `rgctl-tests/ecommerce-java` | default | **0.36 s** wall; **0.013 s** `index_graph_build` |
| `ecommerce_java_kantra_cold_discover_within_baseline` | `rgctl-tests/ecommerce-java` | `--with-kantra --kantra-rules <fixture>` | env `RGCTL_ECOMMERCE_JAVA_KANTRA_*` (fixture catalog; fast CI path) |
| `rust_cold_discover_within_baseline` | `example/rust` | `-l rust` | **25 s** (2026-10-21; ~23.5 s) |
| `llvm_cpp_cold_discover_within_baseline` | `example/llvm-project/clang` | `-l cpp` | **12 s** (2026-10-21; ~9.7 s) |
| `roslyn_csharp_cold_discover_within_baseline` | `example/roslyn/src` | `-l csharp` | **22 s** (2026-11-03 post-98% AST; ~20.2 s process) |
| `vscode_typescript_cold_discover_within_baseline` | `example/vscode/src` | `-l typescript` | **17 s** (2026-10-21; ~15.0 s) |
| `node_javascript_cold_discover_within_baseline` | `example/node/test` | `-l javascript` | **4 s** (2026-10-21; ~3.2 s) |
| `node_javascript_cold_discover_with_cfg_within_baseline` | `example/node/test` | `-l javascript --with-cfg` | **5 s** (2026-10-21; ~3.8 s) |
| `home_assistant_python_cold_discover_within_baseline` | `example/home-assistant` | `-l python` | **15 s** (2026-10-21; ~13.4 s) |
| `discourse_cold_discover_within_baseline` | `example/discourse` | `-l ruby` | env `RGCTL_DISCOURSE_RUBY_COLD_BASELINE_SECS` (default **8 s**; 2026-11-03 ~7.0 s process) |
| `kotlin_cold_discover_within_baseline` | `example/kotlin` | `-l kotlin` | **15 s** (2026-11-03 post-98% AST; ~13.7 s) |
| `groovy_cold_discover_within_baseline` | `example/groovy` | `-l groovy` | **8 s** (2026-11-03 post-98% AST; ~6.9 s) |
| `pr_check_rgctl_graph_slice_within_baseline` | `crates/rgctl-graph` | delta `pr-check` (base cache only) | **1.0 s** |
| `linux_cold_diff_within_baseline` | `example/linux/.rgctl-diff` | `diff` (prep script; not discover) | **30 s** provisional |

Gates call `run_cold_discover_timed` in `tests/cold_profile_gates.rs` (`-r <corpus>`, `discover . -v`).

**Cold diff** (compare path only): `./scripts/prepare-linux-diff-snapshots.sh` then `linux_cold_diff_within_baseline`. See [Cold diff profile](#cold-diff-profile-linux-two-ref-pair).

**ecommerce-java gate** (inheritance external stubs): small Java fixture; asserts wall time and `[profile] stage index_graph_build` after `Extends`/`Implements`/`Permits` stub edges. Override with `RGCTL_ECOMMERCE_JAVA_COLD_BASELINE_SECS` / `RGCTL_ECOMMERCE_JAVA_INDEX_GRAPH_BUILD_BASELINE_SECS`.

### Manual profile (stage breakdown)

**Important:** `discover .` sets the session root from the positional `.` (cwd). **`-r /path/to/linux` is ignored** when you pass `.`. Always `cd` into the corpus (or pass an absolute path instead of `.`).

**Linux — default discover:**

```bash
cargo build --release --bin rgctl
rm -rf example/linux/.rgctl
cd example/linux
/usr/bin/time -l env RUST_LOG=info,profile=info \
  ../../target/release/rgctl -f json discover . -v \
  2>&1 | tee /tmp/linux-cold-profile.log
grep '\[profile\]' /tmp/linux-cold-profile.log
```

**metasfresh — full pipeline** (same as cold gate):

```bash
rm -rf example/metasfresh-4.9.8b/.rgctl
cd example/metasfresh-4.9.8b
/usr/bin/time -l env RUST_LOG=info,profile=info \
  ../../target/release/rgctl -f json discover . --full -v \
  2>&1 | tee /tmp/metasfresh-full-profile.log
```

Optional single-pass deep discover (not the cold gate):

```bash
cd example/metasfresh-4.9.8b
rm -rf .rgctl
/usr/bin/time -l env RUST_LOG=info,profile=info \
  ../../target/release/rgctl -f json discover . \
  --with-cfg --with-security --with-taint -v \
  2>&1 | tee /tmp/metasfresh-deep-profile.log
```

### Declarative taint rules (post-#16)

`discover --with-taint` applies language packs from `crates/rgctl-analysis/rules/taint/*.yaml`
(compiled once per session; scoped by `-l` / detected languages). Override with
`.rgctl/taint-rules.d/` or `--taint-rules PATH`. Release smoke:
`cargo test --release --test taint_rules_release_smoke -- --ignored --nocapture`.
When claiming wall regression vs pre-declarative baselines, keep ≤10% on gbuilder /
document metasfresh sampling; Gate A (linux cold discover) is N/A unless the default
(non-taint) discover path changes.

---

## Reading `[profile]` output

| Line | Meaning |
|------|---------|
| `[profile] discover summary` | Wall time, `index_secs`, `post_index_secs`, peak RSS (`peak_rss_mb`, `ingest_peak_rss_mb`, `analysis_peak_rss_mb`), node/function counts |
| `[profile] stage` | Per-stage wall seconds and `%` of discover wall (`index_extract`, `extract_pass1`, `index_graph_build`, `graph_resolution_index`, `graph_pass2`, `graph_spill_columnar`, `centrality`, …) |
| `[profile] extract cpu stage` | CPU-sum across extract workers (`extract_read`, `extract_parse`); may exceed `index_extract` wall |
| `[profile] centrality breakdown` | PageRank / betweenness / harmonic sub-times |
| `[profile] save_dashboard stage` | Dashboard export substeps (e.g. `export_cfg_slice`) |
| `[profile] cfg cpu stage` | CFG thread CPU sums (can exceed wall on parallel passes) |

`index_extract` wall = parallel file extract + sequential pass-1 merge. Sub-breakdown:

| Stage | Kind | Meaning |
|-------|------|---------|
| `extract_read` | CPU sum | `fs::read` across workers |
| `extract_parse` | CPU sum | plugin `extract_all` / tree-sitter across workers |
| `extract_pass1` | Wall | sequential symbol/config commit on the merge thread |
| `graph_resolution_index` | Wall | build resolution indexes |
| `graph_pass2` | Wall | relation / config-usage resolution |
| `graph_spill_columnar` | Wall | spill finish + columnar snapshot compile (`DEFAULT_SORT_RUN_BYTES` = 256 MiB) |

Default discover does **not** attach a body-storing `CodeIndex` (avoids multi-GB `code_index.json` / RAM). Nodes still receive `code_hash` from worker-precomputed prep.

Harmonic runs only when `--with-harmonic` or **`discover --full`** (deep stage). Default linux discover skips harmonic and dashboard export.

### Kantra stages (`--with-kantra`)

| Stage | When | Notes |
|-------|------|-------|
| `kantra_load` | Eval | Catalog decode / engine setup |
| `kantra_eval` | Eval | Total eval wall (includes sub-stages below) |
| `kantra_filecontent` | Eval | `builtin.filecontent` / `builtin.file` |
| `kantra_referenced` | Eval | `go.referenced` / `java.referenced` |
| `kantra_compose` | Eval | `and` / `or` / `not` composition |
| `kantra_index` | After analysis persist | Rewrites `graph.snapshot.bin` with `KantraRule` nodes (runs after all cold mmap use) |

Cold gate `ecommerce_java_kantra_cold_discover_within_baseline` uses `--kantra-rules tests/fixtures/kantra-rules` (small fixture, stable timing). Embedded-catalog discover on the same corpus is heavier (~2.6k rules); profile manually when bumping the rulesets submodule pin.

Algorithm detail (sampled betweenness, HyperBall, adaptive gating): [analysis-architecture.md](../analysis-architecture.md), [harmonic-centrality.md](../harmonic-centrality.md), [graph-metrics-design.md](../design/graph-metrics-design.md).

---

## Gate baselines (2026-10-21, single-pass AST)

Recorded on the **reference machine** above, release `rgctl`, cold `.rgctl/` removed. These values are the **`cold_profile_gates`** baselines (+10% tolerance). Prior 2026-08/09 envelopes kept in git history for comparison.

### Linux (`example/linux`) — default discover

| Metric | Value |
|--------|-------|
| Wall (3-run mean / max) | **71.0 s** / **72.3 s** |
| **Gate baseline** | **75 s** (pass ≤ 82.5 s) |
| Peak RSS (rgctl) | **~13.1–14.1 GB** |
| Nodes / functions | 2,700,362 / 1,864,594 |

Top stages (% of wall): `index_extract` **~18 s** (25%), `extract_pass1` **~18 s** (25%), `index_graph_build` **~13 s** (18%), `graph_spill_columnar` **~10 s** (14%).

### metasfresh (`example/metasfresh-4.9.8b`) — `--full`

| Metric | Value |
|--------|-------|
| Wall (final summary / process elapsed) | **~25 s** / **~45 s** |
| **Gate baseline** | **50 s** (pass ≤ 55 s; gate times process elapsed) |
| Peak RSS | **~5.1 GB** (final); ingest ~1.9 GB |
| Nodes / functions | 311,051 / 133,143 |

Stage walls (profile summaries): **basic ~18 s**, **deep ~46 s**, **semantic ~9 s** (inferred from timestamps; semantic has little `[profile]` logging).

Deep-pass hotspots: `cfg_total` **~18 s**, `save_dashboard` **~11 s** (`export_cfg_slice` **~9.7 s**), harmonic centrality **~6.7 s**, `field_write` **~4.6 s**.

### Rust (`example/rust`) — `-l rust`

| Metric | Value |
|--------|-------|
| Wall (real / profile) | **~23 s** / **~21 s** |
| **Gate baseline** | **25 s** (pass ≤ 27.5 s) |
| Peak RSS | **~3.9 GB** |
| Nodes / functions | 445,634 / 186,416 |
| Files indexed | 38,631 / 38,640 discovered |
| `index_graph_build` | **~4.9 s** |

Top stages (% of wall): `index_extract` **~6.4 s** (30%), `index_graph_build` **~4.9 s** (23%), `save_tracker` **~1.5 s** (7%).

### Roslyn (`example/roslyn/src`) — `-l csharp`

| Metric | Value |
|--------|-------|
| **Gate baseline** | **90 s** (pass ≤ 99 s; record on reference machine, override `RGCTL_ROSLYN_COLD_BASELINE_SECS`) |
| Corpus | `dotnet/roslyn` `src/` (~8k+ `.cs`) |

### VS Code (`example/vscode/src`) — `-l typescript`

| Metric | Value |
|--------|-------|
| **Gate baseline** | **120 s** (pass ≤ 132 s; record on reference machine, override `RGCTL_VSCODE_TYPESCRIPT_COLD_BASELINE_SECS`) |
| Corpus | `microsoft/vscode` `src/` (~10k `.ts`) |

### Node.js (`example/node/test`) — `-l javascript`

| Metric | Value |
|--------|-------|
| **Gate baseline** | **5 s** (pass ≤ 5.5 s; record on reference machine, override `RGCTL_NODE_JAVASCRIPT_COLD_BASELINE_SECS`) |
| Corpus | `nodejs/node` `test/` (sparse checkout; ~9.2k discoverable `.js`/`.mjs`) |
| Wall (reference, 2026-09-04) | **~4.5 s** |
| Nodes / functions | **54,744** / **15,635** |
| Files indexed | **9,213** |
| `index_graph_build` | **~0.37 s** |

**With `--with-cfg`:**

| Metric | Value |
|--------|-------|
| **Gate baseline** | **7 s** (pass ≤ 7.7 s; override `RGCTL_NODE_JAVASCRIPT_COLD_WITH_CFG_BASELINE_SECS`) |
| Wall (reference, 2026-09-04) | **~6.2 s** |
| Wall (reference, 2026-10-21, after `accelerate-cfg-discover-pipeline`) | **~4.8 s** |
| Peak RSS | **~614 MB** |
| `cfg_total` | **~1.1 s** |

### Home Assistant (`example/home-assistant`) — `-l python`

| Metric | Value |
|--------|-------|
| **Gate baseline** | **20 s** (pass ≤ 22 s; override `RGCTL_HOME_ASSISTANT_PYTHON_COLD_BASELINE_SECS`) |
| Corpus | `home-assistant/core` (~18.6k `.py`) |
| Wall (reference, 2026-09-04) | **~18.5 s** |
| Nodes / functions | **558,729** / **122,320** |
| Files indexed | **18,628** |
| `index_graph_build` | **~3.4 s** |

### Discourse (`example/discourse`) — `-l ruby`

| Metric | Value |
|--------|-------|
| **Gate baseline** | **120 s** (pass ≤ 132 s; override `RGCTL_DISCOURSE_RUBY_COLD_BASELINE_SECS`) — conservative placeholder until the team pins a stricter wall from reference hardware |
| Corpus | [discourse/discourse](https://github.com/discourse/discourse) (`./scripts/fetch-profile-repos.sh` → `example/discourse`, depth-1 clone OK) |
| Discover | `discover . -v -l ruby` from repo root (exclude heavy dirs via `-e vendor,tmp,node_modules` when profiling manually) |
| Wall (reference, 2026-09-17, M3 Pro) | **~4.0 s** |
| Nodes / functions | **~71,888** / **~42,944** |
| `index_graph_build` | **~0.6 s** |

Fixture-scale checks: `rgctl-tests/ecommerce-ruby` (`tests/ruby_langfeatures.rs`, `tests/ruby_cfg_analysis.rs`, `tests/dashboard_ecommerce_ruby.rs`).

### Kotlin (`example/kotlin`) — `-l kotlin`

| Metric | Value |
|--------|-------|
| **Gate baseline** | **15 s** (pass ≤ 16.5 s; override `RGCTL_KOTLIN_COLD_BASELINE_SECS`; 2026-11-03 post-98% AST) |
| Corpus | [JetBrains/kotlin](https://github.com/JetBrains/kotlin) sparse `libraries`+`plugins`+`analysis` (`./scripts/fetch-profile-repos.sh` → `example/kotlin`) |
| Discover | `discover . -v -l kotlin` from repo root |
| Wall (reference, 2026-09-29) | **~8.9 s** |
| Nodes / functions | **178,238** / **64,386** |
| `index_graph_build` | **~1.5 s** |
| `.kt` sources (approx.) | **~18k** |

Fixture-scale: `rgctl-tests/ecommerce-kotlin`, `tests/dashboard_ecommerce_kotlin.rs`.

### Groovy (`example/groovy`) — `-l groovy`

| Metric | Value |
|--------|-------|
| **Gate baseline** | **8 s** (pass ≤ 8.8 s; override `RGCTL_GROOVY_COLD_BASELINE_SECS`; 2026-11-03 post-98% AST) |
| Corpus | [gradle/gradle](https://github.com/gradle/gradle) (`./scripts/fetch-profile-repos.sh` → `example/groovy`; Jenkins core is not dense enough in `.groovy`) |
| Discover | `discover . -v -l groovy` from repo root |
| Wall (reference, 2026-09-29) | **~4.3 s** |
| Nodes / functions | **67,507** / **16,552** |
| `index_graph_build` | **~0.31 s** |
| `.groovy` sources (approx.) | **~6.7k** |

Fixture-scale: `rgctl-tests/ecommerce-groovy`, `tests/dashboard_ecommerce_groovy.rs`.

### CFG on large C++ corpora (`--with-cfg`)

`discover --with-cfg` builds per-function CFGs on a dedicated **16 MiB** Rayon pool (`with_large_pool` / `rgctl-worker-*`) with the pass coordinated on a **`rgctl-large-stack`** thread. Default discover/extract uses the normal pool (OS default ~2 MiB worker stacks). Field-write indexing after CFG also runs on a large-stack thread.

Pathological inputs (e.g. llvm `clang/test/Index/annotate-deep-statements.cpp` with thousands of nested `call_expression` nodes) are capped at **depth 2048** during CFG expression walks and def-use extraction (`cfg_builder` / `def_use`); one warning per function is logged and deeper branches are skipped. For full llvm `clang/` CFG discover on older builds, `RUST_MIN_STACK=16777216` was the workaround.

---

## Snapshot diff micro-benchmarks

Compare-path benches (no discover required):

```bash
cargo bench -p rgctl-graph --bench snapshot_diff
```

Groups: `digest_fast_path_equal`, `node_index_parallel`, `edge_merge_join`, `full_diff_noop_sink`.

**Reference M3 Pro (2026-09-10, release bench profile):**

| Benchmark | Result |
|-----------|--------|
| `digest_fast_path_equal` | ~1.34 ns |
| `node_index_parallel/5000` | ~3.19 ms |
| `node_index_parallel/20000` | ~13.4 ms |
| `edge_merge_join/10000` | ~1.58 ms |
| `edge_merge_join/50000` | ~8.28 ms |
| `full_diff_noop_sink` (10k nodes / 40k edges) | ~6.51 ms |

### Cold diff profile (linux two-ref pair)

**Separate from cold discover.** Measures open + digest + `diff_snapshots` on two pre-built columnar snapshots (noop sink). Default pair: **v7.1** vs **HEAD** of `example/linux`.

```bash
cargo build --release --bin rgctl
./scripts/prepare-linux-diff-snapshots.sh   # worktrees + discover → example/linux/.rgctl-diff/{base,head}/
RUST_LOG=info,profile=info ./target/release/rgctl -f json diff \
  --base example/linux/.rgctl-diff/base \
  --head example/linux/.rgctl-diff/head
cargo test --release --test cold_profile_gates linux_cold_diff_within_baseline -- --ignored --nocapture
```

| Gate | Artifacts | Baseline (wall, +10%) |
|------|-----------|------------------------|
| `linux_cold_diff_within_baseline` | `example/linux/.rgctl-diff/{base,head}/graph.snapshot.bin` | **30 s** provisional (`RGCTL_LINUX_COLD_DIFF_BASELINE_SECS`) |

Look for `[profile] diff summary` (`wall_secs`, `open_secs`, `digest_secs`, `diff_secs`, node/edge delta counts). Prep is expensive (two cold discovers); the gate only times the compare path. Override refs: `BASE_REF=v6.1 HEAD_REF=v6.6 ./scripts/prepare-linux-diff-snapshots.sh`.

### `pr-check` end-to-end (rgctl-graph self-slice)

Warm base cache at `crates/rgctl-graph/.rgctl-base/.rgctl/` only; delta head is synthesized at runtime (no pre-built head snapshot). Policy: `rgctl-tests/rgctl-pr-policy.json`. Refs: `HEAD~5..HEAD`. Scoped blast/centrality subgraph (upstream `Calls` closure of PR entities).

```bash
cargo build --release --bin rgctl
cd crates/rgctl-graph
mkdir -p .rgctl-base/.rgctl
cp .rgctl/graph.snapshot.bin .rgctl-base/.rgctl/   # one-time base seed
../../target/release/rgctl -r . -f json pr-check \
  --policy-file ../../rgctl-tests/rgctl-pr-policy.json \
  --base-ref HEAD~5 --head-ref HEAD
```

**2026-09-10 (Phase 0 dual snapshot):** subprocess wall **~0.9 s**. **2026-09-10 (Phase 5 scoped + delta head):** same ballpark with mmap subset hydrate instead of dual full `hydrate_backend`. Ignored gate: `pr_check_rgctl_graph_slice_within_baseline` (baseline **1.0 s**, +10%).

### `temporal-delta-pr-gate` final gates (2026-09-10)

Release binary rebuilt immediately before each run (`cargo build --release --bin rgctl`).

| Gate | Wall | Baseline (+10%) | Result |
|------|------|-----------------|--------|
| `linux_cold_discover_within_baseline` | **144.9 s** (2,702,784 nodes) | 145 s | pass |
| `metasfresh_cold_discover_within_baseline` (`--full`) | **30.0 s** (319,225 nodes) | 74 s | pass |

Deterministic node IDs + delta `pr-check` did not regress cold discover on the primary corpora.

### `accelerate-cfg-discover-pipeline` (2026-10-21)

Single-pass CFG/CPG (fused field-write + skeleton), worker-side bincode, and `STAGE_DEEP` analytics reuse. Release binary rebuilt immediately before each run.

| Gate | Wall | Baseline (+10%) | Result |
|------|------|-----------------|--------|
| `node_javascript_cold_discover_with_cfg_within_baseline` | **4.8 s** (88,168 nodes / 48,670 fn) | 7 s | pass |
| `metasfresh_cold_discover_within_baseline` (`--full`) | **25.7 s** (311,051 nodes / 133,143 fn) | 74 s | pass |
| `linux_cold_discover_within_baseline` (default; no CFG) | **133.7 s** (2,700,362 nodes) | 145 s | pass |

`--with-cfg` / `--full` benefit from fused AST reuse and pre-encoded archive writes; Gate A (linux default) stays within the existing envelope (no intentional default-path change).

### `rgctl-all-dimension-supremacy` §10 hot-path / zero-cold (2026-10-23)

Performance engineering pass: markdown parse pool, relation-resolution scratch buffers, structured-query hot-column scans, PageRank/SampledBetweenness parallel thresholds, expanded `columnar_zero_cold_touch_gate`, CI bench smoke + `$GITHUB_STEP_SUMMARY`. Release binary rebuilt immediately before Gate A.

| Gate | Discover wall | Nodes / fn | Process elapsed vs baseline×1.10 | Result |
|------|---------------|------------|----------------------------------|--------|
| `linux_cold_discover_within_baseline` | **55.8 s** (`index_graph_build` 13.3 s) | 1,769,607 | under 82.5 s | pass |
| `metasfresh_cold_discover_within_baseline` | **26.5 s** | 313,398 / 133,383 | under 55 s | pass |
| `rust_cold_discover_within_baseline` | **25.6 s** (`index_graph_build` 9.3 s) | 476,868 / 186,244 | under 27.5 s | pass |
| `home_assistant_python_cold_discover_within_baseline` | **12.1 s** | 582,389 / 132,504 | under 16.5 s | pass |
| `discourse_cold_discover_within_baseline` | **4.9–5.3 s** | 73,168 / 44,127 | near 6.6 s process budget | pass (retry) |
| `kotlin_cold_discover_within_baseline` | **8.5–12.1 s** | 178,896 / 64,383 | under 12.1 s when warm | pass (retry) |
| `groovy_cold_discover_within_baseline` | **4.5–5.8 s** | 67,697 / 16,552 | process elapsed ~6.05 s vs 5.5×1.10 | pass (retry) |
| `ecommerce_java_inheritance_cold_discover_within_baseline` | **0.353 s** (`index_graph_build` 0.0075 s) | 1,349 | under 0.36×1.10 (0.396 s) | pass |
| `ecommerce_java_kantra_cold_discover_within_baseline` | **0.325 s** (`kantra_eval` 0.0040 s) | 1,349 | under 0.40×1.10 (0.440 s) | pass |
| `columnar_zero_cold_touch_gate` (PageRank/Leiden + hot name/type scan + `filter_function_impact`) | — | 2k synthetic | `cold_ext == 0` | pass |

> Note: Gate assertions use **process elapsed** (not only `[profile] wall_secs`). Near-budget corpora (discourse / kotlin / groovy) can flake under desktop load even when discover wall is fine — re-run before changing baselines. `MemoryMonitor` single-PID refresh (`refresh_pids_specifics`) and `park_timeout`/`unpark` eliminated ~425 ms of startup/shutdown overhead on small fixtures like `ecommerce-java`. This machine’s `example/linux` produced fewer nodes than the 2026-10-21 ~2.7M reference; wall stayed well under Gate A.

### `rgctl-all-dimension-supremacy` §2 single-pass AST (2026-10-21)

Tier-1 language `extract_all` migrated to `ScopeStack` + `KindDispatchTable` + `parse_with_pool`. Cold walls re-measured and gate constants tightened:

| Corpus / gate | Measured wall | New baseline | peak_rss_mb |
|---------------|---------------|--------------|-------------|
| linux (default) | **~71 s** (3-run) | **75 s** | ~13–14k |
| metasfresh `--full` | **~25 s** summary / **~45 s** elapsed | **50 s** | ~5.1k |
| kafka | **9.6 s** | **12 s** | 2194 |
| rust `-l rust` | **23.5 s** | **25 s** | 4173 |
| llvm `clang/` `-l cpp` | **9.7 s** | **12 s** | 1539 |
| roslyn `src/` `-l csharp` | **16.0 s** | **18 s** | 3656 |
| vscode `src/` `-l typescript` | **15.0 s** | **17 s** | 3149 |
| node `test/` `-l javascript` | **3.2 s** | **4 s** | 551 |
| node + `--with-cfg` | **3.8 s** | **5 s** | 1183 |
| home-assistant `-l python` | **13.4 s** | **15 s** | 2873 |
| discourse `-l ruby` | **4.5 s** | **6 s** | 643 |
| kotlin | **10.0 s** | **11 s** | 1285 |
| groovy | **4.8 s** | **5.5 s** | 415 |
| k8s-website `-l markdown` | **7.7 s** | **9 s** | 1024 |
| magento2 `-l php` (no gate yet) | **11.9 s** | — | 2026 |

---

## Related

- `tests/cold_profile_gates.rs` — baselines and `TOLERANCE` (+10%)
- `example/README.md` — fetch script and optional corpora
- `docs/analysis-architecture.md` — discover pipeline and centrality gating
- [`AGENTS.md`](../../AGENTS.md) — agent-facing cold-profile + Gate A/B summary (canonical for contributors)

## Ansible/Jinja structural support — 2026-10-30 smoke measurements

Gate A passed with a release binary on macOS arm64 (10 visible CPUs, 32 GiB RAM)
using Linux commit `4dd16e31135ea8437a720222df34e8745f385d6d` on a
case-sensitive APFS volume. The final-source cold discover reported 61.5 s wall,
1,813,103 nodes, and 15.317 s `index_graph_build`; test elapsed was 63.81 s,
below the 82.5 s limit. This is one run, not a new reference baseline.

On 2026-11-03, after integrating onto upstream `c88773f1289bc2908454b76555e9a63a119f391c`,
the same pinned corpus and case-sensitive volume passed Gate A with 60.6 s
discover wall, 1,819,406 nodes, 15.370 s `index_graph_build`, and 62.88 s test
elapsed. This single result is not a new reference baseline.

On 2026-11-04, after the ingest review fixes, the same Linux revision on an
isolated case-sensitive checkout passed Gate A with 63.1 s discover wall,
1,819,406 nodes, 15.528 s `index_graph_build`, and 65.37 s test elapsed.
This single run is not a new reference baseline.

Pinned corpus: `ansible/ansible` **v2.19.0**, commit
`82529e534dd3edd84aba03d86b337f88c58b9982`, sparse checkout of
`test/integration/targets` (recipe in `example/README.md`). A NUL-delimited
`git ls-tree -rz --name-only HEAD test/integration/targets` count gives 3,537
tracked files, 1,779 YAML files (710 under `tasks/`), 72 standard Jinja-suffix
files, and 93 files under `templates/`. NUL delimiting matters for Unicode and
unusual upstream fixture filenames.

The corpus uses nonstandard standalone test-role layouts. For these measurements,
its local, untracked `rgctl.toml` contained:

```toml
[languages.ansible]
include = [
  "test/integration/targets/**/tasks/*.yml",
  "test/integration/targets/**/tasks/*.yaml",
  "test/integration/targets/**/handlers/*.yml",
  "test/integration/targets/**/handlers/*.yaml",
  "test/integration/targets/**/meta/main.yml",
  "test/integration/targets/**/meta/main.yaml",
]
[languages.jinja]
include = ["test/integration/targets/**/templates/**"]
```

Hardware: macOS arm64, 10 visible CPUs, 32 GiB RAM. Release profile: repository
`opt-level=3`, fat LTO, one codegen unit. Each row is one run, with the corpus's
`.rgctl/` removed immediately beforehand, executed **inside** `example/ansible`:

```bash
RUST_LOG=info,profile=info ../../target/release/rgctl -f json discover . -v -l ansible,jinja
# Repeat cold with -l ansible, -l jinja, and joint selection plus:
# --with-limits max-mem-mb=4096,threads=1
```

| Cold selection | Indexed files | `wall_secs` | Nodes | Functions | `index_graph_build` | Observed RSS MiB |
|---|---:|---:|---:|---:|---:|---:|
| `ansible,jinja` | 1,503 | 1.366 s | 25,353 | 106 | 0.184 s | 355 |
| `ansible` | 1,400 | 1.331 s | 24,901 | 0 | 0.178 s | 329 |
| `jinja` | 103 | 0.741 s | 451 | 106 | 0.011 s | 144 |
| Joint, 4,096 MiB / 1 thread | 1,503 | 1.593 s | 25,353 | 106 | 0.189 s | 365 |

A 2026-11-04 joint-selection smoke run on an isolated copy of the same pinned
checkout and configuration indexed 1,503 files in 1.345 s, with 25,358 nodes,
106 functions, 0.203 s `index_graph_build`, and 372 MiB observed peak RSS.
Process elapsed was 1.374 s. On that same copy, the preceding PR head and the
review fixes produced identical Ansible and Jinja symbol sets and identical
node and edge counts. These single runs are not a performance baseline.

Process elapsed times were 1.47 / 1.38 / 0.79 / 1.65 s respectively. RSS is
the larger of the summary's sampled peak and ingest peak. Function counts include Jinja's
synthetic render scopes and named macros/blocks; Ansible tasks are distinct
`AnsibleTask` entities. Language-filtered generic configuration candidates are
not counted as indexed files.

These are **smoke measurements**, not a Gate B baseline: neither the Ansible
source selection nor the Jinja template selection approaches ~10k relevant
files. No +10% language-scale gate or scale-readiness claim is added from these
numbers. A larger pinned corpus and repeated measurements on the reference
machine remain necessary for a language-scale baseline.
