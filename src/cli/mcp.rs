//! MCP server CLI command

use crate::api::state::AppState;
use crate::error::Result;
use crate::mcp::server::{run_http, McpServer};
use std::path::Path;

/// Run `rbuilder mcp serve`.
pub fn run_mcp_serve(
    repo_root: &Path,
    transport: &str,
    port: u16,
    verbose: bool,
    watch: bool,
) -> Result<()> {
    match transport.to_ascii_lowercase().as_str() {
        "stdio" => {
            let mut server = McpServer::new(repo_root)?;
            if verbose {
                eprintln!("MCP stdio server started (JSON-RPC over stdin/stdout)");
                if watch {
                    eprintln!("Watch mode enabled — graph updates emit notifications");
                }
            }
            server.run_stdio(watch)
        }
        "http" => {
            let state = AppState::from_repo(repo_root)?;
            if watch {
                let (tx, rx) = std::sync::mpsc::channel();
                let debounce = crate::config::project::RbuilderConfig::load(repo_root)
                    .map(|c| c.watch.debounce_ms)
                    .unwrap_or(500);
                let _handle = crate::watch::spawn_watch_with_state(state.clone_handle(), debounce, tx)?;
                std::thread::spawn(move || {
                    while let Ok(notification) = rx.recv() {
                        eprintln!(
                            "graph_updated: {}",
                            serde_json::to_string(&notification).unwrap_or_default()
                        );
                    }
                });
                if verbose {
                    eprintln!("Watch mode enabled (notifications logged to stderr)");
                }
            }
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| crate::error::Error::Other(format!("Failed to start runtime: {e}")))?;
            rt.block_on(run_http(state, port, verbose))
        }
        other => Err(crate::error::Error::InvalidQuery(format!(
            "Unknown transport '{other}'. Use 'stdio' or 'http'."
        ))),
    }
}
