#!/usr/bin/env bash
# Validate Query Guide CLI workflows against gbuilder (read-only; no discover).
set -uo pipefail

REPO="${RBUILDER_DASHBOARD_GOLDEN_REPO:-/Users/sshaaf/git/java/gbuilder}"
RB="${RBUILDER_BIN:-/Users/sshaaf/git/rust/rBuilder/target/release/rbuilder}"
TMP="${TMPDIR:-/tmp}/rbuilder-guide-gbuilder-$$"
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

if [[ ! -d "$REPO/.rbuilder" ]]; then
  echo "ERROR: no .rbuilder at $REPO — run discover first"
  exit 1
fi

echo "[graph]"
run "gql all_functions macro" \
  "$RB" -r "$REPO" gql --macro-name all_functions unused
run "gql Service pattern (gbuilder: *parse*)" \
  "$RB" -r "$REPO" gql "MATCH (n:Function) WHERE n.name LIKE '*parse*' RETURN n LIMIT 20"
run "gql call chain 1..3" \
  "$RB" -r "$REPO" gql "MATCH (a:Function)-[:CALLS*1..3]->(b:Function) RETURN a,b LIMIT 50"
run "gql call_chain macro" \
  "$RB" -r "$REPO" gql --macro-name call_chain unused
run "export graphml subgraph (filter query)" \
  "$RB" -r "$REPO" export --export-format graphml --export-output "$EXPORT_OUT" \
  --query "name:parseFile"
run "export mermaid all" \
  "$RB" -r "$REPO" export --export-format mermaid --export-output "$MERMAID_OUT" --query all
skip "serve + gql" "daemon (manual)"

echo "[functions]"
run "gql all_functions" \
  "$RB" -r "$REPO" gql --macro-name all_functions unused
run "gql json row count" \
  bash -c "$RB -r \"$REPO\" -f json gql \"MATCH (n:Function) RETURN n\" | jq '.rows | length' >/dev/null"
run "metrics pagerank" \
  "$RB" -r "$REPO" metrics --pagerank
run "metrics betweenness" \
  "$RB" -r "$REPO" metrics --betweenness
run "gql Class limit 30" \
  "$RB" -r "$REPO" gql "MATCH (n:Class) RETURN n LIMIT 30"
run "gql file_path filter" \
  "$RB" -r "$REPO" gql "MATCH (n:Function) WHERE n.file_path LIKE '*gbuilder*' RETURN n LIMIT 10"

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
run "gql Endpoint pattern (gbuilder: *main*)" \
  "$RB" -r "$REPO" gql "MATCH (n:Function) WHERE n.name LIKE '*main*' RETURN n LIMIT 10"

echo "[migration]"
if [[ -f "$REPO/.rbuilder/dashboard/migration_plan.json" ]]; then
  run "migration_plan.json readable" \
    jq '.packages[:1]' "$REPO/.rbuilder/dashboard/migration_plan.json"
else
  skip "migration_plan.json" "run discover --all --export-migration-plan first"
fi
skip "discover --export-migration-plan" "would re-index (slow)"

echo "[guide / gql]"
run "gql direct_calls macro" \
  "$RB" -r "$REPO" gql --macro-name direct_calls unused
run "gql count functions (json + jq)" \
  bash -c "$RB -r \"$REPO\" -f json gql \"MATCH (n:Function) RETURN n\" | jq '.count' >/dev/null"
run "gql direct calls limit 25" \
  "$RB" -r "$REPO" gql "MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a,b LIMIT 25"
run "gql explain" \
  "$RB" -r "$REPO" gql --explain "MATCH (n:Function) WHERE n.name = 'parseFile' RETURN n"
run "gql json first row" \
  bash -c "$RB -r \"$REPO\" -f json gql \"MATCH (n:Function) RETURN n\" | jq '.rows[0]' >/dev/null"

echo ""
echo "=== Summary: PASS=$PASS FAIL=$FAIL SKIP=$SKIP ==="
rm -rf "$TMP"
[[ "$FAIL" -eq 0 ]]
