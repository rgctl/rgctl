//! GQL query execution against [`MemoryBackend`] (Phase 12.4).

use crate::ast::{
    EdgePattern, NodePattern, Pattern, Predicate, PropertyMatcher, Query, WhereClause,
};
use crate::explain::{ExplainPlan, ExplainStep};
use petgraph::Direction;
use rbuilder_analysis::graph_utils::PetGraphView;
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::{GraphBackend, MemoryBackend};
use rbuilder_graph::schema::{EdgeType, Node};
use std::collections::{HashMap, HashSet};

/// One row of query results keyed by bound variable name.
pub type Binding = HashMap<String, Node>;

/// Query execution output.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Matching rows
    pub rows: Vec<Binding>,
    /// Optional explain plan when requested
    pub plan: Option<ExplainPlan>,
}

/// Executes parsed GQL queries.
pub struct QueryExecutor<'a> {
    backend: &'a MemoryBackend,
    explain: bool,
    optimization_report: Option<crate::optimizer::OptimizationReport>,
}

impl<'a> QueryExecutor<'a> {
    /// Create an executor for the given backend.
    pub fn new(backend: &'a MemoryBackend) -> Self {
        Self {
            backend,
            explain: false,
            optimization_report: None,
        }
    }

    /// Enable explain-plan collection.
    pub fn with_explain(mut self, explain: bool) -> Self {
        self.explain = explain;
        self
    }

    /// Attach optimizer report to explain output.
    pub fn with_optimization_report(
        mut self,
        report: crate::optimizer::OptimizationReport,
    ) -> Self {
        self.optimization_report = Some(report);
        self
    }

    /// Execute a parsed query.
    pub fn execute(&self, query: &Query) -> Result<QueryResult> {
        let view = PetGraphView::from_backend(self.backend)?;
        let mut plan = if self.explain {
            let mut p = ExplainPlan::new();
            if let Some(ref report) = self.optimization_report {
                p.optimizer_applied = !report.optimizations.is_empty();
                p.optimizations = report.optimizations.clone();
            }
            Some(p)
        } else {
            None
        };

        let mut bindings = vec![HashMap::new()];
        for pattern in &query.patterns {
            bindings = self.match_pattern(&view, pattern, bindings, plan.as_mut())?;
        }

        if let Some(where_clause) = &query.where_clause {
            let before = bindings.len();
            bindings.retain(|b| eval_where(where_clause, b));
            if let Some(p) = plan.as_mut() {
                p.push(ExplainStep {
                    operation: "Filter".into(),
                    detail: format!("WHERE ({})", where_clause_summary(where_clause)),
                    rows_in: before,
                    rows_out: bindings.len(),
                });
            }
        }

        let mut rows: Vec<Binding> = bindings
            .into_iter()
            .map(|b| project_return(&query.return_clause.variables, b))
            .collect();

        if let Some(p) = plan.as_mut() {
            p.push(ExplainStep {
                operation: "Project".into(),
                detail: format!("RETURN {}", query.return_clause.variables.join(", ")),
                rows_in: rows.len(),
                rows_out: rows.len(),
            });
        }

        if let Some(limit) = query.limit {
            let before = rows.len();
            rows.truncate(limit);
            if let Some(p) = plan.as_mut() {
                p.push(ExplainStep {
                    operation: "Limit".into(),
                    detail: format!("LIMIT {limit}"),
                    rows_in: before,
                    rows_out: rows.len(),
                });
            }
        }

        Ok(QueryResult { rows, plan })
    }

    fn match_pattern(
        &self,
        view: &PetGraphView,
        pattern: &Pattern,
        bindings: Vec<Binding>,
        plan: Option<&mut ExplainPlan>,
    ) -> Result<Vec<Binding>> {
        let mut out = Vec::new();
        for binding in bindings {
            let candidates = self.match_node_pattern(&pattern.node, &binding)?;
            for node in candidates {
                let mut row = binding.clone();
                row.insert(pattern.node.variable.clone(), node);
                if pattern.hops.is_empty() {
                    out.push(row);
                } else {
                    out.extend(self.match_hops(
                        view,
                        &pattern.node.variable,
                        &pattern.hops,
                        row,
                    )?);
                }
            }
        }
        if let Some(p) = plan {
            p.push(ExplainStep {
                operation: "Match".into(),
                detail: format!(
                    "MATCH ({}{})",
                    pattern.node.variable,
                    pattern
                        .node
                        .node_type
                        .map(|t| format!(":{t:?}"))
                        .unwrap_or_default()
                ),
                rows_in: out.len(),
                rows_out: out.len(),
            });
        }
        Ok(out)
    }

    fn match_node_pattern(&self, pattern: &NodePattern, binding: &Binding) -> Result<Vec<Node>> {
        let mut matching_nodes = Vec::new();

        if let Some(node_type) = pattern.node_type {
            // Use indexed lookup for typed queries
            let node_ids = self.backend.find_node_ids_by_type(node_type)?;
            for node_id in node_ids {
                if let Ok(Some(node)) = self.backend.with_node(node_id, |n| {
                    if node_matches_pattern(n, pattern, binding) {
                        Some(n.clone())
                    } else {
                        None
                    }
                }) {
                    if let Some(n) = node {
                        matching_nodes.push(n);
                    }
                }
            }
        } else {
            // Untyped query: scan all nodes but only clone matches
            self.backend.for_each_node(|n| {
                if node_matches_pattern(n, pattern, binding) {
                    matching_nodes.push(n.clone());
                }
            })?;
        }

        Ok(matching_nodes)
    }

    fn match_hops(
        &self,
        view: &PetGraphView,
        start_var: &str,
        hops: &[(EdgePattern, NodePattern)],
        binding: Binding,
    ) -> Result<Vec<Binding>> {
        let mut rows = vec![binding];
        let mut current_var = start_var.to_string();
        for (edge, target) in hops {
            let mut next_rows = Vec::new();
            for row in rows {
                let start_node = row
                    .get(&current_var)
                    .cloned()
                    .ok_or_else(|| Error::QueryError(format!("unbound variable {current_var}")))?;
                let start_idx = view
                    .uuid_to_index
                    .get(&start_node.id)
                    .copied()
                    .ok_or_else(|| Error::NodeNotFound(start_node.name.clone()))?;

                for end_idx in traverse_edge(view, start_idx, edge) {
                    let end_uuid = view
                        .index_to_uuid
                        .get(&end_idx)
                        .copied()
                        .ok_or_else(|| Error::GraphError("missing node".into()))?;
                    let end_node = self.backend
                        .get_node(end_uuid)?
                        .ok_or_else(|| Error::NodeNotFound(end_uuid.to_string()))?;
                    if node_matches_pattern(&end_node, target, &row) {
                        let mut new_row = row.clone();
                        new_row.insert(target.variable.clone(), end_node);
                        next_rows.push(new_row);
                    }
                }
            }
            rows = next_rows;
            current_var = target.variable.clone();
        }
        Ok(rows)
    }
}

fn traverse_edge(
    view: &PetGraphView,
    start: petgraph::graph::NodeIndex,
    edge: &EdgePattern,
) -> Vec<petgraph::graph::NodeIndex> {
    let max = edge.max_hops.unwrap_or(10);
    let mut results = Vec::new();
    let mut queue = vec![(start, 0usize)];
    let mut visited_paths: HashSet<(petgraph::graph::NodeIndex, usize)> = HashSet::new();

    while let Some((node, depth)) = queue.pop() {
        if depth >= edge.min_hops && depth <= max && depth > 0 {
            results.push(node);
        }
        if depth >= max {
            continue;
        }
        for succ in view.directed.neighbors_directed(node, Direction::Outgoing) {
            if is_edge_type(view, node, succ, edge.edge_type)
                && visited_paths.insert((succ, depth + 1))
            {
                queue.push((succ, depth + 1));
            }
        }
    }
    results
}

fn is_edge_type(
    view: &PetGraphView,
    from: petgraph::graph::NodeIndex,
    to: petgraph::graph::NodeIndex,
    _edge_type: EdgeType,
) -> bool {
    // In zero-clone topology view, we include all edges
    // TODO: Filter by edge type for accurate GQL queries
    view.directed.find_edge(from, to).is_some()
}

fn node_matches_pattern(node: &Node, pattern: &NodePattern, binding: &Binding) -> bool {
    if let Some(node_type) = pattern.node_type {
        if node.node_type != node_type {
            return false;
        }
    }
    for (key, matcher) in &pattern.properties {
        if !property_matches(node, key, matcher) {
            return false;
        }
    }
    if let Some(bound) = binding.get(&pattern.variable) {
        if bound.id != node.id {
            return false;
        }
    }
    true
}

fn property_matches(node: &Node, key: &str, matcher: &PropertyMatcher) -> bool {
    let value = resolve_property(node, key);
    match matcher {
        PropertyMatcher::Equals(expected) => value.as_deref() == Some(expected.as_str()),
        PropertyMatcher::Like(pattern) => value
            .map(|v| glob_match(pattern, v.as_str()))
            .unwrap_or(false),
    }
}

fn resolve_property(node: &Node, key: &str) -> Option<String> {
    match key {
        "name" => Some(node.name.clone()),
        "type" => Some(format!("{:?}", node.node_type)),
        "signature" => node.signature_text().map(str::to_string),
        "return_type" => node.return_type_text().map(str::to_string),
        _ => node.get_property(key).map(String::from),
    }
}

fn eval_where(where_clause: &WhereClause, binding: &Binding) -> bool {
    where_clause
        .predicates
        .iter()
        .all(|p| eval_predicate(p, binding))
}

fn eval_predicate(predicate: &Predicate, binding: &Binding) -> bool {
    match predicate {
        Predicate::Equals {
            variable,
            property,
            value,
        } => binding
            .get(variable)
            .map(|n| resolve_property(n, property).as_deref() == Some(value.as_str()))
            .unwrap_or(false),
        Predicate::Like {
            variable,
            property,
            pattern,
        } => binding
            .get(variable)
            .and_then(|n| resolve_property(n, property))
            .map(|v| glob_match(pattern, &v))
            .unwrap_or(false),
    }
}

fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(rest) = pattern.strip_prefix('*') {
        if rest.is_empty() {
            return true;
        }
        return value.ends_with(rest);
    }
    if let Some(rest) = pattern.strip_suffix('*') {
        if rest.is_empty() {
            return true;
        }
        return value.starts_with(rest);
    }
    pattern == value
}

fn project_return(variables: &[String], binding: Binding) -> Binding {
    variables
        .iter()
        .filter_map(|v| binding.get(v).map(|n| (v.clone(), n.clone())))
        .collect()
}

fn where_clause_summary(where_clause: &WhereClause) -> String {
    where_clause
        .predicates
        .iter()
        .map(|p| match p {
            Predicate::Equals {
                variable,
                property,
                value,
            } => format!("{variable}.{property} = '{value}'"),
            Predicate::Like {
                variable,
                property,
                pattern,
            } => format!("{variable}.{property} LIKE '{pattern}'"),
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, EdgeType, Node, NodeType};

    fn call_chain() -> MemoryBackend {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a".to_string());
        let b = Node::new(NodeType::Function, "b".to_string());
        let c = Node::new(NodeType::Function, "c".to_string());
        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_node(c).unwrap();
        backend
            .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_b, id_c, EdgeType::Calls))
            .unwrap();
        backend
    }

    #[test]
    fn test_execute_name_filter() {
        let backend = call_chain();
        let query = parse("MATCH (n:Function) WHERE n.name = 'foo' RETURN n LIMIT 10").unwrap();
        let result = QueryExecutor::new(&backend).execute(&query).unwrap();
        assert!(result.rows.is_empty());
    }

    #[test]
    fn test_execute_multi_hop() {
        let backend = call_chain();
        let query = parse("MATCH (a:Function)-[:CALLS*1..2]->(b:Function) RETURN a,b").unwrap();
        let result = QueryExecutor::new(&backend).execute(&query).unwrap();
        assert!(!result.rows.is_empty());
    }
}
