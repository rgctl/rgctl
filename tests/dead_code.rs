//! Integration tests for `rgctl dead-code` analysis.

use rgctl::analysis::dead_code::{
    DeadCodeAnalyzer, DeadCodeConfidence, DeadCodeKind, DeadCodeReport,
};
use rgctl::graph::SnapshotNodeStore;
use rgctl::graph::backend::{GraphBackend, MemoryBackend};
use rgctl::graph::schema::{Edge, EdgeType, Node, NodeType};

fn snapshot_from_backend(backend: &MemoryBackend) -> (SnapshotNodeStore, std::path::PathBuf) {
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
fn unused_import_with_internal_target_detected() {
    let mut backend = MemoryBackend::new();
    let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
    let target_mod = Node::new(NodeType::Module, "used_module").with_file_path("src/used.rs");
    let used_imp = Node::new(NodeType::Import, "used_module").with_file_path("src/main.rs");
    let unused_imp = Node::new(NodeType::Import, "unused_internal").with_file_path("src/main.rs");
    let unused_target =
        Node::new(NodeType::Module, "unused_internal").with_file_path("src/unused.rs");

    let main_id = main_fn.id;
    let used_imp_id = used_imp.id;
    let target_mod_id = target_mod.id;
    let unused_imp_id = unused_imp.id;
    let unused_target_id = unused_target.id;

    backend.insert_node(main_fn).unwrap();
    backend.insert_node(target_mod).unwrap();
    backend.insert_node(used_imp).unwrap();
    backend.insert_node(unused_imp).unwrap();
    backend.insert_node(unused_target).unwrap();
    backend
        .insert_edge(Edge::new(main_id, used_imp_id, EdgeType::Uses))
        .unwrap();
    backend
        .insert_edge(Edge::new(used_imp_id, target_mod_id, EdgeType::Uses))
        .unwrap();
    backend
        .insert_edge(Edge::new(unused_imp_id, unused_target_id, EdgeType::Uses))
        .unwrap();

    let (report, path) = analyze(&backend);

    let unused: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.kind == DeadCodeKind::UnusedImport)
        .collect();
    assert!(
        unused.iter().any(|f| f.symbol_name == "unused_internal"),
        "import with internal target but no references should be flagged"
    );
    assert!(
        unused
            .iter()
            .all(|f| f.confidence == DeadCodeConfidence::High)
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn external_import_not_flagged() {
    let mut backend = MemoryBackend::new();
    let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
    let ext_import =
        Node::new(NodeType::Import, "std::collections::HashMap").with_file_path("src/main.rs");

    backend.insert_node(main_fn).unwrap();
    backend.insert_node(ext_import).unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.kind == DeadCodeKind::UnusedImport),
        "external imports without Imports edges should not be flagged"
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
fn unreferenced_public_fn_reported_as_low_confidence() {
    let mut backend = MemoryBackend::new();
    let public_fn = Node::new(NodeType::Function, "exported_api")
        .with_file_path("src/api.rs")
        .with_property("visibility".to_string(), "pub".to_string());

    backend.insert_node(public_fn).unwrap();

    let (report, path) = analyze(&backend);

    let finding = report
        .findings
        .iter()
        .find(|f| f.symbol_name == "exported_api");
    assert!(
        finding.is_some(),
        "unreferenced public function should be reported"
    );
    assert_eq!(
        finding.unwrap().confidence,
        DeadCodeConfidence::Low,
        "unreferenced public function should have Low confidence"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn public_fn_callees_still_reachable() {
    let mut backend = MemoryBackend::new();
    let public_fn = Node::new(NodeType::Function, "exported_api")
        .with_file_path("src/api.rs")
        .with_property("visibility".to_string(), "pub".to_string());
    let callee_fn = Node::new(NodeType::Function, "called_from_pub").with_file_path("src/api.rs");

    let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
    let main_id = main_fn.id;
    let pub_id = public_fn.id;
    let callee_id = callee_fn.id;

    backend.insert_node(main_fn).unwrap();
    backend.insert_node(public_fn).unwrap();
    backend.insert_node(callee_fn).unwrap();
    backend
        .insert_edge(Edge::new(main_id, pub_id, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(pub_id, callee_id, EdgeType::Calls))
        .unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "exported_api"),
        "public fn called from main should be reachable"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "called_from_pub"),
        "callee of reachable public fn should be reachable"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn entry_point_class_methods_reachable_via_contains() {
    let mut backend = MemoryBackend::new();
    let class = Node::new(NodeType::Class, "MyController")
        .with_file_path("src/controller.rs")
        .with_property("visibility".to_string(), "pub".to_string());
    let annotation = Node::new(NodeType::Annotation, "RestController");
    let method =
        Node::new(NodeType::Function, "handle_request").with_file_path("src/controller.rs");

    let class_id = class.id;
    let ann_id = annotation.id;
    let method_id = method.id;

    backend.insert_node(class).unwrap();
    backend.insert_node(annotation).unwrap();
    backend.insert_node(method).unwrap();
    backend
        .insert_edge(Edge::new(class_id, ann_id, EdgeType::AnnotatedWith))
        .unwrap();
    backend
        .insert_edge(Edge::new(class_id, method_id, EdgeType::Contains))
        .unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "handle_request"),
        "methods on entry-point classes should be reachable via Contains"
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
    assert!(json["summary"]["total_symbols"].is_number());
    assert!(json["summary"]["dead_symbols"].is_number());
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
