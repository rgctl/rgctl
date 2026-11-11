//! Snapshot diff benchmarks.
//!
//! Run: `cargo bench -p rgctl-graph --bench snapshot_diff`

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::snapshot::SnapshotNodeStore;
use rgctl_graph::snapshot_diff::{NoopDiffSink, SnapshotPair, diff_snapshots};
use rgctl_graph::write_columnar_from_nodes_edges;
use std::path::PathBuf;
use tempfile::TempDir;

fn build_snapshots(nodes: usize, edges: usize) -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let base_path = tmp.path().join("base.bin");
    let head_path = tmp.path().join("head.bin");

    let mut base_nodes = Vec::with_capacity(nodes);
    let mut head_nodes = Vec::with_capacity(nodes);
    let mut base_ids = Vec::with_capacity(nodes);
    let mut head_ids = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let base = Node::new(NodeType::Function, format!("fn{i}"))
            .with_file_path("src/a.rs")
            .with_byte_spans(
                Some((i * 100) as u32),
                Some((i * 100 + 60) as u32),
                Some(3),
                None,
                None,
            );
        let head_name = if i + 1 == nodes {
            format!("fn{i}_modified")
        } else {
            format!("fn{i}")
        };
        let head = Node::new(NodeType::Function, head_name)
            .with_file_path("src/a.rs")
            .with_byte_spans(
                Some((i * 100 + 50) as u32),
                Some((i * 100 + 110) as u32),
                Some(3),
                None,
                None,
            );
        base_ids.push(base.id);
        head_ids.push(head.id);
        base_nodes.push(base);
        head_nodes.push(head);
    }

    let mut base_edges = Vec::with_capacity(edges);
    let mut head_edges = Vec::with_capacity(edges);
    for e in 0..edges {
        base_edges.push(Edge::new(
            base_ids[e % nodes],
            base_ids[(e * 7 + 3) % nodes],
            EdgeType::Calls,
        ));
        head_edges.push(Edge::new(
            head_ids[e % nodes],
            head_ids[(e * 7 + 3) % nodes],
            EdgeType::Calls,
        ));
    }

    write_columnar_from_nodes_edges(base_nodes, base_edges, &base_path).unwrap();
    write_columnar_from_nodes_edges(head_nodes, head_edges, &head_path).unwrap();
    (tmp, base_path, head_path)
}

fn bench_digest_fast_path(c: &mut Criterion) {
    let (_tmp, base_path, _head_path) = build_snapshots(1_000, 4_000);
    let base = SnapshotNodeStore::open(&base_path).unwrap();
    let base_dup = SnapshotNodeStore::open(&base_path).unwrap();
    c.bench_function("digest_fast_path_equal", |b| {
        b.iter(|| black_box(base.content_digest().unwrap() == base_dup.content_digest().unwrap()))
    });
}

fn bench_node_index_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("node_index_parallel");
    for nodes in [5_000, 20_000] {
        let (_tmp, base_path, head_path) = build_snapshots(nodes, nodes * 4);
        let pair = SnapshotPair::open(&base_path, &head_path).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(nodes), &pair, |b, pair| {
            let mut sink = NoopDiffSink;
            b.iter(|| black_box(diff_snapshots(&pair.base, &pair.head, &mut sink).unwrap()))
        });
    }
    group.finish();
}

fn bench_edge_merge_join(c: &mut Criterion) {
    let mut group = c.benchmark_group("edge_merge_join");
    for edges in [10_000, 50_000] {
        let nodes = edges / 4;
        let (_tmp, base_path, head_path) = build_snapshots(nodes, edges);
        let pair = SnapshotPair::open(&base_path, &head_path).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(edges), &pair, |b, pair| {
            let mut sink = NoopDiffSink;
            b.iter(|| black_box(diff_snapshots(&pair.base, &pair.head, &mut sink).unwrap()))
        });
    }
    group.finish();
}

fn bench_full_diff_noop_sink(c: &mut Criterion) {
    let (_tmp, base_path, head_path) = build_snapshots(10_000, 40_000);
    let pair = SnapshotPair::open(&base_path, &head_path).unwrap();
    c.bench_function("full_diff_noop_sink", |b| {
        let mut sink = NoopDiffSink;
        b.iter(|| black_box(diff_snapshots(&pair.base, &pair.head, &mut sink).unwrap()))
    });
}

criterion_group!(
    benches,
    bench_digest_fast_path,
    bench_node_index_parallel,
    bench_edge_merge_join,
    bench_full_diff_noop_sink
);
criterion_main!(benches);
