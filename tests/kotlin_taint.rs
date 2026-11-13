//! Kotlin intra-procedural, sanitizer-policy, and inter-procedural taint tests.

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
fn kotlin_canonical_language_id() {
    assert_eq!(canonical_language_id("kt"), Some("kotlin"));
    assert_eq!(
        cfg_language_id_from_path(Path::new("src/main/kotlin/App.kt")),
        Some("kotlin")
    );
}

#[test]
fn kotlin_taint_http_to_sql_vulnerable() {
    let code = r#"
class OrderController {
    fun handle(request: HttpRequest, stmt: Statement) {
        val q = request.getParameter("q")
        val sql = "SELECT * FROM orders WHERE code = '" + q + "'"
        stmt.executeQuery(sql)
    }
}
"#;
    let cfg = build_cfg_for_function("kotlin", code, "handle").expect("kotlin cfg");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("kotlin pdg");
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("kotlin");
    let flows = analyzer.vulnerable_flows();
    assert!(!flows.is_empty(), "expected Kotlin HTTP -> SQL taint flow");
    assert_eq!(flows[0].source_type, TaintSource::HttpParameter);
    assert_eq!(flows[0].sink_type, TaintSink::SqlQuery);
}

#[test]
fn kotlin_taint_dominating_sanitizer_suppresses_flow() {
    let code = r#"
class OrderController {
    fun handleSafe(request: HttpRequest, stmt: Statement) {
        var q = request.getParameter("q")
        q = HtmlUtils.htmlEscape(q)
        stmt.executeQuery(q)
    }
}
"#;
    let cfg = build_cfg_for_function("kotlin", code, "handleSafe").expect("kotlin cfg");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("kotlin pdg");
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("kotlin");
    assert!(
        analyzer.analyze_with_policy().is_ok(),
        "dominating Kotlin sanitizer must satisfy taint policy"
    );
}

#[test]
fn kotlin_taint_interprocedural_caller_to_callee() {
    let code = r#"
class OrderController {
    fun entry(request: HttpRequest, stmt: Statement) {
        val raw = request.getParameter("q")
        runSink(raw, stmt)
    }

    fun runSink(queryInput: String, stmt: Statement) {
        stmt.executeQuery(queryInput)
    }
}
"#;
    let mut backend = MemoryBackend::new();
    let entry = Node::new(NodeType::Function, "entry")
        .with_file_path("OrderController.kt")
        .with_location(3, 6);
    let sink = Node::new(NodeType::Function, "runSink")
        .with_file_path("OrderController.kt")
        .with_location(8, 10);
    let entry_id = entry.id;
    let sink_id = sink.id;
    backend.insert_node(entry).unwrap();
    backend.insert_node(sink).unwrap();
    backend
        .insert_edge(Edge::new(entry_id, sink_id, EdgeType::Calls))
        .unwrap();

    let mut files = HashMap::new();
    files.insert("OrderController.kt".to_string(), code.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).expect("kotlin icfg");
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
        "interprocedural slice must cross into Kotlin caller: {:?}",
        slice.functions
    );
    assert!(
        slice.lines.contains(&4),
        "interprocedural slice must include Kotlin source line 4, got {:?}",
        slice.lines
    );
}
