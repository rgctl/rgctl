//! Language plugin system

pub mod builtin;
pub mod config;
pub mod plugin_trait;
pub mod registry;

#[cfg(feature = "plugin-system")]
pub mod plugin_abi;
