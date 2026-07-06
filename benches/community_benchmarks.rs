//! Community detection performance benchmarks with regression gates.
//!
//! Run: `cargo bench --bench community_benchmarks`

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use rbuilder::analysis::{default_community_edge_types, CommunityDetector, PetGraphView};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::CodeGraph;
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

fn bench_petgraph_view_community_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("petgraph_view_community_large");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    let graph = build_monorepo_mock(150_000, 700_000);
    let view = PetGraphView::from_backend(graph.backend()).unwrap();
    let detector = CommunityDetector::new();
    let allowed = default_community_edge_types();

    group.throughput(Throughput::Elements(150_000));
    group.bench_function("label_propagation_150k_700k", |b| {
        b.iter(|| {
            black_box(detector.detect_with_view_filtered(&view, allowed).unwrap());
        });
    });

    let start = Instant::now();
    detector.detect_with_view_filtered(&view, allowed).unwrap();
    let execution_time = start.elapsed();
    assert!(
        execution_time < Duration::from_millis(150),
        "Community detection algorithm regressed! Time taken: {execution_time:?}"
    );

    group.finish();
}

criterion_group!(benches, bench_petgraph_view_community_large);
criterion_main!(benches);
