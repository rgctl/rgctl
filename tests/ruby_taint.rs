//! Ruby intra-procedural, sanitizer-policy, and inter-procedural taint tests.

use rgctl::analysis::{
    InterproceduralCFG, InterproceduralSlicer, ProgramDependenceGraph, SliceCriterion,
    TaintAnalyzer, TaintSink, TaintSource, build_cfg_for_function,
};
use rgctl::graph::backend::{GraphBackend, MemoryBackend};
use rgctl::graph::schema::{Edge, EdgeType, Node, NodeType};
use std::collections::HashMap;

#[test]
fn ruby_taint_params_to_shell_vulnerable() {
    let code = r#"
class OrdersController
  def run(params)
    cmd = params[:cmd]
    system(cmd)
  end
end
"#;
    let cfg = build_cfg_for_function("ruby", code, "run").expect("ruby cfg");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("ruby pdg");
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("ruby");
    let flows = analyzer.vulnerable_flows();
    assert!(
        !flows.is_empty(),
        "expected Ruby params -> system taint flow"
    );
    assert_eq!(flows[0].source_type, TaintSource::HttpParameter);
    assert_eq!(flows[0].sink_type, TaintSink::ShellCommand);
}

#[test]
fn ruby_taint_dominating_sanitizer_suppresses_flow() {
    let code = r#"
class OrdersController
  def run_safe(params)
    cmd = params[:cmd]
    cmd = CGI.escapeHTML(cmd)
    system(cmd)
  end
end
"#;
    let cfg = build_cfg_for_function("ruby", code, "run_safe").expect("ruby cfg");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("ruby pdg");
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("ruby");
    assert!(
        analyzer.analyze_with_policy().is_ok(),
        "dominating Ruby sanitizer must satisfy taint policy"
    );
}

#[test]
fn ruby_taint_interprocedural_caller_to_callee() {
    let code = r#"
class OrdersController
  def entry(params)
    raw = params[:cmd]
    dispatch_shell(raw)
  end

  def dispatch_shell(command_arg)
    system(command_arg)
  end
end
"#;
    let mut backend = MemoryBackend::new();
    let entry = Node::new(NodeType::Function, "entry")
        .with_file_path("orders_controller.rb")
        .with_location(3, 6);
    let sink = Node::new(NodeType::Function, "dispatch_shell")
        .with_file_path("orders_controller.rb")
        .with_location(8, 10);
    let entry_id = entry.id;
    let sink_id = sink.id;
    backend.insert_node(entry).unwrap();
    backend.insert_node(sink).unwrap();
    backend
        .insert_edge(Edge::new(entry_id, sink_id, EdgeType::Calls))
        .unwrap();

    let mut files = HashMap::new();
    files.insert("orders_controller.rb".to_string(), code.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).expect("ruby icfg");
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).expect("slicer");
    let slice = slicer
        .slice(
            sink_id,
            SliceCriterion {
                variable: "command_arg".into(),
                line: 9,
            },
        )
        .expect("interprocedural slice");
    assert!(slice.functions.contains(&sink_id));
    assert!(
        slice.functions.contains(&entry_id),
        "interprocedural slice must cross into Ruby caller: {:?}",
        slice.functions
    );
    assert!(
        slice.lines.contains(&4),
        "interprocedural slice must include Ruby source line 4, got {:?}",
        slice.lines
    );
}
