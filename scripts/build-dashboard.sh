#!/usr/bin/env bash
# Build both dashboard UIs (Vite) and the WASM engine, for embed + local preview.
#
# Produces:
#   dashboard/dist      — v1, the Preact dashboard. **No flag reaches it any more.** Since `--with-universe`
#                         became the only dashboard flag, v1 survives as a legacy hatch: it is served only
#                         when `RGCTL_DASHBOARD_LEGACY=1` is set, and that run prints "removed next release"
#   dashboard/dist-v2   — v2, the React universe dashboard, and the default: `discover --with-universe`
#                         exports it, and `serve --open` always serves this shell (swapping it in when an
#                         older bundle is on disk)
#
# Both are committed and both are embedded at compile time by `rgctl-dashboard`, so both must be
# regenerated in the same commit as any change under `dashboard/src`. A source change with a forgotten
# rebuild has no effect on a built binary while every test still passes.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DASH="$ROOT/dashboard"

# Recorded budgets, in KiB of *gzipped* JS + CSS, which is what actually ships in the binary and crosses a
# network. Measured on this tree rather than chosen: v1 (Preact, Sigma, graphology, CodeMirror, Bootstrap)
# measured 312 KiB when this gate was written and 321 KiB today; v2 (React, TanStack Router, Radix, Tailwind)
# was 131 KiB before the rooms — smaller than v1 despite adding React, because it does not carry CodeMirror's
# ~150 KiB or Sigma's ~29 KiB. Headroom is ~10%, enough for a dependency bump without silencing a real
# regression.
BUDGET_KIB_V1=345
# v2 was 145 KiB until the Architecture room arrived. The universe draws, so it carries three.js: measured on
# this tree, `dist-v2` is **300 KiB gzipped** against 137 before — a +163 KiB delta, dominated by three.js,
# which lands in the `OrbitControls` chunk (**131 KiB gzipped**) because that addon is what imports it; the
# scene chunk itself is 4 KiB, so the chunk names here describe entry points, not cost. The vendored,
# untree-shaken three.js copy the demos use is 430 KiB, so the bundled cost is about a third of it — which is
# why the number is measured here rather than quoted from the demos.
#
# The split is deliberate: the shell's first paint is the assets `index.html` references — **157 KiB gzipped**
# today (react 71, tanstack 38, index.v2 31, radix 9, css 5) — and three.js is fetched only when someone opens
# a room that draws. The gate sums the whole directory, so it is the total that is budgeted, not the initial
# payload.
#
# What the gate does **not** weigh: it globs `assets/*.js` and `assets/*.css`, so the ~193 KiB wasm engine
# (`assets/rgctl_wasm_bg-*.wasm`) is outside both budgets. If that engine ever needs a budget of its own, it
# needs a new check rather than a raised constant here.
# 330, not 320: the level's outgoing coupling is drawn with `Line2` (see `UniverseScene`'s
# `LEVEL_EDGE_WIDTH_PX`), which costs 7.2 KiB gzipped — measured, 312.85 → 320.07 — because WebGL ignores
# `LineBasicMaterial.linewidth`, so one-pixel lines cannot be thickened any other way. The old budget was 7.15 KiB
# above the tree, so this restores the same headroom rather than spending it.
BUDGET_KIB_V2=330

echo "==> wasm-pack (rgctl-wasm)"
WASM_JS="$DASH/wasm/rgctl_wasm.js"
if command -v wasm-pack >/dev/null 2>&1; then
  wasm-pack build "$ROOT/crates/rgctl-wasm" \
    --target web \
    --out-dir "$DASH/wasm" \
    --release \
    --no-opt
elif [[ -f "$WASM_JS" ]]; then
  echo "note: wasm-pack not found — reusing the engine already in dashboard/wasm/"
else
  cat >&2 <<'MSG'
error: wasm-pack is not installed and dashboard/wasm/ has no engine.

Refusing to continue, and deliberately not writing anything.

This script used to write a *throwing stub* here:

    export default async function init(){}
    export class EngineContext { constructor(){ throw new Error('wasm not built'); } }

`dashboard/wasm/` is gitignored apart from its package.json, so `rgctl_wasm.js` is
absent in every fresh checkout — which made the "only if missing" guard true every
time. `npm run build` then bundled the stub into dashboard/dist, so a contributor
without wasm-pack could replace the real ~197 KiB engine with a stub that throws at
runtime, commit it, and have the bundle look entirely fine.

Install wasm-pack (cargo install wasm-pack), or build the Rust side only with cargo.
MSG
  exit 1
fi

echo "==> npm run build (dashboard/ — v1)"
cd "$DASH"
npm install
npm run build

echo "==> npm run build:v2 (dashboard/ — v2)"
npm run build:v2

# --- bundle-size gate ------------------------------------------------------------------------------
#
# Gzipped, because that is the cost that ships. Measured over the built assets rather than the source, so
# the gate cannot be satisfied by moving code into a lazily-loaded chunk that is still requested on load.
size_kib() {
  local dir="$1" total=0 part
  for part in "$dir"/assets/*.js "$dir"/assets/*.css; do
    [[ -f "$part" ]] || continue
    total=$((total + $(gzip -9 -c "$part" | wc -c)))
  done
  echo $((total / 1024))
}

check_budget() {
  local label="$1" dir="$2" budget="$3" actual
  if [[ ! -d "$dir" ]]; then
    echo "error: $dir missing — $label did not build" >&2
    return 1
  fi
  actual="$(size_kib "$dir")"
  printf '  %-4s %4s KiB gzipped (budget %s)\n' "$label" "$actual" "$budget"
  if (( actual > budget )); then
    cat >&2 <<MSG
error: $label bundle is ${actual} KiB gzipped, over the recorded budget of ${budget} KiB.

If the growth is intended, raise BUDGET_KIB in this script in the same commit and say why
in the commit message. The number is here so that growth is a decision rather than a drift.
MSG
    return 1
  fi
}

echo "==> bundle-size gate"
check_budget v1 "$DASH/dist" "$BUDGET_KIB_V1"
check_budget v2 "$DASH/dist-v2" "$BUDGET_KIB_V2"

echo "==> done — dashboard/dist and dashboard/dist-v2 ready for rgctl-dashboard include_dir"
