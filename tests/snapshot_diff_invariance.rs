//! Inserting comment lines must not produce false-positive `NodeDeltaKind::Changed`.

use rgctl_extraction::{DiscoveryConfig, Extractor, FileDiscoverer, GraphBuilder};
use rgctl_graph::schema::NodeType;
use rgctl_graph::{
    NodeDeltaKind, SnapshotNodeStore, VecDiffSink, assert_graph_invariants, diff_snapshots,
    write_columnar_from_nodes_edges,
};
use rgctl_pipeline::stream::{DEFAULT_STREAM_CHANNEL_CAPACITY, stream_into_graph};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

fn shared_registry() -> Arc<rgctl_registry::LanguageRegistry> {
    static REGISTRY: std::sync::OnceLock<Arc<rgctl_registry::LanguageRegistry>> =
        std::sync::OnceLock::new();
    Arc::clone(REGISTRY.get_or_init(|| Arc::new(rgctl_languages::default_registry())))
}

fn write_snapshot(root: &Path, out: &Path) {
    let registry = shared_registry();
    let files = FileDiscoverer::with_config(Arc::clone(&registry), DiscoveryConfig::default())
        .discover(root)
        .expect("discover");
    let extractor = Extractor::new(Arc::clone(&registry));
    let mut builder = GraphBuilder::new();
    let (_stats, tails, _lossless) = stream_into_graph(
        Some(1),
        &extractor,
        registry,
        &files,
        DEFAULT_STREAM_CHANNEL_CAPACITY,
        &mut builder,
        || {},
    )
    .expect("extract");
    builder.build_resolution_indexes();
    extractor
        .populate_pass2(&tails, &mut builder)
        .expect("pass2");
    let (nodes, edges) = builder.into_graph();
    assert!(
        nodes.iter().any(|n| matches!(
            n.node_type,
            NodeType::Function | NodeType::Class | NodeType::Struct | NodeType::Interface
        )),
        "expected symbol nodes in {}",
        root.display()
    );
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    write_columnar_from_nodes_edges(nodes, edges, out).expect("write");
    let store = SnapshotNodeStore::open(out).unwrap();
    assert_graph_invariants(store.mmap().columnar().unwrap()).unwrap();
}

#[test]
fn fifty_comment_lines_produce_zero_changed_deltas() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("rust");
    fs::create_dir_all(&root).unwrap();
    let src_path = root.join("lib.rs");
    let source = "pub fn target_fn() {\n    let x = 1;\n}\n\
                  pub fn other_fn() {\n    let y = 2;\n}\n";
    fs::write(&src_path, source).unwrap();

    let base_snap = tmp.path().join("base.bin");
    write_snapshot(&root, &base_snap);

    let mut shifted = String::new();
    for _ in 0..50 {
        shifted.push_str("// comment shift line\n");
    }
    shifted.push_str(source);
    fs::write(&src_path, shifted).unwrap();

    let head_snap = tmp.path().join("head.bin");
    write_snapshot(&root, &head_snap);

    let base = SnapshotNodeStore::open(&base_snap).unwrap();
    let head = SnapshotNodeStore::open(&head_snap).unwrap();
    let mut sink = VecDiffSink::default();
    let stats = diff_snapshots(&base, &head, &mut sink).expect("diff");

    assert_eq!(stats.nodes_added, 0, "nodes_added");
    assert_eq!(stats.nodes_removed, 0, "nodes_removed");

    // File nodes may change (content hash / line span); symbol StableNodeKeys must not.
    let symbol_changed: Vec<_> = sink
        .nodes
        .iter()
        .filter(|e| e.kind == NodeDeltaKind::Changed)
        .filter(|e| {
            let ty = e
                .head
                .as_ref()
                .or(e.base.as_ref())
                .map(|n| n.node_type)
                .unwrap_or(NodeType::File);
            !matches!(ty, NodeType::File | NodeType::Module)
        })
        .collect();
    assert!(
        symbol_changed.is_empty(),
        "inserting 50 comment lines must not mark unchanged downstream symbols Changed; got {symbol_changed:?} (stats.nodes_changed={})",
        stats.nodes_changed
    );
}
