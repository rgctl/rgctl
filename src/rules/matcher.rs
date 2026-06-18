//! Rule matching engine
//!
//! Task 3.1.2: Match nodes against rule conditions

use crate::error::Result;
use crate::graph::backend::trait_def::GraphBackend;
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::{EdgeType, Node, NodeType};
use crate::rules::schema::{MatchCondition, MatchLeaf, Rule};
use regex::Regex;
use std::collections::HashSet;
use uuid::Uuid;

/// Context for evaluating graph-aware rule conditions.
pub struct MatchContext {
    outgoing_calls: HashSet<Uuid>,
}

impl MatchContext {
    fn new(backend: &MemoryBackend, node_id: Uuid) -> Result<Self> {
        let edges = backend.all_edges()?;
        let outgoing_calls = edges
            .iter()
            .filter(|e| e.from == node_id && e.edge_type == EdgeType::Calls)
            .map(|e| e.to)
            .collect();
        Ok(Self {
            outgoing_calls,
        })
    }
}

/// Matches nodes against rule conditions.
pub struct RuleMatcher;

impl RuleMatcher {
    /// Check if a node matches a rule.
    pub fn matches(backend: &MemoryBackend, rule: &Rule, node: &Node) -> Result<bool> {
        let ctx = MatchContext::new(backend, node.id)?;
        Self::match_condition(backend, &ctx, node, &rule.match_condition)
    }

    fn match_condition(
        backend: &MemoryBackend,
        ctx: &MatchContext,
        node: &Node,
        condition: &MatchCondition,
    ) -> Result<bool> {
        match condition {
            MatchCondition::And { and } => and
                .iter()
                .try_fold(true, |acc, c| {
                    Ok(acc && Self::match_condition(backend, ctx, node, c)?)
                }),
            MatchCondition::Or { or } => or
                .iter()
                .try_fold(false, |acc, c| {
                    Ok(acc || Self::match_condition(backend, ctx, node, c)?)
                }),
            MatchCondition::Not { not } => {
                Ok(!Self::match_condition(backend, ctx, node, not)?)
            }
            MatchCondition::Leaf(leaf) => Self::match_leaf(backend, ctx, node, leaf),
        }
    }

    fn match_leaf(
        backend: &MemoryBackend,
        ctx: &MatchContext,
        node: &Node,
        leaf: &MatchLeaf,
    ) -> Result<bool> {
        match leaf {
            MatchLeaf::NodeType { value } => Ok(node_type_name(node.node_type).eq_ignore_ascii_case(value)),
            MatchLeaf::NamePattern { pattern } => {
                let re = Regex::new(pattern).map_err(|e| {
                    crate::error::Error::ConfigError(format!("Invalid regex '{pattern}': {e}"))
                })?;
                Ok(re.is_match(&node.name))
            }
            MatchLeaf::HasLabel { value } => Ok(node.has_label(value)),
            MatchLeaf::ComplexityGt { value } => {
                Ok(node_cyclomatic(node).map(|c| c > *value).unwrap_or(false))
            }
            MatchLeaf::ComplexityLt { value } => {
                Ok(node_cyclomatic(node).map(|c| c < *value).unwrap_or(false))
            }
            MatchLeaf::HasProperty { key, value } => match value {
                Some(v) => Ok(node.get_property(key).map(|p| p == v).unwrap_or(false)),
                None => Ok(node.get_property(key).is_some()),
            },
            MatchLeaf::CallsAny { symbols } => {
                for target_id in &ctx.outgoing_calls {
                    if let Ok(Some(target)) = backend.get_node(*target_id) {
                        if symbols.iter().any(|s| target.name.contains(s)) {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            MatchLeaf::NodeTypeField {
                node_type,
                name_pattern,
            } => {
                if !node_type_name(node.node_type).eq_ignore_ascii_case(node_type) {
                    return Ok(false);
                }
                if let Some(pattern) = name_pattern {
                    let re = Regex::new(pattern).map_err(|e| {
                        crate::error::Error::ConfigError(format!("Invalid regex '{pattern}': {e}"))
                    })?;
                    Ok(re.is_match(&node.name))
                } else {
                    Ok(true)
                }
            }
        }
    }
}

fn node_type_name(node_type: NodeType) -> &'static str {
    match node_type {
        NodeType::Function => "Function",
        NodeType::Class => "Class",
        NodeType::Struct => "Struct",
        NodeType::Enum => "Enum",
        NodeType::Interface => "Interface",
        NodeType::Module => "Module",
        NodeType::Variable => "Variable",
        NodeType::File => "File",
        NodeType::ConfigKey => "ConfigKey",
        NodeType::TypeAlias => "TypeAlias",
        NodeType::Macro => "Macro",
        NodeType::Import => "Import",
        NodeType::Table => "Table",
        NodeType::Dependency => "Dependency",
        NodeType::Job => "Job",
        NodeType::BuildStep => "BuildStep",
        NodeType::AnsiblePlaybook => "AnsiblePlaybook",
        NodeType::AnsiblePlay => "AnsiblePlay",
        NodeType::AnsibleTask => "AnsibleTask",
        NodeType::AnsibleRole => "AnsibleRole",
        NodeType::AnsibleHandler => "AnsibleHandler",
        NodeType::AnsibleVariable => "AnsibleVariable",
        NodeType::AnsibleTemplate => "AnsibleTemplate",
        NodeType::ChefCookbook => "ChefCookbook",
        NodeType::ChefRecipe => "ChefRecipe",
        NodeType::ChefResource => "ChefResource",
        NodeType::ChefAttribute => "ChefAttribute",
        NodeType::ChefTemplate => "ChefTemplate",
        NodeType::ChefCustomResource => "ChefCustomResource",
    }
}

fn node_cyclomatic(node: &Node) -> Option<usize> {
    node.get_property("cyclomatic").and_then(|v| v.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::schema::{Rule, RuleAction};

    fn auth_node() -> Node {
        let mut node = Node::new(NodeType::Function, "authenticate_user".to_string());
        node.properties.insert("cyclomatic".to_string(), "12".to_string());
        node
    }

    #[test]
    fn test_rule_matching() {
        let rule = Rule {
            name: "security".to_string(),
            description: None,
            match_condition: MatchCondition::Leaf(MatchLeaf::NamePattern {
                pattern: "(?i)(auth|login|verify)".to_string(),
            }),
            actions: vec![],
        };
        let backend = MemoryBackend::new();
        assert!(RuleMatcher::matches(&backend, &rule, &auth_node()).unwrap());
    }

    #[test]
    fn test_composite_conditions() {
        let rule = Rule {
            name: "complex_test".to_string(),
            description: None,
            match_condition: MatchCondition::And {
                and: vec![
                    MatchCondition::Leaf(MatchLeaf::NamePattern {
                        pattern: ".*_test$".to_string(),
                    }),
                    MatchCondition::Leaf(MatchLeaf::ComplexityGt { value: 10 }),
                ],
            },
            actions: vec![RuleAction::AddLabel {
                label: "test".to_string(),
            }],
        };

        let mut complex = Node::new(NodeType::Function, "complex_test".to_string());
        complex.properties.insert("cyclomatic".to_string(), "15".to_string());
        let mut simple = Node::new(NodeType::Function, "simple_test".to_string());
        simple.properties.insert("cyclomatic".to_string(), "5".to_string());

        let backend = MemoryBackend::new();
        assert!(RuleMatcher::matches(&backend, &rule, &complex).unwrap());
        assert!(!RuleMatcher::matches(&backend, &rule, &simple).unwrap());
    }
}
