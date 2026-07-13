//! Centrality performance benchmarks with hard regression gates.
//!
//! Run: `cargo bench --bench centrality_benchmarks`
//! Large gate: `cargo test --release --test centrality_audit -- --ignored`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rbuilder::analysis::{FastPageRank, FlatGraphIndex, PetGraphView};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::CodeGraph;
use std::path::Path;
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

fn bench_petgraph_view_centrality_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("petgraph_view_centrality_large");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(15));

    let graph = build_monorepo_mock(150_000, 700_000);
    let view = PetGraphView::from_backend(graph.backend()).unwrap();
    let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
    let engine = FastPageRank::new(20, 0.85);

    group.throughput(Throughput::Elements(index.flat_edges.len() as u64));
    group.bench_function("pagerank_150k_700k_calls", |b| {
        b.iter(|| {
            black_box(engine.compute_flat(&index));
        });
    });

    // Hard gate embedded in bench warmup path (fails loudly on regression).
    let start = Instant::now();
    engine.compute_flat(&index);
    let latency = start.elapsed();
    assert!(
        latency < Duration::from_millis(20),
        "Algorithmic scale regression detected: {latency:?}"
    );

    group.finish();
}

fn bench_kafka_centrality_optional(c: &mut Criterion) {
    let kafka_path = Path::new("example/kafka");
    if !kafka_path.exists() {
        return;
    }

    use rbuilder_pipeline::{PipelineConfig, ProcessingPipeline};
    use rbuilder_registry::LanguageRegistry;
    use std::sync::Arc;

    let registry = Arc::new(LanguageRegistry::new());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        },
    );
    let (graph, _) = pipeline
        .process_repository(kafka_path)
        .expect("kafka index");
    let view = PetGraphView::from_backend(graph.backend()).unwrap();
    let engine = FastPageRank::new(20, 0.85);

    c.bench_function("kafka_behavioral_pagerank", |b| {
        b.iter(|| black_box(engine.compute(&view, &[EdgeType::Calls])));
    });
}

criterion_group!(
    benches,
    bench_petgraph_view_centrality_large,
    bench_kafka_centrality_optional,
);
criterion_main!(benches);
