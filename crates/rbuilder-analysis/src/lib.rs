//! Graph analysis algorithms for rBuilder

#![warn(missing_docs)]

pub mod blast_engine_snapshot;
pub mod blast_radius;
pub mod blast_radius_scc;
pub mod blast_slice_handoff;
pub mod callgraph;
pub mod centrality;
pub mod cfg;
pub mod cfg_builder;
pub mod cfg_pdg_archive;
pub mod community;
pub mod complexity;
pub mod dataflow;
pub mod def_use;
pub mod dependency;
pub mod dominance;
pub mod flow_cache;
pub mod graph_utils;
pub mod language_profile;
pub mod interprocedural_cfg;
pub mod interprocedural_slicing;
pub mod macro_call_index;
pub mod macro_call_lookup;
pub mod pdg;
pub mod policy;
pub mod results;
pub mod slicing;
pub mod storage;
pub mod taint;
pub mod type_inference;

pub use blast_engine_snapshot::{try_load_engine, BlastEngineSnapshot, BLAST_SNAPSHOT_FILE};
pub use blast_radius::{
    resolve_unique_symbol, BlastRadiusAnalyzer, BlastRadiusReport, DataFlowImpact,
};
pub use blast_radius_scc::{
    impact_score_from_counts, BlastRadiusEngine, BlastRadiusResult, EngineStats, SccNode,
};
pub use blast_slice_handoff::{
    criterion_for_parameter, filter_handoff_seeds_by_index, load_source_files,
    resolve_handoff_seeds, resolve_handoff_seeds_for_indices, trace_blast_to_slices,
    trace_blast_to_slices_with_blast, BlastSliceTrace, SliceHandoffSeed,
};
pub use callgraph::CallGraph;
pub use centrality::{
    default_behavioral_edges, degree_centrality, BetweennessCentrality, CentralityAnalyzer,
    CentralityReport, CentralityScore, CentralityScores, DegreeCentrality, FastPageRank,
    FlatGraphIndex, PageRankStats, PAGERANK_TOLERANCE, STRUCTURAL_EDGE_TYPES,
};
pub use cfg::{
    BasicBlock, BlockId, CfgEdge, CfgEdgeType, ControlFlowGraph, Statement, StatementKind,
};
pub use cfg_builder::build_cfg_for_function;
pub use cfg_pdg_archive::{CfgPdgArchive, CfgPdgRecord, CFG_PDG_ARCHIVE_FILE};
pub use community::{
    default_community_edge_types, detect_communities, Community, CommunityDetector,
    CommunityResult, DashboardCommunity,
};
pub use complexity::{classify_complexity, ComplexityAnalyzer, ComplexityLevel, ComplexityReport};
pub use dataflow::{compute_reaching_definitions, Definition, ReachingDefs};
pub use def_use::{extract_def_use, extract_used_variables};
pub use dependency::{CircularDependency, DependencyAnalyzer, ImpactResult};
pub use dominance::{verify_idom_acyclic, DominatorTree};
pub use flow_cache::{CachedAnalysis, CfgPdgCache, FlowCache, NodePdgCache};
pub use graph_utils::{filter_impact_by_caller_depth, PetGraphView};
pub use language_profile::{
    canonical_language_id, cfg_language_id_from_path, cfg_language_ids, cfg_language_list,
    function_kinds_for, language_id_from_path, parse_source, profile_for_language,
    taint_enabled_for, LanguageAnalysisProfile,
};
pub use interprocedural_cfg::InterproceduralCFG;
pub use interprocedural_slicing::{InterproceduralSlice, InterproceduralSlicer};
pub use macro_call_index::{GraphFingerprint, MacroCallIndex, MacroCallIndexEntry, SymbolContext};
pub use macro_call_lookup::{
    candidates_from_backend, candidates_from_snapshot, canonical_fqn_from_node,
    canonical_fqn_from_qualified_name, class_name_from_node, inferred_target_metadata,
    language_from_node, parse_fqn_symbol, resolve_symbol_uuid, try_parse_symbol_uuid,
    MacroCallLookupDb, MacroCallLookupRow, MacroIndexEntry, ParsedSymbol,
};
pub use pdg::{
    ControlDependency, DataDepType, DataDependency, PdgNode, PdgNodeId, ProgramDependenceGraph,
};
pub use policy::{check_policies, evaluate_policies, DomainId, PolicyRegistry, PolicyViolation};
pub use results::{
    AnalysisResults, BlastRadiusMetrics, BlastRadiusTable, CentralityMetrics, CentralityTable,
    CommunityTable, ComplexityTable,
};
pub use slicing::{BackwardSlicer, CodeSlice, SliceCriterion};
pub use storage::{AnalysisStorage, FunctionAnalysis};
pub use taint::{Sanitizer, TaintAnalyzer, TaintFlow, TaintSink, TaintSource};
pub use type_inference::{confidence_for, InferredType, TypeInferenceEngine, VariableType};
