//! Phase 1 end-to-end integration tests
#![allow(dead_code, unused_imports, unused_macros)]

use rbuilder::graph::schema::NodeType;
use rbuilder::graph::CodeGraph;
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::pipeline::{PipelineConfig, ProcessingPipeline};
use std::fs;
use tempfile::TempDir;

fn write(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
#[cfg(feature = "bundle-extended")]
fn test_end_to_end_multi_language_repo() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    write(
        &root.join("src/main.rs"),
        "fn main() { helper(); }\nfn helper() {}\n",
    );
    write(
        &root.join("app.py"),
        "import os\ndef run():\n    host = os.environ['DB_HOST']\n",
    );
    write(&root.join("config.yaml"), "database:\n  host: localhost\n");
    write(&root.join("README.md"), "# Demo\n\n## Setup\n");

    let pipeline = ProcessingPipeline::with_config(
        LanguageRegistry::new().into(),
        PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        },
    );

    let (graph, stats) = pipeline.process_repository(root).unwrap();

    assert!(stats.files_processed >= 4);
    assert!(graph.node_count() > stats.files_processed);
    assert!(!graph.find_by_type(NodeType::Function).unwrap().is_empty());
    assert!(!graph.find_by_type(NodeType::ConfigKey).unwrap().is_empty());
}

#[test]
fn test_init_save_and_query() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(
        &root.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    );

    let graph = rbuilder::code_graph_from_repository(root).unwrap();
    graph.save_to_repo(root).unwrap();

    let loaded = CodeGraph::load_from_repo(root).unwrap();
    let functions = loaded.query("functions").unwrap();
    assert!(!functions.is_empty());
}

#[test]
fn test_gitignore_excluded_from_graph() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(&root.join("src/lib.rs"), "pub fn kept() {}\n");
    write(&root.join("target/generated.rs"), "pub fn skipped() {}\n");
    write(&root.join(".gitignore"), "target/\n");

    let graph = rbuilder::code_graph_from_repository(root).unwrap();
    let functions: Vec<_> = graph
        .find_by_type(NodeType::Function)
        .unwrap()
        .into_iter()
        .map(|n| n.name)
        .collect();

    assert!(functions.contains(&"kept".to_string()));
    assert!(!functions.contains(&"skipped".to_string()));
}
