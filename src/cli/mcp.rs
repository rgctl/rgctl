//! MCP server CLI command

use crate::api::state::AppState;
use crate::error::Result;
use crate::mcp::server::{run_http, McpServer};
use std::path::Path;

/// Run `rbuilder mcp serve`.
pub fn run_mcp_serve(repo_root: &Path, transport: &str, port: u16, verbose: bool) -> Result<()> {
    match transport.to_ascii_lowercase().as_str() {
        "stdio" => {
            let mut server = McpServer::new(repo_root)?;
            if verbose {
                eprintln!("MCP stdio server started (JSON-RPC over stdin/stdout)");
            }
            server.run_stdio()
        }
        "http" => {
            let state = AppState::from_repo(repo_root)?;
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| crate::error::Error::Other(format!("Failed to start runtime: {e}")))?;
            rt.block_on(run_http(state, port, verbose))
        }
        other => Err(crate::error::Error::InvalidQuery(format!(
            "Unknown transport '{other}'. Use 'stdio' or 'http'."
        ))),
    }
}
