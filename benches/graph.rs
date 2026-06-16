//! Graph operation benchmarks
//!
//! Run with: cargo bench --bench graph

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
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

fn make_function_nodes(count: usize) -> Vec<Node> {
    (0..count)
        .map(|i| Node::new(NodeType::Function, format!("fn{i}")))
        .collect()
}

fn bench_insert_nodes_single_vs_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_nodes");
    for size in [1_000, 5_000, 10_000] {
        let nodes = make_function_nodes(size);

        group.bench_with_input(BenchmarkId::new("single", size), &nodes, |b, nodes| {
            b.iter(|| {
                let mut graph = CodeGraph::new();
                let backend = graph.backend_mut();
                for node in nodes {
                    backend.insert_node(node.clone()).unwrap();
                }
                black_box(backend.node_count())
            });
        });

        group.bench_with_input(BenchmarkId::new("batch", size), &nodes, |b, nodes| {
            b.iter(|| {
                let mut graph = CodeGraph::new();
                graph.load(nodes.clone(), vec![]).unwrap();
                black_box(graph.node_count())
            });
        });
    }
    group.finish();
}

fn bench_insert_edges_single_vs_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_edges");
    for size in [1_000, 5_000] {
        let nodes = make_function_nodes(size);
        let mut graph = CodeGraph::new();
        graph.load(nodes, vec![]).unwrap();
        let backend = graph.backend();
        let ids: Vec<_> = backend
            .find_nodes_by_type(NodeType::Function)
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();

        let edges: Vec<_> = ids
            .windows(2)
            .map(|w| Edge::new(w[0], w[1], EdgeType::Calls))
            .collect();

        group.bench_with_input(BenchmarkId::new("single", size), &edges, |b, edges| {
            b.iter(|| {
                let mut graph = CodeGraph::new();
                graph.load(make_function_nodes(size), vec![]).unwrap();
                let backend = graph.backend_mut();
                for edge in edges {
                    backend.insert_edge(edge.clone()).unwrap();
                }
                black_box(backend.edge_count())
            });
        });

        group.bench_with_input(BenchmarkId::new("batch", size), &edges, |b, edges| {
            b.iter(|| {
                let mut graph = CodeGraph::new();
                graph
                    .load(make_function_nodes(size), edges.clone())
                    .unwrap();
                black_box(graph.edge_count())
            });
        });
    }
    group.finish();
}

fn build_compound_query_graph() -> CodeGraph {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    for i in 0..5_000 {
        let repo = if i % 2 == 0 { "backend" } else { "frontend" };
        let node_type = if i % 10 == 0 {
            NodeType::Class
        } else {
            NodeType::Function
        };
        backend
            .insert_node(
                Node::new(node_type, format!("handler{i}"))
                    .with_property("repo".into(), repo.into()),
            )
            .unwrap();
    }

    backend
        .insert_node(
            Node::new(NodeType::Function, "needle".into())
                .with_property("repo".into(), "backend".into()),
        )
        .unwrap();

    graph
}

fn bench_compound_query_clause_order(c: &mut Criterion) {
    let graph = build_compound_query_graph();
    let backend = graph.backend();

    let mut group = c.benchmark_group("compound_query_order");
    for query in [
        "repo:backend|type:Function|name:needle",
        "type:Function|name:needle|repo:backend",
        "name:needle|repo:backend|type:Function",
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(query), &query, |b, query| {
            b.iter(|| black_box(rbuilder::graph::query::execute(backend, query).unwrap()));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_query_by_label,
    bench_query_by_type,
    bench_insert_nodes_single_vs_batch,
    bench_insert_edges_single_vs_batch,
    bench_compound_query_clause_order,
);
criterion_main!(benches);
