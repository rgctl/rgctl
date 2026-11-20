//! Full-pipeline status document and exclusive repo lock.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Status JSON schema version.
pub const PIPELINE_STATUS_SCHEMA_VERSION: u32 = 1;

/// Filename under `.rgctl/`.
pub const PIPELINE_STATUS_FILE: &str = "pipeline_status.json";

/// Exclusive lock filename under `.rgctl/`.
pub const PIPELINE_LOCK_FILE: &str = "pipeline.lock";

/// Exclusive lock for `serve --watch` / `rgctl update` (one writer per repo).
pub const WATCH_LOCK_FILE: &str = "watch.lock";

/// Marker: snapshot was indexed with field materialization.
pub const MATERIALIZED_FIELDS_DIGEST_FILE: &str = "materialized_fields.digest";

/// Marker: harmonic centrality was computed for a graph digest.
pub const HARMONIC_DIGEST_FILE: &str = "analysis/harmonic.digest";

/// Stage identifiers for the full pipeline plan.
pub const STAGE_BASIC: &str = "basic_discover";
/// CFG + dashboard + harmonic.
pub const STAGE_DEEP: &str = "deep_pass";
/// Semantic index (vocab default).
pub const STAGE_SEMANTIC: &str = "semantic_index";

/// Per-stage status in the plan array.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Running,
    Complete,
    Skipped,
    Failed,
}

/// One row in the execution plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineStagePlan {
    /// Stage id (`basic_discover`, `deep_pass`, `semantic_index`).
    pub id: String,
    /// Current status.
    pub status: StageStatus,
}

/// Shared pipeline status (`schema_version` 1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineStatus {
    pub schema_version: u32,
    pub command: String,
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_digest: Option<String>,
    pub dashboard_ready: bool,
    pub semantic_ready: bool,
    pub cfg_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub plan: Vec<PipelineStagePlan>,
}

/// Exclusive lock via pid file (works across processes and in-process).
pub struct PipelineLock {
    path: PathBuf,
}

impl Drop for PipelineLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Exclusive watch/update writer lock (pid file under `.rgctl/watch.lock`).
#[derive(Debug)]
pub struct WatchLock {
    path: PathBuf,
}

impl Drop for WatchLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Path of the status JSON for `repo`.
#[must_use]
pub fn status_path(repo: &Path) -> PathBuf {
    rgctl_graph::paths::artifact_path(repo, PIPELINE_STATUS_FILE)
}

/// Path of the lock file for `repo`.
#[must_use]
pub fn lock_path(repo: &Path) -> PathBuf {
    rgctl_graph::paths::artifact_path(repo, PIPELINE_LOCK_FILE)
}

/// Try to acquire an exclusive pipeline lock. Fails if another pipeline holds it.
pub fn try_acquire_lock(repo: &Path) -> Result<PipelineLock> {
    let dir = rgctl_graph::paths::artifact_dir(repo);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create artifact dir {}", dir.display()))?;
    let path = dir.join(PIPELINE_LOCK_FILE);
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            let _ = writeln!(file, "{}", std::process::id());
            Ok(PipelineLock { path })
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => bail!(
            "pipeline already running for {} (lock {})",
            repo.display(),
            path.display()
        ),
        Err(err) => Err(err).with_context(|| format!("create pipeline lock {}", path.display())),
    }
}

/// Path of the watch/update lock file for `repo`.
#[must_use]
pub fn watch_lock_path(repo: &Path) -> PathBuf {
    rgctl_graph::paths::artifact_path(repo, WATCH_LOCK_FILE)
}

/// Try to acquire the exclusive watcher lock (`rgctl watch` / `rgctl serve --watch`).
///
/// Only one watcher or concurrent `rgctl update` writer may hold `.rgctl/watch.lock`
/// per repository. Watcher locks record the bare PID (or `watch:<pid>`) so
/// [`detect_live_watcher`] routes `rgctl update` requests through the update queue.
///
/// If the lock file exists but its PID is not alive, the stale file is removed
/// and acquisition is retried once.
pub fn try_acquire_watch_lock(repo: &Path) -> Result<WatchLock> {
    try_acquire_writer_lock_inner(repo, true)
}

/// Try to acquire the exclusive writer lock for a one-shot `rgctl update`.
///
/// Records `update:<pid>` in `.rgctl/watch.lock` so concurrent writers are blocked
/// while [`detect_live_watcher`] knows no queue-draining watcher daemon is active.
pub fn try_acquire_update_writer_lock(repo: &Path) -> Result<WatchLock> {
    try_acquire_writer_lock_inner(repo, false)
}

fn try_acquire_writer_lock_inner(repo: &Path, is_watcher: bool) -> Result<WatchLock> {
    let dir = rgctl_graph::paths::artifact_dir(repo);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create artifact dir {}", dir.display()))?;
    let path = dir.join(WATCH_LOCK_FILE);
    match try_create_watch_lock(&path, is_watcher) {
        Ok(lock) => Ok(lock),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            if reclaim_stale_watch_lock_at(&path) {
                match try_create_watch_lock(&path, is_watcher) {
                    Ok(lock) => return Ok(lock),
                    Err(err2) if err2.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(err2) => {
                        return Err(err2)
                            .with_context(|| format!("create watch lock {}", path.display()));
                    }
                }
            }
            let raw_holder = std::fs::read_to_string(&path).unwrap_or_default();
            let holder_note = match parse_watch_lock_text(&raw_holder) {
                Some((pid, true)) => format!(" (held by watcher pid {pid})"),
                Some((pid, false)) => format!(" (held by update pid {pid})"),
                None => {
                    let trimmed = raw_holder.trim();
                    if trimmed.is_empty() {
                        String::new()
                    } else {
                        format!(" (held by pid {trimmed})")
                    }
                }
            };
            bail!(
                "another rgctl watch/update already active for {}{holder_note}\n\
                 lock: {}\n\
                 Only one watcher (or update writer) is allowed per repo. \
                 Stop the other `rgctl watch` / `rgctl serve --watch`, wait for `rgctl update` to finish, \
                 or omit --watch.",
                repo.display(),
                path.display()
            )
        }
        Err(err) => Err(err).with_context(|| format!("create watch lock {}", path.display())),
    }
}

fn try_create_watch_lock(path: &Path, is_watcher: bool) -> std::io::Result<WatchLock> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let pid = std::process::id();
    if is_watcher {
        let _ = writeln!(file, "{pid}");
    } else {
        let _ = writeln!(file, "update:{pid}");
    }
    Ok(WatchLock {
        path: path.to_path_buf(),
    })
}

fn parse_watch_lock_text(text: &str) -> Option<(u32, bool)> {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("update:") {
        return rest.trim().parse::<u32>().ok().map(|pid| (pid, false));
    }
    if let Some(rest) = trimmed.strip_prefix("watch:") {
        return rest.trim().parse::<u32>().ok().map(|pid| (pid, true));
    }
    trimmed.parse::<u32>().ok().map(|pid| (pid, true))
}

fn reclaim_stale_watch_lock_at(path: &Path) -> bool {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return std::fs::remove_file(path).is_ok();
    }
    let Some((pid, _)) = parse_watch_lock_text(trimmed) else {
        return false;
    };
    if pid == std::process::id() || process_alive(pid) {
        return false;
    }
    std::fs::remove_file(path).is_ok()
}

/// Parsed watch.lock holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveWatcherInfo {
    /// Process id written into the lock file.
    pub pid: u32,
}

/// Read the watcher pid from `watch.lock` if held by a watcher (`rgctl watch` / `serve --watch`).
///
/// One-shot `rgctl update` writer locks (`update:<pid>`) return `None` because
/// they do not drain the update queue.
#[must_use]
pub fn read_watch_lock_pid(repo: &Path) -> Option<LiveWatcherInfo> {
    let path = watch_lock_path(repo);
    let text = std::fs::read_to_string(path).ok()?;
    let (pid, is_watcher) = parse_watch_lock_text(&text)?;
    is_watcher.then_some(LiveWatcherInfo { pid })
}

/// True if `pid` appears to refer to a running process.
#[must_use]
pub fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    use sysinfo::{Pid, System};
    let mut sys = System::new();
    let pid = Pid::from_u32(pid);
    sys.refresh_process(pid);
    sys.process(pid).is_some()
}

/// Detect a live external watcher holding `watch.lock`.
///
/// - Missing lock or one-shot `update:<pid>` lock → `None`
/// - Lock held by **this** process → `None` (not an external watcher)
/// - Watcher lock pid alive → `Some(info)`
/// - Lock pid dead → remove stale lock, return `None`
pub fn detect_live_watcher(repo: &Path) -> Option<LiveWatcherInfo> {
    let path = watch_lock_path(repo);
    let text = std::fs::read_to_string(&path).ok()?;
    let (pid, is_watcher) = parse_watch_lock_text(&text)?;
    if pid == std::process::id() {
        return None;
    }
    if !process_alive(pid) {
        let _ = std::fs::remove_file(path);
        return None;
    }
    is_watcher.then_some(LiveWatcherInfo { pid })
}

/// Reclaim a stale watch/update lock (pid dead). Returns true if a lock file was removed.
pub fn reclaim_stale_watch_lock(repo: &Path) -> bool {
    reclaim_stale_watch_lock_at(&watch_lock_path(repo))
}

/// Initial plan with all stages pending.
#[must_use]
pub fn pending_plan() -> Vec<PipelineStagePlan> {
    vec![
        PipelineStagePlan {
            id: STAGE_BASIC.to_string(),
            status: StageStatus::Pending,
        },
        PipelineStagePlan {
            id: STAGE_DEEP.to_string(),
            status: StageStatus::Pending,
        },
        PipelineStagePlan {
            id: STAGE_SEMANTIC.to_string(),
            status: StageStatus::Pending,
        },
    ]
}

/// Empty/default status before a session writes the file.
#[must_use]
pub fn default_status(repo: &Path) -> PipelineStatus {
    PipelineStatus {
        schema_version: PIPELINE_STATUS_SCHEMA_VERSION,
        command: "pipeline_status".into(),
        repo: repo.display().to_string(),
        mode: Some("full".into()),
        phase: Some("pending".into()),
        graph_digest: None,
        dashboard_ready: false,
        semantic_ready: false,
        cfg_ready: false,
        message: Some("Pipeline has not started".into()),
        plan: pending_plan(),
    }
}

/// Fixture for schema tests.
#[must_use]
pub fn fixture_pipeline_status() -> PipelineStatus {
    let mut status = default_status(Path::new("/tmp/example"));
    status.phase = Some(STAGE_DEEP.into());
    status.message = Some("Dashboard is being prepared".into());
    status.graph_digest = Some("abc123".into());
    status.plan[0].status = StageStatus::Complete;
    status.plan[1].status = StageStatus::Running;
    status
}

/// JSON value of [`fixture_pipeline_status`].
#[must_use]
pub fn fixture_pipeline_status_json() -> Value {
    serde_json::to_value(fixture_pipeline_status()).expect("PipelineStatus serializes")
}

/// Read status from disk, or [`default_status`] if missing.
pub fn read_status(repo: &Path) -> PipelineStatus {
    let path = status_path(repo);
    let Ok(bytes) = std::fs::read(&path) else {
        return default_status(repo);
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|_| default_status(repo))
}

/// Atomically write the status document.
pub fn write_status(repo: &Path, status: &PipelineStatus) -> Result<()> {
    let dir = rgctl_graph::paths::artifact_dir(repo);
    std::fs::create_dir_all(&dir)?;
    let path = status_path(repo);
    let tmp = dir.join("pipeline_status.json.tmp");
    let json = serde_json::to_vec_pretty(status)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("write pipeline status {}", path.display()))?;
    Ok(())
}

/// Set one stage's status and persist.
pub fn set_stage(repo: &Path, stage_id: &str, stage_status: StageStatus) -> Result<PipelineStatus> {
    let mut status = read_status(repo);
    if let Some(row) = status.plan.iter_mut().find(|s| s.id == stage_id) {
        row.status = stage_status;
    }
    status.phase = Some(stage_id.to_string());
    status.message = Some(message_for(&status));
    refresh_ready_flags(&mut status, repo);
    write_status(repo, &status)?;
    Ok(status)
}

/// Refresh ready flags from artifacts on disk.
pub fn refresh_ready_flags(status: &mut PipelineStatus, repo: &Path) {
    let dash = rgctl_graph::paths::artifact_path(repo, "dashboard/index.html");
    status.dashboard_ready = dash.is_file();
    status.cfg_ready = rgctl_analysis::CfgPdgArchive::default_path(repo).is_file();
    status.semantic_ready = rgctl_analysis::SemanticIndex::default_path(repo).is_file();
}

fn message_for(status: &PipelineStatus) -> String {
    if status.dashboard_ready && status.semantic_ready && status.cfg_ready {
        return "Full pipeline complete".into();
    }
    if !status.dashboard_ready {
        return "Dashboard is being prepared".into();
    }
    if !status.semantic_ready {
        return "Semantic index is being prepared".into();
    }
    "Pipeline running".into()
}

/// Write a one-line digest marker.
pub fn write_digest_marker(repo: &Path, relative: &str, digest: &str) -> Result<()> {
    let path = rgctl_graph::paths::artifact_path(repo, relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, digest.as_bytes())
        .with_context(|| format!("write marker {}", path.display()))?;
    Ok(())
}

/// Read a digest marker if present.
#[must_use]
pub fn read_digest_marker(repo: &Path, relative: &str) -> Option<String> {
    let path = rgctl_graph::paths::artifact_path(repo, relative);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_schema_sanity() {
        let doc = fixture_pipeline_status_json();
        assert_eq!(doc["schema_version"], 1);
        assert_eq!(doc["command"], "pipeline_status");
        assert!(doc["plan"].as_array().is_some_and(|p| p.len() == 3));
        assert_eq!(doc["plan"][0]["id"], STAGE_BASIC);
        assert_eq!(doc["plan"][1]["id"], STAGE_DEEP);
        assert_eq!(doc["plan"][2]["id"], STAGE_SEMANTIC);
        assert!(doc["dashboard_ready"].is_boolean());
        assert!(doc["cfg_ready"].is_boolean());
        assert!(doc["semantic_ready"].is_boolean());
    }

    #[test]
    fn roundtrip_status_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        let mut status = default_status(repo);
        status.plan[0].status = StageStatus::Complete;
        write_status(repo, &status).expect("write");
        let loaded = read_status(repo);
        assert_eq!(loaded.plan[0].status, StageStatus::Complete);
        assert_eq!(loaded.schema_version, 1);
    }

    #[test]
    fn exclusive_lock_rejects_second() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        let _first = try_acquire_lock(repo).expect("first lock");
        let second = try_acquire_lock(repo);
        assert!(second.is_err(), "second lock should fail");
    }

    #[test]
    fn exclusive_watch_lock_rejects_second() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        let _first = try_acquire_watch_lock(repo).expect("first watch lock");
        let second = try_acquire_watch_lock(repo);
        assert!(second.is_err(), "second watch lock should fail");
        let msg = format!("{:#}", second.unwrap_err());
        assert!(
            msg.contains("only one watcher") || msg.contains("Only one watcher"),
            "expected friendly message, got: {msg}"
        );
    }

    #[test]
    fn detect_live_watcher_same_process_is_not_external() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        let _lock = try_acquire_watch_lock(repo).expect("lock");
        // Same process holds the lock → not an *external* live watcher.
        assert!(detect_live_watcher(repo).is_none());
    }

    #[test]
    fn stale_watch_lock_reclaimed_on_acquire() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        let path = watch_lock_path(repo);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let dead_pid = 4_294_967_294u32; // u32::MAX - 1
        if process_alive(dead_pid) {
            return; // skip on exotic hosts
        }
        std::fs::write(&path, format!("{dead_pid}\n")).unwrap();
        let _lock = try_acquire_watch_lock(repo).expect("reclaim stale");
    }

    #[test]
    fn update_writer_lock_blocks_writers_without_advertising_watcher() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        let path = watch_lock_path(repo);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");

        // Simulate an external one-shot `rgctl update` process (pid 1).
        let external_pid = 1u32;
        if process_alive(external_pid) && external_pid != std::process::id() {
            std::fs::write(&path, format!("update:{external_pid}\n")).expect("write lock");
            assert!(
                detect_live_watcher(repo).is_none(),
                "one-shot update lock must not advertise a live queue-draining watcher"
            );
            let err = try_acquire_update_writer_lock(repo).expect_err("concurrent update blocked");
            assert!(format!("{err:#}").contains("held by update pid"));
            let err2 = try_acquire_watch_lock(repo).expect_err("concurrent watch blocked");
            assert!(format!("{err2:#}").contains("held by update pid"));
            std::fs::remove_file(&path).expect("cleanup");
        }

        let dead_pid = 4_294_967_294u32;
        if !process_alive(dead_pid) {
            std::fs::write(&path, format!("update:{dead_pid}\n")).expect("write dead lock");
            let _lock = try_acquire_update_writer_lock(repo).expect("reclaim stale update lock");
        }
    }
}
