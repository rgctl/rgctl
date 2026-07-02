//! Phase 13: change detection and incremental updates.

use rbuilder::changes::ChangeDetector;
use rbuilder::config::project::{RbuilderConfig, RiskLevel};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::incremental::{changes_for_paths, IncrementalUpdater, UpdateOptions};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::pipeline::{PipelineConfig, ProcessingPipeline};
use std::fs;
use tempfile::TempDir;

fn chain_graph_repo(temp: &TempDir) -> rbuilder::CodeGraph {
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

    let mut tracker = rbuilder::incremental::FileTracker::new(root);
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
    let mut graph = rbuilder::CodeGraph::new();
    let backend = graph.backend_mut();
    let a = Node::new(NodeType::Function, "a".into()).with_file_path("src/lib.rs".into());
    let b = Node::new(NodeType::Function, "b".into()).with_file_path("src/lib.rs".into());
    let c = Node::new(NodeType::Function, "c".into()).with_file_path("src/lib.rs".into());
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
fn test_rbuilder_config_defaults() {
    let temp = TempDir::new().unwrap();
    let cfg = RbuilderConfig::load(temp.path()).unwrap();
    assert_eq!(cfg.hooks.block_on_risk, RiskLevel::Critical);
    assert_eq!(cfg.watch.debounce_ms, 500);
}

#[test]
fn test_manual_graph_blast_risk() {
    let mut graph = rbuilder::CodeGraph::new();
    let backend = graph.backend_mut();
    let a = Node::new(NodeType::Function, "a".into()).with_file_path("f.rs".into());
    let b = Node::new(NodeType::Function, "b".into()).with_file_path("f.rs".into());
    let c = Node::new(NodeType::Function, "c".into()).with_file_path("f.rs".into());
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
    use rbuilder::config::project::RbuilderConfig;
    let cfg = RbuilderConfig::default();
    assert!(cfg.hooks.block_on_risk.blocks(RiskLevel::Critical));
    assert!(!cfg.hooks.block_on_risk.blocks(RiskLevel::Medium));
}

#[test]
fn test_high_risk_blocked_when_configured() {
    use rbuilder::config::project::{RbuilderConfig, RiskLevel};
    let mut cfg = RbuilderConfig::default();
    cfg.hooks.block_on_risk = RiskLevel::High;
    assert!(cfg.hooks.block_on_risk.blocks(RiskLevel::Critical));
    assert!(cfg.hooks.block_on_risk.blocks(RiskLevel::High));
    assert!(!cfg.hooks.block_on_risk.blocks(RiskLevel::Medium));
}

#[test]
fn test_detect_changes_json_contains_summary() {
    let mut graph = rbuilder::CodeGraph::new();
    let backend = graph.backend_mut();
    let leaf = Node::new(NodeType::Function, "leaf".into()).with_file_path("f.rs".into());
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
