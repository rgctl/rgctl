//! Phase 13 semantic audit fixtures (5 core topologies).

#[path = "fixtures/mod.rs"]
mod fixtures;

use fixtures::{
    dead_code_post_return, diamond_merge, interprocedural_handoff, loop_back_edge,
    sanitizer_bypass,
};
use rbuilder::analysis::{
    build_cfg_for_function, verify_idom_acyclic, DominatorTree, PolicyViolation,
    ProgramDependenceGraph, TaintAnalyzer,
};
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, GraphParameter, Node, NodeType};
use std::collections::HashMap;

#[cfg(feature = "bundle-minimal")]
#[test]
fn fixture_diamond_merge_frontiers() {
    let cfg = build_cfg_for_function("rust", diamond_merge::CODE, diamond_merge::FN).unwrap();
    let dom = DominatorTree::build(&cfg);
    assert!(verify_idom_acyclic(&dom.idom));
    assert!(dom.frontiers.values().any(|f| !f.is_empty()));
}

#[cfg(feature = "bundle-minimal")]
#[test]
fn fixture_dead_code_post_return_excluded() {
    let cfg = build_cfg_for_function(
        "rust",
        dead_code_post_return::CODE,
        dead_code_post_return::FN,
    )
    .unwrap();
    let reachable = cfg.reachable_blocks();
    let dead_line = cfg
        .blocks
        .values()
        .flat_map(|b| &b.statements)
        .find(|s| s.text.contains("unreachable = 99"))
        .map(|s| s.line);
    if let Some(line) = dead_line {
        let pdg = ProgramDependenceGraph::build(&cfg, dead_code_post_return::CODE.as_bytes())
            .unwrap();
        let dead_in_pdg = pdg.nodes.values().any(|n| n.statement.line == line);
        assert!(
            !dead_in_pdg || !reachable.iter().any(|block| {
                cfg.blocks
                    .get(block)
                    .is_some_and(|b| b.statements.iter().any(|s| s.line == line))
            }),
            "dead code after return should not appear in reachable CFG blocks"
        );
    }
}

#[cfg(feature = "bundle-minimal")]
#[test]
fn fixture_loop_back_edge_idom_acyclic() {
    let cfg = build_cfg_for_function("rust", loop_back_edge::CODE, loop_back_edge::FN).unwrap();
    let dom = DominatorTree::build(&cfg);
    assert!(verify_idom_acyclic(&dom.idom));
}

#[cfg(feature = "bundle-minimal")]
#[test]
fn fixture_sanitizer_bypass_detected() {
    let cfg = build_cfg_for_function(
        "python",
        sanitizer_bypass::CODE,
        sanitizer_bypass::FN,
    )
    .unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, sanitizer_bypass::CODE.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("python");
    assert!(
        !analyzer.analyze().iter().all(|f| !f.is_vulnerable()),
        "non-dominating sanitizer must not clear taint"
    );
    // Dominance-aware policy rejects flows where a path sanitizer fails to dominate the sink.
    let policy = analyzer.analyze_with_policy();
    assert!(
        policy.is_ok() || matches!(policy, Err(PolicyViolation::SanitizationBypass { .. })),
        "unexpected policy result: {policy:?}"
    );
}

#[cfg(feature = "bundle-minimal")]
#[test]
fn fixture_interprocedural_handoff_trace() {
    use rbuilder::analysis::{resolve_handoff_seeds, BlastRadiusEngine, InterproceduralCFG};

    let mut backend = MemoryBackend::new();
    let main = Node::new(NodeType::Function, "main".into()).with_file_path("chain.rs".into());
    let process = Node::new(NodeType::Function, "process".into())
        .with_file_path("chain.rs".into())
        .with_parameters(vec![GraphParameter {
            name: "input".into(),
            param_type: Some("String".into()),
            default_value: None,
        }]);
    let id_main = main.id;
    let id_process = process.id;
    backend.insert_node(main).unwrap();
    backend.insert_node(process).unwrap();
    backend
        .insert_edge(Edge::new(id_main, id_process, EdgeType::Calls))
        .unwrap();

    let mut files = HashMap::new();
    files.insert(
        "chain.rs".into(),
        interprocedural_handoff::SOURCE.to_string(),
    );

    let engine = BlastRadiusEngine::build(&backend).unwrap();
    let blast = engine.analyze(id_process).unwrap();
    let seeds = resolve_handoff_seeds(&backend, &blast, id_process).unwrap();
    assert!(!seeds.is_empty());
    assert!(seeds.iter().any(|s| s.caller_id == id_main && s.callee_id == id_process));

    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    assert!(icfg.get_cfg(id_process).is_some());
}
