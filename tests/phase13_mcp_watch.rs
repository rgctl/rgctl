//! Phase 13.1.2: MCP watch integration tests.

#![cfg(feature = "mcp-server")]

use rbuilder::api::state::AppState;
use rbuilder::config::project::RiskLevel;
use rbuilder::incremental::{IncrementalUpdater, UpdateOptions};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::mcp::protocol::graph_updated_notification;
use rbuilder::pipeline::{PipelineConfig, ProcessingPipeline};
use rbuilder::watch::{
    latest_notification, new_notification_store, record_notification, GraphUpdateNotification,
};
use std::fs;
use tempfile::TempDir;

fn sample_repo() -> (TempDir, AppState) {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

    let pipeline = ProcessingPipeline::with_config(
        LanguageRegistry::new().into(),
        PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        },
    );
    let (graph, _) = pipeline.process_repository(root).unwrap();
    graph.save_to_repo(root).unwrap();

    let state = AppState::from_repo(root).unwrap();
    (temp, state)
}

#[test]
fn test_mcp_state_reflects_incremental_update() {
    let (_temp, state) = sample_repo();
    let root = state.repo_root();
    let before = state
        .with_graph(|g| Ok(g.backend().all_nodes()?.len()))
        .unwrap();

    fs::write(
        root.join("src/lib.rs"),
        "pub fn hello() {}\npub fn world() {}\n",
    )
    .unwrap();

    state
        .with_graph_mut(|graph| {
            let updater = IncrementalUpdater::with_options(
                LanguageRegistry::new().into(),
                UpdateOptions {
                    show_progress: false,
                    ..Default::default()
                },
            );
            updater.update_files(graph, &root, &["src/lib.rs".into()])
        })
        .unwrap();

    let after = state
        .with_graph(|g| Ok(g.backend().all_nodes()?.len()))
        .unwrap();
    assert!(after >= before);
}

#[test]
fn test_notification_store_for_http_clients() {
    let store = new_notification_store();
    let notification = GraphUpdateNotification {
        timestamp: 42,
        files_changed: vec!["src/lib.rs".into()],
        nodes_added: 1,
        nodes_removed: 0,
        edges_changed: 2,
    };
    record_notification(&store, notification.clone());
    let latest = latest_notification(&store).unwrap();
    assert_eq!(latest.files_changed, notification.files_changed);
    let wire = graph_updated_notification(&latest).unwrap();
    assert!(wire.contains("notifications/graph_updated"));
}

#[test]
fn test_critical_risk_blocks_commit_threshold() {
    use rbuilder::config::project::RbuilderConfig;
    let cfg = RbuilderConfig::default();
    assert!(cfg.hooks.block_on_risk.blocks(RiskLevel::Critical));
    assert!(!cfg.hooks.block_on_risk.blocks(RiskLevel::High));
}

#[test]
fn test_graph_saved_before_mcp_load() {
    let (temp, state) = sample_repo();
    let count = state
        .with_graph(|g| Ok(g.backend().all_nodes()?.len()))
        .unwrap();
    assert!(count > 0);

    let reloaded = AppState::from_repo(temp.path()).unwrap();
    let reloaded_count = reloaded
        .with_graph(|g| {
            let nodes = g.backend().all_nodes()?;
            Ok(nodes.iter().filter(|n| n.name == "hello").count())
        })
        .unwrap();
    assert_eq!(reloaded_count, 1);
}
