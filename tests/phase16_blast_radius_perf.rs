//! Phase 16 blast-radius performance gates — snapshot, engine, and query paths.

use rbuilder::analysis::{
    BlastEngineSnapshot, BlastRadiusEngine, MacroCallLookupDb, MacroCallLookupRow,
    MacroIndexEntry, PetGraphView,
};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::{CodeGraph, PreparedGraphSnapshot, SnapshotNodeStore};
use std::time::{Duration, Instant};
use uuid::Uuid;

fn build_monorepo_mock(nodes: usize, edges: usize) -> CodeGraph {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let mut ids = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let node = Node::new(NodeType::Function, format!("n{i}"));
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for e in 0..edges {
        let from = ids[e % nodes];
        let to = ids[(e * 13 + 7) % nodes];
        if from != to {
            backend
                .insert_edge(Edge::new(from, to, EdgeType::Calls))
                .unwrap();
        }
    }
    graph
}

/// Pre-built engine analyze must stay sub-millisecond on warm engine.
#[test]
fn blast_analyze_warm_engine_under_1ms() {
    let graph = build_monorepo_mock(5_000, 25_000);
    let backend = graph.backend();
    let engine = BlastRadiusEngine::build(backend).unwrap();
    let target = backend.find_nodes_by_name("n1000").unwrap()[0].id;

    let start = Instant::now();
    engine.analyze(target).unwrap();
    let latency = start.elapsed();

    assert!(
        latency < Duration::from_millis(1),
        "br.query.analyze_ms regression: {latency:?} >= 1ms"
    );
}

/// SQLite unique-symbol lookup on a tiny in-memory DB.
#[test]
fn sqlite_unique_lookup_under_15ms() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("macro_call_index.db");

    let callers: Vec<String> = (0..100).map(|i| format!("caller_{i}")).collect();
    let impact: Vec<String> = (0..500).map(|i| format!("impact_{i}")).collect();
    MacroCallLookupDb::replace_all(
        &db_path,
        &[MacroCallLookupRow {
            symbol_name: "target_fn".into(),
            score: 42.0,
            direct_callers: callers,
            impact_zone: impact,
            direct_caller_ids: vec![],
            impact_zone_ids: vec![],
            node_id: Uuid::new_v4(),
        }],
    )
    .unwrap();

    let start = Instant::now();
    let row = MacroCallLookupDb::lookup(&db_path, "target_fn")
        .unwrap()
        .expect("row");
    let latency = start.elapsed();

    assert_eq!(row.symbol_name, "target_fn");
    assert!(
        latency < Duration::from_millis(15),
        "br.query.sqlite_unique_ms regression: {latency:?} >= 15ms"
    );
}

/// Candidate-table resolved lookup with a single unambiguous entry.
#[test]
fn sqlite_fqn_resolved_lookup_under_50ms() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("macro_call_index.db");
    let id = Uuid::new_v4();

    MacroCallLookupDb::replace_candidates(
        &db_path,
        &[MacroIndexEntry {
            id,
            symbol_name: "saveError".into(),
            class_name: Some("MRequest".into()),
            file_path: "src/MRequest.java".into(),
            score: 55.0,
            direct_caller_ids: vec![Uuid::new_v4()],
            impact_zone_ids: vec![Uuid::new_v4()],
            direct_callers: vec!["caller".into()],
            impact_zone: vec!["impact".into()],
            language: "java".into(),
            signature: Some("void saveError()".into()),
            canonical_fqn: "MRequest::saveError".into(),
        }],
    )
    .unwrap();

    let parsed = rbuilder::analysis::parse_fqn_symbol("MRequest::saveError", None, None);
    let start = Instant::now();
    let entry = MacroCallLookupDb::lookup_resolved(&db_path, &parsed)
        .unwrap()
        .expect("entry");
    let latency = start.elapsed();

    assert_eq!(entry.id, id);
    assert!(
        latency < Duration::from_millis(50),
        "br.query.sqlite_fqn_ms regression: {latency:?} >= 50ms"
    );
}

/// 150k mock: PetGraphView from prepared snapshot.
#[test]
#[ignore = "performance gate: run with `cargo test --release --test phase16_blast_radius_perf -- --ignored`"]
fn petgraph_from_prepared_150k_under_30s() {
    let graph = build_monorepo_mock(150_000, 700_000);
    let prepared = PreparedGraphSnapshot::from_backend(graph.backend()).unwrap();

    let start = Instant::now();
    PetGraphView::from_prepared(&prepared).unwrap();
    let latency = start.elapsed();

    assert!(
        latency < Duration::from_secs(30),
        "br.load.petgraph_from_prepared_ms regression: {latency:?} >= 30s"
    );
}

/// 150k mock: mmap snapshot open + node store index build.
#[test]
#[ignore = "performance gate: run with `cargo test --release --test phase16_blast_radius_perf -- --ignored`"]
fn snapshot_node_store_open_150k_under_15s() {
    let graph = build_monorepo_mock(150_000, 700_000);
    let prepared = PreparedGraphSnapshot::from_backend(graph.backend()).unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("graph.snapshot.bin");
    prepared.write_to_path(&path).unwrap();

    let start = Instant::now();
    let store = SnapshotNodeStore::open(&path).unwrap();
    let latency = start.elapsed();

    assert!(store.node_count() > 100_000);
    assert!(
        latency < Duration::from_secs(15),
        "br.load.snapshot_node_store_ms regression: {latency:?} >= 15s"
    );
}

/// 150k mock: blast engine snapshot load + hydrate.
#[test]
#[ignore = "performance gate: run with `cargo test --release --test phase16_blast_radius_perf -- --ignored`"]
fn engine_snapshot_load_150k_under_60s() {
    let graph = build_monorepo_mock(150_000, 700_000);
    let backend = graph.backend();
    let engine = BlastRadiusEngine::build(backend).unwrap();
    let digest = PreparedGraphSnapshot::from_backend(backend)
        .unwrap()
        .content_digest;
    let snap = engine.to_engine_snapshot(digest);

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("blast_engine.snapshot.bin");
    snap.write_to_path(&path).unwrap();

    let start = Instant::now();
    let loaded = BlastEngineSnapshot::load_from_path(&path).unwrap();
    BlastRadiusEngine::from_engine_snapshot(loaded).unwrap();
    let latency = start.elapsed();

    assert!(
        latency < Duration::from_secs(60),
        "br.load.engine_snapshot_ms regression: {latency:?} >= 60s"
    );
}
