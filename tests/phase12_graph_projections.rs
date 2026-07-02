//! Phase 12 — Strategy 2 audit: coverage, policy, chaos, and parity tests

#[path = "graph_audit.rs"]
mod graph_audit;

use graph_audit::{
    deep_chain, mixed_edge_hub, random_call_graph, star, structural_topology,
};
use rbuilder::analysis::graph_utils::PetGraphView;
use rbuilder::analysis::{
    check_policies, resolve_unique_symbol, BlastRadiusAnalyzer, BlastRadiusEngine,
    CentralityScores, PolicyRegistry, PolicyViolation,
};
use rbuilder::gql::{parse, QueryExecutor};
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use std::collections::HashMap;
use uuid::Uuid;

fn sorted_function_impact(backend: &MemoryBackend, ids: &[Uuid]) -> Vec<Uuid> {
    let mut ids = BlastRadiusEngine::filter_function_impact(backend, ids)
        .unwrap()
        .into_iter()
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

#[test]
fn test_type_isolation_incoming_filtered() {
    let backend = mixed_edge_hub();
    let view = PetGraphView::from_backend(&backend).unwrap();
    let target_idx = view.uuid_to_index[&backend
        .find_nodes_by_name("target")
        .unwrap()[0]
        .id];

    let call_incoming: Vec<_> = view
        .incoming_filtered(target_idx, &[EdgeType::Calls])
        .collect();
    let all_incoming: Vec<_> = view
        .directed
        .neighbors_directed(target_idx, petgraph::Direction::Incoming)
        .collect();

    assert_eq!(call_incoming.len(), 1);
    assert_eq!(all_incoming.len(), 3);
}

#[test]
fn test_engine_unification_random_parity() {
    for seed in 0..50 {
        let backend = random_call_graph(seed, 8 + (seed as usize % 12));
        let engine = BlastRadiusEngine::build(&backend).unwrap();
        let analyzer = BlastRadiusAnalyzer::new(&backend);

        for node in backend.all_nodes().unwrap() {
            if node.node_type != NodeType::Function {
                continue;
            }
            let scc = engine.analyze(node.id).unwrap();
            let bfs = analyzer.analyze_by_id(node.id).unwrap();
            let scc_ids = sorted_function_impact(&backend, &scc.impact_zone_ids);
            let mut bfs_ids = bfs.impact_zone_ids.clone();
            bfs_ids.sort();
            assert_eq!(scc_ids, bfs_ids, "seed={seed} node={}", node.name);
        }
    }
}

#[test]
fn test_policy_scale_exceeded() {
    let backend = deep_chain(7);
    let leaf = backend
        .all_nodes()
        .unwrap()
        .into_iter()
        .find(|n| n.name == "f6")
        .unwrap()
        .id;
    let engine = BlastRadiusEngine::build(&backend).unwrap();
    let result = engine.analyze(leaf).unwrap();

    let mut registry = PolicyRegistry::permissive();
    registry.max_impact_nodes = 5;
    assert_eq!(
        check_policies(leaf, &result.impact_zone_ids, &registry, &backend, None),
        Err(PolicyViolation::ScaleFailure {
            count: 6,
            max: 5,
        })
    );
}

#[test]
fn test_policy_domain_boundary_breached() {
    let mut backend = MemoryBackend::new();
    let a = Node::new(NodeType::Function, "node_a".into());
    let b = Node::new(NodeType::Function, "node_b".into());
    let id_a = a.id;
    let id_b = b.id;
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend
        .insert_edge(Edge::new(id_b, id_a, EdgeType::Calls))
        .unwrap();

    let mut registry = PolicyRegistry::permissive();
    registry.assign_domain(id_a, "restricted");
    registry.assign_domain(id_b, "public");
    registry
        .forbidden_crossings
        .push(("restricted".into(), "public".into()));

    let engine = BlastRadiusEngine::build(&backend).unwrap();
    let result = engine.analyze(id_a).unwrap();
    assert_eq!(
        check_policies(id_a, &result.impact_zone_ids, &registry, &backend, None),
        Err(PolicyViolation::DomainIsolation {
            source_domain: "restricted".into(),
            reached_domain: "public".into(),
            node: id_b,
        })
    );
}

#[test]
fn test_policy_centrality_bridge_block() {
    let backend = star(20);
    let hub = backend
        .all_nodes()
        .unwrap()
        .into_iter()
        .find(|n| n.name == "hub")
        .unwrap()
        .id;
    let leaf = backend
        .all_nodes()
        .unwrap()
        .into_iter()
        .find(|n| n.name.starts_with("leaf"))
        .unwrap()
        .id;

    let bridge = Uuid::new_v4();
    let mut centrality = HashMap::new();
    centrality.insert(
        bridge,
        CentralityScores {
            betweenness: 0.95,
            ..Default::default()
        },
    );

    let engine = BlastRadiusEngine::build(&backend).unwrap();
    let mut result = engine.analyze(leaf).unwrap();
    result.impact_zone_ids.push(bridge);

    let mut registry = PolicyRegistry::permissive();
    registry.centrality_alert_threshold = 0.5;

    assert_eq!(
        check_policies(hub, &result.impact_zone_ids, &registry, &backend, Some(&centrality)),
        Err(PolicyViolation::CascadeHazard {
            node: bridge,
            betweenness: 0.95,
            threshold: 0.5,
        })
    );
}

#[test]
fn test_phantom_symbol_rejects_ambiguous_name() {
    let mut backend = MemoryBackend::new();
    for ns in ["pkg::a", "pkg::b", "pkg::c"] {
        backend
            .insert_node(
                Node::new(NodeType::Function, "handler".into())
                    .with_qualified_name(ns.into()),
            )
            .unwrap();
    }

    let err = resolve_unique_symbol(&backend, "handler").unwrap_err();
    assert!(err.to_string().contains("ambiguous"));
    assert!(BlastRadiusAnalyzer::new(&backend)
        .analyze("handler")
        .is_err());
}

#[test]
fn test_unknown_edge_type_excluded_from_call_traversal() {
    let mut backend = MemoryBackend::new();
    let caller = Node::new(NodeType::Function, "caller".into());
    let target = Node::new(NodeType::Function, "target".into());
    let decoy = Node::new(NodeType::Module, "decoy".into());
    let id_c = caller.id;
    let id_t = target.id;
    let id_d = decoy.id;
    backend.insert_node(caller).unwrap();
    backend.insert_node(target).unwrap();
    backend.insert_node(decoy).unwrap();
    backend
        .insert_edge(Edge::new(id_c, id_t, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_d, id_t, EdgeType::Unknown))
        .unwrap();

    let engine = BlastRadiusEngine::build(&backend).unwrap();
    let result = engine.analyze(id_t).unwrap();
    let impact = sorted_function_impact(&backend, &result.impact_zone_ids);
    assert_eq!(impact, vec![id_c]);
    assert!(!result.impact_zone_ids.contains(&id_d));
}

#[test]
fn test_structural_isolation() {
    let (backend, module_id, main_id, init_id) = structural_topology();
    let engine = BlastRadiusEngine::build(&backend).unwrap();
    let result = engine.analyze(init_id).unwrap();

    let impact = sorted_function_impact(&backend, &result.impact_zone_ids);
    assert_eq!(impact, vec![main_id]);
    assert!(!result.impact_zone_ids.contains(&module_id));

    let bfs = BlastRadiusAnalyzer::new(&backend)
        .analyze_by_id(init_id)
        .unwrap();
    assert_eq!(bfs.impact_zone, vec!["main".to_string()]);
}

#[test]
fn test_deep_chain_parity() {
    let backend = deep_chain(15);
    let leaf = backend
        .all_nodes()
        .unwrap()
        .into_iter()
        .find(|n| n.name == "f14")
        .unwrap()
        .id;

    let scc = BlastRadiusEngine::build(&backend)
        .unwrap()
        .analyze(leaf)
        .unwrap();
    let bfs = BlastRadiusAnalyzer::new(&backend)
        .analyze_by_id(leaf)
        .unwrap();

    assert_eq!(
        sorted_function_impact(&backend, &scc.impact_zone_ids).len(),
        14
    );
    assert_eq!(bfs.impact_zone.len(), 14);
}

#[test]
fn test_gql_edge_accuracy() {
    let (backend, module_id, main_id, init_id) = structural_topology();
    let query = parse("MATCH (a)-[:CALLS]->(b) RETURN a, b").unwrap();
    let result = QueryExecutor::new(&backend).execute(&query).unwrap();

    assert!(result.rows.iter().any(|row| {
        let ids: Vec<_> = row.values().map(|n| n.id).collect();
        ids.contains(&main_id) && ids.contains(&init_id)
    }));

    let module_to_main = parse("MATCH (m)-[:CALLS]->(f) RETURN m, f").unwrap();
    let cross = QueryExecutor::new(&backend).execute(&module_to_main).unwrap();
    assert!(!cross.rows.iter().any(|row| {
        row.values().any(|n| n.id == module_id) && row.values().any(|n| n.id == main_id)
    }));
}

#[test]
fn test_scc_loop_sweep() {
    let mut backend = MemoryBackend::new();
    let a = Node::new(NodeType::Function, "a".into());
    let b = Node::new(NodeType::Function, "b".into());
    let c = Node::new(NodeType::Function, "c".into());
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
    backend
        .insert_edge(Edge::new(id_c, id_b, EdgeType::Calls))
        .unwrap();

    let engine = BlastRadiusEngine::build(&backend).unwrap();
    let result = engine.analyze(id_c).unwrap();
    let impact = sorted_function_impact(&backend, &result.impact_zone_ids);

    assert!(impact.contains(&id_a));
    assert!(impact.contains(&id_b));
    assert_eq!(result.scc_size, 2);
}

#[test]
#[ignore = "run manually: memory audit at 150k nodes / 1M edges (RBUILDER_BENCH_LARGE=1)"]
fn test_memory_footprint_large_graph() {
    if std::env::var("RBUILDER_BENCH_LARGE").is_err() {
        return;
    }
    let backend = graph_audit::large_mixed_graph(150_000, 1_000_000);
    let view = PetGraphView::from_backend(&backend).unwrap();
    assert_eq!(view.directed.node_count(), 150_000);
    assert_eq!(view.directed.edge_count(), 1_000_000);
}
