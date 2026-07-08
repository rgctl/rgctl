//! Integration tests for core rBuilder features
//!
//! These tests gate critical functionality to prevent regressions:
//! 1. Edge extraction (Calls, Implements, Extends)
//! 2. Analysis result persistence
//! 3. Query functionality
//!
//! All tests use real example repositories to validate end-to-end behavior.

use rbuilder_analysis::{CentralityAnalyzer, CommunityDetector, ComplexityAnalyzer};
use rbuilder_graph::backend::GraphBackend;
use rbuilder_graph::code_graph::CodeGraph;
use rbuilder_graph::schema::{EdgeType, NodeType};
use rbuilder_pipeline::{PipelineConfig, ProcessingPipeline};
use std::sync::Arc;
use tempfile::TempDir;

/// Test fixture: Simple Java example with method calls and inheritance
fn create_java_test_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    std::fs::create_dir(&src).unwrap();

    // Base interface
    std::fs::write(
        src.join("Service.java"),
        r#"
public interface Service {
    String process(String input);
}
"#,
    )
    .unwrap();

    // Implementation with method calls
    std::fs::write(
        src.join("ServiceImpl.java"),
        r#"
public class ServiceImpl implements Service {
    private Helper helper = new Helper();

    @Override
    public String process(String input) {
        return helper.transform(input);
    }
}
"#,
    )
    .unwrap();

    // Helper class
    std::fs::write(
        src.join("Helper.java"),
        r#"
public class Helper {
    public String transform(String value) {
        return validate(value);
    }

    private String validate(String value) {
        return value.toUpperCase();
    }
}
"#,
    )
    .unwrap();

    // Inheritance example
    std::fs::write(
        src.join("BaseClass.java"),
        r#"
public class BaseClass {
    protected void baseMethod() {}
}
"#,
    )
    .unwrap();

    std::fs::write(
        src.join("DerivedClass.java"),
        r#"
public class DerivedClass extends BaseClass {
    @Override
    protected void baseMethod() {
        super.baseMethod();
    }
}
"#,
    )
    .unwrap();

    temp
}

#[test]
fn test_edge_extraction_calls() {
    let repo = create_java_test_repo();
    let registry = Arc::new(rbuilder_languages::default_registry());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );

    let (graph, _) = pipeline.process_repository(repo.path()).unwrap();
    let backend = graph.backend();

    // Count Calls edges
    let all_edges = backend.all_edges().unwrap();
    let calls_edges: Vec<_> = all_edges
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Calls))
        .collect();

    // CRITICAL: Must have Calls edges
    assert!(
        !calls_edges.is_empty(),
        "REGRESSION: No Calls edges found! Java plugin extract_relations may be broken.\n\
         Expected edges: ServiceImpl.process() -> Helper.transform(), Helper.transform() -> Helper.validate()\n\
         Found: {} edges total, 0 Calls edges",
        all_edges.len()
    );

    // Verify specific call relationships exist
    let calls_count = calls_edges.len();
    assert!(
        calls_count >= 2,
        "Expected at least 2 Calls edges (process->transform, transform->validate), found {calls_count}"
    );

    println!("✓ Calls edge extraction working: {calls_count} edges found");
}

#[test]
fn test_edge_extraction_implements() {
    let repo = create_java_test_repo();
    let registry = Arc::new(rbuilder_languages::default_registry());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );

    let (graph, _) = pipeline.process_repository(repo.path()).unwrap();
    let backend = graph.backend();

    // Count Implements edges
    let all_edges = backend.all_edges().unwrap();
    let implements_edges: Vec<_> = all_edges
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Implements))
        .collect();

    // CRITICAL: Must have Implements edges
    assert!(
        !implements_edges.is_empty(),
        "REGRESSION: No Implements edges found! Java plugin extract_relations may be broken.\n\
         Expected: ServiceImpl implements Service\n\
         Found: {} edges total, 0 Implements edges",
        all_edges.len()
    );

    println!(
        "✓ Implements edge extraction working: {} edges found",
        implements_edges.len()
    );
}

#[test]
fn test_edge_extraction_extends() {
    let repo = create_java_test_repo();
    let registry = Arc::new(rbuilder_languages::default_registry());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );

    let (graph, _) = pipeline.process_repository(repo.path()).unwrap();
    let backend = graph.backend();

    // Count Extends edges
    let all_edges = backend.all_edges().unwrap();
    let extends_edges: Vec<_> = all_edges
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Extends))
        .collect();

    // CRITICAL: Must have Extends edges
    assert!(
        !extends_edges.is_empty(),
        "REGRESSION: No Extends edges found! Java plugin extract_relations may be broken.\n\
         Expected: DerivedClass extends BaseClass\n\
         Found: {} edges total, 0 Extends edges",
        all_edges.len()
    );

    println!(
        "✓ Extends edge extraction working: {} edges found",
        extends_edges.len()
    );
}

#[test]
fn test_edge_type_diversity() {
    let repo = create_java_test_repo();
    let registry = Arc::new(rbuilder_languages::default_registry());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );

    let (graph, _) = pipeline.process_repository(repo.path()).unwrap();
    let backend = graph.backend();

    let all_edges = backend.all_edges().unwrap();
    let mut edge_types = std::collections::HashSet::new();
    for edge in &all_edges {
        edge_types.insert(format!("{:?}", edge.edge_type));
    }

    // CRITICAL: Must have diverse edge types
    assert!(
        edge_types.len() >= 4,
        "REGRESSION: Only {} edge types found, expected at least 4 (DefinedIn, Contains, Calls, Implements/Extends).\n\
         Found types: {:?}\n\
         This indicates extract_relations is not working properly.",
        edge_types.len(),
        edge_types
    );

    println!("✓ Edge type diversity: {} types found", edge_types.len());
    println!("  Types: {:?}", edge_types);
}

#[test]
fn test_analysis_persistence_community() {
    let repo = create_java_test_repo();
    let registry = Arc::new(rbuilder_languages::default_registry());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );

    let (mut graph, _) = pipeline.process_repository(repo.path()).unwrap();

    // Run community detection
    let community_result = CommunityDetector::new()
        .detect(graph.backend_mut())
        .unwrap();

    // Persist to graph (this is what main.rs should do)
    for (node_id, community_id) in &community_result.assignments {
        if let Ok(Some(mut node)) = graph.backend().get_node(*node_id) {
            node.properties
                .insert("community".to_string(), community_id.to_string());
            graph.backend_mut().insert_node(node).unwrap();
        }
    }

    // Save and reload
    let save_path = repo.path();
    graph.save_to_repo(save_path).unwrap();
    let reloaded = CodeGraph::load_from_repo(save_path).unwrap();

    // CRITICAL: Community assignments must persist
    let nodes_with_community = reloaded
        .backend()
        .all_nodes()
        .unwrap()
        .into_iter()
        .filter(|n| n.get_property("community").is_some())
        .count();

    assert!(
        nodes_with_community > 0,
        "REGRESSION: Community assignments not persisted!\n\
         Ran community detection on {} nodes, but 0 nodes have community property after save/reload.\n\
         This indicates analysis results are not being saved to the graph.",
        reloaded.backend().all_nodes().unwrap().len()
    );

    println!(
        "✓ Community persistence working: {}/{} nodes have community assignments",
        nodes_with_community,
        reloaded.backend().all_nodes().unwrap().len()
    );
}

#[test]
fn test_analysis_persistence_centrality() {
    let repo = create_java_test_repo();
    let registry = Arc::new(rbuilder_languages::default_registry());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );

    let (mut graph, _) = pipeline.process_repository(repo.path()).unwrap();

    // Run centrality analysis
    let centrality_report = CentralityAnalyzer::new()
        .analyze(graph.backend_mut())
        .unwrap();

    // Persist to graph
    for (node_id, scores) in &centrality_report.scores {
        if let Ok(Some(mut node)) = graph.backend().get_node(*node_id) {
            node.properties
                .insert("pagerank".to_string(), scores.pagerank.to_string());
            graph.backend_mut().insert_node(node).unwrap();
        }
    }

    // Save and reload
    let save_path = repo.path();
    graph.save_to_repo(save_path).unwrap();
    let reloaded = CodeGraph::load_from_repo(save_path).unwrap();

    // CRITICAL: PageRank scores must persist
    let nodes_with_pagerank = reloaded
        .backend()
        .all_nodes()
        .unwrap()
        .into_iter()
        .filter(|n| n.get_property("pagerank").is_some())
        .count();

    assert!(
        nodes_with_pagerank > 0,
        "REGRESSION: PageRank scores not persisted!\n\
         Computed PageRank for {} nodes, but 0 nodes have pagerank property after save/reload.\n\
         This indicates analysis results are not being saved to the graph.",
        reloaded.backend().all_nodes().unwrap().len()
    );

    println!(
        "✓ Centrality persistence working: {}/{} nodes have pagerank scores",
        nodes_with_pagerank,
        reloaded.backend().all_nodes().unwrap().len()
    );
}

#[test]
fn test_analysis_persistence_complexity() {
    let repo = create_java_test_repo();
    let registry = Arc::new(rbuilder_languages::default_registry());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );

    let (mut graph, _) = pipeline.process_repository(repo.path()).unwrap();

    // Run complexity analysis
    let complexity_report = ComplexityAnalyzer::analyze(graph.backend()).unwrap();

    // Persist to graph
    for func in &complexity_report.functions {
        if let Ok(Some(mut node)) = graph.backend().get_node(func.node.id) {
            node.properties
                .insert("cyclomatic".to_string(), func.cyclomatic.to_string());
            graph.backend_mut().insert_node(node).unwrap();
        }
    }

    // Save and reload
    let save_path = repo.path();
    graph.save_to_repo(save_path).unwrap();
    let reloaded = CodeGraph::load_from_repo(save_path).unwrap();

    // CRITICAL: Complexity scores must persist
    let functions_with_complexity = reloaded
        .backend()
        .all_nodes()
        .unwrap()
        .into_iter()
        .filter(|n| n.node_type == NodeType::Function && n.get_property("cyclomatic").is_some())
        .count();

    assert!(
        functions_with_complexity > 0,
        "REGRESSION: Complexity metrics not persisted!\n\
         Computed complexity for {} functions, but 0 functions have cyclomatic property after save/reload.\n\
         This indicates analysis results are not being saved to the graph.",
        complexity_report.functions.len()
    );

    println!(
        "✓ Complexity persistence working: {}/{} functions have complexity metrics",
        functions_with_complexity,
        complexity_report.functions.len()
    );
}

#[test]
fn test_call_graph_usability() {
    let repo = create_java_test_repo();
    let registry = Arc::new(rbuilder_languages::default_registry());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );

    let (graph, _) = pipeline.process_repository(repo.path()).unwrap();
    let backend = graph.backend();

    // Find the 'process' method implementation (not the interface declaration)
    // We want the one that has outgoing calls, which is in ServiceImpl
    let all_nodes = backend.all_nodes().unwrap();

    let process_method = all_nodes.into_iter().find(|n| {
        n.name == "process"
            && n.node_type == NodeType::Function
            && n.file_path
                .as_ref()
                .map_or(false, |p| p.contains("ServiceImpl"))
    });

    assert!(
        process_method.is_some(),
        "Test setup issue: Could not find 'process' method in ServiceImpl"
    );

    let process_id = process_method.unwrap().id;

    // Find what 'process' calls
    let outgoing_calls: Vec<_> = backend
        .all_edges()
        .unwrap()
        .into_iter()
        .filter(|e| e.from == process_id && matches!(e.edge_type, EdgeType::Calls))
        .collect();

    // CRITICAL: Call graph must be queryable
    assert!(
        !outgoing_calls.is_empty(),
        "REGRESSION: Call graph not usable!\n\
         Method 'process' should call 'transform', but no outgoing Calls edges found.\n\
         This breaks blast radius and dependency analysis."
    );

    println!(
        "✓ Call graph usability: 'process' has {} outgoing calls",
        outgoing_calls.len()
    );
}

#[test]
fn test_full_pipeline_edge_count() {
    let repo = create_java_test_repo();
    let registry = Arc::new(rbuilder_languages::default_registry());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );

    let (graph, stats) = pipeline.process_repository(repo.path()).unwrap();

    // CRITICAL: Edge count must be reasonable
    // We have 5 files, so at minimum:
    // - 5 DefinedIn edges (each class/interface in a file)
    // - 5 Contains edges (each file contains a class/interface)
    // - At least 2 Calls edges
    // - At least 1 Implements edge
    // - At least 1 Extends edge
    // = Minimum 14 edges
    assert!(
        stats.edges_created >= 14,
        "REGRESSION: Too few edges created!\n\
         Expected at least 14 edges (5 DefinedIn + 5 Contains + 2 Calls + 1 Implements + 1 Extends).\n\
         Found: {} edges\n\
         This indicates extract_relations is not working.",
        stats.edges_created
    );

    println!(
        "✓ Edge count reasonable: {} edges (>= 14 expected)",
        stats.edges_created
    );

    // Also verify edge type diversity
    let edge_types: std::collections::HashSet<_> = graph
        .backend()
        .all_edges()
        .unwrap()
        .into_iter()
        .map(|e| format!("{:?}", e.edge_type))
        .collect();

    assert!(
        edge_types.len() >= 4,
        "REGRESSION: Only {} edge types, expected at least 4",
        edge_types.len()
    );

    println!("  Edge types: {:?}", edge_types);
}
