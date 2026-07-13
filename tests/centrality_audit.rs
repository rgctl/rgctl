//! Phase 14 centrality audit — edge isolation, bridge detection, convergence, policy.

use rbuilder::analysis::{
    check_policies, default_behavioral_edges, BetweennessCentrality, BlastRadiusEngine,
    CentralityAnalyzer, CentralityScores, FastPageRank, FlatGraphIndex, PetGraphView,
    PolicyRegistry, PolicyViolation, PAGERANK_TOLERANCE,
};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::CodeGraph;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn insert_call(backend: &mut rbuilder::graph::backend::MemoryBackend, from: Uuid, to: Uuid) {
    backend
        .insert_edge(Edge::new(from, to, EdgeType::Calls))
        .unwrap();
}

/// Star contamination: 1 module + 1000 functions linked only via Contains.
#[test]
fn star_contamination_module_pagerank_zero() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let module = Node::new(NodeType::Module, "root_mod".into());
    let id_mod = module.id;
    backend.insert_node(module).unwrap();

    let mut func_ids = Vec::with_capacity(1_000);
    for i in 0..1_000 {
        let func = Node::new(NodeType::Function, format!("fn_{i}"));
        func_ids.push(func.id);
        backend.insert_node(func).unwrap();
        backend
            .insert_edge(Edge::new(id_mod, func_ids[i], EdgeType::Contains))
            .unwrap();
    }

    // Sparse call edges so PageRank is non-trivial on functions only.
    for w in func_ids.windows(2) {
        insert_call(backend, w[0], w[1]);
    }

    let view = PetGraphView::from_backend(backend).unwrap();
    let (scores, _) = FastPageRank::new(20, 0.85).compute(&view, &[EdgeType::Calls]);

    assert_eq!(
        scores.get(&id_mod).copied().unwrap_or(0.0),
        0.0,
        "module must not inherit rank from Contains edges"
    );
    assert!(
        func_ids
            .iter()
            .any(|id| scores.get(id).copied().unwrap_or(0.0) > 0.0),
        "call-connected functions must receive non-zero rank"
    );
}

/// Two dense clusters joined by a single bridge node.
#[test]
fn betweenness_bridge_is_maximum() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    let bridge = Node::new(NodeType::Function, "Util_Bridge".into());
    let id_bridge = bridge.id;
    backend.insert_node(bridge).unwrap();

    let mut left = Vec::new();
    let mut right = Vec::new();
    for side in 0..2 {
        let mut chain = Vec::new();
        for i in 0..8 {
            let n = Node::new(NodeType::Function, format!("c{side}_{i}"));
            chain.push(n.id);
            backend.insert_node(n).unwrap();
            if i > 0 {
                insert_call(backend, chain[i - 1], chain[i]);
            }
        }
        if side == 0 {
            left = chain;
        } else {
            right = chain;
        }
    }

    insert_call(backend, *left.last().unwrap(), id_bridge);
    insert_call(backend, id_bridge, right[0]);
    for w in right.windows(2) {
        insert_call(backend, w[0], w[1]);
    }

    let view = PetGraphView::from_backend(backend).unwrap();
    let bc = BetweennessCentrality::compute_unbounded(&view, &[EdgeType::Calls]);

    let bridge_score = bc.get(&id_bridge).copied().unwrap_or(0.0);
    let max_other = bc
        .iter()
        .filter(|(id, _)| **id != id_bridge)
        .map(|(_, s)| *s)
        .fold(0.0, f64::max);

    assert!(
        bridge_score > max_other,
        "Util_Bridge {bridge_score} must exceed max peer {max_other}"
    );
}

/// Active cycle draining into a sink — PageRank converges within tolerance.
#[test]
fn cyclic_sink_drainage_converges() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    let nodes: Vec<_> = (0..5)
        .map(|i| {
            let n = Node::new(NodeType::Function, format!("cycle_{i}"));
            backend.insert_node(n.clone()).unwrap();
            n
        })
        .collect();

    // 0 -> 1 -> 2 -> 0 cycle
    insert_call(backend, nodes[0].id, nodes[1].id);
    insert_call(backend, nodes[1].id, nodes[2].id);
    insert_call(backend, nodes[2].id, nodes[0].id);
    // cycle nodes drain to sink 3 -> exit 4
    for i in 0..3 {
        insert_call(backend, nodes[i].id, nodes[3].id);
    }
    insert_call(backend, nodes[3].id, nodes[4].id);

    let view = PetGraphView::from_backend(backend).unwrap();
    let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
    let (ranks, stats) = FastPageRank::new(100, 0.85).compute_flat(&index);

    assert!(
        stats.converged,
        "did not converge: delta={}",
        stats.max_delta
    );
    assert!(stats.max_delta < PAGERANK_TOLERANCE);
    assert!(ranks.iter().all(|r| r.is_finite()));
    assert!(ranks.iter().sum::<f64>() > 0.0);
}

/// Policy registry must surface CascadeHazard when blast radius crosses a high-betweenness bridge.
#[test]
fn cascade_hazard_reads_betweenness_scores() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    let source = Node::new(NodeType::Function, "source".into());
    let bridge = Node::new(NodeType::Function, "Util_Bridge".into());
    let target = Node::new(NodeType::Function, "target".into());
    let id_src = source.id;
    let id_bridge = bridge.id;
    let id_target = target.id;
    backend.insert_node(source).unwrap();
    backend.insert_node(bridge).unwrap();
    backend.insert_node(target).unwrap();
    insert_call(backend, id_src, id_bridge);
    insert_call(backend, id_bridge, id_target);

    let view = PetGraphView::from_backend(backend).unwrap();
    let centrality: HashMap<Uuid, CentralityScores> = CentralityAnalyzer::new()
        .with_allowed_types(&[EdgeType::Calls])
        .analyze_with_view(&view)
        .unwrap()
        .scores;

    let bridge_bt = centrality
        .get(&id_bridge)
        .map(|s| s.betweenness)
        .unwrap_or(0.0);
    assert!(bridge_bt > 0.0);

    let engine = BlastRadiusEngine::build(backend).unwrap();
    let mut registry = PolicyRegistry::permissive();
    registry.centrality_alert_threshold = bridge_bt * 0.5;

    let _result = engine
        .analyze_with_policy(id_src, Some(backend), Some(&registry), Some(&centrality))
        .unwrap();

    // Mutating through a high-betweenness bridge must trip cascade hazard policy.
    let zone_with_bridge = vec![id_bridge];
    let violation = check_policies(
        id_src,
        &zone_with_bridge,
        &registry,
        backend,
        Some(&centrality),
    );
    assert_eq!(
        violation,
        Err(PolicyViolation::CascadeHazard {
            node: id_bridge,
            betweenness: bridge_bt,
            threshold: registry.centrality_alert_threshold,
        })
    );
}

/// Behavioral edges exclude Contains/DefinedIn from default analyzer projection.
#[test]
fn default_analyzer_skips_structural_edges() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let module = Node::new(NodeType::Module, "pkg".into());
    let a = Node::new(NodeType::Function, "a".into());
    let b = Node::new(NodeType::Function, "b".into());
    let id_mod = module.id;
    let id_a = a.id;
    let id_b = b.id;
    backend.insert_node(module).unwrap();
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend
        .insert_edge(Edge::new(id_mod, id_a, EdgeType::Contains))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_mod, id_b, EdgeType::DefinedIn))
        .unwrap();
    insert_call(backend, id_a, id_b);

    let report = CentralityAnalyzer::new().analyze(backend).unwrap();
    assert_eq!(
        report
            .scores
            .get(&id_mod)
            .map(|s| s.pagerank)
            .unwrap_or(0.0),
        0.0
    );
    assert_eq!(
        report.scores.get(&id_mod).map(|s| s.in_degree).unwrap_or(0),
        0
    );
    assert!(report.scores.get(&id_a).map(|s| s.out_degree).unwrap_or(0) >= 1);
    assert!(default_behavioral_edges().contains(&EdgeType::Calls));
    assert!(!default_behavioral_edges().contains(&EdgeType::Contains));
}

/// Large mock monorepo PageRank must stay under 20 ms (release builds).
#[test]
#[ignore = "performance gate: run with `cargo test --release --test centrality_audit -- --ignored`"]
fn pagerank_150k_nodes_under_20ms() {
    let graph = build_monorepo_mock(150_000, 700_000);
    let view = PetGraphView::from_backend(graph.backend()).unwrap();

    let start = Instant::now();
    FastPageRank::new(20, 0.85).compute(&view, &[EdgeType::Calls]);
    let latency = start.elapsed();

    assert!(
        latency < Duration::from_millis(20),
        "PageRank regression: {latency:?} >= 20ms"
    );
}

/// Real kafka repository: module nodes stay at zero PageRank under Calls filter.
#[test]
fn kafka_module_behavioral_pagerank_isolated() {
    let kafka_path = Path::new("example/kafka");
    if !kafka_path.exists() {
        eprintln!("skip kafka_module_behavioral_pagerank_isolated: example/kafka missing");
        return;
    }

    use rbuilder_pipeline::{PipelineConfig, ProcessingPipeline};
    use rbuilder_registry::LanguageRegistry;
    use std::sync::Arc;

    let registry = Arc::new(LanguageRegistry::new());
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );
    let (graph, _) = pipeline
        .process_repository(kafka_path)
        .expect("kafka index");
    let backend = graph.backend();

    let view = PetGraphView::from_backend(backend).unwrap();
    let (scores, stats) = FastPageRank::new(20, 0.85).compute(&view, &[EdgeType::Calls]);
    assert!(stats.max_delta.is_finite());

    let mut module_nonzero = 0usize;
    for node in backend.all_nodes().unwrap() {
        if node.node_type == NodeType::Module {
            if scores.get(&node.id).copied().unwrap_or(0.0) > 0.0 {
                module_nonzero += 1;
            }
        }
    }
    assert_eq!(
        module_nonzero, 0,
        "modules must not receive behavioral PageRank from kafka graph"
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
