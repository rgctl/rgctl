#!/usr/bin/env bash
# Golden-repo validation: release build → discover --all → serve → Playwright.
#
# Usage:
#   ./scripts/validate-golden-repos.sh
#   RBUILDER_DASHBOARD_GOLDEN_REPO=/path/to/gbuilder ./scripts/validate-golden-repos.sh
#
# Requires: Node.js + Playwright (dashboard/), golden repo checkouts, embedded dashboard dist.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

GBUILDER="${RBUILDER_DASHBOARD_GOLDEN_REPO:-/Users/sshaaf/git/java/gbuilder}"
METASFRESH="${RBUILDER_METASFRESH_REPO:-$ROOT/example/metasfresh-4.9.8b}"
SERVE_PORT="${RBUILDER_SERVE_PORT:-8080}"
DASHBOARD_URL="http://127.0.0.1:${SERVE_PORT}/"
GIT_SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
DATE="$(date -u +%Y-%m-%d)"

log() { echo "[validate-golden-repos] $*"; }

require_dir() {
  if [[ ! -d "$1" ]]; then
    log "skip: repo not found at $1"
    return 1
  fi
  return 0
}

run_discover_all() {
  local repo="$1"
  local langs="${2:-}"
  log "discover --all in $repo"
  local start=$SECONDS
  if [[ -n "$langs" ]]; then
    "$ROOT/target/release/rbuilder" -r "$repo" discover . --all --languages "$langs"
  else
    "$ROOT/target/release/rbuilder" -r "$repo" discover . --all
  fi
  local elapsed=$((SECONDS - start))
  echo "$elapsed"
}

append_baseline_row() {
  local repo="$1" nodes="$2" edges="$3" discover_s="$4" notes="$5"
  {
    echo ""
    echo "| ${DATE} | ${GIT_SHA} | ${repo} | ${discover_s} | — | ${notes} |"
  } >> "$ROOT/docs/performance-baselines.md"
}

log "Building dashboard dist (if script present)..."
if [[ -x "$ROOT/scripts/build-dashboard.sh" ]]; then
  "$ROOT/scripts/build-dashboard.sh"
elif [[ -x "$ROOT/dashboard/scripts/build-dashboard.sh" ]]; then
  "$ROOT/dashboard/scripts/build-dashboard.sh"
else
  log "no build-dashboard.sh — assuming dashboard/dist already embedded"
fi

log "Building release rbuilder..."
cargo build --release

GBUILDER_DISCOVER_S=""
METASFRESH_DISCOVER_S=""

if require_dir "$GBUILDER"; then
  GBUILDER_DISCOVER_S="$(run_discover_all "$GBUILDER" java)"
  cargo test --release --test dashboard_gbuilder discover_all_writes_dashboard_bundle_on_gbuilder -- --ignored --nocapture || true
fi

if require_dir "$METASFRESH"; then
  METASFRESH_DISCOVER_S="$(run_discover_all "$METASFRESH" "")"
  cargo test --release --test dashboard_metasfresh discover_all_writes_dashboard_bundle_on_metasfresh -- --ignored --nocapture || true
fi

serve_repo="${GBUILDER}"
if [[ ! -d "$serve_repo/.rbuilder/dashboard" && -d "$METASFRESH/.rbuilder/dashboard" ]]; then
  serve_repo="$METASFRESH"
fi

if [[ -d "$serve_repo/.rbuilder/dashboard" ]]; then
  log "Starting rbuilder serve on port ${SERVE_PORT} for ${serve_repo}"
  "$ROOT/target/release/rbuilder" -r "$serve_repo" serve --port "$SERVE_PORT" &
  SERVE_PID=$!
  cleanup() { kill "$SERVE_PID" 2>/dev/null || true; }
  trap cleanup EXIT

  export DASHBOARD_URL
  pushd "$ROOT/dashboard" >/dev/null
  for script in test-serve.mjs test-graph-tabs.mjs test-blast-sort.mjs; do
    if [[ -f "scripts/${script}" ]]; then
      log "Playwright: ${script}"
      node "scripts/${script}"
    fi
  done
  if [[ "$serve_repo" == "$GBUILDER" && -f scripts/test-migration-gbuilder.mjs ]]; then
    log "Playwright: test-migration-gbuilder.mjs"
    node scripts/test-migration-gbuilder.mjs
  fi
  popd >/dev/null
else
  log "skip Playwright: no dashboard bundle found"
fi

if [[ -n "$GBUILDER_DISCOVER_S" ]]; then
  append_baseline_row "gbuilder" "—" "—" "$GBUILDER_DISCOVER_S" "validate-golden-repos discover --all"
fi
if [[ -n "$METASFRESH_DISCOVER_S" ]]; then
  append_baseline_row "metasfresh" "—" "—" "$METASFRESH_DISCOVER_S" "validate-golden-repos discover --all"
fi

log "Optional: cargo bench --bench graph_benchmarks (record in docs/performance-baselines.md)"
log "Done."
