//! Graph query language AST (Phase 12.4).

use rbuilder_graph::schema::{EdgeType, NodeType};
use std::collections::HashMap;

/// A parsed GQL query.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// MATCH patterns
    pub patterns: Vec<Pattern>,
    /// Optional WHERE filter
    pub where_clause: Option<WhereClause>,
    /// RETURN projection
    pub return_clause: ReturnClause,
    /// Optional row limit
    pub limit: Option<usize>,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            where_clause: None,
            return_clause: ReturnClause {
                variables: Vec::new(),
            },
            limit: None,
        }
    }
}

/// A MATCH pattern: node followed by zero or more edge-node pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    /// Starting node pattern
    pub node: NodePattern,
    /// Chained edge and target node patterns
    pub hops: Vec<(EdgePattern, NodePattern)>,
}

/// Node pattern `(var:Type {props})`.
#[derive(Debug, Clone, PartialEq)]
pub struct NodePattern {
    /// Binding variable name
    pub variable: String,
    /// Optional node type label
    pub node_type: Option<NodeType>,
    /// Inline property matchers
    pub properties: HashMap<String, PropertyMatcher>,
}

/// Edge pattern `-[:TYPE*min..max]->`.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePattern {
    /// Relationship type
    pub edge_type: EdgeType,
    /// Minimum hop count (inclusive)
    pub min_hops: usize,
    /// Maximum hop count (inclusive); `None` means unbounded
    pub max_hops: Option<usize>,
}

/// WHERE clause with one or more predicates (AND-ed).
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    /// Predicates combined with logical AND
    pub predicates: Vec<Predicate>,
}

/// A single WHERE predicate.
#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    /// `var.prop = value`
    Equals {
        /// Bound variable
        variable: String,
        /// Property name
        property: String,
        /// Expected value
        value: String,
    },
    /// `var.prop LIKE pattern` (supports `*` wildcards)
    Like {
        /// Bound variable
        variable: String,
        /// Property name
        property: String,
        /// Glob-like pattern
        pattern: String,
    },
}

/// RETURN clause listing bound variables.
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnClause {
    /// Variable names to project
    pub variables: Vec<String>,
}

/// Property comparison operators in patterns and WHERE.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyMatcher {
    /// Exact string match
    Equals(String),
    /// Glob pattern with `*`
    Like(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_query() {
        let q = Query::default();
        assert!(q.patterns.is_empty());
        assert!(q.limit.is_none());
    }
}
