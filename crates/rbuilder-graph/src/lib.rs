//! Graph storage and query layer

pub mod backend;
pub mod code_graph;
pub mod code_index;
pub mod export;
pub mod intern;
pub mod migration;
pub mod query;
pub mod schema;

pub use code_graph::CodeGraph;
pub use code_index::{hash_code, CodeIndex, CodeLocation};
pub use export::{export_json, import_json, GraphSnapshot};
pub use migration::{migrate_snapshot, migrate_v1_to_v2};
pub use schema::{AccessType, CallType, GraphParameter, GRAPH_SCHEMA_VERSION};

/// Normalize path separators for consistent comparison.
pub fn normalize_path_str(path: &str) -> String {
    path.replace('\\', "/")
}
