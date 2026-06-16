//! REST API server for web-based graph browser

use crate::analysis::community::CommunityDetector;
use crate::analysis::complexity::ComplexityAnalyzer;
use crate::api::state::AppState;
use crate::error::Error;
use crate::graph::query;
use crate::graph::schema::{Edge, Node, NodeType};
use crate::nlp::pattern_matcher::PatternMatcher;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

/// Query request body.
#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    /// Natural language or DSL query
    pub question: Option<String>,
    /// Direct graph query DSL
    pub query: Option<String>,
}

/// Paginated node list parameters.
#[derive(Debug, Deserialize)]
pub struct NodeListParams {
    /// Page number (0-based)
    pub page: Option<usize>,
    /// Page size
    pub limit: Option<usize>,
    /// Filter by node type
    pub node_type: Option<String>,
    /// Filter by label
    pub label: Option<String>,
    /// Search query
    pub q: Option<String>,
}

/// Start the web API and static file server.
pub async fn run_server(state: AppState, port: u16, web_dir: Option<std::path::PathBuf>) -> crate::error::Result<()> {
    let app = build_router(state, web_dir);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    eprintln!("Web server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| Error::Other(format!("Failed to bind port {port}: {e}")))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| Error::Other(format!("HTTP server error: {e}")))?;

    Ok(())
}

/// Build the axum router (for testing).
pub fn build_router(state: AppState, web_dir: Option<std::path::PathBuf>) -> Router {
    let api = Router::new()
        .route("/api/graph/stats", get(graph_stats))
        .route("/api/graph/nodes", get(list_nodes))
        .route("/api/graph/edges", get(list_edges))
        .route("/api/graph/search", get(search_nodes))
        .route("/api/query", post(nlp_query))
        .route("/api/communities", get(list_communities))
        .with_state(state);

    if let Some(dir) = web_dir {
        if dir.exists() {
            return api.nest_service("/", ServeDir::new(dir)).layer(CorsLayer::permissive());
        }
    }

    api.layer(CorsLayer::permissive())
}

/// Get graph statistics including node/edge counts and complexity metrics.
pub async fn graph_stats(State(state): State<AppState>) -> std::result::Result<Json<Value>, ApiError> {
    let stats = state.with_graph(|graph| {
        let backend = graph.backend();
        let nodes = backend.all_nodes()?;
        let edges = backend.all_edges()?;
        let complexity = ComplexityAnalyzer::analyze(backend).ok();

        Ok(json!({
            "node_count": nodes.len(),
            "edge_count": edges.len(),
            "function_count": backend.find_nodes_by_type(NodeType::Function)?.len(),
            "class_count": backend.find_nodes_by_type(NodeType::Class)?.len(),
            "file_count": backend.find_nodes_by_type(NodeType::File)?.len(),
            "avg_complexity": complexity.as_ref().map(|c| c.avg_cyclomatic),
            "max_complexity": complexity.as_ref().map(|c| c.max_cyclomatic),
        }))
    })?;
    Ok(Json(stats))
}

async fn list_nodes(
    State(state): State<AppState>,
    Query(params): Query<NodeListParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    let page = params.page.unwrap_or(0);
    let limit = params.limit.unwrap_or(50).min(200);

    let result = state.with_graph(|graph| {
        let backend = graph.backend();
        let mut nodes = if let Some(ref q) = params.q {
            backend
                .all_nodes()?
                .into_iter()
                .filter(|n| {
                    n.name.to_lowercase().contains(&q.to_lowercase())
                        || n
                            .file_path
                            .as_ref()
                            .is_some_and(|f| f.to_lowercase().contains(&q.to_lowercase()))
                })
                .collect::<Vec<_>>()
        } else if let Some(ref label) = params.label {
            backend.find_nodes_by_label(label)?
        } else if let Some(ref nt) = params.node_type {
            let node_type = parse_node_type(nt)?;
            backend.find_nodes_by_type(node_type)?
        } else {
            backend.all_nodes()?
        };

        nodes.sort_by(|a, b| a.name.cmp(&b.name));
        let total = nodes.len();
        let page_nodes: Vec<Value> = nodes
            .iter()
            .skip(page * limit)
            .take(limit)
            .map(node_summary)
            .collect();

        Ok(json!({
            "total": total,
            "page": page,
            "limit": limit,
            "nodes": page_nodes,
        }))
    })?;

    Ok(Json(result))
}

async fn list_edges(
    State(state): State<AppState>,
    Query(params): Query<NodeListParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(100).min(500);
    let page = params.page.unwrap_or(0);

    let result = state.with_graph(|graph| {
        let edges = graph.backend().all_edges()?;
        let total = edges.len();
        let page_edges: Vec<Value> = edges
            .into_iter()
            .skip(page * limit)
            .take(limit)
            .map(edge_summary)
            .collect();

        Ok(json!({
            "total": total,
            "page": page,
            "limit": limit,
            "edges": page_edges,
        }))
    })?;

    Ok(Json(result))
}

async fn search_nodes(
    State(state): State<AppState>,
    Query(params): Query<NodeListParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    list_nodes(State(state), Query(params)).await
}

async fn nlp_query(
    State(state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> std::result::Result<Json<Value>, ApiError> {
    let result = state.with_graph(|graph| {
        let backend = graph.backend();

        if let Some(ref dsl) = req.query {
            let nodes = query::execute(backend, dsl)?;
            return Ok(json!({
                "query": dsl,
                "count": nodes.len(),
                "results": nodes.iter().take(50).map(node_summary).collect::<Vec<_>>(),
            }));
        }

        let question = req
            .question
            .as_deref()
            .ok_or_else(|| Error::InvalidQuery("Missing question or query".into()))?;

        let matcher = PatternMatcher::from_graph(backend)?;
        let translated = matcher.translate(question)?;
        let query_result = matcher.execute(&translated, backend)?;

        Ok(json!({
            "question": question,
            "internal_query": translated.internal_query,
            "confidence": translated.confidence,
            "answer": format_query_result(&query_result),
            "intent": format!("{:?}", translated.intent),
        }))
    })?;

    Ok(Json(result))
}

async fn list_communities(State(state): State<AppState>) -> std::result::Result<Json<Value>, ApiError> {
    let result = state.with_graph(|graph| {
        let detection = CommunityDetector::new().detect(graph.backend())?;
        let communities: Vec<Value> = detection
            .communities
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "member_count": c.members.len(),
                })
            })
            .collect();

        Ok(json!({
            "modularity": detection.modularity,
            "communities": communities,
        }))
    })?;

    Ok(Json(result))
}

fn node_summary(node: &Node) -> Value {
    json!({
        "id": node.id.to_string(),
        "name": node.name,
        "type": format!("{:?}", node.node_type),
        "file": node.file_path,
        "line": node.start_line,
        "labels": node.labels,
        "complexity": node.get_property("cyclomatic"),
    })
}

fn edge_summary(edge: Edge) -> Value {
    json!({
        "from": edge.from.to_string(),
        "to": edge.to.to_string(),
        "type": format!("{:?}", edge.edge_type),
    })
}

fn parse_node_type(value: &str) -> std::result::Result<NodeType, Error> {
    match value.to_ascii_lowercase().as_str() {
        "function" => Ok(NodeType::Function),
        "class" => Ok(NodeType::Class),
        "struct" => Ok(NodeType::Struct),
        "file" => Ok(NodeType::File),
        "module" => Ok(NodeType::Module),
        "configkey" | "config" => Ok(NodeType::ConfigKey),
        other => Err(Error::InvalidQuery(format!("Unknown node type: {other}"))),
    }
}

fn format_query_result(result: &crate::nlp::QueryResult) -> String {
    use crate::nlp::QueryResult;
    match result {
        QueryResult::Count(n) => format!("Found {n} result(s)"),
        QueryResult::Nodes(nodes) => format!("Found {} result(s)", nodes.len()),
        QueryResult::Text(lines) => lines.join("\n"),
    }
}

/// API error wrapper for axum.
pub struct ApiError(Error);

impl std::fmt::Debug for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ApiError({})", self.0)
    }
}

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::Node;
    use crate::graph::CodeGraph;
    use crate::api::state::AppState;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_api_graph_stats() {
        let temp = TempDir::new().unwrap();
        let mut graph = CodeGraph::new();
        graph
            .backend_mut()
            .insert_node(Node::new(NodeType::Function, "main".into()))
            .unwrap();
        graph.save_to_repo(temp.path()).unwrap();
        let state = AppState::from_repo(temp.path()).unwrap();

        let stats = graph_stats(axum::extract::State(state))
            .await
            .expect("stats request failed")
            .0;
        assert_eq!(stats["node_count"], 1);
    }
}
