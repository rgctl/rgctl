//! Phase 8 integration tests: parallel processing, batch APIs, query optimization
#![allow(dead_code, unused_imports, unused_macros)]

use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::query::{execute, execute_chunks};
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::CodeGraph;
use rbuilder::incremental::{FileTracker, IncrementalUpdater, UpdateOptions};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::pipeline::{PipelineConfig, ProcessingPipeline};
use std::collections::HashSet;
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
fn test_batch_load_populates_graph() {
    let nodes: Vec<_> = (0..500)
        .map(|i| Node::new(NodeType::Function, format!("fn{i}")))
        .collect();

    let mut graph = CodeGraph::new();
    graph.load(nodes, vec![]).unwrap();

    assert_eq!(graph.node_count(), 500);
    let functions = graph.find_by_type(NodeType::Function).unwrap();
    assert_eq!(functions.len(), 500);
}

#[test]
fn test_batch_load_with_edges() {
    let n1 = Node::new(NodeType::Function, "caller".to_string());
    let n2 = Node::new(NodeType::Function, "callee".to_string());
    let id1 = n1.id;
    let id2 = n2.id;

    let mut graph = CodeGraph::new();
    graph
        .load(vec![n1, n2], vec![Edge::new(id1, id2, EdgeType::Calls)])
        .unwrap();

    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn test_batch_insert_equivalent_to_individual() {
    let nodes: Vec<_> = (0..5_000)
        .map(|i| Node::new(NodeType::Function, format!("fn{i}")))
        .collect();

    let mut batch_graph = CodeGraph::new();
    batch_graph.load(nodes.clone(), vec![]).unwrap();

    let mut single_graph = CodeGraph::new();
    for node in &nodes {
        single_graph
            .backend_mut()
            .insert_node(node.clone())
            .unwrap();
    }

    assert_eq!(batch_graph.node_count(), 5_000);
    assert_eq!(single_graph.node_count(), 5_000);
    // Throughput comparison lives in benches/graph.rs (insert_nodes single vs batch).
}
#[test]
fn test_parallel_pipeline_many_files() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    for i in 0..25 {
        write(
            &root.join(format!("src/module{i}.rs")),
            &format!("pub fn func{i}() {{}}\n"),
        );
    }

    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            thread_count: Some(4),
            ..PipelineConfig::default()
        },
    );

    let start = Instant::now();
    let (graph, stats) = pipeline.process_repository(root).unwrap();
    let duration = start.elapsed();

    assert_eq!(stats.files_processed, 25);
    assert!(graph.node_count() >= 25);
    let names: std::collections::HashSet<_> = graph
        .find_by_type(NodeType::Function)
        .unwrap()
        .into_iter()
        .map(|n| n.name)
        .collect();
    for i in 0..25 {
        assert!(names.contains(&format!("func{i}")));
    }
    assert!(
        duration < Duration::from_secs(5),
        "parallel pipeline too slow: {duration:?}"
    );
}
#[test]
fn test_parallel_incremental_update_many_files() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    for i in 0..20 {
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
            thread_count: Some(4),
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

    for i in 0..20 {
        write(
            &root.join(format!("src/file{i}.rs")),
            &format!("fn func{i}() {{}}\nfn extra{i}() {{}}\n"),
        );
    }

    let updater = IncrementalUpdater::with_options(
        registry,
        UpdateOptions {
            show_progress: false,
            thread_count: Some(4),
            ..Default::default()
        },
    );

    let start = Instant::now();
    let result = updater.update(&mut graph, root).unwrap();
    let duration = start.elapsed();

    assert!(result.files_changed >= 20 || result.nodes_added > 0);
    let names: std::collections::HashSet<_> = graph
        .find_by_type(NodeType::Function)
        .unwrap()
        .into_iter()
        .map(|n| n.name)
        .collect();
    for i in 0..20 {
        assert!(names.contains(&format!("extra{i}")));
    }
    assert!(
        duration < Duration::from_secs(5),
        "parallel incremental update too slow: {duration:?}"
    );
}

#[test]
fn test_repo_query_performance_with_property_index() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    for i in 0..10_000 {
        let repo = if i % 2 == 0 { "backend" } else { "frontend" };
        backend
            .insert_node(
                Node::new(NodeType::Function, format!("handler{i}"))
                    .with_property("repo".into(), repo.into()),
            )
            .unwrap();
    }

    let start = Instant::now();
    let results = execute(backend, "repo:backend").unwrap();
    let duration = start.elapsed();

    assert_eq!(results.len(), 5_000);
    assert!(
        duration < Duration::from_millis(50),
        "repo: query too slow: {duration:?}"
    );
}

#[test]
fn test_compound_repo_and_type_query() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    backend
        .insert_node(
            Node::new(NodeType::Function, "api_main".into())
                .with_property("repo".into(), "backend".into()),
        )
        .unwrap();
    backend
        .insert_node(
            Node::new(NodeType::Class, "ApiService".into())
                .with_property("repo".into(), "backend".into()),
        )
        .unwrap();
    backend
        .insert_node(
            Node::new(NodeType::Function, "ui_main".into())
                .with_property("repo".into(), "frontend".into()),
        )
        .unwrap();

    let results = execute(backend, "repo:backend|type:Function").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "api_main");
}

#[test]
fn test_execute_chunks_large_result() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    for i in 0..250 {
        backend
            .insert_node(Node::new(NodeType::Function, format!("fn{i}")))
            .unwrap();
    }

    let chunks = execute_chunks(backend, "functions", 100).unwrap();
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].len(), 100);
    assert_eq!(chunks[1].len(), 100);
    assert_eq!(chunks[2].len(), 50);
    assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), 250);
}

/// Build a graph where compound filters have very different selectivity.
fn build_selectivity_graph() -> CodeGraph {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    // 4,500 backend functions (broad type:Function + repo:backend)
    for i in 0..4_500 {
        backend
            .insert_node(
                Node::new(NodeType::Function, format!("handler{i}"))
                    .with_property("repo".into(), "backend".into()),
            )
            .unwrap();
    }

    // 500 backend classes
    for i in 0..500 {
        backend
            .insert_node(
                Node::new(NodeType::Class, format!("Service{i}"))
                    .with_property("repo".into(), "backend".into()),
            )
            .unwrap();
    }

    // 5,000 frontend functions (same type, different repo)
    for i in 0..5_000 {
        backend
            .insert_node(
                Node::new(NodeType::Function, format!("ui{i}"))
                    .with_property("repo".into(), "frontend".into()),
            )
            .unwrap();
    }

    // Single unique target
    backend
        .insert_node(
            Node::new(NodeType::Function, "needle".into())
                .with_property("repo".into(), "backend".into()),
        )
        .unwrap();

    graph
}

fn name_set(nodes: &[Node]) -> HashSet<String> {
    nodes.iter().map(|n| n.name.clone()).collect()
}

#[test]
fn test_compound_query_clause_order_invariant() {
    let graph = build_selectivity_graph();
    let backend = graph.backend();

    let queries = [
        "repo:backend|type:Function|name:needle",
        "type:Function|name:needle|repo:backend",
        "name:needle|repo:backend|type:Function",
        "name:needle|type:Function|repo:backend",
        "repo:backend|name:needle|type:Function",
        "type:Function|repo:backend|name:needle",
    ];

    let baseline = name_set(&execute(backend, queries[0]).unwrap());
    assert_eq!(baseline.len(), 1);
    assert_eq!(execute(backend, queries[0]).unwrap()[0].name, "needle");

    for query in &queries[1..] {
        assert_eq!(
            name_set(&execute(backend, query).unwrap()),
            baseline,
            "order should not change results for {query}"
        );
    }
}

#[test]
fn test_compound_query_selectivity_narrow_name_broad_type() {
    let graph = build_selectivity_graph();
    let backend = graph.backend();

    // Broad clauses first in user input — engine reorders by selectivity rank
    let broad_first = execute(backend, "type:Function|name:needle").unwrap();
    let name_first = execute(backend, "name:needle|type:Function").unwrap();

    assert_eq!(broad_first.len(), 1);
    assert_eq!(name_first.len(), 1);
    assert_eq!(name_set(&broad_first), name_set(&name_first));
    assert_eq!(broad_first[0].name, "needle");
}

#[test]
fn test_compound_query_via_code_graph_e2e() {
    let graph = build_selectivity_graph();

    let permutations = ["repo:backend|type:Class", "type:Class|repo:backend"];

    let baseline = name_set(&graph.query(permutations[0]).unwrap());
    assert_eq!(baseline.len(), 500);

    for query in &permutations[1..] {
        assert_eq!(name_set(&graph.query(query).unwrap()), baseline);
    }

    let needle = graph
        .query("name:needle|repo:backend|type:Function")
        .unwrap();
    assert_eq!(needle.len(), 1);
    assert_eq!(needle[0].name, "needle");
}

#[test]
fn test_compound_query_selectivity_beats_unordered_intersection() {
    let graph = build_selectivity_graph();
    let backend = graph.backend();

    // Sanity: individual clause sizes confirm selectivity spread
    assert_eq!(execute(backend, "name:needle").unwrap().len(), 1);
    assert!(execute(backend, "type:Function").unwrap().len() > 9_000);
    assert_eq!(execute(backend, "repo:backend").unwrap().len(), 5_001);

    let start = Instant::now();
    let result = execute(backend, "type:Function|repo:backend|name:needle").unwrap();
    let duration = start.elapsed();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "needle");
    assert!(
        duration < Duration::from_millis(100),
        "compound selectivity query too slow: {duration:?}"
    );
}
