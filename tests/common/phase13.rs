//! Shared Phase 13 integration helpers.

use rbuilder::analysis::{
    build_cfg_for_function, CallGraph, ControlFlowGraph, DominatorTree, InferredType,
    ProgramDependenceGraph, TaintAnalyzer, TaintFlow, TaintSink, TaintSource, TypeInferenceEngine,
    VariableType,
};
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, GraphParameter, Node, NodeType};
use rbuilder::security::{SecurityAnalyzer, SecurityVulnerability};
use std::collections::HashMap;

/// Run taint analysis on a single function and return all flows (including sanitized).
pub fn analyze_taint(lang: &str, code: &str, fn_name: &str) -> Vec<TaintFlow> {
    let cfg = build_cfg_for_function(lang, code, fn_name).expect("cfg build");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("pdg build");
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns(lang);
    analyzer.analyze()
}

/// Run taint with type inference attached (for sanitizer detection).
pub fn analyze_taint_with_types(lang: &str, code: &str, fn_name: &str) -> Vec<TaintFlow> {
    let cfg = build_cfg_for_function(lang, code, fn_name).expect("cfg build");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("pdg build");
    let mut type_engine = TypeInferenceEngine::new(&pdg, &cfg, lang);
    type_engine.infer();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg).with_type_inference(type_engine);
    analyzer.detect_patterns(lang);
    analyzer.analyze()
}

/// Vulnerable taint flows only.
pub fn analyze_vulnerable_taint(lang: &str, code: &str, fn_name: &str) -> Vec<TaintFlow> {
    analyze_taint(lang, code, fn_name)
        .into_iter()
        .filter(|f| f.is_vulnerable())
        .collect()
}

/// Full security scan: taint + CWE mapping.
pub fn run_taint_security(lang: &str, code: &str, fn_name: &str) -> Vec<SecurityVulnerability> {
    let cfg = build_cfg_for_function(lang, code, fn_name).expect("cfg build");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("pdg build");
    let mut taint = TaintAnalyzer::new(&pdg, &cfg);
    taint.detect_patterns(lang);
    let flows = taint.vulnerable_flows();
    SecurityAnalyzer::new().analyze(flows, &pdg, code)
}

/// Run dominance assertions with CFG + dominator tree.
pub fn with_dominance<F>(lang: &str, code: &str, fn_name: &str, f: F)
where
    F: FnOnce(&ControlFlowGraph, &DominatorTree),
{
    let (flow_cfg, dominator) = build_dominance(lang, code, fn_name);
    f(&flow_cfg, &dominator);
}

/// Run type inference assertions.
pub fn with_inferred_types<F>(lang: &str, code: &str, fn_name: &str, f: F)
where
    F: FnOnce(&[VariableType]),
{
    let (_pdg, _flow_cfg, inferred) = infer_types(lang, code, fn_name);
    f(&inferred);
}

/// Build CFG + PDG + dominator tree for dominance tests.
pub fn build_dominance(lang: &str, code: &str, fn_name: &str) -> (ControlFlowGraph, DominatorTree) {
    let cfg = build_cfg_for_function(lang, code, fn_name).expect("cfg build");
    let dom = DominatorTree::build(&cfg);
    (cfg, dom)
}

/// Build CFG + PDG + type inference results.
pub fn infer_types(
    lang: &str,
    code: &str,
    fn_name: &str,
) -> (ProgramDependenceGraph, ControlFlowGraph, Vec<VariableType>) {
    let cfg = build_cfg_for_function(lang, code, fn_name).expect("cfg build");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("pdg build");
    let mut engine = TypeInferenceEngine::new(&pdg, &cfg, lang);
    let types = engine.infer();
    (pdg, cfg, types)
}

/// Variable has inferred type in results.
pub fn has_type(types: &[VariableType], var: &str, expected: InferredType) -> bool {
    types.iter().any(|t| t.variable == var && t.inferred_type == expected)
}

/// Two-function backend (main -> helper).
pub fn sample_backend() -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    let main = Node::new(NodeType::Function, "main".into()).with_file_path("app.rs".into());
    let helper = Node::new(NodeType::Function, "helper".into()).with_file_path("app.rs".into());
    let id_main = main.id;
    let id_helper = helper.id;
    backend.insert_node(main).unwrap();
    backend.insert_node(helper).unwrap();
    backend
        .insert_edge(Edge::new(id_main, id_helper, EdgeType::Calls))
        .unwrap();
    backend
}

/// Linear call chain: f0 -> f1 -> ... -> f{depth-1} in `app.rs`.
pub fn build_sample_backend_with_chain(depth: usize) -> (MemoryBackend, HashMap<String, String>) {
    assert!(depth >= 2, "chain depth must be >= 2");
    let mut backend = MemoryBackend::new();
    let mut ids = Vec::with_capacity(depth);
    for i in 0..depth {
        let name = format!("f{i}");
        let node = Node::new(NodeType::Function, name.clone()).with_file_path("app.rs".into());
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for i in 0..depth - 1 {
        backend
            .insert_edge(Edge::new(ids[i], ids[i + 1], EdgeType::Calls))
            .unwrap();
    }

    let mut source = String::new();
    for i in 0..depth {
        let name = format!("f{i}");
        if i == 0 {
            source.push_str(&format!("fn {name}() {{\n    f1(0);\n}}\n"));
        } else if i < depth - 1 {
            let next = format!("f{}", i + 1);
            source.push_str(&format!("fn {name}(v: i32) {{\n    {next}(v);\n}}\n"));
        } else {
            source.push_str(&format!("fn {name}(input: i32) -> i32 {{\n    input + 1\n}}\n"));
        }
    }
    let mut files = HashMap::new();
    files.insert("app.rs".into(), source);
    (backend, files)
}

/// Backend with GraphParameter metadata on the leaf function.
pub fn build_backend_with_parameters() -> (MemoryBackend, HashMap<String, String>) {
    let mut backend = MemoryBackend::new();
    let main = Node::new(NodeType::Function, "main".into()).with_file_path("chain.rs".into());
    let process = Node::new(NodeType::Function, "process".into())
        .with_file_path("chain.rs".into())
        .with_parameters(vec![
            GraphParameter {
                name: "input".into(),
                param_type: Some("String".into()),
                default_value: None,
            },
            GraphParameter {
                name: "mode".into(),
                param_type: Some("i32".into()),
                default_value: None,
            },
        ]);
    let id_main = main.id;
    let id_process = process.id;
    backend.insert_node(main).unwrap();
    backend.insert_node(process).unwrap();
    backend
        .insert_edge(Edge::new(id_main, id_process, EdgeType::Calls))
        .unwrap();

    let source = r#"
fn main() {
    let data = read_input();
    let result = process(data, 1);
    write_output(result);
}
fn process(input: String, mode: i32) -> String {
    let trimmed = input.trim();
    format!("Processed: {}", trimmed)
}
fn read_input() -> String { String::new() }
fn write_output(_: String) {}
"#;
    let mut files = HashMap::new();
    files.insert("chain.rs".into(), source.to_string());
    (backend, files)
}

/// Populate backend with `n` function nodes for GQL optimizer tests.
pub fn large_graph(n: usize) -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    for i in 0..n {
        backend
            .insert_node(Node::new(NodeType::Function, format!("fn_{i}")))
            .unwrap();
    }
    backend
        .insert_node(Node::new(NodeType::Function, "rare_target".into()))
        .unwrap();
    backend
}

/// Assert at least one flow matches source and sink kinds.
pub fn assert_flow_kind(flows: &[TaintFlow], source: TaintSource, sink: TaintSink) {
    assert!(
        flows.iter().any(|f| f.source_type == source && f.sink_type == sink),
        "expected flow {:?} -> {:?}, got {:?}",
        source,
        sink,
        flows
            .iter()
            .map(|f| (f.source_type, f.sink_type))
            .collect::<Vec<_>>()
    );
}

/// PDG node texts for pattern-based JS tests without JS parser.
pub fn pdg_statement_texts(lang: &str, code: &str, fn_name: &str) -> Vec<String> {
    let cfg = build_cfg_for_function(lang, code, fn_name).expect("cfg build");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("pdg build");
    pdg.nodes.values().map(|n| n.statement.text.clone()).collect()
}

/// Call graph from backend wrapper.
pub fn call_graph_from(backend: &MemoryBackend) -> CallGraph {
    CallGraph::from_backend(backend).expect("call graph")
}
