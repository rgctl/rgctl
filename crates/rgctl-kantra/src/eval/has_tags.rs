//! `builtin.hasTags` evaluation.

use crate::engine::EvalNode;
use crate::eval::{MatchSite, violation};
use crate::findings::KantraViolation;
use std::collections::{HashMap, HashSet};

/// Match nodes whose labels contain all required tags, or match sites emitted
/// by earlier tag-producing rules (`tag: [...]`).
pub fn eval_has_tags(
    rule_id: &str,
    tags: &[String],
    nodes: &[EvalNode],
    active_tag_sites: &HashMap<String, Vec<MatchSite>>,
) -> Vec<KantraViolation> {
    let mut out = Vec::new();
    let mut seen: HashSet<(String, usize)> = HashSet::new();
    for node in nodes {
        if tags.iter().all(|t| node.labels.iter().any(|l| l == t)) {
            let file = node.file_path.clone().unwrap_or_default();
            let line = node.start_line.unwrap_or(1);
            if seen.insert((file.clone(), line)) {
                out.push(violation(
                    rule_id,
                    "builtin.hasTags",
                    &MatchSite::new(file, line).with_symbol(&node.name),
                ));
            }
        }
    }
    if !tags.is_empty() && tags.iter().all(|t| active_tag_sites.contains_key(t)) {
        if let Some(sites) = active_tag_sites.get(&tags[0]) {
            for site in sites {
                if seen.insert((site.file.clone(), site.line)) {
                    out.push(violation(rule_id, "builtin.hasTags", site));
                }
            }
        }
    }
    out
}
