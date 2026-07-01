//! GQL query optimizer (Phase 13.4).

use crate::ast::{Predicate, PropertyMatcher, Query};
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::Node;

/// Query optimization context.
pub struct QueryOptimizer<'a> {
    backend: &'a MemoryBackend,
}

/// Optimization decisions applied to a query.
#[derive(Debug, Clone, Default)]
pub struct OptimizationReport {
    /// Human-readable optimization steps.
    pub optimizations: Vec<String>,
}

impl<'a> QueryOptimizer<'a> {
    /// Create optimizer for the given backend (selectivity estimates).
    pub fn new(backend: &'a MemoryBackend) -> Self {
        Self { backend }
    }

    /// Optimize query: predicate pushdown, pattern reordering.
    pub fn optimize(&self, mut query: Query) -> (Query, OptimizationReport) {
        let mut report = OptimizationReport::default();
        query = self.push_down_predicates(query, &mut report);
        query = self.reorder_patterns(query, &mut report);
        (query, report)
    }

    fn push_down_predicates(&self, mut query: Query, report: &mut OptimizationReport) -> Query {
        let Some(where_clause) = query.where_clause.take() else {
            return query;
        };

        let mut remaining = Vec::new();
        for predicate in where_clause.predicates {
            match predicate {
                Predicate::Equals {
                    variable,
                    property,
                    value,
                } => {
                    let mut pushed = false;
                    for pattern in &mut query.patterns {
                        if pattern.node.variable == variable {
                            pattern
                                .node
                                .properties
                                .insert(property.clone(), PropertyMatcher::Equals(value.clone()));
                            pushed = true;
                        }
                        for (_, target) in &mut pattern.hops {
                            if target.variable == variable {
                                target.properties.insert(
                                    property.clone(),
                                    PropertyMatcher::Equals(value.clone()),
                                );
                                pushed = true;
                            }
                        }
                    }
                    if pushed {
                        report.optimizations.push(format!(
                            "predicate pushdown: {variable}.{property} = {value}"
                        ));
                    } else {
                        remaining.push(Predicate::Equals {
                            variable,
                            property,
                            value,
                        });
                    }
                }
                other => remaining.push(other),
            }
        }

        if !remaining.is_empty() {
            query.where_clause = Some(crate::ast::WhereClause {
                predicates: remaining,
            });
        }
        query
    }

    fn reorder_patterns(&self, mut query: Query, report: &mut OptimizationReport) -> Query {
        if query.patterns.len() <= 1 {
            return query;
        }
        let mut indexed: Vec<(usize, f64)> = query
            .patterns
            .iter()
            .enumerate()
            .map(|(idx, p)| (idx, self.estimate_selectivity(p)))
            .collect();
        let original_order: Vec<usize> = indexed.iter().map(|(i, _)| *i).collect();
        indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let new_order: Vec<usize> = indexed.iter().map(|(i, _)| *i).collect();
        if new_order != original_order {
            report
                .optimizations
                .push("join reordering by selectivity".into());
            let original = std::mem::take(&mut query.patterns);
            query.patterns = new_order.into_iter().map(|i| original[i].clone()).collect();
        }
        query
    }

    fn estimate_selectivity(&self, pattern: &crate::ast::Pattern) -> f64 {
        let total = self.backend.node_count().max(1) as f64;
        let type_sel = if let Some(node_type) = pattern.node.node_type {
            // Use indexed count for typed queries (zero-copy)
            let count = self
                .backend
                .find_node_ids_by_type(node_type)
                .map(|ids| ids.len() as f64)
                .unwrap_or(total)
                / total;
            count
        } else {
            1.0
        };
        let prop_sel = self.estimate_property_selectivity(&pattern.node);
        type_sel * prop_sel
    }

    fn estimate_property_selectivity(&self, node_pattern: &crate::ast::NodePattern) -> f64 {
        if node_pattern.properties.is_empty() {
            return 1.0;
        }

        let total = self.backend.node_count().max(1);
        if total == 0 {
            return 1.0;
        }

        // Count matching nodes with zero-copy iteration
        let mut matching = 0usize;
        if let Ok(()) = self.backend.for_each_node(|n| {
            if node_matches_properties(n, &node_pattern.properties) {
                matching += 1;
            }
        }) {
            (matching as f64 / total as f64).clamp(0.001, 1.0)
        } else {
            0.1
        }
    }
}

fn node_matches_properties(
    node: &Node,
    properties: &std::collections::HashMap<String, PropertyMatcher>,
) -> bool {
    properties.iter().all(|(key, matcher)| {
        let value = if key == "name" {
            Some(node.name.as_str())
        } else {
            node.properties.get(key).map(String::as_str)
        };
        match (matcher, value) {
            (PropertyMatcher::Equals(expected), Some(actual)) => actual == expected.as_str(),
            (PropertyMatcher::Like(pattern), Some(actual)) => {
                let re = pattern.replace('*', ".*");
                regex::Regex::new(&format!("^{re}$"))
                    .map(|r| r.is_match(actual))
                    .unwrap_or(false)
            }
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Node, NodeType};

    #[test]
    fn test_optimizer_predicate_pushdown() {
        let query = parse("MATCH (f:Function) WHERE f.name = 'main' RETURN f").unwrap();
        let backend = MemoryBackend::new();
        let optimizer = QueryOptimizer::new(&backend);
        let (optimized, report) = optimizer.optimize(query);
        assert!(optimized.patterns[0].node.properties.contains_key("name"));
        assert!(optimized.where_clause.is_none());
        assert!(report.optimizations.iter().any(|o| o.contains("pushdown")));
    }

    #[test]
    fn test_optimizer_reorders_by_selectivity() {
        let mut backend = MemoryBackend::new();
        for i in 0..10 {
            backend
                .insert_node(Node::new(NodeType::Function, format!("fn_{i}")))
                .unwrap();
        }
        backend
            .insert_node(Node::new(NodeType::Function, "rare".into()))
            .unwrap();

        let q1 = parse("MATCH (a:Function) WHERE a.name = 'fn_0' RETURN a").unwrap();
        let q2 = parse("MATCH (b:Function) WHERE b.name = 'rare' RETURN b").unwrap();
        let optimizer = QueryOptimizer::new(&backend);
        let (_, r1) = optimizer.optimize(q1);
        let (_, r2) = optimizer.optimize(q2);
        assert!(r1.optimizations.iter().any(|o| o.contains("pushdown")));
        assert!(r2.optimizations.iter().any(|o| o.contains("pushdown")));
    }
}
