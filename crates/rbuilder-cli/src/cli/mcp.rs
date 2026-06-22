//! MCP server CLI command

use rbuilder_error::Result;
use rbuilder_mcp::api::state::AppState;
use rbuilder_mcp::mcp::server::{run_http, McpServer};
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
            let notification_store = if watch {
                Some(rbuilder_mcp::watch::new_notification_store())
            } else {
                None
            };
            if watch {
                let (tx, rx) = std::sync::mpsc::channel();
                let debounce = rbuilder_project_config::project::RbuilderConfig::load(repo_root)
                    .map(|c| c.watch.debounce_ms)
                    .unwrap_or(500);
                let _handle = rbuilder_mcp::watch::spawn_watch_with_state(
                    state.clone_handle(),
                    debounce,
                    tx,
                )?;
                let store = notification_store
                    .as_ref()
                    .expect("notification store")
                    .clone();
                std::thread::spawn(move || {
                    while let Ok(notification) = rx.recv() {
                        rbuilder_mcp::watch::record_notification(&store, notification);
                    }
                });
                if verbose {
                    eprintln!(
                        "Watch mode enabled — poll GET /notifications/latest for graph updates"
                    );
                }
            }
            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                rbuilder_error::Error::Other(format!("Failed to start runtime: {e}"))
            })?;
            rt.block_on(run_http(state, port, verbose, notification_store))
        }
        other => Err(rbuilder_error::Error::InvalidQuery(format!(
            "Unknown transport '{other}'. Use 'stdio' or 'http'."
        ))),
    }
}
