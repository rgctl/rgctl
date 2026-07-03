//! Blast-radius `--depth` hop limiting.

use rbuilder::analysis::{
    filter_impact_by_caller_depth, impact_score_from_counts, BlastRadiusEngine, PetGraphView,
};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::CodeGraph;

fn build_chain() -> (CodeGraph, uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let a = Node::new(NodeType::Function, "a".to_string());
    let b = Node::new(NodeType::Function, "b".to_string());
    let c = Node::new(NodeType::Function, "c".to_string());
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
    (graph, id_a, id_b, id_c)
}

#[test]
fn depth_one_excludes_transitive_callers_on_chain() {
    let (graph, id_a, id_b, id_c) = build_chain();
    let backend = graph.backend();
    let engine = BlastRadiusEngine::build(backend).unwrap();
    let result = engine.analyze(id_c).unwrap();
    let view = PetGraphView::from_backend(backend).unwrap();
    let full = BlastRadiusEngine::filter_function_impact(backend, &result.impact_zone_ids).unwrap();

    let limited =
        filter_impact_by_caller_depth(&view, id_c, &full, 1);
    assert_eq!(limited, vec![id_b]);
    assert!(!limited.contains(&id_a));

    let score = impact_score_from_counts(result.direct_caller_ids.len(), limited.len());
    assert!(score < result.score);
}

#[test]
fn depth_unlimited_matches_full_engine_zone() {
    let (graph, _id_a, _id_b, id_c) = build_chain();
    let backend = graph.backend();
    let engine = BlastRadiusEngine::build(backend).unwrap();
    let result = engine.analyze(id_c).unwrap();
    let view = PetGraphView::from_backend(backend).unwrap();
    let full = BlastRadiusEngine::filter_function_impact(backend, &result.impact_zone_ids).unwrap();
    let unlimited = filter_impact_by_caller_depth(&view, id_c, &full, usize::MAX);
    assert_eq!(unlimited, full);
}
