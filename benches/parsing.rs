//! Parsing benchmarks
//!
//! Run with: cargo bench --bench parsing

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::pipeline::{PipelineConfig, ProcessingPipeline};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

fn bench_parallel_parsing(c: &mut Criterion) {
    let temp = TempDir::new().unwrap();
    for i in 0..100 {
        fs::write(
            temp.path().join(format!("file{i}.rs")),
            format!("fn func{i}() {{}}\n"),
        )
        .unwrap();
    }

    let registry = LanguageRegistry::new().into();
    c.bench_function("parse_100_files", |b| {
        b.iter(|| {
            let pipeline = ProcessingPipeline::with_config(
                Arc::clone(&registry),
                PipelineConfig {
                    show_progress: false,
                    thread_count: Some(4),
                    ..PipelineConfig::default()
                },
            );
            black_box(pipeline.process_repository(temp.path()).unwrap())
        });
    });
}

criterion_group!(benches, bench_parallel_parsing);
criterion_main!(benches);
