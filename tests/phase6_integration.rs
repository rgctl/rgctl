//! Phase 6 integration tests: MCP, chat, API, and formatting

#![cfg(feature = "mcp-server")]

use rbuilder::api::state::AppState;
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Node, NodeType};
use rbuilder::graph::CodeGraph;
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::mcp::protocol::McpHandler;
use rbuilder::mcp::tools::ToolExecutor;
use rbuilder::nlp::conversation::ConversationContext;
use rbuilder::output::formatter::{format_impact_report, Severity};
use rbuilder::pipeline::{PipelineConfig, ProcessingPipeline};
use serde_json::json;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

fn write(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn setup_repo() -> (TempDir, CodeGraph) {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(
        &root.join("src/auth.rs"),
        r#"
fn authenticate_user(token: &str) -> bool {
    verify_token(token)
}

fn verify_token(token: &str) -> bool {
    token.len() > 0
}
"#,
    );

    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        },
    );
    let (graph, _) = pipeline.process_repository(root).unwrap();
    graph.save_to_repo(root).unwrap();
    (temp, graph)
}

#[test]
fn test_mcp_tool_query_codebase() {
    let (_temp, graph) = setup_repo();
    let executor = ToolExecutor::new(".");
    let result = executor
        .execute(
            &graph,
            "query_codebase",
            json!({ "question": "how many functions?" }),
        )
        .unwrap();
    let answer = result["answer"].as_str().unwrap();
    assert!(answer.contains('2') || answer.contains('3'));
}

#[test]
fn test_mcp_tool_impact_analysis() {
    let (_temp, graph) = setup_repo();
    let executor = ToolExecutor::new(".");
    let result = executor
        .execute(
            &graph,
            "impact_analysis",
            json!({ "symbol": "verify_token", "depth": 3 }),
        )
        .unwrap();
    assert!(result["direct_dependencies"].is_array());
    assert!(result["indirect_dependencies"].is_array());
}

#[test]
fn test_mcp_tool_symbol_info() {
    let (_temp, graph) = setup_repo();
    let executor = ToolExecutor::new(".");
    let result = executor
        .execute(
            &graph,
            "symbol_info",
            json!({ "symbol_name": "verify_token" }),
        )
        .unwrap();
    let json_str = serde_json::to_string(&result).unwrap();
    assert!(json_str.len() < 1024);
    assert_eq!(result["name"], "verify_token");
}

#[test]
fn test_mcp_stdio_protocol() {
    let (temp, _graph) = setup_repo();
    let state = AppState::from_repo(temp.path()).unwrap();
    let mut handler = McpHandler::new(state);

    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
    let response = handler.handle_message(init).unwrap().unwrap();
    assert!(response.contains("rbuilder"));

    let tools = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
    let response = handler.handle_message(tools).unwrap().unwrap();
    assert!(response.contains("query_codebase"));
}

#[test]
fn test_conversation_context_pronouns() {
    let mut ctx = ConversationContext::new();
    ctx.add_query("How many services?");
    ctx.add_focused_node("AuthenticationService");
    let resolved = ctx.resolve_references("What's its complexity?");
    assert!(resolved.contains("AuthenticationService"));
}

#[test]
fn test_context_efficient_response() {
    let mut graph = CodeGraph::new();
    let node = Node::new(NodeType::Function, "verify_token".into())
        .with_file_path("src/auth/jwt.rs".into())
        .with_location(89, 120)
        .with_property("cyclomatic".into(), "12".into());
    graph.backend_mut().insert_node(node).unwrap();

    let executor = ToolExecutor::new(".");
    let result = executor
        .execute(
            &graph,
            "symbol_info",
            json!({ "symbol_name": "verify_token" }),
        )
        .unwrap();
    let json_str = serde_json::to_string(&result).unwrap();
    assert!(
        json_str.len() < 1024,
        "Response too verbose: {} bytes",
        json_str.len()
    );
}

#[test]
fn test_formatted_impact_output() {
    let report = format_impact_report(
        "verify_token",
        &["authenticate_user".into()],
        &["login".into()],
        Severity::Warning,
    );
    assert!(report.contains("verify_token"));
    assert!(report.contains("RECOMMENDATION"));
}

#[test]
fn test_find_by_complexity_tool() {
    let mut graph = CodeGraph::new();
    let node = Node::new(NodeType::Function, "complex_fn".into())
        .with_property("cyclomatic".into(), "25".into())
        .with_label("security:critical".into());
    graph.backend_mut().insert_node(node).unwrap();

    let executor = ToolExecutor::new(".");
    let result = executor
        .execute(
            &graph,
            "find_by_complexity",
            json!({ "min_complexity": 20 }),
        )
        .unwrap();
    assert_eq!(result["count"], 1);
}

#[test]
fn test_config_analysis_tool() {
    let (_temp, graph) = setup_repo();
    let executor = ToolExecutor::new(".");
    let result = executor
        .execute(
            &graph,
            "config_analysis",
            json!({ "analysis_type": "unused_keys" }),
        )
        .unwrap();
    assert!(result["count"].is_number());
}

#[test]
fn test_get_community_info_tool() {
    let (_temp, graph) = setup_repo();
    let executor = ToolExecutor::new(".");
    let result = executor
        .execute(&graph, "get_community_info", json!({}))
        .unwrap();
    assert!(result["community_count"].is_number());
}

#[test]
fn test_mcp_resources() {
    let (_temp, graph) = setup_repo();
    let stats =
        rbuilder::mcp::resources::ResourceProvider::read(graph.backend(), "rbuilder://graph/stats")
            .unwrap();
    assert!(stats["node_count"].as_u64().unwrap() > 0);
}

#[test]
fn test_api_graph_stats_endpoint() {
    let (temp, graph) = setup_repo();
    graph.save_to_repo(temp.path()).unwrap();
    let state = AppState::from_repo(temp.path()).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let stats = rt.block_on(async {
        use axum::extract::State;
        rbuilder::api::server::graph_stats(State(state))
            .await
            .unwrap()
            .0
    });
    assert!(stats["node_count"].as_u64().unwrap() > 0);
}
