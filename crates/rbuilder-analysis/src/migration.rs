//! Community-level migration graph and scheduling planner.
//!
//! Builds a macro graph of Louvain communities connected by call edges, aggregates
//! centrality and blast-radius metrics, and produces a dependency-aware roadmap using
//! Kahn topological scheduling with priority tie-breaking.

use crate::results::AnalysisResults;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;
use uuid::Uuid;

pub const MIGRATION_GRAPH_SCHEMA_VERSION: u32 = 1;
pub const MIGRATION_PLAN_SCHEMA_VERSION: u32 = 2;

/// How the exported `steps` array is sorted (each row always carries both ranks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationOrderMode {
    /// Dependency-aware schedule (callees before callers).
    Scheduled,
    /// Pure priority score descending.
    Priority,
}

impl MigrationOrderMode {
    /// Parse CLI / dashboard order mode (`scheduled` default).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "priority" | "priority_rank" | "rank" => Self::Priority,
            _ => Self::Scheduled,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Priority => "priority",
        }
    }
}

/// Weight coefficients for the multi-objective priority score.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MigrationWeights {
    /// PageRank coefficient (α).
    pub alpha: f64,
    /// Harmonic centrality coefficient (β).
    pub beta: f64,
    /// Blast radius coefficient (γ, subtracted).
    pub gamma: f64,
}

impl Default for MigrationWeights {
    fn default() -> Self {
        Self::hybrid_default()
    }
}

impl MigrationWeights {
    /// Balanced default strategy.
    pub fn hybrid_default() -> Self {
        Self {
            alpha: 0.33,
            beta: 0.33,
            gamma: 0.34,
        }
    }

    /// Named preset weights.
    pub fn from_preset(preset: &str) -> Self {
        match preset {
            "foundational_first" => Self {
                alpha: 0.6,
                beta: 0.3,
                gamma: 0.1,
            },
            "dense_cluster" => Self {
                alpha: 0.2,
                beta: 0.5,
                gamma: 0.3,
            },
            "risk_mitigation" => Self {
                alpha: 0.1,
                beta: 0.2,
                gamma: 0.7,
            },
            _ => Self::hybrid_default(),
        }
    }

    pub fn preset_label(preset: &str) -> &'static str {
        match preset {
            "foundational_first" => "Foundational First",
            "dense_cluster" => "Dense Cluster Extraction",
            "risk_mitigation" => "Risk Mitigation",
            _ => "Hybrid Default",
        }
    }
}

/// Per-community aggregated metrics for the migration macro graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationCommunityNode {
    pub id: usize,
    pub label: String,
    pub member_count: u32,
    pub avg_pagerank: f64,
    pub avg_harmonic: f64,
    pub avg_betweenness: f64,
    pub max_blast: f64,
}

/// Directed inter-community call edge (caller → callee).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationCommunityEdge {
    pub source: usize,
    pub target: usize,
    pub weight: u32,
    pub kind: String,
}

/// Community macro graph exported to the dashboard bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationGraphPayload {
    pub schema_version: u32,
    pub modularity: f64,
    pub communities: Vec<MigrationCommunityNode>,
    pub edges: Vec<MigrationCommunityEdge>,
}

/// One community in a migration roadmap (both schedule and priority rank).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationPlanStep {
    /// Display row number in the current `order_mode` sort (1..n).
    pub step: u32,
    pub community_id: usize,
    pub label: String,
    pub priority_score: f64,
    /// Position in dependency-aware topological schedule (1..n).
    pub schedule_step: u32,
    /// Position when sorted by priority score descending (1..n).
    pub priority_rank: u32,
    pub avg_pagerank: f64,
    pub avg_harmonic: f64,
    pub max_blast: f64,
}

/// Full migration plan (CLI / dashboard).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationPlanPayload {
    pub schema_version: u32,
    pub preset: String,
    pub preset_label: String,
    pub weights: MigrationWeights,
    /// Controls `steps` row order: `scheduled` or `priority`.
    pub order_mode: String,
    pub steps: Vec<MigrationPlanStep>,
}

#[derive(Default)]
struct CommunityAgg {
    member_count: u32,
    pagerank_sum: f64,
    harmonic_sum: f64,
    betweenness_sum: f64,
    max_blast: f64,
}

/// Build the community-level migration graph from indexed analysis results.
pub fn build_migration_graph(
    backend: &MemoryBackend,
    results: &AnalysisResults,
) -> Option<MigrationGraphPayload> {
    let community_table = results.community.as_ref()?;
    let centrality = results.centrality.as_ref();
    let blast = results.blast_radius.as_ref();

    let mut agg: HashMap<usize, CommunityAgg> = HashMap::new();
    let mut uuid_to_community: HashMap<Uuid, usize> = HashMap::new();

    let _ = backend.for_each_node(|n| {
        if !matches!(n.node_type, NodeType::Function) {
            return;
        }
        let Some(compact_id) = results.get_compact_id(n.id) else {
            return;
        };
        let Some(cid) = community_table.get(compact_id) else {
            return;
        };
        uuid_to_community.insert(n.id, cid);
        let entry = agg.entry(cid).or_default();
        entry.member_count += 1;

        if let Some(c) = centrality {
            let idx = compact_id as usize;
            if idx < c.pagerank.len() {
                entry.pagerank_sum += c.pagerank[idx] as f64;
            }
            if idx < c.harmonic.len() {
                entry.harmonic_sum += c.harmonic[idx] as f64;
            }
            if idx < c.betweenness.len() {
                entry.betweenness_sum += c.betweenness[idx] as f64;
            }
        }
        if let Some(b) = blast {
            let idx = compact_id as usize;
            if idx < b.scores.len() {
                entry.max_blast = entry.max_blast.max(b.scores[idx] as f64);
            }
        }
    });

    if agg.is_empty() {
        return None;
    }

    let mut edge_weights: HashMap<(usize, usize), u32> = HashMap::new();
    let _ = backend.for_each_edge(|e| {
        if e.edge_type != EdgeType::Calls {
            return;
        }
        let Some(&from_c) = uuid_to_community.get(&e.from) else {
            return;
        };
        let Some(&to_c) = uuid_to_community.get(&e.to) else {
            return;
        };
        if from_c == to_c {
            return;
        }
        *edge_weights.entry((from_c, to_c)).or_insert(0) += 1;
    });

    let mut communities: Vec<MigrationCommunityNode> = agg
        .into_iter()
        .map(|(id, a)| {
            let count = a.member_count.max(1) as f64;
            MigrationCommunityNode {
                id,
                label: format!("Community {id}"),
                member_count: a.member_count,
                avg_pagerank: a.pagerank_sum / count,
                avg_harmonic: a.harmonic_sum / count,
                avg_betweenness: a.betweenness_sum / count,
                max_blast: a.max_blast,
            }
        })
        .collect();
    communities.sort_by_key(|c| c.id);

    let edges: Vec<MigrationCommunityEdge> = edge_weights
        .into_iter()
        .map(|((source, target), weight)| MigrationCommunityEdge {
            source,
            target,
            weight,
            kind: "calls".into(),
        })
        .collect();

    Some(MigrationGraphPayload {
        schema_version: MIGRATION_GRAPH_SCHEMA_VERSION,
        modularity: community_table.modularity,
        communities,
        edges,
    })
}

/// Compute a migration roadmap from a community graph and strategy weights.
pub fn compute_migration_plan(
    graph: &MigrationGraphPayload,
    preset: &str,
    weights: MigrationWeights,
    order_mode: MigrationOrderMode,
) -> MigrationPlanPayload {
    let norm_pr = normalize_values(graph.communities.iter().map(|c| c.avg_pagerank));
    let norm_hm = normalize_values(graph.communities.iter().map(|c| c.avg_harmonic));
    let norm_bl = normalize_values(graph.communities.iter().map(|c| c.max_blast));

    let mut scores: HashMap<usize, f64> = HashMap::new();
    for (i, node) in graph.communities.iter().enumerate() {
        let pr = norm_pr[i];
        let hm = norm_hm[i];
        let bl = norm_bl[i];
        let score = weights.alpha * pr + weights.beta * hm - weights.gamma * bl;
        scores.insert(node.id, score);
    }

    let schedule_order = topological_schedule(&graph.communities, &graph.edges, &scores);
    let priority_order = priority_rank_order(&graph.communities, &scores);

    let schedule_rank: HashMap<usize, u32> = schedule_order
        .iter()
        .enumerate()
        .map(|(idx, id)| (*id, (idx + 1) as u32))
        .collect();
    let priority_rank: HashMap<usize, u32> = priority_order
        .iter()
        .enumerate()
        .map(|(idx, id)| (*id, (idx + 1) as u32))
        .collect();

    let mut steps: Vec<MigrationPlanStep> = graph
        .communities
        .iter()
        .map(|node| MigrationPlanStep {
            step: 0,
            community_id: node.id,
            label: node.label.clone(),
            priority_score: *scores.get(&node.id).unwrap_or(&0.0),
            schedule_step: *schedule_rank.get(&node.id).unwrap_or(&0),
            priority_rank: *priority_rank.get(&node.id).unwrap_or(&0),
            avg_pagerank: node.avg_pagerank,
            avg_harmonic: node.avg_harmonic,
            max_blast: node.max_blast,
        })
        .collect();

    sort_steps(&mut steps, order_mode);
    for (idx, row) in steps.iter_mut().enumerate() {
        row.step = (idx + 1) as u32;
    }

    MigrationPlanPayload {
        schema_version: MIGRATION_PLAN_SCHEMA_VERSION,
        preset: preset.to_string(),
        preset_label: MigrationWeights::preset_label(preset).to_string(),
        weights,
        order_mode: order_mode.as_str().to_string(),
        steps,
    }
}

fn priority_rank_order(
    communities: &[MigrationCommunityNode],
    scores: &HashMap<usize, f64>,
) -> Vec<usize> {
    let mut ids: Vec<usize> = communities.iter().map(|c| c.id).collect();
    ids.sort_by(|&a, &b| {
        let sa = scores.get(&a).copied().unwrap_or(0.0);
        let sb = scores.get(&b).copied().unwrap_or(0.0);
        sb.partial_cmp(&sa)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.cmp(&b))
    });
    ids
}

fn sort_steps(steps: &mut [MigrationPlanStep], order_mode: MigrationOrderMode) {
    match order_mode {
        MigrationOrderMode::Scheduled => {
            steps.sort_by(|a, b| a.schedule_step.cmp(&b.schedule_step).then(a.community_id.cmp(&b.community_id)));
        }
        MigrationOrderMode::Priority => {
            steps.sort_by(|a, b| a.priority_rank.cmp(&b.priority_rank).then(a.community_id.cmp(&b.community_id)));
        }
    }
}

fn normalize_values(values: impl ExactSizeIterator<Item = f64>) -> Vec<f64> {
    let vals: Vec<f64> = values.collect();
    if vals.is_empty() {
        return vals;
    }
    let min = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let max = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (max - min).abs() < f64::EPSILON {
        return vec![0.5; vals.len()];
    }
    vals.into_iter().map(|v| (v - min) / (max - min)).collect()
}

/// Kahn topological sort with priority queue; breaks cycles by highest score.
///
/// Call edge `caller → callee` implies callee should migrate before caller
/// (scheduling edge `callee → caller`).
fn topological_schedule(
    communities: &[MigrationCommunityNode],
    edges: &[MigrationCommunityEdge],
    scores: &HashMap<usize, f64>,
) -> Vec<usize> {
    let ids: HashSet<usize> = communities.iter().map(|c| c.id).collect();
    let mut in_degree: HashMap<usize, u32> = ids.iter().map(|&id| (id, 0)).collect();
    let mut outgoing: HashMap<usize, Vec<usize>> = HashMap::new();

    for edge in edges {
        if !ids.contains(&edge.source) || !ids.contains(&edge.target) {
            continue;
        }
        // callee (target) must be scheduled before caller (source)
        let sched_from = edge.target;
        let sched_to = edge.source;
        if sched_from == sched_to {
            continue;
        }
        outgoing.entry(sched_from).or_default().push(sched_to);
        *in_degree.entry(sched_to).or_insert(0) += 1;
    }

    let mut ready = BinaryHeap::new();
    for &id in &ids {
        if in_degree.get(&id).copied().unwrap_or(0) == 0 {
            ready.push(ScheduleNode {
                id,
                score: *scores.get(&id).unwrap_or(&0.0),
            });
        }
    }

    let mut order = Vec::with_capacity(ids.len());
    let mut scheduled = HashSet::new();

    while let Some(node) = ready.pop() {
        if scheduled.contains(&node.id) {
            continue;
        }
        scheduled.insert(node.id);
        order.push(node.id);
        if let Some(neighbors) = outgoing.get(&node.id) {
            for &next in neighbors {
                let deg = in_degree.get_mut(&next).expect("neighbor in graph");
                *deg = deg.saturating_sub(1);
                if *deg == 0 && !scheduled.contains(&next) {
                    ready.push(ScheduleNode {
                        id: next,
                        score: *scores.get(&next).unwrap_or(&0.0),
                    });
                }
            }
        }
    }

    // Cycle fallback: highest priority remaining communities.
    let mut remaining: Vec<ScheduleNode> = ids
        .iter()
        .filter(|id| !scheduled.contains(id))
        .map(|&id| ScheduleNode {
            id,
            score: *scores.get(&id).unwrap_or(&0.0),
        })
        .collect();
    remaining.sort_by(|a, b| b.cmp(a));
    for node in remaining {
        if scheduled.insert(node.id) {
            order.push(node.id);
        }
    }

    order
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScheduleNode {
    id: usize,
    score: f64,
}

impl Eq for ScheduleNode {}

impl PartialOrd for ScheduleNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduleNode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .partial_cmp(&other.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.id.cmp(&self.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::AnalysisResults;
    use rbuilder_graph::backend::{GraphBackend, MemoryBackend};
    use rbuilder_graph::schema::{Edge, EdgeType, Node, NodeType};
    use uuid::Uuid;

    fn build_fixture_graph() -> (MemoryBackend, AnalysisResults) {
        let na = Node::new(NodeType::Function, "svc_a".to_string());
        let nb = Node::new(NodeType::Function, "svc_b".to_string());
        let nc = Node::new(NodeType::Function, "svc_c".to_string());
        let a = na.id;
        let b = nb.id;
        let c = nc.id;

        let mut backend = MemoryBackend::new();
        backend.insert_node(na).unwrap();
        backend.insert_node(nb).unwrap();
        backend.insert_node(nc).unwrap();
        backend.insert_edge(Edge::new(a, b, EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(b, c, EdgeType::Calls)).unwrap();

        let mut results = AnalysisResults::new(vec![a, b, c]);
        {
            let table = results.init_community();
            table.modularity = 0.42;
            table.num_communities = 2;
            table.assignments[0] = 0; // a in community 0
            table.assignments[1] = 1; // b in community 1
            table.assignments[2] = 1; // c in community 1
        }
        {
            let table = results.init_centrality();
            table.pagerank = vec![0.5, 0.3, 0.2];
            table.harmonic = vec![0.4, 0.6, 0.1];
            table.betweenness = vec![0.0, 0.0, 0.0];
        }
        {
            let table = results.init_blast_radius();
            table.scores = vec![10.0, 80.0, 20.0];
        }

        (backend, results)
    }

    #[test]
    fn build_migration_graph_aggregates_and_edges() {
        let (backend, results) = build_fixture_graph();
        let graph = build_migration_graph(&backend, &results).expect("graph");
        assert_eq!(graph.communities.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].source, 0);
        assert_eq!(graph.edges[0].target, 1);

        let c0 = graph.communities.iter().find(|c| c.id == 0).unwrap();
        let c1 = graph.communities.iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c0.member_count, 1);
        assert_eq!(c1.member_count, 2);
        assert!((c1.max_blast - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn schedule_respects_callee_before_caller() {
        let (backend, results) = build_fixture_graph();
        let graph = build_migration_graph(&backend, &results).unwrap();
        let plan = compute_migration_plan(
            &graph,
            "hybrid_default",
            MigrationWeights::hybrid_default(),
            MigrationOrderMode::Scheduled,
        );
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].community_id, 1);
        assert_eq!(plan.steps[0].schedule_step, 1);
        assert_eq!(plan.steps[1].community_id, 0);
        assert_eq!(plan.steps[1].schedule_step, 2);
    }

    #[test]
    fn priority_order_differs_from_schedule_when_constrained() {
        let (backend, results) = build_fixture_graph();
        let graph = build_migration_graph(&backend, &results).unwrap();
        let scheduled = compute_migration_plan(
            &graph,
            "hybrid_default",
            MigrationWeights::hybrid_default(),
            MigrationOrderMode::Scheduled,
        );
        let priority = compute_migration_plan(
            &graph,
            "hybrid_default",
            MigrationWeights::hybrid_default(),
            MigrationOrderMode::Priority,
        );
        assert_eq!(scheduled.order_mode, "scheduled");
        assert_eq!(priority.order_mode, "priority");
        assert_eq!(priority.steps[0].priority_rank, 1);
        assert!(priority.steps[0].schedule_step >= 1);
    }

    #[test]
    fn scoring_prefers_high_pagerank_under_foundational_preset() {
        let graph = MigrationGraphPayload {
            schema_version: 1,
            modularity: 0.5,
            communities: vec![
                MigrationCommunityNode {
                    id: 0,
                    label: "low".into(),
                    member_count: 1,
                    avg_pagerank: 0.1,
                    avg_harmonic: 0.1,
                    avg_betweenness: 0.0,
                    max_blast: 10.0,
                },
                MigrationCommunityNode {
                    id: 1,
                    label: "high".into(),
                    member_count: 1,
                    avg_pagerank: 0.9,
                    avg_harmonic: 0.1,
                    avg_betweenness: 0.0,
                    max_blast: 10.0,
                },
            ],
            edges: vec![],
        };
        let plan = compute_migration_plan(
            &graph,
            "foundational_first",
            MigrationWeights::from_preset("foundational_first"),
            MigrationOrderMode::Priority,
        );
        assert_eq!(plan.steps[0].community_id, 1);
        assert_eq!(plan.steps[0].priority_rank, 1);
    }

    #[test]
    fn tie_break_by_lowest_community_id() {
        let graph = MigrationGraphPayload {
            schema_version: 1,
            modularity: 0.5,
            communities: vec![
                MigrationCommunityNode {
                    id: 5,
                    label: "b".into(),
                    member_count: 1,
                    avg_pagerank: 0.5,
                    avg_harmonic: 0.5,
                    avg_betweenness: 0.0,
                    max_blast: 0.0,
                },
                MigrationCommunityNode {
                    id: 2,
                    label: "a".into(),
                    member_count: 1,
                    avg_pagerank: 0.5,
                    avg_harmonic: 0.5,
                    avg_betweenness: 0.0,
                    max_blast: 0.0,
                },
            ],
            edges: vec![],
        };
        let plan = compute_migration_plan(
            &graph,
            "hybrid_default",
            MigrationWeights::hybrid_default(),
            MigrationOrderMode::Priority,
        );
        assert_eq!(plan.steps[0].community_id, 2);
        assert_eq!(plan.steps[1].community_id, 5);
    }
}
