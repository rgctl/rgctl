#!/usr/bin/env bash
# Run the same steps as .github/workflows/ci.yml locally.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export FEATURES="${FEATURES:-bundle-full}"
export CARGO_TERM_COLOR=always
export RUST_BACKTRACE=1

echo "==> FEATURES=$FEATURES"
echo "==> rustc $(rustc --version)"

echo "==> cargo fmt"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --features "$FEATURES" --lib --bins -- -D warnings

echo "==> cargo test (workspace)"
cargo test --features "$FEATURES" --workspace --lib --bins --tests

echo "==> cargo test cli_output"
cargo test --features "$FEATURES" --test cli_output

echo "==> cargo test subprocess_golden_path (release)"
cargo test --release --features "$FEATURES" --test subprocess_golden_path

echo "==> cargo test all_commands_sanity (release)"
cargo test --release --features "$FEATURES" --test all_commands_sanity

echo "==> cargo test phase16_blast_radius_perf (release)"
cargo test --release --features "$FEATURES" --test phase16_blast_radius_perf

echo "==> CI local: all steps passed"
