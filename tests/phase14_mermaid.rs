//! Phase 14: Mermaid diagram export tests.

use rbuilder::export::{generate_mermaid, DiagramType, MermaidOptions};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};

fn sample_backend() -> rbuilder::graph::backend::MemoryBackend {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    let func = Node::new(NodeType::Function, "authenticate".into());
    let cls = Node::new(NodeType::Class, "AuthService".into());
    let id_f = func.id;
    let id_c = cls.id;
    backend.insert_node(func).unwrap();
    backend.insert_node(cls).unwrap();
    backend
        .insert_edge(Edge::new(id_f, id_c, EdgeType::Calls))
        .unwrap();
    backend
}

#[test]
fn test_mermaid_flowchart_header() {
    let backend = sample_backend();
    let out = generate_mermaid(
        &backend,
        "all",
        MermaidOptions::default(),
    )
    .unwrap();
    assert!(out.starts_with("graph TD"));
    assert!(out.contains("authenticate"));
    assert!(out.contains("AuthService"));
}

#[test]
fn test_mermaid_function_node_shape() {
    let backend = sample_backend();
    let out = generate_mermaid(
        &backend,
        "name:authenticate",
        MermaidOptions::default(),
    )
    .unwrap();
    assert!(out.contains("authenticate"));
    assert!(out.contains('('));
}

#[test]
fn test_mermaid_class_diagram() {
    let backend = sample_backend();
    let out = generate_mermaid(
        &backend,
        "type:Class",
        MermaidOptions {
            diagram_type: DiagramType::ClassDiagram,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(out.starts_with("classDiagram"));
    assert!(out.contains("class AuthService"));
}

#[test]
fn test_mermaid_call_graph_filters_calls() {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    let a = Node::new(NodeType::Function, "a".into());
    let b = Node::new(NodeType::Function, "b".into());
    let id_a = a.id;
    let id_b = b.id;
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend
        .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
        .unwrap();

    let out = generate_mermaid(
        &backend,
        "functions",
        MermaidOptions {
            diagram_type: DiagramType::CallGraph,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(out.contains("|call|"));
    assert!(out.contains("a"));
    assert!(out.contains("b"));
}

#[test]
fn test_mermaid_horizontal_layout() {
    let backend = sample_backend();
    let out = generate_mermaid(
        &backend,
        "all",
        MermaidOptions {
            vertical: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(out.starts_with("graph LR"));
}

#[test]
fn test_mermaid_empty_query_errors() {
    let backend = rbuilder::graph::backend::MemoryBackend::new();
    let err = generate_mermaid(
        &backend,
        "name:missing",
        MermaidOptions::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("No nodes matched"));
}

#[test]
fn test_mermaid_escapes_quotes() {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    backend
        .insert_node(Node::new(NodeType::Function, r#"say "hi""#.into()))
        .unwrap();
    let out = generate_mermaid(&backend, "all", MermaidOptions::default()).unwrap();
    assert!(out.contains("\\\""));
}

#[test]
fn test_mermaid_max_depth_expansion() {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    let a = Node::new(NodeType::Function, "a".into());
    let b = Node::new(NodeType::Function, "b".into());
    let c = Node::new(NodeType::Function, "c".into());
    let id_a = a.id;
    let id_b = b.id;
    let id_c = c.id;
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend.insert_node(c).unwrap();
    backend
        .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_b, id_c, EdgeType::Calls))
        .unwrap();

    let out = generate_mermaid(
        &backend,
        "name:a",
        MermaidOptions {
            diagram_type: DiagramType::CallGraph,
            max_depth: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(out.contains("[\"a\"]"));
    assert!(out.contains("[\"b\"]"));
    assert!(!out.contains("[\"c\"]"));
}
