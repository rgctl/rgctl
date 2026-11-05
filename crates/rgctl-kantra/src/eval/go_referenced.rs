//! `go.referenced` evaluation.

use crate::engine::EvalNode;
use crate::eval::{MatchSite, compile_kantra_pattern, violation};
use crate::findings::KantraViolation;

const CODE_EXTENSIONS: &[&str] = &[".go"];

/// Match Go import nodes by qualified_name.
pub fn eval_go_referenced(
    rule_id: &str,
    pattern: &str,
    nodes: &[EvalNode],
) -> Result<Vec<KantraViolation>, regex::Error> {
    let re = compile_kantra_pattern(pattern)?;
    let mut out = Vec::new();
    for node in nodes {
        if node.node_type != "Import" {
            continue;
        }
        if !is_code_file(node.file_path.as_deref()) {
            continue;
        }
        let qn = node.qualified_name.as_deref().unwrap_or(&node.name);
        if re.is_match(qn) || re.is_match(&node.name) {
            let file = node.file_path.clone().unwrap_or_default();
            let line = node.start_line.unwrap_or(1);
            out.push(violation(
                rule_id,
                "go.referenced",
                &MatchSite::new(file, line).with_symbol(qn),
            ));
        }
    }
    Ok(out)
}

fn is_code_file(path: Option<&str>) -> bool {
    path.map(|p| CODE_EXTENSIONS.iter().any(|ext| p.ends_with(ext)))
        .unwrap_or(false)
}
