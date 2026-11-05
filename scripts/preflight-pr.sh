#!/usr/bin/env bash
# Pre-flight PR check script — mirrors the split GitHub Actions workflows:
#   - Tier 1 (ci-lint.yml): commit policy, cargo fmt, cargo check, cargo clippy (--all-targets)
#   - Tier 2 (ci.yml):      workspace unit/bin tests + fast core integration tests
#   - Tier 3 (ci-qe.yml):   QE/E2E correctness suites + AST handler coverage (opt-in; disabled on PR CI)
#   - Tier 4 (ci-perf.yml): --release CLI golden paths, scale/perf gates & bench smoke (opt-in; disabled on PR CI)
#
# Usage:
#   ./scripts/preflight-pr.sh              # Default PR gate: Tier 1 (lint) + Tier 2 (unit & core tests)
#   ./scripts/preflight-pr.sh --lint       # Tier 1 only (ci-lint.yml; alias: --quick)
#   ./scripts/preflight-pr.sh --test       # Tier 1 + Tier 2 (ci.yml; same as default)
#   ./scripts/preflight-pr.sh --qe         # Opt-in: run Tier 3 (ci-qe.yml)
#   ./scripts/preflight-pr.sh --perf       # Opt-in: run Tier 4 (ci-perf.yml)
#   ./scripts/preflight-pr.sh --full       # Opt-in: run all 4 tiers (lint + test + qe + perf)
#   ./scripts/preflight-pr.sh --fix        # Run `cargo fmt --all` before checking formatting
#   ./scripts/preflight-pr.sh --base main  # Override base git ref for commit-range checks

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

RUN_LINT=1
RUN_TEST=1
RUN_QE=0
RUN_PERF=0
EXPLICIT_TIER=0
FIX_FMT=0
BASE_REF=""
FEATURES="${FEATURES:-}"

usage() {
  cat <<'EOF'
Usage: ./scripts/preflight-pr.sh [OPTIONS]

Tier selection (default: --lint + --test, matching active PR CI; --qe, --perf, --full are opt-in):
  --lint, --quick    Tier 1 only (.github/workflows/ci-lint.yml):
                     commit checks, cargo fmt, cargo check, and all cargo clippy gates (--all-targets)
  --test             Tier 1 + Tier 2 (.github/workflows/ci.yml):
                     lint + workspace unit/bin tests + fast core integration tests (default)
  --tier2            Tier 2 only (.github/workflows/ci.yml job):
                     workspace unit/bin tests + fast core integration tests (without re-running Tier 1)
  --qe               Tier 3 (.github/workflows/ci-qe.yml — disabled on PR CI):
                     QE/E2E suites + RGCTL_AST_COVERAGE_TRACK=1
  --perf             Tier 4 (.github/workflows/ci-perf.yml — disabled on PR CI):
                     --release CLI golden paths, scale/perf gates, cargo bench --no-run
  --full             Run all 4 tiers (lint + test + qe + perf)

Other options:
  --fix              Run `cargo fmt --all` before checking formatting
  --base <git-ref>   Base ref for commit-range checks (default: auto-detect upstream/main, origin/main, or main)
  --features <list>  Cargo features to pass (mirrors CI FEATURES input; default: "")
  -h, --help         Show this help message
EOF
}

enable_custom_tiers() {
  if [[ "$EXPLICIT_TIER" -eq 0 ]]; then
    RUN_LINT=0
    RUN_TEST=0
    RUN_QE=0
    RUN_PERF=0
    EXPLICIT_TIER=1
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --lint|--quick)
      enable_custom_tiers
      RUN_LINT=1
      shift
      ;;
    --test)
      enable_custom_tiers
      RUN_LINT=1
      RUN_TEST=1
      shift
      ;;
    --tier2|--only-test)
      enable_custom_tiers
      RUN_TEST=1
      shift
      ;;
    --qe)
      enable_custom_tiers
      RUN_QE=1
      shift
      ;;
    --perf)
      enable_custom_tiers
      RUN_PERF=1
      shift
      ;;
    --full)
      enable_custom_tiers
      RUN_LINT=1
      RUN_TEST=1
      RUN_QE=1
      RUN_PERF=1
      shift
      ;;
    --fix)
      FIX_FMT=1
      shift
      ;;
    --base)
      BASE_REF="${2:-}"
      shift 2
      ;;
    --features)
      FEATURES="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
# Disable incremental cache bloat (saves ~7.5+ GiB in target/*/incremental per run)
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
# On GitHub Actions runners (~14 GB free disk), omit DWARF debug info in dev/test
# to cut target/debug and cache size by ~70% and speed up linking 2-3x.
if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
  export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
  export CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
fi
# Match .github/workflows/ci*.yml: fail on grammar <-> *-ast-coverage.json drift
export RGCTL_AST_COVERAGE_STRICT=1
if [[ "$RUN_QE" -eq 1 ]]; then
  export RGCTL_AST_COVERAGE_TRACK=1
fi

step() {
  printf '\n\033[1;34m==> %s\033[0m\n' "$1"
}

pass() {
  printf '\033[1;32m✓ %s\033[0m\n' "$1"
}

warn() {
  printf '\033[1;33m⚠ %s\033[0m\n' "$1"
}

cleanup_preflight_artifacts() {
  python3 - "$REPO_ROOT" <<'PY'
import collections
import os
import re
import shutil
import sys

repo_root = sys.argv[1]
in_ci = os.environ.get("GITHUB_ACTIONS") == "true"
HASH_RE = re.compile(r"^(.*)-([0-9a-f]{16})(\..+)?$")

removed_files = 0
removed_bytes = 0

def dir_size(path):
    total = 0
    for r, _, files in os.walk(path):
        for f in files:
            try:
                total += os.path.getsize(os.path.join(r, f))
            except OSError:
                pass
    return total

# Clean temporary test directories and fixture .rgctl caches
for pattern_root in ["rgctl-tests", "tests/fixtures"]:
    base = os.path.join(repo_root, pattern_root)
    if os.path.isdir(base):
        for root, dirs, _ in os.walk(base):
            if ".rgctl" in dirs:
                shutil.rmtree(os.path.join(root, ".rgctl"), ignore_errors=True)
                dirs.remove(".rgctl")

target_dir = os.path.join(repo_root, "target")
if os.path.isdir(target_dir):
    for entry in os.listdir(target_dir):
        if entry.startswith("agent-pack-test-"):
            shutil.rmtree(os.path.join(target_dir, entry), ignore_errors=True)

for prof in ["debug", "release"]:
    prof_dir = os.path.join(target_dir, prof)
    if not os.path.isdir(prof_dir):
        continue

    inc = os.path.join(prof_dir, "incremental")
    if os.path.isdir(inc):
        shutil.rmtree(inc, ignore_errors=True)

    # Remove legacy pre-rename binaries in target/<prof>/
    for fname in os.listdir(prof_dir):
        if fname in ("rbuilder", "rg_ctl", "rg-build", "rgbuilder") or fname.startswith(("librbuilder", "librgbuilder", "rbuilder.", "rg_ctl.", "rg-build.", "rgbuilder.")):
            fpath = os.path.join(prof_dir, fname)
            if os.path.isfile(fpath):
                try:
                    removed_bytes += os.path.getsize(fpath)
                    os.remove(fpath)
                    removed_files += 1
                except OSError:
                    pass

    # Prune stale target/<prof>/build/<pkg>-<hash> directories (e.g. 74 MiB rgctl-languages build dirs).
    # Partition by (pkg, has_out_dir) so build-script-build executable dirs are never evicted by OUT_DIR dirs.
    build_dir = os.path.join(prof_dir, "build")
    if os.path.isdir(build_dir):
        build_groups = collections.defaultdict(list)
        for entry in os.listdir(build_dir):
            m = HASH_RE.match(entry)
            if not m:
                continue
            bpath = os.path.join(build_dir, entry)
            if not os.path.isdir(bpath):
                continue
            has_out = os.path.isdir(os.path.join(bpath, "out"))
            ts_file = os.path.join(bpath, "invoked.timestamp")
            try:
                mtime = os.path.getmtime(ts_file) if os.path.isfile(ts_file) else os.path.getmtime(bpath)
                build_groups[(m.group(1), has_out)].append((mtime, bpath, entry))
            except OSError:
                pass
        keep_build = 2 if in_ci else 6
        pruned_build_entries = set()
        for (_, has_out), items in build_groups.items():
            items.sort(reverse=True)
            limit = keep_build if has_out else (1 if in_ci else 2)
            for _, stale_dir, entry in items[limit:]:
                removed_bytes += dir_size(stale_dir)
                shutil.rmtree(stale_dir, ignore_errors=True)
                pruned_build_entries.add(entry)
                removed_files += 1
        fp_dir = os.path.join(prof_dir, ".fingerprint")
        if pruned_build_entries and os.path.isdir(fp_dir):
            for entry in pruned_build_entries:
                shutil.rmtree(os.path.join(fp_dir, entry), ignore_errors=True)

    deps = os.path.join(prof_dir, "deps")
    if not os.path.isdir(deps):
        continue

    exe_groups = collections.defaultdict(list)
    lib_hash_groups = collections.defaultdict(lambda: collections.defaultdict(list))

    for fname in os.listdir(deps):
        fpath = os.path.join(deps, fname)
        if not os.path.isfile(fpath):
            continue
        # Always remove incremental .rcgu.o object files
        if fname.endswith(".rcgu.o"):
            try:
                removed_bytes += os.path.getsize(fpath)
                os.remove(fpath)
                removed_files += 1
            except OSError:
                pass
            continue
        m = HASH_RE.match(fname)
        if not m:
            continue
        prefix, h, ext = m.group(1), m.group(2), m.group(3) or ""
        try:
            mtime = os.path.getmtime(fpath)
        except OSError:
            continue

        if ext == "":
            exe_groups[prefix].append((mtime, h, fpath))
        elif ext == ".d" and os.path.isfile(fpath[:-2]):
            # Paired with an extensionless test executable; handled via exe_groups
            continue
        else:
            crate_key = prefix[3:] if prefix.startswith("lib") else prefix
            lib_hash_groups[crate_key][h].append((mtime, fpath))

    pruned_hashes = set()

    # In CI, delete all linked test executables before rust-cache saves target/
    # Locally, keep the newest linked executable per test target (up to 3 for crates
    # that have lib unit-test, bin, and bin unit-test binaries).
    for prefix, items in exe_groups.items():
        items.sort(reverse=True)
        keep = 0 if in_ci else (3 if prefix in ("rgctl", "rgctl_agent_pack_codegen", "rgctl-agent-pack-codegen") else 1)
        for _, stale_hash, stale_path in items[keep:]:
            if not in_ci:
                pruned_hashes.add(stale_hash)
            for candidate in (stale_path, f"{stale_path}.d"):
                if os.path.isfile(candidate):
                    try:
                        removed_bytes += os.path.getsize(candidate)
                        os.remove(candidate)
                        removed_files += 1
                    except OSError:
                        pass

    # Group library/metadata files (.rlib/.rmeta/.dylib/.d) by crate + hash so paired
    # .rlib and .rmeta files are never split. Keep 16 newest hashes per crate locally
    # (4 in CI) so the full working set fits without LRU eviction.
    for crate_key, hash_map in lib_hash_groups.items():
        ranked = []
        for h, file_list in hash_map.items():
            max_mtime = max(m for m, _ in file_list)
            ranked.append((max_mtime, h, file_list))
        ranked.sort(reverse=True)
        keep = 4 if in_ci else 16
        for _, stale_hash, file_list in ranked[keep:]:
            pruned_hashes.add(stale_hash)
            for _, stale_path in file_list:
                try:
                    removed_bytes += os.path.getsize(stale_path)
                    os.remove(stale_path)
                    removed_files += 1
                except OSError:
                    pass

    fp_dir = os.path.join(prof_dir, ".fingerprint")
    if pruned_hashes and os.path.isdir(fp_dir):
        for entry in os.listdir(fp_dir):
            m = HASH_RE.match(entry)
            if m and m.group(2) in pruned_hashes:
                shutil.rmtree(os.path.join(fp_dir, entry), ignore_errors=True)

if removed_files > 0:
    gib = removed_bytes / (1024 ** 3)
    print(f"\033[1;34m==> Post-run cleanup: pruned {removed_files} stale artifact(s) ({gib:.2f} GiB freed)\033[0m", flush=True)
PY
}

trap cleanup_preflight_artifacts EXIT

# Compile test targets in a single Cargo invocation (--no-run) and execute the
# resulting test binaries concurrently across CPU cores.
# Usage: run_cargo_tests_parallel <debug|release> <cargo-test-args...>
run_cargo_tests_parallel() {
  local profile_dir="$1"
  shift
  python3 - "$REPO_ROOT" "$profile_dir" "$@" <<'PY'
import concurrent.futures
import json
import os
import subprocess
import sys
import time

repo_root = sys.argv[1]
profile_dir = sys.argv[2]
cargo_args = sys.argv[3:]

cmd = ["cargo", "test", "--no-run", "--message-format=json-diagnostic-rendered-ansi", *cargo_args]
proc = subprocess.Popen(
    cmd,
    cwd=repo_root,
    stdout=subprocess.PIPE,
    stderr=None,
    text=True,
)

executables = []
seen = set()
assert proc.stdout is not None
for line in proc.stdout:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except json.JSONDecodeError:
        print(line, flush=True)
        continue
    reason = msg.get("reason")
    if reason == "compiler-message":
        rendered = msg.get("message", {}).get("rendered")
        if rendered:
            sys.stderr.write(rendered)
            sys.stderr.flush()
    elif reason == "compiler-artifact":
        if msg.get("profile", {}).get("test"):
            exe = msg.get("executable")
            if exe and exe not in seen:
                seen.add(exe)
                target_name = msg.get("target", {}).get("name", os.path.basename(exe))
                pkg_id = msg.get("package_id", "")
                pkg_dir = repo_root
                if pkg_id.startswith("path+file://"):
                    pkg_path = pkg_id[len("path+file://"):].split("#")[0]
                    if os.path.isdir(pkg_path):
                        pkg_dir = pkg_path
                executables.append((target_name, exe, pkg_dir))

ret = proc.wait()
if ret != 0:
    sys.exit(ret)

if not executables:
    sys.exit(0)

env = os.environ.copy()
rgctl_bin = os.path.join(repo_root, "target", profile_dir, "rgctl")
if os.path.isfile(rgctl_bin):
    env["CARGO_BIN_EXE_rgctl"] = rgctl_bin
sysroot_lib = subprocess.check_output(["rustc", "--print", "target-libdir"], text=True).strip()
deps_dir = os.path.join(repo_root, "target", profile_dir, "deps")
env["DYLD_FALLBACK_LIBRARY_PATH"] = f"{deps_dir}:{sysroot_lib}:" + env.get("DYLD_FALLBACK_LIBRARY_PATH", "")
env["LD_LIBRARY_PATH"] = f"{deps_dir}:{sysroot_lib}:" + env.get("LD_LIBRARY_PATH", "")

max_workers = min(len(executables), 4)
print(
    f"Running {len(executables)} integration test binary target(s) in parallel (workers={max_workers})...",
    flush=True,
)

def run_one(item):
    name, exe, cwd = item
    t0 = time.monotonic()
    res = subprocess.run(
        [exe, "--color", "always"],
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    dt = time.monotonic() - t0
    return name, exe, res.returncode, dt, res.stdout

failures = []
t_start = time.monotonic()
with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as pool:
    futures = [pool.submit(run_one, item) for item in executables]
    for fut in concurrent.futures.as_completed(futures):
        name, exe, code, dt, out = fut.result()
        summary = ""
        for line in reversed(out.splitlines()):
            if "test result:" in line:
                summary = line.strip()
                break
        if code == 0:
            print(f"  \033[1;32m✓\033[0m {name:<42} ({dt:5.2f}s) {summary}", flush=True)
        else:
            print(f"  \033[1;31m✗\033[0m {name:<42} ({dt:5.2f}s) FAILED (exit {code})", flush=True)
            failures.append((name, exe, code, out))

total_dt = time.monotonic() - t_start
if failures:
    print("\n\033[1;31m=== Failed Test Suites ===\033[0m", flush=True)
    for name, exe, code, out in failures:
        print(f"\n--- {name} ({exe}) [exit {code}] ---\n{out}", flush=True)
    sys.exit(1)

print(f"Completed {len(executables)} test binary target(s) in {total_dt:.2f}s wall time.", flush=True)
PY
}

# ==============================================================================
# Tier 1: Format, Clippy & Commit Policy (.github/workflows/ci-lint.yml)
# ==============================================================================
if [[ "$RUN_LINT" -eq 1 ]]; then
  if [[ -z "$BASE_REF" ]]; then
    for candidate in upstream/main origin/main main; do
      if git rev-parse --verify --quiet "$candidate" >/dev/null; then
        BASE_REF="$candidate"
        break
      fi
    done
  fi

  if [[ -n "$BASE_REF" ]]; then
    MERGE_BASE="$(git merge-base "$BASE_REF" HEAD 2>/dev/null || true)"
    if [[ -n "$MERGE_BASE" && "$MERGE_BASE" != "$(git rev-parse HEAD)" ]]; then
      step "Tier 1 (ci-lint): Checking commits in ${MERGE_BASE:0:8}..HEAD (no AI trailers & DCO sign-off)"
      python3 scripts/check-no-ai-commit-trailers.py --range "${MERGE_BASE}..HEAD" --require-dco
      pass "Commit range checks passed (no AI trailers, DCO sign-off present)"
    fi
  fi

  if [[ "$FIX_FMT" -eq 1 ]]; then
    step "Tier 1 (ci-lint): Formatting workspace (cargo fmt --all)"
    cargo fmt --all
  fi

  step "Tier 1 (ci-lint): cargo fmt --all -- --check"
  cargo fmt --all -- --check
  pass "Formatting check passed"

  step "Tier 1 (ci-lint): cargo clippy --locked --workspace --lib --bins -- -D warnings"
  cargo clippy --locked --features "${FEATURES}" \
    --workspace --lib --bins -- -D warnings
  pass "All CI clippy gates (--locked, --workspace, -D warnings) passed"

  step "Tier 1 (ci-lint): Typecheck all workspace unit & integration test targets"
  cargo check --locked --features "${FEATURES}" --workspace --lib --bins --tests
  pass "All workspace unit & integration test targets typechecked cleanly"
fi

# ==============================================================================
# Tier 2: Unit & Core Integration Tests (.github/workflows/ci.yml)
# ==============================================================================
if [[ "$RUN_TEST" -eq 1 ]]; then
  step "Tier 2 (ci): cargo test --features \"${FEATURES}\" --workspace --exclude rgctl-macros --lib --bins --no-fail-fast"
  cargo test --features "${FEATURES}" --workspace --exclude rgctl-macros --lib --bins --no-fail-fast
  if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
    cleanup_preflight_artifacts
  fi

  step "Tier 2 (ci): Core integration contracts (single compile pass + parallel execution)"
  run_cargo_tests_parallel debug --features "${FEATURES}" \
    --workspace \
    --test cli_output \
    --test single_pass_complexity_parity \
    --test slicing \
    --test taint_analysis \
    --test community_audit \
    --test centrality_audit \
    --test incremental_performance_integration
  pass "Tier 2 unit & core integration tests passed"
fi

# ==============================================================================
# Tier 3: QE, E2E & AST Coverage (.github/workflows/ci-qe.yml — opt-in)
# ==============================================================================
if [[ "$RUN_QE" -eq 1 ]]; then
  step "Tier 3 (ci-qe): Scrubbing gitignored .rgctl caches in test fixtures"
  find rgctl-tests tests/fixtures -name ".rgctl" -type d -prune -exec rm -rf {} + 2>/dev/null || true

  step "Tier 3 (ci-qe): Running QE & E2E suites + AST Handler Runtime Coverage Gate (single compile pass + parallel execution)"
  if [[ "$RUN_TEST" -eq 0 ]]; then
    # If Tier 2 was skipped (--qe standalone), also run unit tests of coverage & language crates
    RGCTL_AST_COVERAGE_TRACK=1 cargo test --features "${FEATURES}" --workspace --exclude rgctl-macros --lib --no-fail-fast
    if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
      cleanup_preflight_artifacts
    fi
  fi
  RGCTL_AST_COVERAGE_TRACK=1 run_cargo_tests_parallel debug --features "${FEATURES}" \
    --workspace \
    --test map_collision_qe \
    --test graph_correctness \
    --test semantic_search_qe \
    --test cross_feature_qe \
    --test graph_determinism_qe \
    --test stable_key_invariance_qe \
    --test columnar_zero_cold_touch_gate \
    --test e2e_workflow_equivalence \
    --test e2e_feature_contracts \
    --test inheritance_edges \
    --test cross_language_mro_and_di \
    --test c_depth_extraction \
    --test cpp_depth_extraction \
    --test csharp_depth_extraction \
    --test go_depth_extraction \
    --test java_depth_extraction \
    --test javascript_depth_extraction \
    --test kotlin_depth_extraction \
    --test php_depth_extraction \
    --test python_depth_extraction \
    --test ruby_depth_extraction \
    --test rust_depth_extraction \
    --test typescript_depth_extraction
  pass "Tier 3 QE, E2E & AST coverage gates passed"
fi

# ==============================================================================
# Tier 4: Release Perf, Scale & Bench (.github/workflows/ci-perf.yml — opt-in)
# ==============================================================================
if [[ "$RUN_PERF" -eq 1 ]]; then
  # Unify release & bench profile settings so bin rgctl, release integration tests,
  # and benchmark smoke check share 100% of compiled .rlibs (1 compile instead of 3)
  # and skip 8-minute single-threaded fat LTO during smoke gates.
  export CARGO_PROFILE_RELEASE_PANIC=unwind
  export CARGO_PROFILE_RELEASE_STRIP=false
  export CARGO_PROFILE_BENCH_STRIP=false
  export CARGO_PROFILE_RELEASE_LTO=off
  export CARGO_PROFILE_BENCH_LTO=off
  export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
  export CARGO_PROFILE_BENCH_CODEGEN_UNITS=16

  step "Tier 4 (ci-perf): Release CLI golden paths & scale/perf gates (single compile pass + parallel execution)"
  run_cargo_tests_parallel release --release --features "${FEATURES}" \
    --workspace \
    --test subprocess_golden_path \
    --test all_commands_sanity \
    --test blast_radius_perf \
    --test scale_perf_ci_gate \
    --test community_audit \
    --test centrality_audit \
    --test centrality_approx_scale

  step "Tier 4 (ci-perf): Benchmark compilation smoke check (cargo check --release --workspace --benches)"
  cargo check --release --features "${FEATURES}" --workspace --benches
  pass "Tier 4 release perf, scale & benchmark gates passed"
fi

printf '\n\033[1;32m============================================================\033[0m\n'
printf '\033[1;32m  rgctl pre-flight PR check PASSED (lint=%s test=%s qe=%s perf=%s)\033[0m\n' \
  "$RUN_LINT" "$RUN_TEST" "$RUN_QE" "$RUN_PERF"
printf '\033[1;32m============================================================\033[0m\n'
