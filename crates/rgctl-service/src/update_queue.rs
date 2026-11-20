//! File-backed update queue between CLI `rgctl update` and `serve --watch`.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Queue protocol schema version.
pub const UPDATE_QUEUE_SCHEMA_VERSION: u32 = 1;

/// Filename under `.rgctl/`.
pub const UPDATE_QUEUE_FILE: &str = "update_queue.jsonl";

/// Temporary name while the watcher drains the queue.
pub const UPDATE_QUEUE_PROCESSING_FILE: &str = "update_queue.jsonl.processing";

/// Advisory OS lock file serializing `enqueue` against `drain_queue` so an appending
/// CLI process can never write into an unlinked `.processing` inode.
const UPDATE_QUEUE_LOCK_FILE: &str = "update_queue.lock";

/// Directory under `.rgctl/` for per-request results.
pub const UPDATE_RESULTS_DIR: &str = "update_results";

/// Max queue file size before enqueue fails (1 MiB).
pub const MAX_QUEUE_BYTES: u64 = 1024 * 1024;

/// Max lines in the queue before enqueue fails.
pub const MAX_QUEUE_LINES: usize = 1000;

/// Prune result files older than this on watch start.
pub const RESULT_MAX_AGE: Duration = Duration::from_secs(60 * 60);

/// Request mode for a queued update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateQueueMode {
    Paths,
    HashDiff,
    Since,
}

/// One line in `update_queue.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateQueueRequest {
    pub schema_version: u32,
    pub request_id: String,
    pub pid: u32,
    pub ts_ms: u64,
    pub mode: UpdateQueueMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cascade_depth: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub languages: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<String>,
}

/// Result written under `.rgctl/update_results/<id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateQueueResult {
    pub schema_version: u32,
    pub request_id: String,
    pub ok: bool,
    pub command: String,
    pub source: String,
    pub files_added: usize,
    pub files_changed: usize,
    pub files_deleted: usize,
    pub files_affected: usize,
    pub nodes_added: usize,
    pub nodes_removed: usize,
    pub edges_added: usize,
    pub edges_removed: usize,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Coalesced plan from a drained batch of requests.
#[derive(Debug, Clone, Default)]
pub struct CoalescedBatch {
    /// Valid request ids that should receive a result (including skipped-malformed? no — only valid).
    pub requests: Vec<UpdateQueueRequest>,
    pub path_set: Vec<String>,
    pub run_hash_diff: bool,
    pub since: Option<String>,
    pub cascade_depth: Option<usize>,
    pub languages: Option<String>,
    pub exclude: Option<String>,
    pub warnings: Vec<String>,
    /// Drain-time warnings (malformed lines, etc.).
    pub drain_warnings: Vec<String>,
}

/// Path to the queue JSONL for `repo`.
#[must_use]
pub fn queue_path(repo: &Path) -> PathBuf {
    rgctl_graph::paths::artifact_path(repo, UPDATE_QUEUE_FILE)
}

/// Path to the processing file for `repo`.
#[must_use]
pub fn queue_processing_path(repo: &Path) -> PathBuf {
    rgctl_graph::paths::artifact_path(repo, UPDATE_QUEUE_PROCESSING_FILE)
}

/// Directory for result files.
#[must_use]
pub fn results_dir(repo: &Path) -> PathBuf {
    rgctl_graph::paths::artifact_path(repo, UPDATE_RESULTS_DIR)
}

/// Path for a single request result.
#[must_use]
pub fn result_path(repo: &Path, request_id: &str) -> PathBuf {
    results_dir(repo).join(format!("{request_id}.json"))
}

/// Current wall-clock milliseconds since Unix epoch.
#[must_use]
pub fn now_ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Build a validated request for the given mode.
pub fn build_request(
    mode: UpdateQueueMode,
    paths: Option<Vec<String>>,
    since: Option<String>,
    cascade_depth: Option<usize>,
    languages: Option<String>,
    exclude: Option<String>,
) -> Result<UpdateQueueRequest> {
    let mut req = UpdateQueueRequest {
        schema_version: UPDATE_QUEUE_SCHEMA_VERSION,
        request_id: Uuid::new_v4().to_string(),
        pid: std::process::id(),
        ts_ms: now_ts_ms(),
        mode,
        paths,
        since,
        cascade_depth,
        languages,
        exclude,
    };
    validate_request(&mut req)?;
    Ok(req)
}

/// Validate a request (schema + mode-specific fields). Mutates nothing except via `&mut` for future hooks.
pub fn validate_request(req: &mut UpdateQueueRequest) -> Result<()> {
    if req.schema_version != UPDATE_QUEUE_SCHEMA_VERSION {
        bail!(
            "unsupported update queue schema_version {} (expected {})",
            req.schema_version,
            UPDATE_QUEUE_SCHEMA_VERSION
        );
    }
    if req.request_id.trim().is_empty() {
        bail!("update queue request_id must be non-empty");
    }
    match req.mode {
        UpdateQueueMode::Paths => {
            let paths = req.paths.as_ref().filter(|p| !p.is_empty());
            if paths.is_none() {
                bail!("mode paths requires a non-empty paths array");
            }
        }
        UpdateQueueMode::Since => {
            let since = req
                .since
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if since.is_none() {
                bail!("mode since requires a non-empty since ref");
            }
        }
        UpdateQueueMode::HashDiff => {}
    }
    Ok(())
}

/// Normalize a repo-relative path and reject escapes outside `repo`.
///
/// Returns the normalized forward-slash relative path, or an error if the path
/// would leave the repository root.
pub fn normalize_repo_path(repo: &Path, path: &str) -> Result<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        bail!("empty path");
    }
    let candidate = Path::new(trimmed);
    if candidate.is_absolute() {
        let canon_repo = fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
        let joined = if candidate.exists() {
            fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf())
        } else {
            candidate.to_path_buf()
        };
        let rel = joined
            .strip_prefix(&canon_repo)
            .map_err(|_| anyhow::anyhow!("path escapes repository root: {trimmed}"))?;
        return Ok(rel.to_string_lossy().replace('\\', "/"));
    }

    let mut out = PathBuf::new();
    for comp in candidate.components() {
        match comp {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    bail!("path escapes repository root: {trimmed}");
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!("path escapes repository root: {trimmed}");
            }
        }
    }
    if out.as_os_str().is_empty() {
        bail!("path resolves to repository root: {trimmed}");
    }
    Ok(out.to_string_lossy().replace('\\', "/"))
}

/// Count non-empty lines in a file (best-effort).
fn count_lines(path: &Path) -> Result<usize> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(bytes
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .count())
}

fn lock_queue_file(dir: &Path) -> Result<fs::File> {
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let lock_path = dir.join(UPDATE_QUEUE_LOCK_FILE);
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("open queue lock {}", lock_path.display()))?;
    lock_file
        .lock()
        .with_context(|| format!("lock {}", lock_path.display()))?;
    Ok(lock_file)
}

/// Append one request line to the queue. Fails if size/line limits would be exceeded.
pub fn enqueue(repo: &Path, request: &UpdateQueueRequest) -> Result<()> {
    let mut validated = request.clone();
    validate_request(&mut validated)?;

    let dir = rgctl_graph::paths::artifact_dir(repo);
    let _queue_lock = lock_queue_file(&dir)?;
    let path = dir.join(UPDATE_QUEUE_FILE);

    let line = serde_json::to_string(&validated).context("serialize update queue request")?;
    let line_bytes = line.len() + 1; // trailing newline

    if path.is_file() {
        let meta = fs::metadata(&path)?;
        if meta.len().saturating_add(line_bytes as u64) > MAX_QUEUE_BYTES {
            bail!(
                "update queue is full (max {} bytes at {}); stop spam or wait for serve --watch to drain",
                MAX_QUEUE_BYTES,
                path.display()
            );
        }
        let lines = count_lines(&path)?;
        if lines >= MAX_QUEUE_LINES {
            bail!(
                "update queue is full (max {} lines at {}); wait for serve --watch to drain",
                MAX_QUEUE_LINES,
                path.display()
            );
        }
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open queue {}", path.display()))?;
    // Single write with trailing newline while holding `_queue_lock`.
    file.write_all(format!("{line}\n").as_bytes())
        .with_context(|| format!("append queue {}", path.display()))?;
    file.flush()?;
    Ok(())
}

/// Whether a non-empty queue file exists (for poll wake).
#[must_use]
pub fn queue_nonempty(repo: &Path) -> bool {
    let dir = rgctl_graph::paths::artifact_dir(repo);
    let has_src = fs::metadata(dir.join(UPDATE_QUEUE_FILE)).is_ok_and(|m| m.len() > 0);
    has_src || fs::metadata(dir.join(UPDATE_QUEUE_PROCESSING_FILE)).is_ok_and(|m| m.len() > 0)
}

/// Cheap pending line count for status (0 if missing).
#[must_use]
pub fn queue_pending_count(repo: &Path) -> usize {
    let path = queue_path(repo);
    if !path.is_file() {
        return 0;
    }
    count_lines(&path).unwrap_or(0)
}

/// Atomically drain the queue: lock → read leftover processing + rename/read queue → unlock.
///
/// Malformed lines are skipped and reported in [`CoalescedBatch::drain_warnings`].
pub fn drain_queue(repo: &Path) -> Result<CoalescedBatch> {
    let dir = rgctl_graph::paths::artifact_dir(repo);
    let src = dir.join(UPDATE_QUEUE_FILE);
    let processing = dir.join(UPDATE_QUEUE_PROCESSING_FILE);

    if !src.is_file() && !processing.is_file() {
        return Ok(CoalescedBatch::default());
    }

    let mut bytes = Vec::new();
    {
        let _queue_lock = lock_queue_file(&dir)?;
        if processing.is_file() {
            let mut prev = OpenOptions::new()
                .read(true)
                .open(&processing)
                .with_context(|| format!("open {}", processing.display()))?;
            prev.read_to_end(&mut bytes)?;
            if bytes.last().is_some_and(|&b| b != b'\n') {
                bytes.push(b'\n');
            }
            let _ = fs::remove_file(&processing);
        }
        if src.is_file() {
            match fs::rename(&src, &processing) {
                Ok(()) => {
                    let mut file = OpenOptions::new()
                        .read(true)
                        .open(&processing)
                        .with_context(|| format!("open {}", processing.display()))?;
                    file.read_to_end(&mut bytes)?;
                    let _ = fs::remove_file(&processing);
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("rename queue {} → {}", src.display(), processing.display())
                    });
                }
            }
        }
    }
    if bytes.is_empty() {
        return Ok(CoalescedBatch::default());
    }

    let text = String::from_utf8_lossy(&bytes);
    let mut batch = CoalescedBatch::default();
    let mut since_ts: Option<u64> = None;

    for (idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: Result<UpdateQueueRequest, _> = serde_json::from_str(line);
        let mut req = match parsed {
            Ok(r) => r,
            Err(err) => {
                batch
                    .drain_warnings
                    .push(format!("skip queue line {}: invalid JSON ({err})", idx + 1));
                continue;
            }
        };
        if let Err(err) = validate_request(&mut req) {
            batch.drain_warnings.push(format!(
                "skip queue line {}: validation failed ({err})",
                idx + 1
            ));
            continue;
        }

        match req.mode {
            UpdateQueueMode::Paths => {
                if let Some(paths) = &req.paths {
                    for p in paths {
                        match normalize_repo_path(repo, p) {
                            Ok(norm) => {
                                if !batch.path_set.iter().any(|x| x == &norm) {
                                    batch.path_set.push(norm);
                                }
                            }
                            Err(err) => {
                                batch.warnings.push(format!(
                                    "request {}: rejected path '{p}': {err}",
                                    req.request_id
                                ));
                            }
                        }
                    }
                }
            }
            UpdateQueueMode::HashDiff => {
                batch.run_hash_diff = true;
            }
            UpdateQueueMode::Since => {
                if let Some(ref s) = req.since {
                    match since_ts {
                        Some(prev) if req.ts_ms < prev => {
                            batch.warnings.push(format!(
                                "conflicting --since in drain; keeping newer ref (ignored {} from {})",
                                s, req.request_id
                            ));
                        }
                        Some(_) => {
                            batch.warnings.push(format!(
                                "conflicting --since in drain; using newest ref '{}' from {}",
                                s, req.request_id
                            ));
                            batch.since = Some(s.clone());
                            since_ts = Some(req.ts_ms);
                        }
                        None => {
                            batch.since = Some(s.clone());
                            since_ts = Some(req.ts_ms);
                        }
                    }
                }
            }
        }

        // Options: last non-None wins (simple v1).
        if let Some(d) = req.cascade_depth {
            batch.cascade_depth = Some(d);
        }
        if req.languages.is_some() {
            batch.languages = req.languages.clone();
        }
        if req.exclude.is_some() {
            batch.exclude = req.exclude.clone();
        }

        batch.requests.push(req);
    }

    Ok(batch)
}

/// Write a success/failure result for `request_id`.
pub fn write_result(repo: &Path, result: &UpdateQueueResult) -> Result<()> {
    let dir = results_dir(repo);
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let path = result_path(repo, &result.request_id);
    let tmp = dir.join(format!("{}.tmp", result.request_id));
    let json = serde_json::to_vec_pretty(result).context("serialize update queue result")?;
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &path).with_context(|| format!("write result {}", path.display()))?;
    Ok(())
}

/// Read a result file if present.
pub fn read_result(repo: &Path, request_id: &str) -> Result<Option<UpdateQueueResult>> {
    let path = result_path(repo, request_id);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let result: UpdateQueueResult =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(result))
}

/// Delete the result file after the CLI has consumed it.
pub fn delete_result(repo: &Path, request_id: &str) -> Result<()> {
    let path = result_path(repo, request_id);
    if path.is_file() {
        fs::remove_file(&path).with_context(|| format!("delete {}", path.display()))?;
    }
    Ok(())
}

/// Poll until a result appears or `timeout` elapses.
pub fn wait_for_result(
    repo: &Path,
    request_id: &str,
    timeout: Duration,
) -> Result<UpdateQueueResult> {
    let start = std::time::Instant::now();
    let mut sleep = Duration::from_millis(10);
    loop {
        if let Some(result) = read_result(repo, request_id)? {
            return Ok(result);
        }
        if start.elapsed() >= timeout {
            bail!(
                "timed out after {}s waiting for update result (request_id={request_id}); \
                 check that `rgctl serve --watch` is running and healthy",
                timeout.as_secs()
            );
        }
        std::thread::sleep(sleep);
        sleep = (sleep + Duration::from_millis(10)).min(Duration::from_millis(50));
    }
}

/// Build an ok result from shared batch stats.
#[must_use]
pub fn ok_result(
    request_id: &str,
    files_added: usize,
    files_changed: usize,
    files_deleted: usize,
    nodes_added: usize,
    nodes_removed: usize,
    edges_added: usize,
    edges_removed: usize,
    duration_ms: u64,
    warnings: Vec<String>,
    message: Option<String>,
) -> UpdateQueueResult {
    let files_affected = files_added + files_changed + files_deleted;
    UpdateQueueResult {
        schema_version: UPDATE_QUEUE_SCHEMA_VERSION,
        request_id: request_id.to_string(),
        ok: true,
        command: "update".into(),
        source: "watch_queue".into(),
        files_added,
        files_changed,
        files_deleted,
        files_affected,
        nodes_added,
        nodes_removed,
        edges_added,
        edges_removed,
        duration_ms,
        message,
        warnings,
        error: None,
    }
}

/// Build an error result.
#[must_use]
pub fn err_result(
    request_id: &str,
    error: impl Into<String>,
    warnings: Vec<String>,
) -> UpdateQueueResult {
    UpdateQueueResult {
        schema_version: UPDATE_QUEUE_SCHEMA_VERSION,
        request_id: request_id.to_string(),
        ok: false,
        command: "update".into(),
        source: "watch_queue".into(),
        files_added: 0,
        files_changed: 0,
        files_deleted: 0,
        files_affected: 0,
        nodes_added: 0,
        nodes_removed: 0,
        edges_added: 0,
        edges_removed: 0,
        duration_ms: 0,
        message: None,
        warnings,
        error: Some(error.into()),
    }
}

/// Remove result files older than [`RESULT_MAX_AGE`].
pub fn prune_old_results(repo: &Path) -> Result<usize> {
    let dir = results_dir(repo);
    if !dir.is_dir() {
        return Ok(0);
    }
    let now = SystemTime::now();
    let mut removed = 0usize;
    for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let meta = entry.metadata()?;
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        if now.duration_since(modified).unwrap_or(Duration::ZERO) > RESULT_MAX_AGE {
            if fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_paths_req(paths: &[&str]) -> UpdateQueueRequest {
        build_request(
            UpdateQueueMode::Paths,
            Some(paths.iter().map(|s| (*s).to_string()).collect()),
            None,
            Some(1),
            None,
            None,
        )
        .expect("build")
    }

    #[test]
    fn append_and_drain_roundtrip() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let r1 = sample_paths_req(&["src/a.rs"]);
        let r2 = build_request(UpdateQueueMode::HashDiff, None, None, None, None, None).unwrap();
        enqueue(repo, &r1).unwrap();
        enqueue(repo, &r2).unwrap();
        assert!(queue_nonempty(repo));
        assert_eq!(queue_pending_count(repo), 2);

        let batch = drain_queue(repo).unwrap();
        assert_eq!(batch.requests.len(), 2);
        assert!(batch.run_hash_diff);
        assert!(batch.path_set.iter().any(|p| p == "src/a.rs"));
        assert!(!queue_nonempty(repo));

        // Concurrent append after rename lands in a fresh file.
        let r3 = sample_paths_req(&["src/b.rs"]);
        enqueue(repo, &r3).unwrap();
        assert!(queue_nonempty(repo));
        let batch2 = drain_queue(repo).unwrap();
        assert_eq!(batch2.requests.len(), 1);
        assert!(batch2.path_set.iter().any(|p| p == "src/b.rs"));
    }

    #[test]
    fn drain_skips_bad_lines() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let path = queue_path(repo);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "not-json\n{\"schema_version\":1,\"request_id\":\"x\",\"pid\":1,\"ts_ms\":1,\"mode\":\"hash_diff\"}\n",
        )
        .unwrap();
        let batch = drain_queue(repo).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert!(!batch.drain_warnings.is_empty());
    }

    #[test]
    fn path_escape_rejected() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        assert!(normalize_repo_path(repo, "../outside.rs").is_err());
        assert!(normalize_repo_path(repo, "src/../../x").is_err());
        assert_eq!(normalize_repo_path(repo, "src/./a.rs").unwrap(), "src/a.rs");
    }

    #[test]
    fn coalesce_conflicting_since_newest_wins() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let mut older = build_request(
            UpdateQueueMode::Since,
            None,
            Some("HEAD~2".into()),
            None,
            None,
            None,
        )
        .unwrap();
        older.ts_ms = 100;
        let mut newer = build_request(
            UpdateQueueMode::Since,
            None,
            Some("HEAD~1".into()),
            None,
            None,
            None,
        )
        .unwrap();
        newer.ts_ms = 200;
        enqueue(repo, &older).unwrap();
        enqueue(repo, &newer).unwrap();
        let batch = drain_queue(repo).unwrap();
        assert_eq!(batch.since.as_deref(), Some("HEAD~1"));
        assert!(!batch.warnings.is_empty());
    }

    #[test]
    fn result_write_read_delete() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let result = ok_result(id, 0, 1, 0, 2, 1, 3, 0, 42, vec![], None);
        write_result(repo, &result).unwrap();
        let loaded = read_result(repo, id).unwrap().expect("present");
        assert!(loaded.ok);
        assert_eq!(loaded.source, "watch_queue");
        assert_eq!(loaded.files_changed, 1);
        delete_result(repo, id).unwrap();
        assert!(read_result(repo, id).unwrap().is_none());
    }

    #[test]
    fn queue_size_guard() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let path = queue_path(repo);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Pretend the queue is already at the line limit.
        let mut big = String::new();
        for i in 0..MAX_QUEUE_LINES {
            big.push_str(&format!(
                "{{\"schema_version\":1,\"request_id\":\"{i}\",\"pid\":1,\"ts_ms\":1,\"mode\":\"hash_diff\"}}\n"
            ));
        }
        fs::write(&path, big).unwrap();
        let req = build_request(UpdateQueueMode::HashDiff, None, None, None, None, None).unwrap();
        let err = enqueue(repo, &req).unwrap_err();
        assert!(format!("{err:#}").contains("full"));
    }
}
