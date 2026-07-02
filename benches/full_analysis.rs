// Benchmark for full repository analysis
// Run with: cargo bench --bench full_analysis

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rbuilder_pipeline::{PipelineConfig, ProcessingPipeline};
use rbuilder_registry::LanguageRegistry;

fn bench_kafka_indexing(c: &mut Criterion) {
    let kafka_path = Path::new("example/kafka");

    if !kafka_path.exists() {
        eprintln!("Skipping benchmark: example/kafka not found");
        return;
    }

    let mut group = c.benchmark_group("kafka_analysis");

    // Set longer measurement time for large repo
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(10);

    group.bench_function("indexing_only", |b| {
        b.iter(|| {
            let registry = Arc::new(LanguageRegistry::new());
            let config = PipelineConfig {
                show_progress: false,
                ..PipelineConfig::default()
            };
            let pipeline = ProcessingPipeline::with_config(registry, config);

            // Run just the indexing phase
            let _result = pipeline.process_repository(kafka_path).unwrap();
        });
    });

    group.finish();
}

fn bench_graph_operations(c: &mut Criterion) {
    // Benchmark individual graph operations after loading
    let kafka_path = Path::new("example/kafka");

    if !kafka_path.exists() {
        return;
    }

    // Pre-build the graph
    let registry = Arc::new(LanguageRegistry::new());
    let config = PipelineConfig {
        show_progress: false,
        ..PipelineConfig::default()
    };
    let pipeline = ProcessingPipeline::with_config(registry, config);
    let (graph, _stats) = pipeline.process_repository(kafka_path).unwrap();

    let mut group = c.benchmark_group("graph_operations");

    group.bench_function("node_count", |b| {
        b.iter(|| graph.node_count());
    });

    group.bench_function("edge_count", |b| {
        b.iter(|| graph.edge_count());
    });

    group.bench_function("find_functions", |b| {
        b.iter(|| {
            use rbuilder_graph::schema::NodeType;
            graph.find_by_type(NodeType::Function).unwrap()
        });
    });

    group.finish();
}

criterion_group!(benches, bench_kafka_indexing, bench_graph_operations);
criterion_main!(benches);
