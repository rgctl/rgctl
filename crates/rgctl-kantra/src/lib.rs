//! Native Konveyor Kantra rule evaluation against rgctl graphs and source cache.

pub mod cache;
pub mod catalog;
pub mod classify;
pub mod engine;
pub mod enrich;
pub mod error;
pub mod eval;
pub mod findings;
pub mod index;
pub mod loader;
pub mod resolve;
pub mod schema;
pub mod violates;

pub use cache::{KantraFileCache, cache_dir, hash_file_content, ruleset_hash};
pub use catalog::{
    KantraCatalog, merge_ruleset_labels, rule_konveyor_sources, rule_konveyor_targets,
    rule_matches_target,
};
pub use engine::{EvalContext, EvalEdge, EvalGraph, EvalNode, EvalStageTimings, KantraEngine};
pub use enrich::{NodeMetrics, enrich_findings};
pub use error::KantraError;
pub use findings::{KantraFindings, KantraViolation, SkippedRule, ViolationEnrichment};
pub use index::{hydrate_catalog, rewrite_snapshot_with_catalog, rule_node_id};
pub use loader::KantraRuleset;
pub use resolve::ViolationResolver;
pub use schema::{RuleSupport, WhenClause};
pub use violates::{materialize_violates_edges, rewrite_snapshot_with_violations};
