//! Configuration format plugins

pub mod yaml;
pub mod json;
pub mod toml_plugin;

pub use json::JsonPlugin;
pub use toml_plugin::TomlPlugin;
pub use yaml::YamlPlugin;

// Placeholder - additional config parsers
// Task 1.3.4: Properties plugin
// Task 1.3.5: Markdown plugin
