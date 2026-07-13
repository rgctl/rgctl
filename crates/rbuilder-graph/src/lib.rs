//! Graph storage and query layer for rBuilder.
#![warn(missing_docs)]

/// Graph storage backends and the [`backend::GraphBackend`] trait.
pub mod backend;
/// High-level [`code_graph::CodeGraph`] API.
pub mod code_graph;
/// Code location hashing and lookup helpers.
pub mod code_index;
/// Columnar mmap snapshot format (v2).
pub mod columnar_snapshot;
/// JSON import/export for graph snapshots.
pub mod export;
/// String interning for index keys.
pub mod intern;
/// Snapshot version migration helpers.
pub mod migration;
/// Mini query language over [`backend::MemoryBackend`].
pub mod query;
/// Node, edge, and graph schema types.
pub mod schema;
/// Prepared and memory-mapped snapshot I/O.
pub mod snapshot;

pub use code_graph::CodeGraph;
pub use code_index::{hash_code, CodeIndex, CodeLocation};
pub use columnar_snapshot::{ColumnarGraphMmap, COLUMNAR_SNAPSHOT_VERSION};
pub use export::{export_json, import_json, GraphSnapshot};
pub use migration::{migrate_snapshot, migrate_v1_to_v2};
pub use schema::{AccessType, CallType, GraphParameter, GRAPH_SCHEMA_VERSION};
pub use snapshot::{
    MmappedGraphSnapshot, PreparedGraphSnapshot, PreparedIndexes, SnapshotNodeStore, SNAPSHOT_FILE,
};

/// Normalize path separators for consistent comparison.
pub fn normalize_path_str(path: &str) -> String {
    path.replace('\\', "/")
}
