//! Blast-radius policy registry and verification.

use crate::centrality::CentralityScores;
use rbuilder_error::{Error, Result as RbResult};
use rbuilder_graph::backend::{GraphBackend, MemoryBackend};
use std::collections::HashMap;
use uuid::Uuid;

/// Logical domain identifier for isolation policies.
pub type DomainId = String;

/// Configurable guardrails for blast-radius analysis.
#[derive(Debug, Clone, Default)]
pub struct PolicyRegistry {
    /// Domain pairs that must not be connected by a blast-radius path.
    pub forbidden_crossings: Vec<(DomainId, DomainId)>,
    /// Maximum allowed impact zone size (strict upper bound).
    pub max_impact_nodes: usize,
    /// Betweenness score above which a reached node triggers cascade hazard.
    pub centrality_alert_threshold: f64,
    /// Node UUID → domain assignment.
    pub node_domains: HashMap<Uuid, DomainId>,
}

impl PolicyRegistry {
    /// Create a registry with default thresholds disabled (no limits).
    pub fn permissive() -> Self {
        Self {
            forbidden_crossings: Vec::new(),
            max_impact_nodes: usize::MAX,
            centrality_alert_threshold: f64::MAX,
            node_domains: HashMap::new(),
        }
    }

    /// Assign a node to a domain.
    pub fn assign_domain(&mut self, node_id: Uuid, domain: impl Into<DomainId>) {
        self.node_domains.insert(node_id, domain.into());
    }
}

/// Policy violation detected during blast-radius evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyViolation {
    /// Blast radius crosses a forbidden domain boundary.
    DomainIsolation {
        /// Source domain of the mutated symbol.
        source_domain: DomainId,
        /// Domain reached by the blast radius.
        reached_domain: DomainId,
        /// Offending node in the impact zone.
        node: Uuid,
    },
    /// Impact zone exceeds configured scale limit.
    ScaleFailure {
        /// Actual impact count.
        count: usize,
        /// Configured maximum.
        max: usize,
    },
    /// Traversal reached a high-betweenness bridge node.
    CascadeHazard {
        /// Bridge node id.
        node: Uuid,
        /// Observed betweenness score.
        betweenness: f64,
        /// Configured threshold.
        threshold: f64,
    },
}

impl std::fmt::Display for PolicyViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DomainIsolation {
                source_domain,
                reached_domain,
                node,
            } => write!(
                f,
                "domain isolation failure: path from '{source_domain}' to '{reached_domain}' via node {node}"
            ),
            Self::ScaleFailure { count, max } => {
                write!(f, "scale failure: impact zone size {count} exceeds max {max}")
            }
            Self::CascadeHazard {
                node,
                betweenness,
                threshold,
            } => write!(
                f,
                "cascade hazard: node {node} betweenness {betweenness:.4} exceeds threshold {threshold:.4}"
            ),
        }
    }
}

/// Evaluate policy guardrails; returns structured violation on failure.
pub fn check_policies(
    source_id: Uuid,
    impact_zone_ids: &[Uuid],
    registry: &PolicyRegistry,
    backend: &MemoryBackend,
    centrality: Option<&HashMap<Uuid, CentralityScores>>,
) -> std::result::Result<(), PolicyViolation> {
    if impact_zone_ids.len() > registry.max_impact_nodes {
        return Err(PolicyViolation::ScaleFailure {
            count: impact_zone_ids.len(),
            max: registry.max_impact_nodes,
        });
    }

    let source_domain = registry.node_domains.get(&source_id).cloned();

    for &node_id in impact_zone_ids {
        if let Some(reached_domain) = registry.node_domains.get(&node_id) {
            if let Some(ref source) = source_domain {
                for (from, to) in &registry.forbidden_crossings {
                    let crosses = (source == from && reached_domain == to)
                        || (source == to && reached_domain == from);
                    if crosses {
                        return Err(PolicyViolation::DomainIsolation {
                            source_domain: source.clone(),
                            reached_domain: reached_domain.clone(),
                            node: node_id,
                        });
                    }
                }
            }
        }

        if let Some(scores) = centrality {
            if let Some(entry) = scores.get(&node_id) {
                if entry.betweenness > registry.centrality_alert_threshold {
                    return Err(PolicyViolation::CascadeHazard {
                        node: node_id,
                        betweenness: entry.betweenness,
                        threshold: registry.centrality_alert_threshold,
                    });
                }
            }
        }

        let _ = backend.get_node(node_id).map_err(|_e| {
            PolicyViolation::ScaleFailure {
                count: impact_zone_ids.len(),
                max: registry.max_impact_nodes,
            }
        })?;
    }

    Ok(())
}

/// Evaluate policy guardrails against a computed impact zone.
pub fn evaluate_policies(
    source_id: Uuid,
    impact_zone_ids: &[Uuid],
    registry: &PolicyRegistry,
    backend: &MemoryBackend,
    centrality: Option<&HashMap<Uuid, CentralityScores>>,
) -> RbResult<()> {
    check_policies(
        source_id,
        impact_zone_ids,
        registry,
        backend,
        centrality,
    )
    .map_err(|v| Error::GraphError(v.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::schema::{Node, NodeType};

    #[test]
    fn test_scale_failure_variant() {
        let backend = MemoryBackend::new();
        let source = Uuid::new_v4();
        let impact: Vec<Uuid> = (0..6).map(|_| Uuid::new_v4()).collect();
        let mut registry = PolicyRegistry::permissive();
        registry.max_impact_nodes = 5;
        assert_eq!(
            check_policies(source, &impact, &registry, &backend, None),
            Err(PolicyViolation::ScaleFailure {
                count: 6,
                max: 5,
            })
        );
    }

    #[test]
    fn test_domain_isolation_variant() {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a".into());
        let b = Node::new(NodeType::Function, "b".into());
        let id_a = a.id;
        let id_b = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();

        let mut registry = PolicyRegistry::permissive();
        registry.assign_domain(id_a, "domain_a");
        registry.assign_domain(id_b, "domain_b");
        registry
            .forbidden_crossings
            .push(("domain_a".into(), "domain_b".into()));

        assert_eq!(
            check_policies(id_a, &[id_b], &registry, &backend, None),
            Err(PolicyViolation::DomainIsolation {
                source_domain: "domain_a".into(),
                reached_domain: "domain_b".into(),
                node: id_b,
            })
        );
    }
}
