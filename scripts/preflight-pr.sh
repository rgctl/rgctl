#!/usr/bin/env bash
# Pre-flight PR check script — mirrors the split GitHub Actions workflows:
#   - Tier 1 (ci-lint.yml): commit policy, cargo fmt, cargo check, cargo clippy (--all-targets)
#   - Tier 2 (ci.yml):      workspace unit/bin tests + fast core integration tests
#   - Tier 3 (ci-qe.yml):   QE/E2E correctness suites + AST handler coverage (opt-in; disabled on PR CI)
#   - Tier 4 (ci-perf.yml): --release CLI golden paths, scale/perf gates & bench smoke (opt-in; disabled on PR CI)
#
# Usage:
#   ./scripts/preflight-pr.sh              # Default PR gate: Tier 1 (lint) + Tier 2 (unit & core tests)
#   ./scripts/preflight-pr.sh --lint       # Tier 1 only (ci-lint.yml; alias: --quick)
#   ./scripts/preflight-pr.sh --test       # Tier 1 + Tier 2 (ci.yml; same as default)
#   ./scripts/preflight-pr.sh --qe         # Opt-in: run Tier 3 (ci-qe.yml)
#   ./scripts/preflight-pr.sh --perf       # Opt-in: run Tier 4 (ci-perf.yml)
#   ./scripts/preflight-pr.sh --full       # Opt-in: run all 4 tiers (lint + test + qe + perf)
#   ./scripts/preflight-pr.sh --fix        # Run `cargo fmt --all` before checking formatting
#   ./scripts/preflight-pr.sh --base main  # Override base git ref for commit-range checks

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

RUN_LINT=1
RUN_TEST=1
RUN_QE=0
RUN_PERF=0
EXPLICIT_TIER=0
FIX_FMT=0
BASE_REF=""
FEATURES="${FEATURES:-}"

usage() {
  cat <<'EOF'
Usage: ./scripts/preflight-pr.sh [OPTIONS]

Tier selection (default: --lint + --test, matching active PR CI; --qe, --perf, --full are opt-in):
  --lint, --quick    Tier 1 only (.github/workflows/ci-lint.yml):
                     commit checks, cargo fmt, cargo check, and all cargo clippy gates (--all-targets)
  --test             Tier 1 + Tier 2 (.github/workflows/ci.yml):
                     lint + workspace unit/bin tests + fast core integration tests (default)
  --qe               Tier 3 (.github/workflows/ci-qe.yml — disabled on PR CI):
                     QE/E2E suites + RGCTL_AST_COVERAGE_TRACK=1
  --perf             Tier 4 (.github/workflows/ci-perf.yml — disabled on PR CI):
                     --release CLI golden paths, scale/perf gates, cargo bench --no-run
  --full             Run all 4 tiers (lint + test + qe + perf)

Other options:
  --fix              Run `cargo fmt --all` before checking formatting
  --base <git-ref>   Base ref for commit-range checks (default: auto-detect upstream/main, origin/main, or main)
  --features <list>  Cargo features to pass (mirrors CI FEATURES input; default: "")
  -h, --help         Show this help message
EOF
}

enable_custom_tiers() {
  if [[ "$EXPLICIT_TIER" -eq 0 ]]; then
    RUN_LINT=0
    RUN_TEST=0
    RUN_QE=0
    RUN_PERF=0
    EXPLICIT_TIER=1
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --lint|--quick)
      enable_custom_tiers
      RUN_LINT=1
      shift
      ;;
    --test)
      enable_custom_tiers
      RUN_LINT=1
      RUN_TEST=1
      shift
      ;;
    --qe)
      enable_custom_tiers
      RUN_QE=1
      shift
      ;;
    --perf)
      enable_custom_tiers
      RUN_PERF=1
      shift
      ;;
    --full)
      enable_custom_tiers
      RUN_LINT=1
      RUN_TEST=1
      RUN_QE=1
      RUN_PERF=1
      shift
      ;;
    --fix)
      FIX_FMT=1
      shift
      ;;
    --base)
      BASE_REF="${2:-}"
      shift 2
      ;;
    --features)
      FEATURES="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
# Match .github/workflows/ci*.yml: fail on grammar <-> *-ast-coverage.json drift
export RGCTL_AST_COVERAGE_STRICT=1

step() {
  printf '\n\033[1;34m==> %s\033[0m\n' "$1"
}

pass() {
  printf '\033[1;32m✓ %s\033[0m\n' "$1"
}

warn() {
  printf '\033[1;33m⚠ %s\033[0m\n' "$1"
}

# ==============================================================================
# Tier 1: Format, Clippy & Commit Policy (.github/workflows/ci-lint.yml)
# ==============================================================================
if [[ "$RUN_LINT" -eq 1 ]]; then
  if [[ -z "$BASE_REF" ]]; then
    for candidate in upstream/main origin/main main; do
      if git rev-parse --verify --quiet "$candidate" >/dev/null; then
        BASE_REF="$candidate"
        break
      fi
    done
  fi

  if [[ -n "$BASE_REF" ]]; then
    MERGE_BASE="$(git merge-base "$BASE_REF" HEAD 2>/dev/null || true)"
    if [[ -n "$MERGE_BASE" && "$MERGE_BASE" != "$(git rev-parse HEAD)" ]]; then
      step "Tier 1 (ci-lint): Checking commits in ${MERGE_BASE:0:8}..HEAD (no AI trailers & DCO sign-off)"
      python3 scripts/check-no-ai-commit-trailers.py --range "${MERGE_BASE}..HEAD" --require-dco
      pass "Commit range checks passed (no AI trailers, DCO sign-off present)"
    fi
  fi

  if [[ "$FIX_FMT" -eq 1 ]]; then
    step "Tier 1 (ci-lint): Formatting workspace (cargo fmt --all)"
    cargo fmt --all
  fi

  step "Tier 1 (ci-lint): cargo fmt --all -- --check"
  cargo fmt --all -- --check
  pass "Formatting check passed"

  step "Tier 1 (ci-lint): cargo check --locked --bin rgctl"
  cargo check --locked --bin rgctl
  pass "cargo check --locked --bin rgctl passed"

  step "Tier 1 (ci-lint): cargo clippy --features \"${FEATURES}\" --lib --bins -- -D warnings"
  cargo clippy --features "${FEATURES}" --lib --bins -- -D warnings
  pass "Workspace clippy (--lib --bins) passed"

  step "Tier 1 (ci-lint): cargo clippy -p rgctl-analysis --features \"${FEATURES}\" -- -D warnings"
  cargo clippy -p rgctl-analysis --features "${FEATURES}" -- -D warnings
  pass "rgctl-analysis clippy passed"

  step "Tier 1 (ci-lint): cargo clippy -p rgctl-graph -p rgctl-core --features \"${FEATURES}\" -- -D warnings"
  cargo clippy -p rgctl-graph -p rgctl-core --features "${FEATURES}" -- -D warnings
  pass "rgctl-graph & rgctl-core clippy passed"

  step "Tier 1 (ci-lint): Compile all unit & integration test targets (--no-run)"
  cargo test --features "${FEATURES}" --workspace --lib --bins --tests --no-run
  pass "All workspace unit & integration test targets compiled cleanly"
fi

# ==============================================================================
# Tier 2: Unit & Core Integration Tests (.github/workflows/ci.yml)
# ==============================================================================
if [[ "$RUN_TEST" -eq 1 ]]; then
  step "Tier 2 (ci): cargo test --features \"${FEATURES}\" --workspace --lib --bins --no-fail-fast"
  cargo test --features "${FEATURES}" --workspace --lib --bins --no-fail-fast

  step "Tier 2 (ci): Running core integration contracts"
  cargo test --features "${FEATURES}" --test cli_output
  cargo test --features "${FEATURES}" -p rgctl-extraction --test single_pass_complexity_parity
  cargo test --features "${FEATURES}" --test slicing --test taint_analysis
  cargo test --features "${FEATURES}" --test community_audit --test centrality_audit
  cargo test --features "${FEATURES}" --test incremental_performance_integration
  pass "Tier 2 unit & core integration tests passed"
fi

# ==============================================================================
# Tier 3: QE, E2E & AST Coverage (.github/workflows/ci-qe.yml — opt-in)
# ==============================================================================
if [[ "$RUN_QE" -eq 1 ]]; then
  step "Tier 3 (ci-qe): Scrubbing gitignored .rgctl caches in test fixtures"
  find rgctl-tests tests/fixtures -name ".rgctl" -type d -prune -exec rm -rf {} + 2>/dev/null || true

  step "Tier 3 (ci-qe): Running QE & E2E suites"
  cargo test --features "${FEATURES}" --test map_collision_qe
  cargo test --features "${FEATURES}" --test graph_correctness
  cargo test --features "${FEATURES}" --test semantic_search_qe
  cargo test --features "${FEATURES}" --test cross_feature_qe
  cargo test --features "${FEATURES}" --test graph_determinism_qe
  cargo test --features "${FEATURES}" --test stable_key_invariance_qe
  cargo test --features "${FEATURES}" --test columnar_zero_cold_touch_gate
  cargo test --features "${FEATURES}" --test e2e_workflow_equivalence
  cargo test --features "${FEATURES}" --test e2e_feature_contracts
  cargo test --features "${FEATURES}" --test inheritance_edges --test cross_language_mro_and_di

  step "Tier 3 (ci-qe): AST Handler Runtime Coverage Gate (RGCTL_AST_COVERAGE_TRACK=1)"
  RGCTL_AST_COVERAGE_TRACK=1 cargo test --features "${FEATURES}" \
    -p rgctl-ast-coverage \
    -p rgctl-languages \
    -p rgctl-lang-c \
    -p rgctl-lang-cpp \
    -p rgctl-lang-csharp \
    -p rgctl-lang-go \
    -p rgctl-lang-java \
    -p rgctl-lang-javascript \
    -p rgctl-lang-kotlin \
    -p rgctl-lang-php \
    -p rgctl-lang-python \
    -p rgctl-lang-ruby \
    -p rgctl-lang-rust \
    -p rgctl-lang-typescript \
    --tests
  pass "Tier 3 QE, E2E & AST coverage gates passed"
fi

# ==============================================================================
# Tier 4: Release Perf, Scale & Bench (.github/workflows/ci-perf.yml — opt-in)
# ==============================================================================
if [[ "$RUN_PERF" -eq 1 ]]; then
  step "Tier 4 (ci-perf): Release CLI golden paths & scale/perf gates"
  cargo test --release --features "${FEATURES}" --test subprocess_golden_path
  cargo test --release --features "${FEATURES}" --test all_commands_sanity
  cargo test --release --features "${FEATURES}" --test blast_radius_perf
  cargo test --release --features "${FEATURES}" --test scale_perf_ci_gate -- --nocapture
  cargo test --release --features "${FEATURES}" --test community_audit --test centrality_audit --test centrality_approx_scale -- --nocapture

  step "Tier 4 (ci-perf): Benchmark compilation smoke check (cargo bench --no-run)"
  cargo bench --no-run
  pass "Tier 4 release perf, scale & benchmark gates passed"
fi

printf '\n\033[1;32m============================================================\033[0m\n'
printf '\033[1;32m  rgctl pre-flight PR check PASSED (lint=%s test=%s qe=%s perf=%s)\033[0m\n' \
  "$RUN_LINT" "$RUN_TEST" "$RUN_QE" "$RUN_PERF"
printf '\033[1;32m============================================================\033[0m\n'
