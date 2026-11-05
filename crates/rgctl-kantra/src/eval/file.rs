//! `builtin.file` glob evaluation.

use crate::eval::{MatchSite, violation};
use crate::findings::KantraViolation;
use glob::Pattern;
use regex::Regex;
use std::path::{Path, PathBuf};

/// Match file paths against a glob or regex pattern.
pub fn eval_file(
    rule_id: &str,
    pattern: &str,
    repo_root: &Path,
    files: &[PathBuf],
) -> Result<Vec<KantraViolation>, glob::PatternError> {
    let glob = Pattern::new(pattern).ok();
    let re = Regex::new(&format!("^(?:{pattern})$")).ok();
    let mut out = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(repo_root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let basename = Path::new(&rel)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&rel);
        let glob_ok = glob.as_ref().is_some_and(|g| {
            g.matches(&rel) || g.matches(basename) || g.matches(file.to_string_lossy().as_ref())
        });
        let re_ok = re
            .as_ref()
            .is_some_and(|r| r.is_match(&rel) || r.is_match(basename));
        if glob_ok || re_ok {
            out.push(violation(rule_id, "builtin.file", &MatchSite::new(&rel, 1)));
        }
    }
    Ok(out)
}
