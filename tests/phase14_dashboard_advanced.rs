//! Phase 14 A+: advanced dashboard widget tests.

#![cfg(feature = "mcp-server")]

use rbuilder::analysis::{centrality, community};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::backend::MemoryBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};

fn sample_graph() -> MemoryBackend {
    let mut backend = MemoryBackend::new();

    let auth_login = Node::new(NodeType::Function, "auth_login".into())
        .with_file_path("src/auth/login.rs".into())
        .with_property("cyclomatic".into(), "15".into());
    let auth_verify = Node::new(NodeType::Function, "auth_verify".into())
        .with_file_path("src/auth/verify.rs".into())
        .with_property("cyclomatic".into(), "20".into());
    let id_login = auth_login.id;
    let id_verify = auth_verify.id;
    backend.insert_node(auth_login).unwrap();
    backend.insert_node(auth_verify).unwrap();
    backend
        .insert_edge(Edge::new(id_login, id_verify, EdgeType::Calls))
        .unwrap();

    let db_query = Node::new(NodeType::Function, "db_query".into());
    let db_connect = Node::new(NodeType::Function, "db_connect".into());
    let id_query = db_query.id;
    let id_conn = db_connect.id;
    backend.insert_node(db_query).unwrap();
    backend.insert_node(db_connect).unwrap();
    backend
        .insert_edge(Edge::new(id_query, id_conn, EdgeType::Calls))
        .unwrap();

    let hub = Node::new(NodeType::Function, "central_hub".into())
        .with_property("cyclomatic".into(), "25".into());
    let id_hub = hub.id;
    backend.insert_node(hub).unwrap();

    for id in [id_login, id_verify, id_query, id_conn] {
        backend
            .insert_edge(Edge::new(id_hub, id, EdgeType::Calls))
            .unwrap();
    }

    backend
}

#[test]
fn test_community_detection_finds_clusters() {
    let backend = sample_graph();
    let communities = community::detect_communities(&backend).unwrap();
    assert!(!communities.is_empty());
    assert!(communities.iter().any(|c| c.size >= 3));
}

#[test]
fn test_community_labels_inferred() {
    let backend = sample_graph();
    let communities = community::detect_communities(&backend).unwrap();
    assert!(
        communities
            .iter()
            .any(|c| c.label.contains("auth") || c.label.contains("cluster"))
    );
}

#[test]
fn test_degree_centrality_finds_hub() {
    let backend = sample_graph();
    let scores = centrality::degree_centrality(&backend).unwrap();
    let top = &scores[0];
    assert_eq!(top.name, "central_hub");
    assert!(top.degree >= 4);
}

#[test]
fn test_identify_hotspots_filters_correctly() {
    let backend = sample_graph();
    let hotspots = centrality::identify_hotspots(&backend).unwrap();
    for hotspot in &hotspots {
        assert!(hotspot.degree >= 3);
        assert!(hotspot.complexity.unwrap_or(0) >= 10);
    }
}

#[test]
fn test_hotspots_sorted_by_risk() {
    let backend = sample_graph();
    let hotspots = centrality::identify_hotspots(&backend).unwrap();
    if hotspots.len() >= 2 {
        assert!(hotspots[0].risk_score >= hotspots[1].risk_score);
    }
}

#[tokio::test]
async fn test_dashboard_advanced_endpoint() {
    use axum::extract::State;
    use rbuilder::api::server::dashboard_advanced;
    use rbuilder::api::state::AppState;
    use rbuilder::graph::CodeGraph;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    let mut graph = CodeGraph::new();
    *graph.backend_mut() = sample_graph();
    graph.save_to_repo(temp.path()).unwrap();

    let state = AppState::from_repo(temp.path()).unwrap();
    let response = dashboard_advanced(State(state)).await.unwrap().0;

    assert!(response.get("communities").is_some());
    assert!(response.get("hotspots").is_some());
    assert!(response.get("centrality").is_some());
    assert!(response["hotspots"].as_array().unwrap().iter().any(|h| {
        h.get("risk_score").and_then(|v| v.as_f64()).unwrap_or(0.0) > 0.0
    }));
}
