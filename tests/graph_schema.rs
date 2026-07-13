//! Phase 12.0 — graph schema enrichment integration tests

use rbuilder::extraction::extractor::Extractor;
use rbuilder::extraction::graph_builder::GraphBuilder;
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::code_index::CodeIndex;
use rbuilder::graph::export::{export_json, import_json};
use rbuilder::graph::migration::migrate_v1_to_v2;
use rbuilder::graph::query;
use rbuilder::graph::schema::{CallType, Edge, EdgeType, Node, NodeType, GRAPH_SCHEMA_VERSION};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::semantic::signature::SignatureExtractor;
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn test_rust_signature_populates_first_class_fields() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("lib.rs");
    std::fs::write(&path, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

    let registry = LanguageRegistry::new().into();
    let extractor = Extractor::new(registry);
    let extraction = extractor.extract_file(&path).unwrap();

    let mut builder = GraphBuilder::new();
    extractor
        .populate_graph(&[extraction], &mut builder)
        .unwrap();

    let func = builder
        .nodes()
        .iter()
        .find(|n| n.name == "add" && n.node_type == NodeType::Function)
        .expect("add function node");

    assert!(func.signature_text().is_some());
    assert!(func.code_hash.is_some());
    assert!(!func.parameters.is_empty() || func.signature_text().unwrap().contains('('));
}

#[test]
fn test_query_signature_and_return_type_filters() {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    backend
        .insert_node(
            Node::new(NodeType::Function, "process".to_string())
                .with_signature("async fn process(data: &[u8]) -> Result<Vec<String>>")
                .with_return_type("Result<Vec<String>>"),
        )
        .unwrap();
    backend
        .insert_node(Node::new(NodeType::Function, "other".to_string()))
        .unwrap();

    let sig_hits = query::execute(&backend, "signature:*async*").unwrap();
    assert_eq!(sig_hits.len(), 1);
    assert_eq!(sig_hits[0].name, "process");

    let ret_hits = query::execute(&backend, "return_type:Result").unwrap();
    assert_eq!(ret_hits.len(), 1);
}

#[test]
fn test_graph_export_import_schema_version() {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    backend
        .insert_node(Node::new(NodeType::Function, "main".to_string()))
        .unwrap();

    let json = export_json(&backend).unwrap();
    let snapshot = import_json(&json).unwrap();
    assert_eq!(snapshot.schema_version, GRAPH_SCHEMA_VERSION);
}

#[test]
fn test_migration_promotes_legacy_properties() {
    let mut nodes = vec![Node::new(NodeType::Function, "legacy".to_string())
        .with_property("return_type".to_string(), "i32".to_string())];
    migrate_v1_to_v2(&mut nodes, &mut []);
    assert_eq!(nodes[0].return_type.as_deref(), Some("i32"));
}

#[test]
fn test_code_index_change_detection() {
    let mut index = CodeIndex::new();
    let loc = rbuilder::languages::plugin_trait::SourceLocation {
        file: "main.rs".into(),
        start_line: 1,
        end_line: 1,
        start_column: 0,
        end_column: 0,
    };
    let hash = index.add_code("fn old() {}", &loc);
    assert!(!CodeIndex::has_changed(&hash, "fn old() {}"));
    assert!(CodeIndex::has_changed(&hash, "fn new() {}"));
}

#[test]
fn test_signature_extractor_reads_first_class_fields() {
    let node = Node::new(NodeType::Function, "add".to_string())
        .with_signature("fn add(a: i32) -> i32")
        .with_return_type("i32".to_string());
    let sig = SignatureExtractor::from_node(&node).unwrap();
    assert_eq!(sig.return_type.as_deref(), Some("i32"));
}

#[test]
fn test_edge_call_type_roundtrip() {
    let edge =
        Edge::new(Uuid::new_v4(), Uuid::new_v4(), EdgeType::Calls).with_call_type(CallType::Direct);
    assert_eq!(edge.call_type, Some(CallType::Direct));
}
