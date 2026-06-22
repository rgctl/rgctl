//! File system watch service for incremental graph updates (Phase 13.1).

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rbuilder_error::{Error, Result};
use rbuilder_extraction::discovery::{DiscoveryConfig, FileDiscoverer};
use rbuilder_graph::CodeGraph;
use rbuilder_incremental::file_tracker::relative_path;
use rbuilder_incremental::{IncrementalUpdater, UpdateOptions, UpdateResult};
use rbuilder_project_config::project::RbuilderConfig;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc as StdArc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Notification payload for MCP clients when the graph changes.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GraphUpdateNotification {
    /// Unix timestamp (seconds).
    pub timestamp: u64,
    /// Repo-relative paths that changed.
    pub files_changed: Vec<String>,
    /// Nodes added during the update.
    pub nodes_added: usize,
    /// Nodes removed during the update.
    pub nodes_removed: usize,
    /// Edges added plus removed.
    pub edges_changed: usize,
}

impl GraphUpdateNotification {
    /// Build a notification from an incremental update result.
    pub fn from_result(files: Vec<String>, result: &UpdateResult) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            timestamp,
            files_changed: files,
            nodes_added: result.nodes_added,
            nodes_removed: result.nodes_removed,
            edges_changed: result.edges_added + result.edges_removed,
        }
    }
}

type UpdateCallback = Arc<dyn Fn(GraphUpdateNotification) + Send + Sync>;

/// Watches a repository and incrementally updates the knowledge graph.
pub struct WatchService {
    repo_root: PathBuf,
    debounce: Duration,
    updater: IncrementalUpdater,
    discoverer: FileDiscoverer,
    on_update: Option<UpdateCallback>,
}

impl WatchService {
    /// Create a watch service for a repository.
    pub fn new(repo_root: &Path) -> Result<Self> {
        let config = RbuilderConfig::load(repo_root)?;
        Self::with_debounce(repo_root, config.watch.debounce_ms)
    }

    /// Create with an explicit debounce interval (milliseconds).
    pub fn with_debounce(repo_root: &Path, debounce_ms: u64) -> Result<Self> {
        let registry = StdArc::new(rbuilder_registry::full_registry());
        Ok(Self {
            repo_root: repo_root.to_path_buf(),
            debounce: Duration::from_millis(debounce_ms),
            updater: IncrementalUpdater::with_options(
                StdArc::clone(&registry),
                UpdateOptions {
                    show_progress: false,
                    ..UpdateOptions::default()
                },
            ),
            discoverer: FileDiscoverer::with_config(registry, DiscoveryConfig::default()),
            on_update: None,
        })
    }

    /// Register a callback invoked after each successful graph update.
    pub fn on_graph_updated<F>(mut self, callback: F) -> Self
    where
        F: Fn(GraphUpdateNotification) + Send + Sync + 'static,
    {
        self.on_update = Some(Arc::new(callback));
        self
    }

    /// Run watch mode, blocking until interrupted (Ctrl+C).
    pub fn run_blocking(repo_root: &Path, debounce_ms: Option<u64>) -> Result<()> {
        let debounce = debounce_ms.unwrap_or_else(|| {
            RbuilderConfig::load(repo_root)
                .map(|c| c.watch.debounce_ms)
                .unwrap_or(500)
        });
        let service = Self::with_debounce(repo_root, debounce)?;
        service.run()
    }

    /// Run the watch loop.
    pub fn run(self) -> Result<()> {
        println!(
            "Watching {} (debounce {}ms). Press Ctrl+C to stop.",
            self.repo_root.display(),
            self.debounce.as_millis()
        );

        let graph = Arc::new(Mutex::new(load_or_init_graph(&self.repo_root)?));
        let (event_tx, event_rx) = mpsc::channel();
        let repo_root = self.repo_root.clone();
        let mut watcher = RecommendedWatcher::new(
            move |res: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if let Some(paths) = watch_paths_from_event(&event) {
                        for path in paths {
                            let _ = event_tx.send(path);
                        }
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| Error::Other(format!("Failed to create watcher: {e}")))?;

        watcher
            .watch(&self.repo_root, RecursiveMode::Recursive)
            .map_err(|e| Error::Other(format!("Failed to watch repo: {e}")))?;

        let tracked = self.discoverer.discover(&self.repo_root)?;
        let tracked_set: HashSet<PathBuf> = tracked.into_iter().collect();

        let mut pending: HashSet<PathBuf> = HashSet::new();
        let mut last_event = Instant::now();

        loop {
            match event_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(path) => {
                    if is_tracked_path(&repo_root, &path, &tracked_set) {
                        pending.insert(path);
                        last_event = Instant::now();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if !debounce_ready(!pending.is_empty(), last_event, self.debounce) {
                        continue;
                    }
                    let batch: Vec<PathBuf> = pending.drain().collect();
                    let rel_paths: Vec<String> = batch
                        .iter()
                        .filter_map(|p| relative_path(&repo_root, p).ok())
                        .collect();

                    let mut graph_guard = graph.lock().unwrap();
                    match self
                        .updater
                        .update_files(&mut graph_guard, &repo_root, &rel_paths)
                    {
                        Ok(result) => {
                            if result.files_affected() > 0 {
                                println!(
                                    "Updated {} file(s): +{} / -{} nodes",
                                    result.files_affected(),
                                    result.nodes_added,
                                    result.nodes_removed
                                );
                                if let Some(cb) = &self.on_update {
                                    cb(GraphUpdateNotification::from_result(rel_paths, &result));
                                }
                            }
                        }
                        Err(e) => eprintln!("Watch update error: {e}"),
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(())
    }
}

/// Notification channel for MCP graph-update events.
#[cfg(feature = "mcp-server")]
pub type NotificationSender = Sender<GraphUpdateNotification>;

/// Shared store for the latest graph-update notification (HTTP MCP clients).
#[cfg(feature = "mcp-server")]
pub type NotificationStore = std::sync::Arc<std::sync::Mutex<Option<GraphUpdateNotification>>>;

/// Create an empty notification store for HTTP polling clients.
#[cfg(feature = "mcp-server")]
pub fn new_notification_store() -> NotificationStore {
    std::sync::Arc::new(std::sync::Mutex::new(None))
}

/// Record the latest notification for polling clients.
#[cfg(feature = "mcp-server")]
pub fn record_notification(store: &NotificationStore, notification: GraphUpdateNotification) {
    if let Ok(mut guard) = store.lock() {
        *guard = Some(notification);
    }
}

/// Read the latest notification, if any.
#[cfg(feature = "mcp-server")]
pub fn latest_notification(store: &NotificationStore) -> Option<GraphUpdateNotification> {
    store.lock().ok().and_then(|guard| guard.clone())
}

/// Returns true when a debounced batch is ready to flush.
pub fn debounce_ready(pending: bool, last_event: Instant, debounce: Duration) -> bool {
    pending && last_event.elapsed() >= debounce
}

/// Spawn watch mode updating shared MCP state, sending notifications.
#[cfg(feature = "mcp-server")]
pub fn spawn_watch_with_state(
    state: crate::api::state::AppState,
    debounce_ms: u64,
    notify_tx: NotificationSender,
) -> Result<std::thread::JoinHandle<()>> {
    let _repo_root = state.repo_root();
    let registry = StdArc::new(rbuilder_registry::full_registry());
    let updater = IncrementalUpdater::with_options(
        StdArc::clone(&registry),
        UpdateOptions {
            show_progress: false,
            ..UpdateOptions::default()
        },
    );
    let discoverer = FileDiscoverer::with_config(registry, DiscoveryConfig::default());
    let debounce = Duration::from_millis(debounce_ms);

    Ok(std::thread::spawn(move || {
        if let Err(e) = run_watch_loop(state, updater, discoverer, debounce, notify_tx) {
            eprintln!("Watch service stopped: {e}");
        }
    }))
}

#[cfg(feature = "mcp-server")]
fn run_watch_loop(
    state: crate::api::state::AppState,
    updater: IncrementalUpdater,
    discoverer: FileDiscoverer,
    debounce: Duration,
    notify_tx: NotificationSender,
) -> Result<()> {
    let repo_root = state.repo_root();
    let (event_tx, event_rx) = mpsc::channel();
    let root = repo_root.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res: std::result::Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if let Some(paths) = watch_paths_from_event(&event) {
                    for path in paths {
                        let _ = event_tx.send(path);
                    }
                }
            }
        },
        Config::default(),
    )
    .map_err(|e| Error::Other(format!("Failed to create watcher: {e}")))?;

    watcher
        .watch(&repo_root, RecursiveMode::Recursive)
        .map_err(|e| Error::Other(format!("Failed to watch repo: {e}")))?;

    let tracked = discoverer.discover(&repo_root)?;
    let tracked_set: HashSet<PathBuf> = tracked.into_iter().collect();
    let mut pending: HashSet<PathBuf> = HashSet::new();
    let mut last_event = Instant::now();

    loop {
        match event_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(path) => {
                if is_tracked_path(&root, &path, &tracked_set) {
                    pending.insert(path);
                    last_event = Instant::now();
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if !debounce_ready(!pending.is_empty(), last_event, debounce) {
                    continue;
                }
                let batch: Vec<PathBuf> = pending.drain().collect();
                let rel_paths: Vec<String> = batch
                    .iter()
                    .filter_map(|p| relative_path(&root, p).ok())
                    .collect();

                let update_result = state
                    .with_graph_mut(|graph| updater.update_files(graph, &repo_root, &rel_paths));

                match update_result {
                    Ok(result) => {
                        if result.files_affected() > 0 {
                            let _ = notify_tx
                                .send(GraphUpdateNotification::from_result(rel_paths, &result));
                        }
                    }
                    Err(e) => eprintln!("Watch update error: {e}"),
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

fn load_or_init_graph(repo_root: &Path) -> Result<CodeGraph> {
    match CodeGraph::load_from_repo(repo_root) {
        Ok(graph) => Ok(graph),
        Err(_) => {
            use rbuilder_pipeline::{PipelineConfig, ProcessingPipeline};
            let registry = StdArc::new(rbuilder_registry::full_registry());
            let pipeline = ProcessingPipeline::with_config(
                registry,
                PipelineConfig {
                    show_progress: false,
                    ..PipelineConfig::default()
                },
            );
            let (graph, _) = pipeline.process_repository(repo_root)?;
            graph.save_to_repo(repo_root)?;
            Ok(graph)
        }
    }
}

fn watch_paths_from_event(event: &Event) -> Option<Vec<PathBuf>> {
    match &event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
        _ => return None,
    }
    if event.paths.is_empty() {
        return None;
    }
    Some(event.paths.clone())
}

fn is_tracked_path(repo_root: &Path, path: &Path, tracked: &HashSet<PathBuf>) -> bool {
    if path.starts_with(repo_root.join(".rbuilder"))
        || path.components().any(|c| c.as_os_str() == ".git")
    {
        return false;
    }
    tracked.contains(path)
        || tracked.contains(&path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, ModifyKind};
    use std::path::PathBuf;

    #[test]
    fn test_graph_update_notification() {
        let result = UpdateResult {
            files_changed: 1,
            nodes_added: 2,
            nodes_removed: 1,
            edges_added: 3,
            edges_removed: 1,
            ..Default::default()
        };
        let n = GraphUpdateNotification::from_result(vec!["a.rs".into()], &result);
        assert_eq!(n.files_changed, vec!["a.rs"]);
        assert_eq!(n.nodes_added, 2);
        assert_eq!(n.edges_changed, 4);
    }

    #[test]
    fn test_debounce_ready_waits_for_window() {
        let debounce = Duration::from_millis(100);
        let last = Instant::now() - Duration::from_millis(200);
        assert!(debounce_ready(true, last, debounce));
        let recent = Instant::now();
        assert!(!debounce_ready(true, recent, debounce));
        assert!(!debounce_ready(false, last, debounce));
    }

    #[test]
    fn test_watch_paths_from_modify_event() {
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            paths: vec![PathBuf::from("src/main.rs")],
            attrs: Default::default(),
        };
        let paths = watch_paths_from_event(&event).unwrap();
        assert_eq!(paths, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_watch_paths_from_create_event() {
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("src/new.rs")],
            attrs: Default::default(),
        };
        assert!(watch_paths_from_event(&event).is_some());
    }

    #[test]
    fn test_is_tracked_path_ignores_git_and_rbuilder() {
        let root = PathBuf::from("/repo");
        let tracked: HashSet<PathBuf> = [root.join("src/main.rs")].into_iter().collect();
        assert!(!is_tracked_path(&root, &root.join(".git/HEAD"), &tracked));
        assert!(!is_tracked_path(
            &root,
            &root.join(".rbuilder/graph.json"),
            &tracked
        ));
    }

    #[cfg(feature = "mcp-server")]
    #[test]
    fn test_notification_store_roundtrip() {
        let store = new_notification_store();
        assert!(latest_notification(&store).is_none());
        let notification = GraphUpdateNotification {
            timestamp: 1,
            files_changed: vec!["a.rs".into()],
            nodes_added: 2,
            nodes_removed: 0,
            edges_changed: 1,
        };
        record_notification(&store, notification.clone());
        assert_eq!(latest_notification(&store), Some(notification));
    }
}
