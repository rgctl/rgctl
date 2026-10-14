#!/usr/bin/env bash
# Phase gate: build the dashboard embed, then run the tiny-fixture and golden-repo tests.
#
# Usage:
#   ./scripts/test-dashboard-golden.sh
#   RGCTL_DASHBOARD_GOLDEN_REPO=/path/to/repo ./scripts/test-dashboard-golden.sh
#
# The golden half is skipped (not failed) when the corpus is absent, so the tiny-fixture
# gate still runs on machines that have not fetched it.
#
# Requires: wasm-pack + Node.js for the bundle build, and a corpus for the second half.
# Referenced by docs/design/dashboard-design.md and docs/contributor-checklist.md.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Defaults to a corpus under `example/`, which is where `scripts/fetch-profile-repos.sh` puts the repos the
# Gate B table in AGENTS.md documents. It previously defaulted to an absolute path on one developer's
# machine (`/Users/sshaaf/git/java/gbuilder` — also pre-rename naming), so on every other machine the
# "golden" half silently skipped and the gate reported success without testing anything.
GOLDEN="${RGCTL_DASHBOARD_GOLDEN_REPO:-$ROOT/example/metasfresh-4.9.8b}"
if [[ ! -d "$GOLDEN" ]]; then
  for candidate in "$ROOT"/example/*; do
    [[ -d "$candidate" ]] || continue
    GOLDEN="$candidate"
    break
  done
fi

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
