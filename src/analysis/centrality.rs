//! Centrality metrics
//!
//! Task 2.1.3: PageRank, betweenness, and degree centrality.

use crate::analysis::graph_utils::PetGraphView;
use crate::error::Result;
use crate::graph::backend::GraphBackend;
use crate::graph::backend::MemoryBackend;
use petgraph::algo::page_rank;
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
    pub fn analyze(&self, backend: &MemoryBackend) -> Result<CentralityReport> {
        let view = PetGraphView::from_backend(backend)?;
        let mut scores: HashMap<Uuid, CentralityScores> = HashMap::new();

        let pagerank_map = page_rank(&view.directed, self.damping, self.iterations);

        for (idx, uuid) in &view.directed_to_uuid {
            let in_degree = view
                .directed
                .neighbors_directed(*idx, petgraph::Direction::Incoming)
                .count();
            let out_degree = view
                .directed
                .neighbors_directed(*idx, petgraph::Direction::Outgoing)
                .count();
            let pr = pagerank_map.get(idx.index()).copied().unwrap_or(0.0);

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
            let bc = self.approximate_betweenness(&view);
            for (uuid, score) in bc {
                if let Some(entry) = scores.get_mut(&uuid) {
                    entry.betweenness = score;
                }
            }
        }

        let mut top_pagerank: Vec<_> = scores.iter().map(|(id, s)| (*id, s.pagerank)).collect();
        top_pagerank.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_pagerank.truncate(10);

        let mut top_betweenness: Vec<_> = scores.iter().map(|(id, s)| (*id, s.betweenness)).collect();
        top_betweenness.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_betweenness.truncate(10);

        Ok(CentralityReport {
            scores,
            top_pagerank,
            top_betweenness,
        })
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
                view.directed_to_uuid
                    .get(&idx)
                    .map(|uuid| (*uuid, score * scale))
            })
            .collect()
    }
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
    let nodes = backend.all_nodes()?;
    if nodes.is_empty() {
        return Ok(vec![]);
    }

    let edges = backend.all_edges()?;
    let mut degree_map: HashMap<Uuid, usize> = HashMap::new();
    for edge in &edges {
        *degree_map.entry(edge.from).or_insert(0) += 1;
        *degree_map.entry(edge.to).or_insert(0) += 1;
    }

    let betweenness = if nodes.len() <= DASHBOARD_CENTRALITY_LIMIT {
        CentralityAnalyzer::new()
            .analyze(backend)
            .ok()
            .map(|r| r.scores)
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let mut scores: Vec<CentralityScore> = nodes
        .iter()
        .map(|node| {
            let degree = degree_map.get(&node.id).copied().unwrap_or(0);
            let complexity = node
                .get_property("cyclomatic")
                .and_then(|v| v.parse::<i64>().ok());
            let bt = betweenness
                .get(&node.id)
                .map(|s| s.betweenness)
                .unwrap_or(0.0);
            let risk_score = degree as f64 * complexity.unwrap_or(1) as f64;

            CentralityScore {
                node_id: node.id,
                name: node.name.clone(),
                file_path: node.file_path.clone(),
                degree,
                betweenness: bt,
                closeness: closeness_estimate(degree, nodes.len()),
                complexity,
                risk_score,
            }
        })
        .collect();

    scores.sort_by(|a, b| b.degree.cmp(&a.degree).then_with(|| {
        b.risk_score
            .partial_cmp(&a.risk_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    }));
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
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::{Edge, Node, NodeType};

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
        backend.insert_edge(Edge::new(id_main, id_helper, crate::graph::schema::EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(id_helper, id_leaf, crate::graph::schema::EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(id_leaf, id_helper, crate::graph::schema::EdgeType::Calls)).unwrap();

        let report = CentralityAnalyzer::new().analyze(&backend).unwrap();
        assert!(report.scores[&id_main].pagerank > 0.0);
    }
}
