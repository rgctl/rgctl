#!/usr/bin/env bash
# rgctl command verification (slice, blast-radius, inspect, metrics, semantic, …).
# Source after extraction-gql-common.sh and rgctl-commands-config.sh.

_rgctl_run() {
  local repo="$1"
  shift
  "$(resolve_rgctl)" -r "$repo" "$@" 2>/dev/null
}

_rgctl_run_json() {
  local repo="$1"
  shift
  "$(resolve_rgctl)" -r "$repo" -f json "$@" 2>&1
}

_is_unsupported() {
  local out="$1"
  local el
  el="$(echo "$out" | tr '[:upper:]' '[:lower:]')"
  [[ "$el" == *unsupported* ]] || [[ "$el" == *"not found"* ]] || \
    [[ "$el" == *"no cfg"* ]] || [[ "$el" == *"no pdg"* ]] || \
    [[ "$el" == *"no pdg node"* ]] || \
    [[ "$el" == *ambiguous* ]]
}

assert_rgctl_cmd() {
  local label="$1"
  local repo="$2"
  local example="$3"
  shift 3
  local out
  echo "  example: rgctl -r \"<repo>\" ${example}"
  if out="$(_rgctl_run "$repo" "$@")"; then
    echo "ok: ${label}"
    return 0
  fi
  if _is_unsupported "$out"; then
    echo "skip: ${label} (unsupported on this fixture)"
    return 0
  fi
  echo "FAIL: ${label}" >&2
  echo "$out" | tail -5 >&2
  return 1
}

_parse_rgctl_json() {
  python3 -c '
import json, sys
lines = [
    line for line in sys.stdin.read().splitlines()
    if not line.lstrip().startswith("[")
]
payload = "\n".join(lines).strip()
if not payload:
    raise SystemExit(1)
json.loads(payload)
' 2>/dev/null
}

assert_rgctl_json_cmd() {
  local label="$1"
  local repo="$2"
  local example="$3"
  shift 3
  local out rc
  echo "  example: rgctl -r \"<repo>\" -f json ${example}"
  set +e
  out="$(_rgctl_run_json "$repo" "$@")"
  rc=$?
  set -e
  if [[ "$rc" -eq 0 ]] && echo "$out" | _parse_rgctl_json; then
    echo "ok: ${label}"
    return 0
  fi
  if _is_unsupported "$out"; then
    echo "skip: ${label} (unsupported on this fixture)"
    return 0
  fi
  echo "FAIL: ${label}" >&2
  echo "$out" | tail -5 >&2
  return 1
}

discover_for_commands() {
  local repo="$1"
  shift
  discover_repo "$repo" "$@"
}

run_rgctl_commands_suite() {
  local repo="$1"
  echo "--- rgctl commands: ${RGCTL_CMD_ID} @ ${repo} ---"

  if [[ -z "${RGCTL_CMD_SKIP_DISCOVER:-}" ]]; then
    discover_for_commands "$repo" "${RGCTL_CMD_DISCOVER_EXTRA[@]}"
  fi

  # blast-radius — macro impact / blast radius for a symbol
  assert_rgctl_json_cmd "blast-radius (primary)" "$repo" \
    "blast-radius '${RGCTL_CMD_BLAST_PRIMARY}'" \
    blast-radius "${RGCTL_CMD_BLAST_PRIMARY}"

  if [[ -n "${RGCTL_CMD_BLAST_COOLSTORE:-}" && "${RGCTL_CMD_BLAST_COOLSTORE}" != "${RGCTL_CMD_BLAST_PRIMARY}" ]]; then
    assert_rgctl_json_cmd "blast-radius (coolstore)" "$repo" \
      "blast-radius '${RGCTL_CMD_BLAST_COOLSTORE}'" \
      blast-radius "${RGCTL_CMD_BLAST_COOLSTORE}"
  fi

  # metrics — PageRank, betweenness, communities
  assert_rgctl_json_cmd "metrics (--communities --pagerank)" "$repo" \
    "metrics --communities --pagerank" \
    metrics --communities --pagerank

  # communities — list named communities overlay
  assert_rgctl_cmd "communities list" "$repo" \
    "communities list" \
    communities list

  # dead-code — graph reachability analysis
  assert_rgctl_json_cmd "dead-code" "$repo" \
    "dead-code" \
    dead-code

  # inspect — CFG / PDG / dominance for a function symbol
  for mode in cfg pdg dom; do
    assert_rgctl_json_cmd "inspect ${mode} (${RGCTL_CMD_INSPECT_FN})" "$repo" \
      "inspect ${RGCTL_CMD_INSPECT_FN} ${mode}" \
      inspect "${RGCTL_CMD_INSPECT_FN}" "${mode}"
  done

  # slice — line-level program slice (and taint trace variant)
  if [[ -n "${RGCTL_CMD_SLICE_FILE:-}" && -f "${repo}/${RGCTL_CMD_SLICE_FILE}" ]]; then
    assert_rgctl_json_cmd "slice (backward)" "$repo" \
      "slice ${RGCTL_CMD_SLICE_FILE} --line ${RGCTL_CMD_SLICE_LINE} --variable ${RGCTL_CMD_SLICE_VAR} --function ${RGCTL_CMD_SLICE_FN}" \
      slice "${RGCTL_CMD_SLICE_FILE}" \
      --line "${RGCTL_CMD_SLICE_LINE}" \
      --variable "${RGCTL_CMD_SLICE_VAR}" \
      --function "${RGCTL_CMD_SLICE_FN}"

    assert_rgctl_json_cmd "slice --taint" "$repo" \
      "slice ${RGCTL_CMD_SLICE_FILE} --line ${RGCTL_CMD_SLICE_LINE} --variable ${RGCTL_CMD_SLICE_VAR} --function ${RGCTL_CMD_SLICE_FN} --taint" \
      slice "${RGCTL_CMD_SLICE_FILE}" \
      --line "${RGCTL_CMD_SLICE_LINE}" \
      --variable "${RGCTL_CMD_SLICE_VAR}" \
      --function "${RGCTL_CMD_SLICE_FN}" \
      --taint
  else
    echo "skip: slice (no slice file configured for ${RGCTL_CMD_ID})"
  fi

  # cpg — hybrid CPG façade (topology + CFG/PDG archive)
  assert_rgctl_json_cmd "cpg status" "$repo" \
    "cpg status" \
    cpg status

  if [[ -n "${RGCTL_CMD_CPG_TYPE:-}" ]]; then
    local cpg_out
    echo "  example: rgctl -r \"<repo>\" cpg mutations --type ${RGCTL_CMD_CPG_TYPE} --exclude-ctors"
    if cpg_out="$(_rgctl_run "$repo" cpg mutations --type "${RGCTL_CMD_CPG_TYPE}" --exclude-ctors)"; then
      local lines
      lines="$(echo "$cpg_out" | grep -cvE '^$|^\[|^Mutations of' || true)"
      if [[ "${RGCTL_CMD_CPG_MIN_LINES:-0}" -eq 0 ]] || [[ "$lines" -ge "${RGCTL_CMD_CPG_MIN_LINES}" ]]; then
        echo "ok: cpg mutations (--type ${RGCTL_CMD_CPG_TYPE})"
      else
        echo "skip: cpg mutations (expected >= ${RGCTL_CMD_CPG_MIN_LINES} body lines, got ${lines})"
      fi
    elif _is_unsupported "$cpg_out"; then
      echo "skip: cpg mutations (unsupported on this fixture)"
    else
      echo "FAIL: cpg mutations" >&2
      echo "$cpg_out" | tail -5 >&2
      return 1
    fi
  fi

  # semantic — opt-in semantic search (separate index artifact)
  echo "  example: rgctl -r \"<repo>\" semantic index --embedder vocab --dimensions 256"
  if _rgctl_run "$repo" semantic index --embedder vocab --dimensions 256 >/dev/null; then
    echo "ok: semantic index"
    assert_rgctl_json_cmd "semantic query" "$repo" \
      "semantic query '${RGCTL_CMD_SEMANTIC_QUERY}' --limit 5" \
      semantic query "${RGCTL_CMD_SEMANTIC_QUERY}" --limit 5
  else
    echo "skip: semantic index (failed)"
  fi

  # export — graph or projection export
  local export_tmp
  export_tmp="$(mktemp "${TMPDIR:-/tmp}/rgctl-export.XXXXXX.json")"
  echo "  example: rgctl -r \"<repo>\" export --export-format json --export-output out.json --query '${RGCTL_CMD_EXPORT_QUERY}'"
  if _rgctl_run "$repo" export \
    --export-format json \
    --export-output "$export_tmp" \
    --query "${RGCTL_CMD_EXPORT_QUERY}"; then
    if [[ -s "$export_tmp" ]]; then
      echo "ok: export (--query ${RGCTL_CMD_EXPORT_QUERY})"
    else
      echo "FAIL: export (empty output file)" >&2
      rm -f "$export_tmp"
      return 1
    fi
  else
    echo "FAIL: export" >&2
    rm -f "$export_tmp"
    return 1
  fi
  rm -f "$export_tmp"

  # check — CI policy gateway
  if [[ -f "${RGCTL_CMD_POLICY}" ]]; then
    assert_rgctl_json_cmd "check (--policy-file)" "$repo" \
      "check --policy-file ${RGCTL_CMD_POLICY}" \
      check --policy-file "${RGCTL_CMD_POLICY}"
  else
    echo "skip: check (policy file missing at ${RGCTL_CMD_POLICY})"
  fi

  echo "--- rgctl commands: ${RGCTL_CMD_ID} OK ---"
}
