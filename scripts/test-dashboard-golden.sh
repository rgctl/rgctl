#!/usr/bin/env bash
# Phase gate: build dashboard embed + run gbuilder golden-repo test.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GOLDEN="${RBUILDER_DASHBOARD_GOLDEN_REPO:-/Users/sshaaf/git/java/gbuilder}"

echo "==> golden repo: $GOLDEN"
if [[ ! -d "$GOLDEN" ]]; then
  echo "error: golden repo missing at $GOLDEN"
  exit 1
fi

"$ROOT/scripts/build-dashboard.sh"
cd "$ROOT"
cargo build --release
cargo test --release --test dashboard_bundle --test dashboard_gbuilder -- --nocapture

echo "==> manual preview:"
echo "    cd $GOLDEN/.rbuilder/dashboard && python3 -m http.server 8765"
