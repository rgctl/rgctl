//! Graph storage and query layer for rgctl.
#![warn(missing_docs)]

/// Graph storage backends and the [`backend::GraphBackend`] trait.
pub mod backend;
/// High-level [`code_graph::CodeGraph`] API.
pub mod code_graph;
/// Code location hashing and lookup helpers.
pub mod code_index;
/// Columnar mmap snapshot format (v2).
pub mod columnar_snapshot;
/// Hash-keyed blob vault for truncated document bodies.
pub mod content_store;
/// Typed bidirectional CSR topology.
pub mod csr;
/// JSON import/export for graph snapshots.
pub mod export;
/// Streaming compaction of columnar snapshots with a delta segment.
pub mod graph_compactor;
/// Universal columnar graph structural invariants.
pub mod invariants;
/// String interning for index keys.
pub mod intern;
/// Lazy heap-backed collections for graph schema types.
pub mod lazy_collections;
/// Snapshot version migration helpers.
pub mod migration;
/// On-disk `.rgctl/` artifact paths and `RGCTL_*` env helpers.
pub mod paths;
/// Mini query language over [`backend::MemoryBackend`].
pub mod query;
/// Node, edge, and graph schema types.
pub mod schema;
/// Append-only extract spill + external-sort compile to columnar.
pub mod segmented_spill;
/// Prepared and memory-mapped snapshot I/O.
pub mod snapshot;
/// Structural diff between two columnar snapshots.
pub mod snapshot_diff;
/// Deterministic mmap structured query (`find` / `callers` / `relations` / `inventory`).
pub mod structured_query;
/// Stable cross-snapshot node identity.
pub mod stable_key;
pub mod structural_sketch;

pub use code_graph::CodeGraph;
pub use code_index::{CodeIndex, CodeLocation, hash_code};
pub use columnar_snapshot::{
    COLUMNAR_SNAPSHOT_VERSION, ColumnarGraphMmap, cold_ext_deserialization_count,
    reset_cold_ext_deserialization_count, write_columnar_from_backend,
    write_columnar_from_nodes_edges,
};
pub use invariants::assert_graph_invariants;
pub use content_store::{
    CONTENT_STORE_FILE, ContentStore, INLINE_BODY_MAX_BYTES, hash_bytes, hash_text,
};
pub use csr::{CodeGraphCsr, edge_type_from_u8, edge_type_to_u8};
pub use export::{GraphSnapshot, export_json, export_json_to, import_json};
pub use query::{execute, execute_chunks, stream_query, QueryStream};
pub use graph_compactor::{
    CompactStats, DeltaSegment, GraphCompactor, compact_repo_snapshot, compact_snapshot_file,
};
pub use migration::{migrate_snapshot, migrate_v1_to_v2};
pub use schema::{AccessType, CallType, GRAPH_SCHEMA_VERSION, GraphParameter, SharedStr};
pub use segmented_spill::{
    DEFAULT_SORT_RUN_BYTES, FinishedSpill, SegmentedSpill, set_sort_run_bytes_override,
    write_columnar_from_spill,
};
pub use snapshot::{
    MmappedGraphSnapshot, PreparedGraphSnapshot, PreparedIndexes, SNAPSHOT_FILE, SnapshotNodeStore,
};
pub use snapshot_diff::{
    DiffSink, DiffStats, EdgeDeltaEvent, EdgeDeltaKind, NodeDeltaEvent, NodeDeltaKind, NoopDiffSink,
    SnapshotPair, VecDiffSink, diff_snapshots,
};
pub use structured_query::{
    ALL_EDGE_TYPES, ALL_NODE_TYPES, CallNeighborsResult, EntityRow, EdgeRow, FindResult,
    InventoryBy, InventoryCount, InventoryResult, QueryFilters, RelationDirection, RelationsResult,
    STRUCTURED_QUERY_SCHEMA_VERSION, ScopeMode, StructuredQuery, glob_match, import_package_prefix,
    parse_annotation_list, parse_edge_type, parse_node_type,
};
pub use stable_key::{
    MmapNodeKey, NodeRowRef, StableNodeKey, NAMESPACE_RGCTL, deterministic_node_id,
    extension_digest, node_row_ref, node_scope_path_at, stable_key_from_facets,
    stable_key_from_node, stable_key_from_row, stable_key_to_uuid,
};
pub use structural_sketch::{
    MIN_TOKEN_LEN, TOKEN_BLOOM_BITS, TOKEN_BLOOM_WORDS, TokenBloom, bloom_jaccard, bloom_popcount,
    build_token_bloom, empty_bloom, keyword_in_bloom, keyword_overlap_score, satisfies_keyword_and,
    tokenize_string_into,
};

/// Normalize path separators for consistent comparison.
///
/// Returns a borrow when the path already uses `/` (typical on POSIX), avoiding
/// a heap allocation on the snapshot-diff / stable-key hot path.
pub fn normalize_path_str(path: &str) -> std::borrow::Cow<'_, str> {
    if path.contains('\\') {
        std::borrow::Cow::Owned(path.replace('\\', "/"))
    } else {
        std::borrow::Cow::Borrowed(path)
    }
}

#[cfg(test)]
mod normalize_path_tests {
    use super::normalize_path_str;
    use std::borrow::Cow;

    #[test]
    fn posix_path_is_borrowed() {
        let path = "src/main.rs";
        match normalize_path_str(path) {
            Cow::Borrowed(b) => assert_eq!(b, path),
            Cow::Owned(_) => panic!("expected borrow for POSIX path"),
        }
    }

    #[test]
    fn backslash_path_is_normalized_owned() {
        let out = normalize_path_str("src\\main.rs");
        assert_eq!(out.as_ref(), "src/main.rs");
        assert!(matches!(out, Cow::Owned(_)));
    }
}
