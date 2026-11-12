//! Approximate centrality for large graphs — sampled betweenness (RANDES) and HyperBall harmonic.

use crate::centrality::{CsrOutAdj, FlatGraphIndex};
use crate::graph_utils::PetGraphView;
use bit_set::BitSet;
use rayon::prelude::*;
use rgctl_graph::schema::EdgeType;
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

/// Default pivot count for sampled betweenness (Eppstein–Wang / RANDES style).
pub const DEFAULT_SAMPLE_PIVOTS: usize = 512;

/// Graphs at or below this size use exact Brandes / BFS harmonic.
pub const DEFAULT_EXACT_CENTRALITY_LIMIT: usize = 500;

/// Graphs at or below this size use exact set propagation inside HyperBall.
pub const HYPERBALL_EXACT_THRESHOLD: usize = 8_192;

/// HyperBall propagation rounds (software graphs typically saturate within 8–16).
pub const DEFAULT_HYPERBALL_ROUNDS: usize = 16;

/// Node count above which HyperBall propagation rounds are capped.
pub const LARGE_GRAPH_HYPERBALL_NODE_LIMIT: usize = 500_000;

/// HyperBall round cap for graphs above [`LARGE_GRAPH_HYPERBALL_NODE_LIMIT`].
pub const LARGE_GRAPH_HYPERBALL_ROUNDS: usize = 8;

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
    /// Harmonic skipped (e.g. discover without `--with-harmonic`).
    Skipped,
}

/// Timing and mode metadata for a centrality pass.
#[derive(Debug, Clone, Default)]
pub struct CentralityApproxStats {
    /// Betweenness algorithm used.
    pub betweenness_mode: Option<BetweennessMode>,
    /// Harmonic algorithm used.
    pub harmonic_mode: Option<HarmonicMode>,
    /// Wall time to build the flat edge index (ms).
    pub flat_index_ms: u64,
    /// Wall time to compute filtered in/out degrees (ms).
    pub degrees_ms: u64,
    /// Wall time for PageRank power iteration (ms).
    pub pagerank_ms: u64,
    /// Wall time for betweenness (ms).
    pub betweenness_ms: u64,
    /// Wall time for harmonic centrality (ms).
    pub harmonic_ms: u64,
    /// Wall time to copy flat scores into columnar table (ms).
    pub columnar_fill_ms: u64,
    /// Wall time to scan for top-k PageRank nodes (ms).
    pub top_k_ms: u64,
}

impl CentralityApproxStats {
    /// Sum of all recorded sub-phase timings (ms).
    pub fn timed_total_ms(&self) -> u64 {
        self.flat_index_ms
            + self.degrees_ms
            + self.pagerank_ms
            + self.betweenness_ms
            + self.harmonic_ms
            + self.columnar_fill_ms
            + self.top_k_ms
    }

    /// Emit `[profile] centrality sub-phase` lines (grep with `RUST_LOG=profile=info`).
    pub fn log_profile(&self) {
        let total_ms = self.timed_total_ms().max(1);
        let total_secs = total_ms as f64 / 1000.0;

        tracing::info!(
            target: "profile",
            total_secs,
            flat_index_secs = self.flat_index_ms as f64 / 1000.0,
            degrees_secs = self.degrees_ms as f64 / 1000.0,
            pagerank_secs = self.pagerank_ms as f64 / 1000.0,
            betweenness_secs = self.betweenness_ms as f64 / 1000.0,
            harmonic_secs = self.harmonic_ms as f64 / 1000.0,
            columnar_fill_secs = self.columnar_fill_ms as f64 / 1000.0,
            top_k_secs = self.top_k_ms as f64 / 1000.0,
            "[profile] centrality breakdown"
        );

        for (name, ms) in [
            ("flat_index", self.flat_index_ms),
            ("degrees", self.degrees_ms),
            ("pagerank", self.pagerank_ms),
            ("betweenness", self.betweenness_ms),
            ("harmonic", self.harmonic_ms),
            ("columnar_fill", self.columnar_fill_ms),
            ("top_k", self.top_k_ms),
        ] {
            if ms == 0 {
                continue;
            }
            tracing::info!(
                target: "profile",
                subphase = name,
                secs = ms as f64 / 1000.0,
                pct_centrality = 100.0 * ms as f64 / total_ms as f64,
                "[profile] centrality sub-phase"
            );
        }
    }
}

/// Sampled betweenness via `k` random Brandes single-source passes (RANDES).
pub struct SampledBetweenness;

impl SampledBetweenness {
    /// Estimate normalized betweenness using `pivot_count` sources on a flat index.
    pub fn compute_flat(index: &FlatGraphIndex, pivot_count: usize, seed: u64) -> Vec<f64> {
        let n = index.node_count;
        if n == 0 || pivot_count == 0 {
            return vec![0.0; n];
        }

        let out_adj = index.build_out_csr();
        // Sources with out-degree 0 never reach another vertex and contribute
        // identically zero to Brandes partial dependencies; restricting the
        // sampling frame to active sources makes Brandes exact and deterministic
        // whenever |active| <= pivot_count and strictly reduces variance otherwise.
        let active_sources: Vec<usize> = (0..n)
            .filter(|&u| !out_adj.neighbors(u).is_empty())
            .collect();
        if active_sources.is_empty() {
            return vec![0.0; n];
        }
        let k = pivot_count.min(active_sources.len());
        let pivots: Vec<usize> = sample_pivot_indices(active_sources.len(), k, seed)
            .into_iter()
            .map(|i| active_sources[i])
            .collect();

        // Small graphs pay Rayon pool sync without enough work to amortize it.
        const PARALLEL_THRESHOLD: usize = 4_096;
        let mut betweenness = if n >= PARALLEL_THRESHOLD {
            pivots
                .par_iter()
                .fold(
                    || (vec![0.0f64; n], BrandesScratch::new(n)),
                    |mut state, &source| {
                        let (ref mut acc, ref mut scratch) = state;
                        scratch.run(&out_adj, source, n);
                        for (i, delta) in scratch.partial().iter().enumerate() {
                            acc[i] += delta;
                        }
                        state
                    },
                )
                .map(|(acc, _)| acc)
                .reduce(
                    || vec![0.0f64; n],
                    |mut a, b| {
                        for (x, y) in a.iter_mut().zip(&b) {
                            *x += y;
                        }
                        a
                    },
                )
        } else {
            let mut acc = vec![0.0f64; n];
            let mut scratch = BrandesScratch::new(n);
            for &source in &pivots {
                scratch.run(&out_adj, source, n);
                for (i, delta) in scratch.partial().iter().enumerate() {
                    acc[i] += delta;
                }
            }
            acc
        };

        let norm = if n > 2 {
            (active_sources.len() as f64 / k as f64) / ((n - 1) as f64 * (n - 2) as f64)
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
        view.index_uuid_iter()
            .filter_map(|(idx, uuid)| flat.get(idx.index()).copied().map(|s| (uuid, s)))
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

        let rounds = adaptive_hyperball_rounds(n, max_rounds);
        if n > LARGE_GRAPH_HYPERBALL_NODE_LIMIT {
            tracing::info!(
                node_count = n,
                rounds,
                "Graph exceeds 500K nodes: adaptive HyperBall gating active"
            );
        }
        hyperball_hll_parallel(index, rounds)
    }

    /// Effective HyperBall rounds for a graph size (after adaptive gating).
    pub fn effective_rounds(node_count: usize, max_rounds: usize) -> usize {
        adaptive_hyperball_rounds(node_count, max_rounds)
    }

    /// Map flat harmonic scores back to UUIDs.
    pub fn compute(
        view: &PetGraphView,
        allowed_types: &[EdgeType],
        max_rounds: usize,
    ) -> HashMap<Uuid, f64> {
        let index = FlatGraphIndex::from_view(view, allowed_types);
        let flat = Self::compute_flat(&index, max_rounds);
        view.index_uuid_iter()
            .filter_map(|(idx, uuid)| flat.get(idx.index()).copied().map(|s| (uuid, s)))
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
    let out_adj = index.build_out_csr();
    let rounds = max_rounds.max(1);
    let norm = 1.0 / (n as f64 - 1.0);

    let mut current: Vec<BitSet> = (0..n)
        .map(|i| {
            let mut b = BitSet::with_capacity(n);
            b.insert(i);
            b
        })
        .collect();
    let mut next: Vec<BitSet> = (0..n).map(|_| BitSet::with_capacity(n)).collect();
    let mut harmonic = vec![0.0f64; n];
    let mut prev_count: Vec<usize> = vec![1; n];

    for distance in 1..=rounds {
        for node in 0..n {
            next[node].clone_from(&current[node]);
            for &neighbor in out_adj.neighbors(node) {
                next[node].union_with(&current[neighbor as usize]);
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

        std::mem::swap(&mut current, &mut next);
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

fn adaptive_hyperball_rounds(node_count: usize, max_rounds: usize) -> usize {
    if node_count > LARGE_GRAPH_HYPERBALL_NODE_LIMIT {
        LARGE_GRAPH_HYPERBALL_ROUNDS.min(max_rounds.max(1))
    } else {
        max_rounds.max(1)
    }
}

fn hyperball_hll_parallel(index: &FlatGraphIndex, rounds: usize) -> Vec<f64> {
    let n = index.node_count;
    let out_adj = index.build_out_csr();
    let norm = 1.0 / (n as f64 - 1.0);
    let precision = hll_precision_for(n);
    let m = 1usize << precision;

    let mut current = vec![0u8; n * m];
    let mut next = vec![0u8; n * m];
    for node in 0..n {
        let slice = &mut current[node * m..(node + 1) * m];
        hll_add(slice, precision, hash_node_id(node));
    }

    let mut harmonic = vec![0.0f64; n];
    let mut prev_count: Vec<f64> = vec![1.0; n];

    for distance in 1..=rounds {
        next.par_chunks_mut(m).enumerate().for_each(|(node, dst)| {
            dst.fill(0);
            hll_add(dst, precision, hash_node_id(node));
            for &neighbor in out_adj.neighbors(node) {
                let nb = neighbor as usize;
                let src = &current[nb * m..(nb + 1) * m];
                hll_merge_max(dst, src);
            }
        });

        let mut grew = false;
        for node in 0..n {
            let estimate = hll_estimate(&next[node * m..(node + 1) * m]);
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

    harmonic.par_iter_mut().for_each(|score| *score *= norm);
    harmonic
}

fn hll_add(registers: &mut [u8], precision: u8, value: u64) {
    let hash = splitmix64(value);
    let m = registers.len();
    let idx = (hash >> (64 - precision)) as usize % m;
    let w = hash | (1u64 << 63);
    let rho = (w.leading_zeros() as u8).saturating_add(1);
    if rho > registers[idx] {
        registers[idx] = rho;
    }
}

fn hll_merge_max(dst: &mut [u8], src: &[u8]) {
    debug_assert_eq!(dst.len(), src.len());
    for (a, &b) in dst.iter_mut().zip(src) {
        if b > *a {
            *a = b;
        }
    }
}

fn hll_estimate(registers: &[u8]) -> f64 {
    let m = registers.len() as f64;
    if m == 0.0 {
        return 0.0;
    }
    let sum: f64 = registers
        .iter()
        .map(|&rho| 2f64.powi(-i32::from(rho)))
        .sum();
    if sum <= 0.0 {
        return 0.0;
    }
    let alpha = match registers.len() {
        16 => 0.673,
        32 => 0.697,
        64 => 0.709,
        _ if registers.len() >= 128 => 0.7213 / (1.0 + 1.079 / m),
        _ => 0.75,
    };
    let raw = alpha * m * m / sum;
    if raw <= 2.5 * m {
        let zeros = registers.iter().filter(|&&r| r == 0).count() as f64;
        if zeros > 0.0 {
            return m * (m / zeros).ln();
        }
    }
    raw
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
    if k >= n / 2 {
        let mut indices: Vec<usize> = (0..n).collect();
        let mut rng = seed;
        for i in 0..k {
            rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let j = i + ((rng as usize) % (n - i));
            indices.swap(i, j);
        }
        indices.truncate(k);
        return indices;
    }
    let mut rng = seed;
    let mut chosen = vec![false; n];
    let mut pivots = Vec::with_capacity(k);
    while pivots.len() < k {
        rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let idx = (rng as usize) % n;
        if !chosen[idx] {
            chosen[idx] = true;
            pivots.push(idx);
        }
    }
    pivots
}

/// Reusable Brandes single-source scratch buffers (per Rayon worker).
pub(crate) struct BrandesScratch {
    pred: Vec<Vec<usize>>,
    sigma: Vec<f64>,
    dist: Vec<i32>,
    dependency: Vec<f64>,
    partial: Vec<f64>,
    stack: Vec<usize>,
    queue: VecDeque<usize>,
}

impl BrandesScratch {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            pred: vec![Vec::new(); n],
            sigma: vec![0.0; n],
            dist: vec![-1; n],
            dependency: vec![0.0; n],
            partial: vec![0.0; n],
            stack: Vec::new(),
            queue: VecDeque::new(),
        }
    }

    pub(crate) fn partial(&self) -> &[f64] {
        &self.partial
    }

    pub(crate) fn run(&mut self, out_adj: &CsrOutAdj, source: usize, _n: usize) {
        self.stack.clear();
        self.queue.clear();
        for list in &mut self.pred {
            list.clear();
        }
        self.sigma.fill(0.0);
        self.dist.fill(-1);
        self.dependency.fill(0.0);
        self.partial.fill(0.0);

        self.dist[source] = 0;
        self.sigma[source] = 1.0;
        self.queue.push_back(source);

        while let Some(v) = self.queue.pop_front() {
            self.stack.push(v);
            for &w in out_adj.neighbors(v) {
                let w = w as usize;
                if self.dist[w] < 0 {
                    self.dist[w] = self.dist[v] + 1;
                    self.queue.push_back(w);
                }
                if self.dist[w] == self.dist[v] + 1 {
                    self.sigma[w] += self.sigma[v];
                    self.pred[w].push(v);
                }
            }
        }

        while let Some(w) = self.stack.pop() {
            for &v in &self.pred[w] {
                if self.sigma[w] > 0.0 {
                    self.dependency[v] +=
                        (self.sigma[v] / self.sigma[w]) * (1.0 + self.dependency[w]);
                }
            }
            if w != source {
                self.partial[w] = self.dependency[w];
            }
        }
    }
}

/// Brandes single-source accumulation on a flat directed adjacency list.
#[allow(dead_code)]
pub(crate) fn brandes_single_source(out_adj: &CsrOutAdj, source: usize, n: usize) -> Vec<f64> {
    let mut scratch = BrandesScratch::new(n);
    scratch.run(out_adj, source, n);
    scratch.partial().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::centrality::{BetweennessCentrality, FlatGraphIndex};
    use crate::graph_utils::PetGraphView;
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
    use uuid::Uuid;

    fn line_graph(n: usize) -> (PetGraphView, Vec<Uuid>) {
        let mut backend = rgctl_graph::backend::MemoryBackend::new();
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
    fn adaptive_hyperball_rounds_gating() {
        assert_eq!(adaptive_hyperball_rounds(100, 16), 16);
        assert_eq!(
            adaptive_hyperball_rounds(600_000, 16),
            LARGE_GRAPH_HYPERBALL_ROUNDS
        );
        assert_eq!(
            HyperBallHarmonic::effective_rounds(600_000, 16),
            LARGE_GRAPH_HYPERBALL_ROUNDS
        );
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
        let mut backend = rgctl_graph::backend::MemoryBackend::new();
        let bridge = Node::new(NodeType::Function, "bridge");
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
        assert!(
            harmonic[tail] < 1e-9,
            "tail should be ~0, got {}",
            harmonic[tail]
        );
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
    fn sample_pivot_indices_fisher_yates_and_full() {
        let full = sample_pivot_indices(10, 10, 1);
        assert_eq!(full.len(), 10);
        let mut sorted = full.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..10).collect::<Vec<_>>());

        let half = sample_pivot_indices(10, 6, 42);
        assert_eq!(half.len(), 6);
        let mut uniq = half.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), 6);
    }

    #[test]
    fn sampled_correlates_with_exact_on_small_graph() {
        let (view, _) = line_graph(40);
        let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
        let sampled = SampledBetweenness::compute_flat(&index, 16, 7);
        let exact_map = BetweennessCentrality::compute_unbounded(&view, &[EdgeType::Calls]);
        let mut exact = vec![0.0; index.node_count];
        for (idx, uuid) in view.index_uuid_iter() {
            exact[idx.index()] = exact_map.get(&uuid).copied().unwrap_or(0.0);
        }

        let corr = spearman_correlation(&sampled, &exact);
        assert!(corr > 0.7, "sampled/exact rank correlation too low: {corr}");
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
