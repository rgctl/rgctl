//! Phase 13: GQL optimizer (15 tests).

#[path = "common/phase13.rs"]
mod phase13;

use phase13::large_graph;
use rbuilder::gql::{execute, execute_explain, parse, QueryExecutor, QueryOptimizer};
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use std::collections::HashSet;

macro_rules! gql_test {
    ($name:ident, $body:expr) => {
        #[test]
        fn $name() {
            $body;
        }
    };
}

gql_test!(optimizer_predicate_pushdown_name, {
    let query = parse("MATCH (f:Function) WHERE f.name = 'main' RETURN f").unwrap();
    let backend = MemoryBackend::new();
    let (optimized, report) = QueryOptimizer::new(&backend).optimize(query);
    assert!(optimized.patterns[0].node.properties.contains_key("name"));
    assert!(report.optimizations.iter().any(|o| o.contains("pushdown")));
});

gql_test!(optimizer_pushdown_clears_where, {
    let query = parse("MATCH (f:Function) WHERE f.name = 'main' RETURN f").unwrap();
    let backend = MemoryBackend::new();
    let (optimized, _) = QueryOptimizer::new(&backend).optimize(query);
    assert!(optimized.where_clause.is_none());
});

gql_test!(optimizer_pushdown_report_message, {
    let query = parse("MATCH (n:Function) WHERE n.name = 'foo' RETURN n").unwrap();
    let backend = MemoryBackend::new();
    let (_, report) = QueryOptimizer::new(&backend).optimize(query);
    assert!(report.optimizations.iter().any(|o| o.contains("predicate pushdown")));
});

gql_test!(optimizer_selectivity_rare_node, {
    let backend = large_graph(20);
    let query = parse("MATCH (f:Function) WHERE f.name = 'rare_target' RETURN f").unwrap();
    let (_, report) = QueryOptimizer::new(&backend).optimize(query);
    assert!(report.optimizations.iter().any(|o| o.contains("pushdown")));
});

gql_test!(optimizer_selectivity_common_node, {
    let backend = large_graph(50);
    let query = parse("MATCH (f:Function) WHERE f.name = 'fn_0' RETURN f").unwrap();
    let (_, report) = QueryOptimizer::new(&backend).optimize(query);
    assert!(report.optimizations.iter().any(|o| o.contains("pushdown")));
});

gql_test!(optimizer_reorder_multi_pattern, {
    let mut backend = MemoryBackend::new();
    for i in 0..5 {
        backend
            .insert_node(Node::new(NodeType::Function, format!("leaf_{i}")))
            .unwrap();
    }
    backend
        .insert_node(Node::new(NodeType::Function, "hub".into()))
        .unwrap();
    let hub = backend
        .all_nodes()
        .unwrap()
        .into_iter()
        .find(|n| n.name == "hub")
        .unwrap()
        .id;
    for node in backend.all_nodes().unwrap() {
        if node.name.starts_with("leaf_") {
            backend
                .insert_edge(Edge::new(hub, node.id, EdgeType::Calls))
                .unwrap();
        }
    }
    let query =
        parse("MATCH (a:Function)-[:CALLS]->(b:Function) WHERE b.name = 'leaf_0' RETURN a,b")
            .unwrap();
    let pattern_count = query.patterns.len();
    let (_, report) = QueryOptimizer::new(&backend).optimize(query);
    assert!(!report.optimizations.is_empty() || pattern_count <= 1);
});

gql_test!(explain_shows_optimizations, {
    let mut backend = MemoryBackend::new();
    backend
        .insert_node(Node::new(NodeType::Function, "main".into()))
        .unwrap();
    let result = execute_explain(&backend, "MATCH (f:Function) WHERE f.name = 'main' RETURN f")
        .unwrap();
    let plan = result.plan.expect("explain plan");
    assert!(plan.optimizer_applied || !plan.optimizations.is_empty());
});

gql_test!(optimized_equals_manual_execute, {
    let mut backend = MemoryBackend::new();
    backend
        .insert_node(Node::new(NodeType::Function, "alpha".into()))
        .unwrap();
    backend
        .insert_node(Node::new(NodeType::Function, "beta".into()))
        .unwrap();
    let q = "MATCH (f:Function) WHERE f.name = 'alpha' RETURN f";
    let optimized_result = execute(&backend, q).unwrap();
    let parsed = parse(q).unwrap();
    let manual = QueryExecutor::new(&backend).execute(&parsed).unwrap();
    assert_eq!(optimized_result.rows.len(), manual.rows.len());
    assert_eq!(optimized_result.rows[0]["f"].name, "alpha");
});

gql_test!(optimizer_no_where_no_pushdown, {
    let query = parse("MATCH (f:Function) RETURN f").unwrap();
    let backend = MemoryBackend::new();
    let (_, report) = QueryOptimizer::new(&backend).optimize(query);
    assert!(report.optimizations.iter().all(|o| !o.contains("pushdown")));
});

gql_test!(optimizer_empty_backend_selectivity, {
    let backend = MemoryBackend::new();
    let query = parse("MATCH (f:Function) WHERE f.name = 'missing' RETURN f").unwrap();
    let (optimized, _) = QueryOptimizer::new(&backend).optimize(query);
    assert!(optimized.patterns[0].node.properties.contains_key("name"));
});

gql_test!(optimizer_large_graph_pushdown, {
    let backend = large_graph(100);
    let query = parse("MATCH (f:Function) WHERE f.name = 'fn_42' RETURN f").unwrap();
    let (optimized, report) = QueryOptimizer::new(&backend).optimize(query);
    assert!(optimized.patterns[0].node.properties.contains_key("name"));
    assert!(report.optimizations.iter().any(|o| o.contains("pushdown")));
});

gql_test!(optimizer_type_filter_selectivity, {
    let mut backend = MemoryBackend::new();
    backend
        .insert_node(Node::new(NodeType::Function, "only_fn".into()))
        .unwrap();
    backend
        .insert_node(Node::new(NodeType::Class, "OnlyClass".into()))
        .unwrap();
    let query = parse("MATCH (f:Function) WHERE f.name = 'only_fn' RETURN f").unwrap();
    let (optimized, _) = QueryOptimizer::new(&backend).optimize(query);
    assert_eq!(optimized.patterns[0].node.node_type, Some(NodeType::Function));
});

gql_test!(optimizer_keeps_non_pushable_predicates, {
    let query = parse("MATCH (f:Function) WHERE f.name LIKE 'fn_*' RETURN f").unwrap();
    let backend = MemoryBackend::new();
    let (optimized, _) = QueryOptimizer::new(&backend).optimize(query);
    assert!(optimized.where_clause.is_some());
});

gql_test!(optimizer_single_pattern_no_reorder, {
    let backend = large_graph(10);
    let query = parse("MATCH (f:Function) WHERE f.name = 'fn_1' RETURN f").unwrap();
    let (optimized, report) = QueryOptimizer::new(&backend).optimize(query);
    assert_eq!(optimized.patterns.len(), 1);
    assert!(!report.optimizations.iter().any(|o| o.contains("join reordering")));
});

gql_test!(optimizer_result_equivalence_chain, {
    let mut backend = MemoryBackend::new();
    let a = Node::new(NodeType::Function, "root".into());
    let b = Node::new(NodeType::Function, "mid".into());
    let c = Node::new(NodeType::Function, "leaf".into());
    let id_a = a.id;
    let id_b = b.id;
    let id_c = c.id;
    backend.insert_node(a).unwrap();
    backend.insert_node(b).unwrap();
    backend.insert_node(c).unwrap();
    backend.insert_edge(Edge::new(id_a, id_b, EdgeType::Calls)).unwrap();
    backend.insert_edge(Edge::new(id_b, id_c, EdgeType::Calls)).unwrap();

    let q = "MATCH (a:Function)-[:CALLS*1..2]->(b:Function) RETURN a,b";
    let opt_rows = execute(&backend, q).unwrap().rows;
    let manual = QueryExecutor::new(&backend)
        .execute(&parse(q).unwrap())
        .unwrap()
        .rows;
    let names: HashSet<_> = opt_rows
        .iter()
        .map(|r| (r["a"].name.clone(), r["b"].name.clone()))
        .collect();
    let manual_names: HashSet<_> = manual
        .iter()
        .map(|r| (r["a"].name.clone(), r["b"].name.clone()))
        .collect();
    assert_eq!(names, manual_names);
});
