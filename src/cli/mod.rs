//! CLI command implementations

pub mod ansible;
pub mod chef;
pub mod chat;
pub mod diagram;
pub mod update;
pub mod workspace;

#[cfg(feature = "mcp-server")]
pub mod mcp;

#[cfg(feature = "mcp-server")]
pub mod serve;
