#!/usr/bin/env bash
# Semantic analysis verification pipeline (Phase 13 audit + performance gates).
set -euo pipefail

FEATURES="${RBUILDER_VERIFY_FEATURES:-bundle-minimal}"

echo "==> Semantic audit fixtures (phase13_semantic_audit)"
cargo test --features "$FEATURES" --test phase13_semantic_audit

echo "==> Interprocedural slicing + call-site parameter mapping (phase13_interprocedural)"
cargo test --features "$FEATURES" --test phase13_interprocedural

echo "==> Boundary, stress, and differential tests (phase13_semantic_boundary)"
cargo test --features "$FEATURES" --test phase13_semantic_boundary

echo "==> Taint policy boundary tests (phase13_taint)"
cargo test --features "$FEATURES" --test phase13_taint bypass
cargo test --features "$FEATURES" --test phase13_taint sanitizer

echo "==> Dominance latency gate (< 15 ms, phase13_analysis bench)"
# Reduced sample size keeps CI fast; assert inside the bench still enforces the gate.
cargo bench --features "$FEATURES" --bench phase13_analysis \
  phase13_dominance_1000_blocks -- --sample-size 10

echo "==> Semantic verification pipeline passed"
