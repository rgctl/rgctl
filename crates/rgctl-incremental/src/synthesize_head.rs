//! Delta head snapshot synthesis from a cached base artifact + git change set.
//!
//! Writes under `{repo}/.rgctl/synth-head/` by default — never clobbers the live
//! session `graph.snapshot.bin` (#124).

use crate::pr_scope::{git_diff_name_status, git_diff_worktree_vs_head, resolve_snapshot_path};
use crate::updater::{IncrementalUpdater, UpdateOptions};
use rgctl_error::{Error, Result};
use rgctl_graph::code_graph::GRAPH_DIR;
use rgctl_graph::snapshot::MmappedGraphSnapshot;
use rgctl_registry::LanguageRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Git refs and compaction options for head synthesis.
#[derive(Debug, Clone)]
pub struct HeadSynthesisOptions {
    pub base_ref: String,
    pub head_ref: String,
    pub cascade_depth: usize,
}

/// Conventional scratch root for synthesized review/pr-check heads.
pub fn default_synth_head_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(GRAPH_DIR).join("synth-head")
}

/// True when `out_root` would overwrite the live session snapshot path.
pub fn out_root_clobbers_live_session(repo_root: &Path, out_root: &Path) -> bool {
    let live = MmappedGraphSnapshot::default_path(repo_root);
    let live_rgctl = repo_root.join(GRAPH_DIR);
    let out = out_root
        .canonicalize()
        .unwrap_or_else(|_| out_root.to_path_buf());
    let live_canon = live
        .canonicalize()
        .unwrap_or_else(|_| live.clone());
    let live_rgctl_canon = live_rgctl
        .canonicalize()
        .unwrap_or_else(|_| live_rgctl.clone());
    if out == live_canon {
        return true;
    }
    // Refuse writing the live `.rgctl` directory itself (not a synth-head child).
    if out == live_rgctl_canon {
        return true;
    }
    let out_snap = if out_root.extension().is_some_and(|e| e == "bin") {
        out_root.to_path_buf()
    } else {
        out_root.join("graph.snapshot.bin")
    };
    let out_snap_canon = out_snap
        .canonicalize()
        .unwrap_or_else(|_| out_snap);
    out_snap_canon == live_canon
}

fn prepare_out_root(repo_root: &Path, out_root: &Path) -> Result<PathBuf> {
    if out_root_clobbers_live_session(repo_root, out_root) {
        return Err(Error::Other(
            "refusing to synthesize head into the live session snapshot path; \
             use `.rgctl/synth-head/` (default) or another scratch directory"
                .into(),
        ));
    }
    if out_root.exists() {
        std::fs::remove_dir_all(out_root)?;
    }
    std::fs::create_dir_all(out_root)?;
    Ok(if out_root.extension().is_some_and(|e| e == "bin") {
        out_root.to_path_buf()
    } else {
        out_root.join("graph.snapshot.bin")
    })
}

/// Build a head columnar snapshot under `{repo}/.rgctl/synth-head/` from base + git delta.
pub fn synthesize_head_snapshot(
    base_artifact_root: &Path,
    repo_root: &Path,
    opts: &HeadSynthesisOptions,
    registry: Arc<LanguageRegistry>,
) -> Result<PathBuf> {
    synthesize_head_snapshot_at(
        base_artifact_root,
        repo_root,
        opts,
        registry,
        &default_synth_head_dir(repo_root),
    )
}

/// Build a head snapshot at `out_root` (directory) from base + git delta.
pub fn synthesize_head_snapshot_at(
    base_artifact_root: &Path,
    repo_root: &Path,
    opts: &HeadSynthesisOptions,
    registry: Arc<LanguageRegistry>,
    out_root: &Path,
) -> Result<PathBuf> {
    let out_snapshot = prepare_out_root(repo_root, out_root)?;
    let base_snapshot = resolve_snapshot_path(base_artifact_root);
    let changes = git_diff_name_status(repo_root, &opts.base_ref, &opts.head_ref)?;
    let updater = IncrementalUpdater::with_options(
        registry,
        UpdateOptions {
            cascade_depth: opts.cascade_depth,
            show_progress: false,
            ..Default::default()
        },
    );
    updater.apply_change_set_to_snapshot(
        &base_snapshot,
        &out_snapshot,
        out_root,
        repo_root,
        changes,
    )?;
    Ok(out_snapshot)
}

/// Compact the live HEAD snapshot with working-tree file changes into synth-head scratch.
pub fn synthesize_worktree_head_snapshot(
    repo_root: &Path,
    cascade_depth: usize,
    registry: Arc<LanguageRegistry>,
) -> Result<PathBuf> {
    synthesize_worktree_head_snapshot_at(
        repo_root,
        cascade_depth,
        registry,
        &default_synth_head_dir(repo_root),
    )
}

/// Worktree head synthesis writing under `out_root`.
pub fn synthesize_worktree_head_snapshot_at(
    repo_root: &Path,
    cascade_depth: usize,
    registry: Arc<LanguageRegistry>,
    out_root: &Path,
) -> Result<PathBuf> {
    let base_snapshot = MmappedGraphSnapshot::default_path(repo_root);
    if !base_snapshot.is_file() {
        return Err(Error::Other(
            "worktree head synthesis requires an existing HEAD snapshot at .rgctl/graph.snapshot.bin (run `rgctl discover` first)".into(),
        ));
    }
    let out_snapshot = prepare_out_root(repo_root, out_root)?;
    let changes = git_diff_worktree_vs_head(repo_root)?;
    let updater = IncrementalUpdater::with_options(
        registry,
        UpdateOptions {
            cascade_depth,
            show_progress: false,
            ..Default::default()
        },
    );
    updater.apply_change_set_to_snapshot(
        &base_snapshot,
        &out_snapshot,
        out_root,
        repo_root,
        changes,
    )?;
    Ok(out_snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_synth_head_is_under_rgctl() {
        let root = Path::new("/tmp/repo");
        assert_eq!(
            default_synth_head_dir(root),
            PathBuf::from("/tmp/repo/.rgctl/synth-head")
        );
    }

    #[test]
    fn refuse_live_session_out_root() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = tmp.path();
        let rgctl = repo.join(".rgctl");
        std::fs::create_dir_all(&rgctl).unwrap();
        let live = rgctl.join("graph.snapshot.bin");
        std::fs::write(&live, b"x").unwrap();
        assert!(out_root_clobbers_live_session(repo, &rgctl));
        assert!(out_root_clobbers_live_session(repo, &live));
        assert!(!out_root_clobbers_live_session(
            repo,
            &rgctl.join("synth-head")
        ));
    }
}
