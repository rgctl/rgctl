//! Phase 13: change detection and incremental updates.

use rgctl::changes::ChangeDetector;
use rgctl::config::project::{RgctlConfig, RiskLevel};
use rgctl::graph::backend::GraphBackend;
use rgctl::graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl::incremental::{IncrementalUpdater, UpdateOptions, changes_for_paths};
use rgctl::languages::registry::LanguageRegistry;
use rgctl::pipeline::{PipelineConfig, ProcessingPipeline};
use std::fs;
use tempfile::TempDir;

fn chain_graph_repo(temp: &TempDir) -> rgctl::CodeGraph {
    let root = temp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn a() { b(); }\npub fn b() { c(); }\npub fn c() {}\n",
    )
    .unwrap();

    let pipeline = ProcessingPipeline::with_config(
        LanguageRegistry::new().into(),
        PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        },
    );
    let (graph, _) = pipeline.process_repository(root).unwrap();
    graph.save_to_repo(root).unwrap();

    let mut tracker = rgctl::incremental::FileTracker::new(root);
    let files = vec![root.join("src/lib.rs")];
    tracker.index_files(&files, &graph).unwrap();
    tracker.save().unwrap();
    graph
}

#[test]
fn test_changes_for_paths_modified() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let _graph = chain_graph_repo(&temp);
    let changes = changes_for_paths(root, &["src/lib.rs".into()]).unwrap();
    assert!(
        changes.changed.contains(&"src/lib.rs".to_string())
            || changes.added.contains(&"src/lib.rs".to_string())
    );
}

#[test]
fn test_detect_changes_risk_on_chain() {
    let mut graph = rgctl::CodeGraph::new();
    let backend = graph.backend_mut();
    let a = Node::new(NodeType::Function, "a").with_file_path("src/lib.rs");
    let b = Node::new(NodeType::Function, "b").with_file_path("src/lib.rs");
    let c = Node::new(NodeType::Function, "c").with_file_path("src/lib.rs");
    let id_a = a.id;
    let id_b = b.id;
    let id_c = c.id;
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend.insert_node(c).unwrap();
    backend
        .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_b, id_c, EdgeType::Calls))
        .unwrap();

    let detector = ChangeDetector::new();
    let result = detector.detect(&graph, &["src/lib.rs".into()]).unwrap();
    assert!(!result.details.is_empty());
    assert!(result.details.iter().any(|d| d.symbol == "c"));
}

#[test]
fn test_update_files_incremental() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let mut graph = chain_graph_repo(&temp);
    let lib = root.join("src/lib.rs");
    fs::write(
        &lib,
        "pub fn a() { b(); }\npub fn b() { c(); }\npub fn c() {}\npub fn d() {}\n",
    )
    .unwrap();

    let updater = IncrementalUpdater::with_options(
        LanguageRegistry::new().into(),
        UpdateOptions {
            show_progress: false,
            ..Default::default()
        },
    );
    let result = updater
        .update_files(&mut graph, root, &["src/lib.rs".into()])
        .unwrap();
    assert!(result.files_changed >= 1 || result.nodes_added > 0);
}

#[test]
fn test_rgctl_config_defaults() {
    let temp = TempDir::new().unwrap();
    let cfg = RgctlConfig::load(temp.path()).unwrap();
    assert_eq!(cfg.hooks.block_on_risk, RiskLevel::Critical);
    assert_eq!(cfg.watch.debounce_ms, 25);
}

#[test]
fn test_manual_graph_blast_risk() {
    let mut graph = rgctl::CodeGraph::new();
    let backend = graph.backend_mut();
    let a = Node::new(NodeType::Function, "a").with_file_path("f.rs");
    let b = Node::new(NodeType::Function, "b").with_file_path("f.rs");
    let c = Node::new(NodeType::Function, "c").with_file_path("f.rs");
    let id_a = a.id;
    let id_b = b.id;
    let id_c = c.id;
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend.insert_node(c).unwrap();
    backend
        .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_b, id_c, EdgeType::Calls))
        .unwrap();

    let result = ChangeDetector::new()
        .detect(&graph, &["f.rs".into()])
        .unwrap();
    assert!(result.details.iter().any(|d| d.symbol == "c"));
}

#[test]
fn test_critical_risk_blocks_per_config() {
    use rgctl::config::project::RgctlConfig;
    let cfg = RgctlConfig::default();
    assert!(cfg.hooks.block_on_risk.blocks(RiskLevel::Critical));
    assert!(!cfg.hooks.block_on_risk.blocks(RiskLevel::Medium));
}

#[test]
fn test_high_risk_blocked_when_configured() {
    use rgctl::config::project::{RgctlConfig, RiskLevel};
    let mut cfg = RgctlConfig::default();
    cfg.hooks.block_on_risk = RiskLevel::High;
    assert!(cfg.hooks.block_on_risk.blocks(RiskLevel::Critical));
    assert!(cfg.hooks.block_on_risk.blocks(RiskLevel::High));
    assert!(!cfg.hooks.block_on_risk.blocks(RiskLevel::Medium));
}

#[test]
fn test_detect_changes_json_contains_summary() {
    let mut graph = rgctl::CodeGraph::new();
    let backend = graph.backend_mut();
    let leaf = Node::new(NodeType::Function, "leaf").with_file_path("f.rs");
    backend.insert_node(leaf).unwrap();
    let result = ChangeDetector::new()
        .detect(&graph, &["f.rs".into()])
        .unwrap();
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("summary"));
    assert!(json.contains("files_analyzed"));
}

#[test]
fn test_changes_for_paths_detects_new_file() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let _graph = chain_graph_repo(&temp);
    fs::write(root.join("src/extra.rs"), "pub fn extra() {}\n").unwrap();
    let changes = changes_for_paths(root, &["src/extra.rs".into()]).unwrap();
    assert!(changes.added.contains(&"src/extra.rs".to_string()));
}

#[test]
fn test_full_workflow_modify_and_update() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let mut graph = chain_graph_repo(&temp);
    let before = graph.node_count();

    let lib = root.join("src/lib.rs");
    fs::write(
        &lib,
        "pub fn a() { b(); }\npub fn b() { c(); }\npub fn c() {}\npub fn added() {}\n",
    )
    .unwrap();

    let updater = IncrementalUpdater::with_options(
        LanguageRegistry::new().into(),
        UpdateOptions {
            show_progress: false,
            ..Default::default()
        },
    );
    let result = updater
        .update_files(&mut graph, root, &["src/lib.rs".into()])
        .unwrap();
    assert!(result.files_affected() >= 1);
    assert!(graph.node_count() >= before);
}

#[test]
fn test_cli_update_paths_and_noop() {
    use rgctl::cli::update::{UpdateArgs, run_update_at, update_paths};

    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let graph = chain_graph_repo(&temp);
    // CLI update requires columnar snapshot (save_to_repo alone is legacy JSON).
    graph.save_snapshot(root).unwrap();

    let noop = run_update_at(
        root,
        &UpdateArgs {
            cascade_depth: 1,
            ..UpdateArgs::default()
        },
        false,
        true,
    )
    .unwrap();
    assert_eq!(noop.files_affected(), 0);

    fs::write(
        root.join("src/lib.rs"),
        "pub fn a() { b(); }\npub fn b() { c(); }\npub fn c() {}\npub fn e() {}\n",
    )
    .unwrap();
    let updated = update_paths(root, &["src/lib.rs".into()], 1).unwrap();
    assert!(updated.files_affected() >= 1 || updated.nodes_added > 0);
}

#[test]
fn test_update_in_process_blocked_when_watch_lock_held() {
    use rgctl::cli::pipeline_status::try_acquire_watch_lock;
    use rgctl::cli::update::{UpdateArgs, run_update_at};

    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let graph = chain_graph_repo(&temp);
    graph.save_snapshot(root).unwrap();

    let _watch = try_acquire_watch_lock(root).unwrap();
    // In-process compact still cannot take the lock while it is held (watch path).
    let err = run_update_at(
        root,
        &UpdateArgs {
            cascade_depth: 1,
            ..UpdateArgs::default()
        },
        false,
        true,
    )
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("watch") || msg.contains("watcher") || msg.contains("lock"),
        "unexpected error: {msg}"
    );
}

#[test]
fn test_update_queue_enqueue_drain_and_result() {
    use rgctl::cli::pipeline_status::{
        WATCH_LOCK_FILE, detect_live_watcher, process_alive, update_queue, watch_lock_path,
    };
    use rgctl::cli::update::apply_queue_batch;
    use update_queue::{
        UpdateQueueMode, build_request, drain_queue, enqueue, queue_nonempty, read_result,
    };

    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let graph = chain_graph_repo(&temp);
    graph.save_snapshot(root).unwrap();

    // Simulate an external live watcher (pid 1 is typically init/launchd).
    let external_pid = 1u32;
    if !process_alive(external_pid) || external_pid == std::process::id() {
        return;
    }
    let lock = watch_lock_path(root);
    fs::create_dir_all(lock.parent().unwrap()).unwrap();
    fs::write(&lock, format!("{external_pid}\n")).unwrap();
    assert!(
        detect_live_watcher(root).is_some(),
        "expected live watcher for pid {external_pid}"
    );
    assert!(lock.file_name().and_then(|n| n.to_str()) == Some(WATCH_LOCK_FILE));

    let req = build_request(
        UpdateQueueMode::Paths,
        Some(vec!["src/lib.rs".into()]),
        None,
        Some(1),
        None,
        None,
    )
    .unwrap();
    let id = req.request_id.clone();
    enqueue(root, &req).unwrap();
    assert!(queue_nonempty(root));

    let batch = drain_queue(root).unwrap();
    assert_eq!(batch.requests.len(), 1);
    apply_queue_batch(root, &batch, &[]).unwrap();
    let result = read_result(root, &id).unwrap().expect("result file");
    assert!(result.ok, "expected ok result, got {:?}", result.error);
    assert_eq!(result.source, "watch_queue");
}

#[test]
fn test_update_queue_no_wait_leaves_pending() {
    use rgctl::cli::pipeline_status::{process_alive, update_queue, watch_lock_path};
    use std::time::Duration;
    use update_queue::{
        UpdateQueueMode, build_request, enqueue, queue_nonempty, queue_pending_count,
        wait_for_result,
    };

    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let external_pid = 1u32;
    if !process_alive(external_pid) || external_pid == std::process::id() {
        return;
    }
    let lock = watch_lock_path(root);
    fs::create_dir_all(lock.parent().unwrap()).unwrap();
    fs::write(&lock, format!("{external_pid}\n")).unwrap();

    let req = build_request(UpdateQueueMode::HashDiff, None, None, Some(1), None, None).unwrap();
    let id = req.request_id.clone();
    enqueue(root, &req).unwrap();
    assert!(queue_nonempty(root));
    assert_eq!(queue_pending_count(root), 1);
    // No watcher drain → wait times out (same failure mode as CLI --wait-timeout).
    let err = wait_for_result(root, &id, Duration::from_millis(80)).unwrap_err();
    assert!(
        format!("{err:#}").contains("timed out"),
        "expected timeout, got {err:#}"
    );
}

#[test]
fn test_stale_watch_lock_allows_local_update() {
    use rgctl::cli::pipeline_status::{process_alive, try_acquire_watch_lock, watch_lock_path};
    use rgctl::cli::update::{UpdateArgs, run_update_at};

    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let graph = chain_graph_repo(&temp);
    graph.save_snapshot(root).unwrap();

    let dead_pid = 4_294_967_294u32;
    if process_alive(dead_pid) {
        return;
    }
    let lock = watch_lock_path(root);
    fs::create_dir_all(lock.parent().unwrap()).unwrap();
    fs::write(&lock, format!("{dead_pid}\n")).unwrap();

    // Reclaim + in-process update succeeds.
    let result = run_update_at(
        root,
        &UpdateArgs {
            cascade_depth: 1,
            ..UpdateArgs::default()
        },
        false,
        true,
    )
    .unwrap();
    assert_eq!(result.files_affected(), 0);
    // Lock should now be held by us or released after Drop of temporary acquire inside run.
    drop(result);
    let _ = try_acquire_watch_lock(root).expect("lock free or reclaimable after update");
}

#[test]
fn test_standalone_watch_holds_lock_and_queues_updates() {
    use rgctl::cli::pipeline_status::watch_lock_path;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let outer = TempDir::new().unwrap();
    let repo = outer.path().join("subrepo");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("tiny.py"), "def ping():\n    return 1\n").unwrap();

    let bin = env!("CARGO_BIN_EXE_rgctl");
    let discover_out = Command::new(bin)
        .current_dir(&repo)
        .args(["discover", "."])
        .output()
        .unwrap();
    assert!(
        discover_out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&discover_out.stderr)
    );

    // Launch `rgctl watch subrepo` from parent dir to also verify relative `PATH` resolution.
    let mut watcher = ChildGuard(
        Command::new(bin)
            .current_dir(outer.path())
            .args(["watch", "subrepo"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );

    let lock_file = watch_lock_path(&repo);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !lock_file.is_file() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for .rgctl/watch.lock to appear"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // 1. `rgctl -f json status` must report `watcher_alive: true`.
    let status_out = Command::new(bin)
        .current_dir(&repo)
        .args(["-f", "json", "status"])
        .output()
        .unwrap();
    assert!(status_out.status.success());
    let status_json: serde_json::Value = serde_json::from_slice(&status_out.stdout).unwrap();
    assert_eq!(status_json["watcher_alive"], serde_json::Value::Bool(true));

    // 2. Starting a second `rgctl watch` against the same repo must fail fast.
    let dup_watch = Command::new(bin)
        .current_dir(&repo)
        .arg("watch")
        .output()
        .unwrap();
    assert!(
        !dup_watch.status.success(),
        "duplicate rgctl watch should fail when lock is held"
    );

    // 3. `rgctl -f json update --no-wait` must enqueue and return a receipt.
    let nowait_out = Command::new(bin)
        .current_dir(&repo)
        .args(["-f", "json", "update", "--no-wait"])
        .output()
        .unwrap();
    assert!(
        nowait_out.status.success(),
        "update --no-wait failed: {}",
        String::from_utf8_lossy(&nowait_out.stderr)
    );
    let nowait_json: serde_json::Value = serde_json::from_slice(&nowait_out.stdout).unwrap();
    assert_eq!(nowait_json["queued"], serde_json::Value::Bool(true));
    assert!(
        nowait_json["request_id"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "expected request_id in {nowait_json}"
    );

    // 4. Waiting `rgctl -f json update` after editing a file must drain via `watch_queue`.
    fs::write(
        repo.join("tiny.py"),
        "def ping():\n    return 2\n\ndef pong():\n    return ping()\n",
    )
    .unwrap();
    let upd_out = Command::new(bin)
        .current_dir(&repo)
        .args(["-f", "json", "update"])
        .output()
        .unwrap();
    assert!(
        upd_out.status.success(),
        "queued update failed: {}",
        String::from_utf8_lossy(&upd_out.stderr)
    );
    let upd_json: serde_json::Value = serde_json::from_slice(&upd_out.stdout).unwrap();
    assert_eq!(
        upd_json["source"],
        serde_json::Value::String("watch_queue".into())
    );

    // 5. On Unix, SIGTERM triggers graceful shutdown and removes `.rgctl/watch.lock`.
    #[cfg(unix)]
    {
        let pid = watcher.0.id().to_string();
        let _ = Command::new("kill").args(["-TERM", &pid]).status();
        let stop_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(exit) = watcher.0.try_wait().unwrap() {
                assert!(exit.success(), "expected clean SIGTERM exit, got {exit}");
                break;
            }
            assert!(
                Instant::now() < stop_deadline,
                "timed out waiting for rgctl watch to exit on SIGTERM"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !lock_file.exists(),
            "expected .rgctl/watch.lock to be removed on graceful shutdown"
        );
    }
}

/// Stress-tests a live `rgctl watch` daemon under simultaneous:
/// 1. Rapid filesystem edits across 10 files (debounce + coalescing),
/// 2. 8 concurrent CLI `rgctl update` / `--files` / `--no-wait` queue writers racing `drain_queue`,
/// 3. 4 concurrent CLI reader loops (`find` + `status`) opening `.rgctl/graph.snapshot.bin` mid-compact.
#[test]
fn test_watch_heavy_fs_queue_and_reader_contention() {
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let tmp = TempDir::new().unwrap();
    let repo = tmp
        .path()
        .canonicalize()
        .unwrap_or_else(|_| tmp.path().to_path_buf());
    for i in 0..10 {
        fs::write(
            repo.join(format!("mod_{i}.py")),
            format!("def sym_{i}_v0():\n    return {i}\n"),
        )
        .unwrap();
    }
    fs::write(
        repo.join("hub.py"),
        "def hub_entry():\n    return sym_0_v0()\n",
    )
    .unwrap();

    let bin = env!("CARGO_BIN_EXE_rgctl");
    let discover = Command::new(bin)
        .current_dir(&repo)
        .args(["discover", "."])
        .output()
        .unwrap();
    assert!(
        discover.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&discover.stderr)
    );

    let mut watcher = ChildGuard(
        Command::new(bin)
            .current_dir(&repo)
            .arg("watch")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );

    let lock_file = rgctl_service::status::watch_lock_path(&repo);
    let wait_lock = Instant::now() + Duration::from_secs(5);
    while !lock_file.is_file() {
        assert!(
            Instant::now() < wait_lock,
            "timed out waiting for watch.lock"
        );
        std::thread::sleep(Duration::from_millis(15));
    }

    let stop_readers = Arc::new(AtomicBool::new(false));
    let reader_queries = Arc::new(AtomicUsize::new(0));

    // Spawn 4 concurrent reader threads hammering `find` and `status` across atomic snapshot swaps.
    let mut reader_handles = Vec::new();
    for reader_idx in 0..4 {
        let repo_r = repo.clone();
        let stop_r = Arc::clone(&stop_readers);
        let count_r = Arc::clone(&reader_queries);
        reader_handles.push(std::thread::spawn(move || {
            while !stop_r.load(Ordering::Relaxed) {
                let args: &[&str] = if reader_idx % 2 == 0 {
                    &["-f", "json", "find", "hub_entry"]
                } else {
                    &["-f", "json", "status"]
                };
                let out = Command::new(bin)
                    .current_dir(&repo_r)
                    .args(args)
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "concurrent reader `{args:?}` failed mid-compact (ENOENT/torn snapshot): {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let parsed: serde_json::Value =
                    serde_json::from_slice(&out.stdout).expect("reader output must be valid JSON");
                assert!(
                    parsed["schema_version"].as_u64().is_some_and(|v| v >= 1),
                    "expected schema_version in {parsed}"
                );
                count_r.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // Spawn rapid FS writer editing all 10 files across 3 waves.
    let repo_fs = repo.clone();
    let fs_writer = std::thread::spawn(move || {
        for wave in 1..=3 {
            for i in 0..10 {
                fs::write(
                    repo_fs.join(format!("mod_{i}.py")),
                    format!("def sym_{i}_v{wave}():\n    return {}\n", i + wave),
                )
                .unwrap();
            }
            fs::write(
                repo_fs.join("hub.py"),
                format!("def hub_entry():\n    return sym_0_v{wave}()\n"),
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(30));
        }
    });

    // Spawn 8 concurrent CLI `rgctl update` callers racing `enqueue` against `drain_queue`.
    let mut queue_handles = Vec::new();
    for q_idx in 0..8 {
        let repo_q = repo.clone();
        queue_handles.push(std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis((q_idx * 12) as u64));
            let args: Vec<&str> = match q_idx % 3 {
                0 => vec!["-f", "json", "update", "--wait-timeout", "15"],
                1 => vec![
                    "-f",
                    "json",
                    "update",
                    "--files",
                    "mod_0.py",
                    "--wait-timeout",
                    "15",
                ],
                _ => vec!["-f", "json", "update", "--no-wait"],
            };
            let out = Command::new(bin)
                .current_dir(&repo_q)
                .args(&args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "concurrent queued update #{q_idx} (`{args:?}`) failed or timed out: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            if q_idx % 3 == 2 {
                assert_eq!(json["queued"], serde_json::json!(true));
            } else {
                assert_eq!(json["source"], serde_json::json!("watch_queue"));
            }
        }));
    }

    fs_writer.join().expect("fs writer thread panicked");
    for h in queue_handles {
        h.join().expect("queued update thread panicked");
    }

    // Final barrier update through `watch_queue` so wave 3 edits are guaranteed committed.
    let barrier = Command::new(bin)
        .current_dir(&repo)
        .args(["-f", "json", "update", "--wait-timeout", "15"])
        .output()
        .unwrap();
    assert!(
        barrier.status.success(),
        "barrier update failed: {}",
        String::from_utf8_lossy(&barrier.stderr)
    );

    stop_readers.store(true, Ordering::Relaxed);
    for h in reader_handles {
        h.join().expect("reader thread panicked");
    }
    assert!(
        reader_queries.load(Ordering::Relaxed) >= 4,
        "expected concurrent readers to execute during contention window"
    );

    // Verify final graph reflects wave-3 symbols (`sym_9_v3`) and dropped wave-0 symbols (`sym_9_v0`).
    let find_v3 = Command::new(bin)
        .current_dir(&repo)
        .args(["-f", "json", "find", "sym_9_v3"])
        .output()
        .unwrap();
    assert!(find_v3.status.success());
    let v3_json: serde_json::Value = serde_json::from_slice(&find_v3.stdout).unwrap();
    assert_eq!(
        v3_json["returned"],
        serde_json::json!(1),
        "expected sym_9_v3 in compacted snapshot after contention storm, got {v3_json}"
    );

    let find_v0 = Command::new(bin)
        .current_dir(&repo)
        .args(["-f", "json", "find", "sym_9_v0"])
        .output()
        .unwrap();
    assert!(find_v0.status.success());
    let v0_json: serde_json::Value = serde_json::from_slice(&find_v0.stdout).unwrap();
    assert_eq!(
        v0_json["returned"],
        serde_json::json!(0),
        "stale sym_9_v0 must not remain after contention storm, got {v0_json}"
    );

    #[cfg(unix)]
    {
        let pid = watcher.0.id().to_string();
        let _ = Command::new("kill").args(["-TERM", &pid]).status();
        let stop_deadline = Instant::now() + Duration::from_secs(5);
        while watcher.0.try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < stop_deadline,
                "watch did not exit on SIGTERM"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!lock_file.exists(), "watch.lock must be removed on exit");
    }
}
