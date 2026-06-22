//! Rule engine for automatic labeling

pub mod actions;
pub mod matcher;
pub mod schema;

pub use actions::{RuleApplicationReport, RuleEngine};
pub use matcher::RuleMatcher;
pub use schema::{MatchCondition, MatchLeaf, Rule, RuleAction, Ruleset};
