//! Community detection
//!
//! CSR queue-based Fast-Leiden (local moving + refinement + aggregation) with
//! modularity scoring, statistical hub stripping, and deterministic
//! importance-weighted tie-breaking.

use crate::centrality::{FastPageRank, FlatGraphIndex};
use crate::graph_utils::PetGraphView;
use petgraph::graph::NodeIndex;
use rgctl_error::Result;
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_graph::snapshot::SnapshotNodeStore;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

/// Default behavioral edge types for community detection.
///
/// Includes [`EdgeType::References`] so markdown/doc graphs (link structure) form
/// communities alongside code [`Calls`] / [`Uses`].
pub fn default_community_edge_types() -> &'static [EdgeType] {
    &[EdgeType::Calls, EdgeType::Uses, EdgeType::References]
}

/// Edge types for community detection on a loaded graph (adds `Contains` when no functions).
pub fn community_edge_types_for_backend(backend: &MemoryBackend) -> Vec<EdgeType> {
    let mut types = default_community_edge_types().to_vec();
    let mut functions = 0usize;
    let _ = backend.for_each_node(|node| {
        if node.node_type == NodeType::Function {
            functions += 1;
        }
    });
    if functions == 0 {
        types.push(EdgeType::Contains);
    }
    types
}

/// Default σ multiplier for statistical hub stripping (`μ + kσ`).
pub const DEFAULT_HUB_SIGMA_K: f64 = 4.0;

/// Never freeze more than this fraction of nodes as infrastructure hubs.
pub const DEFAULT_MAX_FROZEN_FRACTION: f64 = 0.05;

/// Skip hub stripping on graphs smaller than this.
pub const DEFAULT_MIN_NODES_FOR_HUB_STRIP: usize = 20;

/// Policy for identifying high-degree utility hubs before Fast-Leiden local moving.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HubStripPolicy {
    /// No hub stripping.
    Off,
    /// Freeze nodes with degree > μ + kσ on the community edge projection.
    Statistical {
        /// Standard-deviation multiplier for the hub cutoff.
        k: f64,
    },
    /// Freeze the top `p` fraction of nodes by degree (0.0–1.0).
    Percentile(f64),
}

/// Tie-breaking strategy when neighbor label vote counts are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TieBreakStrategy {
    /// Lowest label id wins (legacy behavior).
    LabelId,
    /// Highest neighbor importance score wins; PageRank computed when not supplied.
    Importance,
}

/// Community detection engine.
pub struct CommunityDetector {
    max_iterations: usize,
    hub_policy: HubStripPolicy,
    tie_break: TieBreakStrategy,
    max_frozen_fraction: f64,
    min_nodes_for_hub_strip: usize,
}

/// A detected community.
#[derive(Debug, Clone, PartialEq)]
pub struct Community {
    /// Community identifier
    pub id: usize,
    /// Node UUIDs in this community
    pub members: Vec<Uuid>,
}

/// Result of community detection.
#[derive(Debug, Clone)]
pub struct CommunityResult {
    /// Detected communities
    pub communities: Vec<Community>,
    /// Modularity score (higher is better, typically 0.3-0.7)
    pub modularity: f64,
    /// Node UUID to community ID mapping
    pub assignments: HashMap<Uuid, usize>,
    /// Community id assigned to stripped infrastructure hubs, if any.
    pub infrastructure_community_id: Option<usize>,
}

impl Default for CommunityDetector {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            hub_policy: HubStripPolicy::Statistical {
                k: DEFAULT_HUB_SIGMA_K,
            },
            tie_break: TieBreakStrategy::Importance,
            max_frozen_fraction: DEFAULT_MAX_FROZEN_FRACTION,
            min_nodes_for_hub_strip: DEFAULT_MIN_NODES_FOR_HUB_STRIP,
        }
    }
}

impl CommunityDetector {
    /// Create a new community detector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override hub-stripping policy.
    pub fn with_hub_policy(mut self, policy: HubStripPolicy) -> Self {
        self.hub_policy = policy;
        self
    }

    /// Override tie-breaking strategy.
    pub fn with_tie_break(mut self, strategy: TieBreakStrategy) -> Self {
        self.tie_break = strategy;
        self
    }

    /// Cap the fraction of nodes that may be frozen as infrastructure hubs.
    pub fn with_max_frozen_fraction(mut self, fraction: f64) -> Self {
        self.max_frozen_fraction = fraction;
        self
    }

    /// Minimum graph size before hub stripping activates.
    pub fn with_min_nodes_for_hub_strip(mut self, min_nodes: usize) -> Self {
        self.min_nodes_for_hub_strip = min_nodes;
        self
    }

    /// Override local-moving iteration budget (scaled by node count in the queue pass).
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Detect communities using doc-aware CSR projection (zero `Vec<Vec<_>>` allocation).
    pub fn detect_with_view_defaults(
        &self,
        view: &PetGraphView,
        store: &SnapshotNodeStore,
    ) -> Result<CommunityResult> {
        let (offsets, targets) = build_community_undirected_csr(view, store)?;
        let allowed = default_community_edge_types();
        self.detect_with_undirected_csr(view, &offsets, &targets, allowed, None)
    }

    /// Detect communities using [`default_community_edge_types`].
    ///
    /// Accepts a pre-built PetGraphView to avoid rebuilding the topology.
    pub fn detect_with_view(&self, view: &PetGraphView) -> Result<CommunityResult> {
        self.detect_with_view_filtered(view, default_community_edge_types())
    }

    /// Detect communities with explicit edge-type isolation.
    pub fn detect_with_view_filtered(
        &self,
        view: &PetGraphView,
        allowed_types: &[EdgeType],
    ) -> Result<CommunityResult> {
        self.detect_with_view_scored(view, allowed_types, None)
    }

    /// Detect communities with optional precomputed importance scores (e.g. PageRank).
    pub fn detect_with_view_scored(
        &self,
        view: &PetGraphView,
        allowed_types: &[EdgeType],
        importance: Option<&HashMap<Uuid, f64>>,
    ) -> Result<CommunityResult> {
        let (offsets, targets) = view.topo.undirected_csr_filtered(allowed_types, &[]);
        self.detect_with_undirected_csr(view, &offsets, &targets, allowed_types, importance)
    }

    /// Detect communities with pre-built undirected neighbor lists.
    ///
    /// Runs CSR queue-based Fast-Leiden (local moving → refinement → aggregation),
    /// with hub stripping and deterministic importance / label-id tie-breaks.
    pub fn detect_with_neighbor_lists(
        &self,
        view: &PetGraphView,
        neighbors: Vec<Vec<usize>>,
        allowed_types: &[EdgeType],
        importance: Option<&HashMap<Uuid, f64>>,
    ) -> Result<CommunityResult> {
        let mut offsets = Vec::with_capacity(neighbors.len() + 1);
        offsets.push(0u32);
        let total: usize = neighbors.iter().map(Vec::len).sum();
        let mut targets = Vec::with_capacity(total);
        for adj in &neighbors {
            for &v in adj {
                targets.push(v as u32);
            }
            offsets.push(targets.len() as u32);
        }
        drop(neighbors);
        self.detect_with_undirected_csr(view, &offsets, &targets, allowed_types, importance)
    }

    fn detect_with_undirected_csr(
        &self,
        view: &PetGraphView,
        offsets: &[u32],
        targets: &[u32],
        allowed_types: &[EdgeType],
        importance: Option<&HashMap<Uuid, f64>>,
    ) -> Result<CommunityResult> {
        let node_count = view.node_count();
        if node_count == 0 {
            return Ok(empty_community_result());
        }

        let degrees: Vec<usize> = (0..node_count)
            .map(|u| (offsets[u + 1] - offsets[u]) as usize)
            .collect();
        let is_hub = select_hubs(
            &degrees,
            self.hub_policy,
            self.max_frozen_fraction,
            self.min_nodes_for_hub_strip,
        );
        // Prefer caller scores; otherwise use projection degree as importance
        // (avoids a second full-graph PageRank during discover — centrality runs next).
        let importance_flat = resolve_importance_flat(
            view,
            allowed_types,
            self.tie_break,
            importance,
            Some(&degrees),
        );

        let mut labels = fast_leiden_csr(
            offsets,
            targets,
            &is_hub,
            &importance_flat,
            self.tie_break,
            self.max_iterations,
        );

        let modularity = self.calculate_modularity_flat(view, &labels, allowed_types);

        let infrastructure_community_id =
            assign_infrastructure_hubs(&mut labels, &is_hub, node_count);

        let mut assignments: HashMap<Uuid, usize> = HashMap::with_capacity(node_count);
        for (idx, uuid) in view.index_uuid_iter() {
            assignments.insert(uuid, labels[idx.index()]);
        }

        // Build member lists with flat indexing (no HashMap<usize, Vec<Uuid>>).
        // On kernel-scale graphs (>200k nodes), skip singleton isolate Vecs.
        let max_label = labels.iter().copied().max().unwrap_or(0);
        let comm_slots = max_label + 1;
        let communities = if node_count <= 200_000 {
            let mut buckets: Vec<Vec<Uuid>> = vec![Vec::new(); comm_slots];
            for (idx, uuid) in view.index_uuid_iter() {
                buckets[labels[idx.index()]].push(uuid);
            }
            buckets
                .into_iter()
                .enumerate()
                .filter(|(_, members)| !members.is_empty())
                .map(|(id, members)| Community { id, members })
                .collect()
        } else {
            let mut counts = vec![0u32; comm_slots];
            for &l in &labels {
                counts[l] += 1;
            }
            let mut buckets: HashMap<usize, Vec<Uuid>> = HashMap::new();
            for (idx, uuid) in view.index_uuid_iter() {
                let cid = labels[idx.index()];
                if counts[cid] > 1 || infrastructure_community_id == Some(cid) {
                    buckets
                        .entry(cid)
                        .or_insert_with(|| Vec::with_capacity(counts[cid] as usize))
                        .push(uuid);
                }
            }
            buckets
                .into_iter()
                .map(|(id, members)| Community { id, members })
                .collect()
        };

        Ok(CommunityResult {
            communities,
            modularity,
            assignments,
            infrastructure_community_id,
        })
    }

    /// Detect communities (Fast-Leiden over the default edge projection).
    ///
    /// Builds a PetGraphView internally. For better performance when running
    /// multiple analyses, build the view once and use `detect_with_view()`.
    pub fn detect(&self, backend: &MemoryBackend) -> Result<CommunityResult> {
        let view = PetGraphView::from_backend(backend)?;
        self.detect_with_view(&view)
    }

    /// Calculate modularity for a partition on a behavioral edge projection.
    pub fn calculate_modularity(
        &self,
        view: &PetGraphView,
        labels: &HashMap<NodeIndex, usize>,
        allowed_types: &[EdgeType],
    ) -> f64 {
        let mut flat = vec![0usize; view.node_count()];
        for (&idx, &label) in labels {
            flat[idx.index()] = label;
        }
        self.calculate_modularity_flat(view, &flat, allowed_types)
    }

    /// Modularity from a dense label slice (view index → community id).
    pub fn calculate_modularity_flat(
        &self,
        view: &PetGraphView,
        node_community: &[usize],
        allowed_types: &[EdgeType],
    ) -> f64 {
        let node_count = view.node_count();
        if node_community.len() != node_count {
            return 0.0;
        }
        let mut m = 0.0;
        let mut degrees = vec![0.0; node_count];
        let max_label = node_community.iter().copied().max().unwrap_or(0);
        let comm_count = max_label + 1;
        let mut internal_by_community = vec![0.0f64; comm_count];
        let mut degree_sum_by_community = vec![0.0f64; comm_count];

        let mut mask = [false; 256];
        for &ty in allowed_types {
            mask[rgctl_graph::csr::edge_type_to_u8(ty) as usize] = true;
        }
        for s in 0..node_count {
            let (targets, types) = view.topo.csr.out_neighbors(s as u32);
            for (i, &dst) in targets.iter().enumerate() {
                if !mask[types[i] as usize] {
                    continue;
                }
                let t = dst as usize;
                m += 1.0;
                degrees[s] += 1.0;
                degrees[t] += 1.0;
                if node_community[s] == node_community[t] {
                    let c = node_community[s];
                    if c < internal_by_community.len() {
                        internal_by_community[c] += 1.0;
                    }
                }
            }
        }

        if m == 0.0 {
            return 0.0;
        }

        for (i, &label) in node_community.iter().enumerate() {
            if label < degree_sum_by_community.len() {
                degree_sum_by_community[label] += degrees[i];
            }
        }

        let mut q = 0.0;
        for (community, &degree_sum) in degree_sum_by_community.iter().enumerate() {
            if degree_sum == 0.0 {
                continue;
            }
            let internal = internal_by_community.get(community).copied().unwrap_or(0.0);
            let expected = (degree_sum * degree_sum) / (4.0 * m);
            q += (internal / m) - (expected / m);
        }
        q
    }
}

fn empty_community_result() -> CommunityResult {
    CommunityResult {
        communities: vec![],
        modularity: 0.0,
        assignments: HashMap::new(),
        infrastructure_community_id: None,
    }
}

/// Select infrastructure hub nodes on the undirected community projection.
fn select_hubs(
    degrees: &[usize],
    policy: HubStripPolicy,
    max_frozen_fraction: f64,
    min_nodes: usize,
) -> Vec<bool> {
    let node_count = degrees.len();
    let mut is_hub = vec![false; node_count];

    if matches!(policy, HubStripPolicy::Off) || node_count < min_nodes {
        return is_hub;
    }

    let participating: Vec<usize> = degrees
        .iter()
        .enumerate()
        .filter(|(_, d)| **d > 0)
        .map(|(i, _)| i)
        .collect();

    if participating.is_empty() {
        return is_hub;
    }

    let max_freeze = ((node_count as f64 * max_frozen_fraction).ceil() as usize).max(1);

    let mut candidates: Vec<usize> = match policy {
        HubStripPolicy::Off => return is_hub,
        HubStripPolicy::Percentile(p) => {
            let mut ranked = participating;
            ranked.sort_by_key(|&i| std::cmp::Reverse(degrees[i]));
            let take = ((node_count as f64 * p).ceil() as usize).max(1);
            ranked.truncate(take);
            ranked
        }
        HubStripPolicy::Statistical { k } => {
            let values: Vec<f64> = participating.iter().map(|&i| degrees[i] as f64).collect();
            let n = values.len() as f64;
            let mu = values.iter().sum::<f64>() / n;
            let variance = values.iter().map(|v| (v - mu).powi(2)).sum::<f64>() / n;
            let sigma = variance.sqrt();
            let threshold = mu + k * sigma;

            let mut selected: Vec<usize> = participating
                .iter()
                .copied()
                .filter(|&i| degrees[i] as f64 > threshold)
                .collect();

            if selected.is_empty()
                && node_count >= 10
                && let Some((idx, _)) = degrees.iter().enumerate().max_by_key(|(_, d)| *d)
                && degrees[idx] as f64 > mu
            {
                selected.push(idx);
            }
            selected
        }
    };

    if candidates.len() > max_freeze {
        candidates.sort_by_key(|&i| std::cmp::Reverse(degrees[i]));
        candidates.truncate(max_freeze);
    }

    for i in candidates {
        is_hub[i] = true;
    }
    is_hub
}

fn resolve_importance_flat(
    view: &PetGraphView,
    allowed_types: &[EdgeType],
    tie_break: TieBreakStrategy,
    importance: Option<&HashMap<Uuid, f64>>,
    degrees: Option<&[usize]>,
) -> Vec<f64> {
    let node_count = view.node_count();
    if tie_break == TieBreakStrategy::LabelId {
        return vec![0.0; node_count];
    }

    if let Some(map) = importance {
        let mut flat = vec![0.0; node_count];
        for (idx, uuid) in view.index_uuid_iter() {
            if let Some(score) = map.get(&uuid) {
                flat[idx.index()] = *score;
            }
        }
        return flat;
    }

    if let Some(deg) = degrees
        && deg.len() == node_count
    {
        return deg.iter().map(|&d| d as f64).collect();
    }

    // Fallback when degrees unavailable: flat PageRank (no Uuid HashMap).
    let _ = allowed_types;
    let index = FlatGraphIndex::from_view(view, allowed_types);
    FastPageRank::new(20, 0.85).compute_flat_only(&index)
}

#[cfg(test)]
fn neighbor_importance_for_label(
    label: usize,
    u: usize,
    neighbors: &[Vec<usize>],
    labels: &[usize],
    importance: &[f64],
    is_hub: &[bool],
) -> f64 {
    neighbors[u]
        .iter()
        .filter(|&&v| !is_hub[v] && labels[v] == label)
        .map(|&v| importance[v])
        .fold(0.0_f64, f64::max)
}

fn assign_infrastructure_hubs(
    labels: &mut [usize],
    is_hub: &[bool],
    node_count: usize,
) -> Option<usize> {
    let hub_count = is_hub.iter().filter(|&&h| h).count();
    if hub_count == 0 {
        return None;
    }
    let raw_infra_id = node_count.saturating_mul(2).saturating_add(1);
    let mut first_hub = None;
    for (u, frozen) in is_hub.iter().enumerate() {
        if *frozen {
            labels[u] = raw_infra_id;
            if first_hub.is_none() {
                first_hub = Some(u);
            }
        }
    }
    renumber_labels(labels);
    first_hub.map(|u| labels[u])
}

/// Weighted undirected CSR for multi-level Leiden (preserves Σₜₒₜ / 2m).
struct WeightedCsr {
    offsets: Vec<u32>,
    targets: Vec<u32>,
    weights: Vec<f64>,
    /// Node strength (sum of incident edge weights); equals degree on level 0.
    node_weight: Vec<f64>,
}

impl WeightedCsr {
    fn len(&self) -> usize {
        self.node_weight.len()
    }

    fn two_m(&self) -> f64 {
        self.node_weight.iter().sum()
    }

    fn degree(&self, u: usize) -> usize {
        (self.offsets[u + 1] - self.offsets[u]) as usize
    }

    fn neighbors(&self, u: usize) -> impl Iterator<Item = (u32, f64)> + '_ {
        let start = self.offsets[u] as usize;
        let end = self.offsets[u + 1] as usize;
        self.targets[start..end]
            .iter()
            .copied()
            .zip(self.weights[start..end].iter().copied())
    }
}

/// CSR queue-based Fast-Leiden over a flat undirected CSR `(offsets, targets)`.
///
/// Hubs are frozen (singleton communities, never moved). Tie-breaks use
/// [`TieBreakStrategy`] when modularity gains are equal within `f64::EPSILON`.
fn fast_leiden_csr(
    offsets: &[u32],
    targets: &[u32],
    is_hub: &[bool],
    importance: &[f64],
    tie_break: TieBreakStrategy,
    max_iterations: usize,
) -> Vec<usize> {
    let n = offsets.len().saturating_sub(1);
    if n == 0 {
        return Vec::new();
    }

    // Compact to degree > 0 nodes — isolates stay fixed singletons (linux: >60%).
    let mut active: Vec<usize> = Vec::with_capacity(n);
    let mut old_to_active = vec![usize::MAX; n];
    for u in 0..n {
        if offsets[u] < offsets[u + 1] {
            old_to_active[u] = active.len();
            active.push(u);
        }
    }
    if active.is_empty() {
        let mut labels: Vec<usize> = (0..n).collect();
        renumber_labels(&mut labels);
        return labels;
    }

    // Build level-0 WeightedCsr directly over active nodes (zero `Vec<Vec<usize>>`).
    let active_len = active.len();
    let mut c_offsets = Vec::with_capacity(active_len + 1);
    c_offsets.push(0u32);
    let mut c_targets = Vec::with_capacity(targets.len());
    let mut c_node_weight = vec![0.0f64; active_len];
    for (ai, &u) in active.iter().enumerate() {
        let s = offsets[u] as usize;
        let e = offsets[u + 1] as usize;
        for &v in &targets[s..e] {
            let av = old_to_active[v as usize];
            if av != usize::MAX {
                c_targets.push(av as u32);
                c_node_weight[ai] += 1.0;
            }
        }
        c_offsets.push(c_targets.len() as u32);
    }
    let c_weights = vec![1.0f64; c_targets.len()];
    let level0_csr = WeightedCsr {
        offsets: c_offsets,
        targets: c_targets,
        weights: c_weights,
        node_weight: c_node_weight,
    };

    let compact_hubs: Vec<bool> = active.iter().map(|&u| is_hub[u]).collect();
    let compact_imp: Vec<f64> = active.iter().map(|&u| importance[u]).collect();

    let compact_labels = fast_leiden_active_csr(
        level0_csr,
        &compact_hubs,
        &compact_imp,
        tie_break,
        max_iterations,
    );

    let max_c = compact_labels.iter().copied().max().unwrap_or(0);
    let isolate_base = max_c.saturating_add(1);
    let mut labels = vec![0usize; n];
    for u in 0..n {
        let a = old_to_active[u];
        if a == usize::MAX {
            labels[u] = isolate_base + u;
        } else {
            labels[u] = compact_labels[a];
        }
    }
    for (u, &hub) in is_hub.iter().enumerate() {
        if hub {
            labels[u] = isolate_base + n + u;
        }
    }
    renumber_labels(&mut labels);
    labels
}

fn fast_leiden_active_csr(
    mut csr: WeightedCsr,
    is_hub: &[bool],
    importance: &[f64],
    tie_break: TieBreakStrategy,
    max_iterations: usize,
) -> Vec<usize> {
    let n = csr.len();
    let mut membership: Vec<usize> = (0..n).collect();
    let mut working_hubs = is_hub.to_vec();
    let mut working_importance = importance.to_vec();
    let mut orig_comm: Vec<usize> = (0..n).collect();

    const MAX_LEVELS: usize = 4;
    for level in 0..MAX_LEVELS {
        let two_m = csr.two_m();
        if two_m <= 0.0 {
            break;
        }

        leiden_local_moving(
            &csr,
            &mut membership,
            &working_hubs,
            &working_importance,
            tie_break,
            two_m,
            max_iterations,
        );
        leiden_refine(&csr, &mut membership, &working_hubs);

        let prev_n = csr.len();
        let aggregated = aggregate_communities_weighted(
            &csr,
            &mut membership,
            &working_hubs,
            &working_importance,
        );

        if level == 0 {
            orig_comm.clone_from(&membership);
        } else {
            for c in &mut orig_comm {
                *c = membership[*c];
            }
        }

        let Some((next_csr, next_hubs, next_imp)) = aggregated else {
            break;
        };
        if next_csr.len() >= prev_n {
            break;
        }

        csr = next_csr;
        working_hubs = next_hubs;
        working_importance = next_imp;
        membership = (0..csr.len()).collect();
    }

    orig_comm
}

fn leiden_local_moving(
    csr: &WeightedCsr,
    membership: &mut [usize],
    is_hub: &[bool],
    importance: &[f64],
    tie_break: TieBreakStrategy,
    two_m: f64,
    max_iterations: usize,
) {
    let n = csr.len();
    let m = two_m / 2.0;
    if m <= 0.0 || n == 0 {
        return;
    }

    // Community ids stay in 0..n after renumber; grow buffer if refine expands ids.
    let mut capacity = n.saturating_mul(2).max(n + 1);
    let mut community_total = vec![0.0f64; capacity];
    for (u, &c) in membership.iter().enumerate() {
        if c >= capacity {
            capacity = c + 1;
            community_total.resize(capacity, 0.0);
        }
        community_total[c] += csr.node_weight[u];
    }

    let mut stamp = vec![0u32; capacity];
    let mut comm_weight = vec![0.0f64; capacity];
    let mut comm_imp = vec![0.0f64; capacity];
    let mut touched: Vec<usize> = Vec::with_capacity(64);
    let mut epoch = 1u32;

    let mut queue: VecDeque<usize> = VecDeque::with_capacity(n);
    let mut in_queue = vec![false; n];
    for u in 0..n {
        // Degree-0 isolates cannot move; skip enqueue (and multi-level coarsening).
        if !is_hub[u] && csr.degree(u) > 0 {
            queue.push_back(u);
            in_queue[u] = true;
        }
    }

    let mut passes = 0usize;
    let pass_limit = max_iterations.saturating_mul(n.max(1));
    while let Some(u) = queue.pop_front() {
        in_queue[u] = false;
        passes += 1;
        if passes > pass_limit {
            break;
        }
        if is_hub[u] {
            continue;
        }

        let from = membership[u];
        let ki = csr.node_weight[u];

        // Single O(deg(u)) accumulation into scratch.
        epoch = epoch.wrapping_add(1);
        if epoch == 0 {
            stamp.fill(0);
            epoch = 1;
        }
        touched.clear();
        for (v, w) in csr.neighbors(u) {
            let v = v as usize;
            if is_hub[v] {
                continue;
            }
            let c = membership[v];
            if c >= capacity {
                capacity = c + 1;
                stamp.resize(capacity, 0);
                comm_weight.resize(capacity, 0.0);
                comm_imp.resize(capacity, 0.0);
                community_total.resize(capacity, 0.0);
            }
            if stamp[c] != epoch {
                stamp[c] = epoch;
                comm_weight[c] = 0.0;
                comm_imp[c] = 0.0;
                touched.push(c);
            }
            comm_weight[c] += w;
            let imp = importance.get(v).copied().unwrap_or(0.0);
            if imp > comm_imp[c] {
                comm_imp[c] = imp;
            }
        }
        // Ensure current community is evaluated even with no neighbor there.
        if stamp[from] != epoch {
            stamp[from] = epoch;
            comm_weight[from] = 0.0;
            comm_imp[from] = 0.0;
            touched.push(from);
        }

        let mut best = from;
        let mut best_gain = 0.0f64;
        let mut best_imp = comm_imp[from];
        let sigma_from = community_total[from] - ki;
        let ki_from = comm_weight[from];

        for &to in &touched {
            if to == from {
                continue;
            }
            let ki_to = comm_weight[to];
            let sigma_to = community_total[to];
            let leave = (ki_from / m) - (sigma_from * ki) / (2.0 * m * m);
            let join = (ki_to / m) - (sigma_to * ki) / (2.0 * m * m);
            let gain = join - leave;
            let imp = comm_imp[to];
            let better = gain > best_gain + f64::EPSILON
                || ((gain - best_gain).abs() <= f64::EPSILON
                    && tie_break == TieBreakStrategy::Importance
                    && imp > best_imp + f64::EPSILON)
                || ((gain - best_gain).abs() <= f64::EPSILON
                    && (tie_break != TieBreakStrategy::Importance
                        || (imp - best_imp).abs() <= f64::EPSILON)
                    && to < best);
            if better {
                best_gain = gain;
                best_imp = imp;
                best = to;
            }
        }

        // Clear scratch for touched communities (O(|touched|)).
        for &c in &touched {
            comm_weight[c] = 0.0;
            comm_imp[c] = 0.0;
        }

        if best != from && best_gain > f64::EPSILON {
            community_total[from] -= ki;
            community_total[best] += ki;
            membership[u] = best;
            for (v, _) in csr.neighbors(u) {
                let v = v as usize;
                if !is_hub[v] && !in_queue[v] {
                    queue.push_back(v);
                    in_queue[v] = true;
                }
            }
        }
    }
}

/// Split communities that are not connected on the projection (Leiden refine).
/// Deterministic: communities processed in ascending id order; flat stamp/visited.
fn leiden_refine(csr: &WeightedCsr, membership: &mut [usize], is_hub: &[bool]) {
    let n = csr.len();
    if n == 0 {
        return;
    }
    let max_c = membership.iter().copied().max().unwrap_or(0);
    let comm_count = max_c + 1;

    // Counting sort: group nodes by community id (deterministic).
    let mut counts = vec![0usize; comm_count];
    for (u, &c) in membership.iter().enumerate() {
        if !is_hub[u] {
            counts[c] += 1;
        }
    }
    let mut offsets = vec![0usize; comm_count + 1];
    for i in 0..comm_count {
        offsets[i + 1] = offsets[i] + counts[i];
    }
    let mut cursor = offsets.clone();
    let mut members_flat = vec![0usize; offsets[comm_count]];
    for (u, &c) in membership.iter().enumerate() {
        if is_hub[u] {
            continue;
        }
        let slot = cursor[c];
        members_flat[slot] = u;
        cursor[c] += 1;
    }

    let mut visited = vec![false; n];
    let mut next_id = comm_count;
    let mut stack = Vec::with_capacity(64);

    for c in 0..comm_count {
        let start = offsets[c];
        let end = offsets[c + 1];
        if end - start <= 1 {
            continue;
        }
        // Reset visited only for this community's members.
        for &u in &members_flat[start..end] {
            visited[u] = false;
        }
        let mut first = true;
        for &seed in &members_flat[start..end] {
            if visited[seed] {
                continue;
            }
            stack.clear();
            stack.push(seed);
            visited[seed] = true;
            let mut component = vec![seed];
            while let Some(u) = stack.pop() {
                for (v, _) in csr.neighbors(u) {
                    let v = v as usize;
                    if membership[v] == c && !visited[v] {
                        visited[v] = true;
                        stack.push(v);
                        component.push(v);
                    }
                }
            }
            if first {
                first = false;
                continue;
            }
            let id = next_id;
            next_id += 1;
            for u in component {
                membership[u] = id;
            }
        }
    }
}

/// Aggregate into a weighted coarse CSR. Rewrites `membership` to dense `0..C`.
fn aggregate_communities_weighted(
    csr: &WeightedCsr,
    membership: &mut [usize],
    is_hub: &[bool],
    importance: &[f64],
) -> Option<(WeightedCsr, Vec<bool>, Vec<f64>)> {
    let n = csr.len();
    if n == 0 {
        return None;
    }
    let max_m = membership.iter().copied().max().unwrap_or(0);
    let mut present = vec![false; max_m + 1];
    for &c in membership.iter() {
        present[c] = true;
    }
    let mut remap = vec![usize::MAX; max_m + 1];
    let mut c_count = 0usize;
    for (c, &is_used) in present.iter().enumerate() {
        if is_used {
            remap[c] = c_count;
            c_count += 1;
        }
    }
    if c_count >= n || c_count == 0 {
        return None;
    }
    for c in membership.iter_mut() {
        *c = remap[*c];
    }

    let mut coarse_hubs = vec![false; c_count];
    let mut coarse_imp = vec![0.0f64; c_count];
    let mut node_weight = vec![0.0f64; c_count];
    for (u, &cu) in membership.iter().enumerate() {
        node_weight[cu] += csr.node_weight[u];
        if is_hub[u] {
            coarse_hubs[cu] = true;
        }
        let imp = importance.get(u).copied().unwrap_or(0.0);
        if imp > coarse_imp[cu] {
            coarse_imp[cu] = imp;
        }
    }

    // Build undirected weighted edges between distinct communities.
    // Intra-community edges (cu == cv) are already accounted for in `node_weight[cu]`
    // (preserving Σₜₒₜ and 2m) and must NOT be emitted as self-loops in `neighbors(cu)`,
    // otherwise `leiden_local_moving` would count `cu`'s internal edges in `ki_from`.
    let mut edge_map: HashMap<(u32, u32), f64> = HashMap::new();
    for u in 0..n {
        let cu = membership[u] as u32;
        for (v, w) in csr.neighbors(u) {
            let cv = membership[v as usize] as u32;
            if cu != cv && (u as u32) < v {
                let a = cu.min(cv);
                let b = cu.max(cv);
                *edge_map.entry((a, b)).or_insert(0.0) += w;
            }
        }
    }

    // Expand to adjacency lists (both directions).
    let mut adj: Vec<Vec<(u32, f64)>> = vec![Vec::new(); c_count];
    for ((a, b), w) in edge_map {
        adj[a as usize].push((b, w));
        adj[b as usize].push((a, w));
    }
    for list in &mut adj {
        list.sort_unstable_by_key(|&(t, _)| t);
    }

    let mut offsets = Vec::with_capacity(c_count + 1);
    let mut targets = Vec::new();
    let mut weights = Vec::new();
    offsets.push(0);
    for list in &adj {
        for &(t, w) in list {
            targets.push(t);
            weights.push(w);
        }
        offsets.push(targets.len() as u32);
    }

    Some((
        WeightedCsr {
            offsets,
            targets,
            weights,
            node_weight,
        },
        coarse_hubs,
        coarse_imp,
    ))
}

fn renumber_labels(labels: &mut [usize]) {
    let Some(max_l) = labels.iter().copied().max() else {
        return;
    };
    if max_l <= labels.len().saturating_mul(4) {
        let mut remap = vec![usize::MAX; max_l + 1];
        let mut next = 0usize;
        for label in labels.iter_mut() {
            let slot = &mut remap[*label];
            if *slot == usize::MAX {
                *slot = next;
                next += 1;
            }
            *label = *slot;
        }
        return;
    }
    let mut map = HashMap::with_capacity(labels.len());
    let mut next = 0usize;
    for label in labels.iter_mut() {
        let e = map.entry(*label).or_insert_with(|| {
            let id = next;
            next += 1;
            id
        });
        *label = *e;
    }
}

/// Pre-build undirected neighbor lists for a filtered behavioral projection.
fn build_filtered_neighbor_lists(
    view: &PetGraphView,
    allowed_types: &[EdgeType],
) -> Vec<Vec<usize>> {
    view.topo.undirected_filtered_neighbors(allowed_types)
}

/// Assert every non-infrastructure community with `|C| > 1` induces a single
/// connected component on the filtered undirected projection.
pub fn verify_community_connectedness(
    view: &PetGraphView,
    result: &CommunityResult,
    allowed_types: &[EdgeType],
) -> Result<()> {
    let neighbors = build_filtered_neighbor_lists(view, allowed_types);
    let infra = result.infrastructure_community_id;
    for community in &result.communities {
        if community.members.len() <= 1 {
            continue;
        }
        if infra == Some(community.id) {
            continue;
        }
        let idxs: HashSet<usize> = community
            .members
            .iter()
            .filter_map(|id| view.get_index(*id).map(|n| n.index()))
            .collect();
        if idxs.is_empty() {
            continue;
        }
        let start = *idxs.iter().next().expect("non-empty idxs");
        let mut seen = HashSet::from([start]);
        let mut stack = vec![start];
        while let Some(u) = stack.pop() {
            for &v in &neighbors[u] {
                if idxs.contains(&v) && seen.insert(v) {
                    stack.push(v);
                }
            }
        }
        if seen.len() != idxs.len() {
            return Err(rgctl_error::Error::Other(format!(
                "community {} induces {} connected components (size {})",
                community.id,
                idxs.len().saturating_sub(seen.len()) + 1,
                community.members.len()
            )));
        }
    }
    Ok(())
}

/// Build a flat deduplicated undirected CSR `(offsets, targets)` for community detection:
/// behavioral edges plus markdown-heading `Contains` edges, with zero `Vec<Vec<usize>>` allocation.
pub fn build_community_undirected_csr(
    view: &PetGraphView,
    store: &SnapshotNodeStore,
) -> Result<(Vec<u32>, Vec<u32>)> {
    let base = default_community_edge_types();
    let n = view.node_count();
    if n == 0 {
        return Ok((vec![0], Vec::new()));
    }
    let mut is_heading = vec![false; n];
    let mut heading_indices: Vec<u32> = Vec::new();

    // Only materialize Module rows for markdown `kind=heading` — never `get_node` every UUID.
    store.for_each_node_of_type(NodeType::Module, |node| {
        if node.get_property("kind") == Some("heading")
            && let Some(view_idx) = view.get_index(node.id)
        {
            let pos = view_idx.index();
            if !is_heading[pos] {
                is_heading[pos] = true;
                heading_indices.push(pos as u32);
            }
        }
    })?;

    if heading_indices.is_empty() {
        return Ok(view.topo.undirected_csr_filtered(base, &[]));
    }

    let mut extra_pairs: Vec<(u32, u32)> = Vec::new();
    for &u in &heading_indices {
        for &v in view.topo.csr.out_neighbors_of_type(u, EdgeType::Contains) {
            if is_heading[v as usize] {
                extra_pairs.push((u, v));
            }
        }
    }
    Ok(view.topo.undirected_csr_filtered(base, &extra_pairs))
}

/// Community neighbor projection: behavioral edges plus scoped `Contains`.
pub fn build_community_neighbor_lists(
    view: &PetGraphView,
    store: &SnapshotNodeStore,
) -> Result<Vec<Vec<usize>>> {
    let (offsets, targets) = build_community_undirected_csr(view, store)?;
    let n = view.node_count();
    let mut neighbors = Vec::with_capacity(n);
    for u in 0..n {
        let s = offsets[u] as usize;
        let e = offsets[u + 1] as usize;
        neighbors.push(targets[s..e].iter().map(|&v| v as usize).collect());
    }
    Ok(neighbors)
}

/// Dashboard community with inferred metadata (Phase 14 A+).
#[derive(Debug, Clone, Serialize)]
pub struct DashboardCommunity {
    /// Community identifier
    pub id: usize,
    /// Member node IDs
    pub nodes: Vec<Uuid>,
    /// Member count
    pub size: usize,
    /// Most common node type in the cluster
    pub primary_type: NodeType,
    /// Average cyclomatic complexity
    pub avg_complexity: f64,
    /// Human-readable label (e.g. "auth cluster")
    pub label: String,
}

/// Detect communities for the analytics dashboard.
///
/// Uses Fast-Leiden (via [`CommunityDetector`]) and enriches each cluster with
/// labels and complexity metadata. Falls back to connected components when
/// detection yields a single cluster on disconnected subgraphs.
pub fn detect_communities(backend: &MemoryBackend) -> Result<Vec<DashboardCommunity>> {
    let view = PetGraphView::from_backend(backend)?;
    let detection = CommunityDetector::new()
        .detect_with_view_filtered(&view, default_community_edge_types())?;
    let infra_id = detection.infrastructure_community_id;

    let mut communities: Vec<DashboardCommunity> = detection
        .communities
        .into_iter()
        .filter(|c| c.members.len() >= 2 || infra_id == Some(c.id))
        .map(|c| build_dashboard_community(c.id, &c.members, backend, infra_id))
        .collect::<Result<_>>()?;

    if communities.len() < 2 {
        let components = connected_components(backend)?;
        if components.len() > communities.len() {
            communities = components
                .into_iter()
                .enumerate()
                .filter(|(_, members)| members.len() >= 2)
                .map(|(idx, members)| build_dashboard_community(idx, &members, backend, None))
                .collect::<Result<_>>()?;
        }
    }

    communities.sort_by_key(|b| std::cmp::Reverse(b.size));
    Ok(communities)
}

fn build_dashboard_community(
    id: usize,
    member_ids: &[Uuid],
    backend: &MemoryBackend,
    infrastructure_community_id: Option<usize>,
) -> Result<DashboardCommunity> {
    // Collect minimal data from each member node (zero-copy scoped access)
    let mut type_counts: HashMap<NodeType, usize> = HashMap::new();
    let mut complexity_sum = 0.0;
    let mut complexity_count = 0;
    let mut file_paths: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();

    for &member_id in member_ids {
        backend.with_node(member_id, |node| {
            *type_counts.entry(node.node_type).or_insert(0) += 1;

            if let Some(complexity_str) = node.get_property("cyclomatic")
                && let Ok(complexity) = complexity_str.parse::<i64>()
            {
                complexity_sum += complexity as f64;
                complexity_count += 1;
            }

            if let Some(path) = &node.file_path {
                file_paths.push(path.to_string());
            }
            names.push(node.name.to_string());
        })?;
    }

    let primary_type = type_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(t, _)| t)
        .unwrap_or(NodeType::Function);

    let avg_complexity = if complexity_count > 0 {
        complexity_sum / complexity_count as f64
    } else {
        0.0
    };

    let label = {
        use crate::community_label::{CommunityLabelHints, infer_community_label};
        let packages: Vec<String> = file_paths
            .iter()
            .map(|p| crate::migration::package_label(p))
            .collect();
        infer_community_label(&CommunityLabelHints {
            id,
            names: &names,
            package_labels: &packages,
            top_pagerank_name: None,
            is_infrastructure: infrastructure_community_id == Some(id),
        })
    };

    Ok(DashboardCommunity {
        id,
        nodes: member_ids.to_vec(),
        size: member_ids.len(),
        primary_type,
        avg_complexity,
        label,
    })
}

fn connected_components(backend: &MemoryBackend) -> Result<Vec<Vec<Uuid>>> {
    // Build adjacency list with zero-copy edge iteration
    let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    backend.for_each_edge(|edge| {
        adj.entry(edge.from).or_default().push(edge.to);
        adj.entry(edge.to).or_default().push(edge.from);
    })?;

    let mut visited = HashSet::new();
    let mut components = Vec::new();

    // Get all node IDs (only copies UUIDs, not full nodes)
    let node_ids = backend.all_node_ids()?;

    for node_id in node_ids {
        if visited.contains(&node_id) {
            continue;
        }
        let mut stack = vec![node_id];
        let mut component = Vec::new();
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            component.push(current);
            if let Some(neighbors) = adj.get(&current) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
        }
        if !component.is_empty() {
            components.push(component);
        }
    }
    Ok(components)
}

// Zero-copy helper removed — path/token naming lives in `community_label`.

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::{Edge, Node};

    fn build_modular_graph() -> MemoryBackend {
        let mut backend = MemoryBackend::new();
        let auth1 = Node::new(NodeType::Function, "login".to_string());
        let auth2 = Node::new(NodeType::Function, "logout".to_string());
        let api1 = Node::new(NodeType::Function, "get_user".to_string());
        let api2 = Node::new(NodeType::Function, "create_user".to_string());
        let ui1 = Node::new(NodeType::Function, "render".to_string());

        let ids: Vec<_> = [&auth1, &auth2, &api1, &api2, &ui1]
            .iter()
            .map(|n| {
                let id = n.id;
                backend.insert_node((*n).clone()).unwrap();
                id
            })
            .collect();

        backend
            .insert_edge(Edge::new(ids[0], ids[1], EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(ids[2], ids[3], EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(ids[0], ids[2], EdgeType::Uses))
            .unwrap();
        backend
            .insert_edge(Edge::new(ids[4], ids[0], EdgeType::Calls))
            .unwrap();
        backend
    }

    #[test]
    fn test_community_detection() {
        let backend = build_modular_graph();
        let detector = CommunityDetector::new();
        let result = detector.detect(&backend).unwrap();
        assert!(!result.communities.is_empty());
        assert!(result.modularity >= 0.0);
    }

    #[test]
    fn test_deterministic_tie_break() {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let c = Node::new(NodeType::Function, "c");
        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_node(c).unwrap();
        backend
            .insert_edge(Edge::new(id_a, id_c, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_b, id_c, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let detector = CommunityDetector::new();
        let first = detector
            .detect_with_view_filtered(&view, &[EdgeType::Calls])
            .unwrap();
        for _ in 0..100 {
            let again = detector
                .detect_with_view_filtered(&view, &[EdgeType::Calls])
                .unwrap();
            assert_eq!(first.assignments, again.assignments);
        }
    }

    #[test]
    fn test_hub_strip_splits_cliques_around_star_hub() {
        let mut backend = MemoryBackend::new();
        let mut clique_a = Vec::new();
        let mut clique_b = Vec::new();
        for i in 0..5 {
            let a = Node::new(NodeType::Function, format!("auth_{i}"));
            let b = Node::new(NodeType::Function, format!("bill_{i}"));
            clique_a.push(a.id);
            clique_b.push(b.id);
            backend.insert_node(a).unwrap();
            backend.insert_node(b).unwrap();
        }
        let hub = Node::new(NodeType::Function, "shared_db");
        let id_hub = hub.id;
        backend.insert_node(hub).unwrap();

        for ids in [&clique_a, &clique_b] {
            for i in 0..ids.len() {
                for j in 0..ids.len() {
                    if i != j {
                        backend
                            .insert_edge(Edge::new(ids[i], ids[j], EdgeType::Calls))
                            .unwrap();
                    }
                }
            }
            for &id in ids {
                backend
                    .insert_edge(Edge::new(id, id_hub, EdgeType::Calls))
                    .unwrap();
                backend
                    .insert_edge(Edge::new(id_hub, id, EdgeType::Calls))
                    .unwrap();
            }
        }

        let view = PetGraphView::from_backend(&backend).unwrap();
        let without = CommunityDetector::new()
            .with_hub_policy(HubStripPolicy::Off)
            .detect_with_view_filtered(&view, &[EdgeType::Calls])
            .unwrap();
        let with = CommunityDetector::new()
            .with_min_nodes_for_hub_strip(5)
            .detect_with_view_filtered(&view, &[EdgeType::Calls])
            .unwrap();

        let domain_labels: HashSet<_> = clique_a
            .iter()
            .chain(clique_b.iter())
            .map(|id| with.assignments[id])
            .collect();
        assert!(
            with.infrastructure_community_id.is_some(),
            "expected infrastructure community for star hub"
        );
        assert_eq!(
            *with.assignments.get(&id_hub).unwrap(),
            with.infrastructure_community_id.unwrap()
        );
        assert_eq!(
            domain_labels.len(),
            2,
            "hub strip should preserve two domain communities, got {domain_labels:?}"
        );
        // Fast-Leiden often keeps the two cliques separate even without strip; the
        // hub-strip contract is isolating the hub into an infrastructure community.
        assert!(
            without.infrastructure_community_id.is_none(),
            "HubStripPolicy::Off must not invent an infrastructure community"
        );
        let hub_label = *without.assignments.get(&id_hub).unwrap();
        let hub_shares_domain = clique_a
            .iter()
            .chain(clique_b.iter())
            .any(|id| without.assignments[id] == hub_label);
        assert!(
            hub_shares_domain,
            "without hub strip, the star hub should share a community with domain nodes"
        );
    }

    #[test]
    fn test_importance_tie_break_beats_label_id() {
        let neighbors = vec![vec![1, 2], vec![0], vec![0]];
        let labels = vec![0, 1, 2];
        let importance = vec![0.0, 0.1, 0.9];
        let is_hub = vec![false; 3];

        let left_imp =
            neighbor_importance_for_label(1, 0, &neighbors, &labels, &importance, &is_hub);
        let right_imp =
            neighbor_importance_for_label(2, 0, &neighbors, &labels, &importance, &is_hub);
        assert!(right_imp > left_imp);

        let mut best_label = labels[0];
        let mut max_count = 0usize;
        let mut best_importance = 0.0_f64;
        let seen_labels = [1usize, 2usize];
        let label_weights = [0, 1, 1];

        for &label in &seen_labels {
            let count = label_weights[label];
            let label_importance =
                neighbor_importance_for_label(label, 0, &neighbors, &labels, &importance, &is_hub);
            let wins = count > max_count
                || (count == max_count && label_importance > best_importance)
                || (count == max_count
                    && (label_importance - best_importance).abs() < f64::EPSILON
                    && label < best_label);
            if wins {
                max_count = count;
                best_importance = label_importance;
                best_label = label;
            }
        }
        assert_eq!(best_label, 2, "importance should select the right label");

        let mut best_label_id = labels[0];
        let mut max_count_id = 0usize;
        for &label in &seen_labels {
            let count = label_weights[label];
            if count > max_count_id || (count == max_count_id && label < best_label_id) {
                max_count_id = count;
                best_label_id = label;
            }
        }
        assert_eq!(best_label_id, 1, "label-id should select the lowest label");
        assert_ne!(best_label, best_label_id);
    }

    #[test]
    fn references_only_graph_forms_communities() {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Module, "doc_a")
            .with_file_path("docs/a.md")
            .with_property("kind".to_string(), "heading".to_string());
        let b = Node::new(NodeType::Module, "doc_b")
            .with_file_path("docs/b.md")
            .with_property("kind".to_string(), "heading".to_string());
        let c = Node::new(NodeType::Module, "doc_c")
            .with_file_path("docs/b.md")
            .with_property("kind".to_string(), "heading".to_string());
        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_node(c).unwrap();
        backend
            .insert_edge(Edge::new(id_a, id_b, EdgeType::References))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_b, id_c, EdgeType::References))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let detector = CommunityDetector::new();
        let result = detector
            .detect_with_view_filtered(&view, default_community_edge_types())
            .unwrap();
        assert!(!result.communities.is_empty());
        assert!(result.modularity >= 0.0);
    }

    #[test]
    fn mixed_code_and_docs_graph_forms_communities() {
        let mut backend = MemoryBackend::new();
        let f_checkout = Node::new(NodeType::Function, "checkout");
        let f_publish = Node::new(NodeType::Function, "publishEvent");
        let h_checkout = Node::new(NodeType::Module, "Checkout Flow")
            .with_file_path("docs/guide.md")
            .with_property("kind".to_string(), "heading".to_string());
        let h_payments = Node::new(NodeType::Module, "Payments")
            .with_file_path("docs/adr.md")
            .with_property("kind".to_string(), "heading".to_string());

        let id_checkout = f_checkout.id;
        let id_publish = f_publish.id;
        let id_h_checkout = h_checkout.id;
        let id_h_payments = h_payments.id;

        backend.insert_node(f_checkout).unwrap();
        backend.insert_node(f_publish).unwrap();
        backend.insert_node(h_checkout).unwrap();
        backend.insert_node(h_payments).unwrap();

        backend
            .insert_edge(Edge::new(id_checkout, id_publish, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(
                id_h_checkout,
                id_h_payments,
                EdgeType::References,
            ))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_h_checkout, id_h_payments, EdgeType::Contains))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let detector = CommunityDetector::new();
        let result = detector
            .detect_with_view_filtered(&view, default_community_edge_types())
            .unwrap();

        assert!(!result.communities.is_empty());
        assert_eq!(result.assignments.len(), 4);
        assert!(result.assignments.contains_key(&id_checkout));
        assert!(result.assignments.contains_key(&id_publish));
        assert!(result.assignments.contains_key(&id_h_checkout));
        assert!(result.assignments.contains_key(&id_h_payments));
    }

    #[test]
    fn test_statistical_hub_threshold_respects_sigma() {
        let degrees = vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 50];
        let hubs = select_hubs(&degrees, HubStripPolicy::Statistical { k: 2.0 }, 0.05, 5);
        assert!(hubs[9]);
        assert_eq!(hubs.iter().filter(|&&h| h).count(), 1);
    }

    #[test]
    fn leiden_communities_are_connected_on_projection() {
        let backend = build_modular_graph();
        let view = PetGraphView::from_backend(&backend).unwrap();
        let allowed = default_community_edge_types();
        let neighbors = build_filtered_neighbor_lists(&view, allowed);
        let result = CommunityDetector::new()
            .with_hub_policy(HubStripPolicy::Off)
            .detect_with_view_filtered(&view, allowed)
            .unwrap();

        for community in &result.communities {
            if community.members.len() <= 1 {
                continue;
            }
            let idxs: HashSet<usize> = community
                .members
                .iter()
                .filter_map(|id| view.get_index(*id).map(|n| n.index()))
                .collect();
            let start = *idxs.iter().next().unwrap();
            let mut seen = HashSet::from([start]);
            let mut stack = vec![start];
            while let Some(u) = stack.pop() {
                for &v in &neighbors[u] {
                    if idxs.contains(&v) && seen.insert(v) {
                        stack.push(v);
                    }
                }
            }
            assert_eq!(
                seen.len(),
                idxs.len(),
                "community {} must induce a connected subgraph",
                community.id
            );
        }
    }
}
