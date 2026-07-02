//! Strategy 2 performance benchmarks — typed PetGraphView and blast-radius latency.
//!
//! Run: `cargo bench --bench graph_benchmarks`
//! Large memory audit: `RBUILDER_BENCH_LARGE=1 cargo bench --bench graph_benchmarks large`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rbuilder::analysis::graph_utils::PetGraphView;
use rbuilder::analysis::{BlastRadiusEngine, PolicyRegistry};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::CodeGraph;
use std::time::Duration;

fn build_typed_graph(nodes: usize, edges: usize) -> CodeGraph {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let mut ids = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let node = Node::new(NodeType::Function, format!("n{i}"));
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    let edge_types = [EdgeType::Calls, EdgeType::Contains, EdgeType::Uses];
    for e in 0..edges {
        let from = ids[e % nodes];
        let to = ids[(e * 13 + 7) % nodes];
        backend
            .insert_edge(Edge::new(from, to, edge_types[e % edge_types.len()]))
            .unwrap();
    }
    graph
}

fn bench_petgraph_view_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("petgraph_view_build");
    group.measurement_time(Duration::from_secs(10));
    for (nodes, edges) in [(5_000, 20_000), (10_000, 50_000), (25_000, 100_000)] {
        let graph = build_typed_graph(nodes, edges);
        group.throughput(Throughput::Elements(edges as u64));
        group.bench_with_input(
            BenchmarkId::new("typed", format!("{nodes}n_{edges}e")),
            &graph,
            |b, graph| {
                b.iter(|| black_box(PetGraphView::from_backend(graph.backend()).unwrap()));
            },
        );
    }
    group.finish();
}

fn bench_incoming_filtered(c: &mut Criterion) {
    let graph = build_typed_graph(10_000, 50_000);
    let view = PetGraphView::from_backend(graph.backend()).unwrap();
    let hub = graph
        .backend()
        .find_nodes_by_name("n0")
        .unwrap()
        .pop()
        .unwrap()
        .id;
    let idx = view.uuid_to_index[&hub];

    c.bench_function("incoming_filtered_calls_10k", |b| {
        b.iter(|| {
            black_box(
                view.incoming_filtered(idx, &[EdgeType::Calls])
                    .count(),
            )
        });
    });
}

fn bench_analyze_with_policy(c: &mut Criterion) {
    let mut group = c.benchmark_group("analyze_with_policy");
    group.measurement_time(Duration::from_secs(8));

    let chain = {
        let mut graph = CodeGraph::new();
        let backend = graph.backend_mut();
        let mut ids = Vec::new();
        for i in 0..1_000 {
            let n = Node::new(NodeType::Function, format!("c{i}"));
            ids.push(n.id);
            backend.insert_node(n).unwrap();
        }
        for w in ids.windows(2) {
            backend
                .insert_edge(Edge::new(w[0], w[1], EdgeType::Calls))
                .unwrap();
        }
        graph
    };

    let star = {
        let mut graph = CodeGraph::new();
        let backend = graph.backend_mut();
        let hub = Node::new(NodeType::Function, "hub".into());
        let hub_id = hub.id;
        backend.insert_node(hub).unwrap();
        for i in 0..1_000 {
            let leaf = Node::new(NodeType::Function, format!("s{i}"));
            let id = leaf.id;
            backend.insert_node(leaf).unwrap();
            backend
                .insert_edge(Edge::new(id, hub_id, EdgeType::Calls))
                .unwrap();
        }
        graph
    };

    let mesh = {
        let mut graph = CodeGraph::new();
        let backend = graph.backend_mut();
        let size = 200;
        let mut ids = Vec::new();
        for i in 0..size {
            let n = Node::new(NodeType::Function, format!("m{i}"));
            ids.push(n.id);
            backend.insert_node(n).unwrap();
        }
        for i in 0..size {
            for j in 0..size {
                if i != j && (i + j) % 5 == 0 {
                    backend
                        .insert_edge(Edge::new(ids[i], ids[j], EdgeType::Calls))
                        .unwrap();
                }
            }
        }
        graph
    };

    let registry = PolicyRegistry::permissive();

    for (label, graph, symbol) in [
        ("deep_chain_1000", chain, "c999"),
        ("star_1000", star, "s0"),
        ("mesh_200", mesh, "m50"),
    ] {
        let backend = graph.backend();
        let engine = BlastRadiusEngine::build(backend).unwrap();
        let target = backend
            .find_nodes_by_name(symbol)
            .unwrap()
            .pop()
            .unwrap()
            .id;

        group.bench_with_input(BenchmarkId::new("topology", label), &target, |b, &target| {
            b.iter(|| {
                black_box(
                    engine
                        .analyze_with_policy(
                            target,
                            Some(backend),
                            Some(&registry),
                            None,
                        )
                        .unwrap(),
                )
            });
        });
    }
    group.finish();
}

fn bench_large_graph_optional(c: &mut Criterion) {
    if std::env::var("RBUILDER_BENCH_LARGE").is_err() {
        return;
    }
    let mut group = c.benchmark_group("large");
    group.sample_size(10);
    let graph = build_typed_graph(150_000, 1_000_000);
    group.bench_function("petgraph_view_150k_1m", |b| {
        b.iter(|| black_box(PetGraphView::from_backend(graph.backend()).unwrap()));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_petgraph_view_build,
    bench_incoming_filtered,
    bench_analyze_with_policy,
    bench_large_graph_optional,
);
criterion_main!(benches);
