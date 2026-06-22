//! Graph query language (GQL) — Phase 12.4.
//!
//! Simplified Cypher-like queries over the in-memory code graph backend.

pub mod ast;
pub mod executor;
pub mod explain;
pub mod macros;
pub mod optimizer;
pub mod parser;

pub use ast::{
    EdgePattern, NodePattern, Pattern, Predicate, PropertyMatcher, Query, ReturnClause, WhereClause,
};
pub use executor::{Binding, QueryExecutor, QueryResult};
pub use explain::{ExplainPlan, ExplainStep};
pub use macros::{QueryMacro, QueryMacroRegistry};
pub use optimizer::{OptimizationReport, QueryOptimizer};
pub use parser::parse;

use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;

/// Parse, optimize, and execute a GQL query string against the backend.
pub fn execute(backend: &MemoryBackend, query: &str) -> Result<QueryResult> {
    let parsed = parse(query)?;
    let (optimized, _) = QueryOptimizer::new(backend).optimize(parsed);
    QueryExecutor::new(backend).execute(&optimized)
}

/// Parse, optimize, and execute with explain plan collection.
pub fn execute_explain(backend: &MemoryBackend, query: &str) -> Result<QueryResult> {
    let parsed = parse(query)?;
    let (optimized, report) = QueryOptimizer::new(backend).optimize(parsed);
    QueryExecutor::new(backend)
        .with_explain(true)
        .with_optimization_report(report)
        .execute(&optimized)
}

/// Resolve a named macro and execute it.
pub fn execute_macro(
    backend: &MemoryBackend,
    registry: &QueryMacroRegistry,
    name: &str,
) -> Result<QueryResult> {
    let query = registry.resolve(name)?;
    execute(backend, query)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Node, NodeType};

    #[test]
    fn test_execute_helper() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(Node::new(NodeType::Function, "main".to_string()))
            .unwrap();
        let result = execute(
            &backend,
            "MATCH (f:Function) WHERE f.name = 'main' RETURN f",
        )
        .unwrap();
        assert_eq!(result.rows.len(), 1);
    }
}
