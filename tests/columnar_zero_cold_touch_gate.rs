//! Zero cold-extension deserialization on hot columnar scan paths.

use rgctl_analysis::BlastRadiusEngine;
use rgctl_analysis::{
    CommunityDetector, FastPageRank, FlatGraphIndex, PetGraphView, default_community_edge_types,
};
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::{
    NoopDiffSink, SnapshotNodeStore, cold_ext_deserialization_count, diff_snapshots,
    reset_cold_ext_deserialization_count, write_columnar_from_backend,
    write_columnar_from_nodes_edges,
};
use rgctl_incremental::incoming_callers_files_depth;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

/// The cold-extension deserialization counter is process-global while test
/// threads run in parallel; tests that reset/assert it must serialize so one
/// test's materializations cannot land inside another's reset→assert window.
static COLD_COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock_cold_counter() -> std::sync::MutexGuard<'static, ()> {
    COLD_COUNTER_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_chain_graph(path: &Path, n: usize) -> Vec<Uuid> {
    let mut backend = MemoryBackend::new();
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let node = Node::new(NodeType::Function, format!("f{i}"))
            .with_file_path(format!("src/f{i}.rs"))
            .with_qualified_name(format!("mod::f{i}"))
            .with_location(1, 10)
            .with_property("note".into(), format!("cold-{i}"));
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
    let _cold = lock_cold_counter();
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
    let _cold = lock_cold_counter();
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
    let _cold = lock_cold_counter();
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
    let _cold = lock_cold_counter();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("g.bin");
    let a = Node::new(NodeType::Function, "a")
        .with_file_path("a.rs")
        .with_qualified_name("a")
        .with_location(1, 2)
        .with_property("x".into(), "y".into());
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

#[test]
fn fast_pagerank_and_leiden_zero_cold_ext() {
    let _cold = lock_cold_counter();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("g.bin");
    write_chain_graph(&path, 2_000);
    let store = SnapshotNodeStore::open(&path).unwrap();

    reset_cold_ext_deserialization_count();
    let view = PetGraphView::from_snapshot_store(&store).unwrap();
    let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
    let _ranks = FastPageRank::new(20, 0.85).compute_flat(&index);
    let _communities = CommunityDetector::new()
        .detect_with_view_filtered(&view, default_community_edge_types())
        .unwrap();
    assert_eq!(
        cold_ext_deserialization_count(),
        0,
        "FastPageRank + Fast-Leiden must not cold-deserialize NodeExtension"
    );
}

#[test]
fn hot_name_and_type_scan_zero_cold_ext() {
    let _cold = lock_cold_counter();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("g.bin");
    write_chain_graph(&path, 2_000);
    let store = SnapshotNodeStore::open(&path).unwrap();
    let col = store.mmap().columnar().expect("columnar");

    reset_cold_ext_deserialization_count();
    let mut matched = 0usize;
    for idx in 0..col.node_count() {
        if col.node_type_at(idx).unwrap() != NodeType::Function {
            continue;
        }
        let Some(qn) = col.node_qualified_name_str(idx).unwrap() else {
            continue;
        };
        if qn.starts_with("mod::") {
            let _name = col.node_name_str(idx).unwrap();
            let _path = col.node_file_path_str(idx).unwrap();
            matched += 1;
        }
    }
    assert!(
        matched >= 1_000,
        "expected hot qn prefix matches, got {matched}"
    );
    let all_ids = store.all_node_ids();
    let funcs = store.filter_function_impact(&all_ids).unwrap();
    assert_eq!(funcs.len(), 2_000);
    assert_eq!(
        cold_ext_deserialization_count(),
        0,
        "hot name/type/qn scan and filter_function_impact must not cold-deserialize NodeExtension"
    );
}

#[test]
fn dead_code_analyzer_zero_cold_ext_on_fully_reachable_graph() {
    // `rgctl dead-code` must stay zero-cold-touch when every node is reachable
    // from the entry point: the single `0..n` Pass 1 reads hot `NodeRow`
    // columns only, BFS walks the CSR, and Pass 3 skips reachable nodes
    // before any `materialize_node_at` (findings == 0 → 0 cold decodes).
    let _cold = lock_cold_counter();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("g.bin");
    let n = 2_000;
    let mut backend = MemoryBackend::new();
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let name = if i == 0 {
            "main".to_string()
        } else {
            format!("f{i}")
        };
        let node = Node::new(NodeType::Function, name)
            .with_file_path(format!("src/f{i}.rs"))
            .with_qualified_name(format!("mod::f{i}"))
            .with_location(1, 10)
            .with_property("note".into(), format!("cold-{i}"));
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for i in 0..n.saturating_sub(1) {
        backend
            .insert_edge(Edge::new(ids[i], ids[i + 1], EdgeType::Calls))
            .unwrap();
    }
    write_columnar_from_backend(&backend, &path).unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();

    reset_cold_ext_deserialization_count();
    let analyzer = rgctl_analysis::DeadCodeAnalyzer::new(&store).unwrap();
    let report = analyzer.analyze().unwrap();
    assert_eq!(
        cold_ext_deserialization_count(),
        0,
        "dead-code analyze() on a fully reachable graph must not cold-deserialize NodeExtension"
    );
    assert_eq!(
        report.summary.dead_symbols, 0,
        "chain from main is reachable"
    );
    assert!(
        report.summary.reachable_symbols > 0,
        "expected reachable symbols from the main root"
    );
}

#[test]
fn dead_code_analyzer_cold_ext_bounded_by_findings() {
    // With unreachable nodes, cold decodes are allowed only for confirmed
    // findings (CSR prechecks filter reachable / import / referenced nodes
    // before materialization), never O(N).
    let _cold = lock_cold_counter();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("g.bin");
    write_chain_graph(&path, 2_000);
    let store = SnapshotNodeStore::open(&path).unwrap();

    reset_cold_ext_deserialization_count();
    let analyzer = rgctl_analysis::DeadCodeAnalyzer::new(&store).unwrap();
    let report = analyzer.analyze().unwrap();
    let cold = cold_ext_deserialization_count();
    // No `main` root in this graph: every chain node is unreachable, but only
    // f0 has no inbound reference edge, so exactly one finding materializes.
    assert_eq!(
        report.summary.dead_symbols, 1,
        "only the chain head is dead"
    );
    assert_eq!(report.findings[0].symbol_name, "f0");
    assert_eq!(
        cold, 1,
        "cold decodes must equal the finding count (sparse materialization), got {cold}"
    );
}
