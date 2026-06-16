//! Centrality metrics
//!
//! Task 2.1.3: PageRank, betweenness, and degree centrality.

use crate::analysis::graph_utils::PetGraphView;
use crate::error::Result;
use crate::graph::backend::MemoryBackend;
use petgraph::algo::page_rank;
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
