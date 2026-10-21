#!/usr/bin/env bash
# Verify documented guide CLI commands + jq filters against clean fixtures.
# Uses git archive (CoolStore) / temp copy (ecommerce-java) so untracked local
# dirt never pollutes counts.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RGCTL="${RGCTL_BIN:-$ROOT/target/release/rgctl}"
TMP="${TMPDIR:-/tmp}/rgctl-verify-docs-$$"
PASS=0
FAIL=0

cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

die() { echo "ERROR: $*" >&2; exit 1; }

need() { command -v "$1" >/dev/null || die "missing required tool: $1"; }

need jq
[[ -x "$RGCTL" ]] || die "rgctl binary not found at $RGCTL (build with: cargo build --release --bin rgctl)"

mkdir -p "$TMP/ecommerce-java" "$TMP/coolstore" "$TMP/out"

materialize_ecommerce() {
  local src="$ROOT/rgctl-tests/ecommerce-java"
  [[ -d "$src" ]] || die "missing $src"
  # Prefer rsync if present; fall back to cp -R
  if command -v rsync >/dev/null; then
    rsync -a --exclude '.rgctl' --exclude 'target' "$src/" "$TMP/ecommerce-java/"
  else
    cp -R "$src/." "$TMP/ecommerce-java/"
    rm -rf "$TMP/ecommerce-java/.rgctl"
  fi
}

materialize_coolstore() {
  local src="$ROOT/example/coolstore"
  [[ -d "$src" ]] || {
    echo "SKIP coolstore (example/coolstore not present)"
    return 1
  }
  local real
  real="$(cd "$src" && pwd -P)"
  if [[ -d "$real/.git" ]]; then
    # Symlink example/coolstore -> coolstore-weblogic; archive from resolved path
    git -C "$real" archive HEAD | tar -x -C "$TMP/coolstore"
  else
    if command -v rsync >/dev/null; then
      rsync -a --exclude '.rgctl' --exclude 'rgctl-benchmark' --exclude '.claude' \
        --exclude 'target' --exclude 'policy.json' "$src/" "$TMP/coolstore/"
    else
      cp -R "$src/." "$TMP/coolstore/"
      rm -rf "$TMP/coolstore/.rgctl" "$TMP/coolstore/rgctl-benchmark" \
        "$TMP/coolstore/.claude" "$TMP/coolstore/policy.json"
    fi
  fi
  local java_count
  java_count="$(find "$TMP/coolstore" -name '*.java' | wc -l | tr -d ' ')"
  [[ "$java_count" == "26" ]] || die "expected 26 CoolStore Java files, got $java_count"
  return 0
}

run() {
  local name="$1"
  shift
  printf "  %-55s " "$name"
  if "$@" >"$TMP/out/$name.stdout" 2>"$TMP/out/$name.stderr"; then
    echo "PASS"
    PASS=$((PASS + 1))
    return 0
  fi
  echo "FAIL"
  FAIL=$((FAIL + 1))
  sed 's/^/    /' "$TMP/out/$name.stderr" | head -5
  return 0
}

assert_jq() {
  local name="$1"
  local filter="$2"
  local file="$3"
  printf "  %-55s " "jq:$name"
  if jq -e "$filter" "$file" >/dev/null 2>"$TMP/out/jq_$name.stderr"; then
    echo "PASS"
    PASS=$((PASS + 1))
  else
    echo "FAIL"
    FAIL=$((FAIL + 1))
    sed 's/^/    /' "$TMP/out/jq_$name.stderr" | head -3
  fi
}

verify_repo() {
  local label="$1"
  local repo="$2"
  local discover_extra=("${@:3}")

  echo "=== $label ($repo) ==="
  run "${label}_discover" \
    "$RGCTL" -r "$repo" discover . "${discover_extra[@]}" --with-cfg --with-harmonic -f json

  run "${label}_status" "$RGCTL" -r "$repo" -f json status
  assert_jq "${label}_status_sv" '.schema_version == 3 and .nodes != null and .edges != null' \
    "$TMP/out/${label}_status.stdout"

  run "${label}_inventory" "$RGCTL" -r "$repo" -f json inventory
  assert_jq "${label}_inventory_fn" \
    '[.counts[] | select(.key=="function" and .count > 0)] | length == 1' \
    "$TMP/out/${label}_inventory.stdout"

  run "${label}_find_fn" "$RGCTL" -r "$repo" -f json find --type function --count-only
  assert_jq "${label}_find_sv" '.schema_version == 2 and .total > 0' \
    "$TMP/out/${label}_find_fn.stdout"

  run "${label}_cpg_status" "$RGCTL" -r "$repo" -f json cpg status
  assert_jq "${label}_cpg_status_keys" \
    '.archive_present == true and .schema_version == 1 and (.field_write_count|type)=="number"' \
    "$TMP/out/${label}_cpg_status.stdout"

  run "${label}_metrics_pr" "$RGCTL" -r "$repo" -f json metrics --pagerank
  assert_jq "${label}_pagerank" '.pagerank.top | type == "array"' \
    "$TMP/out/${label}_metrics_pr.stdout"

  run "${label}_communities" "$RGCTL" -r "$repo" -f json communities list
  assert_jq "${label}_communities_arr" '.communities | type == "array"' \
    "$TMP/out/${label}_communities.stdout"

  run "${label}_clones" "$RGCTL" -r "$repo" -f json clones
  assert_jq "${label}_clones_sv" '.schema_version == 1 and (.groups|type)=="array"' \
    "$TMP/out/${label}_clones.stdout"

  run "${label}_install_list" "$RGCTL" -r "$repo" -f json install --list-agents
  assert_jq "${label}_install_list_agents" '.agents | type == "array"' \
    "$TMP/out/${label}_install_list.stdout"
}

echo "RGCTL=$RGCTL"
echo "TMP=$TMP"

materialize_ecommerce
verify_repo "ecommerce" "$TMP/ecommerce-java" -l java

# ecommerce-specific guide commands
SYM="clearCart"
run "ecommerce_callers" "$RGCTL" -r "$TMP/ecommerce-java" -f json callers "$SYM" --class CartService
assert_jq "ecommerce_callers_key" '.callers | type == "array"' "$TMP/out/ecommerce_callers.stdout"
run "ecommerce_callees" "$RGCTL" -r "$TMP/ecommerce-java" -f json callees "$SYM" --class CartService
assert_jq "ecommerce_callees_key" '.callees | type == "array"' "$TMP/out/ecommerce_callees.stdout"
run "ecommerce_blast" "$RGCTL" -r "$TMP/ecommerce-java" -f json blast-radius "$SYM" --class CartService
assert_jq "ecommerce_blast_sv" '.schema_version == 2 and .metrics != null' \
  "$TMP/out/ecommerce_blast.stdout"
run "ecommerce_inspect_cfg" "$RGCTL" -r "$TMP/ecommerce-java" -f json inspect checkout --class OrderService cfg
assert_jq "ecommerce_inspect_cfg_keys" \
  'has("nodes") and has("edges") and has("layer") and has("symbol") and .schema_version == 1 and (has("node_count")|not) and (has("nodes_sample")|not)' \
  "$TMP/out/ecommerce_inspect_cfg.stdout"
run "ecommerce_cpg_calls" "$RGCTL" -r "$TMP/ecommerce-java" -f json cpg calls checkout --class OrderService
assert_jq "ecommerce_cpg_calls_keys" \
  'has("edges") and has("function_id") and has("function_name") and .schema_version == 1 and (has("edges_sample")|not) and (has("returned")|not)' \
  "$TMP/out/ecommerce_cpg_calls.stdout"

SLICE_FILE="src/main/java/com/example/ecommerce/service/CartService.java"
run "ecommerce_slice" \
  "$RGCTL" -r "$TMP/ecommerce-java" -f json slice "$SLICE_FILE" \
  --line 53 --variable item --function addItem
assert_jq "ecommerce_slice_keys" 'has("lines") or has("steps") or .ok == false' \
  "$TMP/out/ecommerce_slice.stdout"

run "ecommerce_export" \
  "$RGCTL" -r "$TMP/ecommerce-java" export \
  --export-format json --export-output "$TMP/out/ecommerce-export.json" \
  --query "name:clearCart"

# migration-hints stdout shape
run "ecommerce_mig_hints" \
  "$RGCTL" -r "$TMP/ecommerce-java" discover . -l java --export-migration-hints -f json
assert_jq "ecommerce_mig_hints_stdout" \
  'has("steps") and .schema_version == 2 and (.preset|type)=="string"' \
  "$TMP/out/ecommerce_mig_hints.stdout"
[[ -f "$TMP/ecommerce-java/.rgctl/migration_plan.json" ]] || {
  echo "  missing migration_plan.json after --export-migration-hints"
  FAIL=$((FAIL + 1))
}

if materialize_coolstore; then
  verify_repo "coolstore" "$TMP/coolstore" -l java

  assert_jq "coolstore_status_counts" \
    '.nodes == 608 and .edges == 1709' \
    "$TMP/out/coolstore_status.stdout"
  assert_jq "coolstore_field_writes" \
    '.field_write_count == 41' \
    "$TMP/out/coolstore_cpg_status.stdout"
  assert_jq "coolstore_functions" \
    '.total == 152' \
    "$TMP/out/coolstore_find_fn.stdout"

  run "coolstore_inspect_cfg" \
    "$RGCTL" -r "$TMP/coolstore" -f json inspect priceShoppingCart cfg
  assert_jq "coolstore_inspect_cfg_keys" \
    'has("nodes") and has("edges") and (.nodes|length)==12 and (.edges|length)==16 and (has("nodes_sample")|not)' \
    "$TMP/out/coolstore_inspect_cfg.stdout"

  run "coolstore_inspect_pdg" \
    "$RGCTL" -r "$TMP/coolstore" -f json inspect priceShoppingCart pdg
  assert_jq "coolstore_inspect_pdg_keys" \
    'has("data_deps") and has("control_deps") and has("nodes") and has("edges")' \
    "$TMP/out/coolstore_inspect_pdg.stdout"

  run "coolstore_inspect_dom" \
    "$RGCTL" -r "$TMP/coolstore" -f json inspect priceShoppingCart dom
  assert_jq "coolstore_inspect_dom_keys" \
    'has("idom") and has("frontiers") and has("nodes")' \
    "$TMP/out/coolstore_inspect_dom.stdout"

  run "coolstore_cpg_calls" \
    "$RGCTL" -r "$TMP/coolstore" -f json cpg calls priceShoppingCart
  assert_jq "coolstore_cpg_calls_keys" \
    'has("edges") and has("function_id") and has("function_name") and (has("edges_sample")|not)' \
    "$TMP/out/coolstore_cpg_calls.stdout"

  run "coolstore_cpg_fn" \
    "$RGCTL" -r "$TMP/coolstore" -f json cpg function priceShoppingCart
  assert_jq "coolstore_cpg_fn" '.name == "priceShoppingCart" and .has_l_proc == true' \
    "$TMP/out/coolstore_cpg_fn.stdout"

  run "coolstore_blast" \
    "$RGCTL" -r "$TMP/coolstore" -f json blast-radius priceShoppingCart
  assert_jq "coolstore_blast" '.schema_version == 2 and .metrics.score != null' \
    "$TMP/out/coolstore_blast.stdout"

  run "coolstore_export_fn" \
    "$RGCTL" -r "$TMP/coolstore" export \
    --export-format json --export-output "$TMP/out/coolstore-fn.json" \
    --query "type:Function"
  if ! grep -qE '152 nodes, 130 edges' \
      "$TMP/out/coolstore_export_fn.stdout" "$TMP/out/coolstore_export_fn.stderr"; then
    echo "  coolstore_export_fn count FAIL"
    FAIL=$((FAIL + 1))
  else
    echo "  coolstore_export_fn counts                          PASS"
    PASS=$((PASS + 1))
  fi

  run "coolstore_export_all" \
    "$RGCTL" -r "$TMP/coolstore" export \
    --export-format json --export-output "$TMP/out/coolstore-all.json" \
    --query all
  if ! grep -qE '608 nodes, 1709 edges' \
      "$TMP/out/coolstore_export_all.stdout" "$TMP/out/coolstore_export_all.stderr"; then
    echo "  coolstore_export_all count FAIL"
    FAIL=$((FAIL + 1))
  else
    echo "  coolstore_export_all counts                         PASS"
    PASS=$((PASS + 1))
  fi

  CS_SLICE="src/main/java/com/redhat/coolstore/service/ShoppingCartService.java"
  run "coolstore_cpg_flows" \
    "$RGCTL" -r "$TMP/coolstore" -f json cpg flows "$CS_SLICE" \
    --line 68 --variable sc --function priceShoppingCart
  assert_jq "coolstore_cpg_flows" 'has("lines") and has("steps") and .schema_version == 1' \
    "$TMP/out/coolstore_cpg_flows.stdout"

  run "coolstore_cpg_pdg" \
    "$RGCTL" -r "$TMP/coolstore" -f json cpg pdg priceShoppingCart
  assert_jq "coolstore_cpg_pdg" \
    'has("data_deps") and has("control_deps") and .schema_version == 1' \
    "$TMP/out/coolstore_cpg_pdg.stdout"

  run "coolstore_semantic_index" \
    "$RGCTL" -r "$TMP/coolstore" -f json semantic index
  run "coolstore_semantic_query" \
    "$RGCTL" -r "$TMP/coolstore" -f json semantic query "shopping cart pricing"
  assert_jq "coolstore_semantic_query" \
    'has("hits") and .schema_version == 3' \
    "$TMP/out/coolstore_semantic_query.stdout"
fi

echo ""
echo "=== Summary: PASS=$PASS FAIL=$FAIL ==="
[[ "$FAIL" -eq 0 ]]
