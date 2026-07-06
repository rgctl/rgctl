//! Phase 15 community detection audit — isolation, determinism, modularity.

use rbuilder::analysis::{default_community_edge_types, CommunityDetector, PetGraphView};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::CodeGraph;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn insert_call(backend: &mut rbuilder::graph::backend::MemoryBackend, from: Uuid, to: Uuid) {
    backend
        .insert_edge(Edge::new(from, to, EdgeType::Calls))
        .unwrap();
}

fn build_clique(
    backend: &mut rbuilder::graph::backend::MemoryBackend,
    prefix: &str,
    n: usize,
) -> Vec<Uuid> {
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let node = Node::new(NodeType::Function, format!("{prefix}_{i}"));
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for i in 0..n {
        for j in 0..n {
            if i != j {
                insert_call(backend, ids[i], ids[j]);
            }
        }
    }
    ids
}

/// Two dense call cliques in one module folder; only `Contains` links to parent.
#[test]
fn dumbbell_coupling_contains_does_not_merge_domains() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    let parent = Node::new(NodeType::Module, "shared_pkg".into());
    let id_parent = parent.id;
    backend.insert_node(parent).unwrap();

    let clique_a = build_clique(backend, "auth", 5);
    let clique_b = build_clique(backend, "billing", 5);

    for id in clique_a.iter().chain(clique_b.iter()) {
        backend
            .insert_edge(Edge::new(id_parent, *id, EdgeType::Contains))
            .unwrap();
    }

    let view = PetGraphView::from_backend(backend).unwrap();
    let result = CommunityDetector::new()
        .detect_with_view_filtered(&view, &[EdgeType::Calls])
        .unwrap();

    let mut function_labels = std::collections::HashSet::new();
    for id in clique_a.iter().chain(clique_b.iter()) {
        function_labels.insert(result.assignments[id]);
    }

    assert_eq!(
        function_labels.len(),
        2,
        "Calls-only detection must yield two behavioral communities, got {function_labels:?}"
    );

    let parent_label = *result.assignments.get(&id_parent).unwrap();
    let func_labels: Vec<usize> = clique_a
        .iter()
        .chain(clique_b.iter())
        .map(|id| result.assignments[id])
        .collect();
    assert!(
        !func_labels.contains(&parent_label),
        "module label must not subsume behavioral communities"
    );
}

/// Symmetrical tie topology must produce identical assignments across repeated runs.
#[test]
fn non_determinism_stress_100_runs_stable() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    let left = Node::new(NodeType::Function, "left_hub".into());
    let right = Node::new(NodeType::Function, "right_hub".into());
    let center = Node::new(NodeType::Function, "center".into());
    let id_left = left.id;
    let id_right = right.id;
    let id_center = center.id;
    backend.insert_node(left).unwrap();
    backend.insert_node(right).unwrap();
    backend.insert_node(center).unwrap();

    insert_call(backend, id_left, id_center);
    insert_call(backend, id_right, id_center);

    let view = PetGraphView::from_backend(backend).unwrap();
    let detector = CommunityDetector::new();
    let baseline = detector
        .detect_with_view_filtered(&view, &[EdgeType::Calls])
        .unwrap();

    for run in 0..100 {
        let again = detector
            .detect_with_view_filtered(&view, &[EdgeType::Calls])
            .unwrap();
        assert_eq!(
            baseline.assignments, again.assignments,
            "assignment drift on run {run}"
        );
    }
}

/// Two disconnected dense cliques should score high modularity when correctly grouped.
#[test]
fn modularity_dense_clique_high_q() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let _ = build_clique(backend, "auth", 8);
    let _ = build_clique(backend, "billing", 8);

    let view = PetGraphView::from_backend(backend).unwrap();
    let result = CommunityDetector::new()
        .detect_with_view_filtered(&view, default_community_edge_types())
        .unwrap();

    assert!(
        result.modularity > 0.4,
        "partitioned dense cliques modularity too low: {}",
        result.modularity
    );
}

/// Singleton-per-node partition on a dense mesh must score below zero.
#[test]
fn modularity_singleton_partition_negative_q() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let ids = build_clique(backend, "mesh", 10);

    let view = PetGraphView::from_backend(backend).unwrap();
    let labels: HashMap<_, _> = view
        .directed
        .node_indices()
        .enumerate()
        .map(|(i, idx)| (idx, i))
        .collect();

    let q = CommunityDetector::new().calculate_modularity(&view, &labels, &[EdgeType::Calls]);
    assert!(q < 0.0, "singleton partition should yield Q < 0, got {q}");
    assert!(ids.len() == 10);
}

/// Large mock monorepo community detection must stay under 150 ms (release).
#[test]
#[ignore = "performance gate: run with `cargo test --release --test phase15_community_audit -- --ignored`"]
fn community_150k_nodes_under_150ms() {
    let graph = build_monorepo_mock(150_000, 700_000);
    let view = PetGraphView::from_backend(graph.backend()).unwrap();

    let start = Instant::now();
    CommunityDetector::new()
        .detect_with_view_filtered(&view, default_community_edge_types())
        .unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(150),
        "community detection regression: {elapsed:?}"
    );
}

fn build_monorepo_mock(nodes: usize, edges: usize) -> CodeGraph {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let mut ids = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let node = Node::new(NodeType::Function, format!("n{i}"));
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for e in 0..edges {
        let from = ids[e % nodes];
        let to = ids[(e * 13 + 7) % nodes];
        if from != to {
            backend
                .insert_edge(Edge::new(from, to, EdgeType::Calls))
                .unwrap();
        }
    }
    graph
}
