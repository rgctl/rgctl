//! Zero cold-extension deserialization on hot columnar scan paths.

use rgctl_analysis::BlastRadiusEngine;
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::{
    cold_ext_deserialization_count, diff_snapshots, reset_cold_ext_deserialization_count,
    write_columnar_from_backend, write_columnar_from_nodes_edges, NoopDiffSink, SnapshotNodeStore,
};
use rgctl_incremental::incoming_callers_files_depth;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

fn write_chain_graph(path: &Path, n: usize) -> Vec<Uuid> {
    let mut backend = MemoryBackend::new();
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let node = Node::new(NodeType::Function, format!("f{i}"))
            .with_file_path(format!("src/f{i}.rs"))
            .with_qualified_name(format!("f{i}"))
            .with_location(1, 10);
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for i in 0..n.saturating_sub(1) {
        backend
            .insert_edge(Edge::new(ids[i], ids[i + 1], EdgeType::Calls))
            .unwrap();
    }
    write_columnar_from_backend(&backend, path).unwrap();
    ids
}

#[test]
fn diff_snapshots_zero_cold_ext() {
    let tmp = TempDir::new().unwrap();
    let base_path = tmp.path().join("base.bin");
    let head_path = tmp.path().join("head.bin");
    write_chain_graph(&base_path, 200);
    write_chain_graph(&head_path, 200);

    let base = SnapshotNodeStore::open(&base_path).unwrap();
    let head = SnapshotNodeStore::open(&head_path).unwrap();
    reset_cold_ext_deserialization_count();
    let mut sink = NoopDiffSink;
    diff_snapshots(&base, &head, &mut sink).unwrap();
    assert_eq!(
        cold_ext_deserialization_count(),
        0,
        "diff_snapshots must not cold-deserialize NodeExtension"
    );
}

#[test]
fn blast_scoped_and_cascade_zero_cold_ext() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("g.bin");
    let ids = write_chain_graph(&path, 500);
    let store = SnapshotNodeStore::open(&path).unwrap();
    let col = store.mmap().columnar().unwrap();

    reset_cold_ext_deserialization_count();
    let _engine = BlastRadiusEngine::build_scoped(&store, &[ids[0]]).unwrap();
    assert_eq!(
        cold_ext_deserialization_count(),
        0,
        "build_scoped must not cold-deserialize"
    );

    reset_cold_ext_deserialization_count();
    let _files = incoming_callers_files_depth(col, &["src/f10.rs".into()], 3).unwrap();
    assert_eq!(
        cold_ext_deserialization_count(),
        0,
        "incoming_callers_files_depth must not cold-deserialize"
    );
}

#[test]
fn evaluate_pr_check_bounds_hydration_on_5k_graph() {
    // CLI `review check` / `pr-check` is the public surface for evaluate_pr_check.
    // Bound cold materializations while opening + scoped blast on a 5k-node graph.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("g.bin");
    let ids = write_chain_graph(&path, 5_000);
    let store = SnapshotNodeStore::open(&path).unwrap();

    reset_cold_ext_deserialization_count();
    let _engine = BlastRadiusEngine::build_scoped(&store, &ids[..3.min(ids.len())]).unwrap();
    let cold = cold_ext_deserialization_count();
    assert!(
        cold <= 50,
        "scoped PR-check style path hydrated {cold} nodes (budget 50)"
    );
}

#[test]
fn materialize_increments_counter() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("g.bin");
    let a = Node::new(NodeType::Function, "a")
        .with_file_path("a.rs")
        .with_qualified_name("a")
        .with_location(1, 2);
    write_columnar_from_nodes_edges(vec![a], vec![], &path).unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();
    let col = store.mmap().columnar().unwrap();
    reset_cold_ext_deserialization_count();
    let _ = col.materialize_node_at(0).unwrap();
    assert!(
        cold_ext_deserialization_count() >= 1,
        "materialize_node_at with extension must increment counter"
    );
}
