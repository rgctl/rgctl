//! Community detection
//!
//! Label-propagation community detection with modularity scoring.
//!
//! Uses dense `Vec` layouts, directional edge-type filters, hub stripping on
//! high-degree utility nodes, and deterministic importance-weighted tie-breaking.

use crate::centrality::FastPageRank;
use crate::graph_utils::PetGraphView;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Default behavioral edge types for community detection.
pub fn default_community_edge_types() -> &'static [EdgeType] {
    &[EdgeType::Calls, EdgeType::Uses]
}

/// Default σ multiplier for statistical hub stripping (`μ + kσ`).
pub const DEFAULT_HUB_SIGMA_K: f64 = 2.0;

/// Never freeze more than this fraction of nodes as infrastructure hubs.
pub const DEFAULT_MAX_FROZEN_FRACTION: f64 = 0.05;

/// Skip hub stripping on graphs smaller than this.
pub const DEFAULT_MIN_NODES_FOR_HUB_STRIP: usize = 20;

/// Policy for identifying high-degree utility hubs before label propagation.
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

    /// Override label-propagation iteration cap.
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Detect communities using behavioral edge types (Calls + Uses).
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
        let node_count = view.directed.node_count();
        if node_count == 0 {
            return Ok(empty_community_result());
        }

        let neighbors = build_filtered_neighbor_lists(view, allowed_types);
        let degrees: Vec<usize> = neighbors.iter().map(|n| n.len()).collect();
        let is_hub = select_hubs(
            &degrees,
            self.hub_policy,
            self.max_frozen_fraction,
            self.min_nodes_for_hub_strip,
        );
        let importance_flat =
            resolve_importance_flat(view, allowed_types, self.tie_break, importance);

        let mut labels: Vec<usize> = (0..node_count).collect();
        let mut label_weights = vec![0_usize; node_count];
        let mut seen_labels = Vec::with_capacity(64);

        for _ in 0..self.max_iterations {
            let mut changed = false;

            for node_idx in view.directed.node_indices() {
                let u = node_idx.index();
                if is_hub[u] {
                    continue;
                }

                seen_labels.clear();

                for &v in &neighbors[u] {
                    if is_hub[v] {
                        continue;
                    }
                    let label = labels[v];
                    if label_weights[label] == 0 {
                        seen_labels.push(label);
                    }
                    label_weights[label] += 1;
                }

                let mut best_label = labels[u];
                let mut max_count = 0usize;
                let mut best_importance = 0.0_f64;

                for &label in &seen_labels {
                    let count = label_weights[label];
                    let label_importance = neighbor_importance_for_label(
                        label,
                        u,
                        &neighbors,
                        &labels,
                        &importance_flat,
                        &is_hub,
                    );
                    let wins = count > max_count
                        || (count == max_count
                            && self.tie_break == TieBreakStrategy::Importance
                            && label_importance > best_importance)
                        || (count == max_count
                            && (self.tie_break != TieBreakStrategy::Importance
                                || (label_importance - best_importance).abs() < f64::EPSILON)
                            && label < best_label);
                    if wins {
                        max_count = count;
                        best_importance = label_importance;
                        best_label = label;
                    }
                }

                for &label in &seen_labels {
                    label_weights[label] = 0;
                }

                if labels[u] != best_label {
                    labels[u] = best_label;
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        let label_map: HashMap<NodeIndex, usize> = view
            .directed
            .node_indices()
            .map(|idx| (idx, labels[idx.index()]))
            .collect();

        let modularity = self.calculate_modularity(view, &label_map, allowed_types);

        let infrastructure_community_id = assign_infrastructure_hubs(
            &mut labels,
            &is_hub,
            node_count,
        );

        let mut community_members: HashMap<usize, Vec<Uuid>> = HashMap::new();
        for (idx, label) in view.directed.node_indices().map(|i| (i, labels[i.index()])) {
            if let Some(uuid) = view.index_to_uuid.get(&idx) {
                community_members.entry(label).or_default().push(*uuid);
            }
        }

        let communities = community_members
            .into_iter()
            .map(|(id, members)| Community { id, members })
            .collect();

        let assignments = view
            .directed
            .node_indices()
            .filter_map(|idx| {
                view.index_to_uuid
                    .get(&idx)
                    .map(|uuid| (*uuid, labels[idx.index()]))
            })
            .collect();

        Ok(CommunityResult {
            communities,
            modularity,
            assignments,
            infrastructure_community_id,
        })
    }

    /// Detect communities using label propagation (Leiden-like heuristic).
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
        let node_count = view.directed.node_count();
        let mut m = 0.0;
        let mut degrees = vec![0.0; node_count];
        let mut node_community = vec![0usize; node_count];

        for (&idx, &label) in labels {
            node_community[idx.index()] = label;
        }

        let mut internal_by_community: HashMap<usize, f64> = HashMap::new();

        for edge in view.directed.edge_references() {
            if !allowed_types.contains(edge.weight()) {
                continue;
            }
            m += 1.0;
            let s = edge.source().index();
            let t = edge.target().index();
            degrees[s] += 1.0;
            degrees[t] += 1.0;
            if node_community[s] == node_community[t] {
                *internal_by_community.entry(node_community[s]).or_default() += 1.0;
            }
        }

        if m == 0.0 {
            return 0.0;
        }

        let mut degree_sum_by_community: HashMap<usize, f64> = HashMap::new();
        for (&idx, &label) in labels {
            *degree_sum_by_community.entry(label).or_default() += degrees[idx.index()];
        }

        let mut q = 0.0;
        for (&community, &degree_sum) in &degree_sum_by_community {
            let internal = internal_by_community
                .get(&community)
                .copied()
                .unwrap_or(0.0);
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

            if selected.is_empty() && node_count >= 10 {
                if let Some((idx, _)) = degrees
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, d)| *d)
                {
                    if degrees[idx] as f64 > mu {
                        selected.push(idx);
                    }
                }
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
) -> Vec<f64> {
    let node_count = view.directed.node_count();
    if tie_break == TieBreakStrategy::LabelId {
        return vec![0.0; node_count];
    }

    let score_map: HashMap<Uuid, f64> = if let Some(map) = importance {
        map.clone()
    } else {
        FastPageRank::new(20, 0.85)
            .compute(view, allowed_types)
            .0
    };

    let mut flat = vec![0.0; node_count];
    for (idx, uuid) in &view.index_to_uuid {
        if let Some(score) = score_map.get(uuid) {
            flat[idx.index()] = *score;
        }
    }
    flat
}

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
    let infrastructure_id = node_count;
    for (u, frozen) in is_hub.iter().enumerate() {
        if *frozen {
            labels[u] = infrastructure_id;
        }
    }
    Some(infrastructure_id)
}

/// Pre-build undirected neighbor lists for a filtered behavioral projection.
fn build_filtered_neighbor_lists(
    view: &PetGraphView,
    allowed_types: &[EdgeType],
) -> Vec<Vec<usize>> {
    let node_count = view.directed.node_count();
    let mut neighbors = vec![Vec::new(); node_count];

    for edge in view.directed.edge_references() {
        if allowed_types.contains(edge.weight()) {
            let s = edge.source().index();
            let t = edge.target().index();
            neighbors[s].push(t);
            if s != t {
                neighbors[t].push(s);
            }
        }
    }

    neighbors
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
/// Uses label propagation (via [`CommunityDetector`]) and enriches each cluster
/// with labels and complexity metadata. Falls back to connected components when
/// propagation yields a single cluster on disconnected subgraphs.
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

            if let Some(complexity_str) = node.get_property("cyclomatic") {
                if let Ok(complexity) = complexity_str.parse::<i64>() {
                    complexity_sum += complexity as f64;
                    complexity_count += 1;
                }
            }

            if let Some(path) = &node.file_path {
                file_paths.push(path.clone());
            }
            names.push(node.name.clone());
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

    let label = if infrastructure_community_id == Some(id) {
        "Infrastructure / Common Library".to_string()
    } else if let Some(common) = find_common_path_prefix_strings(&file_paths) {
        if !common.is_empty() {
            common
        } else {
            infer_label_from_names(&names, id)
        }
    } else {
        infer_label_from_names(&names, id)
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

#[allow(dead_code)]
fn most_common_type(nodes: &[&rbuilder_graph::schema::Node]) -> NodeType {
    let mut counts = HashMap::new();
    for node in nodes {
        *counts.entry(node.node_type).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(t, _)| t)
        .unwrap_or(NodeType::Function)
}

#[allow(dead_code)]
fn avg_complexity(nodes: &[&rbuilder_graph::schema::Node]) -> f64 {
    if nodes.is_empty() {
        return 0.0;
    }
    let sum: f64 = nodes.iter().map(|n| node_complexity(n) as f64).sum();
    sum / nodes.len() as f64
}

#[allow(dead_code)]
fn node_complexity(node: &rbuilder_graph::schema::Node) -> i64 {
    node.get_property("cyclomatic")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
}

#[allow(dead_code)]
fn infer_community_label(nodes: &[&rbuilder_graph::schema::Node], idx: usize) -> String {
    let paths: Vec<_> = nodes.iter().filter_map(|n| n.file_path.as_ref()).collect();

    if let Some(common) = find_common_path_prefix(&paths) {
        if !common.is_empty() {
            return common;
        }
    }

    let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
    if names
        .iter()
        .any(|n| n.contains("auth") || n.contains("Auth"))
    {
        return "auth cluster".into();
    }
    if names.iter().any(|n| n.contains("api") || n.contains("Api")) {
        return "API layer".into();
    }
    if names
        .iter()
        .any(|n| n.contains("db") || n.contains("database") || n.contains("query"))
    {
        return "database layer".into();
    }

    format!("cluster_{idx}")
}

#[allow(dead_code)]
fn find_common_path_prefix(paths: &[&String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let first = paths[0].as_str();
    let mut prefix_len = first.len();
    for path in &paths[1..] {
        prefix_len = first
            .chars()
            .zip(path.chars())
            .take(prefix_len)
            .take_while(|(a, b)| a == b)
            .count();
    }
    if prefix_len == 0 {
        return None;
    }
    if let Some(last_slash) = first[..prefix_len].rfind('/') {
        return Some(first[..last_slash].to_string());
    }
    Some(first[..prefix_len].to_string())
}

// Zero-copy helper: work with String references instead of Node references
fn find_common_path_prefix_strings(paths: &[String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let first = &paths[0];
    let mut prefix_len = first.len();
    for path in &paths[1..] {
        prefix_len = first
            .chars()
            .zip(path.chars())
            .take(prefix_len)
            .take_while(|(a, b)| a == b)
            .count();
    }
    if prefix_len == 0 {
        return None;
    }
    if let Some(last_slash) = first[..prefix_len].rfind('/') {
        return Some(first[..last_slash].to_string());
    }
    Some(first[..prefix_len].to_string())
}

fn infer_label_from_names(names: &[String], idx: usize) -> String {
    if names.is_empty() {
        return format!("Community {}", idx + 1);
    }

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for name in names {
        let tokens: Vec<&str> = name.split(&['_', '-', '.'][..]).collect();
        for token in tokens {
            if !token.is_empty() && token.len() > 2 {
                *counts.entry(token).or_insert(0) += 1;
            }
        }
    }

    if let Some((token, _)) = counts.iter().max_by_key(|(_, count)| *count) {
        if counts[token] >= names.len() / 3 {
            return token.to_string();
        }
    }

    format!("Community {}", idx + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, Node};

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
        let a = Node::new(NodeType::Function, "a".into());
        let b = Node::new(NodeType::Function, "b".into());
        let c = Node::new(NodeType::Function, "c".into());
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
        let hub = Node::new(NodeType::Function, "shared_db".into());
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
        let without_domains: HashSet<_> = clique_a
            .iter()
            .chain(clique_b.iter())
            .map(|id| without.assignments[id])
            .collect();
        assert!(
            without_domains.len() < domain_labels.len(),
            "without hub strip, shared hub should collapse domains"
        );
    }

    #[test]
    fn test_importance_tie_break_beats_label_id() {
        let neighbors = vec![
            vec![1, 2],
            vec![0],
            vec![0],
        ];
        let labels = vec![0, 1, 2];
        let importance = vec![0.0, 0.1, 0.9];
        let is_hub = vec![false; 3];

        let left_imp = neighbor_importance_for_label(1, 0, &neighbors, &labels, &importance, &is_hub);
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
    fn test_statistical_hub_threshold_respects_sigma() {
        let degrees = vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 50];
        let hubs = select_hubs(
            &degrees,
            HubStripPolicy::Statistical { k: 2.0 },
            0.05,
            5,
        );
        assert!(hubs[9]);
        assert_eq!(hubs.iter().filter(|&&h| h).count(), 1);
    }
}
