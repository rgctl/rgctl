#!/usr/bin/env bash
# Validate Query Guide CLI workflows against gbuilder (read-only; no discover).
set -uo pipefail

REPO="${RGCTL_DASHBOARD_GOLDEN_REPO:-/Users/sshaaf/git/java/gbuilder}"
RB="${RGCTL_BIN:-/Users/sshaaf/git/rust/rbuilder/target/release/rgctl}"
TMP="${TMPDIR:-/tmp}/rgctl-guide-gbuilder-$$"
mkdir -p "$TMP"

SLICE_FILE="src/main/java/dev/shaaf/gbuilder/lang/java/JavaTreeSitterExtractor.java"
CLASS="JavaTreeSitterExtractor"
SYMBOL="parseFile"
EXPORT_OUT="$TMP/subgraph.graphml"
MERMAID_OUT="$TMP/calls.mmd"

PASS=0
FAIL=0
SKIP=0

run() {
  local name="$1"
  shift
  printf "  %-55s " "$name"
  if "$@" >/dev/null 2>"$TMP/err.txt"; then
    echo "PASS"
    PASS=$((PASS + 1))
  else
    echo "FAIL"
    FAIL=$((FAIL + 1))
    sed 's/^/    /' "$TMP/err.txt" | head -3
  fi
}

skip() {
  local name="$1"
  local reason="$2"
  printf "  %-55s " "$name"
  echo "SKIP ($reason)"
  SKIP=$((SKIP + 1))
}

echo "=== gbuilder Query Guide CLI validation ==="
echo "REPO=$REPO"
echo ""

if [[ ! -d "$REPO/.rgctl" ]]; then
  echo "ERROR: no .rgctl at $REPO — run discover first"
  exit 1
fi

echo "[graph]"
run "find all functions (type filter)" \
  "$RB" -r "$REPO" find --type function --limit 20
run "find Service-like names (gbuilder: *parse*)" \
  "$RB" -r "$REPO" find '*parse*' --type function --limit 20
run "callers parseFile" \
  "$RB" -r "$REPO" callers "$SYMBOL" --class "$CLASS"
run "callees parseFile" \
  "$RB" -r "$REPO" callees "$SYMBOL" --class "$CLASS"
run "relations CALLS depth 2" \
  "$RB" -r "$REPO" relations --edge calls --depth 2 "$SYMBOL" --class "$CLASS"
run "export graphml subgraph (filter query)" \
  "$RB" -r "$REPO" export --export-format graphml --export-output "$EXPORT_OUT" \
  --query "name:parseFile"
run "export mermaid all" \
  "$RB" -r "$REPO" export --export-format mermaid --export-output "$MERMAID_OUT" --query all
skip "serve + query" "daemon (manual)"

echo "[functions]"
run "inventory by type" \
  "$RB" -r "$REPO" inventory --by type
run "find json function count" \
  bash -c "$RB -r \"$REPO\" -f json find --type function --count-only | jq '.total' >/dev/null"
run "metrics pagerank" \
  "$RB" -r "$REPO" metrics --pagerank
run "metrics betweenness" \
  "$RB" -r "$REPO" metrics --betweenness
run "find Class limit 30" \
  "$RB" -r "$REPO" find --type class --limit 30
run "find file_path filter" \
  "$RB" -r "$REPO" find --type function --file '*gbuilder*' --limit 10

echo "[cfg]"
run "inspect parseFile cfg" \
  "$RB" -r "$REPO" inspect "$SYMBOL" cfg
run "inspect parseFile cfg mermaid" \
  "$RB" -r "$REPO" -f mermaid inspect "$SYMBOL" cfg
run "inspect parseFile dom frontiers" \
  "$RB" -r "$REPO" inspect "$SYMBOL" dom --frontiers

echo "[dataflow]"
run "inspect parseFile pdg data" \
  "$RB" -r "$REPO" inspect "$SYMBOL" pdg --edge-layer data
run "inspect parseFile pdg def-use" \
  "$RB" -r "$REPO" inspect "$SYMBOL" pdg --def-use
run "slice pdg mermaid" \
  "$RB" -r "$REPO" -f mermaid slice "$SLICE_FILE" --line 80 --variable source --function "$SYMBOL" --view pdg

echo "[slice]"
run "slice backward" \
  "$RB" -r "$REPO" slice "$SLICE_FILE" --line 80 --variable source --function "$SYMBOL"
run "slice forward" \
  "$RB" -r "$REPO" slice "$SLICE_FILE" --line 81 --variable packageName --function "$SYMBOL" --direction forward
run "slice json lines" \
  bash -c "$RB -r \"$REPO\" -f json slice \"$SLICE_FILE\" --line 80 --variable source --function \"$SYMBOL\" | jq '.lines' >/dev/null"

echo "[blast]"
run "blast-radius parseFile" \
  "$RB" -r "$REPO" blast-radius "$SYMBOL" --class "$CLASS"
run "blast-radius depth 1" \
  "$RB" -r "$REPO" blast-radius "$SYMBOL" --class "$CLASS" --depth 1
run "blast-radius depth 5 json" \
  bash -c "$RB -r \"$REPO\" -f json blast-radius \"$SYMBOL\" --class \"$CLASS\" --depth 5 | jq '.metrics' >/dev/null"
skip "check policy" "no policy.json in repo"

echo "[taint]"
run "slice --taint" \
  "$RB" -r "$REPO" slice "$SLICE_FILE" --line 80 --variable source --function "$SYMBOL" --taint
run "find Endpoint-like (*main*)" \
  "$RB" -r "$REPO" find '*main*' --type function --limit 10

echo "[migration]"
if [[ -f "$REPO/.rgctl/migration_plan.json" ]]; then
  run "migration_plan.json readable" \
    jq '.steps[:1]' "$REPO/.rgctl/migration_plan.json"
elif [[ -f "$REPO/.rgctl/dashboard/migration_plan.json" ]]; then
  run "migration_plan.json readable (dashboard)" \
    jq '.' "$REPO/.rgctl/dashboard/migration_plan.json" >/dev/null
else
  skip "migration_plan.json" "run discover --export-migration-hints first"
fi
skip "discover --export-migration-hints" "would re-index (slow)"

echo "[guide / structured query]"
run "relations direct CALLS" \
  "$RB" -r "$REPO" relations --edge calls "$SYMBOL" --class "$CLASS" --limit 25
run "inventory count functions (json + jq)" \
  bash -c "$RB -r \"$REPO\" -f json inventory --by type | jq '[.counts[]|select(.key==\"function\")][0].count' >/dev/null"
run "find json first hit" \
  bash -c "$RB -r \"$REPO\" -f json find --type function --limit 1 | jq '.results[0] // .hits[0] // .' >/dev/null"
# Experimental GQL still available; prefer structured verbs above.
run "gql explain (experimental)" \
  "$RB" -r "$REPO" gql --explain "MATCH (n:Function) WHERE n.name = 'parseFile' RETURN n"

echo ""
echo "=== Summary: PASS=$PASS FAIL=$FAIL SKIP=$SKIP ==="
rm -rf "$TMP"
[[ "$FAIL" -eq 0 ]]
