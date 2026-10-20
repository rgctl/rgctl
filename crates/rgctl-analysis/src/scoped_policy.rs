//! Scoped subgraph materialization for PR policy gates.

use crate::centrality::CentralityScores;
use crate::results::{AnalysisResults, CentralityMetrics};
use rgctl_error::Result;
use rgctl_graph::SnapshotNodeStore;
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Edge, EdgeType};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use uuid::Uuid;

/// Collect all upstream `Calls` predecessors for `seeds` (full reverse closure).
pub fn collect_upstream_call_closure(
    store: &SnapshotNodeStore,
    seeds: &HashSet<Uuid>,
) -> Result<HashSet<Uuid>> {
    let mut all = seeds.clone();
    let mut frontier = seeds.clone();
    while !frontier.is_empty() {
        let mut next = HashSet::new();
        store.for_each_edge(|from, to, edge_type| {
            if edge_type == EdgeType::Calls && frontier.contains(&to) && all.insert(from) {
                next.insert(from);
            }
            Ok(())
        })?;
        frontier = next;
    }
    Ok(all)
}

/// Materialize an induced subgraph over `ids` into a [`MemoryBackend`].
pub fn hydrate_subset(store: &SnapshotNodeStore, ids: &HashSet<Uuid>) -> Result<MemoryBackend> {
    let mut backend = MemoryBackend::new();
    for id in ids {
        if let Some(node) = store.get_node(*id)? {
            backend.insert_node(node)?;
        }
    }
    store.for_each_edge(|from, to, edge_type| {
        if ids.contains(&from) && ids.contains(&to) {
            backend.insert_edge(Edge::new(from, to, edge_type))?;
        }
        Ok(())
    })?;
    Ok(backend)
}

/// Hot-column hydrate for scoped blast / policy (no cold `NodeExtension` decode).
pub fn hydrate_subset_hot(store: &SnapshotNodeStore, ids: &HashSet<Uuid>) -> Result<MemoryBackend> {
    let mut backend = MemoryBackend::new();
    if let Some(col) = store.columnar() {
        for id in ids {
            let Some(idx) = col.find_node_index(*id) else {
                continue;
            };
            backend.insert_node(col.materialize_node_hot_at(idx)?)?;
        }
    } else {
        for id in ids {
            if let Some(node) = store.get_node(*id)? {
                backend.insert_node(node)?;
            }
        }
    }
    store.for_each_edge(|from, to, edge_type| {
        if ids.contains(&from) && ids.contains(&to) {
            backend.insert_edge(Edge::new(from, to, edge_type))?;
        }
        Ok(())
    })?;
    Ok(backend)
}

/// Resolve scoped entity UUIDs on a snapshot from stable keys.
pub fn scope_entity_ids(
    store: &SnapshotNodeStore,
    scope_keys: &[rgctl_graph::stable_key::StableNodeKey],
) -> Result<Vec<Uuid>> {
    use rgctl_graph::schema::NodeType;
    use rgctl_graph::stable_key::{stable_key_from_row, node_row_ref};

    let col = store
        .columnar()
        .ok_or_else(|| rgctl_error::Error::GraphError("scoped policy requires columnar snapshot".into()))?;
    let key_set: HashSet<_> = scope_keys.iter().copied().collect();
    let mut ids = Vec::new();
    for idx in 0..col.node_count() {
        let row = node_row_ref(col, idx)?;
        if row.node_type != NodeType::Function {
            continue;
        }
        let key = stable_key_from_row(col, idx)?;
        if key_set.contains(&key) {
            ids.push(row.id);
        }
    }
    Ok(ids)
}

/// Load cached centrality when `analysis_results.bin` matches snapshot node count.
pub fn load_centrality_cache(
    repo: &Path,
    store: &SnapshotNodeStore,
) -> Result<HashMap<Uuid, CentralityScores>> {
    let path = repo.join(".rgctl/analysis_results.bin");
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let analysis = AnalysisResults::load(&path)?;
    if analysis.node_count() != store.node_count() {
        return Ok(HashMap::new());
    }
    Ok(centrality_map_from_analysis(&analysis))
}

fn centrality_map_from_analysis(analysis: &AnalysisResults) -> HashMap<Uuid, CentralityScores> {
    let Some(table) = analysis.centrality.as_ref() else {
        return HashMap::new();
    };
    let mut scores = HashMap::new();
    for compact_id in 0..analysis.node_count() {
        let Some(uuid) = analysis.get_uuid(compact_id as u32) else {
            continue;
        };
        let Some(metrics) = table.get(compact_id as u32) else {
            continue;
        };
        scores.insert(uuid, metrics_to_scores(metrics));
    }
    scores
}

fn metrics_to_scores(metrics: CentralityMetrics) -> CentralityScores {
    CentralityScores {
        pagerank: metrics.pagerank as f64,
        betweenness: metrics.betweenness as f64,
        harmonic: metrics.harmonic as f64,
        in_degree: metrics.in_degree as usize,
        out_degree: metrics.out_degree as usize,
    }
}

/// Merge cached head centrality with scoped computation for missing seed entities.
pub fn build_pr_check_centrality(
    repo: &Path,
    head_store: &SnapshotNodeStore,
    seed_ids: &[Uuid],
) -> Result<HashMap<Uuid, CentralityScores>> {
    let mut scores = load_centrality_cache(repo, head_store)?;
    let missing: Vec<Uuid> = seed_ids
        .iter()
        .copied()
        .filter(|id| !scores.contains_key(id))
        .collect();
    if !missing.is_empty() {
        let computed = crate::CentralityAnalyzer::new()
            .with_harmonic(false)
            .analyze_scoped(head_store, &missing)?;
        scores.extend(computed);
    }
    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlastRadiusEngine;
    use rgctl_graph::schema::{Edge, Node, NodeType};
    use rgctl_graph::write_columnar_from_nodes_edges;
    use rgctl_graph::SnapshotNodeStore;
    use tempfile::TempDir;

    fn open_fixture(caller_count: usize) -> (TempDir, SnapshotNodeStore, Uuid) {
        let mut backend = MemoryBackend::new();
        let target = Node::new(NodeType::Function, "target_fn").with_file_path("src/pkg.rs");
        let target_id = target.id;
        backend.insert_node(target).unwrap();
        for i in 0..caller_count {
            let caller =
                Node::new(NodeType::Function, format!("caller{i}")).with_file_path("src/pkg.rs");
            let caller_id = caller.id;
            backend.insert_node(caller).unwrap();
            backend
                .insert_edge(Edge::new(caller_id, target_id, EdgeType::Calls))
                .unwrap();
        }
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("snap.bin");
        write_columnar_from_nodes_edges(
            backend.all_nodes().unwrap(),
            backend.all_edges().unwrap(),
            &path,
        )
        .unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        (tmp, store, target_id)
    }

    #[test]
    fn scoped_blast_matches_full_on_fixture() {
        let (_tmp, store, target_id) = open_fixture(6);
        let full = store.hydrate_backend().unwrap();
        let full_engine = BlastRadiusEngine::build(&full).unwrap();
        let scoped_engine = BlastRadiusEngine::build_scoped(&store, &[target_id]).unwrap();

        let full_result = full_engine.analyze(target_id).unwrap();
        let scoped_result = scoped_engine.analyze(target_id).unwrap();
        assert_eq!(
            full_result.impact_zone_ids.len(),
            scoped_result.impact_zone_ids.len()
        );
        assert_eq!(
            full_result.direct_caller_ids.len(),
            scoped_result.direct_caller_ids.len()
        );
    }
}
