//! Puppet intra-procedural, sanitizer-policy, and inter-procedural taint tests.

use rgctl::analysis::{
    InterproceduralCFG, InterproceduralSlicer, ProgramDependenceGraph, SliceCriterion,
    TaintAnalyzer, TaintSink, build_cfg_for_function,
};
use rgctl::graph::backend::{GraphBackend, MemoryBackend};
use rgctl::graph::schema::{Edge, EdgeType, Node, NodeType};
use std::collections::HashMap;

#[test]
fn puppet_taint_lookup_to_exec_vulnerable() {
    let code = r#"
define profile::deploy() {
  $cmd = lookup('deploy_cmd')
  exec { 'run_deploy': command => $cmd }
}
"#;
    let cfg = build_cfg_for_function("puppet", code, "profile::deploy").expect("puppet cfg");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("puppet pdg");
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("puppet");
    let flows = analyzer.vulnerable_flows();
    assert!(
        !flows.is_empty(),
        "expected Puppet lookup -> exec taint flow"
    );
    assert_eq!(flows[0].sink_type, TaintSink::ShellCommand);
}

#[test]
fn puppet_taint_shellquote_sanitizer_suppresses_flow() {
    let code = r#"
define profile::deploy_safe() {
  $cmd = lookup('deploy_cmd')
  $cmd = shellquote($cmd)
  exec { 'run_deploy': command => $cmd }
}
"#;
    let cfg = build_cfg_for_function("puppet", code, "profile::deploy_safe").expect("puppet cfg");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("puppet pdg");
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("puppet");
    assert!(
        analyzer.analyze_with_policy().is_ok(),
        "dominating Puppet shellquote sanitizer must satisfy taint policy"
    );
}

#[test]
fn puppet_taint_interprocedural_caller_to_callee() {
    let code = r#"
define profile::entry() {
  $raw = lookup('deploy_cmd')
  profile::runner($raw)
}

define profile::runner(String $command_str) {
  exec { 'run_deploy': command => $command_str }
}
"#;
    let mut backend = MemoryBackend::new();
    let entry = Node::new(NodeType::Function, "profile::entry")
        .with_file_path("init.pp")
        .with_location(2, 5);
    let sink = Node::new(NodeType::Function, "profile::runner")
        .with_file_path("init.pp")
        .with_location(7, 9);
    let entry_id = entry.id;
    let sink_id = sink.id;
    backend.insert_node(entry).unwrap();
    backend.insert_node(sink).unwrap();
    backend
        .insert_edge(Edge::new(entry_id, sink_id, EdgeType::Calls))
        .unwrap();

    let mut files = HashMap::new();
    files.insert("init.pp".to_string(), code.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).expect("puppet icfg");
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).expect("slicer");
    let slice = slicer
        .slice(
            sink_id,
            SliceCriterion {
                variable: "command_str".into(),
                line: 8,
            },
        )
        .expect("interprocedural slice");
    assert!(slice.functions.contains(&sink_id));
    assert!(
        slice.functions.contains(&entry_id),
        "interprocedural slice must cross into Puppet caller: {:?}",
        slice.functions
    );
    assert!(
        slice.lines.contains(&3),
        "interprocedural slice must include Puppet lookup line 3, got {:?}",
        slice.lines
    );
}
