//! MCP JSON-RPC protocol types and message handling

use crate::api::state::AppState;
use crate::error::{Error, Result};
use crate::mcp::resources::ResourceProvider;
use crate::mcp::tools::ToolExecutor;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// MCP protocol version supported by this server.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC request from an MCP client.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (must be "2.0")
    pub jsonrpc: String,
    /// Request ID (null for notifications)
    pub id: Option<Value>,
    /// Method name
    pub method: String,
    /// Method parameters
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC response to an MCP client.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Matching request ID
    pub id: Value,
    /// Success result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error object
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error object.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// MCP server handler for JSON-RPC requests.
pub struct McpHandler {
    state: AppState,
    executor: ToolExecutor,
    initialized: bool,
}

impl McpHandler {
    /// Create a new MCP handler for a repository.
    pub fn new(state: AppState) -> Self {
        let repo_root = state.repo_root();
        Self {
            state,
            executor: ToolExecutor::new(repo_root),
            initialized: false,
        }
    }

    /// Handle a raw JSON string (one JSON-RPC message).
    pub fn handle_message(&mut self, raw: &str) -> Result<Option<String>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let request: JsonRpcRequest = serde_json::from_str(trimmed)
            .map_err(|e| Error::SerdeError(format!("Invalid JSON-RPC: {e}")))?;

        if request.jsonrpc != "2.0" {
            return Ok(Some(serialize_response(error_response(
                request.id.unwrap_or(Value::Null),
                -32600,
                "Invalid Request: jsonrpc must be '2.0'",
            ))?));
        }

        // Notifications have no id
        if request.id.is_none() {
            if request.method == "notifications/initialized" {
                self.initialized = true;
            }
            return Ok(None);
        }

        let id = request.id.clone().unwrap_or(Value::Null);
        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(&request.params),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": ToolExecutor::list_tools() })),
            "tools/call" => self.handle_tools_call(&request.params),
            "resources/list" => Ok(json!({ "resources": ResourceProvider::list_resources() })),
            "resources/read" => self.handle_resources_read(&request.params),
            _ => Err(Error::InvalidQuery(format!("Unknown method: {}", request.method))),
        };

        match response {
            Ok(result) => Ok(Some(serialize_response(JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(result),
                error: None,
            })?)),
            Err(e) => Ok(Some(serialize_response(error_response(
                id,
                -32000,
                &e.to_string(),
            ))?)),
        }
    }

    /// Simplified tool call for testing and HTTP transport.
    pub fn execute_tool(&self, name: &str, args: Value) -> Result<Value> {
        self.state.with_graph(|graph| self.executor.execute(graph, name, args))
    }

    /// Shared application state (for watch mode integration).
    pub fn state(&self) -> &AppState {
        &self.state
    }

    fn handle_initialize(&mut self, _params: &Value) -> Result<Value> {
        Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {},
                "resources": {},
                "notifications": {
                    "graph_updated": {}
                }
            },
            "serverInfo": {
                "name": "rbuilder",
                "version": crate::VERSION
            }
        }))
    }

    fn handle_tools_call(&self, params: &Value) -> Result<Value> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidQuery("Missing tool name".into()))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(json!({}));

        let result = self.state.with_graph(|graph| self.executor.execute(graph, name, arguments))?;
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| Error::SerdeError(e.to_string()))?;

        Ok(json!({
            "content": [{ "type": "text", "text": text }],
            "isError": false
        }))
    }

    fn handle_resources_read(&self, params: &Value) -> Result<Value> {
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidQuery("Missing resource uri".into()))?;

        let content = self.state.with_graph(|graph| {
            ResourceProvider::read(graph.backend(), uri)
        })?;
        let text = serde_json::to_string_pretty(&content)
            .map_err(|e| Error::SerdeError(e.to_string()))?;

        Ok(json!({
            "contents": [{
                "uri": uri,
                "mimeType": "application/json",
                "text": text
            }]
        }))
    }
}

/// Serialize an MCP graph-updated notification.
pub fn graph_updated_notification(notification: &crate::watch::GraphUpdateNotification) -> Result<String> {
    let value = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/graph_updated",
        "params": notification,
    });
    serde_json::to_string(&value).map_err(|e| Error::SerdeError(e.to_string()))
}

fn serialize_response(response: JsonRpcResponse) -> Result<String> {
    serde_json::to_string(&response).map_err(|e| Error::SerdeError(e.to_string()))
}

fn error_response(id: Value, code: i32, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
            data: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::Node;
    use crate::graph::CodeGraph;
    use tempfile::TempDir;

    fn setup_test_handler() -> (McpHandler, TempDir) {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let mut graph = CodeGraph::new();
        graph
            .backend_mut()
            .insert_node(Node::new(
                crate::graph::schema::NodeType::Function,
                "main".into(),
            ))
            .unwrap();
        graph.save_to_repo(root).unwrap();
        let state = AppState::from_repo(root).unwrap();
        (McpHandler::new(state), temp)
    }

    #[test]
    fn test_mcp_server_stdio() {
        let (mut handler, _temp) = setup_test_handler();
        let request = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"query_codebase","arguments":{"question":"how many functions?"}}}"#;
        let response = handler.handle_message(request).unwrap().unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("answer"));
    }

    #[test]
    fn test_tools_list() {
        let (mut handler, _temp) = setup_test_handler();
        let request = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let response = handler.handle_message(request).unwrap().unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert!(parsed["result"]["tools"].as_array().unwrap().len() >= 7);
    }

    #[test]
    fn test_initialize() {
        let (mut handler, _temp) = setup_test_handler();
        let request = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
        let response = handler.handle_message(request).unwrap().unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["result"]["serverInfo"]["name"], "rbuilder");
    }
}
