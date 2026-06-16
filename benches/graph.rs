//! Graph operation benchmarks
//!
//! Run with: cargo bench --bench graph

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Node, NodeType};
use rbuilder::graph::CodeGraph;

fn build_labeled_graph(count: usize) -> CodeGraph {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    for i in 0..count {
        let mut node = Node::new(NodeType::Class, format!("Component{i}"));
        node.labels.push("react:component".to_string());
        backend.insert_node(node).unwrap();
    }
    graph
}

fn bench_query_by_label(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_by_label");
    for size in [1_000, 10_000, 100_000] {
        let graph = build_labeled_graph(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &graph, |b, graph| {
            b.iter(|| black_box(graph.backend().find_nodes_by_label("react:component").unwrap()));
        });
    }
    group.finish();
}

fn bench_query_by_type(c: &mut Criterion) {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    for i in 0..100_000 {
        backend
            .insert_node(Node::new(NodeType::Function, format!("fn{i}")))
            .unwrap();
    }

    c.bench_function("query_by_type_100k", |b| {
        b.iter(|| black_box(backend.find_nodes_by_type(NodeType::Function).unwrap()));
    });
}

criterion_group!(benches, bench_query_by_label, bench_query_by_type);
criterion_main!(benches);
