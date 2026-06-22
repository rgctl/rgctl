//! MCP (Model Context Protocol) server

#[cfg(feature = "mcp-server")]
pub mod protocol;

#[cfg(feature = "mcp-server")]
pub mod server;

#[cfg(feature = "mcp-server")]
pub mod tools;

#[cfg(feature = "mcp-server")]
pub mod resources;

#[cfg(feature = "mcp-server")]
pub use protocol::McpHandler;
#[cfg(feature = "mcp-server")]
pub use server::McpServer;
#[cfg(feature = "mcp-server")]
pub use tools::{ToolDefinition, ToolExecutor};
