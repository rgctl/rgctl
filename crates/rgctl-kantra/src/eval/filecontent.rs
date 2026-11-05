//! `builtin.filecontent` evaluation.

use crate::eval::{MatchSite, violation};
use crate::findings::KantraViolation;
use glob::Pattern;
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Source text cache keyed by repo-relative file path strings.
pub type SourceCache = HashMap<String, Arc<String>>;
use std::collections::HashSet;

/// Evaluate filecontent rules in parallel over files.
pub fn eval_filecontent(
    rule_id: &str,
    pattern: &str,
    file_pattern: Option<&str>,
    repo_root: &Path,
    files: &[PathBuf],
    sources: &SourceCache,
    skip_files: &HashSet<String>,
) -> Result<Vec<KantraViolation>, regex::Error> {
    let re = Regex::new(pattern)?;
    let glob = file_pattern.map(Pattern::new).transpose().ok().flatten();
    let file_re = file_pattern.and_then(|fp| Regex::new(fp).ok());

    let violations: Vec<KantraViolation> = files
        .par_iter()
        .flat_map(|file| {
            let rel = file
                .strip_prefix(repo_root)
                .unwrap_or(file)
                .to_string_lossy()
                .replace('\\', "/");
            if skip_files.contains(&rel) {
                return Vec::new();
            }
            if file_pattern.is_some() {
                let glob_ok = glob.as_ref().is_some_and(|g| g.matches(&rel));
                let re_ok = file_re.as_ref().is_some_and(|r| r.is_match(&rel));
                if !glob_ok && !re_ok {
                    return Vec::new();
                }
            }
            let content = match sources.get(&rel).or_else(|| sources.get(file.to_str()?)) {
                Some(c) => c.as_str(),
                None => return Vec::new(),
            };
            let mut hits = Vec::new();
            for (i, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    hits.push(violation(
                        rule_id,
                        "builtin.filecontent",
                        &MatchSite::new(&rel, i + 1),
                    ));
                }
            }
            hits
        })
        .collect();
    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_line() {
        let mut sources = SourceCache::new();
        sources.insert(
            "main.go".into(),
            Arc::new("import \"golang.org/x/crypto/hkdf\"\n".into()),
        );
        let files = vec![PathBuf::from("main.go")];
        let hits = eval_filecontent(
            "r1",
            "hkdf",
            None,
            Path::new("."),
            &files,
            &sources,
            &HashSet::new(),
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 1);
    }
}
