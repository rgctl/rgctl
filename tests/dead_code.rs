//! Integration tests for `rgctl dead-code` analysis.

use rgctl::analysis::dead_code::{
    DeadCodeAnalyzer, DeadCodeConfidence, DeadCodeKind, DeadCodeReport,
};
use rgctl::graph::backend::{GraphBackend, MemoryBackend};
use rgctl::graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl::graph::{SnapshotNodeStore, write_columnar_from_backend};

fn snapshot_from_backend(backend: &MemoryBackend) -> (SnapshotNodeStore, std::path::PathBuf) {
    // `write_columnar_from_backend` (not `PreparedGraphSnapshot::write_to_path`) so node
    // rows are written sorted by id: `SnapshotNodeStore::get_node` binary-searches the
    // node column, and `write_to_path` currently emits `from_backend` HashMap order
    // (unsorted), which makes lookups miss nodes nondeterministically. Tracked in a
    // separate issue; once `write_columnar_to_path` sorts internally this can revert.
    let path = std::env::temp_dir().join(format!("dc_integ_{}.bin", uuid::Uuid::new_v4()));
    rgctl::graph::write_columnar_from_backend(backend, &path).unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();
    (store, path)
}

fn analyze(backend: &MemoryBackend) -> (DeadCodeReport, std::path::PathBuf) {
    let (store, path) = snapshot_from_backend(backend);
    let analyzer = DeadCodeAnalyzer::new(&store).unwrap();
    let report = analyzer.analyze().unwrap();
    (report, path)
}

#[test]
fn unused_import_detected() {
    let mut backend = MemoryBackend::new();
    let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
    let used_imp = Node::new(NodeType::Import, "used_module").with_file_path("src/main.rs");
    let unused_imp = Node::new(NodeType::Import, "unused_module").with_file_path("src/main.rs");

    let main_id = main_fn.id;
    let used_imp_id = used_imp.id;

    backend.insert_node(main_fn).unwrap();
    backend.insert_node(used_imp).unwrap();
    backend.insert_node(unused_imp).unwrap();
    backend
        .insert_edge(Edge::new(main_id, used_imp_id, EdgeType::Uses))
        .unwrap();

    let (report, path) = analyze(&backend);

    let unused: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.kind == DeadCodeKind::UnusedImport)
        .collect();
    assert!(!unused.is_empty());
    assert!(unused.iter().any(|f| f.symbol_name == "unused_module"));
    assert!(
        unused
            .iter()
            .all(|f| f.confidence == DeadCodeConfidence::High)
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn unreferenced_function_detected() {
    let mut backend = MemoryBackend::new();
    let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
    let called = Node::new(NodeType::Function, "called_fn").with_file_path("src/lib.rs");
    let dead = Node::new(NodeType::Function, "dead_fn").with_file_path("src/lib.rs");

    let main_id = main_fn.id;
    let called_id = called.id;

    backend.insert_node(main_fn).unwrap();
    backend.insert_node(called).unwrap();
    backend.insert_node(dead).unwrap();
    backend
        .insert_edge(Edge::new(main_id, called_id, EdgeType::Calls))
        .unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        report
            .findings
            .iter()
            .any(|f| f.symbol_name == "dead_fn" && f.kind == DeadCodeKind::UnreferencedFunction)
    );
    assert!(!report.findings.iter().any(|f| f.symbol_name == "called_fn"));
    assert!(!report.findings.iter().any(|f| f.symbol_name == "main"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn public_barrel_export_treated_as_entry_point() {
    let mut backend = MemoryBackend::new();
    let public_fn = Node::new(NodeType::Function, "exported_api")
        .with_file_path("src/api.rs")
        .with_property("visibility".to_string(), "pub".to_string());
    let callee = Node::new(NodeType::Function, "internal_helper").with_file_path("src/api.rs");

    let pub_id = public_fn.id;
    let callee_id = callee.id;

    backend.insert_node(public_fn).unwrap();
    backend.insert_node(callee).unwrap();
    backend
        .insert_edge(Edge::new(pub_id, callee_id, EdgeType::Calls))
        .unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "exported_api"),
        "public API without inbound refs should be treated as entry point"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "internal_helper"),
        "helper called from entry point should be reachable"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_symbols_excluded_by_default() {
    let mut backend = MemoryBackend::new();
    let test_fn = Node::new(NodeType::Function, "test_login").with_file_path("tests/test_auth.rs");
    backend.insert_node(test_fn).unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "test_login"),
        "test symbols should not appear in findings by default"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn report_includes_source_locations() {
    let mut backend = MemoryBackend::new();
    let dead = Node::new(NodeType::Function, "orphan")
        .with_file_path("src/orphan.rs")
        .with_location(10, 25);
    backend.insert_node(dead).unwrap();

    let (report, path) = analyze(&backend);

    let finding = report
        .findings
        .iter()
        .find(|f| f.symbol_name == "orphan")
        .expect("orphan should be reported");
    assert_eq!(finding.file_path.as_deref(), Some("src/orphan.rs"));
    assert_eq!(finding.start_line, Some(10));
    assert_eq!(finding.end_line, Some(25));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn entry_point_config_overrides_default() {
    let mut backend = MemoryBackend::new();
    let custom_entry = Node::new(NodeType::Function, "plugin_init").with_file_path("src/plugin.rs");
    let helper = Node::new(NodeType::Function, "plugin_helper").with_file_path("src/plugin.rs");

    let entry_id = custom_entry.id;
    let helper_id = helper.id;

    backend.insert_node(custom_entry).unwrap();
    backend.insert_node(helper).unwrap();
    backend
        .insert_edge(Edge::new(entry_id, helper_id, EdgeType::Calls))
        .unwrap();

    let (store, path) = snapshot_from_backend(&backend);
    let analyzer = DeadCodeAnalyzer::new(&store)
        .unwrap()
        .with_entry_points(vec!["plugin_init".to_string()]);
    let report = analyzer.analyze().unwrap();

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "plugin_init"),
        "configured entry point should not be dead"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "plugin_helper"),
        "symbol called from entry point should be reachable"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn machine_readable_json_output() {
    let mut backend = MemoryBackend::new();
    let dead = Node::new(NodeType::Function, "json_dead")
        .with_file_path("src/dead.rs")
        .with_location(5, 15);
    backend.insert_node(dead).unwrap();

    let (store, path) = snapshot_from_backend(&backend);
    let analyzer = DeadCodeAnalyzer::new(&store).unwrap();
    let report = analyzer.analyze().unwrap();

    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert!(json["findings"].is_array());

    let findings = json["findings"].as_array().unwrap();
    if let Some(f) = findings.iter().find(|f| f["symbol_name"] == "json_dead") {
        assert!(f["kind"].is_string());
        assert!(f["confidence"].is_string());
        assert!(f["reason"].is_string());
        assert!(f["file_path"].is_string());
    }

    let _ = std::fs::remove_file(&path);
}
