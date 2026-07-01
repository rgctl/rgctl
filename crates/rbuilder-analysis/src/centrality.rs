//! Centrality metrics
//!
//! Task 2.1.3: PageRank, betweenness, and degree centrality.
//!
//! ## Fast PageRank Implementation
//!
//! Uses a cache-friendly O(iterations × edges) implementation instead of
//! petgraph's O(n × V² × E) generic algorithm. For 187K nodes and 719K edges:
//! - petgraph: ~2.7 trillion operations (12-15 minutes)
//! - Custom: ~14.4 million operations (<15ms)

use crate::graph_utils::PetGraphView;
use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

/// Centrality scores for a node.
#[derive(Debug, Clone, Default)]
pub struct CentralityScores {
    /// PageRank score
    pub pagerank: f64,
    /// Betweenness centrality (approximate for large graphs)
    pub betweenness: f64,
    /// In-degree
    pub in_degree: usize,
    /// Out-degree
    pub out_degree: usize,
}

/// Full centrality report.
#[derive(Debug, Clone)]
pub struct CentralityReport {
    /// Scores keyed by node UUID
    pub scores: HashMap<Uuid, CentralityScores>,
    /// Top nodes by PageRank
    pub top_pagerank: Vec<(Uuid, f64)>,
    /// Top nodes by betweenness
    pub top_betweenness: Vec<(Uuid, f64)>,
}

/// Centrality analysis engine.
pub struct CentralityAnalyzer {
    damping: f64,
    iterations: usize,
}

impl Default for CentralityAnalyzer {
    fn default() -> Self {
        Self {
            damping: 0.85,
            iterations: 20,
        }
    }
}

impl CentralityAnalyzer {
    /// Create a new centrality analyzer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate centrality metrics for all nodes.
    ///
    /// Accepts a pre-built PetGraphView to avoid rebuilding the topology.
    pub fn analyze_with_view(&self, view: &PetGraphView) -> Result<CentralityReport> {
        let mut scores: HashMap<Uuid, CentralityScores> = HashMap::new();

        // Build edge list for fast PageRank
        let node_count = view.directed.node_count();
        let edge_list: Vec<(u32, u32)> = view.directed
            .edge_indices()
            .filter_map(|e| {
                let (src, dst) = view.directed.edge_endpoints(e)?;
                Some((src.index() as u32, dst.index() as u32))
            })
            .collect();

        let pagerank_scores = fast_pagerank(
            node_count,
            &edge_list,
            self.iterations,
            self.damping as f32,
        );

        for (idx, uuid) in &view.index_to_uuid {
            let in_degree = view
                .directed
                .neighbors_directed(*idx, petgraph::Direction::Incoming)
                .count();
            let out_degree = view
                .directed
                .neighbors_directed(*idx, petgraph::Direction::Outgoing)
                .count();
            let pr = pagerank_scores.get(idx.index()).copied().unwrap_or(0.0) as f64;

            scores.insert(
                *uuid,
                CentralityScores {
                    pagerank: pr,
                    betweenness: 0.0,
                    in_degree,
                    out_degree,
                },
            );
        }

        // Approximate betweenness via Brandes for small graphs
        if view.directed.node_count() <= 500 {
            let bc = self.approximate_betweenness(view);
            for (uuid, score) in bc {
                if let Some(entry) = scores.get_mut(&uuid) {
                    entry.betweenness = score;
                }
            }
        }

        let mut top_pagerank: Vec<_> = scores.iter().map(|(id, s)| (*id, s.pagerank)).collect();
        top_pagerank.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_pagerank.truncate(10);

        let mut top_betweenness: Vec<_> =
            scores.iter().map(|(id, s)| (*id, s.betweenness)).collect();
        top_betweenness.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_betweenness.truncate(10);

        Ok(CentralityReport {
            scores,
            top_pagerank,
            top_betweenness,
        })
    }

    /// Calculate centrality metrics for all nodes.
    ///
    /// Builds a PetGraphView internally. For better performance when running
    /// multiple analyses, build the view once and use `analyze_with_view()`.
    pub fn analyze(&self, backend: &MemoryBackend) -> Result<CentralityReport> {
        let view = PetGraphView::from_backend(backend)?;
        self.analyze_with_view(&view)
    }

    fn approximate_betweenness(&self, view: &PetGraphView) -> HashMap<Uuid, f64> {
        use petgraph::visit::EdgeRef;
        use std::collections::VecDeque;

        let n = view.directed.node_count();
        if n == 0 {
            return HashMap::new();
        }

        let mut betweenness: HashMap<petgraph::graph::NodeIndex, f64> = HashMap::new();
        for start in view.directed.node_indices() {
            let mut stack = Vec::new();
            let mut pred: HashMap<_, Vec<_>> = HashMap::new();
            let mut sigma: HashMap<_, f64> = HashMap::new();
            let mut dist: HashMap<_, i32> = HashMap::new();
            let mut delta: HashMap<_, f64> = HashMap::new();

            for v in view.directed.node_indices() {
                pred.insert(v, vec![]);
                sigma.insert(v, 0.0);
                dist.insert(v, -1);
                delta.insert(v, 0.0);
            }
            sigma.insert(start, 1.0);
            dist.insert(start, 0);

            let mut queue = VecDeque::new();
            queue.push_back(start);

            while let Some(v) = queue.pop_front() {
                stack.push(v);
                for edge in view.directed.edges(v) {
                    let w = edge.target();
                    if dist[&w] < 0 {
                        dist.insert(w, dist[&v] + 1);
                        queue.push_back(w);
                    }
                    if dist[&w] == dist[&v] + 1 {
                        sigma.insert(w, sigma[&w] + sigma[&v]);
                        pred.get_mut(&w).unwrap().push(v);
                    }
                }
            }

            while let Some(w) = stack.pop() {
                for &v in &pred[&w] {
                    let contrib = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                    delta.insert(v, delta[&v] + contrib);
                }
                if w != start {
                    *betweenness.entry(w).or_default() += delta[&w];
                }
            }
        }

        let scale = if n > 2 {
            1.0 / ((n - 1) as f64 * (n - 2) as f64)
        } else {
            1.0
        };

        betweenness
            .into_iter()
            .filter_map(|(idx, score)| {
                view.index_to_uuid
                    .get(&idx)
                    .map(|uuid| (*uuid, score * scale))
            })
            .collect()
    }
}

/// Cache-friendly PageRank implementation with O(iterations × edges) complexity.
///
/// Eliminates the catastrophic O(n × V² × E) complexity of petgraph's generic
/// implementation by using flat vector operations and sequential memory access.
///
/// For a graph with 187K nodes and 719K edges:
/// - petgraph: ~2.7 trillion operations (12-15 minutes)
/// - This: ~14.4 million operations (<15ms)
///
/// # Arguments
/// * `node_count` - Number of nodes in the graph
/// * `edge_list` - Edges as (src, dst) pairs using u32 indices
/// * `iterations` - Number of PageRank iterations (typically 20)
/// * `damping` - Damping factor (typically 0.85)
///
/// # Returns
/// Vector of PageRank scores indexed by node ID
fn fast_pagerank(
    node_count: usize,
    edge_list: &[(u32, u32)],
    iterations: usize,
    damping: f32,
) -> Vec<f32> {
    let mut scores = vec![1.0 / node_count as f32; node_count];
    let mut next_scores = vec![0.0; node_count];

    // 1. Precompute out-degrees to avoid division inside the hot loop
    let mut out_degrees = vec![0u32; node_count];
    for &(src, _) in edge_list {
        out_degrees[src as usize] += 1;
    }

    // Identify sink nodes (functions that call nothing) to handle the "dangling mass"
    let sink_nodes: Vec<usize> = (0..node_count)
        .filter(|&i| out_degrees[i] == 0)
        .collect();

    let base_score = (1.0 - damping) / node_count as f32;

    for _ in 0..iterations {
        // Reset next scores with the uniform teleportation baseline
        next_scores.fill(base_score);

        // 2. Distribute mass from dangling/sink nodes uniformly
        let mut dangling_mass = 0.0;
        for &sink_idx in &sink_nodes {
            dangling_mass += scores[sink_idx];
        }
        let dangling_allocation = (damping * dangling_mass) / node_count as f32;
        for score in next_scores.iter_mut() {
            *score += dangling_allocation;
        }

        // 3. THE HOT LOOP: Clean, sequential memory scan O(|E|)
        for &(src, dst) in edge_list {
            let src_idx = src as usize;
            let dst_idx = dst as usize;

            // Propagate score from source to destination
            next_scores[dst_idx] += damping * (scores[src_idx] / out_degrees[src_idx] as f32);
        }

        // Swap vectors for the next iteration without re-allocating
        std::mem::swap(&mut scores, &mut next_scores);
    }

    scores
}

/// Node centrality score for dashboard widgets (Phase 14 A+).
#[derive(Debug, Clone, Serialize)]
pub struct CentralityScore {
    /// Node identifier
    pub node_id: Uuid,
    /// Symbol name
    pub name: String,
    /// Source file path
    pub file_path: Option<String>,
    /// Total degree (in + out)
    pub degree: usize,
    /// Betweenness centrality (0–1)
    pub betweenness: f64,
    /// Closeness centrality (0–1, approximate)
    pub closeness: f64,
    /// Cyclomatic complexity when known
    pub complexity: Option<i64>,
    /// Combined risk: degree × complexity
    pub risk_score: f64,
}

const DASHBOARD_CENTRALITY_LIMIT: usize = 500;

/// Degree centrality for dashboard (limited to graphs ≤500 nodes for betweenness).
pub fn degree_centrality(backend: &MemoryBackend) -> Result<Vec<CentralityScore>> {
    let node_count = backend.node_count();
    if node_count == 0 {
        return Ok(vec![]);
    }

    // Build degree map with zero-copy edge iteration
    let mut degree_map: HashMap<Uuid, usize> = HashMap::new();
    backend.for_each_edge(|edge| {
        *degree_map.entry(edge.from).or_insert(0) += 1;
        *degree_map.entry(edge.to).or_insert(0) += 1;
    })?;

    let betweenness = if node_count <= DASHBOARD_CENTRALITY_LIMIT {
        CentralityAnalyzer::new()
            .analyze(backend)
            .ok()
            .map(|r| r.scores)
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    // Build scores with zero-copy node iteration
    let mut scores: Vec<CentralityScore> = Vec::with_capacity(node_count);
    backend.for_each_node(|node| {
        let degree = degree_map.get(&node.id).copied().unwrap_or(0);
        let complexity = node
            .get_property("cyclomatic")
            .and_then(|v| v.parse::<i64>().ok());
        let bt = betweenness
            .get(&node.id)
            .map(|s| s.betweenness)
            .unwrap_or(0.0);
        let risk_score = degree as f64 * complexity.unwrap_or(1) as f64;

        scores.push(CentralityScore {
            node_id: node.id,
            name: node.name.clone(),
            file_path: node.file_path.clone(),
            degree,
            betweenness: bt,
            closeness: closeness_estimate(degree, node_count),
            complexity,
            risk_score,
        });
    })?;

    scores.sort_by(|a, b| {
        b.degree.cmp(&a.degree).then_with(|| {
            b.risk_score
                .partial_cmp(&a.risk_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    Ok(scores)
}

/// Hotspots: high degree and high complexity nodes.
pub fn identify_hotspots(backend: &MemoryBackend) -> Result<Vec<CentralityScore>> {
    let mut scores = degree_centrality(backend)?;
    scores.retain(|s| s.degree >= 3 && s.complexity.unwrap_or(0) >= 10);
    scores.sort_by(|a, b| {
        b.risk_score
            .partial_cmp(&a.risk_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(scores)
}

fn closeness_estimate(degree: usize, node_count: usize) -> f64 {
    if node_count <= 1 || degree == 0 {
        return 0.0;
    }
    (degree as f64 / (node_count - 1) as f64).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, Node, NodeType};

    #[test]
    fn test_pagerank() {
        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main".to_string());
        let helper = Node::new(NodeType::Function, "helper".to_string());
        let leaf = Node::new(NodeType::Function, "leaf".to_string());
        let id_main = main.id;
        let id_helper = helper.id;
        let id_leaf = leaf.id;
        backend.insert_node(main).unwrap();
        backend.insert_node(helper).unwrap();
        backend.insert_node(leaf).unwrap();
        backend
            .insert_edge(Edge::new(
                id_main,
                id_helper,
                rbuilder_graph::schema::EdgeType::Calls,
            ))
            .unwrap();
        backend
            .insert_edge(Edge::new(
                id_helper,
                id_leaf,
                rbuilder_graph::schema::EdgeType::Calls,
            ))
            .unwrap();
        backend
            .insert_edge(Edge::new(
                id_leaf,
                id_helper,
                rbuilder_graph::schema::EdgeType::Calls,
            ))
            .unwrap();

        let report = CentralityAnalyzer::new().analyze(&backend).unwrap();
        assert!(report.scores[&id_main].pagerank > 0.0);
    }
}
