//! Phase 12.2 — blast radius integration tests

use rbuilder::analysis::cfg::{Statement, StatementKind};
use rbuilder::analysis::pdg::{DataDepType, DataDependency, PdgNode};
use rbuilder::analysis::{BlastRadiusAnalyzer, FlowCache, ProgramDependenceGraph};
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use std::collections::HashSet;

fn build_call_chain() -> (MemoryBackend, uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let mut backend = MemoryBackend::new();
    let a = Node::new(NodeType::Function, "a".to_string())
        .with_property("complexity".into(), "10".into());
    let b = Node::new(NodeType::Function, "b".to_string())
        .with_property("complexity".into(), "12".into());
    let c = Node::new(NodeType::Function, "c".to_string())
        .with_property("complexity".into(), "8".into());
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
    (backend, id_a, id_b, id_c)
}

#[test]
fn test_blast_radius_call_chain() {
    let (backend, _, _, _) = build_call_chain();
    let report = BlastRadiusAnalyzer::new(&backend).analyze("c").unwrap();

    assert_eq!(report.symbol_name, "c");
    assert_eq!(report.direct_callers, vec!["b".to_string()]);
    assert_eq!(report.impact_zone.len(), 2);
    assert!(report.score > 50.0);
}

#[test]
fn test_blast_radius_leaf_has_zero_score() {
    let mut backend = MemoryBackend::new();
    backend
        .insert_node(Node::new(NodeType::Function, "leaf".to_string()))
        .unwrap();

    let report = BlastRadiusAnalyzer::new(&backend).analyze("leaf").unwrap();
    assert_eq!(report.score, 0.0);
    assert!(report.direct_callers.is_empty());
    assert!(report.impact_zone.is_empty());
}

#[test]
fn test_blast_radius_pdg_enriches_data_flow_depth() {
    let (backend, _, id_b, _) = build_call_chain();
    let mut cache = FlowCache::new();

    let block = uuid::Uuid::new_v4();
    let n1 = uuid::Uuid::new_v4();
    let n2 = uuid::Uuid::new_v4();
    let mut pdg = ProgramDependenceGraph::default();
    pdg.nodes.insert(
        n1,
        PdgNode {
            id: n1,
            statement: Statement {
                kind: StatementKind::Expression,
                line: 1,
                text: "let tmp = c()".into(),
                defined_vars: HashSet::new(),
                used_vars: HashSet::new(),
            },
            block,
            defined_vars: ["tmp"].into_iter().map(String::from).collect(),
            used_vars: ["c"].into_iter().map(String::from).collect(),
        },
    );
    pdg.nodes.insert(
        n2,
        PdgNode {
            id: n2,
            statement: Statement {
                kind: StatementKind::Return,
                line: 2,
                text: "return tmp".into(),
                defined_vars: HashSet::new(),
                used_vars: HashSet::new(),
            },
            block,
            defined_vars: std::collections::HashSet::new(),
            used_vars: ["tmp"].into_iter().map(String::from).collect(),
        },
    );
    pdg.data_deps.push(DataDependency {
        from: n1,
        to: n2,
        variable: "tmp".into(),
        dep_type: DataDepType::Flow,
    });
    cache.insert_pdg(id_b, pdg);

    let report = BlastRadiusAnalyzer::new(&backend)
        .with_flow_cache(&cache)
        .analyze("c")
        .unwrap();

    assert_eq!(report.data_flow_depth, 1);
    assert_eq!(report.data_flow_impact.len(), 1);
    assert_eq!(report.data_flow_impact[0].depth, 1);
}

#[test]
fn test_blast_radius_unknown_symbol_errors() {
    let backend = MemoryBackend::new();
    let err = BlastRadiusAnalyzer::new(&backend)
        .analyze("missing")
        .unwrap_err();
    assert!(err.to_string().contains("missing"));
}
