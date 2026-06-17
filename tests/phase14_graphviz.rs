//! Phase 14: Graphviz DOT export tests.

use rbuilder::export::{generate_dot, GraphvizOptions, Layout, RankDir};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};

fn mixed_backend() -> rbuilder::graph::backend::MemoryBackend {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    let func = Node::new(NodeType::Function, "run".into());
    let cls = Node::new(NodeType::Class, "Runner".into());
    let id_f = func.id;
    let id_c = cls.id;
    backend.insert_node(func).unwrap();
    backend.insert_node(cls).unwrap();
    backend
        .insert_edge(Edge::new(id_c, id_f, EdgeType::Extends))
        .unwrap();
    backend
}

#[test]
fn test_dot_basic_digraph() {
    let backend = mixed_backend();
    let dot = generate_dot(&backend, "all", GraphvizOptions::default(), None).unwrap();
    assert!(dot.contains("digraph CodeGraph"));
    assert!(dot.contains("->"));
    assert!(dot.ends_with("}\n"));
}

#[test]
fn test_dot_node_shapes_by_type() {
    let backend = mixed_backend();
    let dot = generate_dot(&backend, "all", GraphvizOptions::default(), None).unwrap();
    assert!(dot.contains("shape=box"));
    assert!(dot.contains("shape=ellipse"));
}

#[test]
fn test_dot_edge_styles_by_type() {
    let backend = mixed_backend();
    let dot = generate_dot(&backend, "all", GraphvizOptions::default(), None).unwrap();
    assert!(dot.contains("style=dashed"));
    assert!(dot.contains("label=\"extends\""));
}

#[test]
fn test_dot_rankdir_horizontal() {
    let backend = mixed_backend();
    let dot = generate_dot(
        &backend,
        "all",
        GraphvizOptions {
            rankdir: RankDir::Lr,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    assert!(dot.contains("rankdir=LR"));
}

#[test]
fn test_dot_special_char_escaping() {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    backend
        .insert_node(Node::new(NodeType::Function, r#"fn "test""#.into()))
        .unwrap();
    let dot = generate_dot(&backend, "all", GraphvizOptions::default(), None).unwrap();
    assert!(dot.contains("\\\""));
}

#[test]
fn test_dot_layout_parsing() {
    use rbuilder::export::parse_layout;
    assert_eq!(parse_layout("neato"), Layout::Neato);
    assert_eq!(parse_layout("dot"), Layout::Dot);
}
