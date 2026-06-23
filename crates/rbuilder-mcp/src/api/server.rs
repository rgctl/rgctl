//! REST API server for web-based graph browser

use crate::api::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rbuilder_analysis::blast_radius::BlastRadiusAnalyzer;
use rbuilder_analysis::centrality::{degree_centrality, identify_hotspots, CentralityAnalyzer};
use rbuilder_analysis::community::{detect_communities, CommunityDetector};
use rbuilder_analysis::complexity::ComplexityAnalyzer;
use rbuilder_analysis::{
    build_cfg_for_function, BackwardSlicer, ProgramDependenceGraph, SliceCriterion, TaintAnalyzer,
    TypeInferenceEngine,
};
use rbuilder_error::Error;
use rbuilder_export::select_subgraph;
use rbuilder_graph::backend::GraphBackend;
use rbuilder_graph::query;
use rbuilder_graph::schema::{Edge, Node, NodeType};
use rbuilder_nlp::pattern_matcher::PatternMatcher;
use rbuilder_security::SecurityAnalyzer;
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

/// Graph query parameters for `/api/graph`.
#[derive(Debug, Deserialize)]
pub struct GraphQueryParams {
    /// DSL query (default: all nodes)
    pub query: Option<String>,
    /// Neighborhood expansion depth
    pub depth: Option<usize>,
    /// Max nodes returned
    pub limit: Option<usize>,
}

/// Taint analysis parameters.
#[derive(Debug, Deserialize)]
pub struct TaintParams {
    /// Source file path (relative to repo root)
    pub file: String,
    /// Function name to analyze
    pub function: String,
    /// Language override (rust, python, javascript, typescript)
    pub language: Option<String>,
    /// Include verbose output
    pub verbose: Option<bool>,
}

/// Security scan parameters.
#[derive(Debug, Deserialize)]
pub struct SecurityScanParams {
    /// Source file path (relative to repo root)
    pub file: String,
    /// Function name to analyze
    pub function: String,
    /// Language override
    pub language: Option<String>,
    /// Include verbose output
    pub verbose: Option<bool>,
}

/// Backward slice parameters.
#[derive(Debug, Deserialize)]
pub struct SliceParams {
    /// Source file path (relative to repo root)
    pub file: String,
    /// Line number (1-based)
    pub line: usize,
    /// Variable of interest
    pub variable: String,
    /// Function name (optional)
    pub function: Option<String>,
    /// Language override
    pub language: Option<String>,
    /// Interprocedural analysis
    pub interprocedural: Option<bool>,
}

/// Blast radius parameters.
#[derive(Debug, Deserialize)]
pub struct BlastRadiusParams {
    /// Symbol name
    pub symbol: String,
    /// Max depth
    pub depth: Option<usize>,
}

/// Start the web API and static file server.
pub async fn run_server(
    state: AppState,
    port: u16,
    web_dir: Option<std::path::PathBuf>,
) -> rbuilder_error::Result<()> {
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
    let mut router = Router::new()
        .route("/api/graph/stats", get(graph_stats))
        .route("/api/stats", get(graph_stats))
        .route("/api/graph", get(graph_by_query))
        .route("/api/graph/nodes", get(list_nodes))
        .route("/api/graph/edges", get(list_edges))
        .route("/api/graph/search", get(search_nodes))
        .route("/api/node/{id}", get(get_node))
        .route("/api/node/{id}/neighbors", get(get_node_neighbors))
        .route("/api/dashboard", get(dashboard_metrics))
        .route("/api/dashboard/advanced", get(dashboard_advanced))
        .route("/api/query", post(nlp_query))
        .route("/api/communities", get(list_communities))
        // Security and analysis endpoints
        .route("/api/taint", get(taint_analysis))
        .route("/api/security-scan", get(security_scan))
        .route("/api/slice", get(backward_slice))
        .route("/api/blast-radius", get(blast_radius))
        .route("/api/symbol/:name", get(symbol_info))
        .with_state(state)
        .layer(CorsLayer::permissive());

    // Serve static files as fallback (after API routes)
    if let Some(dir) = web_dir {
        if dir.exists() {
            router = router.fallback_service(ServeDir::new(dir));
        }
    }

    router
}

/// Get graph statistics including node/edge counts and complexity metrics.
pub async fn graph_stats(
    State(state): State<AppState>,
) -> std::result::Result<Json<Value>, ApiError> {
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

/// Return nodes and edges for a DSL query (Phase 14 web explorer).
pub async fn graph_by_query(
    State(state): State<AppState>,
    Query(params): Query<GraphQueryParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    let query = params.query.unwrap_or_else(|| "all".into());
    let limit = params.limit.unwrap_or(200).min(1000);

    let result = state.with_graph(|graph| {
        let backend = graph.backend();
        let subgraph = select_subgraph(backend, &query, params.depth)?;
        let nodes: Vec<Value> = subgraph
            .nodes
            .iter()
            .take(limit)
            .map(node_summary)
            .collect();
        let node_ids: std::collections::HashSet<_> =
            subgraph.nodes.iter().take(limit).map(|n| n.id).collect();
        let edges: Vec<Value> = subgraph
            .edges
            .into_iter()
            .filter(|e| node_ids.contains(&e.from) && node_ids.contains(&e.to))
            .map(edge_summary)
            .collect();

        Ok(json!({
            "query": query,
            "nodes": nodes,
            "edges": edges,
        }))
    })?;

    Ok(Json(result))
}

/// Return details for a single node.
pub async fn get_node(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<Value>, ApiError> {
    let node_id = uuid::Uuid::parse_str(&id)
        .map_err(|e| Error::InvalidQuery(format!("Invalid node id: {e}")))?;

    let result = state.with_graph(|graph| {
        let backend = graph.backend();
        let node = backend
            .get_node(node_id)?
            .ok_or_else(|| Error::InvalidQuery(format!("Node not found: {id}")))?;
        let mut detail = node_summary(&node);
        if let Some(obj) = detail.as_object_mut() {
            obj.insert("properties".into(), json!(node.properties));
            obj.insert("signature".into(), json!(node.signature));
            obj.insert("return_type".into(), json!(node.return_type));
        }
        Ok(detail)
    })?;

    Ok(Json(result))
}

/// Return adjacent nodes and connecting edges.
pub async fn get_node_neighbors(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<Value>, ApiError> {
    let node_id = uuid::Uuid::parse_str(&id)
        .map_err(|e| Error::InvalidQuery(format!("Invalid node id: {e}")))?;

    let result = state.with_graph(|graph| {
        let backend = graph.backend();
        backend
            .get_node(node_id)?
            .ok_or_else(|| Error::InvalidQuery(format!("Node not found: {id}")))?;

        let edges = backend.all_edges()?;
        let mut neighbor_ids = std::collections::HashSet::new();
        let mut connecting = Vec::new();
        for edge in edges {
            if edge.from == node_id {
                neighbor_ids.insert(edge.to);
                connecting.push(edge_summary(edge));
            } else if edge.to == node_id {
                neighbor_ids.insert(edge.from);
                connecting.push(edge_summary(edge));
            }
        }

        let neighbors: Vec<Value> = neighbor_ids
            .iter()
            .filter_map(|nid| backend.get_node(*nid).ok().flatten())
            .map(|n| node_summary(&n))
            .collect();

        Ok(json!({
            "id": id,
            "neighbors": neighbors,
            "edges": connecting,
        }))
    })?;

    Ok(Json(result))
}

/// Dashboard chart data (complexity, languages, node types).
pub async fn dashboard_metrics(
    State(state): State<AppState>,
) -> std::result::Result<Json<Value>, ApiError> {
    let result = state.with_graph(|graph| {
        let backend = graph.backend();
        let nodes = backend.all_nodes()?;
        let complexity = ComplexityAnalyzer::analyze(backend).ok();

        let mut type_counts = std::collections::HashMap::<String, usize>::new();
        let mut lang_counts = std::collections::HashMap::<String, usize>::new();
        for node in &nodes {
            let t = format!("{:?}", node.node_type);
            *type_counts.entry(t).or_default() += 1;
            if let Some(path) = &node.file_path {
                let ext = std::path::Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                *lang_counts.entry(ext).or_default() += 1;
            }
        }

        let mut top_complex: Vec<Value> = Vec::new();
        if let Some(ref report) = complexity {
            let mut ranked: Vec<_> = report.functions.iter().collect();
            ranked.sort_by_key(|b| std::cmp::Reverse(b.cyclomatic));
            for func in ranked.into_iter().take(10) {
                top_complex.push(json!({
                    "name": func.node.name,
                    "complexity": func.cyclomatic,
                    "file": func.node.file_path,
                }));
            }
        }

        let complexity_histogram = complexity
            .as_ref()
            .map(|c| {
                let mut buckets = [0usize; 6];
                for func in &c.functions {
                    let idx = match func.cyclomatic {
                        0..=1 => 0,
                        2..=5 => 1,
                        6..=10 => 2,
                        11..=20 => 3,
                        21..=50 => 4,
                        _ => 5,
                    };
                    buckets[idx] += 1;
                }
                buckets
            })
            .unwrap_or([0; 6]);

        let community_data = CommunityDetector::new().detect(backend).ok();
        let centrality_data = CentralityAnalyzer::new().analyze(backend).ok();

        let mut communities_summary: Vec<Value> = Vec::new();
        let mut community_sizes: Vec<usize> = Vec::new();
        let mut modularity = None;
        let mut community_count = 0usize;

        if let Some(ref detection) = community_data {
            modularity = Some(detection.modularity);
            community_count = detection.communities.len();
            let mut sorted = detection.communities.clone();
            sorted.sort_by_key(|b| std::cmp::Reverse(b.members.len()));
            for community in sorted.iter().take(12) {
                community_sizes.push(community.members.len());
                let top_members: Vec<String> = community
                    .members
                    .iter()
                    .filter_map(|id| backend.get_node(*id).ok().flatten().map(|n| n.name))
                    .take(5)
                    .collect();
                communities_summary.push(json!({
                    "id": community.id,
                    "member_count": community.members.len(),
                    "top_members": top_members,
                }));
            }
        }

        let mut top_connected: Vec<Value> = Vec::new();
        let mut hotspots: Vec<Value> = Vec::new();

        if let Some(ref centrality) = centrality_data {
            let mut ranked: Vec<_> = centrality.scores.iter().collect();
            ranked.sort_by(|a, b| {
                let da = a.1.in_degree + a.1.out_degree;
                let db = b.1.in_degree + b.1.out_degree;
                db.cmp(&da).then_with(|| {
                    b.1.pagerank
                        .partial_cmp(&a.1.pagerank)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });

            for (id, scores) in ranked.into_iter().take(10) {
                if let Ok(Some(node)) = backend.get_node(*id) {
                    top_connected.push(json!({
                        "name": node.name,
                        "type": format!("{:?}", node.node_type),
                        "in_degree": scores.in_degree,
                        "out_degree": scores.out_degree,
                        "total_degree": scores.in_degree + scores.out_degree,
                        "pagerank": scores.pagerank,
                        "file": node.file_path,
                    }));
                }
            }
        }

        if let (Some(complexity_report), Some(centrality)) =
            (complexity.as_ref(), centrality_data.as_ref())
        {
            let mut ranked_hotspots: Vec<(
                f64,
                &rbuilder_analysis::complexity::FunctionComplexity,
            )> = complexity_report
                .functions
                .iter()
                .map(|func| {
                    let degree = centrality
                        .scores
                        .get(&func.node.id)
                        .map(|s| s.in_degree + s.out_degree)
                        .unwrap_or(0);
                    let score = func.cyclomatic as f64 * (1.0 + degree as f64);
                    (score, func)
                })
                .collect();
            ranked_hotspots
                .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            for (score, func) in ranked_hotspots.into_iter().take(10) {
                let degree = centrality
                    .scores
                    .get(&func.node.id)
                    .map(|s| s.in_degree + s.out_degree)
                    .unwrap_or(0);
                hotspots.push(json!({
                    "name": func.node.name,
                    "complexity": func.cyclomatic,
                    "total_degree": degree,
                    "hotspot_score": score,
                    "file": func.node.file_path,
                }));
            }
        }

        Ok(json!({
            "node_count": nodes.len(),
            "function_count": backend.find_nodes_by_type(NodeType::Function)?.len(),
            "class_count": backend.find_nodes_by_type(NodeType::Class)?.len(),
            "file_count": backend.find_nodes_by_type(NodeType::File)?.len(),
            "avg_complexity": complexity.as_ref().map(|c| c.avg_cyclomatic),
            "modularity": modularity,
            "community_count": community_count,
            "node_types": type_counts,
            "languages": lang_counts,
            "top_complex_functions": top_complex,
            "complexity_histogram": complexity_histogram,
            "communities": communities_summary,
            "community_sizes": community_sizes,
            "top_connected_nodes": top_connected,
            "hotspots": hotspots,
        }))
    })?;

    Ok(Json(result))
}

/// Advanced dashboard analytics: communities, hotspots, centrality (Phase 14 A+).
pub async fn dashboard_advanced(
    State(state): State<AppState>,
) -> std::result::Result<Json<Value>, ApiError> {
    let result = state.with_graph(|graph| {
        let backend = graph.backend();
        let communities = detect_communities(backend)?;
        let hotspots: Vec<_> = identify_hotspots(backend)?.into_iter().take(10).collect();
        let centrality: Vec<_> = degree_centrality(backend)?.into_iter().take(20).collect();

        Ok(json!({
            "communities": communities,
            "hotspots": hotspots,
            "centrality": centrality,
        }))
    })?;

    Ok(Json(result))
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
                        || n.file_path
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
    let limit = params.limit.unwrap_or(1000).min(10000);
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

async fn list_communities(
    State(state): State<AppState>,
) -> std::result::Result<Json<Value>, ApiError> {
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
        "ansibleplaybook" | "playbook" => Ok(NodeType::AnsiblePlaybook),
        "ansibleplay" => Ok(NodeType::AnsiblePlay),
        "ansibletask" | "task" => Ok(NodeType::AnsibleTask),
        "ansiblerole" | "role" => Ok(NodeType::AnsibleRole),
        "ansiblehandler" | "handler" => Ok(NodeType::AnsibleHandler),
        "ansiblevariable" => Ok(NodeType::AnsibleVariable),
        "ansibletemplate" => Ok(NodeType::AnsibleTemplate),
        "chefcookbook" | "cookbook" => Ok(NodeType::ChefCookbook),
        "chefrecipe" | "recipe" => Ok(NodeType::ChefRecipe),
        "chefresource" | "resource" => Ok(NodeType::ChefResource),
        "puppetmodule" | "puppetmodules" => Ok(NodeType::PuppetModule),
        "puppetclass" | "puppetclasses" => Ok(NodeType::PuppetClass),
        "puppetresource" => Ok(NodeType::PuppetResource),
        other => Err(Error::InvalidQuery(format!("Unknown node type: {other}"))),
    }
}

fn format_query_result(result: &rbuilder_nlp::QueryResult) -> String {
    use rbuilder_nlp::QueryResult;
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

/// Taint analysis endpoint - track untrusted data from sources to sinks.
async fn taint_analysis(
    State(state): State<AppState>,
    Query(params): Query<TaintParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    let file_path = state.repo_root().join(&params.file);
    let source = std::fs::read_to_string(&file_path)
        .map_err(|e| Error::Other(format!("Failed to read {}: {}", params.file, e)))?;

    let lang = params.language.unwrap_or_else(|| {
        file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    let cfg = build_cfg_for_function(&lang, &source, &params.function)?;
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
    let mut type_engine = TypeInferenceEngine::new(&pdg, &cfg, &lang);
    type_engine.infer();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg).with_type_inference(type_engine);
    analyzer.detect_patterns(&lang);
    let flows = analyzer.analyze();
    let vulnerable = flows.iter().filter(|f| f.is_vulnerable()).count();

    let verbose = params.verbose.unwrap_or(false);
    let flow_json: Vec<Value> = flows
        .iter()
        .map(|f| {
            json!({
                "variable": f.variable,
                "severity": f.severity,
                "vulnerable": f.is_vulnerable(),
                "source_type": format!("{:?}", f.source_type),
                "sink_type": format!("{:?}", f.sink_type),
                "sanitizers": f.sanitizers.len(),
            })
        })
        .collect();

    let response = json!({
        "file": params.file,
        "function": params.function,
        "language": lang,
        "total_flows": flows.len(),
        "vulnerable_flows": vulnerable,
        "flows": if verbose { json!(flow_json) } else { json!(flow_json.iter().take(10).collect::<Vec<_>>()) },
    });
    Ok(Json(response))
}

/// Security scan endpoint - detect CWE/OWASP vulnerabilities.
async fn security_scan(
    State(state): State<AppState>,
    Query(params): Query<SecurityScanParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    let file_path = state.repo_root().join(&params.file);
    let source = std::fs::read_to_string(&file_path)
        .map_err(|e| Error::Other(format!("Failed to read {}: {}", params.file, e)))?;

    let lang = params.language.unwrap_or_else(|| {
        file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    let cfg = build_cfg_for_function(&lang, &source, &params.function)?;
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
    let mut type_engine = TypeInferenceEngine::new(&pdg, &cfg, &lang);
    type_engine.infer();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg).with_type_inference(type_engine);
    analyzer.detect_patterns(&lang);
    let flows = analyzer.vulnerable_flows();
    let vulns = SecurityAnalyzer::new().analyze(flows, &pdg, &source);

    let verbose = params.verbose.unwrap_or(false);
    let vuln_json: Vec<Value> = vulns
        .iter()
        .map(|v| {
            json!({
                "cwe_id": v.cwe_id,
                "cwe_name": v.cwe_name,
                "severity": v.severity,
                "variable": v.taint_flow.variable,
                "source_line": v.source_line,
                "sink_line": v.sink_line,
                "recommendation": v.recommendation,
            })
        })
        .collect();

    let response = json!({
        "file": params.file,
        "function": params.function,
        "total_vulnerabilities": vulns.len(),
        "critical": vulns.iter().filter(|v| v.severity >= 9).count(),
        "high": vulns.iter().filter(|v| v.severity >= 7 && v.severity < 9).count(),
        "vulnerabilities": if verbose { json!(vuln_json) } else { json!(vuln_json.iter().take(10).collect::<Vec<_>>()) },
    });
    Ok(Json(response))
}

/// Backward slice endpoint - track variable dependencies.
async fn backward_slice(
    State(state): State<AppState>,
    Query(params): Query<SliceParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    let file_path = state.repo_root().join(&params.file);
    let source = std::fs::read_to_string(&file_path)
        .map_err(|e| Error::Other(format!("Failed to read {}: {}", params.file, e)))?;

    let lang = params.language.unwrap_or_else(|| {
        file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    let fn_name = params.function.as_deref().unwrap_or("");
    let cfg = build_cfg_for_function(&lang, &source, fn_name)?;
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
    let slice = BackwardSlicer::new(&pdg, &cfg).slice(SliceCriterion {
        variable: params.variable.clone(),
        line: params.line,
    })?;

    let total_lines = source.lines().count();
    let reduction_percent = if total_lines > 0 {
        ((total_lines - slice.lines.len()) as f64 / total_lines as f64) * 100.0
    } else {
        0.0
    };

    let response = json!({
        "file": params.file,
        "line": params.line,
        "variable": params.variable,
        "function": fn_name,
        "language": lang,
        "reduction_percent": reduction_percent,
        "total_lines": total_lines,
        "slice_lines": slice.lines,
    });
    Ok(Json(response))
}

/// Blast radius endpoint - PDG-enhanced impact analysis.
async fn blast_radius(
    State(state): State<AppState>,
    Query(params): Query<BlastRadiusParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    let result = state.with_graph(|graph| {
        let backend = graph.backend();
        let analyzer = BlastRadiusAnalyzer::new(backend);
        let impact = analyzer.analyze(&params.symbol)?;

        Ok(json!({
            "symbol": params.symbol,
            "score": impact.score,
            "direct_callers": impact.direct_callers.len(),
            "impact_zone": impact.impact_zone.len(),
            "data_flow_depth": impact.data_flow_depth,
            "callers": impact.direct_callers.iter().take(20).collect::<Vec<_>>(),
        }))
    })?;

    Ok(Json(result))
}

/// Symbol info endpoint - detailed information about a symbol.
async fn symbol_info(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> std::result::Result<Json<Value>, ApiError> {
    let result = state.with_graph(|graph| {
        let backend = graph.backend();
        let nodes = backend.all_nodes()?;
        let symbol = nodes
            .iter()
            .find(|n| n.name == name || n.qualified_name.as_deref() == Some(&name))
            .ok_or_else(|| Error::InvalidQuery(format!("Symbol not found: {}", name)))?;

        let edges = backend.all_edges()?;
        let callers: Vec<String> = edges
            .iter()
            .filter(|e| e.to == symbol.id && format!("{:?}", e.edge_type).contains("Calls"))
            .filter_map(|e| {
                nodes
                    .iter()
                    .find(|n| n.id == e.from)
                    .map(|n| n.name.clone())
            })
            .collect();

        let callees: Vec<String> = edges
            .iter()
            .filter(|e| e.from == symbol.id && format!("{:?}", e.edge_type).contains("Calls"))
            .filter_map(|e| {
                nodes
                    .iter()
                    .find(|n| n.id == e.to)
                    .map(|n| n.name.clone())
            })
            .collect();

        Ok(json!({
            "name": symbol.name,
            "type": format!("{:?}", symbol.node_type),
            "qualified_name": symbol.qualified_name,
            "file": symbol.file_path,
            "start_line": symbol.start_line,
            "end_line": symbol.end_line,
            "signature": symbol.signature,
            "complexity": symbol.get_property("cyclomatic"),
            "callers": callers,
            "callees": callees,
        }))
    })?;

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::state::AppState;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::Node;
    use rbuilder_graph::CodeGraph;
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
