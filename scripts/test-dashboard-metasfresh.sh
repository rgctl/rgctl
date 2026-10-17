#!/usr/bin/env bash
# Manual gate: metasfresh example with discover --with-cfg --with-security --with-taint
# (CFG/PDG + dashboard taint export).
#
# Expect 30+ minutes on a first run (~128k Java functions). The checkout comes from
# ./scripts/fetch-profile-repos.sh; this script skips (rather than fails) when it is absent.
#
# Usage:
#   ./scripts/test-dashboard-metasfresh.sh
#   RGCTL_METASFRESH_REPO=/path/to/metasfresh ./scripts/test-dashboard-metasfresh.sh
#
# Requires: wasm-pack + Node.js for the bundle build, and the metasfresh checkout.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

REPO="${RGCTL_METASFRESH_REPO:-$ROOT/example/metasfresh-4.9.8b}"

log() { echo "[test-dashboard-metasfresh] $*"; }

if [[ ! -d "$REPO" ]]; then
  log "skip: metasfresh example not found at $REPO"
  log "      fetch it with ./scripts/fetch-profile-repos.sh, or set RGCTL_METASFRESH_REPO"
  exit 0
fi

log "building dashboard bundle (scripts/build-dashboard.sh)"
"$ROOT/scripts/build-dashboard.sh"

log "release build"
cargo build --release

log "discover . --with-cfg --with-security --with-taint in $REPO (this takes a long time)"
/usr/bin/time -p "$ROOT/target/release/rgctl" -r "$REPO" discover . --with-cfg --with-security --with-taint

log "metasfresh dashboard gates"
cargo test --release --test dashboard_metasfresh -- --ignored --nocapture
cargo test --release --test dashboard_metasfresh metasfresh_dashboard_bundle_when_cache_present -- --nocapture

log "preview: cd $REPO/.rgctl/dashboard && python3 -m http.server 8765"
