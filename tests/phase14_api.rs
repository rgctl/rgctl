//! Phase 14: Web API endpoint tests.

#![cfg(feature = "mcp-server")]

use axum::extract::{Path, Query, State};
use rbuilder::api::server::{
    dashboard_metrics, get_node, get_node_neighbors, graph_by_query, graph_stats,
    GraphQueryParams,
};
use rbuilder::api::state::AppState;
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::CodeGraph;
use tempfile::TempDir;

fn setup_graph() -> (TempDir, AppState, uuid::Uuid) {
    let temp = TempDir::new().unwrap();
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let a = Node::new(NodeType::Function, "alpha".into());
    let b = Node::new(NodeType::Function, "beta".into());
    let c = Node::new(NodeType::Class, "Gamma".into());
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
        .insert_edge(Edge::new(id_b, id_c, EdgeType::Uses))
        .unwrap();
    graph.save_to_repo(temp.path()).unwrap();
    let state = AppState::from_repo(temp.path()).unwrap();
    (temp, state, id_a)
}

#[tokio::test]
async fn test_api_stats_alias() {
    let (_temp, state, _) = setup_graph();
    let stats = graph_stats(State(state)).await.unwrap().0;
    assert_eq!(stats["node_count"], 3);
}

#[tokio::test]
async fn test_api_graph_by_query() {
    let (_temp, state, _) = setup_graph();
    let data = graph_by_query(
        State(state),
        Query(GraphQueryParams {
            query: Some("type:Function".into()),
            depth: None,
            limit: Some(50),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(data["nodes"].as_array().unwrap().len(), 2);
    assert!(!data["edges"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_api_get_node() {
    let (_temp, state, id_a) = setup_graph();
    let node = get_node(State(state), Path(id_a.to_string()))
        .await
        .unwrap()
        .0;
    assert_eq!(node["name"], "alpha");
}

#[tokio::test]
async fn test_api_get_node_neighbors() {
    let (_temp, state, id_a) = setup_graph();
    let data = get_node_neighbors(State(state), Path(id_a.to_string()))
        .await
        .unwrap()
        .0;
    let neighbors = data["neighbors"].as_array().unwrap();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0]["name"], "beta");
}

#[tokio::test]
async fn test_api_get_node_not_found() {
    let (_temp, state, _) = setup_graph();
    let result = get_node(
        State(state),
        Path(uuid::Uuid::new_v4().to_string()),
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_api_dashboard_metrics() {
    let (_temp, state, _) = setup_graph();
    let data = dashboard_metrics(State(state)).await.unwrap().0;
    assert_eq!(data["node_count"], 3);
    assert!(data["node_types"].is_object());
    assert!(data["complexity_histogram"].is_array());
    assert!(data["communities"].is_array());
    assert!(data["top_connected_nodes"].is_array());
    assert!(data["hotspots"].is_array());
}

#[tokio::test]
async fn test_api_graph_depth_expansion() {
    let (_temp, state, id_a) = setup_graph();
    let data = graph_by_query(
        State(state),
        Query(GraphQueryParams {
            query: Some(format!("name:alpha")),
            depth: Some(2),
            limit: Some(50),
        }),
    )
    .await
    .unwrap()
    .0;
    let names: Vec<_> = data["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    assert!(names.contains(&"Gamma"));
    let _ = id_a;
}

#[tokio::test]
async fn test_mcp_generate_diagram() {
    let (_temp, state, _) = setup_graph();
    let graph = state.with_graph(|g| Ok(g.clone())).unwrap();
    let result = rbuilder::mcp::tools::ToolExecutor::new(_temp.path())
        .execute(
            &graph,
            "generate_diagram",
            serde_json::json!({
                "query": "type:Function",
                "format": "mermaid",
                "diagram_type": "call-graph"
            }),
        )
        .unwrap();
    assert!(result["content"].as_str().unwrap().contains("graph TD"));
}
