//! Groovy intra-procedural, sanitizer-policy, and inter-procedural taint tests.

use rgctl::analysis::{
    InterproceduralCFG, InterproceduralSlicer, ProgramDependenceGraph, SliceCriterion,
    TaintAnalyzer, TaintSink, TaintSource, build_cfg_for_function, canonical_language_id,
    cfg_language_id_from_path,
};
use rgctl::graph::backend::{GraphBackend, MemoryBackend};
use rgctl::graph::schema::{Edge, EdgeType, Node, NodeType};
use std::collections::HashMap;
use std::path::Path;

#[test]
fn groovy_canonical_language_id() {
    assert_eq!(canonical_language_id("groovy"), Some("groovy"));
    assert_eq!(
        cfg_language_id_from_path(Path::new("scripts/Job.groovy")),
        Some("groovy")
    );
}

#[test]
fn groovy_taint_http_to_sql_vulnerable() {
    let code = r#"
class OrderController {
    void handle(def request, def stmt) {
        def q = request.getParameter("q")
        def sql = "SELECT * FROM orders WHERE code = '" + q + "'"
        stmt.executeQuery(sql)
    }
}
"#;
    let cfg = build_cfg_for_function("groovy", code, "handle").expect("groovy cfg");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("groovy pdg");
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("groovy");
    let flows = analyzer.vulnerable_flows();
    assert!(!flows.is_empty(), "expected Groovy HTTP -> SQL taint flow");
    assert_eq!(flows[0].source_type, TaintSource::HttpParameter);
    assert_eq!(flows[0].sink_type, TaintSink::SqlQuery);
}

#[test]
fn groovy_taint_dominating_sanitizer_suppresses_flow() {
    let code = r#"
class OrderController {
    void handleSafe(def request, def stmt) {
        def q = request.getParameter("q")
        q = StringEscapeUtils.escapeHtml(q)
        stmt.executeQuery(q)
    }
}
"#;
    let cfg = build_cfg_for_function("groovy", code, "handleSafe").expect("groovy cfg");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("groovy pdg");
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("groovy");
    assert!(
        analyzer.analyze_with_policy().is_ok(),
        "dominating Groovy sanitizer must satisfy taint policy"
    );
}

#[test]
fn groovy_taint_interprocedural_caller_to_callee() {
    let code = r#"
class OrderController {
    void entry(def request, def stmt) {
        def raw = request.getParameter("q")
        runSink(raw, stmt)
    }

    void runSink(String queryInput, def stmt) {
        stmt.executeQuery(queryInput)
    }
}
"#;
    let mut backend = MemoryBackend::new();
    let entry = Node::new(NodeType::Function, "entry")
        .with_file_path("OrderController.groovy")
        .with_location(3, 6);
    let sink = Node::new(NodeType::Function, "runSink")
        .with_file_path("OrderController.groovy")
        .with_location(8, 10);
    let entry_id = entry.id;
    let sink_id = sink.id;
    backend.insert_node(entry).unwrap();
    backend.insert_node(sink).unwrap();
    backend
        .insert_edge(Edge::new(entry_id, sink_id, EdgeType::Calls))
        .unwrap();

    let mut files = HashMap::new();
    files.insert("OrderController.groovy".to_string(), code.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).expect("groovy icfg");
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).expect("slicer");
    let slice = slicer
        .slice(
            sink_id,
            SliceCriterion {
                variable: "queryInput".into(),
                line: 9,
            },
        )
        .expect("interprocedural slice");
    assert!(slice.functions.contains(&sink_id));
    assert!(
        slice.functions.contains(&entry_id),
        "interprocedural slice must cross into Groovy caller: {:?}",
        slice.functions
    );
    assert!(
        slice.lines.contains(&4),
        "interprocedural slice must include Groovy source line 4, got {:?}",
        slice.lines
    );
}
