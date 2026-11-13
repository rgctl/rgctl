# Contributing to rgctl

Thanks for helping improve rgctl. This guide covers local setup, tests, and where to put changes.

**Documentation map:** [docs/README.md](docs/README.md)

---

## Contributor tracks

Pick a lane — each links to the checklist / architecture docs you will touch most:

| Track | Start here |
|-------|------------|
| **Language Plugins & AST Coverage** | [contributor-checklist.md](docs/contributor-checklist.md) · [tier-1-language-support.md](docs/tier-1-language-support.md) · [tier-2-language-support.md](docs/tier-2-language-support.md) · [language-testing-guide.md](docs/language-testing-guide.md) · [languages/README.md](docs/languages/README.md) |
| **Declarative Taint Rules** | [Vulnerability and Osv Analysis](https://rgctl.io/docs/guides/vulnerability-and-osv-analysis/) · [analysis-architecture.md](docs/analysis-architecture.md) · [policy-format.md](docs/policy-format.md) |
| **Agent Host Adapters & Skills** | [Agent Skill](https://rgctl.io/docs/guides/agent-skill/) · [`skills/rgctl/`](skills/rgctl/) · [USER_AGENTS_TEMPLATE.md](docs/agents/USER_AGENTS_TEMPLATE.md) |
| **Graph & CPG Algorithms** | [analysis-architecture.md](docs/analysis-architecture.md) · [graph-storage-architecture.md](docs/graph-storage-architecture.md) · [Hybrid Cpg](https://rgctl.io/docs/guides/hybrid-cpg/) · [Blast Radius Analysis](https://rgctl.io/docs/guides/blast-radius-analysis/) |

---

## Prerequisites

- **Rust** stable (via [rustup](https://rustup.rs/))
- **Node.js 18+** and npm (dashboard UI only)
- **git**

Optional: **Playwright** (dashboard browser tests) — installed via `dashboard/` npm scripts.

---

## Clone and build

```bash
git clone https://github.com/rgctl/rgctl.git
cd rgctl
cargo build --release
./target/release/rgctl --version
```

### Dashboard (when changing `dashboard/`)

```bash
cd dashboard
npm ci
npm run build
cd ..
cargo build --release   # embeds dashboard/dist
```

WASM worker:

```bash
# from repo root — see dashboard/wasm/ or project scripts if present
cargo build -p rgctl-wasm --target wasm32-unknown-unknown --release
```

---

## Pull request CI (maintainer-gated & split by tier)

GitHub Actions CI does **not** run automatically on every PR, and heavy QE/perf suites are split into separate manual workflows (`ci-qe.yml` and `ci-perf.yml`) and disabled on PRs to conserve Actions minutes:

| Workflow | What it runs | How to trigger |
|----------|--------------|----------------|
| [`.github/workflows/ci-lint.yml`](.github/workflows/ci-lint.yml) | Commit policy, `cargo fmt`, `cargo check`, `cargo clippy (--all-targets)` | Label **`ci:lint`** or **`ci`** on PR (or Actions → Run workflow) |
| [`.github/workflows/ci.yml`](.github/workflows/ci.yml) | Workspace `--lib --bins` unit tests + fast core integration tests | Label **`ci:test`** or **`ci`** on PR (or Actions → Run workflow) |
| [`.github/workflows/ci-qe.yml`](.github/workflows/ci-qe.yml) | Heavy QE/E2E suites + `RGCTL_AST_COVERAGE_TRACK=1` *(disabled on PRs)* | Manual only (Actions → Run workflow) or `./scripts/preflight-pr.sh --qe` |
| [`.github/workflows/ci-perf.yml`](.github/workflows/ci-perf.yml) | `--release` CLI golden paths, scale/perf gates & `cargo bench --no-run` *(disabled on PRs)* | Manual only (Actions → Run workflow) or `./scripts/preflight-pr.sh --perf` |

### Local pre-flight PR check (`./scripts/preflight-pr.sh`)

Before opening or pushing to a PR, run the pre-flight check script (mirrors the split CI tiers, including `RGCTL_AST_COVERAGE_STRICT=1`, AI commit trailer checks, DCO sign-off checks, `cargo fmt`, all CI `cargo clippy` gates, and unit/core integration tests):

```bash
# Tier 1 only (~15–30s incremental): commit checks + fmt + all CI clippy gates (--all-targets)
./scripts/preflight-pr.sh --lint

# Default pre-PR check (Tier 1 + Tier 2, matching active PR CI: lint + unit & core integration tests)
./scripts/preflight-pr.sh

# Opt-in heavier tiers (disabled on PR CI; run locally when touching relevant areas):
./scripts/preflight-pr.sh --qe     # Tier 3: QE/E2E suites + AST handler coverage tracking
./scripts/preflight-pr.sh --perf   # Tier 4: --release CLI golden paths, scale/perf gates & bench smoke
./scripts/preflight-pr.sh --full   # All 4 tiers (lint + test + qe + perf)
```

Without `ci` (or `ci:lint` / `ci:test`), the Checks tab shows a single green **awaiting maintainer `ci` label** notice. Adding the label re-runs the workflow and starts the corresponding tier.

**Code review “Approve”** (Files changed → Review changes → Approve) is separate from CI. You need write access on `rgctl/rgctl`; it is unrelated to the Actions “Approve and run workflows” button (that only appears for pending *fork* workflow approvals).

---

## Running tests

> **Note (`rgctl gql` is deprecated):** Do **not** write new tests or correctness harnesses against `rgctl gql`. Always assert against canonical structured JSON verbs (`rgctl -f json find`, `callers`, `callees`, `relations`, `inventory`, `blast-radius`, `inspect`, `slice`, `cpg`, `security`, `clones`, `semantic`, `review`) or typed Rust graph/snapshot APIs.

```bash
# Unit / integration (workspace)
cargo test

# Release-mode CLI golden paths (slower)
cargo test --release --test subprocess_golden_path
cargo test --release --test all_commands_sanity

# Dashboard bundle assertions
cargo test dashboard_harness

# Golden repos (optional, long)
./scripts/validate-golden-repos.sh
# Discover timing baselines (manual): cargo test --release --test discover_perf_baselines -- --ignored --nocapture
```

### Dashboard Playwright scripts

Serve a discovered dashboard, then:

```bash
cd dashboard
DASHBOARD_URL=http://127.0.0.1:8765/ node scripts/test-guide-cli.mjs
```

---

## Project layout (short)

| Area | Path |
|------|------|
| CLI entry | `src/cli/` |
| Analysis (CFG, PDG, taint) | `crates/rgctl-analysis/` |
| Graph storage | `crates/rgctl-graph/` |
| Dashboard export | `crates/rgctl-dashboard/` |
| Browser UI | `dashboard/src/` |
| WASM engine | `crates/rgctl-wasm/` |
| Language plugins | `crates/rgctl-lang-*/` |

Full map: [docs/Code_structure.md](docs/Code_structure.md)

---

## Releasing (version bump)

Lockstep workspace version via `[workspace.package]` + `version.workspace = true`.
See **[docs/releasing.md](docs/releasing.md)** for `./scripts/bump-version.sh` and `cargo release`.

---

## Adding or improving a language / feature

Use the hub checklist for path choice, test matrices, and pre-PR commands:

**[docs/contributor-checklist.md](docs/contributor-checklist.md)**

Tier 1 depth (Layers A–F): [docs/tier-1-language-support.md](docs/tier-1-language-support.md) · language matrix SSOT: `crates/rgctl-lang-*/{id}-ast-coverage.json` ([docs/languages/README.md](docs/languages/README.md))

---

## Documentation changes

- **User-facing:** `docs/Introduction.md`, `docs/user-guide.md`, [`https://rgctl.io/docs/guides/http-server-and-dashboard/`](https://rgctl.io/docs/guides/http-server-and-dashboard/)
- **Agents (contribute to rgctl):** root [`AGENTS.md`](AGENTS.md) (starting-context, profiles/tests/benches) · [`docs/json-api.md`](docs/json-api.md)
- **Agents (use rgctl elsewhere):** [`https://rgctl.io/docs/guides/agent-skill/`](https://rgctl.io/docs/guides/agent-skill/) · [`docs/agents/USER_AGENTS_TEMPLATE.md`](docs/agents/USER_AGENTS_TEMPLATE.md)
- **Accuracy:** keep CLI examples aligned with `dashboard/scripts/validate-guide-cli-gbuilder.sh` where possible

---

## Pull requests

1. Branch from `main` (or the active integration branch).
2. Keep commits focused; match existing Rust style and `cargo fmt` / `clippy` expectations.
3. **Sign every commit** with **DCO sign-off** (`git commit -s`). See [Developer Certificate of Origin](https://developercertificate.org/) and [contributor checklist §6](docs/contributor-checklist.md#6-documentation--pr).
4. Run `./scripts/preflight-pr.sh` (or `--quick` + targeted tests from the [standard test workflow](docs/contributor-checklist.md#5-standard-test-workflow)) and list the commands you ran in the PR template.

### AI coding tools and authorship

We respect each contributor’s free will to use AI agents (Cursor, Claude Code, Copilot, and similar) when writing code. Using those tools is welcome.

What we expect in return:

- **You reviewed the change.** Every line in the PR should be something you understand and stand behind.
- **You own the PR.** The human author is responsible for correctness, licensing, security, and fit with this project — not the agent.
- **No AI co-author / assisted trailers in commits.** PR CI runs `scripts/check-no-ai-commit-trailers.py` on the PR commit range and fails on attribution trailers (`Co-Authored-By`, `Assisted-By`, `Generated-By`, `Made-With`, …), “Generated with …” banners, and known agent emails/bot identities. The catalog covers Cursor, Claude, Copilot, Codex/ChatGPT, Aider, Devin, Windsurf/Cascade, Gemini/Jules, Continue, Cline/Roo, Amazon Q, Cody, Replit, Goose, OpenHands, Antigravity, and more (`--list-agents` to print it). Commit as yourself (plus DCO `-s`); turn off agent co-author settings so the check stays green.

Local check before you push:

```bash
./scripts/check-no-ai-commit-trailers.py --range origin/main..HEAD
./scripts/check-no-ai-commit-trailers.py --list-agents   # optional
```

---

## Releases

Maintainers: [docs/releasing.md](docs/releasing.md)
