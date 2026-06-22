//! Phase 4 integration tests: IDL generation and domain learning

use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Node, NodeType};
use rbuilder::graph::CodeGraph;
use rbuilder::nlp::{PatternDetector, PatternMatcher};
use rbuilder::semantic::{IdlFormat, IdlGenerator, SignatureExtractor};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_idl_generation_pipeline() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("auth.rs"),
        "pub fn authenticate(token: String) -> bool { true }\n",
    )
    .unwrap();

    let graph = rbuilder::code_graph_from_repository(temp.path()).unwrap();
    let generator = IdlGenerator::new();
    let output_dir = temp.path().join("idl");
    let path = generator
        .write_module(graph.backend(), IdlFormat::Proto, "auth", &output_dir)
        .unwrap();

    let content = fs::read_to_string(path).unwrap();
    assert!(content.contains("syntax = \"proto3\""));
    assert!(content.contains("authenticate"));
}

#[test]
fn test_domain_aware_nlp() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    for i in 0..5 {
        let mut node = Node::new(NodeType::Class, format!("OrderService{i}"));
        node.labels.push("soa:service".to_string());
        backend.insert_node(node).unwrap();
    }

    let domain = PatternDetector::new().analyze(backend).unwrap();
    let matcher = PatternMatcher::new().with_domain(domain);
    let translated = matcher.translate("how many services?").unwrap();
    assert!(translated.internal_query.contains("soa:service"));
}

#[test]
fn test_signature_and_proto_roundtrip() {
    let sig = SignatureExtractor::from_source("fn add(a: i32, b: i32) -> i32").unwrap();
    let proto = IdlGenerator::new().generate_proto(&sig).unwrap();
    assert!(proto.contains("message AddRequest"));
}

#[test]
fn test_thrift_generation() {
    let sig =
        SignatureExtractor::from_source("fn calculate(price: f64, quantity: i64) -> f64").unwrap();
    let generator = IdlGenerator::new();
    let thrift = generator.generate(IdlFormat::Thrift, &sig).unwrap();

    // Verify Thrift-specific type mapping
    assert!(thrift.contains("namespace rs Calculate"));
    assert!(thrift.contains("struct CalculateRequest"));
    assert!(thrift.contains("double price")); // f64 -> double
    assert!(thrift.contains("i64 quantity")); // i64 -> i64
    assert!(thrift.contains("struct CalculateResponse"));
    assert!(thrift.contains("1: double result")); // return type is double, not int64
    assert!(thrift.contains("service CalculateService"));
}

#[test]
fn test_openapi_generation() {
    let sig =
        SignatureExtractor::from_source("fn get_user(id: i64, active: bool) -> String").unwrap();
    let generator = IdlGenerator::new();
    let openapi = generator.generate(IdlFormat::OpenApi, &sig).unwrap();

    // Verify OpenAPI-specific type mapping
    assert!(openapi.contains("openapi: \"3.0.0\""));
    assert!(openapi.contains("title: GetUser API"));
    assert!(openapi.contains("/get_user:"));
    assert!(openapi.contains("id:"));
    assert!(openapi.contains("type: integer")); // i64 -> integer, not int64
    assert!(openapi.contains("active:"));
    assert!(openapi.contains("type: boolean")); // bool -> boolean
    assert!(openapi.contains("result:"));
    assert!(openapi.contains("type: string")); // String -> string
}
