//! Package-level migration graph and scheduling planner.
//!
//! Builds a macro graph of packages/modules connected by call edges, aggregates
//! centrality and blast-radius metrics, and produces a dependency-aware roadmap using
//! Kahn topological scheduling with priority tie-breaking.

use crate::results::AnalysisResults;
use rgctl_graph::graph_source::GraphSource;
use rgctl_graph::schema::{EdgeType, NodeType};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// JSON schema version for [`MigrationGraphPayload`].
///
/// v3: package macro nodes serialize Fast-Leiden majority vote as `community_id`
/// (was `louvain_community_id` in v2). Deserialization still accepts the old key via
/// [`serde`] alias so existing `migration_graph.json` sidecars keep cluster colors.
pub const MIGRATION_GRAPH_SCHEMA_VERSION: u32 = 3;
/// JSON schema version for [`MigrationPlanPayload`].
pub const MIGRATION_PLAN_SCHEMA_VERSION: u32 = 2;

/// Cap macro nodes (matches dashboard metagraph); tail merges into `(other)`.
pub const MAX_MIGRATION_MACRO_NODES: usize = 256;

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

    /// Stable string name for JSON export (`scheduled` or `priority`).
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

    /// Human-readable label for a preset id (falls back to Hybrid Default).
    pub fn preset_label(preset: &str) -> &'static str {
        match preset {
            "foundational_first" => "Foundational First",
            "dense_cluster" => "Dense Cluster Extraction",
            "risk_mitigation" => "Risk Mitigation",
            _ => "Hybrid Default",
        }
    }
}

/// Per-package aggregated metrics for the migration macro graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationCommunityNode {
    /// Stable macro node id (0..n-1).
    pub id: usize,
    /// Package / module label derived from member file paths.
    pub label: String,
    /// Number of indexed functions in this package.
    pub member_count: u32,
    /// Mean PageRank of member functions.
    pub avg_pagerank: f64,
    /// Mean harmonic centrality of member functions.
    pub avg_harmonic: f64,
    /// Mean betweenness centrality of member functions.
    pub avg_betweenness: f64,
    /// Maximum blast radius among member functions.
    pub max_blast: f64,
    /// Majority Fast-Leiden community of member functions (for layout clustering).
    ///
    /// Serialized as `community_id` (schema v3+). Older sidecars used
    /// `louvain_community_id`; that key is still accepted on read.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "louvain_community_id"
    )]
    pub community_id: Option<usize>,
}

/// Directed inter-package call edge (caller macro node → callee macro node).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationCommunityEdge {
    /// Caller package macro node id.
    pub source: usize,
    /// Callee package macro node id.
    pub target: usize,
    /// Aggregated call count between the two packages.
    pub weight: u32,
    /// Edge kind (currently always `calls`).
    pub kind: String,
}

/// Package macro graph exported to the dashboard bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationGraphPayload {
    /// Payload schema version.
    pub schema_version: u32,
    /// `package_macro` — one node per package/module (not raw Fast-Leiden community).
    pub mode: String,
    /// Modularity of the underlying Fast-Leiden partition.
    pub modularity: f64,
    /// Macro nodes (packages/modules).
    pub communities: Vec<MigrationCommunityNode>,
    /// Inter-package call edges.
    pub edges: Vec<MigrationCommunityEdge>,
}

/// One package in a migration roadmap (both schedule and priority rank).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationPlanStep {
    /// Display row number in the current `order_mode` sort (1..n).
    pub step: u32,
    /// Macro node id (package).
    pub community_id: usize,
    /// Package / module label.
    pub label: String,
    /// Weighted multi-objective priority score.
    pub priority_score: f64,
    /// Position in dependency-aware topological schedule (1..n).
    pub schedule_step: u32,
    /// Position when sorted by priority score descending (1..n).
    pub priority_rank: u32,
    /// Mean PageRank of member functions.
    pub avg_pagerank: f64,
    /// Mean harmonic centrality of member functions.
    pub avg_harmonic: f64,
    /// Maximum blast radius among member functions.
    pub max_blast: f64,
}

/// Full migration plan (CLI / dashboard export).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationPlanPayload {
    /// Payload schema version.
    pub schema_version: u32,
    /// Preset id (e.g. `hybrid_default`).
    pub preset: String,
    /// Human-readable preset name.
    pub preset_label: String,
    /// α/β/γ weights used for scoring.
    pub weights: MigrationWeights,
    /// Controls `steps` row order: `scheduled` or `priority`.
    pub order_mode: String,
    /// Ordered roadmap rows.
    pub steps: Vec<MigrationPlanStep>,
}

#[derive(Default)]
struct MacroAgg {
    label: String,
    member_count: u32,
    pagerank_sum: f64,
    harmonic_sum: f64,
    betweenness_sum: f64,
    max_blast: f64,
    community_votes: HashMap<usize, u32>,
}

/// Derive a stable, host-independent package / module label from a source file path.
///
/// - Matches the innermost JVM source root (`src/main/java/`, `src/test/java/`, `src/main/kotlin/`,
///   `src/test/kotlin/`, `src/main/scala/`, `src/test/scala/`, `java/`) via `rfind`.
/// - Matches the innermost `src/` directory via `rfind` and qualifies it with the owning
///   workspace crate/package (e.g. `crates/<pkg>/src/...` -> `<pkg>` or `<pkg>.<subdir>`),
///   preventing both empty-parent fallthrough on `src/lib.rs` and cross-crate collisions on
///   shared subdirectory names.
/// - Anchors non-`src/` paths at workspace/repo markers and never emits leading dots or
///   host filesystem prefixes (`/Users/...`, `/home/...`).
///
/// When `file_path` is already `/`-normalized (no backslashes), this avoids intermediate
/// `String` / `Vec` allocations beyond the returned label.
pub fn package_label(file_path: &str) -> String {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return "root".into();
    }
    if trimmed.as_bytes().contains(&b'\\') {
        let norm = trimmed.replace('\\', "/");
        return package_label_normalized(&norm);
    }
    package_label_normalized(trimmed)
}

/// Cache of [`package_label`] results keyed by columnar `file_path_off` (mmap) or path string
/// (in-memory graphs where `file_path_off` is always 0).
#[derive(Debug, Default)]
pub struct PackageLabelCache {
    by_off: HashMap<u32, String>,
    by_path: HashMap<String, String>,
}

impl PackageLabelCache {
    /// Return a cached package label for `(file_path_off, file_path)`.
    pub fn get(&mut self, file_path_off: u32, file_path: &str) -> &str {
        if file_path_off != 0 {
            return self
                .by_off
                .entry(file_path_off)
                .or_insert_with(|| package_label(file_path));
        }
        if !self.by_path.contains_key(file_path) {
            let label = package_label(file_path);
            self.by_path.insert(file_path.to_string(), label);
        }
        self.by_path
            .get(file_path)
            .map(|s| s.as_str())
            .unwrap_or("root")
    }

    /// Owned clone of the cached label (for HashMap keys / bucket inserts).
    pub fn get_owned(&mut self, file_path_off: u32, file_path: &str) -> String {
        self.get(file_path_off, file_path).to_string()
    }
}

fn package_label_normalized(norm: &str) -> String {
    const JVM_MARKERS: &[&str] = &[
        "/src/main/java/",
        "/src/test/java/",
        "/src/main/kotlin/",
        "/src/test/kotlin/",
        "/src/main/scala/",
        "/src/test/scala/",
        "/java/",
    ];
    const WORKSPACE_CONTAINERS: &[&str] = &[
        "crates", "packages", "services", "apps", "modules", "libs", "plugins",
    ];
    const SUBPROJECT_DIRS: &[&str] = &[
        "dashboard",
        "website",
        "web",
        "frontend",
        "backend",
        "client",
        "server",
        "ui",
        "cli",
    ];
    const REPO_ANCHORS: &[&str] = &[
        "cmd",
        "internal",
        "pkg",
        "lib",
        "tests",
        "benches",
        "examples",
        "docs",
        "content",
        "dashboard",
        "website",
        "scripts",
        "openspec",
    ];
    const OS_ROOT_SEGMENTS: &[&str] = &[
        "Users", "home", "var", "tmp", "opt", "private", "etc", "usr", "Volumes", "mnt",
    ];

    if norm.is_empty() {
        return "root".into();
    }

    let is_windows_abs =
        norm.len() >= 3 && norm.as_bytes()[1] == b':' && norm.as_bytes()[2] == b'/';
    let is_abs = norm.starts_with('/') || is_windows_abs;

    let clean = if is_windows_abs {
        norm[3..].trim_start_matches('/')
    } else {
        norm.trim_start_matches('/')
    };
    if clean.is_empty() {
        return "root".into();
    }

    // Prefer borrowing when `norm` is already `/{clean}` (absolute Unix path).
    let slash_owned;
    let slash_path: &str = if norm.starts_with('/') && norm.trim_start_matches('/') == clean {
        norm
    } else {
        slash_owned = format!("/{clean}");
        slash_owned.as_str()
    };

    // 1. Innermost JVM / Kotlin / Scala source root
    let mut best_jvm_end: Option<usize> = None;
    for marker in JVM_MARKERS {
        if let Some(idx) = slash_path.rfind(marker) {
            let end = idx + marker.len();
            if best_jvm_end.is_none_or(|prev| end > prev) {
                best_jvm_end = Some(end);
            }
        }
    }
    if let Some(end) = best_jvm_end {
        let parent = str_parent(&slash_path[end..]);
        if parent.is_empty() {
            return "default".into();
        }
        return join_dot_segments(parent);
    }

    // 2. Innermost `/src/` directory with crate / package qualification
    if let Some(idx) = slash_path.rfind("/src/") {
        let prefix = &slash_path[..idx];
        let after = &slash_path[idx + 5..];
        let sub_parent = str_parent(after);
        let owner = owner_from_src_prefix(prefix, is_abs, WORKSPACE_CONTAINERS, SUBPROJECT_DIRS);

        return match (owner, sub_parent.is_empty()) {
            (Some(pkg), true) => pkg.to_string(),
            (Some(pkg), false) => {
                let mut out = String::with_capacity(pkg.len() + 1 + sub_parent.len());
                out.push_str(pkg);
                out.push('.');
                push_dot_segments(&mut out, sub_parent);
                out
            }
            (None, true) => "src".into(),
            (None, false) => {
                let mut out = String::with_capacity(4 + sub_parent.len());
                out.push_str("src.");
                push_dot_segments(&mut out, sub_parent);
                out
            }
        };
    }

    // 3. Non-`src/` path resolution
    let parent = str_parent(clean);
    if parent.is_empty() {
        return "root".into();
    }

    if let Some(rest) = after_innermost_container(parent, WORKSPACE_CONTAINERS) {
        return join_dot_segments(rest);
    }

    if let Some(from_anchor) = from_innermost_anchor(parent, REPO_ANCHORS) {
        return join_dot_segments(from_anchor);
    }

    if !is_abs {
        return join_dot_segments(parent);
    }

    // Last two non-OS-root segments (small path → stack buffer, else fall back to Vec).
    let mut buf: [&str; 32] = [""; 32];
    let mut n = 0usize;
    for seg in parent.split('/').filter(|s| !s.is_empty()) {
        if OS_ROOT_SEGMENTS.contains(&seg) {
            continue;
        }
        if n < buf.len() {
            buf[n] = seg;
            n += 1;
        } else {
            // Rare deep paths: collect and take last two.
            let filtered: Vec<&str> = parent
                .split('/')
                .filter(|s| !s.is_empty() && !OS_ROOT_SEGMENTS.contains(s))
                .collect();
            if filtered.is_empty() {
                return "root".into();
            }
            let start = filtered.len().saturating_sub(2);
            return filtered[start..].join(".");
        }
    }
    if n == 0 {
        return "root".into();
    }
    let start = n.saturating_sub(2);
    buf[start..n].join(".")
}

fn str_parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

fn join_dot_segments(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    push_dot_segments(&mut out, path);
    out
}

fn push_dot_segments(out: &mut String, path: &str) {
    let mut first = true;
    for seg in path.split('/').filter(|s| !s.is_empty()) {
        if !first {
            out.push('.');
        }
        out.push_str(seg);
        first = false;
    }
}

fn innermost_after_container<'a>(prefix: &'a str, containers: &[&str]) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    let mut expect_pkg = false;
    for seg in prefix.split('/').filter(|s| !s.is_empty()) {
        if containers.contains(&seg) {
            expect_pkg = true;
            continue;
        }
        if expect_pkg {
            best = Some(seg);
            expect_pkg = false;
        }
    }
    best
}

fn owner_from_src_prefix<'a>(
    prefix: &'a str,
    is_abs: bool,
    containers: &[&str],
    subprojects: &[&str],
) -> Option<&'a str> {
    if let Some(pkg) = innermost_after_container(prefix, containers) {
        return Some(pkg);
    }
    let last = prefix.split('/').rfind(|s| !s.is_empty())?;
    if subprojects.contains(&last) || !is_abs {
        Some(last)
    } else {
        None
    }
}

fn after_innermost_container<'a>(parent: &'a str, containers: &[&str]) -> Option<&'a str> {
    let mut last_container_end: Option<usize> = None;
    let mut offset = 0usize;
    for seg in parent.split('/') {
        if seg.is_empty() {
            if offset < parent.len() {
                offset += 1;
            }
            continue;
        }
        let end = offset + seg.len();
        if containers.contains(&seg) {
            last_container_end = Some(end);
        }
        offset = end + 1;
    }
    let end = last_container_end?;
    if end >= parent.len() {
        return None;
    }
    let rest = parent[end..].trim_start_matches('/');
    if rest.is_empty() { None } else { Some(rest) }
}

fn from_innermost_anchor<'a>(parent: &'a str, anchors: &[&str]) -> Option<&'a str> {
    let mut best_start: Option<usize> = None;
    let mut offset = 0usize;
    for seg in parent.split('/') {
        if seg.is_empty() {
            if offset < parent.len() {
                offset += 1;
            }
            continue;
        }
        let start = offset;
        let end = start + seg.len();
        if anchors.contains(&seg) {
            best_start = Some(start);
        }
        offset = end + 1;
    }
    best_start.map(|s| &parent[s..])
}

/// Build the package-level migration macro graph from indexed analysis results.
///
/// Accepts [`GraphSource`] (or anything convertible: `&MemoryBackend`, `&MmappedGraphSnapshot`)
/// so discover can pass an mmap snapshot without hydrating a `MemoryBackend`.
///
/// Package labels are cached per unique `file_path_off` / path; edge aggregation uses a dense
/// `node_to_pkg: Vec<u32>` indexed by [`HotNodeRef::idx`].
pub fn build_migration_graph<'a>(
    graph: impl Into<GraphSource<'a>>,
    results: &AnalysisResults,
) -> Option<MigrationGraphPayload> {
    let graph = graph.into();
    let (payload, _) = build_migration_graph_with_index(graph, results)?;
    Some(payload)
}

/// Like [`build_migration_graph`], also returning the dense `node_to_pkg` table (`u32::MAX` =
/// unmapped) for reuse by dashboard metagraph sharing.
pub fn build_migration_graph_with_index<'a>(
    graph: GraphSource<'a>,
    results: &AnalysisResults,
) -> Option<(MigrationGraphPayload, Vec<u32>)> {
    let community_table = results.community.as_ref();
    let centrality = results.centrality.as_ref();
    let blast = results.blast_radius.as_ref();
    let modularity = community_table.map(|c| c.modularity).unwrap_or(0.0);

    let node_count = graph.node_count();
    let mut agg: HashMap<String, MacroAgg> = HashMap::new();
    let mut label_cache = PackageLabelCache::default();
    // Temporary: store label key id per node during the first pass (avoids a second label compute).
    let mut node_label_key: Vec<u32> = vec![u32::MAX; node_count];
    let mut label_keys: HashMap<String, u32> = HashMap::new();
    let mut key_labels: Vec<String> = Vec::new();

    let _ = graph.for_each_hot_node(|n| {
        if !matches!(n.node_type, NodeType::Function) {
            return;
        }
        let Some(compact_id) = results.get_compact_id(n.id) else {
            return;
        };
        let label = label_cache.get_owned(n.file_path_off, n.file_path);
        let key = *label_keys.entry(label.clone()).or_insert_with(|| {
            let id = key_labels.len() as u32;
            key_labels.push(label.clone());
            id
        });
        if (n.idx as usize) < node_label_key.len() {
            node_label_key[n.idx as usize] = key;
        }
        let entry = agg.entry(label.clone()).or_insert_with(|| MacroAgg {
            label,
            ..Default::default()
        });
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
        if let Some(table) = community_table {
            if let Some(cid) = table.get(compact_id) {
                *entry.community_votes.entry(cid).or_insert(0) += 1;
            }
        }
    });

    if agg.is_empty() {
        return None;
    }

    let mut ranked: Vec<MacroAgg> = agg.into_values().collect();
    ranked.sort_by_key(|a| std::cmp::Reverse(a.member_count));

    let tail = if ranked.len() > MAX_MIGRATION_MACRO_NODES {
        ranked.split_off(MAX_MIGRATION_MACRO_NODES - 1)
    } else {
        vec![]
    };
    let top = ranked;

    let mut communities: Vec<MigrationCommunityNode> = Vec::new();
    let mut label_to_id: HashMap<String, usize> = HashMap::new();

    for (idx, bucket) in top.into_iter().enumerate() {
        label_to_id.insert(bucket.label.clone(), idx);
        communities.push(macro_to_node(idx, bucket));
    }

    if !tail.is_empty() {
        let id = communities.len();
        let mut merged = MacroAgg {
            label: "(other)".into(),
            ..Default::default()
        };
        for b in tail {
            merged.member_count += b.member_count;
            merged.pagerank_sum += b.pagerank_sum;
            merged.harmonic_sum += b.harmonic_sum;
            merged.betweenness_sum += b.betweenness_sum;
            merged.max_blast = merged.max_blast.max(b.max_blast);
            for (cid, votes) in b.community_votes {
                *merged.community_votes.entry(cid).or_insert(0) += votes;
            }
        }
        label_to_id.insert(merged.label.clone(), id);
        communities.push(macro_to_node(id, merged));
    }

    let other_id = label_to_id.get("(other)").copied();
    let mut key_to_pkg: Vec<u32> = vec![u32::MAX; key_labels.len()];
    for (i, label) in key_labels.iter().enumerate() {
        let pkg = label_to_id.get(label).copied().or(other_id).unwrap_or(0) as u32;
        key_to_pkg[i] = pkg;
    }

    let mut node_to_pkg: Vec<u32> = vec![u32::MAX; node_count];
    for (idx, key) in node_label_key.into_iter().enumerate() {
        if key == u32::MAX {
            continue;
        }
        node_to_pkg[idx] = key_to_pkg[key as usize];
    }

    let mut edge_weights: HashMap<(usize, usize), u32> = HashMap::new();
    let _ = graph.for_each_hot_edge(|e| {
        if e.edge_type != EdgeType::Calls {
            return;
        }
        if e.from_idx == u32::MAX || e.to_idx == u32::MAX {
            return;
        }
        let from_i = e.from_idx as usize;
        let to_i = e.to_idx as usize;
        if from_i >= node_to_pkg.len() || to_i >= node_to_pkg.len() {
            return;
        }
        let from_p = node_to_pkg[from_i];
        let to_p = node_to_pkg[to_i];
        if from_p == u32::MAX || to_p == u32::MAX || from_p == to_p {
            return;
        }
        *edge_weights
            .entry((from_p as usize, to_p as usize))
            .or_insert(0) += 1;
    });

    let edges: Vec<MigrationCommunityEdge> = edge_weights
        .into_iter()
        .map(|((source, target), weight)| MigrationCommunityEdge {
            source,
            target,
            weight,
            kind: "calls".into(),
        })
        .collect();

    Some((
        MigrationGraphPayload {
            schema_version: MIGRATION_GRAPH_SCHEMA_VERSION,
            mode: "package_macro".into(),
            modularity,
            communities,
            edges,
        },
        node_to_pkg,
    ))
}

fn macro_to_node(id: usize, bucket: MacroAgg) -> MigrationCommunityNode {
    let count = bucket.member_count.max(1) as f64;
    let community_id = bucket
        .community_votes
        .into_iter()
        .max_by_key(|(_, votes)| *votes)
        .map(|(cid, _)| cid);
    MigrationCommunityNode {
        id,
        label: bucket.label,
        member_count: bucket.member_count,
        avg_pagerank: bucket.pagerank_sum / count,
        avg_harmonic: bucket.harmonic_sum / count,
        avg_betweenness: bucket.betweenness_sum / count,
        max_blast: bucket.max_blast,
        community_id,
    }
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
            steps.sort_by(|a, b| {
                a.schedule_step
                    .cmp(&b.schedule_step)
                    .then(a.community_id.cmp(&b.community_id))
            });
        }
        MigrationOrderMode::Priority => {
            steps.sort_by(|a, b| {
                a.priority_rank
                    .cmp(&b.priority_rank)
                    .then(a.community_id.cmp(&b.community_id))
            });
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
                let Some(deg) = in_degree.get_mut(&next) else {
                    continue;
                };
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
    use rgctl_graph::backend::{GraphBackend, MemoryBackend};
    use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};

    fn build_fixture_graph() -> (MemoryBackend, AnalysisResults) {
        let mut na = Node::new(NodeType::Function, "svc_a".to_string());
        na.file_path = Some("src/main/java/com/foo/a/A.java".into());
        let mut nb = Node::new(NodeType::Function, "svc_b".to_string());
        nb.file_path = Some("src/main/java/com/foo/b/B.java".into());
        let mut nc = Node::new(NodeType::Function, "svc_c".to_string());
        nc.file_path = Some("src/main/java/com/foo/b/C.java".into());
        let a = na.id;
        let b = nb.id;
        let c = nc.id;

        let mut backend = MemoryBackend::new();
        backend.insert_node(na).unwrap();
        backend.insert_node(nb).unwrap();
        backend.insert_node(nc).unwrap();
        backend
            .insert_edge(Edge::new(a, b, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(b, c, EdgeType::Calls))
            .unwrap();

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
        assert_eq!(graph.mode, "package_macro");
        assert_eq!(graph.communities.len(), 2);
        assert_eq!(graph.edges.len(), 1);

        let pkg_b = graph
            .communities
            .iter()
            .find(|c| c.label == "com.foo.b")
            .unwrap();
        let pkg_a = graph
            .communities
            .iter()
            .find(|c| c.label == "com.foo.a")
            .unwrap();
        assert_eq!(pkg_a.member_count, 1);
        assert_eq!(pkg_b.member_count, 2);
        assert!((pkg_b.max_blast - 80.0).abs() < f64::EPSILON);
        assert_eq!(graph.edges[0].source, pkg_a.id);
        assert_eq!(graph.edges[0].target, pkg_b.id);
    }

    #[test]
    fn package_label_java_and_rust() {
        assert_eq!(
            package_label("src/main/java/com/example/foo/Bar.java"),
            "com.example.foo"
        );
        assert_eq!(
            package_label("src/graph/detection/mod.rs"),
            "src.graph.detection"
        );
    }

    #[test]
    fn package_label_issue_134_regression_cases() {
        // Direct file under crate `src/` (relative and absolute)
        assert_eq!(
            package_label("crates/rgctl-analysis/src/lib.rs"),
            "rgctl-analysis"
        );
        assert_eq!(
            package_label("/Users/sami/code/rgctl/crates/rgctl-analysis/src/lib.rs"),
            "rgctl-analysis"
        );
        assert_eq!(
            package_label("/Users/sami/code/rgctl/crates/rgctl-analysis/src/foo/bar.rs"),
            "rgctl-analysis.foo"
        );

        // Two same-named subdirectories across different crates do not collide
        assert_eq!(
            package_label("crates/crate-a/src/models/user.rs"),
            "crate-a.models"
        );
        assert_eq!(
            package_label("crates/crate-b/src/models/user.rs"),
            "crate-b.models"
        );

        // Top-level `src/`
        assert_eq!(package_label("src/main.rs"), "src");
        assert_eq!(package_label("/Users/sami/code/rgctl/src/main.rs"), "src");
        assert_eq!(
            package_label("/Users/sami/code/rgctl/src/cli/mod.rs"),
            "src.cli"
        );

        // Nested `/java/` checkout picks innermost JVM source root via rfind
        assert_eq!(
            package_label("/Users/dev/java/project/src/main/java/com/acme/orders/Order.java"),
            "com.acme.orders"
        );

        // Non-`src/` absolute paths never start with `.` or leak `/Users/...`
        for path in [
            "/Users/sami/code/rgctl/dashboard/src/App.tsx",
            "/Users/sami/code/rgctl/dashboard/src/components/Button.tsx",
            "/Users/sami/code/rgctl/tests/cold_profile_gates.rs",
            "/Users/sami/code/rgctl/crates/rgctl-core/tests/unit.rs",
            "/Users/sami/code/rgctl/custom/dir/file.py",
        ] {
            let lbl = package_label(path);
            assert!(
                !lbl.starts_with('.'),
                "label `{lbl}` for `{path}` must not start with '.'"
            );
            assert!(
                !lbl.contains("Users") && !lbl.contains("sami"),
                "label `{lbl}` for `{path}` must not leak host path segments"
            );
        }

        assert_eq!(
            package_label("/Users/sami/code/rgctl/dashboard/src/App.tsx"),
            "dashboard"
        );
        assert_eq!(
            package_label("/Users/sami/code/rgctl/dashboard/src/components/Button.tsx"),
            "dashboard.components"
        );
        assert_eq!(
            package_label("/Users/sami/code/rgctl/tests/cold_profile_gates.rs"),
            "tests"
        );
        assert_eq!(
            package_label("/Users/sami/code/rgctl/crates/rgctl-core/tests/unit.rs"),
            "rgctl-core.tests"
        );
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
        let callee = graph
            .communities
            .iter()
            .find(|c| c.label == "com.foo.b")
            .unwrap();
        let caller = graph
            .communities
            .iter()
            .find(|c| c.label == "com.foo.a")
            .unwrap();
        assert_eq!(plan.steps[0].community_id, callee.id);
        assert_eq!(plan.steps[0].schedule_step, 1);
        assert_eq!(plan.steps[1].community_id, caller.id);
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
            schema_version: 2,
            mode: "package_macro".into(),
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
                    community_id: None,
                },
                MigrationCommunityNode {
                    id: 1,
                    label: "high".into(),
                    member_count: 1,
                    avg_pagerank: 0.9,
                    avg_harmonic: 0.1,
                    avg_betweenness: 0.0,
                    max_blast: 10.0,
                    community_id: None,
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
            schema_version: 2,
            mode: "package_macro".into(),
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
                    community_id: None,
                },
                MigrationCommunityNode {
                    id: 2,
                    label: "a".into(),
                    member_count: 1,
                    avg_pagerank: 0.5,
                    avg_harmonic: 0.5,
                    avg_betweenness: 0.0,
                    max_blast: 0.0,
                    community_id: None,
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

    #[test]
    fn community_id_deserializes_louvain_alias() {
        let json = r#"{
            "id": 1,
            "label": "com.example",
            "member_count": 3,
            "avg_pagerank": 0.1,
            "avg_harmonic": 0.2,
            "avg_betweenness": 0.0,
            "max_blast": 1.0,
            "louvain_community_id": 42
        }"#;
        let node: MigrationCommunityNode = serde_json::from_str(json).expect("alias");
        assert_eq!(node.community_id, Some(42));
        let out = serde_json::to_value(&node).expect("ser");
        assert!(out.get("community_id").is_some());
        assert!(out.get("louvain_community_id").is_none());
        assert_eq!(MIGRATION_GRAPH_SCHEMA_VERSION, 3);
    }
}
