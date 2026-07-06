//! Phase 13: interprocedural analysis (20 tests).
#![allow(dead_code, unused_imports, unused_macros)]

#[path = "common/phase13.rs"]
mod phase13;

use phase13::{
    build_backend_with_parameters, build_sample_backend_with_chain, call_graph_from, sample_backend,
};
use rbuilder::analysis::{
    InterproceduralCFG, InterproceduralSlicer, ProgramDependenceGraph, SliceCriterion,
};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, GraphParameter, Node, NodeType};
use std::collections::HashMap;

macro_rules! ip_test {
    ($name:ident, $body:expr) => {
        #[test]
        fn $name() {
            $body;
        }
    };
}

ip_test!(call_graph_node_count, {
    let cg = call_graph_from(&sample_backend());
    assert_eq!(cg.function_count(), 2);
    assert_eq!(cg.call_edge_count(), 1);
});

ip_test!(call_graph_callees_main, {
    let cg = call_graph_from(&sample_backend());
    let main_id = cg.id_by_name("main").unwrap();
    let helper_id = cg.id_by_name("helper").unwrap();
    assert_eq!(cg.callees(main_id), vec![helper_id]);
});

ip_test!(call_graph_callers_helper, {
    let cg = call_graph_from(&sample_backend());
    let main_id = cg.id_by_name("main").unwrap();
    let helper_id = cg.id_by_name("helper").unwrap();
    assert_eq!(cg.callers(helper_id), vec![main_id]);
});

ip_test!(topological_order_chain, {
    let (backend, _) = build_sample_backend_with_chain(4);
    let cg = call_graph_from(&backend);
    let order = cg.topological_order().unwrap();
    assert_eq!(order.len(), 4);
    let f0 = cg.id_by_name("f0").unwrap();
    assert_eq!(order[0], f0);
});

ip_test!(topological_order_diamond, {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    let a = Node::new(NodeType::Function, "a".into());
    let b = Node::new(NodeType::Function, "b".into());
    let c = Node::new(NodeType::Function, "c".into());
    let d = Node::new(NodeType::Function, "d".into());
    let id_a = a.id;
    let id_b = b.id;
    let id_c = c.id;
    let id_d = d.id;
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend.insert_node(c).unwrap();
    backend.insert_node(d).unwrap();
    backend
        .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_a, id_c, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_b, id_d, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_c, id_d, EdgeType::Calls))
        .unwrap();
    let cg = call_graph_from(&backend);
    let order = cg.topological_order().unwrap();
    assert_eq!(order.len(), 4);
    assert_eq!(order[0], id_a);
    assert!(order.contains(&id_d));
});

ip_test!(recursive_self_loop_detected, {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    let f = Node::new(NodeType::Function, "fact".into());
    let id = f.id;
    backend.insert_node(f).unwrap();
    backend
        .insert_edge(Edge::new(id, id, EdgeType::Calls))
        .unwrap();
    let cg = call_graph_from(&backend);
    let recursive = cg.recursive_functions();
    assert!(recursive.contains(&id));
});

ip_test!(recursive_mutual_pair, {
    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    let a = Node::new(NodeType::Function, "a".into());
    let b = Node::new(NodeType::Function, "b".into());
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
    let cg = call_graph_from(&backend);
    let recursive = cg.recursive_functions();
    assert!(recursive.contains(&id_a));
    assert!(recursive.contains(&id_b));
    assert!(cg.topological_order().is_err());
});

#[cfg(feature = "bundle-minimal")]
ip_test!(icfg_builds_per_function_cfg, {
    let backend = sample_backend();
    let source = r#"
fn main() {
    let data = 1;
    let result = helper(data);
}
fn helper(input: i32) -> i32 {
    input + 1
}
"#;
    let mut files = HashMap::new();
    files.insert("app.rs".into(), source.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    assert_eq!(icfg.function_cfgs.len(), 2);
});

#[cfg(feature = "bundle-minimal")]
ip_test!(icfg_get_cfg_by_id, {
    let backend = sample_backend();
    let source = r#"
fn main() { helper(1); }
fn helper(x: i32) -> i32 { x + 1 }
"#;
    let mut files = HashMap::new();
    files.insert("app.rs".into(), source.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let helper_id = icfg.call_graph.id_by_name("helper").unwrap();
    assert!(icfg.get_cfg(helper_id).is_some());
});

#[cfg(feature = "bundle-minimal")]
ip_test!(icfg_caller_cfgs, {
    let backend = sample_backend();
    let source = r#"
fn main() { helper(1); }
fn helper(x: i32) -> i32 { x + 1 }
"#;
    let mut files = HashMap::new();
    files.insert("app.rs".into(), source.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let helper_id = icfg.call_graph.id_by_name("helper").unwrap();
    let callers = icfg.caller_cfgs(helper_id);
    assert_eq!(callers.len(), 1);
});

#[cfg(feature = "bundle-minimal")]
ip_test!(interprocedural_slice_helper, {
    let backend = sample_backend();
    let source = r#"
fn main() {
    let data = 1;
    let result = helper(data);
}
fn helper(input: i32) -> i32 {
    input + 1
}
"#;
    let mut files = HashMap::new();
    files.insert("app.rs".into(), source.to_string());
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let helper_id = icfg.call_graph.id_by_name("helper").unwrap();
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();
    let pdg =
        ProgramDependenceGraph::build(icfg.get_cfg(helper_id).unwrap(), source.as_bytes()).unwrap();
    let line = pdg
        .nodes
        .values()
        .find(|n| n.statement.text.contains("input + 1"))
        .map(|n| n.statement.line)
        .unwrap_or(7);
    let slice = slicer
        .slice(
            helper_id,
            SliceCriterion {
                variable: "input".into(),
                line,
            },
        )
        .unwrap();
    assert!(slice.functions.contains(&helper_id));
});

#[cfg(feature = "bundle-minimal")]
ip_test!(multi_hop_slice_three_deep, {
    let (backend, files) = build_sample_backend_with_chain(4);
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let leaf = icfg.call_graph.id_by_name("f3").unwrap();
    let source = files.get("app.rs").unwrap();
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();
    let pdg =
        ProgramDependenceGraph::build(icfg.get_cfg(leaf).unwrap(), source.as_bytes()).unwrap();
    let line = pdg
        .nodes
        .values()
        .find(|n| n.statement.text.contains("input + 1"))
        .map(|n| n.statement.line)
        .unwrap_or(1);
    let slice = slicer
        .slice(
            leaf,
            SliceCriterion {
                variable: "input".into(),
                line,
            },
        )
        .unwrap();
    assert!(slice.functions.contains(&leaf));
});

ip_test!(parameters_from_graph_parameter, {
    let (backend, _) = build_backend_with_parameters();
    let cg = call_graph_from(&backend);
    let process_id = cg.id_by_name("process").unwrap();
    let params = cg.parameter_names(process_id);
    assert_eq!(params, &["input", "mode"]);
});

ip_test!(graph_parameter_stored_on_node, {
    let node = Node::new(NodeType::Function, "foo".into()).with_parameters(vec![GraphParameter {
        name: "x".into(),
        param_type: Some("i32".into()),
        default_value: None,
    }]);
    assert_eq!(node.parameters[0].name, "x");
    let id = node.id;
    let cg = call_graph_from(&{
        let mut b = rbuilder::graph::backend::MemoryBackend::new();
        b.insert_node(node).unwrap();
        b
    });
    assert_eq!(cg.parameter_names(id), &["x"]);
});

#[cfg(feature = "bundle-minimal")]
ip_test!(slice_reduction_percent_non_negative, {
    let (backend, files) = build_backend_with_parameters();
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let process_id = icfg.call_graph.id_by_name("process").unwrap();
    let source = files.get("chain.rs").unwrap();
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();
    let pdg = ProgramDependenceGraph::build(icfg.get_cfg(process_id).unwrap(), source.as_bytes())
        .unwrap();
    let line = pdg
        .nodes
        .values()
        .find(|n| n.statement.text.contains("trimmed"))
        .map(|n| n.statement.line)
        .unwrap_or(10);
    let slice = slicer
        .slice(
            process_id,
            SliceCriterion {
                variable: "trimmed".into(),
                line,
            },
        )
        .unwrap();
    assert!(slice.reduction_percent >= 0.0);
    assert!(!slice.lines.is_empty());
});

ip_test!(chain_backend_depth_three, {
    let (backend, files) = build_sample_backend_with_chain(3);
    let cg = call_graph_from(&backend);
    assert_eq!(cg.function_count(), 3);
    assert_eq!(cg.call_edge_count(), 2);
    assert!(files.get("app.rs").unwrap().contains("fn f0"));
});

ip_test!(chain_backend_depth_five, {
    let (backend, _) = build_sample_backend_with_chain(5);
    let cg = call_graph_from(&backend);
    assert_eq!(cg.function_count(), 5);
    let order = cg.topological_order().unwrap();
    assert_eq!(order.len(), 5);
});

ip_test!(call_graph_edge_call_type_default, {
    let mut cg = call_graph_from(&sample_backend());
    assert_eq!(cg.call_edge_count(), 1);
    assert_eq!(cg.edges()[0].call_site, 0);
});

ip_test!(call_graph_node_metadata, {
    let mut cg = call_graph_from(&sample_backend());
    let main = cg.nodes().values().find(|n| n.name == "main").unwrap();
    assert_eq!(main.file_path, "app.rs");
});

ip_test!(topological_order_preserves_edge_direction, {
    let (backend, _) = build_sample_backend_with_chain(4);
    let mut cg = call_graph_from(&backend);
    let order = cg.topological_order().unwrap();
    let pos: std::collections::HashMap<_, _> =
        order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    for edge in cg.edges().iter() {
        assert!(pos[&edge.from] < pos[&edge.to]);
    }
});

#[cfg(feature = "bundle-minimal")]
ip_test!(test_multi_argument_index_isolation, {
    use rbuilder::analysis::{
        criterion_for_parameter, filter_handoff_seeds_by_index, resolve_handoff_seeds,
        BlastRadiusEngine, InterproceduralCFG, InterproceduralSlicer,
    };

    let mut backend = rbuilder::graph::backend::MemoryBackend::new();
    let main = Node::new(NodeType::Function, "main".into()).with_file_path("vars.rs".into());
    let foo = Node::new(NodeType::Function, "foo".into())
        .with_file_path("vars.rs".into())
        .with_parameters(vec![
            GraphParameter {
                name: "x".into(),
                param_type: Some("i32".into()),
                default_value: None,
            },
            GraphParameter {
                name: "y".into(),
                param_type: Some("i32".into()),
                default_value: None,
            },
            GraphParameter {
                name: "z".into(),
                param_type: Some("i32".into()),
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
fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    foo(a, b, c);
}
fn foo(x: i32, y: i32, z: i32) -> i32 {
    let w = y + 1;
    let _x = x;
    let _z = z;
    w
}
"#;
    let mut files = HashMap::new();
    files.insert("vars.rs".into(), source.to_string());

    let engine = BlastRadiusEngine::build(&backend).unwrap();
    let blast = engine.analyze(id_foo).unwrap();
    let seeds = resolve_handoff_seeds(&backend, &blast, id_foo).unwrap();
    assert!(seeds
        .iter()
        .any(|s| s.param_index == 0 && s.param_name == "x"));
    assert!(seeds
        .iter()
        .any(|s| s.param_index == 1 && s.param_name == "y"));
    assert!(seeds
        .iter()
        .any(|s| s.param_index == 2 && s.param_name == "z"));

    let y_only = filter_handoff_seeds_by_index(&seeds, 1);
    assert_eq!(y_only.len(), 1);
    assert_eq!(y_only[0].param_index, 1);
    assert_eq!(y_only[0].param_name, "y");

    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();
    let criterion_y = criterion_for_parameter(&backend, &icfg, &files, id_foo, "y").unwrap();
    let slice_y = slicer.slice(id_foo, criterion_y).unwrap();
    assert!(!slice_y.lines.is_empty());

    let criterion_x = criterion_for_parameter(&backend, &icfg, &files, id_foo, "x").unwrap();
    let slice_x = slicer.slice(id_foo, criterion_x).unwrap();

    let y_line = source.lines().position(|l| l.contains("y + 1")).unwrap() + 1;
    assert!(slice_y.lines.contains(&y_line));
    assert!(!slice_x.lines.contains(&y_line));

    let criterion_z = criterion_for_parameter(&backend, &icfg, &files, id_foo, "z").unwrap();
    let slice_z = slicer.slice(id_foo, criterion_z).unwrap();
    assert!(!slice_z.lines.contains(&y_line));
});
