//! Condition evaluators.

pub mod builtin_path;
pub mod compose;
pub mod dependency;
pub mod file;
pub mod filecontent;
pub mod go_referenced;
pub mod has_tags;
pub mod java_referenced;
pub mod pattern;

pub use pattern::{compile_kantra_pattern, normalize_kantra_glob};

use crate::findings::KantraViolation;

/// Match context for one evaluation site.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchSite {
    pub file: String,
    pub line: usize,
    pub symbol: Option<String>,
}

impl MatchSite {
    /// New site at file/line.
    pub fn new(file: impl Into<String>, line: usize) -> Self {
        Self {
            file: file.into(),
            line,
            symbol: None,
        }
    }

    /// Attach optional symbol name.
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }
}

/// Build a violation from a site.
pub fn violation(rule_id: &str, matched_by: &str, site: &MatchSite) -> KantraViolation {
    KantraViolation {
        rule_id: rule_id.to_string(),
        category: None,
        file: site.file.clone(),
        line: site.line,
        message: None,
        matched_by: matched_by.to_string(),
        symbol: site.symbol.clone(),
        targets: Vec::new(),
        sources: Vec::new(),
        effort: None,
        confidence: None,
        matched_location: None,
        enrichment: None,
    }
}

/// Build a high-confidence graph/call-site violation.
pub fn violation_high(
    rule_id: &str,
    matched_by: &str,
    site: &MatchSite,
    matched_location: &str,
) -> KantraViolation {
    let mut v = violation(rule_id, matched_by, site);
    v.confidence = Some("high".into());
    v.matched_location = Some(matched_location.into());
    v
}

/// Build a low-confidence lexical-fallback violation.
pub fn violation_low(
    rule_id: &str,
    matched_by: &str,
    site: &MatchSite,
    matched_location: &str,
) -> KantraViolation {
    let mut v = violation(rule_id, matched_by, site);
    v.confidence = Some("low".into());
    v.matched_location = Some(matched_location.into());
    v
}
