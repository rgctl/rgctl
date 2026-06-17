//! MCP server implementation with stdio and HTTP transports

use crate::api::state::AppState;
use crate::error::{Error, Result};
use crate::mcp::protocol::McpHandler;
use crate::mcp::tools::ToolExecutor;
use std::io::{self, BufRead, Write};
use std::path::Path;

/// MCP server for AI agent integration.
pub struct McpServer {
    handler: McpHandler,
}

impl McpServer {
    /// Create a new MCP server for a repository.
    pub fn new(repo_root: impl AsRef<Path>) -> Result<Self> {
        let state = AppState::from_repo(repo_root)?;
        Ok(Self {
            handler: McpHandler::new(state),
        })
    }

    /// Create from an existing app state.
    pub fn from_state(state: AppState) -> Self {
        Self {
            handler: McpHandler::new(state),
        }
    }

    /// Handle a simplified tool request (for direct API use).
    pub fn handle_tool_request(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.handler.execute_tool(name, params)
    }

    /// Run the MCP server over stdio (newline-delimited JSON-RPC).
    pub fn run_stdio(&mut self, watch: bool) -> Result<()> {
        use crate::config::project::RbuilderConfig;
        use crate::mcp::protocol::graph_updated_notification;
        use std::sync::mpsc::{self, RecvTimeoutError};
        use std::thread;
        use std::time::Duration;

        let notify_rx = if watch {
            let state = self.handler.state().clone_handle();
            let repo_root = state.repo_root();
            let debounce = RbuilderConfig::load(&repo_root)
                .map(|c| c.watch.debounce_ms)
                .unwrap_or(500);
            let (tx, rx) = mpsc::channel();
            crate::watch::spawn_watch_with_state(state, debounce, tx)?;
            Some(rx)
        } else {
            None
        };

        let (stdin_tx, stdin_rx) = mpsc::channel();
        thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) => {
                        if stdin_tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut stdout = io::stdout();
        loop {
            if let Some(rx) = &notify_rx {
                if let Ok(notification) = rx.try_recv() {
                    let msg = graph_updated_notification(&notification)?;
                    writeln!(stdout, "{msg}")
                        .map_err(|e| Error::Other(format!("stdout write error: {e}")))?;
                    stdout.flush()
                        .map_err(|e| Error::Other(format!("stdout flush error: {e}")))?;
                }
            }

            match stdin_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    if let Some(response) = self.handler.handle_message(&line)? {
                        writeln!(stdout, "{response}")
                            .map_err(|e| Error::Other(format!("stdout write error: {e}")))?;
                        stdout.flush()
                            .map_err(|e| Error::Other(format!("stdout flush error: {e}")))?;
                    }
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(())
    }

    /// List available tools.
    pub fn list_tools() -> Vec<crate::mcp::tools::ToolDefinition> {
        ToolExecutor::list_tools()
    }
}

/// Run MCP HTTP transport on the given port.
#[cfg(feature = "mcp-server")]
pub async fn run_http(state: AppState, port: u16, verbose: bool) -> Result<()> {
    use axum::{
        extract::{Path as AxumPath, State},
        routing::{get, post},
        Json, Router,
    };
    use serde_json::{json, Value};
    use std::net::SocketAddr;
    use tower_http::cors::CorsLayer;

    #[derive(Clone)]
    struct HttpState {
        handler: std::sync::Arc<std::sync::Mutex<McpHandler>>,
    }

    let handler = McpHandler::new(state);
    let http_state = HttpState {
        handler: std::sync::Arc::new(std::sync::Mutex::new(handler)),
    };

    async fn health() -> Json<Value> {
        Json(json!({ "status": "ok", "server": "rbuilder-mcp" }))
    }

    async fn list_tools() -> Json<Value> {
        Json(json!({ "tools": ToolExecutor::list_tools() }))
    }

    async fn call_tool(
        State(state): State<HttpState>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let name = body
            .get("name")
            .or_else(|| body.get("tool"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let args = body
            .get("arguments")
            .or_else(|| body.get("params"))
            .cloned()
            .unwrap_or(json!({}));

        match state.handler.lock().unwrap().execute_tool(name, args) {
            Ok(value) => Json(json!({ "result": value })),
            Err(e) => Json(json!({ "error": e.to_string() })),
        }
    }

    async fn call_tool_by_name(
        State(state): State<HttpState>,
        AxumPath(name): AxumPath<String>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        match state.handler.lock().unwrap().execute_tool(&name, body) {
            Ok(value) => Json(json!({ "result": value })),
            Err(e) => Json(json!({ "error": e.to_string() })),
        }
    }

    async fn mcp_jsonrpc(
        State(state): State<HttpState>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let raw = body.to_string();
        match state.handler.lock().unwrap().handle_message(&raw) {
            Ok(Some(response)) => Json(
                serde_json::from_str(&response).unwrap_or(json!({ "error": "invalid response" })),
            ),
            Ok(None) => Json(json!({ "result": null })),
            Err(e) => Json(json!({ "error": e.to_string() })),
        }
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/tools", get(list_tools))
        .route("/tools/call", post(call_tool))
        .route("/tools/:name", post(call_tool_by_name))
        .route("/mcp", post(mcp_jsonrpc))
        .layer(CorsLayer::permissive())
        .with_state(http_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    if verbose {
        tracing::info!("MCP HTTP server listening on http://{addr}");
    } else {
        eprintln!("MCP HTTP server listening on http://{addr}");
    }

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| Error::Other(format!("Failed to bind port {port}: {e}")))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| Error::Other(format!("HTTP server error: {e}")))?;

    Ok(())
}
