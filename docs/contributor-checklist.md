# Contributor checklist

Single hub for **adding or updating a language**, **config/markup plugins**, and **shipping a feature change** — including which tests to run before opening a PR.

This doc **does not replace** the deep guides it links to. Use it to pick a path, run the right tests, and paste the commands you ran in your PR.

**Setup and clone:** [CONTRIBUTING.md](../CONTRIBUTING.md) · **Crate map:** [Code_structure.md](Code_structure.md)

---

## 0. Before you code

| Step | Action |
|------|--------|
| Issue | Open or claim an issue (language: [Language Support Request](../.github/ISSUE_TEMPLATE/language_request.md); feature: [Feature request](../.github/ISSUE_TEMPLATE/feature_request.md)). |
| Tier / path | Decide Tier 1 vs 2 vs 3, markup, config, or feature — see [§1](#1-choose-your-path). |
| Design | Large features: skim or add a note under [design/](design/README.md). Tier 1 languages: read [tier-1-language-support.md](tier-1-language-support.md) Layers A–F first. |
| Crate map | Know where your change lives — [Code_structure.md](Code_structure.md). |
| Commits | Include **DCO sign-off** on every commit — [§6](#6-documentation--pr). |

---

## 1. Choose your path

| Path | When | Deep guide |
|------|------|------------|
| **Tier 1 language** | Custom `LanguagePlugin`, full CFG/PDG/taint + Layer F CPG | [tier-1-language-support.md](tier-1-language-support.md) |
| **Tier 2 language** | Generic tree-sitter + `LanguageConfig` | [languages/README.md](languages/README.md) (coverage JSON + website) · scaffold in [tier-1 §3–4](tier-1-language-support.md#3-repository-layout) (Tier 2 uses `config.rs`) |
| **Tier 3 language** | Regex patterns only | [languages/README.md](languages/README.md) |
| **Config formats** | JSON, YAML, TOML, properties, … | `crates/rgctl-config-formats` |
| **Markup (Markdown)** | Doc context graph (not Tier 1/2) | [markdown-context.md](markdown-context.md) |
| **Feature change** | CLI, analysis, graph, dashboard, semantic, … | [§4](#4-feature-updates) + matching [design/](design/README.md) doc |

---

## 2. Programming languages

**Testing (where to add tests, what to assert, PR checklist):** → **[language-testing-guide.md](language-testing-guide.md)**

### Scaffold (all tiers)

1. Add `crates/rgctl-lang-{id}/` and register in `crates/rgctl-languages/`.
2. Update [`languages.toml`](../languages.toml) (extensions, `handler`, kinds).
3. Wire workspace `Cargo.toml`.

Full layout and naming: [tier-1-language-support.md §3–4](tier-1-language-support.md#3-repository-layout).

### Tier 2 / Tier 3 — minimum tests

| Tier | Run before PR |
|------|----------------|
| **Tier 2** | Plugin unit tests in `crates/rgctl-lang-{id}/`; `cargo test` on touched crates; optional fixture under `tests/fixtures/` — details in [language-testing-guide §3](language-testing-guide.md#3-what-to-add-for-a-new-language) |
| **Tier 3** | Same as Tier 2; no CFG/taint/dashboard gates |

Promoting Tier 2 → Tier 1: [tier-1 §5](tier-1-language-support.md#5-promoting-tier-2--tier-1).

### Tier 1 — test matrix (Layers E + F)

Layer **definitions** (A–F prose, F1–F7 table, honesty limits): [tier-1 §2](tier-1-language-support.md#2-capability-checklist--fully-supported).  
**Where to add each test + what to assert:** [language-testing-guide.md](language-testing-guide.md).  
Copy-paste **PR checklist** block: [tier-1 §7](tier-1-language-support.md#7-pr-submission-checklist) · [language-testing-guide §7](language-testing-guide.md#7-checklist-copy-into-pr).

| Gate | Layer | Command / location |
|------|-------|-------------------|
| AST coverage (build) | A | `cargo check -p rgctl-languages` warns on grammar/`*-ast-coverage.json` drift; `RGCTL_AST_COVERAGE_STRICT=1` fails |
| E1 Plugin symbols + `Calls` | E | `cargo test -p rgctl-lang-{id}` |
| E2 CFG branching + loop | E | `cargo test --test {id}_cfg_analysis` · `cargo test -p rgctl-analysis cfg_builder` |
| E3 Taint source→sink | E | `cargo test --test {id}_taint` · pack in `rules/taint/{id}.yaml` |
| E4 Fixture / langfeatures | E | `cargo test --test {id}_langfeatures` |
| E5 Dashboard bundle | E | `cargo test --release --test dashboard_ecommerce_{id}` + shared [dashboard_harness.rs](../tests/dashboard_harness.rs) |
| E6 Graph correctness + invariants | E | `cargo test --test graph_correctness` (`ecommerce-{id}/correctness/expected-facts.json`) |
| E7 Stable-key / determinism QE | E | `cargo test --test stable_key_invariance_qe` · `--test graph_determinism_qe` |
| E8 Workspace clean | E | [§5 standard test workflow](#5-standard-test-workflow) |
| F6 Field-write golden | F | `crates/rgctl-analysis/src/field_write.rs` — `{id}_cfg_captures_field_write_and_query` |
| Gate B cold discover | scale | [AGENTS.md](../AGENTS.md) Gate B · `tests/cold_profile_gates.rs` |

**Dashboard gates by language** (release mode; external fixture repos — set `RGCTL_*_REPO` if needed):

| Language | Test target |
|----------|-------------|
| Go | `dashboard_ecommerce_go` |
| Java | `dashboard_gbuilder` (gbuilder golden) |
| C# | `dashboard_ecommerce_csharp` |
| C | `dashboard_ecommerce_c` |
| C++ | `dashboard_ecommerce_cpp`, `cpp_langfeatures` |
| Python | `dashboard_ecommerce_python` |
| Rust | `dashboard_ecommerce_rust` |
| JavaScript | `dashboard_ecommerce_javascript` |
| TypeScript | `dashboard_ecommerce_typescript` |
| PHP | `dashboard_ecommerce_php` |
| Ruby | `dashboard_ecommerce_ruby` |

Fast dashboard smoke (tiny in-tree fixture): `cargo test --test dashboard_bundle`.

Parity snapshot: [tier-1 §8](tier-1-language-support.md#8-current-parity-snapshot-2026-07).

---

## 3. Config / markup

### Config format plugins

- Code: `crates/rgctl-config-formats`
- Tier table / coverage SSOT: [languages/README.md](languages/README.md) (config formats do not run CFG/PDG)

Run workspace tests touching the format crate; add fixture tests if you change extraction behavior.

### Markdown context graph

Deep guide: [markdown-context.md](markdown-context.md). Fixture: `tests/fixtures/markdown-context/`.

| Gate | Command |
|------|---------|
| CLI discover + markdown queries | `cargo test --test markdown_context_cli` |
| In-memory spec matrix | `cargo test -p rgctl-extraction markdown_spec_coverage` |
| Extraction unit tests | `cargo test -p rgctl-lang-markdown` · `cargo test -p rgctl-extraction` |

Optional cold profile (large corpus): [markdown-context.md § Cold profile](markdown-context.md#cold-profile-kuberneteswebsite).

---

## 4. Feature updates

Map **what you touched** → **tests to run**. Design detail lives in [design/README.md](design/README.md).

| Touch area | Primary tests | Notes |
|------------|---------------|-------|
| CLI JSON serializers | `cargo test --test cli_output` | [cli-io-sanity-qe.md](cli-io-sanity-qe.md) Layer 1 |
| CLI subprocess / flags | `cargo test --release --test subprocess_golden_path` · `--test all_commands_sanity` | Layers 2–3 |
| Blast radius / policy | `cargo test --test blast_radius` · `--release --test blast_radius_perf` | [blast-radius-design.md](design/blast-radius-design.md) |
| Semantic search | `cargo test --test semantic_search_qe` · `semantic_audit` · `semantic_boundary` | [semantic-search-design.md](design/semantic-search-design.md) |
| Graph / metrics / communities | `cargo test --test graph_correctness` · `map_collision_qe` · `cross_feature_qe` · `community_audit` | QE suite |
| CFG / PDG / slice | `cargo test --test slicing` · `with_cfg_cli` · `dominance` | [cfg-design.md](design/cfg-design.md) · [pdg-design.md](design/pdg-design.md) |
| Taint | `cargo test --test taint_analysis` · language `*_taint.rs` | [taint-analysis-design.md](design/taint-analysis-design.md) |
| Hybrid CPG (`cpg` CLI) | `field_write` unit tests · `with_cfg_cli` | [hybrid-cpg-plan.md](design/hybrid-cpg-plan.md) |
| Migration planner | `cargo test --test migration_plan_cli` · `with_dashboard_cli` | [migration-planner-design.md](design/migration-planner-design.md) |
| CI policy `check` | subprocess golden paths · `graph_projections` | [ci-policy-checks-design.md](design/ci-policy-checks-design.md) |
| HTTP `serve` | `cargo test --test http_serve` | [HTTP Server and Dashboard](guides/http-server-and-dashboard.md) |
| Dashboard export / UI | `cargo test dashboard_harness` · `./scripts/test-dashboard-golden.sh` | [dashboard-design.md](dashboard-design.md) |
| User-guide workflow | `cargo test --test user_guide_scenarios` (needs release `rgctl`, `jq`) | [user-guide.md](user-guide.md) |
| Core integration | `cargo test --test integration_core_features` · `bundles` | Edge extraction + persistence |

### Dashboard PR checklist

When changing `dashboard/` or `crates/rgctl-dashboard/`:

- [ ] Update [dashboard-design.md](dashboard-design.md) implementation status
- [ ] `./scripts/test-dashboard-golden.sh` passes
- [ ] Full checklist: [dashboard-design § PR checklist](dashboard-design.md#pr-checklist-every-phase)

---

## 5. Standard test workflow

Run before opening a PR (add path-specific targets from [§2](#2-programming-languages), [§3](#3-config--markup), or [§4](#4-feature-updates)):

```bash
cargo fmt --all -- --check

cargo clippy --lib --bins -- -D warnings
cargo clippy -p rgctl-analysis -- -D warnings
cargo clippy -p rgctl-graph -p rgctl-core -- -D warnings

cargo test --workspace --lib --bins --tests

cargo build --release -p rgctl
CARGO_BIN_EXE_rgctl="$PWD/target/release/rgctl" \
  cargo test --test user_guide_scenarios

cargo test --test map_collision_qe
cargo test --test graph_correctness
cargo test --test semantic_search_qe
cargo test --test cross_feature_qe

cargo test --test cli_output
cargo test --release --test subprocess_golden_path
cargo test --release --test all_commands_sanity

cargo test --release --test blast_radius_perf
```

Optional release-mode dashboard gates: add `--release --test dashboard_*` targets from [§2](#2-programming-languages).

CLI I/O layer reference: [cli-io-sanity-qe.md](cli-io-sanity-qe.md). Workflow mirror: [.github/workflows/ci.yml](../.github/workflows/ci.yml).

---

## 6. Documentation & PR

### Doc updates

| Change type | Update |
|-------------|--------|
| User CLI | [user-guide.md](user-guide.md) · validate with `cargo test --test user_guide_scenarios` |
| Contribute (agent README) | [AGENTS.md](../AGENTS.md) |
| Use rgctl / JSON | [USER_AGENTS_TEMPLATE](agents/USER_AGENTS_TEMPLATE.md) · [json-api.md](json-api.md) · [agent-recipes.md](agent-recipes.md) · [agent-skill](guides/agent-skill.md) |
| Languages (coverage JSON → website) | [languages/README.md](languages/README.md) |
| Language tests (new / Tier 1) | [language-testing-guide.md](language-testing-guide.md) |
| Dashboard UX | [dashboard-user-guide.md](dashboard-user-guide.md) |
| New capability | Matching doc in [design/](design/README.md) |

### Signed commits and DCO

Every commit in a PR must include **DCO sign-off** — use `-s` / `--signoff` so the commit message contains:

   ```text
   Signed-off-by: Your Name <your.email@example.com>
   ```

The sign-off certifies agreement with the [Developer Certificate of Origin (DCO)](https://developercertificate.org/).

**One-shot:**

```bash
git commit -s -m "your message"
```

Verify before push:

```bash
git log -1 --format=%B | grep -i '^Signed-off-by:'
```

### Open the PR

1. Branch from `main`.
2. Fill in [.github/PULL_REQUEST_TEMPLATE.md](../.github/PULL_REQUEST_TEMPLATE.md).
3. List **exact commands** you ran (see [§5](#5-standard-test-workflow)).
4. Tier 1 language PRs: paste the checklist from [tier-1 §7](tier-1-language-support.md#7-pr-submission-checklist).

---

## 7. Decision tree

```text
                    ┌─────────────────┐
                    │  What changed?  │
                    └────────┬────────┘
                             │
         ┌───────────────────┼───────────────────┐
         ▼                   ▼                   ▼
   ┌───────────┐      ┌─────────────┐     ┌─────────────┐
   │ Language  │      │ Markup /    │     │ Feature /   │
   │ plugin    │      │ config      │     │ infra       │
   └─────┬─────┘      └──────┬──────┘     └──────┬──────┘
         │                   │                   │
    Tier 1?              Markdown?           See §4 table
    ├─ yes → §2          ├─ yes → §3         + design doc
    │   Tier 1 matrix    │   markdown_*      │
    └─ no → §2           └─ else config      Dashboard?
       Tier 2/3 min          formats              │
                              crate tests     §4 dashboard
                                                checklist
         │                   │                   │
         └───────────────────┴───────────────────┘
                             │
                             ▼
              §5 standard test workflow
                             │
                             ▼
              §6 signed commits + DCO + PR template
```

---

## See also

- [CONTRIBUTING.md](../CONTRIBUTING.md) — clone, build, test overview
- [tier-1-language-support.md](tier-1-language-support.md) — Layer A–F depth
- [cli-io-sanity-qe.md](cli-io-sanity-qe.md) — CLI test layers
- [dashboard-design.md](dashboard-design.md) — WASM export pipeline
