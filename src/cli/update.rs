//! `rgctl update` — incremental structural graph patch (not full discover).

use super::OutputFormat;
use super::context::CliContext;
use crate::discovery::DiscoveryConfig;
use crate::languages::registry::LanguageRegistry;
use anyhow::{Context, Result, bail};
use rgctl_graph::snapshot::MmappedGraphSnapshot;
use rgctl_incremental::{IncrementalUpdater, UpdateOptions, UpdateResult};
use rgctl_service::status::detect_live_watcher;
use rgctl_service::update_queue::{
    self, UpdateQueueMode, UpdateQueueRequest, UpdateQueueResult, build_request, delete_result,
    enqueue, wait_for_result,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const UPDATE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 60;

/// Arguments for `rgctl update`.
#[derive(Debug, Clone)]
pub struct UpdateArgs {
    /// Optional session path (defaults to `--repo` / cwd).
    pub path: Option<String>,
    /// Explicit repo-relative paths (CSV via clap).
    pub files: Option<Vec<String>>,
    /// Git ref for `git diff --name-only` change set.
    pub since: Option<String>,
    /// Force full rebuild via updater.
    pub force: bool,
    /// Reverse CALLS cascade hops.
    pub cascade_depth: usize,
    pub languages: Option<String>,
    pub exclude: Option<String>,
    /// After enqueue to a live watcher, return immediately.
    pub no_wait: bool,
    /// Seconds to wait for a queue result (default 60).
    pub wait_timeout_secs: u64,
}

impl Default for UpdateArgs {
    fn default() -> Self {
        Self {
            path: None,
            files: None,
            since: None,
            force: false,
            cascade_depth: 1,
            languages: None,
            exclude: None,
            no_wait: false,
            wait_timeout_secs: default_wait_timeout_secs(),
        }
    }
}

fn default_wait_timeout_secs() -> u64 {
    std::env::var("RGCTL_UPDATE_WAIT_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_WAIT_TIMEOUT_SECS)
}

#[derive(Debug, Serialize)]
struct UpdateJson {
    schema_version: u32,
    command: &'static str,
    files_added: usize,
    files_changed: usize,
    files_deleted: usize,
    files_affected: usize,
    nodes_added: usize,
    nodes_removed: usize,
    edges_added: usize,
    edges_removed: usize,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct QueuedJson {
    schema_version: u32,
    command: &'static str,
    queued: bool,
    request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// Run incremental update for the session repo.
pub fn run(ctx: &CliContext, args: UpdateArgs) -> Result<()> {
    let root = PathBuf::from(super::discover::resolve_session_root(
        ctx,
        args.path.as_deref(),
    ));

    if let Some(watcher) = detect_live_watcher(&root) {
        return run_queued(ctx, &root, &args, watcher.pid);
    }

    let result = run_update_at(&root, &args, ctx.format != OutputFormat::Json, true)?;
    emit_result(ctx, &result, None, None)?;
    Ok(())
}

fn run_queued(ctx: &CliContext, root: &Path, args: &UpdateArgs, watcher_pid: u32) -> Result<()> {
    if args.force {
        bail!(
            "rgctl update --force cannot run while rgctl watch / serve --watch is active (pid {watcher_pid}).\n\
             Stop the watcher, or run a full `rgctl discover` in another session after stopping watch."
        );
    }

    let request = request_from_args(root, args)?;
    let request_id = request.request_id.clone();
    enqueue(root, &request).context("enqueue update for active watcher")?;

    if args.no_wait {
        if ctx.format == OutputFormat::Json {
            let payload = QueuedJson {
                schema_version: UPDATE_SCHEMA_VERSION,
                command: "update",
                queued: true,
                request_id: request_id.clone(),
                message: Some(format!(
                    "enqueued for active watcher (pid {watcher_pid}); not waiting for result"
                )),
            };
            ctx.emit_json_value(&serde_json::to_value(&payload)?)?;
        } else {
            println!("Queued update request {request_id} for active watcher (pid {watcher_pid})");
        }
        return Ok(());
    }

    let timeout = Duration::from_secs(args.wait_timeout_secs);
    let queue_result = wait_for_result(root, &request_id, timeout)?;
    let _ = delete_result(root, &request_id);

    if !queue_result.ok {
        let err = queue_result
            .error
            .unwrap_or_else(|| "watch queue update failed".into());
        bail!("{err}");
    }

    emit_queue_result(ctx, &queue_result)?;
    Ok(())
}

fn request_from_args(root: &Path, args: &UpdateArgs) -> Result<UpdateQueueRequest> {
    let (mode, paths, since) = if let Some(files) = &args.files {
        if files.is_empty() {
            bail!("--files requires at least one path");
        }
        let mut normalized = Vec::with_capacity(files.len());
        for f in files {
            normalized.push(
                update_queue::normalize_repo_path(root, f)
                    .with_context(|| format!("invalid --files path '{f}'"))?,
            );
        }
        (UpdateQueueMode::Paths, Some(normalized), None)
    } else if let Some(since) = &args.since {
        (UpdateQueueMode::Since, None, Some(since.clone()))
    } else {
        (UpdateQueueMode::HashDiff, None, None)
    };

    build_request(
        mode,
        paths,
        since,
        Some(args.cascade_depth),
        args.languages.clone(),
        args.exclude.clone(),
    )
}

/// Shared entry used by CLI, `rgctl watch`, and `serve --watch`.
///
/// When `acquire_lock` is true (CLI `rgctl update` with no live watcher), takes
/// `.rgctl/watch.lock` with `update:<pid>` for the duration of the in-process compact.
/// The watch loop passes `false` because `rgctl watch` / `serve --watch` already
/// holds the watcher lock for the process lifetime.
pub fn run_update_at(
    root: &Path,
    args: &UpdateArgs,
    show_progress: bool,
    acquire_lock: bool,
) -> Result<UpdateResult> {
    let _lock = if acquire_lock {
        Some(
            super::pipeline_status::try_acquire_update_writer_lock(root)
                .context("cannot run rgctl update while another watch/update holds the lock")?,
        )
    } else {
        None
    };

    let snapshot = MmappedGraphSnapshot::default_path(root);
    if !snapshot.is_file() {
        anyhow::bail!(
            "no graph snapshot at {}; run `rgctl discover` first",
            snapshot.display()
        );
    }

    let discovery = discovery_from_args(args);
    let registry: Arc<rgctl_registry::LanguageRegistry> = LanguageRegistry::new().into();

    let updater = IncrementalUpdater::with_options(
        registry,
        UpdateOptions {
            since: args.since.clone(),
            force: args.force,
            discovery,
            show_progress,
            cascade_depth: args.cascade_depth,
            ..Default::default()
        },
    );

    if let Some(files) = &args.files {
        updater
            .update_repo_files(root, files)
            .with_context(|| "incremental file update")
    } else {
        updater
            .update_repo(root)
            .with_context(|| "incremental graph update")
    }
}

/// Incremental update for an explicit path list (watch path; lock already held by serve).
pub fn update_paths(
    root: &Path,
    relative_paths: &[String],
    cascade_depth: usize,
) -> Result<UpdateResult> {
    let args = UpdateArgs {
        files: Some(relative_paths.to_vec()),
        cascade_depth,
        ..UpdateArgs::default()
    };
    run_update_at(root, &args, false, false)
}

fn discovery_from_args(args: &UpdateArgs) -> DiscoveryConfig {
    let mut discovery = DiscoveryConfig::default();
    if let Some(langs) = &args.languages {
        discovery.languages = Some(
            langs
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        );
    }
    if let Some(excludes) = &args.exclude {
        discovery.exclude_patterns = excludes
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    discovery
}

fn emit_result(
    ctx: &CliContext,
    result: &UpdateResult,
    source: Option<&'static str>,
    warnings: Option<Vec<String>>,
) -> Result<()> {
    let affected = result.files_affected();
    let message = if affected == 0 {
        Some("already current".to_string())
    } else {
        None
    };

    if ctx.format == OutputFormat::Json {
        let payload = UpdateJson {
            schema_version: UPDATE_SCHEMA_VERSION,
            command: "update",
            files_added: result.files_added,
            files_changed: result.files_changed,
            files_deleted: result.files_deleted,
            files_affected: affected,
            nodes_added: result.nodes_added,
            nodes_removed: result.nodes_removed,
            edges_added: result.edges_added,
            edges_removed: result.edges_removed,
            duration_ms: duration_ms(result.duration),
            message,
            source,
            warnings,
        };
        ctx.emit_json_value(&serde_json::to_value(&payload)?)?;
    } else if affected == 0 {
        println!("Graph already current (no files changed)");
    } else {
        let src = source.map(|s| format!(" [{s}]")).unwrap_or_default();
        println!(
            "Updated {} files (+{}/-{} nodes, +{}/-{} edges) in {:.2}s{src}",
            affected,
            result.nodes_added,
            result.nodes_removed,
            result.edges_added,
            result.edges_removed,
            result.duration.as_secs_f64()
        );
    }
    Ok(())
}

fn emit_queue_result(ctx: &CliContext, result: &UpdateQueueResult) -> Result<()> {
    let warnings = if result.warnings.is_empty() {
        None
    } else {
        Some(result.warnings.clone())
    };
    if ctx.format == OutputFormat::Json {
        let payload = UpdateJson {
            schema_version: UPDATE_SCHEMA_VERSION,
            command: "update",
            files_added: result.files_added,
            files_changed: result.files_changed,
            files_deleted: result.files_deleted,
            files_affected: result.files_affected,
            nodes_added: result.nodes_added,
            nodes_removed: result.nodes_removed,
            edges_added: result.edges_added,
            edges_removed: result.edges_removed,
            duration_ms: result.duration_ms,
            message: result.message.clone().or_else(|| {
                if result.files_affected == 0 {
                    Some("already current".into())
                } else {
                    None
                }
            }),
            source: Some("watch_queue"),
            warnings,
        };
        ctx.emit_json_value(&serde_json::to_value(&payload)?)?;
    } else if result.files_affected == 0 {
        println!("Graph already current (no files changed) [watch_queue]");
    } else {
        println!(
            "Updated {} files (+{}/-{} nodes, +{}/-{} edges) in {:.2}s [watch_queue]",
            result.files_affected,
            result.nodes_added,
            result.nodes_removed,
            result.edges_added,
            result.edges_removed,
            result.duration_ms as f64 / 1000.0
        );
    }
    Ok(())
}

fn duration_ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Apply a coalesced queue batch on the watch thread (sole writer).
pub fn apply_queue_batch(
    root: &Path,
    batch: &update_queue::CoalescedBatch,
    pending_fs: &[String],
) -> Result<()> {
    use update_queue::{err_result, ok_result, write_result};

    if batch.requests.is_empty() && pending_fs.is_empty() {
        return Ok(());
    }

    let mut warnings = batch.warnings.clone();
    warnings.extend(batch.drain_warnings.iter().cloned());

    let cascade = batch.cascade_depth.unwrap_or(1);
    let mut path_union: Vec<String> = pending_fs.to_vec();
    for p in &batch.path_set {
        if !path_union.iter().any(|x| x == p) {
            path_union.push(p.clone());
        }
    }

    let apply_start = std::time::Instant::now();
    let apply = if batch.run_hash_diff {
        let args = UpdateArgs {
            files: None,
            since: None,
            cascade_depth: cascade,
            languages: batch.languages.clone(),
            exclude: batch.exclude.clone(),
            ..UpdateArgs::default()
        };
        run_update_at(root, &args, false, false)
    } else if let Some(since) = &batch.since {
        // since-only (or since + paths): run since update; paths already unioned into
        // hash-diff/paths path above when run_hash_diff. For since, IncrementalUpdater
        // computes the change set from git.
        let args = UpdateArgs {
            files: None,
            since: Some(since.clone()),
            cascade_depth: cascade,
            languages: batch.languages.clone(),
            exclude: batch.exclude.clone(),
            ..UpdateArgs::default()
        };
        run_update_at(root, &args, false, false)
    } else if !path_union.is_empty() {
        let args = UpdateArgs {
            files: Some(path_union),
            since: None,
            cascade_depth: cascade,
            languages: batch.languages.clone(),
            exclude: batch.exclude.clone(),
            ..UpdateArgs::default()
        };
        run_update_at(root, &args, false, false)
    } else if !batch.requests.is_empty() {
        // Empty / noop requests (e.g. all paths rejected).
        for req in &batch.requests {
            let mut w = warnings.clone();
            if req.mode == UpdateQueueMode::Paths {
                w.push("no valid paths to update".into());
            }
            write_result(
                root,
                &err_result(&req.request_id, "no work to apply for request", w),
            )?;
        }
        return Ok(());
    } else {
        return Ok(());
    };

    match apply {
        Ok(result) => {
            let duration_ms = duration_ms(result.duration.max(apply_start.elapsed()));
            let message = if result.files_affected() == 0 {
                Some("already current".into())
            } else {
                None
            };
            for req in &batch.requests {
                write_result(
                    root,
                    &ok_result(
                        &req.request_id,
                        result.files_added,
                        result.files_changed,
                        result.files_deleted,
                        result.nodes_added,
                        result.nodes_removed,
                        result.edges_added,
                        result.edges_removed,
                        duration_ms,
                        warnings.clone(),
                        message.clone(),
                    ),
                )?;
            }
            Ok(())
        }
        Err(err) => {
            let msg = format!("{err:#}");
            for req in &batch.requests {
                write_result(
                    root,
                    &err_result(&req.request_id, msg.clone(), warnings.clone()),
                )?;
            }
            Err(err)
        }
    }
}
