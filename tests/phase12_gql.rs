//! Phase 12.4 — GQL integration tests

use rbuilder::gql::{
    execute, execute_explain, execute_macro, parse, QueryExecutor, QueryMacroRegistry,
};
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use std::collections::HashSet;

fn sample_graph() -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    let main = Node::new(NodeType::Function, "main".to_string());
    let auth = Node::new(NodeType::Function, "authenticate".to_string());
    let db = Node::new(NodeType::Function, "execute_query".to_string());
    let id_main = main.id;
    let id_auth = auth.id;
    let id_db = db.id;
    backend.insert_node(main).unwrap();
    backend.insert_node(auth).unwrap();
    backend.insert_node(db).unwrap();
    backend
        .insert_edge(Edge::new(id_main, id_auth, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_auth, id_db, EdgeType::Calls))
        .unwrap();
    backend
}

#[test]
fn test_parse_where_limit_query() {
    let q = parse("MATCH (n:Function) WHERE n.name = 'foo' RETURN n LIMIT 10").unwrap();
    assert_eq!(q.limit, Some(10));
    assert!(q.where_clause.is_some());
}

#[test]
fn test_parse_multi_hop_pattern() {
    let q = parse("MATCH (a:Function)-[:CALLS*1..2]->(b:Function) RETURN a,b").unwrap();
    let (edge, target) = &q.patterns[0].hops[0];
    assert_eq!(edge.min_hops, 1);
    assert_eq!(edge.max_hops, Some(2));
    assert_eq!(target.variable, "b");
}

#[test]
fn test_execute_name_filter() {
    let backend = sample_graph();
    let result = execute(
        &backend,
        "MATCH (f:Function) WHERE f.name = 'authenticate' RETURN f",
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0]["f"].name, "authenticate");
}

#[test]
fn test_execute_multi_hop_calls() {
    let backend = sample_graph();
    let result = execute(
        &backend,
        "MATCH (a:Function)-[:CALLS*1..2]->(b:Function) RETURN a,b",
    )
    .unwrap();

    let pairs: HashSet<(String, String)> = result
        .rows
        .iter()
        .map(|row| (row["a"].name.clone(), row["b"].name.clone()))
        .collect();

    assert!(pairs.contains(&("main".into(), "authenticate".into())));
    assert!(pairs.contains(&("authenticate".into(), "execute_query".into())));
    assert!(pairs.contains(&("main".into(), "execute_query".into())));
}

#[test]
fn test_execute_limit() {
    let backend = sample_graph();
    let result = execute(&backend, "MATCH (n:Function) RETURN n LIMIT 2").unwrap();
    assert_eq!(result.rows.len(), 2);
}

#[test]
fn test_explain_plan_steps() {
    let backend = sample_graph();
    let result = execute_explain(
        &backend,
        "MATCH (f:Function) WHERE f.name = 'main' RETURN f LIMIT 1",
    )
    .unwrap();
    let plan = result.plan.expect("explain plan");
    assert!(plan.steps.iter().any(|s| s.operation == "Match"));
    assert!(
        plan.steps.iter().any(|s| s.operation == "Filter")
            || plan.optimizer_applied
            || plan.optimizations.iter().any(|o| o.contains("pushdown")),
        "expected filter step or predicate pushdown optimization"
    );
    assert!(plan.steps.iter().any(|s| s.operation == "Limit"));
}

#[test]
fn test_query_macro_registry() {
    let backend = sample_graph();
    let registry = QueryMacroRegistry::with_defaults();
    let result = execute_macro(&backend, &registry, "all_functions").unwrap();
    assert_eq!(result.rows.len(), 3);
}

#[test]
fn test_query_executor_with_explain_flag() {
    let backend = sample_graph();
    let query = parse("MATCH (a:Function)-[:CALLS*1..1]->(b:Function) RETURN a,b").unwrap();
    let result = QueryExecutor::new(&backend)
        .with_explain(true)
        .execute(&query)
        .unwrap();
    assert!(!result.rows.is_empty());
    assert!(result.plan.is_some());
}
