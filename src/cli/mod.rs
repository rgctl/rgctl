//! CLI command implementations

pub mod update;

#[cfg(feature = "mcp-server")]
pub mod chat;

#[cfg(feature = "mcp-server")]
pub mod mcp;

#[cfg(feature = "mcp-server")]
pub mod serve;
pub mod workspace;
