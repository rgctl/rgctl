//! Boundary, stress, and differential tests for the macro→micro semantic pipeline.
#![allow(dead_code, unused_imports, unused_macros)]

#[path = "common/phase13.rs"]
mod phase13;

use phase13::build_dominance;
use rbuilder::analysis::{
    build_cfg_for_function, criterion_for_parameter, filter_handoff_seeds_by_index,
    resolve_handoff_seeds, verify_idom_acyclic, BackwardSlicer, BlastRadiusEngine,
    ControlFlowGraph, DominatorTree, InterproceduralCFG, InterproceduralSlicer,
    ProgramDependenceGraph, SliceCriterion, TaintAnalyzer,
};
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, GraphParameter, Node, NodeType};
use std::collections::{HashMap, HashSet};
#[test]
fn alias_handoff_interprocedural_slice() {
    let mut backend = MemoryBackend::new();
    let main = Node::new(NodeType::Function, "main".into()).with_file_path("alias.rs".into());
    let process = Node::new(NodeType::Function, "process".into())
        .with_file_path("alias.rs".into())
        .with_parameters(vec![GraphParameter {
            name: "input".into(),
            param_type: Some("String".into()),
            default_value: None,
        }]);
    let helper = Node::new(NodeType::Function, "helper".into())
        .with_file_path("alias.rs".into())
        .with_parameters(vec![GraphParameter {
            name: "value".into(),
            param_type: Some("String".into()),
            default_value: None,
        }]);
    let id_main = main.id;
    let id_process = process.id;
    let id_helper = helper.id;
    backend.insert_node(main).unwrap();
    backend.insert_node(process).unwrap();
    backend.insert_node(helper).unwrap();
    backend
        .insert_edge(Edge::new(id_main, id_process, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_process, id_helper, EdgeType::Calls))
        .unwrap();

    let source = r#"
fn main() {
    let data = read_input();
    process(data);
}
fn process(input: String) {
    let alias = input;
    helper(alias);
}
fn helper(value: String) {
    let trimmed = value.trim();
    let _ = trimmed;
}
fn read_input() -> String { String::new() }
"#;
    let mut files = HashMap::new();
    files.insert("alias.rs".into(), source.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();
    let criterion = criterion_for_parameter(&backend, &icfg, &files, id_helper, "value").unwrap();
    let slice = slicer.slice(id_helper, criterion).unwrap();
    assert!(slice.functions.contains(&id_helper));
    assert!(slice.functions.contains(&id_process) || slice.functions.contains(&id_main));
}
#[test]
fn mutually_recursive_slice_no_refcell_panic() {
    let mut backend = MemoryBackend::new();
    let a = Node::new(NodeType::Function, "a".into()).with_file_path("rec.rs".into());
    let b = Node::new(NodeType::Function, "b".into())
        .with_file_path("rec.rs".into())
        .with_parameters(vec![GraphParameter {
            name: "flag".into(),
            param_type: Some("bool".into()),
            default_value: None,
        }]);
    let id_a = a.id;
    let id_b = b.id;
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend
        .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_b, id_a, EdgeType::Calls))
        .unwrap();

    let source = r#"
fn a(flag: bool) {
    if flag {
        b(false);
    }
}
fn b(flag: bool) {
    if flag {
        a(false);
    }
    let marker = 42;
    let _ = marker;
}
"#;
    let mut files = HashMap::new();
    files.insert("rec.rs".into(), source.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();
    let pdg =
        ProgramDependenceGraph::build(icfg.get_cfg(id_b).unwrap(), source.as_bytes()).unwrap();
    let marker_line = pdg
        .nodes
        .values()
        .find(|n| n.statement.text.contains("marker"))
        .map(|n| n.statement.line)
        .unwrap_or(8);
    let slice = slicer
        .slice(
            id_b,
            SliceCriterion {
                variable: "marker".into(),
                line: marker_line,
            },
        )
        .unwrap();
    assert!(slice.functions.contains(&id_b));
}
#[test]
fn irreducible_cfg_maintains_acyclic_idom() {
    let mut cfg = ControlFlowGraph::new();
    let entry = cfg.entry;
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let c = uuid::Uuid::new_v4();
    let exit = uuid::Uuid::new_v4();
    use rbuilder::analysis::cfg::{BasicBlock, CfgEdgeType, Statement, StatementKind};
    use std::collections::HashSet;

    for (id, label) in [(a, "a"), (b, "b"), (c, "c")] {
        cfg.add_block(BasicBlock {
            id,
            statements: vec![Statement {
                kind: StatementKind::Expression,
                line: 1,
                text: label.into(),
                defined_vars: HashSet::new(),
                used_vars: HashSet::new(),
            }],
            start_line: 1,
            end_line: 1,
        });
    }
    cfg.add_block(BasicBlock {
        id: exit,
        statements: vec![],
        start_line: 2,
        end_line: 2,
    });
    cfg.add_edge(entry, a, CfgEdgeType::Next);
    cfg.add_edge(a, b, CfgEdgeType::IfTrue);
    cfg.add_edge(a, c, CfgEdgeType::IfFalse);
    cfg.add_edge(b, c, CfgEdgeType::Jump);
    cfg.add_edge(c, b, CfgEdgeType::Jump);
    cfg.add_edge(c, exit, CfgEdgeType::Next);
    cfg.exits.push(exit);

    let dom = DominatorTree::build(&cfg);
    assert!(verify_idom_acyclic(&dom.idom));
    let pdg = ProgramDependenceGraph::build(&cfg, b"synthetic").unwrap();
    assert!(!pdg.nodes.is_empty());
}

#[test]
fn kafka_example_exceeds_10k_source_lines() {
    use std::path::Path;

    let kafka_root = Path::new("example/kafka");
    if !kafka_root.exists() {
        eprintln!("skip kafka_example_exceeds_10k_source_lines: kafka example missing");
        return;
    }

    let kafka_test = kafka_root
        .join("clients/src/test/java/org/apache/kafka/clients/admin/KafkaAdminClientTest.java");
    let source = std::fs::read_to_string(&kafka_test).expect("read kafka test source");
    assert!(
        source.lines().count() >= 10_000,
        "kafka fixture must provide >=10k source lines for scale testing"
    );

    // Real monorepo indexing smoke test (graph volume, not synthetic generation).
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
    let (graph, stats) = pipeline
        .process_repository(kafka_root)
        .expect("index kafka example");
    assert!(
        graph.node_count() > 1_000,
        "kafka graph too small: {stats:?}"
    );
}
#[test]
fn linear_block_pdg_memory_scaling() {
    let mut code = String::from("fn linear() {\n    let v0 = 0;\n");
    let stmt_count = 10_000usize;
    for i in 1..stmt_count {
        code.push_str(&format!("    let v{i} = v{} + 1;\n", i - 1));
    }
    code.push_str("}\n");
    let cfg = build_cfg_for_function("rust", &code, "linear").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    assert!(pdg.nodes.len() >= stmt_count / 2);
    let approx_bytes = pdg.nodes.len() * 512 + pdg.data_deps.len() * 64;
    assert!(
        approx_bytes < 50 * 1024 * 1024,
        "PDG footprint {approx_bytes} exceeds 50MB budget"
    );
}
#[test]
fn differential_forward_taint_backward_slice_coherence() {
    let templates = [
        (
            "python",
            r#"
def f(request):
    a = request.GET['a']
    b = a + 1
    c = b * 2
    cursor.execute(c)
"#,
            "f",
            "c",
        ),
        (
            "python",
            r#"
def g(request):
    x = request.GET['x']
    y = x
    z = y + 0
    cursor.execute(z)
"#,
            "g",
            "z",
        ),
    ];

    for _ in 0..50 {
        for (lang, code, fn_name, var) in templates {
            let cfg = build_cfg_for_function(lang, code, fn_name).unwrap();
            let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
            let mut taint = TaintAnalyzer::new(&pdg, &cfg);
            taint.detect_patterns(lang);
            let flows = taint.analyze();
            let Some(flow) = flows.first() else {
                continue;
            };
            let sink_line = pdg.nodes[&flow.sink].statement.line;
            let backward = BackwardSlicer::new(&pdg, &cfg)
                .slice(SliceCriterion {
                    variable: var.to_string(),
                    line: sink_line,
                })
                .unwrap();
            let forward_nodes: HashSet<_> = flow.path.iter().copied().collect();
            let backward_nodes: HashSet<_> = backward.statements.iter().copied().collect();
            assert!(
                !forward_nodes.is_disjoint(&backward_nodes),
                "forward/backward paths must intersect at sink region"
            );
        }
    }
}
#[test]
fn handoff_index_one_leaves_other_param_slices_empty_of_mutation() {
    let mut backend = MemoryBackend::new();
    let main = Node::new(NodeType::Function, "main".into()).with_file_path("idx.rs".into());
    let foo = Node::new(NodeType::Function, "foo".into())
        .with_file_path("idx.rs".into())
        .with_parameters(vec![
            GraphParameter {
                name: "x".into(),
                param_type: None,
                default_value: None,
            },
            GraphParameter {
                name: "y".into(),
                param_type: None,
                default_value: None,
            },
            GraphParameter {
                name: "z".into(),
                param_type: None,
                default_value: None,
            },
        ]);
    let id_main = main.id;
    let id_foo = foo.id;
    backend.insert_node(main).unwrap();
    backend.insert_node(foo).unwrap();
    backend
        .insert_edge(Edge::new(id_main, id_foo, EdgeType::Calls))
        .unwrap();

    let source = r#"
fn main() { foo(1, 2, 3); }
fn foo(x: i32, y: i32, z: i32) -> i32 {
    let mid = y + 10;
    let _x = x;
    let _z = z;
    mid
}
"#;
    let mut files = HashMap::new();
    files.insert("idx.rs".into(), source.to_string());
    let engine = BlastRadiusEngine::build(&backend).unwrap();
    let blast = engine.analyze(id_foo).unwrap();
    let seeds = resolve_handoff_seeds(&backend, &blast, id_foo).unwrap();
    let y_seeds = filter_handoff_seeds_by_index(&seeds, 1);
    assert_eq!(y_seeds.len(), 1);
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let mid_line = source.lines().position(|l| l.contains("y + 10")).unwrap() + 1;
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();
    for idx in [0usize, 2] {
        let param = ["x", "y", "z"][idx];
        let criterion = criterion_for_parameter(&backend, &icfg, &files, id_foo, param).unwrap();
        let slice = slicer.slice(id_foo, criterion).unwrap();
        if idx != 1 {
            assert!(
                !slice.lines.contains(&mid_line),
                "param {param} slice must not include y-mutation line"
            );
        }
    }
}
#[test]
fn nested_loop_dominance_under_stress() {
    let mut code = String::from("fn nested(mut x: i32) -> i32 {\n");
    for i in 0..20 {
        code.push_str(&format!("    while x > {i} {{ x -= 1; }}\n"));
    }
    code.push_str("    x\n}\n");
    let (cfg, dom) = build_dominance("rust", &code, "nested");
    assert!(verify_idom_acyclic(&dom.idom));
    assert!(cfg.blocks.len() > 20);
    for block in dom.reachable.iter() {
        assert!(dom.dominates(cfg.entry, *block));
    }
}
