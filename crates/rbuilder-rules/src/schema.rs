//! Rule schema definitions
//!
//! Task 3.1.1: JSON schema for labeling rules

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A collection of labeling rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ruleset {
    /// Ruleset name
    pub name: String,
    /// Ruleset version
    pub version: String,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Rules in this ruleset
    pub rules: Vec<Rule>,
}

/// A single labeling rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    /// Rule name
    pub name: String,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Match condition
    #[serde(rename = "match")]
    pub match_condition: MatchCondition,
    /// Actions to apply when matched
    pub actions: Vec<RuleAction>,
}

/// Match conditions with composite logic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MatchCondition {
    /// Composite AND
    And {
        /// Child conditions
        and: Vec<MatchCondition>,
    },
    /// Composite OR
    Or {
        /// Child conditions
        or: Vec<MatchCondition>,
    },
    /// Composite NOT
    Not {
        /// Negated condition
        not: Box<MatchCondition>,
    },
    /// Leaf condition
    Leaf(MatchLeaf),
}

/// Leaf match condition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MatchLeaf {
    /// Match node type (Function, Class, etc.)
    NodeType {
        /// Node type name
        value: String,
    },
    /// Match node name via regex
    NamePattern {
        /// Regex pattern
        pattern: String,
    },
    /// Match existing label
    HasLabel {
        /// Label value
        value: String,
    },
    /// Cyclomatic complexity greater than threshold
    ComplexityGt {
        /// Threshold
        value: usize,
    },
    /// Cyclomatic complexity less than threshold
    ComplexityLt {
        /// Threshold
        value: usize,
    },
    /// Match property key/value
    HasProperty {
        /// Property key
        key: String,
        /// Optional property value
        value: Option<String>,
    },
    /// Node calls any of the given symbols
    CallsAny {
        /// Symbol names
        symbols: Vec<String>,
    },
    /// Shorthand: node_type field in JSON examples
    NodeTypeField {
        /// Node type
        node_type: String,
        /// Optional name pattern
        #[serde(default)]
        name_pattern: Option<String>,
    },
}

/// Action to apply to a matched node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleAction {
    /// Add a label to the node
    AddLabel {
        /// Label to add
        label: String,
    },
    /// Set a metadata property
    SetMetadata {
        /// Property key
        key: String,
        /// Property value
        value: String,
    },
    /// Override complexity classification label
    SetComplexityOverride {
        /// Complexity level (LOW, MEDIUM, HIGH, CRITICAL)
        level: String,
    },
}

/// Shorthand action forms from task examples.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RuleActionRaw {
    /// `{ "add_label": "security:critical" }`
    AddLabel {
        /// Label
        add_label: String,
    },
    /// `{ "set_metadata": { "key": "value" } }`
    SetMetadata {
        /// Metadata map
        set_metadata: HashMap<String, serde_json::Value>,
    },
}

impl Ruleset {
    /// Load a ruleset from a JSON file.
    pub fn from_file(path: &std::path::Path) -> rbuilder_error::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Parse a ruleset from JSON.
    pub fn from_json(json: &str) -> rbuilder_error::Result<Self> {
        let mut ruleset: Ruleset = serde_json::from_str(json)
            .map_err(|e| rbuilder_error::Error::SerdeError(e.to_string()))?;
        for rule in &mut ruleset.rules {
            normalize_rule_actions(&mut rule.actions);
        }
        Ok(ruleset)
    }
}

fn normalize_rule_actions(actions: &mut Vec<RuleAction>) {
    // Actions are already typed; placeholder for future raw form normalization.
    let _ = actions;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_deserialization() {
        let json = r#"{
            "name": "security",
            "version": "1.0",
            "rules": [{
                "name": "critical_security_function",
                "match": {
                    "type": "name_pattern",
                    "pattern": "(?i)(auth|login|verify|token)"
                },
                "actions": [
                    {"type": "add_label", "label": "security:critical"}
                ]
            }]
        }"#;
        let ruleset = Ruleset::from_json(json).unwrap();
        assert_eq!(ruleset.name, "security");
        assert_eq!(ruleset.rules[0].name, "critical_security_function");
    }

    #[test]
    fn test_composite_rule_deserialization() {
        let json = r#"{
            "name": "quality",
            "version": "1.0",
            "rules": [{
                "name": "complex_test",
                "match": {
                    "and": [
                        {"type": "name_pattern", "pattern": ".*_test$"},
                        {"type": "complexity_gt", "value": 10}
                    ]
                },
                "actions": [{"type": "add_label", "label": "test:complex"}]
            }]
        }"#;
        let ruleset = Ruleset::from_json(json).unwrap();
        assert!(matches!(
            ruleset.rules[0].match_condition,
            MatchCondition::And { .. }
        ));
    }
}
