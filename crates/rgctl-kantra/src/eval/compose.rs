//! Composite `and` / `or` / `not` evaluation.

use crate::eval::MatchSite;
use crate::findings::KantraViolation;
use crate::schema::WhenClause;
use std::collections::HashSet;

/// Evaluate composite clause against per-clause match sites.
pub fn eval_compose(
    clause: &WhenClause,
    rule_id: &str,
    leaf_sites: &[Vec<MatchSite>],
    leaf_index: &mut usize,
) -> Vec<KantraViolation> {
    match clause {
        WhenClause::And(items) => eval_and_sites(items, leaf_sites, leaf_index)
            .into_iter()
            .map(|s| crate::eval::violation(rule_id, "and", &s))
            .collect(),
        WhenClause::Or(items) => {
            let mut union = HashSet::new();
            for item in items {
                union.extend(eval_compose_sites(item, leaf_sites, leaf_index));
            }
            union
                .into_iter()
                .map(|s| crate::eval::violation(rule_id, "or", &s))
                .collect()
        }
        WhenClause::Not(inner) => {
            let inner_sites: HashSet<_> = eval_compose_sites(inner, leaf_sites, leaf_index)
                .into_iter()
                .collect();
            // NOT is only meaningful with a file-scoped universe; caller supplies leaf sites.
            let _ = inner_sites;
            Vec::new()
        }
        _ => {
            let sites = leaf_sites.get(*leaf_index).cloned().unwrap_or_default();
            *leaf_index += 1;
            sites
                .into_iter()
                .map(|s| {
                    let matched_by = match clause {
                        WhenClause::FileContent { .. } => "builtin.filecontent",
                        WhenClause::File { .. } => "builtin.file",
                        WhenClause::HasTags { .. } => "builtin.hasTags",
                        WhenClause::GoReferenced { .. } => "go.referenced",
                        WhenClause::JavaReferenced { .. } => "java.referenced",
                        _ => "leaf",
                    };
                    crate::eval::violation(rule_id, matched_by, &s)
                })
                .collect()
        }
    }
}

fn eval_compose_sites(
    clause: &WhenClause,
    leaf_sites: &[Vec<MatchSite>],
    leaf_index: &mut usize,
) -> Vec<MatchSite> {
    match clause {
        WhenClause::And(items) => eval_and_sites(items, leaf_sites, leaf_index),
        WhenClause::Or(items) => {
            let mut union = HashSet::new();
            for item in items {
                union.extend(eval_compose_sites(item, leaf_sites, leaf_index));
            }
            union.into_iter().collect()
        }
        WhenClause::Not(inner) => {
            let _ = eval_compose_sites(inner, leaf_sites, leaf_index);
            Vec::new()
        }
        _ => {
            let sites = leaf_sites.get(*leaf_index).cloned().unwrap_or_default();
            *leaf_index += 1;
            sites
        }
    }
}

/// Evaluate an `and` block following Konveyor `analyzer-lsp` semantics:
/// - Every positive sub-clause must match at least once (`!sites.is_empty()`).
/// - Every `not` sub-clause excludes files where the negated condition matched
///   (and fails the `and` if all positive matches are excluded or if a project-wide
///   negated check matched).
/// - When all conditions are satisfied, emit the deduplicated union of positive
///   sub-clause match sites.
fn eval_and_sites(
    items: &[WhenClause],
    leaf_sites: &[Vec<MatchSite>],
    leaf_index: &mut usize,
) -> Vec<MatchSite> {
    if items.is_empty() {
        return Vec::new();
    }
    let mut positive_sets: Vec<Vec<MatchSite>> = Vec::new();
    let mut excluded_files: HashSet<String> = HashSet::new();
    for item in items {
        if let WhenClause::Not(inner) = item {
            let neg = eval_compose_sites(inner, leaf_sites, leaf_index);
            for s in neg {
                excluded_files.insert(s.file);
            }
        } else {
            positive_sets.push(eval_compose_sites(item, leaf_sites, leaf_index));
        }
    }
    if positive_sets.is_empty() || positive_sets.iter().any(|s| s.is_empty()) {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for set in positive_sets {
        for s in set {
            if !excluded_files.contains(&s.file)
                && seen.insert((s.file.clone(), s.line, s.symbol.clone()))
            {
                out.push(s);
            }
        }
    }
    out
}
