//! Phase 10 integration tests: multi-repo, config drift, workspace
//!
//! Note: This is early implementation of Phase 10 features (originally scheduled for Week 28+).
//! Implemented ahead of schedule and will be refined after Phase 7-9 complete.

use rbuilder::config::drift::compare_configs;
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::query;
use rbuilder::graph::schema::{Node, NodeType};
use rbuilder::graph::CodeGraph;
use rbuilder::multi_repo::{
    link_cross_repo, stamp_repo_namespace, sync_workspace, WorkspaceManifest,
};
use std::fs;
use tempfile::TempDir;

fn write(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn test_config_drift_cli_paths() {
    let temp = TempDir::new().unwrap();
    write(&temp.path().join("prod.yaml"), "server:\n  port: 8080\n");
    write(&temp.path().join("dev.yaml"), "server:\n  port: 3000\n");

    let report = compare_configs(
        &temp.path().join("prod.yaml"),
        &temp.path().join("dev.yaml"),
    )
    .unwrap();
    assert!(!report.is_clean());
    assert!(report.changed.iter().any(|e| e.key == "server.port"));
}

#[test]
fn test_multi_repo_namespace_stamping() {
    let mut graph = CodeGraph::new();
    graph
        .backend_mut()
        .insert_node(Node::new(NodeType::Function, "handler".into()))
        .unwrap();

    stamp_repo_namespace(&mut graph, "api");
    let nodes = graph.backend().all_nodes().unwrap();
    assert!(nodes
        .iter()
        .all(|n| n.get_property("repo") == Some(&"api".to_string())));
}

#[test]
fn test_query_repo_filter() {
    let mut graph = CodeGraph::new();
    graph
        .backend_mut()
        .insert_node(
            Node::new(NodeType::Function, "a".into())
                .with_property("repo".into(), "backend".into()),
        )
        .unwrap();
    graph
        .backend_mut()
        .insert_node(
            Node::new(NodeType::Function, "b".into())
                .with_property("repo".into(), "frontend".into()),
        )
        .unwrap();

    let results = query::execute(graph.backend(), "repo:backend").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "a");
}

#[test]
fn test_compound_repo_query() {
    let mut graph = CodeGraph::new();
    graph
        .backend_mut()
        .insert_node(
            Node::new(NodeType::Function, "main".into())
                .with_property("repo".into(), "backend".into()),
        )
        .unwrap();
    graph
        .backend_mut()
        .insert_node(
            Node::new(NodeType::Class, "App".into()).with_property("repo".into(), "backend".into()),
        )
        .unwrap();

    let results = query::execute(graph.backend(), "repo:backend|type:Function").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "main");
}

#[test]
fn test_cross_repo_linking() {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    let import = Node::new(NodeType::Import, "SharedLib".into())
        .with_property("repo".into(), "client".into());
    let lib =
        Node::new(NodeType::Class, "SharedLib".into()).with_property("repo".into(), "lib".into());
    backend.insert_node(import).unwrap();
    backend.insert_node(lib).unwrap();

    let report = link_cross_repo(&mut backend).unwrap();
    assert!(report.edges_added >= 1);
}

#[test]
fn test_workspace_sync() {
    rbuilder::init();
    let workspace = TempDir::new().unwrap();
    let repo_a = TempDir::new().unwrap();
    let repo_b = TempDir::new().unwrap();

    write(&repo_a.path().join("lib.rs"), "pub fn api_handler() {}\n");
    write(
        &repo_b.path().join("main.rs"),
        "fn main() { println!(\"client\"); }\n",
    );

    let mut manifest = WorkspaceManifest::default();
    manifest
        .add_repo("backend", repo_a.path().to_path_buf())
        .unwrap();
    manifest
        .add_repo("frontend", repo_b.path().to_path_buf())
        .unwrap();
    manifest.save(workspace.path()).unwrap();

    let (graph, report) = sync_workspace(workspace.path(), false).unwrap();
    assert_eq!(report.repos_indexed, 2);
    assert!(graph.node_count() > 0);

    let backend_nodes = graph.backend().all_nodes().unwrap();
    let repos: std::collections::HashSet<_> = backend_nodes
        .iter()
        .filter_map(|n| n.get_property("repo"))
        .collect();
    assert!(repos.contains(&"backend".to_string()));
    assert!(repos.contains(&"frontend".to_string()));
}
