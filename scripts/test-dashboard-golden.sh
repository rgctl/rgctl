#!/usr/bin/env bash
# Phase gate: build the dashboard embed, then run the tiny-fixture and gbuilder golden tests.
#
# Usage:
#   ./scripts/test-dashboard-golden.sh
#   RGCTL_DASHBOARD_GOLDEN_REPO=/path/to/gbuilder ./scripts/test-dashboard-golden.sh
#
# The gbuilder half is skipped (not failed) when the checkout is absent, so the
# tiny-fixture gate still runs on machines that do not have it.
#
# Requires: wasm-pack + Node.js for the bundle build, and a golden repo checkout
# for the second half. Referenced by docs/design/dashboard-design.md and
# docs/contributor-checklist.md.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

GOLDEN="${RGCTL_DASHBOARD_GOLDEN_REPO:-/Users/sshaaf/git/java/gbuilder}"

log() { echo "[test-dashboard-golden] $*"; }

log "building dashboard bundle (scripts/build-dashboard.sh)"
"$ROOT/scripts/build-dashboard.sh"

log "release build"
cargo build --release

log "tiny-fixture bundle gate (tests/dashboard_bundle.rs)"
cargo test --release --test dashboard_bundle -- --nocapture

if [[ -d "$GOLDEN" ]]; then
  log "gbuilder golden gate: $GOLDEN"
  cargo test --release --test dashboard_gbuilder -- --nocapture
  log "preview: cd $GOLDEN/.rgctl/dashboard && python3 -m http.server 8765"
else
  log "skip: golden repo not found at $GOLDEN (set RGCTL_DASHBOARD_GOLDEN_REPO)"
fi

log "done"
