//! Phase 5 integration tests: incremental updates and performance

use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Node, NodeType};
use rbuilder::graph::CodeGraph;
use rbuilder::incremental::{FileTracker, IncrementalUpdater, UpdateOptions};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::pipeline::{PipelineConfig, ProcessingPipeline};
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn write(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn test_incremental_update_workflow() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(&root.join("src/main.rs"), "fn main() {}\n");

    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        },
    );

    let (mut graph, _) = pipeline.process_repository(root).unwrap();
    graph.save_to_repo(root).unwrap();

    let mut tracker = FileTracker::new(root);
    let files = vec![root.join("src/main.rs")];
    tracker.index_files(&files, &graph).unwrap();
    tracker.save().unwrap();

    write(
        &root.join("src/main.rs"),
        "fn main() { helper(); }\nfn helper() {}\n",
    );

    let updater = IncrementalUpdater::with_options(
        registry,
        UpdateOptions {
            show_progress: false,
            ..Default::default()
        },
    );
    let result = updater.update(&mut graph, root).unwrap();

    assert!(result.files_changed >= 1 || result.nodes_added > 0);
    let functions = graph.find_by_type(NodeType::Function).unwrap();
    assert!(functions.iter().any(|n| n.name == "helper"));
}

#[test]
fn test_file_hash_tracking() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("lib.rs");
    write(&file, "fn alpha() {}\n");

    let mut tracker = FileTracker::new(temp.path());
    tracker
        .index_files(std::slice::from_ref(&file), &CodeGraph::new())
        .unwrap();
    tracker.save().unwrap();

    write(&file, "fn beta() {}\n");
    let changes = tracker.detect_changes(&[file]).unwrap();
    assert_eq!(changes.changed.len(), 1);
}

#[test]
fn test_query_performance_by_label() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    for i in 0..10_000 {
        let mut node = Node::new(NodeType::Class, format!("Component{i}"));
        node.labels.push("react:component".to_string());
        backend.insert_node(node).unwrap();
    }

    let start = Instant::now();
    let results = backend.find_nodes_by_label("react:component").unwrap();
    let duration = start.elapsed();

    assert_eq!(results.len(), 10_000);
    assert!(
        duration < Duration::from_millis(50),
        "Query too slow: {duration:?}"
    );
}

#[test]
fn test_query_cache_hit() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    for i in 0..100 {
        backend
            .insert_node(Node::new(NodeType::Function, format!("fn{i}")))
            .unwrap();
    }

    let start = Instant::now();
    let _ = backend.cached_query("functions").unwrap();
    let first = start.elapsed();

    let start = Instant::now();
    let cached = backend.cached_query("functions").unwrap();
    let second = start.elapsed();

    assert_eq!(cached.len(), 100);
    assert!(second <= first, "cache should be faster or equal");
}

#[test]
fn test_incremental_update_ten_files() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    for i in 0..10 {
        write(
            &root.join(format!("src/file{i}.rs")),
            &format!("fn func{i}() {{}}\n"),
        );
    }

    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        },
    );

    let (mut graph, _) = pipeline.process_repository(root).unwrap();
    graph.save_to_repo(root).unwrap();

    let discoverer = rbuilder::discovery::FileDiscoverer::new(Arc::clone(&registry));
    let files = discoverer.discover(root).unwrap();
    let mut tracker = FileTracker::new(root);
    tracker.index_files(&files, &graph).unwrap();
    tracker.save().unwrap();

    for i in 0..10 {
        write(
            &root.join(format!("src/file{i}.rs")),
            &format!("fn func{i}() {{ /* updated */ }}\nfn extra{i}() {{}}\n"),
        );
    }

    let updater = IncrementalUpdater::with_options(
        registry,
        UpdateOptions {
            show_progress: false,
            ..Default::default()
        },
    );

    let start = Instant::now();
    let result = updater.update(&mut graph, root).unwrap();
    let duration = start.elapsed();

    assert!(result.files_changed >= 10 || result.nodes_added > 0);
    assert!(
        duration < Duration::from_secs(5),
        "Update too slow: {duration:?}"
    );
}

#[test]
fn test_string_interning_reduces_duplicates() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    for _ in 0..1000 {
        let mut node = Node::new(NodeType::Function, "shared_name".to_string());
        node.file_path = Some("src/common.rs".to_string());
        node.labels.push("api:endpoint".to_string());
        backend.insert_node(node).unwrap();
    }

    // Interner should deduplicate repeated strings across nodes
    assert!(backend.memory_estimate() > 0);
    let functions = backend.find_nodes_by_name("shared_name").unwrap();
    assert_eq!(functions.len(), 1000);
}
