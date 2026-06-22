//! Phase 11 polyglot repository parsing benchmarks.
//!
//! Run with: `cargo bench --features bundle-extended --bench phase11_polyglot`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rbuilder::discovery::DiscoveryConfig;
use rbuilder::extraction::extractor::Extractor;
use rbuilder::extraction::graph_builder::GraphBuilder;
use rbuilder::languages::registry::LanguageRegistry;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

fn write_polyglot_repo(root: &Path, extra_rs_files: usize) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join(".github/workflows")).unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();

    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("src/util.py"), "def helper(): pass\n").unwrap();
    fs::write(root.join("src/app.ts"), "export function helper() {}\n").unwrap();
    fs::write(
        root.join("schema.sql"),
        "CREATE TABLE users (id INTEGER);\n",
    )
    .unwrap();
    fs::write(root.join("Dockerfile"), "FROM alpine:3.19\n").unwrap();
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    runs-on: ubuntu-latest\n",
    )
    .unwrap();
    fs::write(root.join("scripts/deploy.sh"), "deploy() { true; }\n").unwrap();

    for i in 0..extra_rs_files {
        fs::write(
            root.join(format!("src/bench_{i}.rs")),
            format!("fn f_{i}() {{}}\n"),
        )
        .unwrap();
    }
}

fn bench_polyglot_repo(c: &mut Criterion) {
    let temp = TempDir::new().unwrap();
    write_polyglot_repo(temp.path(), 100);

    let registry = LanguageRegistry::new().into();
    let discovery = DiscoveryConfig::default();

    c.bench_function("phase11_polyglot_extract_100_files", |b| {
        b.iter(|| {
            let extractor = Extractor::new(Arc::clone(&registry));
            let extractions = extractor
                .extract_repository(temp.path(), &discovery)
                .unwrap();
            let mut builder = GraphBuilder::new();
            extractor
                .populate_graph(&extractions, &mut builder)
                .unwrap();
            black_box((
                extractions.len(),
                builder.node_count(),
                builder.edge_count(),
            ))
        });
    });
}

criterion_group!(benches, bench_polyglot_repo);
criterion_main!(benches);
