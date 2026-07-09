//! Approximate centrality for large graphs — sampled betweenness (RANDES) and HyperBall harmonic.

use crate::centrality::FlatGraphIndex;
use crate::graph_utils::PetGraphView;
use rbuilder_graph::schema::EdgeType;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

/// Default pivot count for sampled betweenness (Eppstein–Wang / RANDES style).
pub const DEFAULT_SAMPLE_PIVOTS: usize = 512;

/// Graphs at or below this size use exact Brandes / BFS harmonic.
pub const DEFAULT_EXACT_CENTRALITY_LIMIT: usize = 500;

/// Graphs at or below this size use exact set propagation inside HyperBall.
pub const HYPERBALL_EXACT_THRESHOLD: usize = 8_192;

/// HyperBall propagation rounds (software graphs typically saturate within 8–16).
pub const DEFAULT_HYPERBALL_ROUNDS: usize = 16;

/// HyperLogLog precision (p=14 → m=16384 registers, ~1.6% typical error).
pub const HYPERLOGLOG_PRECISION: u8 = 14;

/// How betweenness was computed for the current analysis pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BetweennessMode {
    /// Exact Brandes on all sources.
    Exact,
    /// Sampled Brandes from `k` pivot sources.
    Sampled {
        /// Number of pivot sources used.
        pivots: usize,
    },
}

/// How harmonic centrality was computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarmonicMode {
    /// Exact all-pairs BFS.
    Exact,
    /// HyperBall / HyperLogLog ball-size estimation.
    HyperBall {
        /// Propagation rounds executed.
        rounds: usize,
    },
}

/// Timing and mode metadata for a centrality pass.
#[derive(Debug, Clone, Default)]
pub struct CentralityApproxStats {
    /// Betweenness algorithm used.
    pub betweenness_mode: Option<BetweennessMode>,
    /// Harmonic algorithm used.
    pub harmonic_mode: Option<HarmonicMode>,
    /// Wall time for betweenness (ms).
    pub betweenness_ms: u64,
    /// Wall time for harmonic (ms).
    pub harmonic_ms: u64,
}

/// Sampled betweenness via `k` random Brandes single-source passes (RANDES).
pub struct SampledBetweenness;

impl SampledBetweenness {
    /// Estimate normalized betweenness using `pivot_count` sources on a flat index.
    pub fn compute_flat(
        index: &FlatGraphIndex,
        pivot_count: usize,
        seed: u64,
    ) -> Vec<f64> {
        let n = index.node_count;
        if n == 0 {
            return Vec::new();
        }
        let k = pivot_count.min(n);
        if k == 0 {
            return vec![0.0; n];
        }

        let out_adj = build_out_adjacency(index);
        let pivots = sample_pivot_indices(n, k, seed);
        let mut betweenness = vec![0.0f64; n];

        for &source in &pivots {
            let partial = brandes_single_source(&out_adj, source, n);
            for (i, delta) in partial.iter().enumerate() {
                betweenness[i] += delta;
            }
        }

        let norm = if n > 2 {
            (n as f64 / k as f64) / ((n - 1) as f64 * (n - 2) as f64)
        } else {
            0.0
        };
        betweenness.iter_mut().for_each(|score| *score *= norm);
        betweenness
    }

    /// Map flat betweenness scores back to UUIDs.
    pub fn compute(
        view: &PetGraphView,
        allowed_types: &[EdgeType],
        pivot_count: usize,
        seed: u64,
    ) -> HashMap<Uuid, f64> {
        let index = FlatGraphIndex::from_view(view, allowed_types);
        let flat = Self::compute_flat(&index, pivot_count, seed);
        view.index_to_uuid
            .iter()
            .map(|(idx, uuid)| (idx.index(), *uuid))
            .filter_map(|(flat_id, uuid)| flat.get(flat_id).copied().map(|s| (uuid, s)))
            .collect()
    }
}

/// HyperBall harmonic centrality using HyperLogLog ball-size estimation.
pub struct HyperBallHarmonic;

impl HyperBallHarmonic {
    /// Estimate normalized out-harmonic scores on a flat directed projection.
    pub fn compute_flat(index: &FlatGraphIndex, max_rounds: usize) -> Vec<f64> {
        let n = index.node_count;
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![0.0];
        }
        if n <= HYPERBALL_EXACT_THRESHOLD {
            return hyperball_exact(index, max_rounds);
        }
        hyperball_hll(index, max_rounds)
    }

    /// Map flat harmonic scores back to UUIDs.
    pub fn compute(
        view: &PetGraphView,
        allowed_types: &[EdgeType],
        max_rounds: usize,
    ) -> HashMap<Uuid, f64> {
        let index = FlatGraphIndex::from_view(view, allowed_types);
        let flat = Self::compute_flat(&index, max_rounds);
        view.index_to_uuid
            .iter()
            .map(|(idx, uuid)| (idx.index(), *uuid))
            .filter_map(|(flat_id, uuid)| flat.get(flat_id).copied().map(|s| (uuid, s)))
            .collect()
    }
}

/// HyperLogLog cardinality sketch (mergeable, fixed precision).
#[derive(Clone)]
pub struct HyperLogLog {
    registers: Vec<u8>,
    precision: u8,
}

impl HyperLogLog {
    /// Create a sketch with `2^precision` registers.
    pub fn new(precision: u8) -> Self {
        let m = 1usize << precision;
        Self {
            registers: vec![0; m],
            precision,
        }
    }

    /// Add an element to the sketch.
    pub fn add(&mut self, value: u64) {
        let hash = splitmix64(value);
        let m = self.registers.len();
        let idx = (hash >> (64 - self.precision)) as usize % m;
        let w = hash | (1u64 << 63);
        let rho = (w.leading_zeros() as u8).saturating_add(1);
        if rho > self.registers[idx] {
            self.registers[idx] = rho;
        }
    }

    /// Merge another sketch (pointwise max).
    pub fn merge(&mut self, other: &Self) {
        debug_assert_eq!(self.precision, other.precision);
        for (a, b) in self.registers.iter_mut().zip(&other.registers) {
            if *b > *a {
                *a = *b;
            }
        }
    }

    /// Reset registers to zero (reuse sketch allocation across HyperBall rounds).
    pub fn reset(&mut self) {
        self.registers.fill(0);
    }

    /// Register count `m = 2^precision`.
    pub fn register_count(&self) -> usize {
        self.registers.len()
    }

    /// Estimate distinct count.
    pub fn estimate(&self) -> f64 {
        let m = self.registers.len() as f64;
        if m == 0.0 {
            return 0.0;
        }
        let sum: f64 = self
            .registers
            .iter()
            .map(|&rho| 2f64.powi(-i32::from(rho)))
            .sum();
        if sum <= 0.0 {
            return 0.0;
        }
        let alpha = match self.registers.len() {
            16 => 0.673,
            32 => 0.697,
            64 => 0.709,
            _ if self.registers.len() >= 128 => 0.7213 / (1.0 + 1.079 / m),
            _ => 0.75,
        };
        let raw = alpha * m * m / sum;
        if raw <= 2.5 * m {
            let zeros = self.registers.iter().filter(|&&r| r == 0).count() as f64;
            if zeros > 0.0 {
                return m * (m / zeros).ln();
            }
        }
        raw
    }
}

fn hyperball_exact(index: &FlatGraphIndex, max_rounds: usize) -> Vec<f64> {
    let n = index.node_count;
    let out_adj = build_out_adjacency(index);
    let rounds = max_rounds.max(1);
    let norm = 1.0 / (n as f64 - 1.0);

    let mut balls: Vec<HashSet<usize>> = (0..n).map(|i| HashSet::from([i])).collect();
    let mut harmonic = vec![0.0f64; n];
    let mut prev_count: Vec<usize> = vec![1; n];

    for distance in 1..=rounds {
        let mut next: Vec<HashSet<usize>> = (0..n).map(|i| HashSet::from([i])).collect();
        for node in 0..n {
            for &neighbor in &out_adj[node] {
                for &reachable in &balls[neighbor] {
                    next[node].insert(reachable);
                }
            }
        }

        let mut grew = false;
        for node in 0..n {
            let count = next[node].len();
            let layer = count.saturating_sub(prev_count[node]);
            if layer > 0 {
                harmonic[node] += layer as f64 / distance as f64;
                grew = true;
            }
            prev_count[node] = count;
        }

        balls = next;
        if !grew {
            break;
        }
    }

    harmonic.iter_mut().for_each(|score| *score *= norm);
    harmonic
}

fn hll_precision_for(node_count: usize) -> u8 {
    if node_count <= HYPERBALL_EXACT_THRESHOLD {
        HYPERLOGLOG_PRECISION
    } else if node_count <= 100_000 {
        12
    } else {
        10
    }
}

fn hyperball_hll(index: &FlatGraphIndex, max_rounds: usize) -> Vec<f64> {
    let n = index.node_count;
    let out_adj = build_out_adjacency(index);
    let rounds = max_rounds.max(1);
    let norm = 1.0 / (n as f64 - 1.0);
    let precision = hll_precision_for(n);

    let mut current: Vec<HyperLogLog> = (0..n)
        .map(|node| {
            let mut hll = HyperLogLog::new(precision);
            hll.add(hash_node_id(node));
            hll
        })
        .collect();
    let mut next: Vec<HyperLogLog> = (0..n)
        .map(|_| HyperLogLog::new(precision))
        .collect();

    let mut harmonic = vec![0.0f64; n];
    let mut prev_count: Vec<f64> = vec![1.0; n];

    for distance in 1..=rounds {
        for node in 0..n {
            next[node].reset();
            next[node].add(hash_node_id(node));
            for &neighbor in &out_adj[node] {
                next[node].merge(&current[neighbor]);
            }
        }

        let mut grew = false;
        for node in 0..n {
            let estimate = next[node].estimate();
            let layer = (estimate - prev_count[node]).max(0.0);
            if layer > 0.0 {
                harmonic[node] += layer / distance as f64;
                grew = true;
            }
            prev_count[node] = estimate;
        }

        std::mem::swap(&mut current, &mut next);
        if !grew {
            break;
        }
    }

    harmonic.iter_mut().for_each(|score| *score *= norm);
    harmonic
}

fn build_out_adjacency(index: &FlatGraphIndex) -> Vec<Vec<usize>> {
    let mut adj = vec![Vec::new(); index.node_count];
    for &(src, dst) in &index.flat_edges {
        adj[src].push(dst);
    }
    adj
}

fn hash_node_id(node: usize) -> u64 {
    splitmix64(node as u64 ^ 0x9E37_79B9_7F4A_7C15)
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn sample_pivot_indices(n: usize, k: usize, seed: u64) -> Vec<usize> {
    if k >= n {
        return (0..n).collect();
    }
    let mut rng = seed;
    let mut chosen = vec![false; n];
    let mut pivots = Vec::with_capacity(k);
    while pivots.len() < k {
        rng = rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let idx = (rng as usize) % n;
        if !chosen[idx] {
            chosen[idx] = true;
            pivots.push(idx);
        }
    }
    pivots
}

/// Brandes single-source accumulation on a flat directed adjacency list.
fn brandes_single_source(out_adj: &[Vec<usize>], source: usize, n: usize) -> Vec<f64> {
    let mut stack = Vec::new();
    let mut pred = vec![Vec::new(); n];
    let mut sigma = vec![0.0f64; n];
    let mut dist = vec![-1i32; n];
    let mut dependency = vec![0.0f64; n];
    let mut partial = vec![0.0f64; n];

    dist[source] = 0;
    sigma[source] = 1.0;
    let mut queue = VecDeque::new();
    queue.push_back(source);

    while let Some(v) = queue.pop_front() {
        stack.push(v);
        for &w in &out_adj[v] {
            if dist[w] < 0 {
                dist[w] = dist[v] + 1;
                queue.push_back(w);
            }
            if dist[w] == dist[v] + 1 {
                sigma[w] += sigma[v];
                pred[w].push(v);
            }
        }
    }

    while let Some(w) = stack.pop() {
        for &v in &pred[w] {
            if sigma[w] > 0.0 {
                dependency[v] += (sigma[v] / sigma[w]) * (1.0 + dependency[w]);
            }
        }
        if w != source {
            partial[w] = dependency[w];
        }
    }

    partial
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::centrality::{BetweennessCentrality, FlatGraphIndex};
    use crate::graph_utils::PetGraphView;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, EdgeType, Node, NodeType};
    use uuid::Uuid;

    fn line_graph(n: usize) -> (PetGraphView, Vec<Uuid>) {
        let mut backend = rbuilder_graph::backend::MemoryBackend::new();
        let nodes: Vec<_> = (0..n)
            .map(|i| {
                let node = Node::new(NodeType::Function, format!("n{i}"));
                backend.insert_node(node.clone()).unwrap();
                node
            })
            .collect();
        for w in nodes.windows(2) {
            backend
                .insert_edge(Edge::new(w[0].id, w[1].id, EdgeType::Calls))
                .unwrap();
        }
        (
            PetGraphView::from_backend(&backend).unwrap(),
            nodes.into_iter().map(|n| n.id).collect(),
        )
    }

    #[test]
    fn hyperloglog_merge_estimates_union() {
        let mut a = HyperLogLog::new(12);
        let mut b = HyperLogLog::new(12);
        for i in 0..100 {
            a.add(i);
        }
        for i in 50..150 {
            b.add(i);
        }
        let mut merged = a.clone();
        merged.merge(&b);
        let est = merged.estimate();
        assert!(
            (120.0..180.0).contains(&est),
            "expected ~150 distinct, got {est}"
        );
    }

    #[test]
    fn sampled_betweenness_finds_bridge() {
        let mut backend = rbuilder_graph::backend::MemoryBackend::new();
        let bridge = Node::new(NodeType::Function, "bridge".into());
        let id_bridge = bridge.id;
        backend.insert_node(bridge).unwrap();

        let mut left = Vec::new();
        let mut right = Vec::new();
        for side in 0..2 {
            let mut chain = Vec::new();
            for i in 0..8 {
                let n = Node::new(NodeType::Function, format!("c{side}_{i}"));
                chain.push(n.id);
                backend.insert_node(n).unwrap();
                if i > 0 {
                    backend
                        .insert_edge(Edge::new(chain[i - 1], chain[i], EdgeType::Calls))
                        .unwrap();
                }
            }
            if side == 0 {
                left = chain;
            } else {
                right = chain;
            }
        }
        backend
            .insert_edge(Edge::new(*left.last().unwrap(), id_bridge, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_bridge, right[0], EdgeType::Calls))
            .unwrap();
        for w in right.windows(2) {
            backend
                .insert_edge(Edge::new(w[0], w[1], EdgeType::Calls))
                .unwrap();
        }

        let view = PetGraphView::from_backend(&backend).unwrap();
        let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
        let sampled = SampledBetweenness::compute_flat(&index, 64, 42);
        let exact = BetweennessCentrality::compute_unbounded(&view, &[EdgeType::Calls]);

        let bridge_flat = view.uuid_to_index[&id_bridge].index();
        let bridge_sampled = sampled[bridge_flat];
        let bridge_exact = exact.get(&id_bridge).copied().unwrap_or(0.0);
        assert!(
            bridge_sampled > 0.0,
            "sampled bridge score must be positive"
        );
        assert!(
            bridge_sampled >= bridge_exact * 0.25,
            "sampled {bridge_sampled} too far below exact {bridge_exact}"
        );
    }

    #[test]
    fn hyperball_harmonic_prefers_hub_on_line() {
        let (view, ids) = line_graph(8);
        let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
        let harmonic = HyperBallHarmonic::compute_flat(&index, 8);
        let head = view.uuid_to_index[&ids[0]].index();
        let tail = view.uuid_to_index[&ids[7]].index();
        assert!(
            harmonic[head] > harmonic[tail],
            "head hub {} should beat tail {}",
            harmonic[head],
            harmonic[tail]
        );
        assert!(harmonic[tail] < 1e-9, "tail should be ~0, got {}", harmonic[tail]);
    }

    #[test]
    fn hyperball_hll_large_mock_produces_scores() {
        let (view, ids) = line_graph(10_000);
        let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
        let harmonic = HyperBallHarmonic::compute_flat(&index, 12);
        let head = view.uuid_to_index[&ids[0]].index();
        assert!(harmonic[head] > 0.0);
        assert!(harmonic.iter().any(|&s| s > 0.0));
    }

    #[test]
    fn sampled_correlates_with_exact_on_small_graph() {
        let (view, _) = line_graph(40);
        let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
        let sampled = SampledBetweenness::compute_flat(&index, 16, 7);
        let exact_map = BetweennessCentrality::compute_unbounded(&view, &[EdgeType::Calls]);
        let mut exact = vec![0.0; index.node_count];
        for (idx, uuid) in &view.index_to_uuid {
            exact[idx.index()] = exact_map.get(uuid).copied().unwrap_or(0.0);
        }

        let corr = spearman_correlation(&sampled, &exact);
        assert!(
            corr > 0.7,
            "sampled/exact rank correlation too low: {corr}"
        );
    }

    fn spearman_correlation(a: &[f64], b: &[f64]) -> f64 {
        let n = a.len().min(b.len());
        if n < 2 {
            return 1.0;
        }
        let rank = |vals: &[f64]| -> Vec<f64> {
            let mut order: Vec<usize> = (0..vals.len()).collect();
            order.sort_by(|&i, &j| vals[i].partial_cmp(&vals[j]).unwrap());
            let mut ranks = vec![0.0; vals.len()];
            for (rank_idx, &pos) in order.iter().enumerate() {
                ranks[pos] = rank_idx as f64;
            }
            ranks
        };
        let ra = rank(&a[..n]);
        let rb = rank(&b[..n]);
        let mean_a = ra.iter().sum::<f64>() / n as f64;
        let mean_b = rb.iter().sum::<f64>() / n as f64;
        let mut num = 0.0;
        let mut den_a = 0.0;
        let mut den_b = 0.0;
        for i in 0..n {
            let da = ra[i] - mean_a;
            let db = rb[i] - mean_b;
            num += da * db;
            den_a += da * da;
            den_b += db * db;
        }
        if den_a == 0.0 || den_b == 0.0 {
            return 1.0;
        }
        num / (den_a.sqrt() * den_b.sqrt())
    }
}
