//! Before/after call-path review between base/head snapshots (`rgctl review paths`).

use rgctl_error::{Error, Result};
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_graph::snapshot::SnapshotNodeStore;
use rgctl_graph::snapshot_diff::{EdgeDeltaEvent, EdgeDeltaKind, NodeDeltaEvent, NodeDeltaKind};
use rgctl_graph::stable_key::{
    StableNodeKey, node_row_ref, node_scope_path_at, stable_key_from_row,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Default upstream Calls hops for representative spines.
pub const DEFAULT_UPSTREAM_DEPTH: usize = 2;
/// Default downstream Calls hops for representative spines.
pub const DEFAULT_DOWNSTREAM_DEPTH: usize = 1;
/// Default max symbols emitted in a paths report.
pub const DEFAULT_MAX_SYMBOLS: usize = 50;
/// Default fan-out cap per hop when selecting neighbors for spines.
pub const DEFAULT_FANOUT_CAP: usize = 10;
/// Soft cap on total neighbor nodes considered across the report.
pub const DEFAULT_MAX_NEIGHBOR_NODES: usize = 200;

/// Options for [`build_review_paths_report`].
#[derive(Clone, Debug)]
pub struct ReviewPathsOptions {
    /// Reverse-Calls depth for `path_before` / `path_after` spines.
    pub upstream_depth: usize,
    /// Forward-Calls depth for spines.
    pub downstream_depth: usize,
    /// Max symbols included in the report.
    pub max_symbols: usize,
    /// Max neighbors considered per hop when building spines.
    pub fanout_cap: usize,
    /// Soft global cap on neighbor nodes walked for spines.
    pub max_neighbor_nodes: usize,
    /// Optional name / FQN filter (`--symbol`).
    pub symbol_filter: Option<String>,
}

impl Default for ReviewPathsOptions {
    fn default() -> Self {
        Self {
            upstream_depth: DEFAULT_UPSTREAM_DEPTH,
            downstream_depth: DEFAULT_DOWNSTREAM_DEPTH,
            max_symbols: DEFAULT_MAX_SYMBOLS,
            fanout_cap: DEFAULT_FANOUT_CAP,
            max_neighbor_nodes: DEFAULT_MAX_NEIGHBOR_NODES,
            symbol_filter: None,
        }
    }
}

/// Top-level `rgctl review paths` JSON report (`schema_version: 1`).
#[derive(Clone, Debug, Serialize)]
pub struct ReviewPathsReport {
    /// Schema version (currently `1`).
    pub schema_version: u32,
    /// Command id (`"review paths"`).
    pub command: String,
    /// Aggregate counts for the comparison.
    pub change_summary: ChangeSummary,
    /// Report-level truncation flags.
    pub truncation: TruncationFlags,
    /// Per-symbol path reports.
    pub symbols: Vec<SymbolPathReport>,
    /// Diff files that did not yield seeded function/method symbols.
    pub unscored_files: Vec<UnscoredFile>,
    /// Candidates when `--symbol` is ambiguous (report may omit paths).
    pub ambiguous: Vec<AmbiguousCandidate>,
}

/// Aggregate edge / symbol counts.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ChangeSummary {
    /// Number of symbols included in `symbols`.
    pub changed_symbols: usize,
    /// Calls edge add/remove/retarget/unchanged counts.
    pub call_edges: CallEdgeCounts,
    /// Files in the git / change-set scope.
    pub files_in_scope: usize,
    /// Count of [`ReviewPathsReport::unscored_files`].
    pub unscored_files: usize,
}

/// Calls edge delta tallies for the PR scope.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CallEdgeCounts {
    /// Added Calls edges.
    pub added: usize,
    /// Removed Calls edges.
    pub removed: usize,
    /// Collapsed retarget pairs (same `from`, removed+added `to`).
    pub retargeted: usize,
    /// Unchanged Calls edges among seeded symbols (optional; often 0 in v1).
    pub unchanged: usize,
}

/// Truncation indicators (never silently drop without flags).
#[derive(Clone, Debug, Default, Serialize)]
pub struct TruncationFlags {
    /// Hit max-symbols cap.
    pub symbols: bool,
    /// Hit fan-out cap on at least one hop.
    pub fanout: bool,
    /// Requested depth could not be fully expanded (reserved; usually false).
    pub depth: bool,
}

/// Location of a symbol on one side of the diff.
#[derive(Clone, Debug, Serialize)]
pub struct SideLocation {
    /// Source file path when known.
    pub file: Option<String>,
    /// Inclusive start line (0 when unknown).
    pub start_line: u32,
    /// Inclusive end line (0 when unknown).
    pub end_line: u32,
}

/// One classified call-edge change for a seeded symbol.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathDelta {
    /// Edge present on head only.
    Added {
        /// Caller display name.
        from: String,
        /// Callee display name.
        to: String,
        /// Optional call-site line when known.
        #[serde(skip_serializing_if = "Option::is_none")]
        call_site_line: Option<u32>,
    },
    /// Edge present on base only.
    Removed {
        /// Caller display name.
        from: String,
        /// Callee display name.
        to: String,
        /// Optional call-site line when known.
        #[serde(skip_serializing_if = "Option::is_none")]
        call_site_line: Option<u32>,
    },
    /// Same caller retargeted from one callee to another.
    Retargeted {
        /// Caller display name.
        from: String,
        /// Callee on base.
        to_before: String,
        /// Callee on head.
        to_after: String,
    },
}

/// Per-symbol before/after path report.
#[derive(Clone, Debug, Serialize)]
pub struct SymbolPathReport {
    /// Stable cross-snapshot key (hex).
    pub stable_key: String,
    /// Display name.
    pub name: String,
    /// Node kind (`function`).
    pub kind: String,
    /// Base location when the symbol exists on base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<SideLocation>,
    /// Head location when the symbol exists on head.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<SideLocation>,
    /// Representative call spine on base (`…callers → symbol → …callees`).
    pub path_before: Vec<String>,
    /// Representative call spine on head.
    pub path_after: Vec<String>,
    /// Classified Calls edge changes involving this symbol.
    pub path_delta: Vec<PathDelta>,
    /// Per-symbol truncation flags.
    pub truncation: TruncationFlags,
}

/// File in scope that was not scored as a function/method seed.
#[derive(Clone, Debug, Serialize)]
pub struct UnscoredFile {
    /// Repo-relative path.
    pub path: String,
    /// Why it was not scored (`no_symbol`, `non_code`, …).
    pub reason: String,
}

/// Ambiguous `--symbol` match candidate.
#[derive(Clone, Debug, Serialize)]
pub struct AmbiguousCandidate {
    /// Display name.
    pub name: String,
    /// File path when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Stable key hex.
    pub stable_key: String,
}

#[derive(Clone, Debug)]
struct Identity {
    name: String,
    file: Option<String>,
    start_line: u32,
    end_line: u32,
}

#[derive(Clone, Debug, Default)]
struct SideGraph {
    by_key: HashMap<StableNodeKey, Identity>,
    by_name: HashMap<String, Vec<StableNodeKey>>,
    outgoing: HashMap<StableNodeKey, Vec<StableNodeKey>>,
    incoming: HashMap<StableNodeKey, Vec<StableNodeKey>>,
}

impl SideGraph {
    fn from_store(store: &SnapshotNodeStore) -> Result<Self> {
        let col = store.columnar().ok_or_else(|| {
            Error::GraphError("review paths requires columnar v2 snapshots".into())
        })?;
        let mut g = SideGraph::default();
        let mut uuid_to_key: HashMap<uuid::Uuid, StableNodeKey> = HashMap::new();
        for idx in 0..col.node_count() {
            let row = node_row_ref(col, idx)?;
            if row.node_type != NodeType::Function {
                continue;
            }
            let key = stable_key_from_row(col, idx)?;
            let file = node_scope_path_at(col, idx)?.map(|s| s.to_string());
            let name = store
                .get_node(row.id)?
                .map(|n| n.name.to_string())
                .unwrap_or_else(|| "<unknown>".into());
            uuid_to_key.insert(row.id, key);
            g.by_name.entry(name.clone()).or_default().push(key);
            g.by_key.insert(
                key,
                Identity {
                    name,
                    file,
                    start_line: row.start_line,
                    end_line: row.end_line,
                },
            );
        }
        store.for_each_edge(|from, to, et| {
            if et != EdgeType::Calls {
                return Ok(());
            }
            let Some(&fk) = uuid_to_key.get(&from) else {
                return Ok(());
            };
            let Some(&tk) = uuid_to_key.get(&to) else {
                return Ok(());
            };
            g.outgoing.entry(fk).or_default().push(tk);
            g.incoming.entry(tk).or_default().push(fk);
            Ok(())
        })?;
        for v in g.outgoing.values_mut() {
            v.sort_unstable();
            v.dedup();
        }
        for v in g.incoming.values_mut() {
            v.sort_unstable();
            v.dedup();
        }
        Ok(g)
    }

    fn display_name(&self, key: StableNodeKey) -> String {
        self.by_key
            .get(&key)
            .map(|i| i.name.clone())
            .unwrap_or_else(|| format!("key:{:016x}", key.as_u64()))
    }

    fn location(&self, key: StableNodeKey) -> Option<SideLocation> {
        self.by_key.get(&key).map(|i| SideLocation {
            file: i.file.clone(),
            start_line: i.start_line,
            end_line: i.end_line,
        })
    }
}

/// Build the seed set: hunk entities ∪ Calls edge endpoints ∪ base-only deletes.
fn seed_review_symbols(
    hunk_seeds: &[StableNodeKey],
    edge_events: &[EdgeDeltaEvent],
    node_events: &[NodeDeltaEvent],
    base: &SideGraph,
    head: &SideGraph,
) -> Vec<StableNodeKey> {
    let mut seeds: HashSet<StableNodeKey> = HashSet::new();
    for k in hunk_seeds {
        if base.by_key.contains_key(k) || head.by_key.contains_key(k) {
            seeds.insert(*k);
        }
    }
    for e in edge_events {
        if e.edge_type != EdgeType::Calls {
            continue;
        }
        if base.by_key.contains_key(&e.from) || head.by_key.contains_key(&e.from) {
            seeds.insert(e.from);
        }
        if base.by_key.contains_key(&e.to) || head.by_key.contains_key(&e.to) {
            seeds.insert(e.to);
        }
    }
    for n in node_events {
        if n.kind != NodeDeltaKind::Removed {
            continue;
        }
        if n.base.map(|r| r.node_type) == Some(NodeType::Function) {
            seeds.insert(n.key);
        }
    }
    let mut out: Vec<_> = seeds.into_iter().collect();
    out.sort_by_key(|k| k.as_u64());
    out
}

/// Rank seeds so Calls-delta-touched symbols come first (for max-symbol truncation).
fn rank_seeds(
    seeds: &[StableNodeKey],
    edge_events: &[EdgeDeltaEvent],
) -> Vec<StableNodeKey> {
    let mut touched: HashSet<StableNodeKey> = HashSet::new();
    for e in edge_events {
        if e.edge_type == EdgeType::Calls {
            touched.insert(e.from);
            touched.insert(e.to);
        }
    }
    let mut ranked = seeds.to_vec();
    ranked.sort_by(|a, b| {
        let ta = touched.contains(a);
        let tb = touched.contains(b);
        tb.cmp(&ta).then_with(|| a.as_u64().cmp(&b.as_u64()))
    });
    ranked
}

enum SymbolResolve {
    One(StableNodeKey),
    Ambiguous(Vec<AmbiguousCandidate>),
}

fn resolve_symbol_filter(
    filter: &str,
    base: &SideGraph,
    head: &SideGraph,
) -> Result<SymbolResolve> {
    let mut matches: HashMap<StableNodeKey, AmbiguousCandidate> = HashMap::new();
    for side in [head, base] {
        for (name, keys) in &side.by_name {
            if name == filter || name.ends_with(&format!(".{filter}")) || name.ends_with(filter) {
                for &k in keys {
                    let id = side.by_key.get(&k);
                    matches.entry(k).or_insert(AmbiguousCandidate {
                        name: id.map(|i| i.name.clone()).unwrap_or_else(|| name.clone()),
                        file: id.and_then(|i| i.file.clone()),
                        stable_key: format!("{:016x}", k.as_u64()),
                    });
                }
            }
        }
        // Exact stable-key hex (as printed in reports).
        if let Ok(raw) = u64::from_str_radix(filter.trim_start_matches("0x"), 16) {
            for (k, id) in &side.by_key {
                if k.as_u64() == raw {
                    matches.entry(*k).or_insert(AmbiguousCandidate {
                        name: id.name.clone(),
                        file: id.file.clone(),
                        stable_key: format!("{:016x}", k.as_u64()),
                    });
                }
            }
        }
    }
    // Also match when filter appears as a path-qualified suffix on identity names.
    if matches.is_empty() {
        for side in [head, base] {
            for (k, id) in &side.by_key {
                if id.name == filter || id.name.ends_with(&format!("::{filter}")) {
                    matches.entry(*k).or_insert(AmbiguousCandidate {
                        name: id.name.clone(),
                        file: id.file.clone(),
                        stable_key: format!("{:016x}", k.as_u64()),
                    });
                }
            }
        }
    }
    match matches.len() {
        0 => Err(Error::GraphError(format!(
            "symbol filter '{filter}' matched no function/method nodes on base or head"
        ))),
        1 => Ok(SymbolResolve::One(*matches.keys().next().expect("len 1"))),
        _ => {
            let mut cands: Vec<_> = matches.into_values().collect();
            cands.sort_by(|a, b| a.stable_key.cmp(&b.stable_key));
            Ok(SymbolResolve::Ambiguous(cands))
        }
    }
}

/// Collapse added/removed Calls edges for a seed into `path_delta` entries.
fn classify_path_delta(
    seed: StableNodeKey,
    edge_events: &[EdgeDeltaEvent],
    base: &SideGraph,
    head: &SideGraph,
) -> (Vec<PathDelta>, CallEdgeCounts) {
    let mut removed: Vec<(StableNodeKey, StableNodeKey)> = Vec::new();
    let mut added: Vec<(StableNodeKey, StableNodeKey)> = Vec::new();
    for e in edge_events {
        if e.edge_type != EdgeType::Calls {
            continue;
        }
        // Scoped to edges involving this seed as caller or callee.
        if e.from != seed && e.to != seed {
            continue;
        }
        match e.kind {
            EdgeDeltaKind::Removed => removed.push((e.from, e.to)),
            EdgeDeltaKind::Added => added.push((e.from, e.to)),
        }
    }

    let mut counts = CallEdgeCounts::default();
    let mut deltas = Vec::new();
    let mut used_removed = HashSet::new();
    let mut used_added = HashSet::new();

    // Retarget: same `from` has ≥1 removed and ≥1 added `to`.
    let mut froms: HashSet<StableNodeKey> = HashSet::new();
    for (f, _) in &removed {
        froms.insert(*f);
    }
    for (f, _) in &added {
        froms.insert(*f);
    }
    let mut from_list: Vec<_> = froms.into_iter().collect();
    from_list.sort_by_key(|k| k.as_u64());
    for from in from_list {
        let rem: Vec<_> = removed
            .iter()
            .enumerate()
            .filter(|(_, (f, _))| *f == from)
            .collect();
        let add: Vec<_> = added
            .iter()
            .enumerate()
            .filter(|(_, (f, _))| *f == from)
            .collect();
        if rem.is_empty() || add.is_empty() {
            continue;
        }
        // Pair greedily by sort order of to keys.
        let mut rem_sorted = rem;
        let mut add_sorted = add;
        rem_sorted.sort_by_key(|(_, (_, t))| t.as_u64());
        add_sorted.sort_by_key(|(_, (_, t))| t.as_u64());
        let pairs = rem_sorted.len().min(add_sorted.len());
        for i in 0..pairs {
            let (ri, (_, to_before)) = rem_sorted[i];
            let (ai, (_, to_after)) = add_sorted[i];
            used_removed.insert(ri);
            used_added.insert(ai);
            counts.retargeted += 1;
            let from_name = head
                .by_key
                .get(&from)
                .or_else(|| base.by_key.get(&from))
                .map(|i| i.name.clone())
                .unwrap_or_else(|| base.display_name(from));
            deltas.push(PathDelta::Retargeted {
                from: from_name,
                to_before: base.display_name(*to_before),
                to_after: head.display_name(*to_after),
            });
        }
    }

    for (i, (from, to)) in removed.iter().enumerate() {
        if used_removed.contains(&i) {
            continue;
        }
        counts.removed += 1;
        deltas.push(PathDelta::Removed {
            from: base.display_name(*from),
            to: base.display_name(*to),
            call_site_line: None,
        });
    }
    for (i, (from, to)) in added.iter().enumerate() {
        if used_added.contains(&i) {
            continue;
        }
        counts.added += 1;
        deltas.push(PathDelta::Added {
            from: head.display_name(*from),
            to: head.display_name(*to),
            call_site_line: None,
        });
    }

    (deltas, counts)
}

fn preferred_neighbor_keys(
    neighbors: &[StableNodeKey],
    prefer: &HashSet<StableNodeKey>,
    side: &SideGraph,
    fanout: usize,
) -> (Vec<StableNodeKey>, bool) {
    let mut preferred: Vec<_> = neighbors
        .iter()
        .copied()
        .filter(|k| prefer.contains(k))
        .collect();
    preferred.sort_by(|a, b| {
        side.display_name(*a)
            .cmp(&side.display_name(*b))
            .then_with(|| a.as_u64().cmp(&b.as_u64()))
    });
    let mut rest: Vec<_> = neighbors
        .iter()
        .copied()
        .filter(|k| !prefer.contains(k))
        .collect();
    rest.sort_by(|a, b| {
        side.display_name(*a)
            .cmp(&side.display_name(*b))
            .then_with(|| a.as_u64().cmp(&b.as_u64()))
    });
    preferred.extend(rest);
    let truncated = preferred.len() > fanout;
    preferred.truncate(fanout);
    (preferred, truncated)
}

/// Build one representative spine: callers… → seed → …callees.
fn build_spine(
    seed: StableNodeKey,
    side: &SideGraph,
    upstream_depth: usize,
    downstream_depth: usize,
    fanout: usize,
    prefer: &HashSet<StableNodeKey>,
    neighbor_budget: &mut usize,
) -> (Vec<String>, TruncationFlags) {
    let mut trunc = TruncationFlags::default();
    if !side.by_key.contains_key(&seed) {
        return (Vec::new(), trunc);
    }

    // Downstream chain
    let mut down_names = Vec::new();
    let mut cur = seed;
    for _ in 0..downstream_depth {
        let Some(outs) = side.outgoing.get(&cur) else {
            break;
        };
        if outs.is_empty() {
            break;
        }
        if *neighbor_budget == 0 {
            trunc.fanout = true;
            break;
        }
        let (picked, hit) = preferred_neighbor_keys(outs, prefer, side, fanout.min(*neighbor_budget));
        if hit || outs.len() > fanout {
            trunc.fanout = true;
        }
        let Some(&next) = picked.first() else {
            break;
        };
        *neighbor_budget = neighbor_budget.saturating_sub(1);
        down_names.push(side.display_name(next));
        cur = next;
    }

    // Upstream: BFS reverse, then pick one chain root→…→seed
    let mut parent: HashMap<StableNodeKey, StableNodeKey> = HashMap::new();
    let mut q = VecDeque::new();
    q.push_back((seed, 0usize));
    let mut best_leaf: Option<StableNodeKey> = None;
    while let Some((node, d)) = q.pop_front() {
        if d >= upstream_depth {
            if best_leaf.is_none() {
                best_leaf = Some(node);
            }
            continue;
        }
        let Some(ins) = side.incoming.get(&node) else {
            if d > 0 && best_leaf.is_none() {
                best_leaf = Some(node);
            }
            continue;
        };
        if ins.is_empty() {
            if d > 0 {
                best_leaf = Some(best_leaf.unwrap_or(node));
            }
            continue;
        }
        if *neighbor_budget == 0 {
            trunc.fanout = true;
            best_leaf = Some(best_leaf.unwrap_or(node));
            break;
        }
        let (picked, hit) = preferred_neighbor_keys(ins, prefer, side, fanout.min(*neighbor_budget));
        if hit || ins.len() > fanout {
            trunc.fanout = true;
        }
        for &caller in &picked {
            if parent.contains_key(&caller) || caller == seed {
                continue;
            }
            parent.insert(caller, node);
            *neighbor_budget = neighbor_budget.saturating_sub(1);
            q.push_back((caller, d + 1));
            // Prefer delta-touched callers as chain leaves.
            if prefer.contains(&caller) {
                best_leaf = Some(caller);
            } else if best_leaf.is_none() {
                best_leaf = Some(caller);
            }
        }
    }

    let mut up_chain = Vec::new();
    if let Some(mut leaf) = best_leaf {
        if leaf != seed {
            up_chain.push(side.display_name(leaf));
            while let Some(&p) = parent.get(&leaf) {
                if p == seed {
                    break;
                }
                up_chain.push(side.display_name(p));
                leaf = p;
            }
            up_chain.reverse();
        }
    }

    let mut path = up_chain;
    path.push(side.display_name(seed));
    path.extend(down_names);
    (path, trunc)
}

fn prefer_keys_from_deltas(edge_events: &[EdgeDeltaEvent], seed: StableNodeKey) -> HashSet<StableNodeKey> {
    let mut prefer = HashSet::new();
    for e in edge_events {
        if e.edge_type != EdgeType::Calls {
            continue;
        }
        if e.from == seed || e.to == seed {
            prefer.insert(e.from);
            prefer.insert(e.to);
        }
    }
    prefer
}

fn is_code_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    !lower.ends_with(".md")
        && !lower.ends_with(".txt")
        && !lower.ends_with(".rst")
        && !lower.ends_with(".json")
        && !lower.contains("/docs/")
}

/// Build the full `review paths` report from open snapshots and precomputed scope inputs.
pub fn build_review_paths_report(
    base_store: &SnapshotNodeStore,
    head_store: &SnapshotNodeStore,
    hunk_seeds: &[StableNodeKey],
    edge_events: &[EdgeDeltaEvent],
    node_events: &[NodeDeltaEvent],
    files_in_scope: &[String],
    options: &ReviewPathsOptions,
) -> Result<ReviewPathsReport> {
    let base = SideGraph::from_store(base_store)?;
    let head = SideGraph::from_store(head_store)?;

    if let Some(filter) = options.symbol_filter.as_deref() {
        match resolve_symbol_filter(filter, &base, &head)? {
            SymbolResolve::One(key) => {
                return build_for_seeds(
                    &[key],
                    &base,
                    &head,
                    edge_events,
                    files_in_scope,
                    options,
                    Vec::new(),
                );
            }
            SymbolResolve::Ambiguous(ambiguous) => {
                return Ok(ReviewPathsReport {
                    schema_version: 1,
                    command: "review paths".into(),
                    change_summary: ChangeSummary {
                        files_in_scope: files_in_scope.len(),
                        ..Default::default()
                    },
                    truncation: TruncationFlags::default(),
                    symbols: Vec::new(),
                    unscored_files: Vec::new(),
                    ambiguous,
                });
            }
        }
    }

    let seeds = seed_review_symbols(hunk_seeds, edge_events, node_events, &base, &head);
    build_for_seeds(
        &seeds,
        &base,
        &head,
        edge_events,
        files_in_scope,
        options,
        Vec::new(),
    )
}

fn build_for_seeds(
    seeds: &[StableNodeKey],
    base: &SideGraph,
    head: &SideGraph,
    edge_events: &[EdgeDeltaEvent],
    files_in_scope: &[String],
    options: &ReviewPathsOptions,
    ambiguous: Vec<AmbiguousCandidate>,
) -> Result<ReviewPathsReport> {
    let ranked = rank_seeds(seeds, edge_events);
    let mut trunc_report = TruncationFlags::default();
    let selected: Vec<StableNodeKey> = if ranked.len() > options.max_symbols {
        trunc_report.symbols = true;
        ranked.into_iter().take(options.max_symbols).collect()
    } else {
        ranked
    };

    let mut neighbor_budget = options.max_neighbor_nodes;
    let mut symbols = Vec::with_capacity(selected.len());
    let mut summary_edges = CallEdgeCounts::default();
    let mut seeded_files: HashSet<String> = HashSet::new();

    for seed in &selected {
        let prefer = prefer_keys_from_deltas(edge_events, *seed);
        let (path_before, t_before) = build_spine(
            *seed,
            base,
            options.upstream_depth,
            options.downstream_depth,
            options.fanout_cap,
            &prefer,
            &mut neighbor_budget,
        );
        let (path_after, t_after) = build_spine(
            *seed,
            head,
            options.upstream_depth,
            options.downstream_depth,
            options.fanout_cap,
            &prefer,
            &mut neighbor_budget,
        );
        let (path_delta, edge_counts) = classify_path_delta(*seed, edge_events, base, head);
        summary_edges.added += edge_counts.added;
        summary_edges.removed += edge_counts.removed;
        summary_edges.retargeted += edge_counts.retargeted;
        summary_edges.unchanged += edge_counts.unchanged;

        let ident = head.by_key.get(seed).or_else(|| base.by_key.get(seed));
        let name = ident
            .map(|i| i.name.clone())
            .unwrap_or_else(|| format!("key:{:016x}", seed.as_u64()));
        if let Some(f) = ident.and_then(|i| i.file.clone()) {
            seeded_files.insert(f);
        }
        let mut sym_trunc = TruncationFlags {
            symbols: false,
            fanout: t_before.fanout || t_after.fanout,
            depth: t_before.depth || t_after.depth,
        };
        if neighbor_budget == 0 {
            sym_trunc.fanout = true;
            trunc_report.fanout = true;
        }
        if sym_trunc.fanout {
            trunc_report.fanout = true;
        }

        symbols.push(SymbolPathReport {
            stable_key: format!("{:016x}", seed.as_u64()),
            name,
            kind: "function".into(),
            base: base.location(*seed),
            head: head.location(*seed),
            path_before,
            path_after,
            path_delta,
            truncation: sym_trunc,
        });
    }

    let unscored_files = files_in_scope
        .iter()
        .filter(|p| !seeded_files.iter().any(|s| s == *p || s.ends_with(p.as_str())))
        .map(|p| UnscoredFile {
            path: p.clone(),
            reason: if is_code_path(p) {
                "no_symbol".into()
            } else {
                "non_code".into()
            },
        })
        .collect::<Vec<_>>();

    Ok(ReviewPathsReport {
        schema_version: 1,
        command: "review paths".into(),
        change_summary: ChangeSummary {
            changed_symbols: symbols.len(),
            call_edges: summary_edges,
            files_in_scope: files_in_scope.len(),
            unscored_files: unscored_files.len(),
        },
        truncation: trunc_report,
        symbols,
        unscored_files,
        ambiguous,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::schema::{Edge, Node};
    use rgctl_graph::snapshot::SnapshotNodeStore;
    use rgctl_graph::snapshot_diff::{VecDiffSink, diff_snapshots};
    use rgctl_graph::stable_key::stable_key_from_facets;
    use rgctl_graph::write_columnar_from_nodes_edges;
    use tempfile::TempDir;

    fn write_snap(
        dir: &std::path::Path,
        name: &str,
        nodes: Vec<Node>,
        edges: Vec<Edge>,
    ) -> std::path::PathBuf {
        let path = dir.join(name);
        write_columnar_from_nodes_edges(nodes, edges, &path).unwrap();
        path
    }

    fn rewired_fixture(tmp: &TempDir) -> (SnapshotNodeStore, SnapshotNodeStore, Vec<EdgeDeltaEvent>) {
        // Base: handle → submitOrder → chargeCard
        let handle = Node::new(NodeType::Function, "CheckoutController.handle")
            .with_file_path("src/Checkout.java");
        let submit = Node::new(NodeType::Function, "submitOrder").with_file_path("src/Order.java");
        let charge = Node::new(NodeType::Function, "chargeCard").with_file_path("src/Pay.java");
        let h = handle.id;
        let s = submit.id;
        let c = charge.id;
        let base_path = write_snap(
            tmp.path(),
            "base.bin",
            vec![handle, submit, charge],
            vec![
                Edge::new(h, s, EdgeType::Calls),
                Edge::new(s, c, EdgeType::Calls),
            ],
        );

        // Head: handle → submitOrder → authorizeThenCapture
        let handle2 = Node::new(NodeType::Function, "CheckoutController.handle")
            .with_file_path("src/Checkout.java");
        let submit2 = Node::new(NodeType::Function, "submitOrder").with_file_path("src/Order.java");
        let auth =
            Node::new(NodeType::Function, "authorizeThenCapture").with_file_path("src/Pay.java");
        let h2 = handle2.id;
        let s2 = submit2.id;
        let a2 = auth.id;
        let head_path = write_snap(
            tmp.path(),
            "head.bin",
            vec![handle2, submit2, auth],
            vec![
                Edge::new(h2, s2, EdgeType::Calls),
                Edge::new(s2, a2, EdgeType::Calls),
            ],
        );

        let base = SnapshotNodeStore::open(&base_path).unwrap();
        let head = SnapshotNodeStore::open(&head_path).unwrap();
        let mut sink = VecDiffSink::default();
        diff_snapshots(&base, &head, &mut sink).unwrap();
        (base, head, sink.edges)
    }

    #[test]
    fn rewired_callee_emits_retargeted_delta() {
        let tmp = TempDir::new().unwrap();
        let (base, head, edges) = rewired_fixture(&tmp);
        let submit_key =
            stable_key_from_facets(
                Some("src/Order.java"),
                "submitOrder",
                NodeType::Function,
                None,
                None,
            );
        let report = build_review_paths_report(
            &base,
            &head,
            &[],
            &edges,
            &[],
            &["src/Order.java".into()],
            &ReviewPathsOptions::default(),
        )
        .unwrap();
        let sym = report
            .symbols
            .iter()
            .find(|s| s.name == "submitOrder")
            .expect("submitOrder seeded from edge delta");
        assert!(
            sym.path_delta.iter().any(|d| matches!(
                d,
                PathDelta::Retargeted {
                    to_before,
                    to_after,
                    ..
                } if to_before == "chargeCard" && to_after == "authorizeThenCapture"
            )),
            "delta={:?}",
            sym.path_delta
        );
        assert!(
            sym.path_before.iter().any(|n| n == "chargeCard"),
            "before={:?}",
            sym.path_before
        );
        assert!(
            sym.path_after.iter().any(|n| n == "authorizeThenCapture"),
            "after={:?}",
            sym.path_after
        );
        let _ = submit_key;
    }

    #[test]
    fn deleted_symbol_one_sided_path() {
        let tmp = TempDir::new().unwrap();
        let keep = Node::new(NodeType::Function, "keep").with_file_path("a.rs");
        let gone = Node::new(NodeType::Function, "gone").with_file_path("a.rs");
        let k = keep.id;
        let g = gone.id;
        let base_path = write_snap(
            tmp.path(),
            "base.bin",
            vec![keep, gone],
            vec![Edge::new(k, g, EdgeType::Calls)],
        );
        let keep2 = Node::new(NodeType::Function, "keep").with_file_path("a.rs");
        let head_path = write_snap(tmp.path(), "head.bin", vec![keep2], vec![]);
        let base = SnapshotNodeStore::open(&base_path).unwrap();
        let head = SnapshotNodeStore::open(&head_path).unwrap();
        let mut sink = VecDiffSink::default();
        diff_snapshots(&base, &head, &mut sink).unwrap();
        let report = build_review_paths_report(
            &base,
            &head,
            &[],
            &sink.edges,
            &sink.nodes,
            &["a.rs".into()],
            &ReviewPathsOptions::default(),
        )
        .unwrap();
        let gone_sym = report
            .symbols
            .iter()
            .find(|s| s.name == "gone")
            .expect("deleted fn seeded");
        assert!(!gone_sym.path_before.is_empty());
        assert!(gone_sym.path_after.is_empty());
        assert!(gone_sym.head.is_none());
    }

    #[test]
    fn fanout_cap_sets_truncation() {
        let tmp = TempDir::new().unwrap();
        let target = Node::new(NodeType::Function, "target").with_file_path("t.rs");
        let tid = target.id;
        let mut nodes = vec![target];
        let mut edges = Vec::new();
        for i in 0..15 {
            let c = Node::new(NodeType::Function, format!("caller{i}")).with_file_path("t.rs");
            let cid = c.id;
            edges.push(Edge::new(cid, tid, EdgeType::Calls));
            nodes.push(c);
        }
        let path = write_snap(tmp.path(), "same.bin", nodes, edges);
        let store = SnapshotNodeStore::open(&path).unwrap();
        let key = stable_key_from_facets(Some("t.rs"), "target", NodeType::Function, None, None);
        let mut opts = ReviewPathsOptions::default();
        opts.fanout_cap = 3;
        opts.upstream_depth = 1;
        let report = build_review_paths_report(
            &store,
            &store,
            &[key],
            &[],
            &[],
            &["t.rs".into()],
            &opts,
        )
        .unwrap();
        let sym = &report.symbols[0];
        assert!(sym.truncation.fanout || report.truncation.fanout);
    }

    #[test]
    fn unscored_markdown_file() {
        let tmp = TempDir::new().unwrap();
        let n = Node::new(NodeType::Function, "main").with_file_path("main.rs");
        let path = write_snap(tmp.path(), "s.bin", vec![n], vec![]);
        let store = SnapshotNodeStore::open(&path).unwrap();
        let report = build_review_paths_report(
            &store,
            &store,
            &[],
            &[],
            &[],
            &["README.md".into()],
            &ReviewPathsOptions::default(),
        )
        .unwrap();
        assert!(report.symbols.is_empty());
        assert_eq!(report.unscored_files.len(), 1);
        assert_eq!(report.unscored_files[0].reason, "non_code");
    }

    #[test]
    fn ambiguous_symbol_filter() {
        let tmp = TempDir::new().unwrap();
        let a = Node::new(NodeType::Function, "dup").with_file_path("a.rs");
        // Same name different file → different stable keys
        let b = Node::new(NodeType::Function, "dup").with_file_path("b.rs");
        let path = write_snap(tmp.path(), "s.bin", vec![a, b], vec![]);
        let store = SnapshotNodeStore::open(&path).unwrap();
        let mut opts = ReviewPathsOptions::default();
        opts.symbol_filter = Some("dup".into());
        let report = build_review_paths_report(
            &store,
            &store,
            &[],
            &[],
            &[],
            &[],
            &opts,
        )
        .unwrap();
        assert!(report.symbols.is_empty());
        assert!(report.ambiguous.len() >= 2);
    }
}
