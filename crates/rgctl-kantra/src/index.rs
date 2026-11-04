//! Hydrate Konveyor catalog nodes into the session graph snapshot.

use crate::catalog::KantraCatalog;
use crate::classify::classify_rules;
use crate::error::{KantraError, Result};
use crate::schema::{KantraRule, RuleSupport};
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::snapshot::MmappedGraphSnapshot;
use rgctl_graph::write_columnar_from_backend;
use std::collections::HashMap;
use std::path::Path;
use uuid::{Uuid, uuid};

const KANTRA_RULESET_NS: Uuid = uuid!("a3f8c1e2-4b5d-4e6f-9a0b-1c2d3e4f5a6b");
const KANTRA_RULE_NS: Uuid = uuid!("b4e9d2f3-5c6e-4f7a-8b1c-2d3e4f5a6b7c");

/// Rewrite `graph.snapshot.bin` with catalog rule nodes (replaces prior Kantra nodes).
pub fn rewrite_snapshot_with_catalog(snapshot_path: &Path, catalog: &KantraCatalog) -> Result<()> {
    let snap = MmappedGraphSnapshot::open(snapshot_path)
        .map_err(|e| KantraError::msg(format!("open snapshot: {e}")))?;
    let mut backend = snap
        .hydrate_backend()
        .map_err(|e| KantraError::msg(format!("hydrate snapshot: {e}")))?;
    strip_kantra_nodes(&mut backend)?;
    hydrate_catalog(&mut backend, catalog)?;
    write_columnar_from_backend(&backend, snapshot_path)
        .map_err(|e| KantraError::msg(format!("write snapshot: {e}")))?;
    Ok(())
}

/// Insert `KantraRuleset` / `KantraRule` nodes and `Contains` edges.
pub fn hydrate_catalog(backend: &mut MemoryBackend, catalog: &KantraCatalog) -> Result<()> {
    let classified = classify_rules(&catalog.rules);
    let support_by_index: HashMap<usize, (&RuleSupport, Option<&str>)> = classified
        .iter()
        .map(|c| (c.rule_index, (&c.support, c.reason.as_deref())))
        .collect();

    let ruleset_id = ruleset_node_id(&catalog.catalog_id);
    let mut ruleset_node = Node::new(NodeType::KantraRuleset, catalog.name.clone());
    ruleset_node.id = ruleset_id;
    ruleset_node.qualified_name = Some(catalog.catalog_id.clone().into());
    if let Some(desc) = &catalog.description {
        ruleset_node
            .properties
            .insert("description".into(), desc.clone());
    }
    ruleset_node
        .properties
        .insert("catalog_id".into(), catalog.catalog_id.clone());
    backend
        .insert_node(ruleset_node)
        .map_err(|e| KantraError::msg(e.to_string()))?;

    for (idx, rule) in catalog.rules.iter().enumerate() {
        let node_id = rule_node_id(&rule.rule_id);
        let mut node = build_rule_node(rule, &support_by_index, idx);
        node.id = node_id;
        backend
            .insert_node(node)
            .map_err(|e| KantraError::msg(e.to_string()))?;
        let edge = Edge::new(ruleset_id, node_id, EdgeType::Contains);
        backend
            .insert_edge(edge)
            .map_err(|e| KantraError::msg(e.to_string()))?;
    }
    Ok(())
}

fn build_rule_node(
    rule: &KantraRule,
    support_by_index: &HashMap<usize, (&RuleSupport, Option<&str>)>,
    idx: usize,
) -> Node {
    let mut node = Node::new(NodeType::KantraRule, rule.rule_id.clone());
    node.qualified_name = Some(rule.rule_id.clone().into());
    node.properties
        .insert("rule_id".into(), rule.rule_id.clone());
    if let Some(cat) = &rule.category {
        node.properties.insert("category".into(), cat.clone());
    }
    if let Some(effort) = rule.effort {
        node.properties.insert("effort".into(), effort.to_string());
    }
    if let Some(msg) = &rule.message {
        node.properties.insert("message".into(), msg.clone());
    }
    if let Some((support, reason)) = support_by_index.get(&idx) {
        node.properties
            .insert("support".into(), support_label(support).to_string());
        if let Some(r) = reason {
            node.properties
                .insert("support_reason".into(), (*r).to_string());
        }
    }
    apply_konveyor_labels(&mut node.properties, &rule.labels);
    node
}

fn apply_konveyor_labels(properties: &mut HashMap<String, String>, labels: &[String]) {
    let mut targets = Vec::new();
    let mut sources = Vec::new();
    for label in labels {
        if let Some((key, value)) = label.split_once('=') {
            match key {
                "konveyor.io/target" => {
                    if !targets.iter().any(|t| t == value) {
                        targets.push(value.to_string());
                    }
                    properties.insert(key.to_string(), value.to_string());
                }
                "konveyor.io/source" => {
                    if !sources.iter().any(|s| s == value) {
                        sources.push(value.to_string());
                    }
                    properties.insert(key.to_string(), value.to_string());
                }
                _ => {
                    properties.insert(key.to_string(), value.to_string());
                }
            }
        }
    }
    if !targets.is_empty() {
        properties.insert("targets".into(), targets.join(","));
    }
    if !sources.is_empty() {
        properties.insert("sources".into(), sources.join(","));
    }
}

fn support_label(support: &RuleSupport) -> &'static str {
    match support {
        RuleSupport::Supported => "supported",
        RuleSupport::Partial => "partial",
        RuleSupport::Unsupported => "unsupported",
    }
}

fn ruleset_node_id(catalog_id: &str) -> Uuid {
    Uuid::new_v5(&KANTRA_RULESET_NS, catalog_id.as_bytes())
}

pub fn rule_node_id(rule_id: &str) -> Uuid {
    Uuid::new_v5(&KANTRA_RULE_NS, rule_id.as_bytes())
}

fn strip_kantra_nodes(backend: &mut MemoryBackend) -> Result<()> {
    let to_remove: Vec<Uuid> = backend
        .all_node_ids()
        .map_err(|e| KantraError::msg(e.to_string()))?
        .into_iter()
        .filter(|id| {
            backend.get_node(*id).ok().flatten().is_some_and(|n| {
                matches!(n.node_type, NodeType::KantraRule | NodeType::KantraRuleset)
            })
        })
        .collect();
    for id in to_remove {
        backend
            .delete_node(id)
            .map_err(|e| KantraError::msg(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::KantraRule;
    use rgctl_graph::backend::MemoryBackend;

    fn sample_rule(id: &str, labels: &[&str]) -> KantraRule {
        KantraRule {
            rule_id: id.into(),
            description: None,
            category: Some("mandatory".into()),
            effort: None,
            message: None,
            labels: labels.iter().map(|s| (*s).to_string()).collect(),
            when: serde_yaml::from_str("builtin.filecontent:\n  pattern: x\n").unwrap(),
        }
    }

    #[test]
    fn hydrates_ruleset_and_rules() {
        let catalog = KantraCatalog {
            catalog_id: "test@1".into(),
            name: "fixture".into(),
            description: None,
            rules: vec![sample_rule("r1", &["konveyor.io/target=quarkus"])],
        };
        let mut backend = MemoryBackend::new();
        hydrate_catalog(&mut backend, &catalog).unwrap();
        let rules = backend.find_nodes_by_type(NodeType::KantraRule).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].get_property("konveyor.io/target"), Some("quarkus"));
        assert_eq!(rules[0].get_property("targets"), Some("quarkus"));
    }

    #[test]
    fn preserves_multi_target_and_source_labels() {
        let catalog = KantraCatalog {
            catalog_id: "test@2".into(),
            name: "fixture".into(),
            description: None,
            rules: vec![sample_rule(
                "r2",
                &[
                    "konveyor.io/target=quarkus",
                    "konveyor.io/target=jakarta-ee9+",
                    "konveyor.io/source=java-ee",
                    "konveyor.io/source=springboot",
                ],
            )],
        };
        let mut backend = MemoryBackend::new();
        hydrate_catalog(&mut backend, &catalog).unwrap();
        let rules = backend.find_nodes_by_type(NodeType::KantraRule).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(
            rules[0].get_property("targets"),
            Some("quarkus,jakarta-ee9+")
        );
        assert_eq!(rules[0].get_property("sources"), Some("java-ee,springboot"));
        // Last label wins for singular konveyor.io/* keys (CSV is authoritative).
        assert_eq!(
            rules[0].get_property("konveyor.io/target"),
            Some("jakarta-ee9+")
        );
        assert_eq!(
            rules[0].get_property("konveyor.io/source"),
            Some("springboot")
        );
    }
}
