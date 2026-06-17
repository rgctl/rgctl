//! Git integration helpers for hooks and incremental updates.

use crate::error::{Error, Result};
use crate::incremental::file_tracker::normalize_path_str;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Run a git command in `repo_root` and return stdout on success.
fn git_output(repo_root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|e| Error::Other(format!("Failed to run git: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Other(format!("git {} failed: {stderr}", args.join(" "))));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Resolve the `.git` directory for a repository.
pub fn git_dir(repo_root: &Path) -> Result<PathBuf> {
    let out = git_output(repo_root, &["rev-parse", "--git-dir"])?;
    let rel = out.trim();
    let path = PathBuf::from(rel);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(repo_root.join(path))
    }
}

/// Staged file paths relative to the repository root.
pub fn git_staged_files(repo_root: &Path) -> Result<Vec<String>> {
    let out = git_output(repo_root, &["diff", "--cached", "--name-only", "--diff-filter=ACMR"])?;
    Ok(parse_name_lines(repo_root, &out))
}

/// Files changed in the latest commit.
pub fn git_committed_files(repo_root: &Path) -> Result<Vec<String>> {
    let out = git_output(
        repo_root,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
    )?;
    Ok(parse_name_lines(repo_root, &out))
}

/// Files that differ between two git refs (relative paths).
pub fn git_diff_files(repo_root: &Path, old_ref: &str, new_ref: &str) -> Result<Vec<String>> {
    let out = git_output(repo_root, &["diff", "--name-only", old_ref, new_ref])?;
    Ok(parse_name_lines(repo_root, &out))
}

/// Current HEAD commit hash.
pub fn git_head(repo_root: &Path) -> Option<String> {
    git_output(repo_root, &["rev-parse", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_name_lines(_repo_root: &Path, stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(normalize_path_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_git_repo(root: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(root)
            .output()
            .unwrap();
    }

    #[test]
    fn test_git_staged_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        init_git_repo(root);
        fs::write(root.join("lib.rs"), "fn main() {}\n").unwrap();
        Command::new("git")
            .args(["add", "lib.rs"])
            .current_dir(root)
            .output()
            .unwrap();

        let staged = git_staged_files(root).unwrap();
        assert_eq!(staged, vec!["lib.rs"]);
    }
}
