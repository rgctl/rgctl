//! PageRank / betweenness / community metrics.

use crate::command::MetricsArgs;
use crate::error::{Result, ServiceError};
use crate::metrics_json::{
    MetricsCommunitiesSection, MetricsPagerankSection, build_metrics_response,
    metrics_response_to_json,
};
use rgctl_analysis::{
    CommunityDetector, DEFAULT_SAMPLE_PIVOTS, DEFAULT_SAMPLE_SEED, FastPageRank, FlatGraphIndex,
    NodeRole, PetGraphView, SampledBetweenness, adaptive_betweenness_pivots, classify_node_role,
    community_edge_types_for_backend, default_behavioral_edges, filter_scores_by_role,
};
use rgctl_graph::CodeGraph;
use rgctl_graph::SnapshotNodeStore;
use rgctl_graph::backend::GraphBackend;
use serde_json::{Value, json};
use uuid::Uuid;

fn enrich_top_entry(graph: &CodeGraph, id: Uuid, score_key: &str, score: f64) -> Value {
    let mut obj = json!({
        "node": id.to_string(),
        score_key: score,
    });
    if let Ok(Some(n)) = graph.backend().get_node(id) {
        obj["name"] = json!(n.name.to_string());
        obj["kind"] = json!(format!("{:?}", n.node_type).to_lowercase());
        if let Some(qn) = n.qualified_name.as_ref() {
            obj["qualified_name"] = json!(qn.to_string());
        }
        if let Some(fp) = n.file_path.as_ref() {
            obj["file_path"] = json!(fp.to_string());
        }
    }
    obj
}

fn enrich_top_entry_store(
    store: &SnapshotNodeStore,
    id: Uuid,
    score_key: &str,
    score: f64,
) -> Value {
    let mut obj = json!({
        "node": id.to_string(),
        score_key: score,
    });
    if let Ok(Some(n)) = store.get_node(id) {
        obj["name"] = json!(n.name.to_string());
        obj["kind"] = json!(format!("{:?}", n.node_type).to_lowercase());
        if let Some(qn) = n.qualified_name.as_ref() {
            obj["qualified_name"] = json!(qn.to_string());
        }
        if let Some(fp) = n.file_path.as_ref() {
            obj["file_path"] = json!(fp.to_string());
        }
    }
    obj
}

/// Compute requested metric sections directly from a mmap [`SnapshotNodeStore`] without hydrating `MemoryBackend`.
pub fn run_metrics_from_store(store: &SnapshotNodeStore, args: &MetricsArgs) -> Result<Value> {
    if !args.pagerank && !args.betweenness && !args.communities {
        return Err(ServiceError::InvalidParams(
            "at least one of pagerank, betweenness, communities is required".into(),
        ));
    }
    let iterations = args.iterations.unwrap_or(20);
    let allowed = default_behavioral_edges();
    let mut pagerank = None;
    let mut betweenness = None;
    let mut communities = None;

    if args.pagerank || args.betweenness {
        let (flat_index, uuids) =
            FlatGraphIndex::from_snapshot_store(store, allowed).map_err(ServiceError::from)?;

        if args.pagerank {
            let engine = FastPageRank::new(iterations, 0.85);
            let (ranks, stats) = engine.compute_flat(&flat_index);
            let col = store.columnar();
            let mut candidates: Vec<(Uuid, f64)> = Vec::new();
            for (idx, &score) in ranks.iter().enumerate() {
                if score <= 0.0 {
                    continue;
                }
                let Some(&id) = uuids.get(idx) else {
                    continue;
                };
                let role = if let Some(col) = col {
                    let qn = col.node_qualified_name_str(idx).ok().flatten();
                    let name = col.node_name_str(idx).unwrap_or("");
                    let fp = col.node_file_path_str(idx).ok().flatten();
                    classify_node_role(qn.unwrap_or(name), fp, false)
                } else {
                    store
                        .get_node(id)
                        .ok()
                        .flatten()
                        .map(|n| {
                            let stub = n.get_property("is_external_stub") == Some("true");
                            let name = n.qualified_name.as_deref().unwrap_or(n.name.as_str());
                            classify_node_role(name, n.file_path.as_deref(), stub)
                        })
                        .unwrap_or(NodeRole::Production)
                };
                let keep = match role {
                    NodeRole::Production => true,
                    NodeRole::Test => args.include_tests,
                    NodeRole::ExternalIntrinsic => false,
                };
                if keep {
                    // Double-check `is_external_stub` only on surviving candidates that might be stubs
                    // when `file_path` is absent (zero cold touches for in-repo file-backed symbols).
                    let is_stub =
                        col.is_some_and(|c| {
                            c.node_file_path_str(idx).ok().flatten().is_none()
                                && store.get_node(id).ok().flatten().is_some_and(|n| {
                                    n.get_property("is_external_stub") == Some("true")
                                })
                        });
                    if !is_stub {
                        candidates.push((id, score));
                    }
                }
            }
            if candidates.is_empty() {
                candidates = uuids
                    .iter()
                    .copied()
                    .zip(ranks.iter().copied())
                    .filter(|(_, s)| *s > 0.0)
                    .collect();
            }
            candidates.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
            candidates.truncate(20);
            let top = candidates
                .iter()
                .map(|(id, score)| enrich_top_entry_store(store, *id, "pagerank", *score))
                .collect();
            pagerank = Some(MetricsPagerankSection {
                top,
                converged: stats.converged,
                iterations: stats.iterations_run,
                max_delta: stats.max_delta,
            });
        }

        if args.betweenness {
            let pivots = adaptive_betweenness_pivots(flat_index.node_count, DEFAULT_SAMPLE_PIVOTS);
            let bc = SampledBetweenness::compute_flat(&flat_index, pivots, DEFAULT_SAMPLE_SEED);
            let mut top: Vec<(Uuid, f64)> = uuids
                .iter()
                .copied()
                .zip(bc)
                .filter(|(_, s)| *s > 0.0)
                .collect();
            top.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
            top.truncate(20);
            betweenness = Some(
                top.iter()
                    .map(|(id, s)| enrich_top_entry_store(store, *id, "score", *s))
                    .collect(),
            );
        }
    }

    if args.communities {
        let view = PetGraphView::from_snapshot_store(store).map_err(ServiceError::from)?;
        let detector = CommunityDetector::new();
        let result = detector
            .detect_with_view_defaults(&view, store)
            .map_err(ServiceError::from)?;
        communities = Some(MetricsCommunitiesSection {
            count: result.communities.len(),
            modularity: result.modularity,
            assignments: result.assignments.len(),
        });
    }

    Ok(metrics_response_to_json(&build_metrics_response(
        pagerank,
        betweenness,
        communities,
    )))
}

/// Compute requested metric sections.
pub fn run_metrics(graph: &CodeGraph, args: &MetricsArgs) -> Result<Value> {
    if !args.pagerank && !args.betweenness && !args.communities {
        return Err(ServiceError::InvalidParams(
            "at least one of pagerank, betweenness, communities is required".into(),
        ));
    }
    let view = PetGraphView::from_backend(graph.backend()).map_err(ServiceError::from)?;
    let iterations = args.iterations.unwrap_or(20);
    let allowed = default_behavioral_edges();
    let mut pagerank = None;
    let mut betweenness = None;
    let mut communities = None;

    if args.pagerank {
        let engine = FastPageRank::new(iterations, 0.85);
        let (scores, stats) = engine.compute(&view, allowed);
        let backend = graph.backend();
        let mut top = filter_scores_by_role(&scores, args.include_tests, |id| {
            backend
                .get_node(id)
                .ok()
                .flatten()
                .map(|n| {
                    let stub = n.get_property("is_external_stub") == Some("true");
                    let name = n.qualified_name.as_deref().unwrap_or(n.name.as_str());
                    classify_node_role(name, n.file_path.as_deref(), stub)
                })
                .unwrap_or(NodeRole::Production)
        });
        if top.is_empty() {
            top = scores.iter().map(|(id, score)| (*id, *score)).collect();
            top.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        }
        top.truncate(20);
        let top = top
            .iter()
            .map(|(id, score)| enrich_top_entry(graph, *id, "pagerank", *score))
            .collect();
        pagerank = Some(MetricsPagerankSection {
            top,
            converged: stats.converged,
            iterations: stats.iterations_run,
            max_delta: stats.max_delta,
        });
    }

    if args.betweenness {
        let bc = SampledBetweenness::compute_adaptive(&view, allowed);
        let mut top: Vec<_> = bc.iter().map(|(id, score)| (id, *score)).collect();
        top.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        top.truncate(20);
        betweenness = Some(
            top.iter()
                .map(|(id, s)| enrich_top_entry(graph, **id, "score", *s))
                .collect(),
        );
    }

    if args.communities {
        let detector = CommunityDetector::new();
        let allowed_comm = community_edge_types_for_backend(graph.backend());
        let result = detector
            .detect_with_view_filtered(&view, &allowed_comm)
            .map_err(ServiceError::from)?;
        communities = Some(MetricsCommunitiesSection {
            count: result.communities.len(),
            modularity: result.modularity,
            assignments: result.assignments.len(),
        });
    }

    Ok(metrics_response_to_json(&build_metrics_response(
        pagerank,
        betweenness,
        communities,
    )))
}
