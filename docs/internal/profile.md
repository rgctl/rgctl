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

**Edge / node identity:** parallel cold `discover` is deterministic (path-sorted pass-2, callable-only `Calls`, QN-aware node IDs). See [#125](https://github.com/sshaaf/rgctl/issues/125) and `docs/releases/unreleased.md` — re-discover after upgrading past that fix.

---

## Commands

### Automated gates (preferred for regression)

```bash
cargo build --release --bin rgctl
cargo test --release --test cold_profile_gates -- --ignored --nocapture --test-threads=1
```

| Gate | Corpus | Discover flags | Baseline (wall, +10%) |
|------|--------|----------------|------------------------|
| `linux_cold_discover_within_baseline` | `example/linux` | default | **145 s** |
| `metasfresh_cold_discover_within_baseline` | `example/metasfresh-4.9.8b` | `--full` | **74 s** |
| `kafka_cold_discover_within_baseline` | `example/kafka` | default | env `RGCTL_KAFKA_COLD_BASELINE_SECS` (default 600 s) |
| `k8s_website_markdown_cold_discover_within_baseline` | `example/k8s-website` | `-l markdown` | **3 s** |
| `ecommerce_java_inheritance_cold_discover_within_baseline` | `rgctl-tests/ecommerce-java` | default | **0.31 s** wall; **0.008 s** `index_graph_build` |
| `ecommerce_java_kantra_cold_discover_within_baseline` | `rgctl-tests/ecommerce-java` | `--with-kantra --kantra-rules <fixture>` | env `RGCTL_ECOMMERCE_JAVA_KANTRA_*` (fixture catalog; fast CI path) |
| `rust_cold_discover_within_baseline` | `example/rust` | `-l rust` | **25 s** |
| `node_javascript_cold_discover_within_baseline` | `example/node/test` | `-l javascript` | **5 s** |
| `node_javascript_cold_discover_with_cfg_within_baseline` | `example/node/test` | `-l javascript --with-cfg` | **7 s** |
| `home_assistant_python_cold_discover_within_baseline` | `example/home-assistant` | `-l python` | **20 s** |
| `discourse_cold_discover_within_baseline` | `example/discourse` | `-l ruby` | env `RGCTL_DISCOURSE_RUBY_COLD_BASELINE_SECS` (default **120 s**; see measured run below) |
| `kotlin_cold_discover_within_baseline` | `example/kotlin` | `-l kotlin` | **10 s** (JetBrains/kotlin sparse; 2026-09-29) |
| `groovy_cold_discover_within_baseline` | `example/groovy` | `-l groovy` | **5 s** (gradle/gradle; 2026-09-29) |
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

## Gate baselines (2026-08-25)

Recorded on the **reference machine** above, release `rgctl`, cold `.rgctl/` removed. These values are the **`cold_profile_gates`** baselines (+10% tolerance).

### Linux (`example/linux`) — default discover

| Metric | Value |
|--------|-------|
| Wall (real / profile) | **144.7 s** / **143.5 s** |
| **Gate baseline** | **145 s** (pass ≤ 159.5 s) |
| Peak RSS | **~14.7 GB** |
| User / sys CPU | 250.6 s / 47.1 s |
| Nodes / functions | 2,657,548 / 1,862,845 |
| Files indexed | 71,341 / 71,348 discovered |

Top stages (% of wall): `index_extract` **71.9 s** (50%), `index_graph_build` **19.2 s** (13%), `save_tracker` **9.4 s** (7%).

### metasfresh (`example/metasfresh-4.9.8b`) — `--full`

| Metric | Value |
|--------|-------|
| Wall (real) | **73.9 s** |
| **Gate baseline** | **74 s** (pass ≤ 81.4 s) |
| Peak RSS | **~6.4 GB** |
| User / sys CPU | 242.6 s / 64.4 s |
| Nodes / functions | 319,175 / 133,112 |

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
| **Gate baseline** | **10 s** (pass ≤ 11 s; override `RGCTL_KOTLIN_COLD_BASELINE_SECS`) |
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
| **Gate baseline** | **5 s** (pass ≤ 5.5 s; override `RGCTL_GROOVY_COLD_BASELINE_SECS`) |
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

---

## Related

- `tests/cold_profile_gates.rs` — baselines and `TOLERANCE` (+10%)
- `example/README.md` — fetch script and optional corpora
- `docs/analysis-architecture.md` — discover pipeline and centrality gating
- [`AGENTS.md`](../../AGENTS.md) — agent-facing cold-profile + Gate A/B summary (canonical for contributors)
