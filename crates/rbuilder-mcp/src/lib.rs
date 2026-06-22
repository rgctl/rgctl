//! MCP server, HTTP API, and file watching

pub mod api;
pub mod watch;

#[cfg(feature = "mcp-server")]
pub mod mcp;

#[cfg(feature = "mcp-server")]
pub use mcp::protocol;
#[cfg(feature = "mcp-server")]
pub use mcp::resources;
#[cfg(feature = "mcp-server")]
pub use mcp::server;
#[cfg(feature = "mcp-server")]
pub use mcp::tools;

#[cfg(feature = "mcp-server")]
pub use protocol::McpHandler;
#[cfg(feature = "mcp-server")]
pub use server::McpServer;
#[cfg(feature = "mcp-server")]
pub use tools::{ToolDefinition, ToolExecutor};

pub use watch::{debounce_ready, GraphUpdateNotification, WatchService};

#[cfg(feature = "mcp-server")]
pub use watch::{
    latest_notification, new_notification_store, record_notification, NotificationStore,
};
