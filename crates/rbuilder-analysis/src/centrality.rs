//! Centrality metrics with type-isolated graph projections.
//!
//! Structural edges ([`EdgeType::Contains`], [`EdgeType::DefinedIn`]) are excluded
//! from behavioral centrality by default so module containment cannot inflate
//! PageRank or betweenness scores.

use crate::graph_utils::PetGraphView;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

/// Convergence tolerance for PageRank power iteration.
pub const PAGERANK_TOLERANCE: f64 = 1e-6;

/// Edge types excluded from behavioral centrality (structural containment).
pub const STRUCTURAL_EDGE_TYPES: &[EdgeType] = &[EdgeType::Contains, EdgeType::DefinedIn];

/// Default behavioral edge types for centrality (all directed semantics except structural).
pub fn default_behavioral_edges() -> &'static [EdgeType] {
    &[
        EdgeType::Calls,
        EdgeType::Uses,
        EdgeType::References,
        EdgeType::Modifies,
        EdgeType::Instantiates,
        EdgeType::Implements,
        EdgeType::Extends,
        EdgeType::DependsOn,
    ]
}

/// Centrality scores for a node.
#[derive(Debug, Clone, Default)]
pub struct CentralityScores {
    /// PageRank score
    pub pagerank: f64,
    /// Betweenness centrality (approximate for large graphs)
    pub betweenness: f64,
    /// In-degree (filtered)
    pub in_degree: usize,
    /// Out-degree (filtered)
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

/// PageRank iteration statistics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRankStats {
    /// Iterations executed before stop
    pub iterations_run: usize,
    /// Whether delta fell below tolerance
    pub converged: bool,
    /// Maximum rank delta on the final iteration
    pub max_delta: f64,
}

/// Dense index over a filtered edge projection (`0..V` contiguous flat ids).
#[derive(Debug, Clone)]
pub struct FlatGraphIndex {
    /// Number of nodes in the parent view
    pub node_count: usize,
    /// Filtered edges as `(source_flat, target_flat)` pairs
    pub flat_edges: Vec<(usize, usize)>,
    /// Flat id → node participates in at least one filtered edge
    pub participates: Vec<bool>,
}

impl FlatGraphIndex {
    /// Build a contiguous flat edge list from a typed petgraph view.
    pub fn from_view(view: &PetGraphView, allowed_types: &[EdgeType]) -> Self {
        let node_count = view.directed.node_count();
        let mut flat_edges = Vec::new();
        let mut participates = vec![false; node_count];

        for edge in view.directed.edge_references() {
            if allowed_types.contains(edge.weight()) {
                let src = edge.source().index();
                let dst = edge.target().index();
                flat_edges.push((src, dst));
                participates[src] = true;
                participates[dst] = true;
            }
        }

        Self {
            node_count,
            flat_edges,
            participates,
        }
    }

    /// Per-node in/out degree on the filtered projection.
    pub fn filtered_degrees(&self) -> (Vec<usize>, Vec<usize>) {
        let mut in_degree = vec![0usize; self.node_count];
        let mut out_degree = vec![0usize; self.node_count];
        for &(src, dst) in &self.flat_edges {
            out_degree[src] += 1;
            in_degree[dst] += 1;
        }
        (in_degree, out_degree)
    }
}

/// Cache-friendly PageRank over a filtered edge projection.
pub struct FastPageRank {
    max_iterations: usize,
    damping: f64,
    tolerance: f64,
}

impl Default for FastPageRank {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            damping: 0.85,
            tolerance: PAGERANK_TOLERANCE,
        }
    }
}

impl FastPageRank {
    /// Create a PageRank engine with explicit iteration and damping settings.
    pub fn new(max_iterations: usize, damping: f64) -> Self {
        Self {
            max_iterations,
            damping,
            tolerance: PAGERANK_TOLERANCE,
        }
    }

    /// Override convergence tolerance (default [`PAGERANK_TOLERANCE`]).
    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Run power iteration on `view` restricted to `allowed_types`.
    pub fn compute(
        &self,
        view: &PetGraphView,
        allowed_types: &[EdgeType],
    ) -> (HashMap<Uuid, f64>, PageRankStats) {
        let index = FlatGraphIndex::from_view(view, allowed_types);
        let (ranks, stats) = self.compute_flat(&index);
        let mut scores = HashMap::new();

        for (idx, uuid) in &view.index_to_uuid {
            let flat = idx.index();
            let score = if index.participates.get(flat).copied().unwrap_or(false) {
                ranks.get(flat).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            scores.insert(*uuid, score);
        }

        (scores, stats)
    }

    /// Run power iteration on a pre-built flat index (hot path).
    pub fn compute_flat(&self, index: &FlatGraphIndex) -> (Vec<f64>, PageRankStats) {
        let node_count = index.node_count;
        if node_count == 0 {
            return (Vec::new(), PageRankStats {
                iterations_run: 0,
                converged: true,
                max_delta: 0.0,
            });
        }

        let mut current_ranks = vec![1.0 / node_count as f64; node_count];
        let mut next_ranks = vec![0.0; node_count];

        let mut out_degrees = vec![0u32; node_count];
        for &(src, _) in &index.flat_edges {
            out_degrees[src] += 1;
        }

        let sink_nodes: Vec<usize> = (0..node_count)
            .filter(|&i| out_degrees[i] == 0)
            .collect();

        let base_score = (1.0 - self.damping) / node_count as f64;
        let mut stats = PageRankStats {
            iterations_run: self.max_iterations,
            converged: false,
            max_delta: f64::MAX,
        };

        for iter in 0..self.max_iterations {
            next_ranks.fill(base_score);

            let mut dangling_mass = 0.0;
            for &sink_idx in &sink_nodes {
                dangling_mass += current_ranks[sink_idx];
            }
            let dangling_allocation = (self.damping * dangling_mass) / node_count as f64;
            for score in next_ranks.iter_mut() {
                *score += dangling_allocation;
            }

            for &(src, dst) in &index.flat_edges {
                let out = out_degrees[src] as f64;
                if out > 0.0 {
                    next_ranks[dst] += self.damping * (current_ranks[src] / out);
                }
            }

            let max_delta = current_ranks
                .iter()
                .zip(next_ranks.iter())
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f64::max);

            stats.max_delta = max_delta;
            stats.iterations_run = iter + 1;

            std::mem::swap(&mut current_ranks, &mut next_ranks);

            if max_delta < self.tolerance {
                stats.converged = true;
                break;
            }
        }

        (current_ranks, stats)
    }
}

/// Brandes betweenness restricted to an edge-type filter.
pub struct BetweennessCentrality;

impl BetweennessCentrality {
    /// Compute normalized betweenness for graphs up to `max_nodes` (skips larger graphs).
    pub fn compute(
        view: &PetGraphView,
        allowed_types: &[EdgeType],
        max_nodes: usize,
    ) -> HashMap<Uuid, f64> {
        let n = view.directed.node_count();
        if n == 0 || n > max_nodes {
            return HashMap::new();
        }
        Self::compute_unbounded(view, allowed_types)
    }

    /// Compute normalized betweenness without a size cap.
    pub fn compute_unbounded(
        view: &PetGraphView,
        allowed_types: &[EdgeType],
    ) -> HashMap<Uuid, f64> {
        use std::collections::VecDeque;

        let n = view.directed.node_count();
        if n == 0 {
            return HashMap::new();
        }

        let mut betweenness: HashMap<NodeIndex, f64> = HashMap::new();

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
                for w in view.outgoing_filtered(v, allowed_types) {
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

/// Degree centrality using filtered adjacency views.
pub struct DegreeCentrality;

impl DegreeCentrality {
    /// Count filtered in/out degree for every node in `view`.
    pub fn compute(
        view: &PetGraphView,
        allowed_types: &[EdgeType],
    ) -> HashMap<Uuid, (usize, usize)> {
        let mut degrees = HashMap::new();
        for (idx, uuid) in &view.index_to_uuid {
            let in_degree = view.incoming_filtered(*idx, allowed_types).count();
            let out_degree = view.outgoing_filtered(*idx, allowed_types).count();
            degrees.insert(*uuid, (in_degree, out_degree));
        }
        degrees
    }

    /// Total filtered degree, skipping structural container nodes.
    pub fn operational_scores(
        backend: &MemoryBackend,
        view: &PetGraphView,
        allowed_types: &[EdgeType],
    ) -> Result<Vec<(Uuid, usize)>> {
        let filtered = Self::compute(view, allowed_types);
        let mut scores = Vec::new();
        backend.for_each_node(|node| {
            if is_structural_container(node.node_type) {
                return;
            }
            let (in_d, out_d) = filtered.get(&node.id).copied().unwrap_or((0, 0));
            scores.push((node.id, in_d + out_d));
        })?;
        Ok(scores)
    }
}

fn is_structural_container(node_type: NodeType) -> bool {
    matches!(
        node_type,
        NodeType::Module | NodeType::File | NodeType::Import | NodeType::ConfigKey
    )
}

/// Centrality analysis engine with configurable edge-type isolation.
pub struct CentralityAnalyzer {
    damping: f64,
    iterations: usize,
    allowed_types: Vec<EdgeType>,
    betweenness_limit: usize,
}

impl Default for CentralityAnalyzer {
    fn default() -> Self {
        Self {
            damping: 0.85,
            iterations: 20,
            allowed_types: default_behavioral_edges().to_vec(),
            betweenness_limit: 500,
        }
    }
}

impl CentralityAnalyzer {
    /// Create a new centrality analyzer using default behavioral edge types.
    pub fn new() -> Self {
        Self::default()
    }

    /// Restrict centrality to specific edge types (e.g. `[EdgeType::Calls]` only).
    pub fn with_allowed_types(mut self, allowed_types: &[EdgeType]) -> Self {
        self.allowed_types = allowed_types.to_vec();
        self
    }

    /// Set PageRank iteration count and damping factor.
    pub fn with_pagerank_config(mut self, iterations: usize, damping: f64) -> Self {
        self.iterations = iterations;
        self.damping = damping;
        self
    }

    /// Maximum graph size for exact betweenness (Brandes).
    pub fn with_betweenness_limit(mut self, max_nodes: usize) -> Self {
        self.betweenness_limit = max_nodes;
        self
    }

    /// Active edge-type filter for this analyzer.
    pub fn allowed_types(&self) -> &[EdgeType] {
        &self.allowed_types
    }

    /// Calculate centrality metrics for all nodes using the configured edge filter.
    pub fn analyze_with_view(&self, view: &PetGraphView) -> Result<CentralityReport> {
        let allowed = &self.allowed_types;
        let pagerank_engine = FastPageRank::new(self.iterations, self.damping);
        let (pagerank_map, _stats) = pagerank_engine.compute(view, allowed);
        let degree_map = DegreeCentrality::compute(view, allowed);

        let betweenness_map =
            BetweennessCentrality::compute(view, allowed, self.betweenness_limit);

        let mut scores: HashMap<Uuid, CentralityScores> = HashMap::new();
        for (uuid, (in_degree, out_degree)) in degree_map {
            scores.insert(
                uuid,
                CentralityScores {
                    pagerank: pagerank_map.get(&uuid).copied().unwrap_or(0.0),
                    betweenness: betweenness_map.get(&uuid).copied().unwrap_or(0.0),
                    in_degree,
                    out_degree,
                },
            );
        }

        for (uuid, pr) in pagerank_map {
            scores
                .entry(uuid)
                .and_modify(|s| s.pagerank = pr)
                .or_insert_with(|| CentralityScores {
                    pagerank: pr,
                    ..Default::default()
                });
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
    pub fn analyze(&self, backend: &MemoryBackend) -> Result<CentralityReport> {
        let view = PetGraphView::from_backend(backend)?;
        self.analyze_with_view(&view)
    }

    /// Export scores map for policy evaluation ([`PolicyViolation::CascadeHazard`]).
    pub fn scores_map(&self, view: &PetGraphView) -> Result<HashMap<Uuid, CentralityScores>> {
        Ok(self.analyze_with_view(view)?.scores)
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
    /// Total degree (in + out) on behavioral edges
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

/// Degree centrality for dashboard (behavioral edges only; skips module/file containers).
pub fn degree_centrality(backend: &MemoryBackend) -> Result<Vec<CentralityScore>> {
    let node_count = backend.node_count();
    if node_count == 0 {
        return Ok(vec![]);
    }

    let view = PetGraphView::from_backend(backend)?;
    let allowed = default_behavioral_edges();
    let degree_map = DegreeCentrality::compute(&view, allowed);

    let betweenness = if node_count <= DASHBOARD_CENTRALITY_LIMIT {
        CentralityAnalyzer::new()
            .analyze_with_view(&view)
            .ok()
            .map(|r| r.scores)
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let mut scores: Vec<CentralityScore> = Vec::new();
    backend.for_each_node(|node| {
        if is_structural_container(node.node_type) {
            return;
        }
        let (in_d, out_d) = degree_map.get(&node.id).copied().unwrap_or((0, 0));
        let degree = in_d + out_d;
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
    use rbuilder_graph::schema::{Edge, Node};

    #[test]
    fn test_pagerank_behavioral_filter() {
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
                EdgeType::Calls,
            ))
            .unwrap();
        backend
            .insert_edge(Edge::new(
                id_helper,
                id_leaf,
                EdgeType::Calls,
            ))
            .unwrap();
        backend
            .insert_edge(Edge::new(
                id_leaf,
                id_helper,
                EdgeType::Calls,
            ))
            .unwrap();

        let report = CentralityAnalyzer::new().analyze(&backend).unwrap();
        assert!(report.scores[&id_main].pagerank > 0.0);
    }

    #[test]
    fn test_module_isolated_from_contains_edges() {
        let mut backend = MemoryBackend::new();
        let module = Node::new(NodeType::Module, "mod".into());
        let func = Node::new(NodeType::Function, "f".into());
        let id_mod = module.id;
        let id_func = func.id;
        backend.insert_node(module).unwrap();
        backend.insert_node(func).unwrap();
        backend
            .insert_edge(Edge::new(id_mod, id_func, EdgeType::Contains))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let (scores, _) =
            FastPageRank::new(20, 0.85).compute(&view, &[EdgeType::Calls]);
        assert_eq!(scores.get(&id_mod).copied().unwrap_or(0.0), 0.0);
    }

    #[test]
    fn test_pagerank_convergence_on_cycle() {
        let mut backend = MemoryBackend::new();
        let nodes: Vec<_> = (0..4)
            .map(|i| {
                let n = Node::new(NodeType::Function, format!("n{i}"));
                backend.insert_node(n.clone()).unwrap();
                n
            })
            .collect();
        for w in nodes.windows(2) {
            backend
                .insert_edge(Edge::new(w[0].id, w[1].id, EdgeType::Calls))
                .unwrap();
        }
        backend
            .insert_edge(Edge::new(
                nodes[3].id,
                nodes[0].id,
                EdgeType::Calls,
            ))
            .unwrap();
        backend
            .insert_edge(Edge::new(
                nodes[2].id,
                nodes[3].id,
                EdgeType::Calls,
            ))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
        let (_, stats) = FastPageRank::new(100, 0.85).compute_flat(&index);
        assert!(stats.converged);
        assert!(stats.max_delta < PAGERANK_TOLERANCE);
    }
}
