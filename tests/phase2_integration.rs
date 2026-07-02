//! Phase 2 integration tests: analysis and config scanning

use rbuilder::analysis::{ComplexityAnalyzer, DependencyAnalyzer};
use rbuilder::config::analyzer::ConfigAnalyzer;
use rbuilder::config::secret_detector::SecretDetector;
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, Node, NodeType};
use rbuilder::graph::CodeGraph;
use std::fs;
use tempfile::TempDir;

fn sample_graph() -> CodeGraph {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    let f1 = Node::new(NodeType::Function, "main".to_string());
    let f2 = Node::new(NodeType::Function, "helper".to_string());
    let cfg = Node::new(NodeType::ConfigKey, "database.host".to_string());
    let id1 = f1.id;
    let id2 = f2.id;
    let cfg_id = cfg.id;
    let _ = cfg_id;

    backend.insert_node(f1).unwrap();
    backend.insert_node(f2).unwrap();
    backend.insert_node(cfg).unwrap();
    backend
        .insert_edge(Edge::new(
            id1,
            id2,
            rbuilder::graph::schema::EdgeType::Calls,
        ))
        .unwrap();
    graph
}

#[test]
fn test_complexity_analysis() {
    let mut graph = sample_graph();
    let mut node = Node::new(NodeType::Function, "complex".to_string());
    node.properties
        .insert("cyclomatic".to_string(), "18".to_string());
    graph.backend_mut().insert_node(node).unwrap();

    let report = ComplexityAnalyzer::analyze(graph.backend()).unwrap();
    assert!(report.functions.iter().any(|f| f.cyclomatic == 18));
}

#[test]
fn test_circular_dependency_via_analyzer() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let a = Node::new(NodeType::Function, "a".to_string());
    let b = Node::new(NodeType::Function, "b".to_string());
    let id_a = a.id;
    let id_b = b.id;
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend
        .insert_edge(Edge::new(
            id_a,
            id_b,
            rbuilder::graph::schema::EdgeType::Calls,
        ))
        .unwrap();
    backend
        .insert_edge(Edge::new(
            id_b,
            id_a,
            rbuilder::graph::schema::EdgeType::Calls,
        ))
        .unwrap();

    let cycles = DependencyAnalyzer::find_circular_dependencies(backend).unwrap();
    assert!(!cycles.is_empty());
}

#[test]
fn test_unused_config_via_analyzer() {
    let mut graph = sample_graph();
    let unused = Node::new(NodeType::ConfigKey, "legacy.feature".to_string());
    graph.backend_mut().insert_node(unused).unwrap();

    let keys = ConfigAnalyzer::find_unused_keys(graph.backend()).unwrap();
    assert!(keys.iter().any(|k| k.key == "legacy.feature"));
    assert!(keys.iter().any(|k| k.key == "database.host"));
}

#[test]
fn test_secret_scanner_on_file() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("config.yaml");
    fs::write(&path, "api_key: sk_live_1234567890abcdef\n").unwrap();
    let content = fs::read_to_string(path).unwrap();
    let secrets = SecretDetector::new().scan(&content);
    assert!(!secrets.is_empty());
}

#[test]
fn test_end_to_end_repo_graph_roundtrip() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("main.rs"),
        "fn main() {}\nfn helper() {}\n",
    )
    .unwrap();

    let graph = rbuilder::code_graph_from_repository(temp.path()).unwrap();
    graph.save_to_repo(temp.path()).unwrap();

    let loaded = CodeGraph::load_from_repo(temp.path()).unwrap();
    let functions = loaded
        .backend()
        .collect_nodes_by_type(NodeType::Function)
        .unwrap();
    assert!(functions.len() >= 2);
}
