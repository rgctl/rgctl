//! Blast-radius performance benchmarks with hard regression gates.
//!
//! Run: `cargo bench --bench blast_radius_benchmarks`
//! Ignored gates: `cargo test --release --test blast_radius_perf -- --ignored`

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use rbuilder::analysis::{BlastEngineSnapshot, BlastRadiusEngine, PetGraphView};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::{CodeGraph, MmappedGraphSnapshot, PreparedGraphSnapshot, SnapshotNodeStore};
use std::time::{Duration, Instant};

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

fn bench_blast_analyze_small(c: &mut Criterion) {
    let graph = build_monorepo_mock(5_000, 25_000);
    let backend = graph.backend();
    let engine = BlastRadiusEngine::build(backend).unwrap();
    let target = backend.find_nodes_by_name("n2500").unwrap()[0].id;

    c.bench_function("blast_analyze_5k", |b| {
        b.iter(|| black_box(engine.analyze(target).unwrap()));
    });

    let start = Instant::now();
    engine.analyze(target).unwrap();
    let latency = start.elapsed();
    assert!(
        latency < Duration::from_millis(5),
        "blast analyze regression: {latency:?} >= 5ms"
    );
}

fn bench_petgraph_from_prepared_150k(c: &mut Criterion) {
    if std::env::var("RBUILDER_BENCH_LARGE").is_err() {
        return;
    }

    let mut group = c.benchmark_group("blast_radius_large");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    let graph = build_monorepo_mock(150_000, 700_000);
    let prepared = PreparedGraphSnapshot::from_backend(graph.backend()).unwrap();
    group.throughput(Throughput::Elements(prepared.nodes.len() as u64));

    group.bench_function("petgraph_from_prepared_150k", |b| {
        b.iter(|| black_box(PetGraphView::from_prepared(&prepared).unwrap()));
    });

    let start = Instant::now();
    PetGraphView::from_prepared(&prepared).unwrap();
    let latency = start.elapsed();
    assert!(
        latency < Duration::from_secs(30),
        "br.load.petgraph_from_prepared_ms regression: {latency:?} >= 30s"
    );

    group.finish();
}

fn bench_engine_snapshot_roundtrip_150k(c: &mut Criterion) {
    if std::env::var("RBUILDER_BENCH_LARGE").is_err() {
        return;
    }

    let graph = build_monorepo_mock(150_000, 700_000);
    let backend = graph.backend();
    let engine = BlastRadiusEngine::build(backend).unwrap();
    let prepared = PreparedGraphSnapshot::from_backend(backend).unwrap();
    let snap = engine.to_engine_snapshot(prepared.content_digest.clone());

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("blast_engine.snapshot.bin");
    snap.write_to_path(&path).unwrap();

    c.bench_function("engine_snapshot_load_150k", |b| {
        b.iter(|| {
            let loaded = BlastEngineSnapshot::load_from_path(&path).unwrap();
            black_box(BlastRadiusEngine::from_engine_snapshot(loaded).unwrap());
        });
    });
}

fn bench_snapshot_open_150k(c: &mut Criterion) {
    if std::env::var("RBUILDER_BENCH_LARGE").is_err() {
        return;
    }

    let graph = build_monorepo_mock(150_000, 700_000);
    let prepared = PreparedGraphSnapshot::from_backend(graph.backend()).unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("graph.snapshot.bin");
    prepared.write_to_path(&path).unwrap();

    c.bench_function("snapshot_node_store_open_150k", |b| {
        b.iter(|| black_box(SnapshotNodeStore::open(&path).unwrap()));
    });

    let start = Instant::now();
    MmappedGraphSnapshot::open(&path).unwrap();
    let latency = start.elapsed();
    assert!(
        latency < Duration::from_secs(15),
        "br.load.graph_snapshot_ms regression: {latency:?} >= 15s"
    );
}

criterion_group!(
    benches,
    bench_blast_analyze_small,
    bench_petgraph_from_prepared_150k,
    bench_engine_snapshot_roundtrip_150k,
    bench_snapshot_open_150k,
);
criterion_main!(benches);
