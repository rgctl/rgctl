//! Centrality metrics with type-isolated graph projections.
//!
//! Structural edges ([`EdgeType::Contains`], [`EdgeType::DefinedIn`]) are excluded
//! from behavioral centrality by default so module containment cannot inflate
//! PageRank or betweenness scores.

use crate::graph_utils::{PetGraphView, edge_type_set};
use petgraph::graph::NodeIndex;
use rgctl_error::Result;
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::schema::{EdgeType, NodeType};
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

/// Convergence tolerance for PageRank power iteration.
pub const PAGERANK_TOLERANCE: f64 = 1e-6;

/// Architectural role of a graph node for production-aware PageRank hotspots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeRole {
    /// Default production application code.
    Production,
    /// Test fixtures / `#[cfg(test)]` / `*.test.*` paths.
    Test,
    /// Unresolved external / stdlib-like intrinsics (`assert`, `println!`, …).
    ExternalIntrinsic,
}

/// Classify a node for production-aware hotspot ranking.
pub fn classify_node_role(name: &str, file_path: Option<&str>, is_external_stub: bool) -> NodeRole {
    if is_test_path(file_path) || is_cfg_test_name(name) {
        return NodeRole::Test;
    }
    if is_external_stub || is_external_intrinsic_name(name) {
        return NodeRole::ExternalIntrinsic;
    }
    NodeRole::Production
}

fn is_test_path(file_path: Option<&str>) -> bool {
    let Some(path) = file_path else {
        return false;
    };
    let norm = path.replace('\\', "/");
    let lower = norm.to_ascii_lowercase();
    if lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.starts_with("tests/")
        || lower.starts_with("test/")
    {
        return true;
    }
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    file.contains(".test.")
        || file.contains(".spec.")
        || file.ends_with("_test.rs")
        || file.ends_with("_test.py")
        || file.ends_with("_test.go")
        || file.ends_with("_test.java")
        || file.ends_with("_tests.rs")
        || file.starts_with("test_")
}

fn is_cfg_test_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("cfg(test)")
        || lower.starts_with("tests::")
        || lower.contains("::tests::")
        || lower.ends_with("::tests")
}

fn is_external_intrinsic_name(name: &str) -> bool {
    let bare = name
        .rsplit(['.', ':', '#'])
        .next()
        .unwrap_or(name)
        .trim_end_matches('!');
    matches!(
        bare,
        "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "panic"
            | "todo"
            | "unimplemented"
            | "unreachable"
            | "println"
            | "eprintln"
            | "print"
            | "eprint"
            | "dbg"
            | "free"
            | "malloc"
            | "calloc"
            | "realloc"
            | "len"
            | "length"
            | "size"
            | "writefilesync"
            | "readfilesync"
            | "console"
            | "log"
            | "require"
            | "include"
    ) || bare.eq_ignore_ascii_case("writeFileSync")
        || bare.eq_ignore_ascii_case("readFileSync")
}

/// Keep production hotspots by default; pass `include_tests = true` to also keep
/// [`NodeRole::Test`] symbols. [`NodeRole::ExternalIntrinsic`] is always excluded.
pub fn filter_scores_by_role<F>(
    scores: &HashMap<Uuid, f64>,
    include_tests: bool,
    mut role_of: F,
) -> Vec<(Uuid, f64)>
where
    F: FnMut(Uuid) -> NodeRole,
{
    let mut out: Vec<(Uuid, f64)> = scores
        .iter()
        .filter_map(|(&id, &score)| {
            if score <= 0.0 {
                return None;
            }
            match role_of(id) {
                NodeRole::Production => {}
                NodeRole::Test if include_tests => {}
                NodeRole::Test | NodeRole::ExternalIntrinsic => return None,
            }
            Some((id, score))
        })
        .collect();
    out.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
    out
}

/// Node count above which discover uses capped PageRank iterations and relaxed tolerance.
pub const LARGE_GRAPH_PAGERANK_NODE_LIMIT: usize = 500_000;

/// PageRank iteration cap for graphs above [`LARGE_GRAPH_PAGERANK_NODE_LIMIT`].
pub const LARGE_GRAPH_PAGERANK_ITERATIONS: usize = 8;

/// PageRank convergence tolerance for graphs above [`LARGE_GRAPH_PAGERANK_NODE_LIMIT`].
pub const LARGE_GRAPH_PAGERANK_TOLERANCE: f64 = 1e-4;

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
        EdgeType::Overrides,
        EdgeType::ExposesRoute,
        EdgeType::Injects,
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
    /// Normalized out-harmonic centrality (skipped on large graphs)
    pub harmonic: f64,
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
    /// Approximation metadata and timings (large graphs).
    pub approx_stats: crate::centrality_approx::CentralityApproxStats,
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
    /// Build a contiguous flat edge list from a typed CSR topology view.
    pub fn from_view(view: &PetGraphView, allowed_types: &[EdgeType]) -> Self {
        let node_count = view.node_count();
        let mut flat_edges = Vec::new();
        let mut participates = vec![false; node_count];
        let allowed = edge_type_set(allowed_types);

        let _ = view.for_each_edge(|src, dst, ty| {
            if allowed.contains(&ty) {
                let s = src.index();
                let t = dst.index();
                flat_edges.push((s, t));
                participates[s] = true;
                participates[t] = true;
            }
        });

        Self {
            node_count,
            flat_edges,
            participates,
        }
    }

    /// Build a contiguous flat edge list directly from a [`rgctl_graph::SnapshotNodeStore`]
    /// without hydrating [`rgctl_graph::backend::MemoryBackend`] or full CSR adjacency.
    pub fn from_snapshot_store(
        store: &rgctl_graph::SnapshotNodeStore,
        allowed_types: &[EdgeType],
    ) -> rgctl_error::Result<(Self, Vec<Uuid>)> {
        let allowed = edge_type_set(allowed_types);
        if let Some(col) = store.columnar() {
            let node_count = col.node_count();
            let uuids: Vec<Uuid> = col.node_ids_by_index().map(|(_, id)| id).collect();
            let mut flat_edges = Vec::with_capacity(col.edge_count().min(262_144));
            let mut participates = vec![false; node_count];
            col.for_each_edge(|from, to, ty| {
                if allowed.contains(&ty)
                    && let (Some(s), Some(t)) = (col.find_node_index(from), col.find_node_index(to))
                {
                    flat_edges.push((s, t));
                    participates[s] = true;
                    participates[t] = true;
                }
                Ok(())
            })?;
            return Ok((
                Self {
                    node_count,
                    flat_edges,
                    participates,
                },
                uuids,
            ));
        }
        let view = PetGraphView::from_snapshot_store(store)?;
        let uuids: Vec<Uuid> = (0..view.node_count())
            .filter_map(|i| view.get_uuid(petgraph::graph::NodeIndex::new(i)))
            .collect();
        Ok((Self::from_view(&view, allowed_types), uuids))
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

    /// Build outgoing adjacency lists from the flat edge list.
    ///
    /// This is O(E) and allocates one `Vec<usize>` per node. Call it once and
    /// pass the result to betweenness / harmonic functions that need per-node
    /// neighbor iteration. PageRank and degree centrality do not need this.
    pub fn build_out_adj(&self) -> Vec<Vec<usize>> {
        let mut adj = vec![Vec::new(); self.node_count];
        for &(src, dst) in &self.flat_edges {
            adj[src].push(dst);
        }
        adj
    }

    /// Build a compact CSR outgoing adjacency (two contiguous buffers).
    pub fn build_out_csr(&self) -> CsrOutAdj {
        let n = self.node_count;
        let mut degree = vec![0u32; n];
        for &(src, _) in &self.flat_edges {
            degree[src] += 1;
        }
        let mut offsets = vec![0u32; n + 1];
        for i in 0..n {
            offsets[i + 1] = offsets[i] + degree[i];
        }
        let mut targets = vec![0u32; self.flat_edges.len()];
        let mut cursor = offsets.clone();
        for &(src, dst) in &self.flat_edges {
            let pos = cursor[src] as usize;
            targets[pos] = dst as u32;
            cursor[src] += 1;
        }
        CsrOutAdj { offsets, targets }
    }
}

/// Compressed sparse row outgoing adjacency for flat graph projections.
#[derive(Debug, Clone)]
pub struct CsrOutAdj {
    /// Row offsets (`offsets[i]..offsets[i+1]` indexes into [`Self::targets`]).
    pub offsets: Vec<u32>,
    /// Flattened neighbor targets.
    pub targets: Vec<u32>,
}

impl CsrOutAdj {
    /// Outgoing neighbors of `node` as dense indices.
    pub fn neighbors(&self, node: usize) -> &[u32] {
        let start = self.offsets[node] as usize;
        let end = self.offsets[node + 1] as usize;
        &self.targets[start..end]
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

        for (idx, uuid) in view.index_uuid_iter() {
            let flat = idx.index();
            let score = if index.participates.get(flat).copied().unwrap_or(false) {
                ranks.get(flat).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            scores.insert(uuid, score);
        }

        (scores, stats)
    }

    /// Run power iteration on a pre-built flat index (hot path).
    ///
    /// Each iteration precomputes `node_contrib[src] = damping * rank[src] / out_degree`
    /// (Rayon) and reduces dangling mass / max Δ with parallel folds (`f64` throughout).
    pub fn compute_flat(&self, index: &FlatGraphIndex) -> (Vec<f64>, PageRankStats) {
        use rayon::prelude::*;

        let node_count = index.node_count;
        if node_count == 0 {
            return (
                Vec::new(),
                PageRankStats {
                    iterations_run: 0,
                    converged: true,
                    max_delta: 0.0,
                },
            );
        }

        let mut current_ranks = vec![1.0 / node_count as f64; node_count];
        let mut next_ranks = vec![0.0; node_count];
        let mut node_contrib = vec![0.0f64; node_count];

        let mut out_degrees = vec![0u32; node_count];
        for &(src, _) in &index.flat_edges {
            out_degrees[src] += 1;
        }

        let sink_nodes: Vec<usize> = (0..node_count).filter(|&i| out_degrees[i] == 0).collect();

        let base_score = (1.0 - self.damping) / node_count as f64;
        let damping = self.damping;
        let mut stats = PageRankStats {
            iterations_run: self.max_iterations,
            converged: false,
            max_delta: f64::MAX,
        };

        // Avoid Rayon pool barriers on small graphs (e.g. Leiden importance on fixtures).
        const PARALLEL_THRESHOLD: usize = 4_096;
        let parallel = node_count >= PARALLEL_THRESHOLD;

        for iter in 0..self.max_iterations {
            next_ranks.fill(base_score);

            let dangling_mass: f64 = if parallel {
                sink_nodes
                    .par_iter()
                    .map(|&sink_idx| current_ranks[sink_idx])
                    .sum()
            } else {
                sink_nodes
                    .iter()
                    .map(|&sink_idx| current_ranks[sink_idx])
                    .sum()
            };
            let dangling_allocation = (damping * dangling_mass) / node_count as f64;
            for score in next_ranks.iter_mut() {
                *score += dangling_allocation;
            }

            // Hoist per-source contribution outside the edge loop.
            if parallel {
                node_contrib
                    .par_iter_mut()
                    .zip(current_ranks.par_iter())
                    .zip(out_degrees.par_iter())
                    .for_each(|((contrib, &rank), &out)| {
                        *contrib = if out > 0 {
                            damping * rank / f64::from(out)
                        } else {
                            0.0
                        };
                    });
            } else {
                for i in 0..node_count {
                    let out = out_degrees[i];
                    node_contrib[i] = if out > 0 {
                        damping * current_ranks[i] / f64::from(out)
                    } else {
                        0.0
                    };
                }
            }

            for &(src, dst) in &index.flat_edges {
                next_ranks[dst] += node_contrib[src];
            }

            let max_delta = if parallel {
                current_ranks
                    .par_iter()
                    .zip(next_ranks.par_iter())
                    .map(|(a, b)| (a - b).abs())
                    .reduce(|| 0.0, f64::max)
            } else {
                current_ranks
                    .iter()
                    .zip(next_ranks.iter())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0, f64::max)
            };

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

    /// Run power iteration and return only the flat rank vector (no UUID map).
    pub fn compute_flat_only(&self, index: &FlatGraphIndex) -> Vec<f64> {
        self.compute_flat(index).0
    }
}

/// Adaptive PageRank iteration budget and tolerance for a graph size.
pub fn adaptive_pagerank_config(
    node_count: usize,
    default_iterations: usize,
    default_tolerance: f64,
) -> (usize, f64) {
    if node_count > LARGE_GRAPH_PAGERANK_NODE_LIMIT {
        (
            LARGE_GRAPH_PAGERANK_ITERATIONS,
            LARGE_GRAPH_PAGERANK_TOLERANCE,
        )
    } else {
        (default_iterations, default_tolerance)
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
        let n = view.node_count();
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
        let index = FlatGraphIndex::from_view(view, allowed_types);
        let out_adj = index.build_out_csr();
        Self::compute_with_adj(view, &index, &out_adj)
    }

    /// Compute normalized betweenness from a pre-built index and CSR adjacency.
    pub fn compute_with_adj(
        view: &PetGraphView,
        index: &FlatGraphIndex,
        out_adj: &CsrOutAdj,
    ) -> HashMap<Uuid, f64> {
        let n = index.node_count;
        if n == 0 {
            return HashMap::new();
        }

        let mut betweenness = vec![0.0f64; n];
        let mut scratch = crate::centrality_approx::BrandesScratch::new(n);
        for source in 0..n {
            scratch.run(out_adj, source, n);
            for (node, score) in scratch.partial().iter().enumerate() {
                betweenness[node] += score;
            }
        }

        let scale = if n > 2 {
            1.0 / ((n - 1) as f64 * (n - 2) as f64)
        } else {
            1.0
        };

        view.index_uuid_iter()
            .filter_map(|(idx, uuid)| {
                let flat = idx.index();
                if flat < n && betweenness[flat] > 0.0 {
                    Some((uuid, betweenness[flat] * scale))
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Normalized out-harmonic centrality via all-pairs BFS on filtered edges.
pub struct HarmonicCentrality;

impl HarmonicCentrality {
    /// Compute normalized harmonic scores for graphs up to `max_nodes` (skips larger graphs).
    pub fn compute(
        view: &PetGraphView,
        allowed_types: &[EdgeType],
        max_nodes: usize,
    ) -> HashMap<Uuid, f64> {
        let n = view.node_count();
        if n == 0 || n > max_nodes {
            return HashMap::new();
        }
        let index = FlatGraphIndex::from_view(view, allowed_types);
        let out_adj = index.build_out_csr();
        Self::compute_with_adj(view, &index, &out_adj)
    }

    /// Compute normalized harmonic scores from a pre-built index and CSR adjacency.
    pub fn compute_with_adj(
        view: &PetGraphView,
        index: &FlatGraphIndex,
        out_adj: &CsrOutAdj,
    ) -> HashMap<Uuid, f64> {
        use std::collections::VecDeque;

        let n = index.node_count;
        if n == 0 {
            return HashMap::new();
        }

        let norm_factor = if n <= 1 { 0.0 } else { 1.0 / (n as f64 - 1.0) };
        let mut visited = vec![false; n];
        let mut result = HashMap::with_capacity(n);

        for start_i in 0..n {
            visited.fill(false);
            let mut sum_reciprocal = 0.0;
            let mut queue = VecDeque::new();
            visited[start_i] = true;
            queue.push_back((start_i, 0u32));

            while let Some((current, dist)) = queue.pop_front() {
                if dist > 0 {
                    sum_reciprocal += 1.0 / f64::from(dist);
                }
                for &next in out_adj.neighbors(current) {
                    let next = next as usize;
                    if !visited[next] {
                        visited[next] = true;
                        queue.push_back((next, dist + 1));
                    }
                }
            }

            if let Some(uuid) = view.get_uuid(NodeIndex::new(start_i)) {
                result.insert(uuid, sum_reciprocal * norm_factor);
            }
        }

        result
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
        for (idx, uuid) in view.index_uuid_iter() {
            let in_degree = view.incoming_filtered(idx, allowed_types).count();
            let out_degree = view.outgoing_filtered(idx, allowed_types).count();
            degrees.insert(uuid, (in_degree, out_degree));
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

/// Align a UUID-keyed score map onto petgraph node indices.
fn align_map_to_flat(view: &PetGraphView, map: &HashMap<Uuid, f64>) -> Vec<f64> {
    let mut flat = vec![0.0; view.node_count()];
    for (node_idx, uuid) in view.index_uuid_iter() {
        if let Some(&score) = map.get(&uuid) {
            flat[node_idx.index()] = score;
        }
    }
    flat
}

fn flat_pagerank_score(index: &FlatGraphIndex, flat_ranks: &[f64], flat_id: usize) -> f64 {
    if index.participates.get(flat_id).copied().unwrap_or(false) {
        flat_ranks.get(flat_id).copied().unwrap_or(0.0)
    } else {
        0.0
    }
}

/// Summary metadata from a columnar centrality pass (no UUID hash maps).
#[derive(Debug, Clone)]
pub struct CentralityRunSummary {
    /// Top nodes by PageRank (up to 10).
    pub top_pagerank: Vec<(Uuid, f64)>,
    /// Whether any node has non-zero betweenness.
    pub has_betweenness: bool,
    /// Approximation metadata and timings.
    pub approx_stats: crate::centrality_approx::CentralityApproxStats,
}

struct FlatCentralityArrays {
    pagerank: Vec<f64>,
    betweenness: Vec<f64>,
    harmonic: Vec<f64>,
    in_degree: Vec<usize>,
    out_degree: Vec<usize>,
}

fn top_pagerank_from_table(
    results: &crate::results::AnalysisResults,
    limit: usize,
) -> (Vec<(Uuid, f64)>, bool) {
    let Some(table) = results.centrality.as_ref() else {
        return (Vec::new(), false);
    };

    let n = results.node_count();
    let mut has_betweenness = false;
    let mut top: Vec<(Uuid, f64)> = Vec::with_capacity(limit);

    for slot in 0..n {
        if table.betweenness[slot] > 0.0 {
            has_betweenness = true;
        }

        let Some(uuid) = results.get_uuid(slot as u32) else {
            continue;
        };
        let pr = f64::from(table.pagerank[slot]);
        if top.len() < limit {
            top.push((uuid, pr));
            if top.len() == limit {
                top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        } else if pr > top[limit - 1].1 {
            top[limit - 1] = (uuid, pr);
            top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    (top, has_betweenness)
}

/// Centrality analysis engine with configurable edge-type isolation.
pub struct CentralityAnalyzer {
    damping: f64,
    iterations: usize,
    allowed_types: Vec<EdgeType>,
    exact_limit: usize,
    sample_pivots: usize,
    sample_seed: u64,
    hyperball_rounds: usize,
    /// When false, skip exact / HyperBall harmonic (scores stay 0).
    compute_harmonic: bool,
}

impl Default for CentralityAnalyzer {
    fn default() -> Self {
        Self {
            damping: 0.85,
            iterations: 20,
            allowed_types: default_behavioral_edges().to_vec(),
            exact_limit: crate::centrality_approx::DEFAULT_EXACT_CENTRALITY_LIMIT,
            sample_pivots: crate::centrality_approx::DEFAULT_SAMPLE_PIVOTS,
            sample_seed: 0xA5A5_5A5A_C3C3_3C3C,
            hyperball_rounds: crate::centrality_approx::DEFAULT_HYPERBALL_ROUNDS,
            // Library / metrics / tests keep harmonic on; discover opts out via CLI.
            compute_harmonic: true,
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

    /// Maximum graph size for exact Brandes / BFS harmonic.
    pub fn with_exact_limit(mut self, max_nodes: usize) -> Self {
        self.exact_limit = max_nodes;
        self
    }

    /// Pivot count for sampled betweenness on graphs larger than [`Self::exact_limit`].
    pub fn with_sample_pivots(mut self, pivots: usize) -> Self {
        self.sample_pivots = pivots.max(1);
        self
    }

    /// Seed for reproducible pivot sampling.
    pub fn with_sample_seed(mut self, seed: u64) -> Self {
        self.sample_seed = seed;
        self
    }

    /// HyperBall propagation rounds for approximate harmonic centrality.
    pub fn with_hyperball_rounds(mut self, rounds: usize) -> Self {
        self.hyperball_rounds = rounds.max(1);
        self
    }

    /// Enable or disable harmonic centrality (exact BFS or HyperBall).
    ///
    /// When disabled, harmonic columns remain zero and no HyperBall sketches are allocated.
    /// Discover defaults this to off (`--with-harmonic` to enable).
    pub fn with_harmonic(mut self, enabled: bool) -> Self {
        self.compute_harmonic = enabled;
        self
    }

    /// Deprecated alias for [`Self::with_exact_limit`].
    pub fn with_betweenness_limit(mut self, max_nodes: usize) -> Self {
        self.exact_limit = max_nodes;
        self
    }

    /// Active edge-type filter for this analyzer.
    pub fn allowed_types(&self) -> &[EdgeType] {
        &self.allowed_types
    }

    /// Zero-allocation columnar centrality analysis for large graphs.
    ///
    /// Writes flat computation buffers directly into [`crate::results::AnalysisResults`]
    /// without intermediate UUID hash maps.
    pub fn analyze_columnar(
        &self,
        view: &PetGraphView,
        results: &mut crate::results::AnalysisResults,
    ) -> Result<CentralityRunSummary> {
        use std::time::Instant;

        let node_count = view.node_count();
        if node_count == 0 {
            return Ok(CentralityRunSummary {
                top_pagerank: Vec::new(),
                has_betweenness: false,
                approx_stats: crate::centrality_approx::CentralityApproxStats::default(),
            });
        }

        let (_index, arrays, mut approx_stats) = self.compute_flat_centrality(view)?;
        let fill_start = Instant::now();
        results.fill_centrality_from_flat(
            view,
            &arrays.pagerank,
            &arrays.betweenness,
            &arrays.harmonic,
            &arrays.in_degree,
            &arrays.out_degree,
        );
        approx_stats.columnar_fill_ms = fill_start.elapsed().as_millis() as u64;

        let top_start = Instant::now();
        let (top_pagerank, has_betweenness) = top_pagerank_from_table(results, 10);
        approx_stats.top_k_ms = top_start.elapsed().as_millis() as u64;
        approx_stats.log_profile();

        Ok(CentralityRunSummary {
            top_pagerank,
            has_betweenness,
            approx_stats,
        })
    }

    fn compute_flat_centrality(
        &self,
        view: &PetGraphView,
    ) -> Result<(
        FlatGraphIndex,
        FlatCentralityArrays,
        crate::centrality_approx::CentralityApproxStats,
    )> {
        use crate::centrality_approx::{
            BetweennessMode, CentralityApproxStats, HarmonicMode, HyperBallHarmonic,
            SampledBetweenness,
        };
        use std::time::Instant;

        let allowed = &self.allowed_types;
        let n = view.node_count();

        let index_start = Instant::now();
        let index = FlatGraphIndex::from_view(view, allowed);
        let mut approx_stats = CentralityApproxStats {
            flat_index_ms: index_start.elapsed().as_millis() as u64,
            ..CentralityApproxStats::default()
        };

        let degrees_start = Instant::now();
        let (in_degree, out_degree) = index.filtered_degrees();
        approx_stats.degrees_ms = degrees_start.elapsed().as_millis() as u64;

        let (max_iters, tolerance) =
            adaptive_pagerank_config(n, self.iterations, PAGERANK_TOLERANCE);
        if n > LARGE_GRAPH_PAGERANK_NODE_LIMIT {
            tracing::info!(
                node_count = n,
                max_iters,
                tolerance,
                "Graph exceeds 500K nodes: adaptive PageRank gating active"
            );
        }

        let pagerank_start = Instant::now();
        let pagerank_engine = FastPageRank::new(max_iters, self.damping).with_tolerance(tolerance);
        let pagerank = pagerank_engine.compute_flat_only(&index);
        approx_stats.pagerank_ms = pagerank_start.elapsed().as_millis() as u64;

        // Build outgoing adjacency once, after PageRank (which doesn't need it).
        // Betweenness and harmonic use it for per-node neighbor iteration.
        let needs_adj = n <= self.exact_limit || self.compute_harmonic;
        let out_adj = if needs_adj {
            index.build_out_csr()
        } else {
            CsrOutAdj {
                offsets: vec![0],
                targets: vec![],
            }
        };

        let betweenness = if n <= self.exact_limit {
            approx_stats.betweenness_mode = Some(BetweennessMode::Exact);
            let start = Instant::now();
            let exact_map = BetweennessCentrality::compute_with_adj(view, &index, &out_adj);
            approx_stats.betweenness_ms = start.elapsed().as_millis() as u64;
            align_map_to_flat(view, &exact_map)
        } else {
            // Scale pivots down on huge graphs (512 Brandes pivots ≈ 2.8s on linux).
            let pivots = if n >= 500_000 {
                self.sample_pivots.min(32).min(n)
            } else if n >= 50_000 {
                self.sample_pivots.min(128).min(n)
            } else {
                self.sample_pivots.min(n)
            };
            approx_stats.betweenness_mode = Some(BetweennessMode::Sampled { pivots });
            let start = Instant::now();
            let flat = SampledBetweenness::compute_flat(&index, pivots, self.sample_seed);
            approx_stats.betweenness_ms = start.elapsed().as_millis() as u64;
            flat
        };

        let harmonic = if !self.compute_harmonic {
            approx_stats.harmonic_mode = Some(HarmonicMode::Skipped);
            approx_stats.harmonic_ms = 0;
            vec![0.0; n]
        } else if n <= self.exact_limit {
            approx_stats.harmonic_mode = Some(HarmonicMode::Exact);
            let start = Instant::now();
            let exact_map = HarmonicCentrality::compute_with_adj(view, &index, &out_adj);
            approx_stats.harmonic_ms = start.elapsed().as_millis() as u64;
            align_map_to_flat(view, &exact_map)
        } else {
            let rounds = HyperBallHarmonic::effective_rounds(n, self.hyperball_rounds);
            approx_stats.harmonic_mode = Some(HarmonicMode::HyperBall { rounds });
            let start = Instant::now();
            let flat = HyperBallHarmonic::compute_flat(&index, self.hyperball_rounds);
            approx_stats.harmonic_ms = start.elapsed().as_millis() as u64;
            flat
        };

        Ok((
            index,
            FlatCentralityArrays {
                pagerank,
                betweenness,
                harmonic,
                in_degree,
                out_degree,
            },
            approx_stats,
        ))
    }

    /// Centrality on the induced subgraph of `seed_ids` only (PR scope, depth 0).
    pub fn analyze_scoped(
        &self,
        store: &rgctl_graph::SnapshotNodeStore,
        seed_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, CentralityScores>> {
        use crate::scoped_policy::hydrate_subset;
        use std::collections::HashSet;

        let ids: HashSet<Uuid> = seed_ids.iter().copied().collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let backend = hydrate_subset(store, &ids)?;
        let view = PetGraphView::from_backend(&backend)?;
        Ok(self.analyze_with_view(&view)?.scores)
    }

    /// Calculate centrality metrics for all nodes using the configured edge filter.
    pub fn analyze_with_view(&self, view: &PetGraphView) -> Result<CentralityReport> {
        let n = view.node_count();
        if n == 0 {
            return Ok(CentralityReport {
                scores: HashMap::new(),
                top_pagerank: Vec::new(),
                top_betweenness: Vec::new(),
                approx_stats: crate::centrality_approx::CentralityApproxStats::default(),
            });
        }

        let (index, arrays, approx_stats) = self.compute_flat_centrality(view)?;

        let mut scores: HashMap<Uuid, CentralityScores> = HashMap::with_capacity(n);
        for (node_idx, uuid) in view.index_uuid_iter() {
            let flat_id = node_idx.index();
            scores.insert(
                uuid,
                CentralityScores {
                    pagerank: flat_pagerank_score(&index, &arrays.pagerank, flat_id),
                    betweenness: arrays.betweenness[flat_id],
                    harmonic: arrays.harmonic[flat_id],
                    in_degree: arrays.in_degree[flat_id],
                    out_degree: arrays.out_degree[flat_id],
                },
            );
        }

        let mut top_pagerank: Vec<_> = scores.iter().map(|(id, s)| (*id, s.pagerank)).collect();
        top_pagerank.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_pagerank.truncate(10);

        let mut top_betweenness: Vec<_> =
            scores.iter().map(|(id, s)| (*id, s.betweenness)).collect();
        top_betweenness.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_betweenness.truncate(10);

        approx_stats.log_profile();

        Ok(CentralityReport {
            scores,
            top_pagerank,
            top_betweenness,
            approx_stats,
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
    /// Cyclomatic complexity when known
    pub complexity: Option<i64>,
    /// Combined risk: degree × complexity
    pub risk_score: f64,
}

/// Degree centrality for dashboard (behavioral edges only; skips module/file containers).
pub fn degree_centrality(backend: &MemoryBackend) -> Result<Vec<CentralityScore>> {
    let node_count = backend.node_count();
    if node_count == 0 {
        return Ok(vec![]);
    }

    let view = PetGraphView::from_backend(backend)?;
    let allowed = default_behavioral_edges();
    let degree_map = DegreeCentrality::compute(&view, allowed);

    let betweenness = CentralityAnalyzer::new()
        .analyze_with_view(&view)
        .ok()
        .map(|r| r.scores)
        .unwrap_or_default();

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
            name: node.name.to_string(),
            file_path: node.file_path.as_ref().map(|s| s.to_string()),
            degree,
            betweenness: bt,
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

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::{Edge, Node};

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
            .insert_edge(Edge::new(id_main, id_helper, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_helper, id_leaf, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_leaf, id_helper, EdgeType::Calls))
            .unwrap();

        let report = CentralityAnalyzer::new().analyze(&backend).unwrap();
        assert!(report.scores[&id_main].pagerank > 0.0);
    }

    #[test]
    fn test_harmonic_rewards_reachability() {
        let mut backend = MemoryBackend::new();
        let hub = Node::new(NodeType::Function, "hub".to_string());
        let leaf = Node::new(NodeType::Function, "leaf".to_string());
        let id_hub = hub.id;
        let id_leaf = leaf.id;
        backend.insert_node(hub).unwrap();
        backend.insert_node(leaf).unwrap();
        backend
            .insert_edge(Edge::new(id_hub, id_leaf, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let scores = HarmonicCentrality::compute(&view, &[EdgeType::Calls], 500);
        assert!(scores.get(&id_hub).copied().unwrap_or(0.0) > 0.0);
        assert_eq!(scores.get(&id_leaf).copied().unwrap_or(0.0), 0.0);
    }

    #[test]
    fn test_with_harmonic_false_skips_hyperball() {
        let mut backend = MemoryBackend::new();
        let hub = Node::new(NodeType::Function, "hub".to_string());
        let leaf = Node::new(NodeType::Function, "leaf".to_string());
        let id_hub = hub.id;
        backend.insert_node(hub).unwrap();
        backend.insert_node(leaf.clone()).unwrap();
        backend
            .insert_edge(Edge::new(id_hub, leaf.id, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let report = CentralityAnalyzer::new()
            .with_harmonic(false)
            .analyze_with_view(&view)
            .unwrap();

        assert_eq!(
            report.approx_stats.harmonic_mode,
            Some(crate::centrality_approx::HarmonicMode::Skipped)
        );
        assert_eq!(report.approx_stats.harmonic_ms, 0);
        assert!(
            report.scores.values().all(|s| s.harmonic == 0.0),
            "harmonic must stay zero when disabled"
        );
        assert!(
            report.scores.values().any(|s| s.pagerank > 0.0),
            "PageRank must still run"
        );
    }

    #[test]
    fn test_with_harmonic_false_columnar_leaves_zeros() {
        let mut backend = MemoryBackend::new();
        let hub = Node::new(NodeType::Function, "hub".to_string());
        let leaf = Node::new(NodeType::Function, "leaf".to_string());
        let id_hub = hub.id;
        let id_leaf = leaf.id;
        backend.insert_node(hub).unwrap();
        backend.insert_node(leaf).unwrap();
        backend
            .insert_edge(Edge::new(id_hub, id_leaf, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let mut results = crate::results::AnalysisResults::new(vec![id_hub, id_leaf]);
        let summary = CentralityAnalyzer::new()
            .with_harmonic(false)
            .analyze_columnar(&view, &mut results)
            .unwrap();

        assert_eq!(
            summary.approx_stats.harmonic_mode,
            Some(crate::centrality_approx::HarmonicMode::Skipped)
        );
        let table = results.centrality.as_ref().unwrap();
        assert!(table.harmonic.iter().all(|&h| h == 0.0));
        assert!(table.pagerank.iter().any(|&p| p > 0.0));
    }

    #[test]
    fn test_with_harmonic_true_exact_nonzero() {
        let mut backend = MemoryBackend::new();
        let hub = Node::new(NodeType::Function, "hub".to_string());
        let leaf = Node::new(NodeType::Function, "leaf".to_string());
        let id_hub = hub.id;
        backend.insert_node(hub).unwrap();
        backend.insert_node(leaf.clone()).unwrap();
        backend
            .insert_edge(Edge::new(id_hub, leaf.id, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let report = CentralityAnalyzer::new()
            .with_harmonic(true)
            .analyze_with_view(&view)
            .unwrap();

        assert_eq!(
            report.approx_stats.harmonic_mode,
            Some(crate::centrality_approx::HarmonicMode::Exact)
        );
        assert!(
            report.scores[&id_hub].harmonic > 0.0,
            "hub must have positive out-harmonic when enabled"
        );
    }

    #[test]
    fn test_with_harmonic_false_skips_even_when_hyperball_would_run() {
        // Force HyperBall path (n > exact_limit) then disable harmonic.
        let mut backend = MemoryBackend::new();
        let nodes: Vec<_> = (0..8)
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

        let view = PetGraphView::from_backend(&backend).unwrap();
        let on = CentralityAnalyzer::new()
            .with_exact_limit(2)
            .with_harmonic(true)
            .analyze_with_view(&view)
            .unwrap();
        assert!(
            matches!(
                on.approx_stats.harmonic_mode,
                Some(crate::centrality_approx::HarmonicMode::HyperBall { .. })
            ),
            "sanity: HyperBall path active when harmonic on"
        );

        let off = CentralityAnalyzer::new()
            .with_exact_limit(2)
            .with_harmonic(false)
            .analyze_with_view(&view)
            .unwrap();
        assert_eq!(
            off.approx_stats.harmonic_mode,
            Some(crate::centrality_approx::HarmonicMode::Skipped)
        );
        assert_eq!(off.approx_stats.harmonic_ms, 0);
        assert!(off.scores.values().all(|s| s.harmonic == 0.0));
    }

    #[test]
    fn test_module_isolated_from_contains_edges() {
        let mut backend = MemoryBackend::new();
        let module = Node::new(NodeType::Module, "mod");
        let func = Node::new(NodeType::Function, "f");
        let id_mod = module.id;
        let id_func = func.id;
        backend.insert_node(module).unwrap();
        backend.insert_node(func).unwrap();
        backend
            .insert_edge(Edge::new(id_mod, id_func, EdgeType::Contains))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let (scores, _) = FastPageRank::new(20, 0.85).compute(&view, &[EdgeType::Calls]);
        assert_eq!(scores.get(&id_mod).copied().unwrap_or(0.0), 0.0);
    }

    #[test]
    fn test_adaptive_pagerank_gating() {
        let (iters, tol) = adaptive_pagerank_config(100, 20, PAGERANK_TOLERANCE);
        assert_eq!(iters, 20);
        assert_eq!(tol, PAGERANK_TOLERANCE);

        let (iters, tol) = adaptive_pagerank_config(600_000, 20, PAGERANK_TOLERANCE);
        assert_eq!(iters, LARGE_GRAPH_PAGERANK_ITERATIONS);
        assert_eq!(tol, LARGE_GRAPH_PAGERANK_TOLERANCE);
    }

    #[test]
    fn test_analyze_columnar_matches_report() {
        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main".to_string());
        let helper = Node::new(NodeType::Function, "helper".to_string());
        let id_main = main.id;
        let id_helper = helper.id;
        backend.insert_node(main).unwrap();
        backend.insert_node(helper).unwrap();
        backend
            .insert_edge(Edge::new(id_main, id_helper, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let mut results = crate::results::AnalysisResults::new(vec![id_main, id_helper]);

        let summary = CentralityAnalyzer::new()
            .analyze_columnar(&view, &mut results)
            .unwrap();
        let report = CentralityAnalyzer::new().analyze_with_view(&view).unwrap();

        assert!(summary.top_pagerank[0].1 > 0.0);
        assert_eq!(summary.top_pagerank[0].0, report.top_pagerank[0].0);

        let table = results.centrality.as_ref().unwrap();
        let main_id = results.get_compact_id(id_main).unwrap() as usize;
        let helper_id = results.get_compact_id(id_helper).unwrap() as usize;
        assert!(
            (f64::from(table.pagerank[main_id]) - report.scores[&id_main].pagerank).abs() < 1e-5
        );
        assert!(
            (f64::from(table.pagerank[helper_id]) - report.scores[&id_helper].pagerank).abs()
                < 1e-5
        );
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
            .insert_edge(Edge::new(nodes[3].id, nodes[0].id, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(nodes[2].id, nodes[3].id, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
        let (_, stats) = FastPageRank::new(100, 0.85).compute_flat(&index);
        assert!(stats.converged);
        assert!(stats.max_delta < PAGERANK_TOLERANCE);
    }

    #[test]
    fn node_role_classifies_tests_and_intrinsics() {
        assert_eq!(
            classify_node_role("login", Some("src/auth.rs"), false),
            NodeRole::Production
        );
        assert_eq!(
            classify_node_role("helper", Some("tests/auth_test.rs"), false),
            NodeRole::Test
        );
        assert_eq!(
            classify_node_role("foo", Some("pkg/foo.test.ts"), false),
            NodeRole::Test
        );
        assert_eq!(
            classify_node_role("assert", Some("src/lib.rs"), true),
            NodeRole::ExternalIntrinsic
        );
        assert_eq!(
            classify_node_role("println!", None, false),
            NodeRole::ExternalIntrinsic
        );
        assert_eq!(
            classify_node_role("foo::tests::case_a", Some("src/lib.rs"), false),
            NodeRole::Test
        );
    }

    #[test]
    fn filter_scores_excludes_tests_by_default() {
        let prod = Uuid::from_u128(1);
        let test = Uuid::from_u128(2);
        let intrinsic = Uuid::from_u128(3);
        let mut scores = HashMap::new();
        scores.insert(prod, 0.5);
        scores.insert(test, 0.9);
        scores.insert(intrinsic, 0.8);
        let role = |id: Uuid| match id.as_u128() {
            1 => NodeRole::Production,
            2 => NodeRole::Test,
            _ => NodeRole::ExternalIntrinsic,
        };
        let filtered = filter_scores_by_role(&scores, false, role);
        assert_eq!(filtered, vec![(prod, 0.5)]);
        let with_tests = filter_scores_by_role(&scores, true, role);
        assert_eq!(with_tests, vec![(test, 0.9), (prod, 0.5)]);
        assert!(
            !with_tests.iter().any(|(id, _)| *id == intrinsic),
            "ExternalIntrinsic must stay excluded when include_tests=true"
        );
    }
}
