//! Graph analysis algorithms for rgctl

#![warn(missing_docs)]

pub mod alias;
pub mod analysis_pack;
pub mod ast_skeleton;
pub mod blast_engine_snapshot;
pub mod blast_radius;
pub mod boundary;
pub mod calendar_policy;
pub mod blast_radius_scc;
pub mod blast_slice_handoff;
pub mod callgraph;
pub mod centrality;
pub mod centrality_approx;
pub mod cfg;
pub mod cfg_builder;
pub mod cfg_pdg_archive;
pub mod clones;
pub mod cold_metadata;
pub mod community;
pub mod community_label;
pub mod community_query;
pub mod complexity;
pub mod cpg;
pub mod cpg_export;
pub mod dataflow;
pub mod def_use;
pub mod dependency;
pub mod dominance;
pub mod field_write;
pub mod field_write_locals;
pub mod flow_cache;
pub mod fragment_clones;
pub mod graph_utils;
pub mod hash_maps;
pub mod interprocedural_cfg;
pub mod interprocedural_slicing;
pub mod language_profile;
pub mod macro_call_index;
pub mod macro_call_lookup;
pub mod migration;
pub mod node_lookup;
pub mod pdg;
pub mod policy;
pub mod policy_diff;
pub mod violation_ledger;
pub mod results;
pub mod review_paths;
pub mod scoped_policy;
pub mod semantic_code_daemon;
pub mod semantic_diffuse;
#[cfg(feature = "semantic-onnx")]
pub mod semantic_embedded;
pub mod semantic_embedder;
pub mod semantic_extract;
pub mod semantic_fusion;
pub mod semantic_hybrid;
#[cfg(feature = "semantic-onnx")]
pub mod semantic_onnx;
pub mod semantic_onnx_tokenizer;
pub mod semantic_search;
pub mod semantic_vocab;
pub mod sese;
pub mod slicing;
pub mod storage;
pub mod structural_topology;
pub mod taint;
pub mod taint_rules;
pub mod type_inference;
pub mod wl_hash;

pub use alias::may_alias_names;
pub use ast_skeleton::{
    AST_SKELETON_ARCHIVE_FILE, AST_SKELETON_VERSION, AstSkeletonArchive, AstSkeletonKind,
    AstSkeletonNode, AstSkeletonRecord, build_function_skeleton, build_function_skeleton_from_tree,
};
pub use blast_engine_snapshot::{BLAST_SNAPSHOT_FILE, BlastEngineSnapshot, try_load_engine};
pub use blast_radius::{
    BlastRadiusAnalyzer, BlastRadiusReport, DataFlowImpact, resolve_unique_symbol,
};
pub use blast_radius_scc::{
    BlastRadiusEngine, BlastRadiusResult, EngineStats, SccNode, impact_score_from_counts,
};
pub use boundary::{
    bundled_boundary_catalogs, classify_boundaries, BoundaryCatalog, BoundaryKind, BoundaryLabel,
    BoundaryNodeRef, BoundaryRule,
};
pub use blast_slice_handoff::{
    BlastSliceTrace, SliceHandoffSeed, criterion_for_parameter, filter_handoff_seeds_by_index,
    load_source_files, resolve_handoff_seeds, resolve_handoff_seeds_for_indices,
    resolve_handoff_seeds_with_call_graph, trace_blast_to_slices, trace_blast_to_slices_with_blast,
};
pub use callgraph::CallGraph;
pub use centrality::{
    BetweennessCentrality, CentralityAnalyzer, CentralityReport, CentralityRunSummary,
    CentralityScore, CentralityScores, CsrOutAdj, DegreeCentrality, FastPageRank, FlatGraphIndex,
    HarmonicCentrality, LARGE_GRAPH_PAGERANK_ITERATIONS, LARGE_GRAPH_PAGERANK_NODE_LIMIT,
    LARGE_GRAPH_PAGERANK_TOLERANCE, PAGERANK_TOLERANCE, PageRankStats, STRUCTURAL_EDGE_TYPES,
    adaptive_pagerank_config, default_behavioral_edges, degree_centrality,
};
pub use centrality_approx::{
    BetweennessMode, CentralityApproxStats, DEFAULT_EXACT_CENTRALITY_LIMIT,
    DEFAULT_HYPERBALL_ROUNDS, DEFAULT_SAMPLE_PIVOTS, HYPERBALL_EXACT_THRESHOLD,
    HYPERLOGLOG_PRECISION, HarmonicMode, HyperBallHarmonic, LARGE_GRAPH_HYPERBALL_NODE_LIMIT,
    LARGE_GRAPH_HYPERBALL_ROUNDS, SampledBetweenness,
};
pub use cfg::{
    BasicBlock, BlockId, CfgEdge, CfgEdgeType, ControlFlowGraph, DefVar, Statement,
    StatementKind,
};
pub use cfg_builder::{
    FunctionLocation, ParsedSourceFile, build_cfg_for_function, build_cfg_for_function_in_tree,
    index_function_locations,
};
pub use cfg_pdg_archive::{CFG_PDG_ARCHIVE_FILE, CfgPdgArchive, CfgPdgRecord};
pub use clones::{
    BloomCloneOptions, CLONE_REPORT_SCHEMA_VERSION, CLONE_REPORT_SCHEMA_VERSION_V2,
    CLONES_FRAGMENT_SIDECAR_FILE, CLONES_SIDECAR_FILE, DEFAULT_BLOOM_MAX_BUCKET,
    DEFAULT_BLOOM_THRESHOLD, DEFAULT_MIN_LOC, FragmentCloneFilters, FragmentCloneGroup,
    FragmentCloneReport, FragmentMember, FragmentSeedInfo, MODE_BLOOM, MODE_EXACT, MODE_FRAGMENT,
    MODE_SEMANTIC, MODE_STRUCTURAL, CloneError, CloneFilters, CloneGroup, CloneMember, CloneReport,
    ExactCloneOptions, bloom_clones_with_cache, build_bloom_report, build_exact_report,
    clones_sidecar_path, clones_sidecar_path_for_mode, exact_clones_with_cache,
    fragment_clones_sidecar_path, group_bloom_from_nodes, group_exact_from_nodes,
    iter_bloom_functions, iter_hashed_functions, load_fragment_sidecar_if_fresh,
    load_sidecar_if_fresh, parse_mode, path_excluded, save_fragment_sidecar, save_sidecar,
};
pub use cold_metadata::ColdMetadataDb;
pub use community::{
    Community, CommunityDetector, CommunityResult, DEFAULT_HUB_SIGMA_K,
    DEFAULT_MAX_FROZEN_FRACTION, DEFAULT_MIN_NODES_FOR_HUB_STRIP, DashboardCommunity,
    HubStripPolicy, TieBreakStrategy, community_edge_types_for_backend,
    default_community_edge_types, detect_communities,
};
pub use community_label::{
    CommunityLabelHints, dedupe_community_labels, fill_community_labels,
    fill_community_labels_from_nodes, infer_community_label,
};
pub use community_query::{
    CommunityInfo, CommunityQueryContext, VIRTUAL_COMMUNITY_PROP, VIRTUAL_COMMUNITY_VALUE,
    is_virtual_community,
};
pub use complexity::{ComplexityAnalyzer, ComplexityLevel, ComplexityReport, classify_complexity};
pub use cpg::{
    CpgCallEdge, CpgCallsInfo, CpgFlowStep, CpgFlowsArgs, CpgFlowsResult, CpgFunctionInfo,
    CpgMutationHit, CpgMutationsResult, CpgStatus, archive_path, cpg_calls, cpg_flows,
    cpg_function, cpg_mutations, cpg_status,
};
pub use cpg_export::{CpgExportFormat, CpgExportScope, export_cpg};
pub use dataflow::{Definition, ReachingDefs, compute_reaching_definitions};
pub use def_use::{extract_def_use, extract_used_variables};
pub use dependency::{CircularDependency, DependencyAnalyzer, ImpactResult};
pub use dominance::{DominatorTree, verify_idom_acyclic};
pub use field_write::{
    FIELD_WRITE_INDEX_FILE, FIELD_WRITE_INDEX_VERSION, FieldWrite, FieldWriteIndex, FieldWriteKind,
    MutationQuery,
    build_and_save_field_write_index, extract_writes_from_cfg,
};
pub use field_write_locals::{
    LocalsParseContext, build_file_local_types_from_tree, build_file_local_types_index,
    language_from_path, merge_local_types,
};
pub use flow_cache::{CachedAnalysis, CfgPdgCache, FlowCache, NodePdgCache};
pub use fragment_clones::{
    discover_fragment_clones, query_fragment_clones, resolve_cfg_for_function,
    stage1_filter_functions, FragmentSeedQuery,
};
pub use graph_utils::{
    DEFAULT_TRAVERSAL_DEPTH, PetGraphView, TraversalConfig, edge_type_set,
    filter_impact_by_caller_depth,
};
pub use interprocedural_cfg::{
    InterproceduralCFG, InterproceduralCfgAccess, InterproceduralCfgView,
};
pub use interprocedural_slicing::{InterproceduralSlice, InterproceduralSlicer};
pub use language_profile::{
    LanguageAnalysisProfile, canonical_language_id, cfg_language_id_from_path, cfg_language_ids,
    cfg_language_list, function_kinds_for, language_id_from_path, parse_source,
    profile_for_language, taint_enabled_for,
};
pub use macro_call_index::{GraphFingerprint, MacroCallIndex, MacroCallIndexEntry, SymbolContext};
pub use macro_call_lookup::{
    MacroCallLookupDb, MacroCallLookupRow, MacroIndexEntry, ParsedSymbol, candidates_from_backend,
    candidates_from_snapshot, canonical_fqn_from_node, canonical_fqn_from_qualified_name,
    class_name_from_node, inferred_target_metadata, language_from_node, parse_fqn_symbol,
    resolve_symbol_uuid, try_parse_symbol_uuid,
};
pub use migration::{
    MIGRATION_GRAPH_SCHEMA_VERSION, MIGRATION_PLAN_SCHEMA_VERSION, MigrationCommunityEdge,
    MigrationCommunityNode, MigrationGraphPayload, MigrationOrderMode, MigrationPlanPayload,
    MigrationPlanStep, MigrationWeights, build_migration_graph, compute_migration_plan,
};
pub use node_lookup::NodeLookup;
pub use pdg::{
    ControlDependency, DataDepType, DataDependency, PdgBuildOptions, PdgNode, PdgNodeId,
    ProgramDependenceGraph,
};
pub use policy::{DomainId, PolicyRegistry, PolicyViolation, check_policies, evaluate_policies};
pub use calendar_policy::{
    CalendarDisposition, CalendarSeverity, GraceSeverity, PolicyTemporal,
    evaluate_calendar_disposition, evaluate_calendar_for_deltas, iso_date_to_day_number,
    system_today_days, system_unix_secs,
};
pub use policy_diff::{
    PolicyDelta, TemporalClass, apply_ledger_regression, evaluate_temporal, record_deltas_to_ledger,
};
pub use violation_ledger::{
    VIOLATION_LEDGER_FILE, ViolationLedger, ViolationLedgerEntry, ledger_entry_from_delta,
};
pub use review_paths::{
    AmbiguousCandidate, CallEdgeCounts, ChangeSummary, DEFAULT_DOWNSTREAM_DEPTH,
    DEFAULT_FANOUT_CAP, DEFAULT_MAX_NEIGHBOR_NODES, DEFAULT_MAX_SYMBOLS, DEFAULT_UPSTREAM_DEPTH,
    PathDelta, ReviewPathsOptions, ReviewPathsReport, SideLocation, SymbolPathReport,
    TruncationFlags, UnscoredFile, build_review_paths_report,
};
pub use scoped_policy::{
    build_pr_check_centrality, collect_upstream_call_closure, hydrate_subset, load_centrality_cache,
    scope_entity_ids,
};
pub use results::{
    AnalysisResults, BlastRadiusMetrics, BlastRadiusTable, CentralityMetrics, CentralityTable,
    CommunityTable, ComplexityTable, StructuralSketchTable,
};
pub use semantic_code_daemon::{
    CODE_DAEMON_MAX_SEQ_LEN, CODE_DAEMON_MODEL_ID, CODE_DAEMON_MRL_DIMS, CODE_DAEMON_NATIVE_DIMS,
    CODE_DAEMON_ONNX_FILE, CODE_DAEMON_TOKENIZER_FILE, default_model_dir, default_model_path,
    default_tokenizer_path, validate_mrl_dimensions,
};
#[cfg(feature = "semantic-onnx")]
pub use semantic_code_daemon::{load_code_daemon_embedder, load_embedded_code_daemon_embedder};
pub use semantic_diffuse::{DiffuseConfig, DiffuseNeighborMode, diffuse_call_topology};
pub use semantic_embedder::{
    EmbedderChoice, OnnxReloadOptions, SemanticEmbedder, SignHashEmbedder, embedder_for_index,
    resolve_embedder,
};
pub use semantic_extract::{
    FunctionTokenSketch, MIN_TOKEN_LEN, extract_body_tokens_for_node,
    extract_body_tokens_from_slice, resolve_source_path,
};
pub use semantic_fusion::{
    DEFAULT_CANDIDATE_POOL, FusionCandidate, SemanticFusionConfig, entry_metadata_tokens,
    entry_package_key, fuse_candidates, hamming_similarity, keyword_and_matches,
    name_overlap_score, query_index_with_fusion, query_keywords,
};
pub use semantic_hybrid::{
    BlastSummaryProvider, SemanticBlastSummary, SemanticExpandConfig, SemanticExpandMode,
    SemanticExpandedNode, SemanticExpansion, blast_summary_from_result, expand_call_neighbors,
    expand_semantic_hits,
};
pub use semantic_search::{
    CommunitySemanticHit, DEFAULT_EMBEDDING_DIMENSIONS, EMBED_BODIES_MODEL_SUFFIX,
    SEMANTIC_INDEX_FILE, SEMANTIC_INDEX_SCHEMA_VERSION, SIGN_HASH_MODEL_ID, SemanticBuildOptions,
    SemanticBuildStats, SemanticEntry, SemanticHit, SemanticIndex, SemanticIndexScope,
    build_from_backend, build_index, embed_text_for_doc_node, embed_text_for_function,
    embed_text_for_node, embed_text_for_scope, embedder_model_id, hamming_distance, hamming_top_k,
    persist_semantic_model_id, quantize_binary, query_communities, query_index,
    query_index_with_embedder, sign_hash_embed,
};
pub use semantic_vocab::{
    DEFAULT_VOCAB_TOKEN_LIST, TokenSpaceAccumulator, VOCAB_ACCUMULATE_DISTILLED_ID,
    VOCAB_ACCUMULATE_MODEL_ID, VOCAB_NATIVE_DIMENSIONS, VocabAccumulateEmbedder,
    distill_vocab_matrix, encode_rbvk, parse_vocab_token_list,
};
pub use slicing::{
    BackwardSlicer, CodeSlice, ForwardSlicer, SliceCriterion, SliceDirection, SliceOptions,
    compute_slice, compute_slice_with_options,
};
pub use sese::{DEFAULT_MAX_STATEMENTS, DEFAULT_MIN_STATEMENTS, SeseRegion, extract_sese_regions};
pub use storage::{AnalysisIndexEntry, AnalysisStorage, FunctionAnalysis, FunctionIdSyncEntry};
pub use structural_topology::StructuralTopology;
pub use taint::{Sanitizer, TaintAnalyzer, TaintFlow, TaintSink, TaintSource};
pub use taint_rules::{TaintRuleSet, bundled_cwe_catalog, CweCatalogEntry};
pub use type_inference::{InferredType, TypeInferenceEngine, VariableType, confidence_for};
pub use wl_hash::{CanonicalEdgeTag, CanonicalStatementKind, WeisfeilerLehmanHasher};
