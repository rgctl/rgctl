//! Graph analysis algorithms for rBuilder

#![warn(missing_docs)]

pub mod blast_radius;
pub mod callgraph;
pub mod centrality;
pub mod cfg;
pub mod cfg_builder;
pub mod community;
pub mod complexity;
pub mod dataflow;
pub mod def_use;
pub mod dependency;
pub mod dominance;
pub mod flow_cache;
pub mod graph_utils;
pub mod interprocedural_cfg;
pub mod interprocedural_slicing;
pub mod pdg;
pub mod slicing;
pub mod taint;
pub mod type_inference;

pub use blast_radius::{BlastRadiusAnalyzer, BlastRadiusReport, DataFlowImpact};
pub use callgraph::{CallGraph, CallGraphEdge, CallGraphNode};
pub use centrality::{
    degree_centrality, identify_hotspots, CentralityAnalyzer, CentralityReport, CentralityScore,
    CentralityScores,
};
pub use cfg::{
    BasicBlock, BlockId, CfgEdge, CfgEdgeType, ControlFlowGraph, Statement, StatementKind,
};
pub use cfg_builder::build_cfg_for_function;
pub use community::{
    detect_communities, Community, CommunityDetector, CommunityResult, DashboardCommunity,
};
pub use complexity::{classify_complexity, ComplexityAnalyzer, ComplexityLevel, ComplexityReport};
pub use dataflow::{compute_reaching_definitions, Definition, ReachingDefs};
pub use def_use::{extract_def_use, extract_used_variables};
pub use dependency::{CircularDependency, DependencyAnalyzer, ImpactResult};
pub use dominance::DominatorTree;
pub use flow_cache::{CachedAnalysis, CfgPdgCache, FlowCache, NodePdgCache};
pub use interprocedural_cfg::InterproceduralCFG;
pub use interprocedural_slicing::{InterproceduralSlice, InterproceduralSlicer};
pub use pdg::{
    ControlDependency, DataDepType, DataDependency, PdgNode, PdgNodeId, ProgramDependenceGraph,
};
pub use slicing::{BackwardSlicer, CodeSlice, SliceCriterion};
pub use taint::{Sanitizer, TaintAnalyzer, TaintFlow, TaintSink, TaintSource};
pub use type_inference::{confidence_for, InferredType, TypeInferenceEngine, VariableType};
