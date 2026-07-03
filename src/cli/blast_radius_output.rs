//! Structured blast-radius CLI response (JSON and text).

use crate::analysis::{
    check_policies, BlastRadiusResult, CentralityAnalyzer, MacroIndexEntry, PetGraphView,
    PolicyRegistry, PolicyViolation, SliceHandoffSeed,
};
use anyhow::Result;
use rbuilder_graph::backend::{GraphBackend, MemoryBackend};
use rbuilder_graph::schema::Node;
use rbuilder_graph::SnapshotNodeStore;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current blast-radius JSON schema version.
pub const BLAST_RADIUS_SCHEMA_VERSION: u32 = 1;

/// Resolved symbol with graph identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolContext {
    /// Graph node UUID.
    pub id: Uuid,
    /// Graph `qualified_name` when present, otherwise bare `name`.
    ///
    /// **Schema v1:** language-native dot notation (e.g. Java `MRequest.beforeSave`).
    /// Downstream tools should treat this as opaque display text keyed by [`Self::id`].
    /// **Schema v2 (planned):** add `language`, structured signature, and a canonical
    /// delimiter form (`Class::method`) so clients need not parse language-specific FQNs.
    pub fqn: String,
    /// Project-relative source path (empty when unknown).
    pub file_path: String,
}

/// Target identification metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlastRadiusTarget {
    /// Definitive graph node UUID.
    pub id: Uuid,
    /// Bare function or method name.
    pub symbol: String,
    /// Containing class or namespace when known.
    pub class_context: Option<String>,
    /// Project-relative source path.
    pub file_path: String,
}

/// Quantitative impact metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlastRadiusMetrics {
    /// Weighted performance/centrality matrix score (0–100).
    pub score: f64,
    /// Deduplicated immediate caller count.
    pub direct_callers_count: usize,
    /// Total transitive reachability zone size.
    pub impact_zone_size: usize,
}

/// Graph structural layout boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlastRadiusTopology {
    /// Recursive cycle group identifier (SCC index).
    pub scc_component_id: Option<usize>,
    /// Immediate callers with graph identity.
    pub direct_callers: Vec<SymbolContext>,
    /// Transitively impacted components.
    pub impact_zone: Vec<SymbolContext>,
}

/// Interprocedural slice hand-off seed (macro-to-micro bridge).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SliceHandoff {
    /// Callee function name.
    pub callee: String,
    /// Callee formal parameter name.
    pub param: String,
    /// Zero-based parameter index.
    pub index: usize,
}

/// Policy and tracing boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlastRadiusGatekeeping {
    /// `"SKIPPED"`, `"PASS"`, or `"VIOLATED"`.
    pub policy_status: String,
    /// Infractions discovered by the policy registry.
    pub violations: Vec<PolicyViolation>,
    /// Slice hand-offs (`[]` when `--with-slices` is omitted).
    pub handoffs: Vec<SliceHandoff>,
}

/// Top-level blast-radius response payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlastRadiusResponse {
    /// Schema version for downstream tooling.
    pub schema_version: u32,
    pub target: BlastRadiusTarget,
    pub metrics: BlastRadiusMetrics,
    pub topology: BlastRadiusTopology,
    pub gatekeeping: BlastRadiusGatekeeping,
}

/// Read-only node lookup for building symbol contexts.
pub enum NodeLookup<'a> {
    /// Hydrated in-memory graph backend.
    Backend(&'a MemoryBackend),
    /// Memory-mapped snapshot store.
    Snapshot(&'a SnapshotNodeStore),
    /// No graph access (name-only fallback).
    None,
}

impl NodeLookup<'_> {
    fn get(&self, id: Uuid) -> Option<SymbolContext> {
        match self {
            Self::Backend(backend) => (*backend)
                .get_node(id)
                .ok()
                .flatten()
                .as_ref()
                .map(symbol_context_from_node),
            Self::Snapshot(store) => store.get_node(id).map(symbol_context_from_node),
            Self::None => None,
        }
    }

    fn resolve_name(&self, name: &str) -> Option<SymbolContext> {
        if let Self::Backend(backend) = self {
            if let Ok(nodes) = (*backend).find_nodes_by_name(name) {
                if let Some(node) = nodes.into_iter().find(|n| n.name == name) {
                    return Some(symbol_context_from_node(&node));
                }
            }
        }
        if let Self::Snapshot(store) = self {
            if let Some(node) = store
                .find_nodes_by_name(name)
                .into_iter()
                .find(|n| n.name == name)
            {
                return Some(symbol_context_from_node(node));
            }
        }
        None
    }
}

fn symbol_context_from_node(node: &Node) -> SymbolContext {
    SymbolContext {
        id: node.id,
        fqn: node
            .qualified_name
            .clone()
            .unwrap_or_else(|| node.name.clone()),
        file_path: node.file_path.clone().unwrap_or_default(),
    }
}

fn contexts_from_ids(ids: &[Uuid], lookup: &NodeLookup<'_>) -> Vec<SymbolContext> {
    ids.iter()
        .filter_map(|id| lookup.get(*id))
        .collect()
}

fn contexts_from_id_name_pairs(
    ids: &[Uuid],
    names: &[String],
    lookup: &NodeLookup<'_>,
) -> Vec<SymbolContext> {
    if !ids.is_empty() {
        return contexts_from_ids(ids, lookup);
    }
    names
        .iter()
        .filter_map(|name| lookup.resolve_name(name))
        .collect()
}

/// Build a response from an SCC engine result and graph lookup.
pub fn build_from_engine_result(
    target_symbol: &str,
    class_context: Option<String>,
    result: &BlastRadiusResult,
    direct_ids: &[Uuid],
    impact_ids: &[Uuid],
    lookup: NodeLookup<'_>,
    gatekeeping: BlastRadiusGatekeeping,
) -> BlastRadiusResponse {
    let target_node = lookup.get(result.symbol_id);
    BlastRadiusResponse {
        schema_version: BLAST_RADIUS_SCHEMA_VERSION,
        target: BlastRadiusTarget {
            id: result.symbol_id,
            symbol: target_symbol.to_string(),
            class_context: class_context.or_else(|| {
                target_node.as_ref().and_then(|ctx| class_from_fqn(&ctx.fqn))
            }),
            file_path: target_node
                .map(|ctx| ctx.file_path)
                .unwrap_or_default(),
        },
        metrics: BlastRadiusMetrics {
            score: result.score,
            direct_callers_count: direct_ids.len(),
            impact_zone_size: impact_ids.len(),
        },
        topology: BlastRadiusTopology {
            scc_component_id: Some(result.scc_id),
            direct_callers: contexts_from_ids(direct_ids, &lookup),
            impact_zone: contexts_from_ids(impact_ids, &lookup),
        },
        gatekeeping,
    }
}

/// Build a response from a macro-call cache entry (fast path).
pub fn build_from_cache_entry(
    entry: &MacroIndexEntry,
    gatekeeping: BlastRadiusGatekeeping,
    lookup: NodeLookup<'_>,
) -> BlastRadiusResponse {
    let direct_callers =
        contexts_from_id_name_pairs(&entry.direct_caller_ids, &entry.direct_callers, &lookup);
    let impact_zone =
        contexts_from_id_name_pairs(&entry.impact_zone_ids, &entry.impact_zone, &lookup);
    BlastRadiusResponse {
        schema_version: BLAST_RADIUS_SCHEMA_VERSION,
        target: BlastRadiusTarget {
            id: entry.id,
            symbol: entry.symbol_name.clone(),
            class_context: entry.class_name.clone(),
            file_path: entry.file_path.clone(),
        },
        metrics: BlastRadiusMetrics {
            score: entry.score,
            direct_callers_count: direct_callers.len(),
            impact_zone_size: impact_zone.len(),
        },
        topology: BlastRadiusTopology {
            scc_component_id: None,
            direct_callers,
            impact_zone,
        },
        gatekeeping,
    }
}

fn class_from_fqn(fqn: &str) -> Option<String> {
    fqn.rsplit_once('.').map(|(class, _)| {
        class.rsplit('.').next().unwrap_or(class).to_string()
    })
}

/// Serialize the response to a JSON value.
pub fn response_to_json(response: &BlastRadiusResponse) -> serde_json::Value {
    serde_json::to_value(response).expect("BlastRadiusResponse serializes")
}

/// Render human-readable terminal output.
pub fn emit_text(response: &BlastRadiusResponse) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Blast radius for '{}'\n",
        response.target.symbol
    ));
    out.push_str(&format!(
        "  Score: {:.1}/100\n",
        response.metrics.score
    ));
    out.push_str(&format!(
        "  Direct callers: {}\n",
        response.metrics.direct_callers_count
    ));
    out.push_str(&format!(
        "  Impact zone: {}\n",
        response.metrics.impact_zone_size
    ));
    if !response.topology.direct_callers.is_empty() {
        let names: Vec<_> = response
            .topology
            .direct_callers
            .iter()
            .map(|c| c.fqn.as_str())
            .collect();
        out.push_str(&format!("  Callers: {}\n", names.join(", ")));
    }
    if !response.topology.impact_zone.is_empty() {
        let names: Vec<_> = response
            .topology
            .impact_zone
            .iter()
            .map(|c| c.fqn.as_str())
            .collect();
        out.push_str(&format!("  Impact: {}\n", names.join(", ")));
    }
    if response.gatekeeping.policy_status == "VIOLATED" {
        out.push_str(&format!(
            "  Policy: VIOLATED ({} violation(s))\n",
            response.gatekeeping.violations.len()
        ));
    }
    out
}

/// Gatekeeping defaults for queries without policy or slice tracing.
pub fn skipped_gatekeeping() -> BlastRadiusGatekeeping {
    BlastRadiusGatekeeping {
        policy_status: "SKIPPED".to_string(),
        violations: Vec::new(),
        handoffs: Vec::new(),
    }
}

/// Evaluate policy guardrails and assemble gatekeeping metadata.
pub fn evaluate_gatekeeping(
    registry: Option<&PolicyRegistry>,
    backend: &MemoryBackend,
    view: Option<&PetGraphView>,
    symbol_id: Uuid,
    impact_zone_ids: &[Uuid],
    handoffs: Vec<SliceHandoff>,
) -> Result<BlastRadiusGatekeeping> {
    let Some(reg) = registry else {
        return Ok(BlastRadiusGatekeeping {
            policy_status: "SKIPPED".to_string(),
            violations: Vec::new(),
            handoffs,
        });
    };

    let built_view;
    let view = match view {
        Some(v) => v,
        None => {
            built_view = PetGraphView::from_backend(backend)?;
            &built_view
        }
    };
    let centrality = CentralityAnalyzer::new().analyze_with_view(view)?.scores;
    match check_policies(
        symbol_id,
        impact_zone_ids,
        reg,
        backend,
        Some(&centrality),
    ) {
        Ok(()) => Ok(BlastRadiusGatekeeping {
            policy_status: "PASS".to_string(),
            violations: Vec::new(),
            handoffs,
        }),
        Err(violation) => Ok(BlastRadiusGatekeeping {
            policy_status: "VIOLATED".to_string(),
            violations: vec![violation],
            handoffs,
        }),
    }
}

/// Map slice hand-off seeds to JSON-serializable hand-offs.
pub fn handoffs_from_seeds(seeds: &[SliceHandoffSeed]) -> Vec<SliceHandoff> {
    seeds
        .iter()
        .map(|seed| SliceHandoff {
            callee: seed.callee_name.clone(),
            param: seed.param_name.clone(),
            index: seed.param_index,
        })
        .collect()
}

#[cfg(test)]
mod output_tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn cache_topology_skips_unresolved_without_nil_uuid() {
        let entry = MacroIndexEntry {
            id: Uuid::new_v4(),
            symbol_name: "t".into(),
            class_name: None,
            file_path: "f.rs".into(),
            score: 0.0,
            direct_caller_ids: vec![Uuid::new_v4()],
            impact_zone_ids: vec![],
            direct_callers: vec![],
            impact_zone: vec![],
        };
        let response = build_from_cache_entry(&entry, skipped_gatekeeping(), NodeLookup::None);
        assert!(response.topology.direct_callers.is_empty());
    }
}

/// Fixture response for schema sanity tests.
pub fn fixture_response() -> BlastRadiusResponse {
    let id_c = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    let id_a = Uuid::new_v4();
    BlastRadiusResponse {
        schema_version: BLAST_RADIUS_SCHEMA_VERSION,
        target: BlastRadiusTarget {
            id: id_c,
            symbol: "c".to_string(),
            class_context: None,
            file_path: "src/main.rs".to_string(),
        },
        metrics: BlastRadiusMetrics {
            score: 65.0,
            direct_callers_count: 1,
            impact_zone_size: 2,
        },
        topology: BlastRadiusTopology {
            scc_component_id: Some(2),
            direct_callers: vec![SymbolContext {
                id: id_b,
                fqn: "b".to_string(),
                file_path: String::new(),
            }],
            impact_zone: vec![
                SymbolContext {
                    id: id_b,
                    fqn: "b".to_string(),
                    file_path: String::new(),
                },
                SymbolContext {
                    id: id_a,
                    fqn: "a".to_string(),
                    file_path: String::new(),
                },
            ],
        },
        gatekeeping: skipped_gatekeeping(),
    }
}
