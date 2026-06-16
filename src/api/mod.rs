//! REST API for web UI

#[cfg(feature = "mcp-server")]
pub mod state;

#[cfg(feature = "mcp-server")]
pub mod server;

#[cfg(feature = "mcp-server")]
pub use state::AppState;
