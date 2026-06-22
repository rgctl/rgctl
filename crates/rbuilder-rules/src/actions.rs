//! Rule action application
//!
//! Task 3.1.3: Apply labeling actions to matched nodes

use crate::matcher::RuleMatcher;
use crate::schema::{RuleAction, Ruleset};
use rbuilder_error::Result;
use rbuilder_graph::backend::trait_def::GraphBackend;
use rbuilder_graph::backend::MemoryBackend;
use std::collections::HashMap;

/// Report from applying a ruleset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleApplicationReport {
    /// Per-rule match counts
    pub rule_matches: HashMap<String, usize>,
    /// Total nodes modified
    pub nodes_modified: usize,
    /// Total labels added
    pub labels_added: usize,
    /// Whether this was a dry run
    pub dry_run: bool,
}

/// Applies rules to graph nodes.
pub struct RuleEngine;

impl RuleEngine {
    /// Apply all rules in a ruleset to the graph backend.
    pub fn apply_ruleset(
        backend: &mut MemoryBackend,
        ruleset: &Ruleset,
        dry_run: bool,
    ) -> Result<RuleApplicationReport> {
        let mut report = RuleApplicationReport {
            dry_run,
            ..Default::default()
        };

        let nodes = backend.all_nodes()?;
        for rule in &ruleset.rules {
            let mut rule_count = 0usize;
            for node in &nodes {
                if RuleMatcher::matches(backend, rule, node)? {
                    rule_count += 1;
                    if !dry_run {
                        Self::apply_actions(backend, node.id, &rule.actions)?;
                        report.nodes_modified += 1;
                    }
                }
            }
            report.rule_matches.insert(rule.name.clone(), rule_count);
        }

        if !dry_run {
            report.labels_added = backend.all_nodes()?.iter().map(|n| n.labels.len()).sum();
        }

        Ok(report)
    }

    fn apply_actions(
        backend: &mut MemoryBackend,
        node_id: uuid::Uuid,
        actions: &[RuleAction],
    ) -> Result<()> {
        let Some(mut node) = backend.get_node(node_id)? else {
            return Ok(());
        };

        for action in actions {
            match action {
                RuleAction::AddLabel { label } => {
                    if !node.has_label(label) {
                        node.labels.push(label.clone());
                    }
                }
                RuleAction::SetMetadata { key, value } => {
                    node.properties.insert(key.clone(), value.clone());
                }
                RuleAction::SetComplexityOverride { level } => {
                    node.properties
                        .insert("complexity_override".to_string(), level.clone());
                    node.labels.push(format!("complexity:{level}"));
                }
            }
        }

        backend.delete_node(node_id)?;
        backend.insert_node(node)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{MatchCondition, MatchLeaf, Rule};
    use rbuilder_graph::schema::{Node, NodeType};

    #[test]
    fn test_rule_actions() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Function, "authenticate".to_string());
        let id = node.id;
        backend.insert_node(node).unwrap();

        let ruleset = Ruleset {
            name: "security".to_string(),
            version: "1.0".to_string(),
            description: None,
            rules: vec![Rule {
                name: "auth".to_string(),
                description: None,
                match_condition: MatchCondition::Leaf(MatchLeaf::NamePattern {
                    pattern: "auth.*".to_string(),
                }),
                actions: vec![
                    RuleAction::AddLabel {
                        label: "security:critical".to_string(),
                    },
                    RuleAction::SetMetadata {
                        key: "priority".to_string(),
                        value: "high".to_string(),
                    },
                ],
            }],
        };

        let report = RuleEngine::apply_ruleset(&mut backend, &ruleset, false).unwrap();
        assert_eq!(report.rule_matches["auth"], 1);

        let updated = backend.get_node(id).unwrap().unwrap();
        assert!(updated.has_label("security:critical"));
        assert_eq!(updated.get_property("priority"), Some(&"high".to_string()));
    }
}
