//! Release-mode synthetic 25k-node / 80k-edge columnar scale gate (< 15s total).

use rgctl_analysis::{
    build_exact_report, evaluate_temporal, BlastRadiusEngine, ExactCloneOptions, PolicyRegistry,
};
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::{
    cold_ext_deserialization_count, diff_snapshots, reset_cold_ext_deserialization_count,
    write_columnar_from_backend, DeltaSegment, GraphCompactor, NoopDiffSink, SnapshotNodeStore,
};
use std::collections::HashMap;
use std::time::Instant;
use tempfile::TempDir;
use uuid::Uuid;

const NODES: usize = 25_000;
const EDGES: usize = 80_000;
const BUDGET_SECS: f64 = 15.0;

fn build_synthetic_backend() -> (MemoryBackend, Vec<Uuid>) {
    let mut backend = MemoryBackend::new();
    let mut ids = Vec::with_capacity(NODES);
    for i in 0..NODES {
        let node = Node::new(NodeType::Function, format!("f{i}"))
            .with_file_path(format!("src/f{}.rs", i % 500))
            .with_qualified_name(format!("mod::f{i}"))
            .with_location(1, 20);
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    // Deterministic edge pattern: each node calls next few neighbors (wrap).
    let mut e = 0usize;
    let mut step = 1usize;
    while e < EDGES {
        let from = e % NODES;
        let to = (from + step) % NODES;
        backend
            .insert_edge(Edge::new(ids[from], ids[to], EdgeType::Calls))
            .unwrap();
        e += 1;
        if e % NODES == 0 {
            step += 1;
        }
    }
    (backend, ids)
}

#[test]
fn scale_25k_80k_within_budget() {
    let started = Instant::now();
    let tmp = TempDir::new().unwrap();
    let (backend, ids) = build_synthetic_backend();

    let base_path = tmp.path().join("base.bin");
    let t0 = Instant::now();
    write_columnar_from_backend(&backend, &base_path).unwrap();
    eprintln!("write_columnar_from_backend: {:.3}s", t0.elapsed().as_secs_f64());

    // Slightly different head: drop one edge by rewriting with one fewer call.
    let mut head_backend = MemoryBackend::new();
    for id in &ids {
        let n = backend.get_node(*id).unwrap().unwrap();
        head_backend.insert_node(n).unwrap();
    }
    let mut count = 0usize;
    backend
        .for_each_edge(|e| {
            if count + 1 < EDGES {
                head_backend.insert_edge(e.clone()).unwrap();
            }
            count += 1;
        })
        .unwrap();
    let head_path = tmp.path().join("head.bin");
    write_columnar_from_backend(&head_backend, &head_path).unwrap();

    let base = SnapshotNodeStore::open(&base_path).unwrap();
    let head = SnapshotNodeStore::open(&head_path).unwrap();

    reset_cold_ext_deserialization_count();
    let t1 = Instant::now();
    let mut sink = NoopDiffSink;
    diff_snapshots(&base, &head, &mut sink).unwrap();
    eprintln!("diff_snapshots: {:.3}s", t1.elapsed().as_secs_f64());
    assert_eq!(
        cold_ext_deserialization_count(),
        0,
        "diff_snapshots cold_ext must be 0"
    );

    let col = base.mmap().columnar().unwrap();
    let mut delta = DeltaSegment::new();
    delta.invalidate_file("src/f0.rs");
    let replacement = Node::new(NodeType::Function, "f0")
        .with_file_path("src/f0.rs")
        .with_qualified_name("mod::f0_new")
        .with_location(1, 20);
    delta.new_nodes.push(replacement);
    let out = tmp.path().join("compact.bin");
    let t2 = Instant::now();
    GraphCompactor::new(col, delta)
        .compact_to_path(&out, tmp.path())
        .unwrap();
    eprintln!(
        "GraphCompactor::compact_to_path: {:.3}s",
        t2.elapsed().as_secs_f64()
    );

    let t3 = Instant::now();
    let registry = PolicyRegistry::permissive();
    let empty_centrality = HashMap::new();
    let base_be = MemoryBackend::new();
    let head_be = MemoryBackend::new();
    let base_eng = BlastRadiusEngine::build(&base_be).unwrap();
    let head_eng = BlastRadiusEngine::build(&head_be).unwrap();
    evaluate_temporal(
        &base,
        &head,
        &base_be,
        &head_be,
        &head_eng,
        &base_eng,
        &[], // scoped: empty key set
        &registry,
        &empty_centrality,
    )
    .unwrap();
    eprintln!(
        "evaluate_temporal (scoped empty): {:.3}s",
        t3.elapsed().as_secs_f64()
    );

    let t4 = Instant::now();
    let _report = build_exact_report(&base, ExactCloneOptions::default()).unwrap();
    eprintln!("clone detection: {:.3}s", t4.elapsed().as_secs_f64());

    let total = started.elapsed().as_secs_f64();
    eprintln!("total: {total:.3}s (budget {BUDGET_SECS})");
    assert!(
        total < BUDGET_SECS,
        "scale_perf_ci_gate exceeded budget: {total:.3}s >= {BUDGET_SECS}s"
    );
}
