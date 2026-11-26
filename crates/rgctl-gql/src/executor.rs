//! GQL query execution against [`MemoryBackend`] (Phase 12.4).

use crate::ast::{
    EdgePattern, NodePattern, Pattern, Predicate, PropertyMatcher, Query, WhereClause,
};
use crate::explain::{ExplainPlan, ExplainStep};
use rgctl_analysis::graph_utils::PetGraphView;
use rgctl_analysis::{CommunityQueryContext, is_virtual_community};
use rgctl_error::{Error, Result};
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::Node;
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
    community: Option<&'a CommunityQueryContext>,
    explain: bool,
    optimization_report: Option<crate::optimizer::OptimizationReport>,
}

impl<'a> QueryExecutor<'a> {
    /// Create an executor for the given backend.
    pub fn new(backend: &'a MemoryBackend) -> Self {
        Self {
            backend,
            community: None,
            explain: false,
            optimization_report: None,
        }
    }

    /// Attach community overlay (virtual `:Community` + `community_id`).
    pub fn with_community(mut self, community: Option<&'a CommunityQueryContext>) -> Self {
        self.community = community;
        self
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
            bindings.retain(|b| eval_where(where_clause, b, self.community));
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
            let type_label = if pattern.node.match_community {
                ":Community".into()
            } else {
                pattern
                    .node
                    .node_type
                    .map(|t| format!(":{t:?}"))
                    .unwrap_or_default()
            };
            p.push(ExplainStep {
                operation: "Match".into(),
                detail: format!("MATCH ({}{})", pattern.node.variable, type_label),
                rows_in: out.len(),
                rows_out: out.len(),
            });
        }
        Ok(out)
    }

    fn match_node_pattern(&self, pattern: &NodePattern, binding: &Binding) -> Result<Vec<Node>> {
        if pattern.match_community {
            let Some(ctx) = self.community else {
                return Ok(Vec::new());
            };
            let mut matching = Vec::new();
            for node in ctx.community_nodes() {
                if node_matches_pattern(&node, pattern, binding, self.community) {
                    matching.push(node);
                }
            }
            return Ok(matching);
        }

        let mut matching_nodes = Vec::new();

        if let Some(node_type) = pattern.node_type {
            let node_ids = self.backend.find_node_ids_by_type(node_type)?;
            for node_id in node_ids {
                if let Ok(Some(Some(n))) = self.backend.with_node(node_id, |node| {
                    if node_matches_pattern(node, pattern, binding, self.community) {
                        Some(node.clone())
                    } else {
                        None
                    }
                }) {
                    matching_nodes.push(n);
                }
            }
        } else {
            self.backend.for_each_node(|n| {
                if node_matches_pattern(n, pattern, binding, self.community) {
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
            if target.match_community {
                return Err(Error::InvalidQuery(
                    "virtual :Community nodes cannot appear as hop targets".into(),
                ));
            }
            let mut next_rows = Vec::new();
            for row in rows {
                let start_node = row
                    .get(&current_var)
                    .cloned()
                    .ok_or_else(|| Error::QueryError(format!("unbound variable {current_var}")))?;
                if is_virtual_community(&start_node) {
                    return Err(Error::InvalidQuery(
                        "cannot traverse edges from virtual :Community nodes".into(),
                    ));
                }
                let start_idx = view
                    .get_index(start_node.id)
                    .ok_or_else(|| Error::NodeNotFound(start_node.name.to_string()))?;

                for end_idx in traverse_edge(view, start_idx, edge) {
                    let end_uuid = view
                        .get_uuid(end_idx)
                        .ok_or_else(|| Error::GraphError("missing node".into()))?;
                    let end_node = self
                        .backend
                        .get_node(end_uuid)?
                        .ok_or_else(|| Error::NodeNotFound(end_uuid.to_string()))?;
                    if node_matches_pattern(&end_node, target, &row, self.community) {
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
    let allowed = [edge.edge_type];

    while let Some((node, depth)) = queue.pop() {
        if depth >= edge.min_hops && depth <= max && depth > 0 {
            results.push(node);
        }
        if depth >= max {
            continue;
        }
        for succ in view.outgoing_filtered(node, &allowed) {
            if visited_paths.insert((succ, depth + 1)) {
                queue.push((succ, depth + 1));
            }
        }
    }
    results
}

fn node_matches_pattern(
    node: &Node,
    pattern: &NodePattern,
    binding: &Binding,
    community: Option<&CommunityQueryContext>,
) -> bool {
    if pattern.match_community {
        if !is_virtual_community(node) {
            return false;
        }
    } else if let Some(node_type) = pattern.node_type
        && node.node_type != node_type
    {
        return false;
    }
    for (key, matcher) in &pattern.properties {
        if !property_matches(node, key, matcher, community) {
            return false;
        }
    }
    if let Some(bound) = binding.get(&pattern.variable)
        && bound.id != node.id
    {
        return false;
    }
    true
}

fn property_matches(
    node: &Node,
    key: &str,
    matcher: &PropertyMatcher,
    community: Option<&CommunityQueryContext>,
) -> bool {
    let value = resolve_property(node, key, community);
    match matcher {
        PropertyMatcher::Equals(expected) => value.as_deref() == Some(expected.as_str()),
        PropertyMatcher::Like(pattern) => value
            .map(|v| glob_match(pattern, v.as_str()))
            .unwrap_or(false),
    }
}

fn resolve_property(
    node: &Node,
    key: &str,
    community: Option<&CommunityQueryContext>,
) -> Option<String> {
    match key {
        "name" => Some(node.name.to_string()),
        "qualified_name" => node.qualified_name.as_ref().map(|s| s.to_string()),
        "type" => {
            if is_virtual_community(node) {
                Some("Community".into())
            } else {
                Some(format!("{:?}", node.node_type))
            }
        }
        "label" if is_virtual_community(node) => node
            .get_property("label")
            .map(String::from)
            .or_else(|| Some(node.name.to_string())),
        "signature" => node.signature_text().map(str::to_string),
        "return_type" => node.return_type_text().map(str::to_string),
        "community_id" => {
            if let Some(v) = node.get_property("community_id") {
                return Some(v.to_string());
            }
            community
                .and_then(|ctx| ctx.community_id(node.id))
                .map(|id| id.to_string())
        }
        "file_path" => node.file_path.as_ref().map(|s| s.to_string()),
        _ => node.get_property(key).map(String::from),
    }
}

fn eval_where(
    where_clause: &WhereClause,
    binding: &Binding,
    community: Option<&CommunityQueryContext>,
) -> bool {
    where_clause
        .predicates
        .iter()
        .all(|p| eval_predicate(p, binding, community))
}

fn eval_predicate(
    predicate: &Predicate,
    binding: &Binding,
    community: Option<&CommunityQueryContext>,
) -> bool {
    match predicate {
        Predicate::Equals {
            variable,
            property,
            value,
        } => binding
            .get(variable)
            .map(|n| resolve_property(n, property, community).as_deref() == Some(value.as_str()))
            .unwrap_or(false),
        Predicate::Like {
            variable,
            property,
            pattern,
        } => binding
            .get(variable)
            .and_then(|n| resolve_property(n, property, community))
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
    use rgctl_analysis::results::AnalysisResults;
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};

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

    /// Issue #49: FQN lives on `Node.qualified_name`, not `name` / properties map.
    #[test]
    fn test_execute_qualified_name_filter() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Class, "Context".to_string())
            .with_qualified_name("org.openmrs.api.context.Context".to_string());
        backend.insert_node(node).unwrap();

        let by_simple = parse("MATCH (n:Class) WHERE n.name = 'Context' RETURN n").unwrap();
        assert_eq!(
            QueryExecutor::new(&backend)
                .execute(&by_simple)
                .unwrap()
                .rows
                .len(),
            1,
            "simple name should match"
        );

        let by_fqn_on_name =
            parse("MATCH (n:Class) WHERE n.name = 'org.openmrs.api.context.Context' RETURN n")
                .unwrap();
        assert!(
            QueryExecutor::new(&backend)
                .execute(&by_fqn_on_name)
                .unwrap()
                .rows
                .is_empty(),
            "FQN must not match n.name"
        );

        let by_qn = parse(
            "MATCH (n:Class) WHERE n.qualified_name = 'org.openmrs.api.context.Context' RETURN n",
        )
        .unwrap();
        assert_eq!(
            QueryExecutor::new(&backend)
                .execute(&by_qn)
                .unwrap()
                .rows
                .len(),
            1,
            "WHERE n.qualified_name should resolve Node.qualified_name"
        );

        let by_like = parse(
            "MATCH (n:Class) WHERE n.qualified_name LIKE 'org.openmrs.api.context.*' RETURN n",
        )
        .unwrap();
        assert_eq!(
            QueryExecutor::new(&backend)
                .execute(&by_like)
                .unwrap()
                .rows
                .len(),
            1,
            "LIKE on qualified_name should work"
        );

        let by_inline =
            parse("MATCH (n:Class {qualified_name: 'org.openmrs.api.context.Context'}) RETURN n")
                .unwrap();
        assert_eq!(
            QueryExecutor::new(&backend)
                .execute(&by_inline)
                .unwrap()
                .rows
                .len(),
            1,
            "inline property matcher should use qualified_name"
        );
    }

    #[test]
    fn test_execute_multi_hop() {
        let backend = call_chain();
        let query = parse("MATCH (a:Function)-[:CALLS*1..2]->(b:Function) RETURN a,b").unwrap();
        let result = QueryExecutor::new(&backend).execute(&query).unwrap();
        assert!(!result.rows.is_empty());
    }

    #[test]
    fn test_virtual_community_list() {
        let backend = call_chain();
        let ids: Vec<_> = backend.find_node_ids_by_type(NodeType::Function).unwrap();
        let mut analysis = AnalysisResults::new(ids.clone());
        let c0 = analysis.get_compact_id(ids[0]).unwrap();
        let c1 = analysis.get_compact_id(ids[1]).unwrap();
        let c2 = analysis.get_compact_id(ids[2]).unwrap();
        let table = analysis.init_community();
        table.num_communities = 2;
        table.modularity = 0.5;
        table.assignments[c0 as usize] = 1;
        table.assignments[c1 as usize] = 1;
        table.assignments[c2 as usize] = 2;
        table.labels.insert(1, "auth".into());
        table.labels.insert(2, "api".into());

        let ctx = CommunityQueryContext::from_analysis(&analysis, |_| None);
        let query = parse("MATCH (c:Community) RETURN c").unwrap();
        let result = QueryExecutor::new(&backend)
            .with_community(Some(&ctx))
            .execute(&query)
            .unwrap();
        assert_eq!(result.rows.len(), 2);

        let query = parse("MATCH (f:Function) WHERE f.community_id = '1' RETURN f").unwrap();
        let result = QueryExecutor::new(&backend)
            .with_community(Some(&ctx))
            .execute(&query)
            .unwrap();
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn test_file_path_property_filter() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Module, "Checkout Flow".to_string())
            .with_file_path("docs/guide.md".to_string())
            .with_property("kind".to_string(), "heading".to_string());
        backend.insert_node(node).unwrap();

        let query = parse("MATCH (n:Module) WHERE n.file_path = 'docs/guide.md' RETURN n").unwrap();
        let result = QueryExecutor::new(&backend).execute(&query).unwrap();
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn test_file_path_like_suffix() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Module, "Checkout Flow".to_string())
            .with_file_path("repo/docs/guide.md".to_string())
            .with_property("kind".to_string(), "heading".to_string());
        backend.insert_node(node).unwrap();

        let query = parse("MATCH (n:Module) WHERE n.file_path LIKE '*guide.md' RETURN n").unwrap();
        let result = QueryExecutor::new(&backend).execute(&query).unwrap();
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn test_kind_property_on_heading_nodes() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Module, "Payments".to_string())
            .with_property("kind".to_string(), "heading".to_string());
        backend.insert_node(node).unwrap();

        let query =
            parse("MATCH (n:Module) WHERE n.kind = 'heading' AND n.name = 'Payments' RETURN n")
                .unwrap();
        assert_eq!(
            QueryExecutor::new(&backend)
                .execute(&query)
                .unwrap()
                .rows
                .len(),
            1
        );
    }

    #[test]
    fn test_community_without_context_empty() {
        let backend = call_chain();
        let query = parse("MATCH (c:Community) RETURN c").unwrap();
        let result = QueryExecutor::new(&backend).execute(&query).unwrap();
        assert!(result.rows.is_empty());
    }
}
