//! Phase 13: interprocedural analysis (20 tests).

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
    assert_eq!(cg.nodes.len(), 2);
    assert_eq!(cg.edges.len(), 1);
});

ip_test!(call_graph_callees_main, {
    let cg = call_graph_from(&sample_backend());
    let main_id = cg.nodes.values().find(|n| n.name == "main").unwrap().id;
    let helper_id = cg.nodes.values().find(|n| n.name == "helper").unwrap().id;
    assert_eq!(cg.callees(main_id), vec![helper_id]);
});

ip_test!(call_graph_callers_helper, {
    let cg = call_graph_from(&sample_backend());
    let main_id = cg.nodes.values().find(|n| n.name == "main").unwrap().id;
    let helper_id = cg.nodes.values().find(|n| n.name == "helper").unwrap().id;
    assert_eq!(cg.callers(helper_id), vec![main_id]);
});

ip_test!(topological_order_chain, {
    let (backend, _) = build_sample_backend_with_chain(4);
    let cg = call_graph_from(&backend);
    let order = cg.topological_order().unwrap();
    assert_eq!(order.len(), 4);
    let f0 = cg.nodes.values().find(|n| n.name == "f0").unwrap().id;
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
    let helper_id = icfg
        .call_graph
        .nodes
        .values()
        .find(|n| n.name == "helper")
        .unwrap()
        .id;
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
    let helper_id = icfg
        .call_graph
        .nodes
        .values()
        .find(|n| n.name == "helper")
        .unwrap()
        .id;
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
    let helper_id = icfg
        .call_graph
        .nodes
        .values()
        .find(|n| n.name == "helper")
        .unwrap()
        .id;
    let slicer = InterproceduralSlicer::new(&icfg, &files).unwrap();
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
    let leaf = icfg
        .call_graph
        .nodes
        .values()
        .find(|n| n.name == "f3")
        .unwrap()
        .id;
    let source = files.get("app.rs").unwrap();
    let slicer = InterproceduralSlicer::new(&icfg, &files).unwrap();
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
    let process_id = cg.nodes.values().find(|n| n.name == "process").unwrap().id;
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
    let cg = call_graph_from(&{
        let mut b = rbuilder::graph::backend::MemoryBackend::new();
        b.insert_node(node).unwrap();
        b
    });
    let id = cg.nodes.keys().next().unwrap();
    assert_eq!(cg.parameter_names(*id), &["x"]);
});

#[cfg(feature = "bundle-minimal")]
ip_test!(slice_reduction_percent_non_negative, {
    let (backend, files) = build_backend_with_parameters();
    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let process_id = icfg
        .call_graph
        .nodes
        .values()
        .find(|n| n.name == "process")
        .unwrap()
        .id;
    let source = files.get("chain.rs").unwrap();
    let slicer = InterproceduralSlicer::new(&icfg, &files).unwrap();
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
    assert_eq!(cg.nodes.len(), 3);
    assert_eq!(cg.edges.len(), 2);
    assert!(files.get("app.rs").unwrap().contains("fn f0"));
});

ip_test!(chain_backend_depth_five, {
    let (backend, _) = build_sample_backend_with_chain(5);
    let cg = call_graph_from(&backend);
    assert_eq!(cg.nodes.len(), 5);
    let order = cg.topological_order().unwrap();
    assert_eq!(order.len(), 5);
});

ip_test!(call_graph_edge_call_type_default, {
    let cg = call_graph_from(&sample_backend());
    assert_eq!(cg.edges.len(), 1);
    assert_eq!(cg.edges[0].call_site, 0);
});

ip_test!(call_graph_node_metadata, {
    let cg = call_graph_from(&sample_backend());
    let main = cg.nodes.values().find(|n| n.name == "main").unwrap();
    assert_eq!(main.file_path, "app.rs");
});

ip_test!(topological_order_preserves_edge_direction, {
    let (backend, _) = build_sample_backend_with_chain(4);
    let cg = call_graph_from(&backend);
    let order = cg.topological_order().unwrap();
    let pos: std::collections::HashMap<_, _> =
        order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    for edge in &cg.edges {
        assert!(pos[&edge.from] < pos[&edge.to]);
    }
});
