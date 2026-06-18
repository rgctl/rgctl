//! CLI command implementations

pub mod ansible;
pub mod chat;
pub mod chef;
pub mod diagram;
pub mod puppet;
pub mod update;
pub mod workspace;

#[cfg(feature = "mcp-server")]
pub mod mcp;

#[cfg(feature = "mcp-server")]
pub mod serve;
