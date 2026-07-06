//! Phase 16 blast-radius performance gates — snapshot, engine, and query paths.

use rayon::prelude::*;
use rbuilder::analysis::{
    BlastEngineSnapshot, BlastRadiusEngine, MacroCallLookupDb, MacroCallLookupRow, MacroIndexEntry,
    PetGraphView,
};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::backend::MemoryBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::{CodeGraph, MmappedGraphSnapshot, PreparedGraphSnapshot, SnapshotNodeStore};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn bench_repo_root() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("RBUILDER_BENCH_REPO") {
        let path = PathBuf::from(path);
        if path.join(".rbuilder/blast_engine.snapshot.bin").exists() {
            return Some(path);
        }
        return None;
    }
    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/metasfresh-4.9.8b");
    if default.join(".rbuilder/blast_engine.snapshot.bin").exists() {
        Some(default)
    } else {
        None
    }
}

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

fn min_elapsed(iterations: u32, mut f: impl FnMut()) -> Duration {
    (0..iterations)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .min()
        .unwrap_or_default()
}

fn write_v1_snapshot(prepared: &PreparedGraphSnapshot, path: &std::path::Path) {
    use rbuilder::graph::snapshot::{SNAPSHOT_MAGIC, SNAPSHOT_VERSION_V1};
    use std::io::Write;
    let payload = bincode::serialize(prepared).unwrap();
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(&SNAPSHOT_MAGIC).unwrap();
    file.write_all(&SNAPSHOT_VERSION_V1.to_le_bytes()).unwrap();
    file.write_all(&(payload.len() as u64).to_le_bytes())
        .unwrap();
    file.write_all(&payload).unwrap();
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
        latency < Duration::from_secs(5),
        "br.load.engine_snapshot_ms regression: {latency:?} >= 5s"
    );
}

/// Real-repo soft gate: lazy engine snapshot load (skip when checkout/cache absent).
#[test]
fn bench_repo_engine_snapshot_lazy_load_under_5s() {
    let Some(repo) = bench_repo_root() else {
        eprintln!("skip bench_repo_engine_snapshot_lazy_load_under_5s: no RBUILDER_BENCH_REPO or metasfresh cache");
        return;
    };
    let path = repo.join(".rbuilder/blast_engine.snapshot.bin");

    let start = Instant::now();
    let loaded = BlastEngineSnapshot::load_from_path(&path).expect("load blast snapshot");
    let engine = BlastRadiusEngine::from_engine_snapshot(loaded).expect("hydrate engine");
    let latency = start.elapsed();

    assert!(
        engine.reachability_is_lazy(),
        "bench repo engine should use lazy ReachabilityStore"
    );
    assert!(
        latency < Duration::from_secs(5),
        "br.load.engine_snapshot_ms (metasfresh) regression: {latency:?} >= 5s"
    );
}

/// Real-repo soft gate: warm T1 lite analyze via engine snapshot (no macro index).
#[test]
fn bench_repo_lite_analyze_under_3s() {
    let Some(repo) = bench_repo_root() else {
        eprintln!(
            "skip bench_repo_lite_analyze_under_3s: no RBUILDER_BENCH_REPO or metasfresh cache"
        );
        return;
    };
    let graph_path = repo.join(".rbuilder/graph.snapshot.bin");
    let engine_path = repo.join(".rbuilder/blast_engine.snapshot.bin");
    if !graph_path.exists() || !engine_path.exists() {
        eprintln!("skip bench_repo_lite_analyze_under_3s: missing graph or engine snapshot");
        return;
    }

    let store = SnapshotNodeStore::open(&graph_path).expect("open graph snapshot");
    let digest = store.content_digest().expect("graph digest");
    let loaded = BlastEngineSnapshot::load_from_path(&engine_path).expect("load blast snapshot");
    let engine = BlastRadiusEngine::from_engine_snapshot(loaded).expect("hydrate engine");
    assert!(
        engine.reachability_is_lazy(),
        "bench repo engine should use lazy ReachabilityStore"
    );

    let target_name = std::env::var("RBUILDER_BENCH_SYMBOL").unwrap_or_else(|_| "saveError".into());
    let nodes = store.find_nodes_by_name(&target_name).expect("name lookup");
    assert!(
        !nodes.is_empty(),
        "bench symbol {target_name} not found in graph snapshot"
    );

    let start = Instant::now();
    engine.analyze(nodes[0].id).expect("analyze");
    let latency = start.elapsed();

    assert_eq!(digest, store.content_digest().expect("graph digest"));
    assert!(
        latency < Duration::from_secs(3),
        "br.query.lite_total_ms (metasfresh analyze) regression: {latency:?} >= 3s"
    );
}

/// Prepared-index hydrate should beat full batch re-index on mock scale.
#[test]
fn hydrate_prepared_faster_than_batch_reindex_5k() {
    let graph = build_monorepo_mock(5_000, 25_000);
    let prepared = PreparedGraphSnapshot::from_backend(graph.backend()).unwrap();

    let hydrate = min_elapsed(3, || {
        prepared.hydrate_backend().unwrap();
    });

    let batch = min_elapsed(3, || {
        let mut slow = MemoryBackend::new();
        slow.insert_nodes_batch(prepared.nodes.clone()).unwrap();
        slow.insert_edges_batch(prepared.edges.clone()).unwrap();
    });

    // At 5k scale both paths are sub-50ms; allow micro-variance on shared CI runners.
    assert!(
        hydrate <= batch.saturating_add(Duration::from_millis(15)),
        "br.load.backend_hydrate_ms regression: hydrate {hydrate:?} >> batch reindex {batch:?}"
    );
}

/// Columnar v2 open should beat legacy v1 bincode deserialize on mock scale.
#[test]
fn columnar_snapshot_open_faster_than_v1_5k() {
    let graph = build_monorepo_mock(5_000, 25_000);
    let prepared = PreparedGraphSnapshot::from_backend(graph.backend()).unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let v1_path = dir.path().join("v1.graph.snapshot.bin");
    let v2_path = dir.path().join("v2.graph.snapshot.bin");
    write_v1_snapshot(&prepared, &v1_path);
    prepared.write_to_path(&v2_path).unwrap();

    let v1_latency = min_elapsed(3, || {
        let mmap = MmappedGraphSnapshot::open(&v1_path).unwrap();
        assert!(!mmap.is_columnar());
    });

    let v2_latency = min_elapsed(3, || {
        let store = SnapshotNodeStore::open(&v2_path).unwrap();
        assert!(store.is_columnar());
    });

    // At 5k scale both opens are sub-50ms; allow micro-variance on shared CI runners.
    assert!(
        v2_latency <= v1_latency.saturating_add(Duration::from_millis(15)),
        "br.load.columnar_open_ms regression: v2={v2_latency:?} >> v1={v1_latency:?}"
    );
}

/// PetGraphView from columnar snapshot store (no full PreparedGraphSnapshot materialize).
#[test]
fn petgraph_from_snapshot_store_5k_under_500ms() {
    let graph = build_monorepo_mock(5_000, 25_000);
    let prepared = PreparedGraphSnapshot::from_backend(graph.backend()).unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("graph.snapshot.bin");
    prepared.write_to_path(&path).unwrap();

    let store = SnapshotNodeStore::open(&path).unwrap();
    assert!(store.is_columnar());

    let start = Instant::now();
    let view = PetGraphView::from_snapshot_store(&store).unwrap();
    let latency = start.elapsed();

    assert_eq!(view.directed.node_count(), 5_000);
    assert!(
        latency < Duration::from_millis(500),
        "br.load.petgraph_from_snapshot_store_ms regression: {latency:?} >= 500ms"
    );
}

/// Parallel blast analyze over all functions on warm engine (discover hot loop).
#[test]
fn parallel_blast_analyze_all_5k_under_2s() {
    let graph = build_monorepo_mock(5_000, 25_000);
    let backend = graph.backend();
    let prepared = PreparedGraphSnapshot::from_backend(backend).unwrap();
    let view = PetGraphView::from_prepared(&prepared).unwrap();
    let engine = BlastRadiusEngine::build_from_view(backend, &view).unwrap();
    let functions = backend.collect_nodes_by_type(NodeType::Function).unwrap();

    let start = Instant::now();
    let count = functions
        .par_iter()
        .filter(|func| engine.analyze(func.id).is_ok())
        .count();
    let latency = start.elapsed();

    assert_eq!(count, functions.len());
    assert!(
        latency < Duration::from_secs(2),
        "br.discover.analyze_all_ms regression: {latency:?} >= 2s"
    );
}

/// RSS delta after engine snapshot load stays bounded on mock scale.
#[test]
fn engine_snapshot_load_rss_delta_under_512mb_5k() {
    use rbuilder_core::memory::MemoryMonitor;

    let graph = build_monorepo_mock(5_000, 25_000);
    let backend = graph.backend();
    let engine = BlastRadiusEngine::build(backend).unwrap();
    let digest = PreparedGraphSnapshot::from_backend(backend)
        .unwrap()
        .content_digest;
    let snap = engine.to_engine_snapshot(digest);

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("blast_engine.snapshot.bin");
    snap.write_to_path(&path).unwrap();

    let monitor = MemoryMonitor::new();
    let before = monitor.current_mb();
    let loaded = BlastEngineSnapshot::load_from_path(&path).unwrap();
    BlastRadiusEngine::from_engine_snapshot(loaded).unwrap();
    let after = monitor.current_mb();

    assert!(
        after - before < 512.0,
        "br.load.engine_snapshot_rss_mb regression: delta {:.1} MB >= 512 MB",
        after - before
    );
}
