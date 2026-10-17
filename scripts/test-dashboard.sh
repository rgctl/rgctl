#!/usr/bin/env bash
# Every dashboard gate, in one command.
#
# These were being run one at a time, which means in practice they were run when someone
# remembered. A gate that needs five invocations from memory is five chances to skip one.
#
# Usage: ./scripts/test-dashboard.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
fail=0

step() { printf '\n==> %s\n' "$1"; }
ok()   { printf '  ok   %s\n' "$1"; }
bad()  { printf '  FAIL %s\n' "$1"; fail=1; }

step "cargo: dashboard crate"
if cargo test -p rgctl-dashboard 2>&1 | tail -3 | grep -q "test result: ok"; then
  ok "rgctl-dashboard unit tests"
else
  bad "rgctl-dashboard unit tests"
fi

step "cargo: dashboard integration gates"
for t in dashboard_bundle dashboard_harness; do
  if cargo test --test "$t" 2>&1 | grep -q "test result: ok"; then ok "$t"; else bad "$t"; fi
done

# Exit status, not output matching. Vitest prints a `failed` count only when something failed, so a check
# for "0 failed" can never match a passing run — it reports the good case as broken, which is the same class
# of mistake as asserting on text that is absent when things go right.
suite() {
  local label="$1" config="$2" out
  if out=$(cd "$ROOT/dashboard" && npx vitest run ${config:+--config "$config"} 2>&1); then
    ok "$label ($(echo "$out" | grep -oE 'Tests +[0-9]+ passed' | head -1))"
  else
    bad "$label"
    echo "$out" | tail -12 | sed 's/^/       /'
  fi
}

step "npm: v1 dashboard (Preact)"
suite "v1 suite" ""

step "npm: v2 dashboard (React)"
suite "v2 suite" "vitest.config.v2.ts"

# --- offline gate ---------------------------------------------------------------------------------
#
# The bundle is embedded in the binary and must work with no network. This checks the *resource*
# references — `src=`, `href=`, CSS `url(...)` — rather than grepping for "http", because library strings
# legitimately contain URLs: React's own error messages are `https://react.dev/errors/N`, and a naive grep
# fails on those while telling you nothing about whether the bundle fetches anything.
step "offline gate: no external resource references"
offline_check() {
  local dir="$1" label="$2" hits
  hits=$( { grep -ohE '(src|href)="https?://[^"]+"' "$dir"/index.html 2>/dev/null || true
            grep -ohE 'url\((https?:)?//[^)]+\)' "$dir"/assets/*.css 2>/dev/null || true
            grep -ohE 'from"https?://[^"]+"|import"https?://[^"]+"' "$dir"/assets/*.js 2>/dev/null || true
          } | sort -u )
  if [[ -z "$hits" ]]; then
    ok "$label loads no external origin"
  else
    bad "$label references an external origin:"
    printf '       %s\n' $hits
  fi
}
offline_check dashboard/dist v1
offline_check dashboard/dist-v2 v2

printf '\n'
if (( fail )); then
  echo "dashboard gates: FAILED"
  exit 1
fi
echo "dashboard gates: all passed"
