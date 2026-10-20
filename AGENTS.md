# Agent instructions for rgctl

## Summary

`rgctl` is a high-performance Rust code knowledge graph for LLM agents: tree-sitter extraction, typed relations, mmap snapshots, blast-radius / communities / CPG, and JSON-first CLI (`-f json`).

**Your goal when contributing here:** preserve ingest scale, query correctness, memory discipline, and deterministic artifacts under `.rgctl/` — not add convenience at the cost of Tokio blocking, whole-repo clones, or ungated cold regressions.

> **Looking for how to *use* rgctl on another codebase?** Install skills (`rgctl install --skill`) or copy [docs/agents/USER_AGENTS_TEMPLATE.md](docs/agents/USER_AGENTS_TEMPLATE.md) into *that* repo’s `AGENTS.md`. See [docs/guides/agent-skill.md](docs/guides/agent-skill.md).

---

## Must-follow rules

- **Async vs CPU:** Discover/serve use `tokio`. Do **not** run heavy CPU (parse, graph analytics, CFG) on the async executor — use `spawn_blocking` / Rayon where the pipeline already does.
- **Parallel ingest:** Per-file plugin extraction runs on the discover worker pool. Do not replace with a serial whole-repo walk when parallel ingest exists.
- **Streaming commits:** Emit symbols/relations file-by-file; avoid unbounded `Vec<Relation>` / whole-repo ASTs before commit (`rgctl-extraction` spill patterns).
- **Clone hygiene:** Prefer `&[u8]` / `Cow` / borrows in tree-sitter walkers; `Vec::with_capacity` when sizes are known; no `unwrap()` in library paths.
- **Ingest hot path:** Follow **Ingest hot-path practices** below (no per-symbol heap strings, hash/prep once on workers, spill scratch reuse, tracker mapping without re-scan).
- **Typed graph:** Respect `EdgeType` / node kinds; do not invent ad-hoc string edges for hot paths.
- **Artifacts:** Session data lives in `{repo}/.rgctl/`. Warm caches invalidate wall-time claims.
- **Constrained discover (opt-in):** Prefer `rgctl discover . --with-limits max-mem-mb=4096,threads=1` (or `RGCTL_WITH_LIMITS`) in containers / cgroups — do **not** change default desktop discover for memory. Soft RSS tripwire at ~95%; smaller spill sort-runs and stream channel when `max-mem-mb` is set. Container smoke: `./scripts/run-container-with-limits-smoke.sh` (`tests/Containerfile`, mounts `example/linux`).
- **Features:** Default semantic embedder is compiled **vocab**. Do not require ONNX / Python ML unless behind an explicit feature (e.g. `semantic-onnx` / code-daemon + Git LFS).
- **OpenSpec language work:** Still cite [openspec/changes/_shared/starting-context.md](openspec/changes/_shared/starting-context.md) (pointer here); follow the sections below.
- **Grammar bumps:** When you bump a tree-sitter grammar pin, update that language’s `*-ast-coverage.json` (and add the language to `rgctl-ast-coverage::bundled_specs` for new languages). Unit tests hard-fail the same drift; `cargo check -p rgctl-languages` warns (`RGCTL_AST_COVERAGE_STRICT=1` fails). The website `/docs/languages/` pages are generated from those JSON files — do not maintain parallel tables under `docs/languages/`.
- **Releases:** Follow **Releases** below (and [docs/releasing.md](docs/releasing.md)). Do not hand-edit dozens of crate `version =` lines.
---

## Context & architecture

- **Discover** walks the tree, runs language plugins (tree-sitter), builds the graph, writes compact caches to `.rgctl/`.
- **Query** paths are read-oriented and return versioned JSON (`schema_version` on stdout — never scrape stderr).
- **Analysis** (`rgctl-analysis`) projects CSR / callgraph / centrality / blast-radius / CFG–PDG; see [docs/analysis-architecture.md](docs/analysis-architecture.md).
- **Languages:** `crates/rgctl-lang-*` + `rgctl-plugin-api`; register in `languages.toml`. See **Grammar bumps** under Must-follow for AST coverage manifests. Adding a language: [docs/language-testing-guide.md](docs/language-testing-guide.md) (where to put tests / what to assert).

---

## Starting context & performance policy

Applies to all extraction / language / discover hot-path work (and OpenSpec `*-extraction-depth` / `add-*-language-support` changes).

### Implementation model

1. **Async** — existing `tokio` orchestration; offload CPU-heavy work.
2. **Parallel** — discover file pool (`rayon` / workers).
3. **Streaming** — incremental graph commit; match extraction spill/channel patterns.
4. **Idiomatic Rust** — `Result` + `thiserror`; follow `rgctl-lang-java` / `rgctl-extraction` conventions.

### Ingest hot-path practices

Rules distilled from linux cold-discover work (`index_extract` / pass-1 / spill / `save_tracker`). Breaking these usually shows up as Gate A wall or RSS regressions — treat O(files)×O(symbols) heap work as a bug.

| Practice | Do | Don't |
|----------|----|-------|
| **No heap strings per symbol** | Pass `&str` / slices into pass-1 (`add_symbol_with_prep`); borrow file bytes | `String::from` / `to_string()` for every symbol body or path key on the merge thread |
| **Prep on workers** | Compute line offsets, BLAKE3 `code_hash`, token bloom in `SymbolPass1Prep` on extract workers | Re-walk source / re-hash on the sequential pass-1 thread |
| **Hash once** | Set `FileExtraction.file_hash` from bytes already in memory; thread through `StreamStats` / `PipelineStats` into `FileTracker::index_files_with_mapping` | Re-`fs::read` + BLAKE3 all files in `save_tracker` after extract already hashed them |
| **Empty-tracker short-circuit** | When `file_hashes.json` is empty, `detect_changes` marks all paths **added** without hashing (keeps ChangeSet non-empty so a stale snapshot is not reused) | Hash the whole tree twice on cold discover (detect + index) |
| **Normalize / map once per file** | `begin_file_batch` → push into `active_tracker_ids`; flush once in `end_file_batch`; accumulate mapping at commit | Per-symbol `HashMap::get_mut` / `normalize_path_str(...).into_owned()`; full mmap node scan/sort just to rebuild file→node ids |
| **Reuse tree-sitter parsers** | Call `rgctl_plugin_helpers::parse_source` (thread-local `Parser` per Rayon worker); never `Parser::new()` per file on the extract hot path | Fresh `Parser::new` + `set_language` inside `extract_*` / `parse` for every file (linux: ~71k×) |
| **Spill alloc reuse** | `SegmentedSpill` scratch `Vec` + `bincode::serialize_into`; keep sort runs at `DEFAULT_SORT_RUN_BYTES` (256 MiB) unless profiling says otherwise | Fresh `bincode::serialize` → new `Vec<u8>` per node/edge; shrinking sort runs without a cold gate |
| **CodeIndex bodies off by default** | Default discover: no body-storing `CodeIndex` (no multi-GB `code_index.json`); nodes still get `code_hash` from prep | Attach a full CodeIndex on the cold path “for convenience” |

When adding extract or graph-commit code, ask: *does this allocate or re-read once per symbol/file on the sequential merge thread?* If yes, move it to workers or reuse an existing buffer/key.

Stage meanings and current linux notes: [docs/internal/profile.md](docs/internal/profile.md).

### Cold profile (mandatory for scale / perf claims)

1. **Release binary only:** `cargo build --release --bin rgctl`
2. **Delete artifacts:** `rm -rf <corpus>/.rgctl/`
3. **Run from inside the corpus** (`cd example/<corpus> && rgctl discover . -v`) — positional `.` sets session root; `-r` is ignored when `.` is passed.
4. **Logging:** `RUST_LOG=info,profile=info`

Deep stage timings and reference machine notes: [docs/internal/profile.md](docs/internal/profile.md) · corpora: [example/README.md](example/README.md).

### Gate A — cross-language regression

| Corpus | Test gate | Discover | Baseline (ref M3 Pro, +10%) |
|--------|-----------|----------|------------------------------|
| Linux kernel | `linux_cold_discover_within_baseline` | default | **145 s** wall |

```bash
cargo build --release --bin rgctl
cargo test --release --test cold_profile_gates linux_cold_discover_within_baseline -- --ignored --nocapture
```

### Gate B — language-scale (~10k source files)

Language changes add (or document) a language-filtered cold discover on a ~10k-file corpus. Record `wall_secs`, `nodes`, `functions`, `index_graph_build` from `[profile] discover summary`; add a gate in `tests/cold_profile_gates.rs` once baselined (+10%).

Fetch: `./scripts/fetch-profile-repos.sh`

| Language | Corpus | Path | Discover | Env override |
|----------|--------|------|----------|--------------|
| **C** | Linux | `example/linux` | default | `RGCTL_LINUX_REPO` |
| **C++** | LLVM | `example/llvm-project` | `-l cpp` on `clang/` | `RGCTL_LLVM_REPO` |
| **C#** | Roslyn | `example/roslyn` | `-l csharp` on `src/` | `RGCTL_ROSLYN_REPO` |
| **Go** | Kubernetes | `example/kubernetes` | `-l go` on `pkg/` `cmd/` | — |
| **Java** | metasfresh | `example/metasfresh-4.9.8b` | `--full` | `METASFRESH_REPO` |
| **JavaScript** | Node.js | `example/node` | `-l javascript` on `test/` | `RGCTL_NODE_REPO` |
| **PHP** | Magento 2 | `example/magento2` | `-l php` | `RGCTL_MAGENTO2_REPO` |
| **Python** | Home Assistant | `example/home-assistant` | `-l python` | `RGCTL_HOME_ASSISTANT_REPO` |
| **Ruby** | Discourse | `example/discourse` | `-l ruby` | — |
| **Puppet** | theforeman | `example/theforeman` | `-l puppet,erb,ruby -e spec,vendor` | `RGCTL_THEFOREMAN_REPO` — smoke corpus (~670 files, not O(10⁴)) |
| **ERB** | *(included in theforeman)* | `example/theforeman` | `-l erb` | *(same corpus — 122 .erb files, 2114 blocks)* |
| **Rust** | rustc | `example/rust` | `-l rust` | `RGCTL_RUST_REPO` |
| **TypeScript** | VS Code | `example/vscode` | `-l typescript` on `src/` | `RGCTL_VSCODE_REPO` |
| **Kotlin** | JetBrains/kotlin | `example/kotlin` | `-l kotlin` (sparse `libraries` `plugins` `analysis`) | `RGCTL_KOTLIN_REPO` |
| **Groovy** | Gradle | `example/groovy` | `-l groovy` | `RGCTL_GROOVY_REPO` |

File counts are approximate (goal **O(10⁴)** sources). Exclude `vendor/`, `node_modules/`, `target/`, `third_party/`.

---

## Profiles, tests, and benches

### Cargo profiles

| Profile | When |
|---------|------|
| default / `dev` | Iterate, unit tests |
| `--release` | Discover wall times, cold gates, any published timing |
| `cargo bench` (`[profile.bench]`) | Criterion microbenchmarks |

### Tests (run what you touched)

| Kind | Command | Practice |
|------|---------|----------|
| Workspace | `cargo test` | Default before merge for touched crates |
| Release CLI goldens | `cargo test --release --test subprocess_golden_path` (and related) | CLI surface changes |
| Cold profile gates | `cargo test --release --test cold_profile_gates -- --ignored --nocapture --test-threads=1` | Perf / extraction / ingest; **Gate A** for scale-sensitive work |
| Dashboard / lang | `dashboard_*`, langfeature / ecommerce fixture tests | When that path changes |
| Corpora | `./scripts/fetch-profile-repos.sh` | Before ignored gates needing `example/` |

Warm or partial `.rgctl/` **invalidates** cold timings.

### Benches

| Target | Command |
|--------|---------|
| Workspace | `cargo bench` — `parsing`, `graph`, `graph_benchmarks`, `analysis_benchmarks`, `centrality_benchmarks`, `community_benchmarks`, `blast_radius_benchmarks` |
| Snapshot diff | `cargo bench -p rgctl-graph --bench snapshot_diff` |
| Cold diff (linux) | `./scripts/prepare-linux-diff-snapshots.sh` then `linux_cold_diff_within_baseline` — see [profile.md](docs/internal/profile.md#cold-diff-profile-linux-two-ref-pair) |

Baselines and notes: [docs/internal/profile.md](docs/internal/profile.md#snapshot-diff-micro-benchmarks).

**Cold diff ≠ cold discover.** Diff gate times mmap open + digest + `diff_snapshots` on a prepared v7.1↔HEAD pair under `example/linux/.rgctl-diff/`.

---

## Must-read documents

| Doc | Why |
|-----|-----|
| [docs/analysis-architecture.md](docs/analysis-architecture.md) | Graph tiers, spill, CSR |
| [docs/design/blast-radius-design.md](docs/design/blast-radius-design.md) | Reachability / SCC |
| [docs/internal/profile.md](docs/internal/profile.md) | Cold profile deep dive |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Setup, tests, PR norms |
| [docs/contributor-checklist.md](docs/contributor-checklist.md) | Language / feature checklist |
| [docs/language-testing-guide.md](docs/language-testing-guide.md) | New-language test locations, asserts, Tier 1 matrix |
| [docs/guides/semantic-search.md](docs/guides/semantic-search.md) | Embedders (if touching semantic) |
| [docs/releasing.md](docs/releasing.md) | Version bump + GitHub Release tags |
| [openspec/changes/_shared/starting-context.md](openspec/changes/_shared/starting-context.md) | OpenSpec pointer (canonical policy is this file) |

---

## Releases

When asked to cut or bump a release, use the lockstep tooling — full detail: [docs/releasing.md](docs/releasing.md).

| Rule | Detail |
|------|--------|
| **One version** | SSOT is `[workspace.package] version` in root `Cargo.toml`. Crates use `version.workspace = true`. Do **not** sed/`version =` across every crate by hand. |
| **Bump TOMLs only** | `./scripts/bump-version.sh patch` (or `minor` / `major` / `X.Y.Z`). Syncs workspace version, `[workspace.dependencies]` path pins, and README release links. |
| **Bump + tag + push** | `cargo release patch --workspace` (dry-run), then `--execute` when the user wants commit/tag/push. Config: [`release.toml`](release.toml) (`shared-version`, `publish = false`, tag `v{{version}}`). Needs a **clean** git tree. |
| **Tools** | `cargo install cargo-edit cargo-release --locked` if missing. |
| **GitHub Release** | Pushing `v*` runs [`.github/workflows/release.yml`](.github/workflows/release.yml) (binaries). Add `docs/releases/vX.Y.Z.md` for curated notes. |
| **No crates.io** | `publish = false` — do not `cargo publish` unless the user explicitly asks to enable it. |
| **Commits / tags / push** | Only when the user explicitly requests them (same standing rule as other git ops). Prefer preparing the bump + release notes and stopping for the user to commit/sign/tag if they GPG-sign locally. |

---

## Build and day-to-day commands

```bash
cargo build --release --bin rgctl
./target/release/rgctl --version
cargo test
```

Dashboard UI changes:

```bash
./scripts/build-dashboard.sh   # or dashboard/ npm ci && npm run build
cargo build --release
```

Code-daemon / ONNX weights: `git lfs pull` when using that embedder feature.

Dogfood fixtures: `rgctl-tests/` (e.g. ecommerce-*). Consumer agent pack: `rgctl install --skill --tools cursor`.

---

## See also

- [docs/README.md](docs/README.md) — docs hub
- [docs/agents/USER_AGENTS_TEMPLATE.md](docs/agents/USER_AGENTS_TEMPLATE.md) — paste into *other* repos
- [docs/json-api.md](docs/json-api.md) — JSON schemas for CLI output
