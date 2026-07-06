//! Phase 11.3 — automated polyglot performance threshold tests (CI-tracked).
#![allow(dead_code, unused_imports, unused_macros)]

#[path = "common/polyglot.rs"]
mod polyglot;

use polyglot::write_scaled_polyglot_repo;
use rbuilder::discovery::DiscoveryConfig;
use rbuilder::extraction::extractor::Extractor;
use rbuilder::extraction::graph_builder::GraphBuilder;
use rbuilder::languages::registry::LanguageRegistry;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;

/// Phase 11 target: analyze a scaled polyglot repo within two minutes.
const POLYGLOT_BENCH_MAX_SECS: u64 = 120;

/// Default scaled fixture size for CI (lightweight but non-trivial).
const POLYGLOT_BENCH_FILE_COUNT: usize = 100;

#[cfg(feature = "bundle-extended")]
#[test]
fn test_polyglot_benchmark_threshold() {
    let tmp = TempDir::new().unwrap();
    write_scaled_polyglot_repo(tmp.path(), POLYGLOT_BENCH_FILE_COUNT);

    let registry = LanguageRegistry::new().into();
    let extractor = Extractor::new(Arc::clone(&registry));
    let start = Instant::now();
    let extractions = extractor
        .extract_repository(tmp.path(), &DiscoveryConfig::default())
        .unwrap();
    let elapsed = start.elapsed();

    assert!(
        extractions.len() >= POLYGLOT_BENCH_FILE_COUNT,
        "expected at least {POLYGLOT_BENCH_FILE_COUNT} extractions, got {}",
        extractions.len()
    );

    let mut builder = GraphBuilder::new();
    extractor
        .populate_graph(&extractions, &mut builder)
        .unwrap();
    assert!(builder.node_count() > POLYGLOT_BENCH_FILE_COUNT);

    assert!(
        elapsed.as_secs() < POLYGLOT_BENCH_MAX_SECS,
        "polyglot benchmark took {:?} (limit {POLYGLOT_BENCH_MAX_SECS}s)",
        elapsed
    );

    eprintln!(
        "phase11_polyglot_bench: files={} nodes={} edges={} elapsed_ms={}",
        extractions.len(),
        builder.node_count(),
        builder.edge_count(),
        elapsed.as_millis()
    );
}
