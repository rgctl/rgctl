#!/usr/bin/env bash
# Manual gate: metasfresh example with discover --all (CFG/PDG + dashboard taint export).
# Expect 30+ minutes on first run (~128k Java functions).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="${RBUILDER_METASFRESH_REPO:-$ROOT/example/metasfresh-4.9.8b}"

echo "==> metasfresh repo: $REPO"
if [[ ! -d "$REPO" ]]; then
  echo "error: metasfresh example missing at $REPO"
  exit 1
fi

"$ROOT/scripts/build-dashboard.sh"
cd "$ROOT"
cargo build --release

echo "==> discover . --all (this may take a long time)…"
/usr/bin/time -p target/release/rbuilder -r "$REPO" discover . --all

cargo test --release --test dashboard_metasfresh -- --ignored --nocapture
cargo test --release --test dashboard_metasfresh metasfresh_dashboard_bundle_when_cache_present -- --nocapture

echo "==> manual preview:"
echo "    cd $REPO/.rbuilder/dashboard && python3 -m http.server 8765"
