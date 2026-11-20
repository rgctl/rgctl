//! Debounced filesystem watcher that triggers incremental graph updates.

use crate::languages::registry::LanguageRegistry;
use anyhow::{Context, Result};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use rgctl_project_config::RgctlConfig;
use rgctl_service::update_queue::{self, drain_queue, prune_old_results, queue_nonempty};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Minimum idle poll interval so queued CLI updates are noticed without FS events (D9).
/// Aligned with default `[watch].debounce_ms` (25) for sub-100ms watch latency (§7.5.4).
const MIN_QUEUE_POLL: Duration = Duration::from_millis(25);

/// Directory name segments that must never trigger an update.
const SKIP_DIR_NAMES: &[&str] = &[".git", "target", "node_modules", ".rgctl"];

/// Run `rgctl watch` in the foreground until the process is interrupted.
pub fn run_watch_foreground(repo: PathBuf) -> Result<()> {
    let repo = repo.canonicalize().unwrap_or(repo);
    let _watch_lock = super::pipeline_status::try_acquire_watch_lock(&repo)
        .with_context(|| format!("cannot start rgctl watch for {}", repo.display()))?;
    let stop = Arc::new(AtomicBool::new(false));
    let handle = spawn_repo_watcher_until(repo.clone(), Arc::clone(&stop))?;
    info!(
        "rgctl watch: monitoring {} (debounce from rgctl.toml [watch], default 25ms)",
        repo.display()
    );
    wait_for_shutdown_signal(&stop)?;
    let join_res = handle
        .join()
        .map_err(|_| anyhow::anyhow!("watch thread panicked"));
    drop(_watch_lock);
    join_res
}

fn wait_for_shutdown_signal(stop: &Arc<AtomicBool>) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("create signal runtime for rgctl watch")?;
    rt.block_on(async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            match signal(SignalKind::terminate()) {
                Ok(mut sigterm) => {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {}
                        _ = sigterm.recv() => {}
                    }
                }
                Err(_) => {
                    let _ = tokio::signal::ctrl_c().await;
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
    });
    stop.store(true, Ordering::SeqCst);
    Ok(())
}

/// Start a background watcher thread. Returns a join handle (detached by caller).
pub fn spawn_repo_watcher(repo: PathBuf) -> Result<thread::JoinHandle<()>> {
    spawn_repo_watcher_until(repo, Arc::new(AtomicBool::new(false)))
}

fn spawn_repo_watcher_until(
    repo: PathBuf,
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>> {
    let repo = repo.canonicalize().unwrap_or(repo);
    let debounce_ms = RgctlConfig::load(&repo)
        .unwrap_or_default()
        .watch
        .debounce_ms;
    let registry = LanguageRegistry::new();
    let extensions: HashSet<String> = registry
        .supported_extensions()
        .into_iter()
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
        .collect();

    match prune_old_results(&repo) {
        Ok(n) if n > 0 => info!("watch: pruned {n} stale update result file(s)"),
        Err(err) => warn!("watch: prune update results failed: {err:#}"),
        _ => {}
    }

    let (tx, rx) = mpsc::channel::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| match res {
        Ok(event) => {
            if !is_interesting_event(&event.kind) {
                return;
            }
            for path in event.paths {
                let _ = tx.send(path);
            }
        }
        Err(err) => warn!("rgctl watch: notify error: {err}"),
    })
    .context("create filesystem watcher")?;

    watcher
        .watch(&repo, RecursiveMode::Recursive)
        .with_context(|| format!("watch {}", repo.display()))?;

    // Keep watcher alive for the thread lifetime.
    let watcher = Arc::new(Mutex::new(watcher));
    let watcher_keep = Arc::clone(&watcher);

    let handle = thread::Builder::new()
        .name("rgctl-file-watch".into())
        .spawn(move || {
            let _keeper = watcher_keep;
            watch_loop(
                repo,
                rx,
                extensions,
                Duration::from_millis(debounce_ms),
                stop,
            );
        })
        .context("spawn watch thread")?;

    Ok(handle)
}

fn is_interesting_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

fn poll_interval(debounce: Duration) -> Duration {
    if debounce < MIN_QUEUE_POLL {
        MIN_QUEUE_POLL
    } else {
        debounce
    }
}

fn watch_loop(
    repo: PathBuf,
    rx: mpsc::Receiver<PathBuf>,
    extensions: HashSet<String>,
    debounce: Duration,
    stop: Arc<AtomicBool>,
) {
    let mut pending: HashSet<String> = HashSet::new();
    let mut deadline: Option<Instant> = None;
    let queue_poll = poll_interval(debounce);

    while !stop.load(Ordering::Relaxed) {
        let timeout = match deadline {
            Some(d) => {
                let until_debounce = d.saturating_duration_since(Instant::now());
                if until_debounce.is_zero() {
                    Duration::ZERO
                } else {
                    until_debounce.min(queue_poll)
                }
            }
            // Idle: wake periodically to drain the CLI update queue (D9 poll).
            None => queue_poll,
        };

        let recv = rx.recv_timeout(timeout);
        if stop.load(Ordering::Relaxed) {
            break;
        }

        match recv {
            Ok(path) => {
                if let Some(rel) = filter_watch_path(&repo, &path, &extensions) {
                    pending.insert(rel);
                    deadline = Some(Instant::now() + debounce);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                let debounce_fired = deadline.map(|d| Instant::now() >= d).unwrap_or(false);
                let has_queue = queue_nonempty(&repo);

                if !debounce_fired && !has_queue {
                    continue;
                }
                // Wait for debounce idle before applying FS pending, unless only queue work.
                if !pending.is_empty() && !debounce_fired && has_queue {
                    // Keep accumulating FS paths until debounce; still allow queue-only apply
                    // when pending empty. With pending + queue, wait for debounce.
                    continue;
                }

                apply_cycle(&repo, &mut pending);
                deadline = None;

                // Drain any events buffered during the update into the next debounce window.
                while let Ok(path) = rx.try_recv() {
                    if let Some(rel) = filter_watch_path(&repo, &path, &extensions) {
                        pending.insert(rel);
                    }
                }
                if !pending.is_empty() {
                    deadline = Some(Instant::now() + debounce);
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                debug!("watch: channel closed");
                break;
            }
        }
    }
}

fn apply_cycle(repo: &Path, pending: &mut HashSet<String>) {
    let batch = match drain_queue(repo) {
        Ok(b) => b,
        Err(err) => {
            warn!("watch: queue drain failed: {err:#}");
            update_queue::CoalescedBatch::default()
        }
    };

    let fs_batch: Vec<String> = pending.drain().collect();
    if batch.requests.is_empty() && fs_batch.is_empty() {
        return;
    }

    info!(
        "watch: applying update (fs_paths={}, queue_requests={}, hash_diff={})",
        fs_batch.len(),
        batch.requests.len(),
        batch.run_hash_diff
    );

    // Extract/compact on this dedicated watch thread (not the tokio serve runtime).
    match super::update::apply_queue_batch(repo, &batch, &fs_batch) {
        Ok(()) => info!("watch: incremental update cycle done"),
        Err(err) => warn!("watch: incremental update failed: {err:#}"),
    }
}

fn filter_watch_path(repo: &Path, path: &Path, extensions: &HashSet<String>) -> Option<String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    };
    let rel = abs.strip_prefix(repo).ok()?;
    if rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        SKIP_DIR_NAMES.iter().any(|skip| *skip == s.as_ref())
    }) {
        return None;
    }
    // Skip editor swap / temp noise
    if let Some(name) = rel.file_name().and_then(|n| n.to_str())
        && (name.starts_with('.') || name.ends_with('~') || name.ends_with(".swp"))
    {
        return None;
    }
    let ext = rel
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    if !extensions.contains(&ext) {
        return None;
    }
    Some(rel.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn filter_skips_target_and_non_source() {
        let repo = PathBuf::from("/tmp/repo");
        let mut exts = HashSet::new();
        exts.insert("rs".into());
        assert!(filter_watch_path(&repo, Path::new("/tmp/repo/target/foo.rs"), &exts).is_none());
        assert!(filter_watch_path(&repo, Path::new("/tmp/repo/README.md"), &exts).is_none());
        assert_eq!(
            filter_watch_path(&repo, Path::new("/tmp/repo/src/lib.rs"), &exts).as_deref(),
            Some("src/lib.rs")
        );
    }
}
