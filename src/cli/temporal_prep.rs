//! Shared base/head snapshot preparation for `review check` / `review paths` / `pr-check`.

use super::context::CliContext;
use crate::languages::registry::LanguageRegistry;
use anyhow::{Context, Result};
use rgctl_graph::code_graph::GRAPH_DIR;
use rgctl_graph::snapshot::MmappedGraphSnapshot;
use rgctl_incremental::{
    HeadSynthesisOptions, HunkIndex, PrCheckArtifactPaths, ScopedPaths, git_diff_worktree_vs_head,
    git_unified_diff, git_unified_diff_worktree, resolve_base_artifact_root,
    resolve_pr_check_artifacts, synthesize_head_snapshot, synthesize_worktree_head_snapshot,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntheticHeadMode {
    None,
    Worktree,
}

impl SyntheticHeadMode {
    pub fn parse(value: Option<&str>) -> Result<Self> {
        match value {
            None => Ok(Self::None),
            Some("worktree") => Ok(Self::Worktree),
            Some(other) => anyhow::bail!(
                "unsupported --synthetic-head value '{other}' (supported: worktree)"
            ),
        }
    }
}

/// Shared temporal artifact / ref selection (no policy fields).
#[derive(Clone, Debug)]
pub struct TemporalArtifactArgs {
    pub base_artifact: Option<String>,
    pub head_artifact: Option<String>,
    pub base_ref: String,
    pub head_ref: String,
    pub cascade_depth: usize,
    pub full_snapshots: bool,
}

const MISSING_BASE_HINT: &str = "hint: build a base snapshot on your base branch \
(`rgctl discover . && mkdir -p .rgctl-base && cp -R .rgctl .rgctl-base/.rgctl`), \
pass `--base-artifact <path>`, or use `--synthetic-head worktree` to compare \
uncommitted edits against `.rgctl/`";

pub fn prepare_artifacts(
    ctx: &CliContext,
    args: &TemporalArtifactArgs,
    synthetic_head: SyntheticHeadMode,
) -> Result<PrCheckArtifactPaths> {
    if synthetic_head == SyntheticHeadMode::Worktree {
        let registry: Arc<rgctl_registry::LanguageRegistry> = LanguageRegistry::new().into();
        let head_snapshot = synthesize_worktree_head_snapshot(&ctx.repo, args.cascade_depth, registry)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let base_snapshot = resolve_worktree_base_snapshot(ctx, args)?;
        return Ok(PrCheckArtifactPaths {
            base_snapshot,
            head_snapshot,
        });
    }

    let delta_head = !args.full_snapshots && args.head_artifact.is_none();
    if delta_head {
        warn_if_delta_reads_diverge_from_head_ref(ctx, args)?;
        let base_root = resolve_base_artifact_root(
            &ctx.repo,
            args.base_artifact.as_deref().map(Path::new),
        )
        .map_err(|e| anyhow::anyhow!("{e}\n  {MISSING_BASE_HINT}"))?;
        let registry: Arc<rgctl_registry::LanguageRegistry> = LanguageRegistry::new().into();
        let head_snapshot = synthesize_head_snapshot(
            &base_root,
            &ctx.repo,
            &HeadSynthesisOptions {
                base_ref: args.base_ref.clone(),
                head_ref: args.head_ref.clone(),
                cascade_depth: args.cascade_depth,
            },
            registry,
        )
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let base_snapshot = rgctl_incremental::resolve_base_snapshot(
            &ctx.repo,
            args.base_artifact.as_deref().map(Path::new),
        )
        .map_err(|e| anyhow::anyhow!("{e}\n  {MISSING_BASE_HINT}"))?;
        Ok(PrCheckArtifactPaths {
            base_snapshot,
            head_snapshot,
        })
    } else {
        resolve_pr_check_artifacts(
            &ctx.repo,
            args.base_artifact.as_deref().map(Path::new),
            args.head_artifact.as_deref().map(Path::new),
        )
        .map_err(|e| {
            anyhow::anyhow!(
                "{e}; prepare base/head graph snapshots (e.g. discover into `.rgctl-base` / `.rgctl`) \
                 or pass `--base-artifact` / `--head-artifact`, or omit `--full-snapshots` to synthesize a delta head"
            )
        })
    }
}

fn resolve_worktree_base_snapshot(
    ctx: &CliContext,
    args: &TemporalArtifactArgs,
) -> Result<PathBuf> {
    match rgctl_incremental::resolve_base_snapshot(
        &ctx.repo,
        args.base_artifact.as_deref().map(Path::new),
    ) {
        Ok(path) => Ok(path),
        Err(err) if args.base_artifact.is_none() => {
            let live = MmappedGraphSnapshot::default_path(&ctx.repo);
            if live.is_file() {
                Ok(live)
            } else {
                Err(anyhow::anyhow!(
                    "{err}\n  {MISSING_BASE_HINT}"
                ))
            }
        }
        Err(err) => Err(anyhow::anyhow!("{err}\n  {MISSING_BASE_HINT}")),
    }
}

/// Warn when non-worktree delta synthesis would read a worktree that may not match `--head-ref`.
fn warn_if_delta_reads_diverge_from_head_ref(
    ctx: &CliContext,
    args: &TemporalArtifactArgs,
) -> Result<()> {
    let head_sha = git_rev_parse(&ctx.repo, &args.head_ref)?;
    let tip_sha = git_rev_parse(&ctx.repo, "HEAD")?;
    let ref_mismatch = head_sha != tip_sha;

    let dirty = match ScopedPaths::from_git_refs(&ctx.repo, &args.base_ref, &args.head_ref) {
        Ok(scope) => scoped_paths_dirty(&ctx.repo, &scope)?,
        Err(_) => false,
    };

    if ref_mismatch || dirty {
        eprintln!(
            "warning: delta head synthesis reads working-tree files on disk, which differ from \
             `--head-ref` (or include dirty scoped files); pass `--synthetic-head worktree` or \
             check out the target ref"
        );
    }
    Ok(())
}

fn git_rev_parse(repo: &Path, rev: &str) -> Result<String> {
    let out = Command::new("git")
        .args(["-C", &repo.to_string_lossy(), "rev-parse", rev])
        .output()
        .with_context(|| format!("git rev-parse {rev}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git rev-parse {rev} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn scoped_paths_dirty(repo: &Path, scope: &ScopedPaths) -> Result<bool> {
    let paths: Vec<&str> = scope
        .files()
        .iter()
        .map(|p| p.as_ref())
        .filter(|p| !p.is_empty())
        .collect();
    if paths.is_empty() {
        return Ok(false);
    }
    let mut cmd = Command::new("git");
    cmd.args(["-C", &repo.to_string_lossy(), "status", "--porcelain", "--"]);
    cmd.args(&paths);
    let out = cmd.output().context("git status --porcelain")?;
    if !out.status.success() {
        return Ok(false);
    }
    Ok(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

pub fn resolve_scope_paths(
    ctx: &CliContext,
    args: &TemporalArtifactArgs,
    synthetic_head: SyntheticHeadMode,
) -> Result<ScopedPaths> {
    if synthetic_head == SyntheticHeadMode::Worktree {
        let change_set = git_diff_worktree_vs_head(&ctx.repo)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        return Ok(ScopedPaths::from_change_set(change_set));
    }
    ScopedPaths::from_git_refs(&ctx.repo, &args.base_ref, &args.head_ref)
        .with_context(|| "resolve git diff paths")
}

pub fn resolve_unified_diff(
    ctx: &CliContext,
    args: &TemporalArtifactArgs,
    synthetic_head: SyntheticHeadMode,
) -> Result<String> {
    if synthetic_head == SyntheticHeadMode::Worktree {
        return git_unified_diff_worktree(&ctx.repo).map_err(|e| anyhow::anyhow!(e.to_string()));
    }
    git_unified_diff(&ctx.repo, &args.base_ref, &args.head_ref)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

pub fn resolve_hunk_index(
    ctx: &CliContext,
    args: &TemporalArtifactArgs,
    synthetic_head: SyntheticHeadMode,
) -> Result<HunkIndex> {
    let unified = resolve_unified_diff(ctx, args, synthetic_head)?;
    Ok(HunkIndex::from_unified_diff(&unified))
}

/// Default session graph dir helper (unused by paths; kept for callers).
#[allow(dead_code)]
pub fn session_graph_dir(repo: &Path) -> std::path::PathBuf {
    repo.join(GRAPH_DIR)
}
