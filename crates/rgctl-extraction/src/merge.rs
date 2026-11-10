//! Cross-repo `park-and-resolve` merge (`rgctl merge`).

use crate::parked_relations::{PARKED_RELATIONS_FILE, ParkedRelationRecord, load_parked_relations};
use rgctl_error::{Error, Result};
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::paths::{ARTIFACT_DIR_NAME, artifact_dir};
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::snapshot::{MmappedGraphSnapshot, SNAPSHOT_FILE};
use rgctl_graph::stable_key::deterministic_node_id_with_discriminator;
use rgctl_graph::write_columnar_from_backend;
use rgctl_plugin_api::RelationType;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Candidate export for cross-repo name / module-path matching.
#[derive(Debug, Clone)]
pub struct ExportCandidate {
    /// Input repository index.
    pub repo_idx: usize,
    /// Node UUID in the merged graph (post-namespace copy).
    pub node_id: Uuid,
    /// Node kind.
    pub node_type: NodeType,
    /// Qualified name (may be namespaced).
    pub qualified_name: String,
    /// Namespaced file path (`<repo>/<rel>`).
    pub file_path: String,
    /// Normalized module path segments from `<repo_name>/<file_path>` (+ qn).
    pub norm_segments: Vec<String>,
}

/// Export indexes built across all input repositories.
#[derive(Debug, Default)]
pub struct CrossRepoExportIndex {
    /// Exact qualified-name → candidates.
    pub by_qualified: FxHashMap<String, Vec<(usize, Uuid)>>,
    /// Route qualified names (`route::METHOD /path`) → candidates.
    pub by_route: FxHashMap<String, Vec<(usize, Uuid)>>,
    /// Simple name → export candidates.
    pub by_name: FxHashMap<String, Vec<ExportCandidate>>,
}

/// Summary of a cross-repo merge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeReport {
    /// Number of input repositories merged.
    pub repos_merged: usize,
    /// Nodes in the merged snapshot.
    pub total_nodes: usize,
    /// Edges in the merged snapshot.
    pub total_edges: usize,
    /// Parked relations evaluated across inputs.
    pub parked_relations_evaluated: usize,
    /// Parked relations resolved to cross-repo edges.
    pub cross_repo_edges_resolved: usize,
    /// External stubs rewired (and typically pruned).
    pub stubs_rewired: usize,
    /// Ambiguous bare / tied candidates skipped (no false edge).
    pub ambiguous_skipped: usize,
}

/// Normalize an import or file module path into lowercase path segments.
///
/// Strips scopes (`@`), relative prefixes, file extensions, and structural
/// noise directories (`src`, `lib`, `app`, …).
pub fn normalize_module_path_segments(raw: &str) -> Vec<String> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return Vec::new();
    }
    // Strip quotes if present.
    s = s.trim_matches(['"', '\'']).to_string();
    while s.starts_with("./") || s.starts_with(".\\") {
        s = s[2..].to_string();
    }
    while s.starts_with("../") || s.starts_with("..\\") {
        s = s[3..].to_string();
    }
    if let Some(rest) = s.strip_prefix('@') {
        s = rest.to_string();
    }

    const EXTS: &[&str] = &[
        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".py", ".rs", ".go", ".java", ".kt", ".cs",
        ".rb",
    ];
    for ext in EXTS {
        if let Some(stripped) = s.strip_suffix(ext) {
            s = stripped.to_string();
            break;
        }
    }
    for suffix in [
        "/index",
        "/mod",
        "/__init__",
        "\\index",
        "\\mod",
        "\\__init__",
    ] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            s = stripped.to_string();
            break;
        }
    }

    const NOISE: &[&str] = &[
        "src", "lib", "app", "pkg", "internal", "main", "java", "kotlin", "python", "crates",
    ];

    let mut segs: Vec<String> = s
        .split(['/', '\\', '.', ':'])
        .filter(|p| !p.is_empty() && *p != ":")
        .map(|p| p.to_ascii_lowercase())
        .filter(|p| !NOISE.contains(&p.as_str()))
        .collect();
    // Collapse accidental empty / duplicate noise after split on `::`.
    segs.retain(|p| !p.is_empty());
    segs
}

fn trailing_suffix_overlap(a: &[String], b: &[String]) -> usize {
    let mut score = 0usize;
    for (x, y) in a.iter().rev().zip(b.iter().rev()) {
        if x == y {
            score += 1;
        } else {
            break;
        }
    }
    score
}

fn simple_name(to_name: &str) -> &str {
    to_name
        .rsplit(['.', ':', '#'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(to_name)
}

fn relation_type_to_edge_type(relation_type: RelationType) -> EdgeType {
    match relation_type {
        RelationType::Calls => EdgeType::Calls,
        RelationType::Uses => EdgeType::Uses,
        RelationType::Extends => EdgeType::Extends,
        RelationType::Implements => EdgeType::Implements,
        RelationType::Injects => EdgeType::Injects,
        _ => EdgeType::References,
    }
}

fn node_type_compatible(relation: RelationType, node_type: NodeType) -> bool {
    match relation {
        RelationType::Calls => matches!(node_type, NodeType::Function),
        RelationType::Extends | RelationType::Implements | RelationType::Injects => matches!(
            node_type,
            NodeType::Class
                | NodeType::Interface
                | NodeType::Struct
                | NodeType::Enum
                | NodeType::Module
                | NodeType::Annotation
        ),
        RelationType::Uses => true,
        _ => true,
    }
}

fn is_external_stub(node: &Node) -> bool {
    node.properties.get("is_external_stub").map(String::as_str) == Some("true")
        || node.file_path.as_deref() == Some("<external>")
}

fn is_route_node(node: &Node) -> bool {
    node.qualified_name
        .as_deref()
        .is_some_and(|q| q.starts_with("route::"))
        || node.labels.iter().any(|l| l.as_str() == "route")
}

fn resolve_repo_rgctl_dir(path: &Path) -> Result<PathBuf> {
    if path.join(SNAPSHOT_FILE).is_file() {
        return Ok(path.to_path_buf());
    }
    if path.file_name().and_then(|s| s.to_str()) == Some(ARTIFACT_DIR_NAME)
        && path.join(SNAPSHOT_FILE).is_file()
    {
        return Ok(path.to_path_buf());
    }
    let art = artifact_dir(path);
    if art.join(SNAPSHOT_FILE).is_file() {
        return Ok(art);
    }
    Err(Error::GraphError(format!(
        "no {} under {}",
        SNAPSHOT_FILE,
        path.display()
    )))
}

fn repo_display_name(path: &Path, rgctl_dir: &Path) -> String {
    // Prefer the repository root name (parent of `.rgctl`).
    if rgctl_dir.ends_with(ARTIFACT_DIR_NAME)
        && let Some(name) = rgctl_dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
    {
        return name.to_string();
    }
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo")
        .to_string()
}

/// Namespace paths with `<repo>/…` and recompute deterministic ids so identical
/// relative paths across repos do not collide in the merged backend.
fn namespace_node(mut node: Node, repo_name: &str) -> (Uuid, Node) {
    let old_id = node.id;
    node.properties
        .insert("repo_name".to_string(), repo_name.to_string());
    if let Some(fp) = node.file_path.as_deref() {
        if !fp.starts_with(&format!("{repo_name}/")) {
            let namespaced = format!("{repo_name}/{fp}");
            node.file_path = Some(namespaced.clone().into());
            if node.node_type == NodeType::File {
                node.name = namespaced.into();
            }
        }
    } else if node.node_type == NodeType::File {
        let namespaced = format!("{repo_name}/{}", node.name.as_str());
        node.name = namespaced.clone().into();
        node.file_path = Some(namespaced.into());
    }
    if node.file_path.is_some() {
        node.id = deterministic_node_id_with_discriminator(
            node.file_path.as_deref(),
            node.name.as_ref(),
            node.node_type,
            node.qualified_name.as_deref(),
            node.identity_discriminator(),
        );
    }
    (old_id, node)
}

fn candidate_norm_segments(repo_name: &str, file_path: &str, qualified_name: &str) -> Vec<String> {
    let from_file = normalize_module_path_segments(&format!("{repo_name}/{file_path}"));
    if !from_file.is_empty() {
        return from_file;
    }
    normalize_module_path_segments(qualified_name)
}

impl CrossRepoExportIndex {
    /// Build export indexes from merged (already namespaced) non-stub nodes.
    pub fn build(nodes: &[Node], node_repo: &HashMap<Uuid, usize>, repo_names: &[String]) -> Self {
        let mut index = Self::default();
        for node in nodes {
            if is_external_stub(node) || node.node_type == NodeType::File {
                continue;
            }
            let Some(&repo_idx) = node_repo.get(&node.id) else {
                continue;
            };
            let repo_name = repo_names.get(repo_idx).map(String::as_str).unwrap_or("");
            let qn = node
                .qualified_name
                .as_deref()
                .unwrap_or(node.name.as_str())
                .to_string();
            let file_path = node.file_path.as_deref().unwrap_or("").to_string();
            let simple = simple_name(node.name.as_str()).to_string();
            let norm_segments = candidate_norm_segments(repo_name, &file_path, &qn);

            index
                .by_qualified
                .entry(qn.clone())
                .or_default()
                .push((repo_idx, node.id));

            if is_route_node(node) || qn.starts_with("route::") {
                index
                    .by_route
                    .entry(qn.clone())
                    .or_default()
                    .push((repo_idx, node.id));
            }

            index
                .by_name
                .entry(simple)
                .or_default()
                .push(ExportCandidate {
                    repo_idx,
                    node_id: node.id,
                    node_type: node.node_type,
                    qualified_name: qn,
                    file_path,
                    norm_segments,
                });
        }
        index
    }

    fn resolve_parked(&self, rec: &ParkedRelationRecord, src_repo: usize) -> ResolveOutcome {
        // Tier 1 — exact cross-repo qualified hint.
        if let Some(hint) = rec.to_qualified_hint.as_deref()
            && let Some(cands) = self.by_qualified.get(hint)
        {
            let cross: Vec<_> = cands
                .iter()
                .copied()
                .filter(|(ri, _)| *ri != src_repo)
                .collect();
            if cross.len() == 1 {
                return ResolveOutcome::Resolved(cross[0].1);
            }
            if cross.len() > 1 {
                return ResolveOutcome::Ambiguous;
            }
        }

        let simple = simple_name(&rec.to_name).to_string();

        // Tier 2 — module-path trailing suffix disambiguation.
        if let Some(hint) = rec.import_module_hint.as_deref() {
            let hint_segs = normalize_module_path_segments(hint);
            if !hint_segs.is_empty()
                && let Some(cands) = self.by_name.get(&simple)
            {
                let mut best_score = 0usize;
                let mut best: Vec<Uuid> = Vec::new();
                for cand in cands.iter().filter(|c| c.repo_idx != src_repo) {
                    let score = trailing_suffix_overlap(&hint_segs, &cand.norm_segments);
                    if score < 1 {
                        continue;
                    }
                    if score > best_score {
                        best_score = score;
                        best.clear();
                        best.push(cand.node_id);
                    } else if score == best_score {
                        best.push(cand.node_id);
                    }
                }
                best.sort();
                best.dedup();
                if best.len() == 1 {
                    return ResolveOutcome::Resolved(best[0]);
                }
                if best.len() > 1 {
                    return ResolveOutcome::Ambiguous;
                }
            }
        }

        // Tier 3 — type hint + member match.
        if let Some(type_hint) = rec.to_type_hint.as_deref()
            && let Some(cands) = self.by_name.get(&simple)
        {
            let mut matches: Vec<&ExportCandidate> = cands
                .iter()
                .filter(|c| c.repo_idx != src_repo)
                .filter(|c| {
                    let q = &c.qualified_name;
                    q.contains(&format!("{type_hint}::{simple}"))
                        || q.contains(&format!("{type_hint}.{simple}"))
                        || q.eq_ignore_ascii_case(&format!("{type_hint}.{simple}"))
                        || (c.name_matches_owner(type_hint) && c.name_is(&simple))
                })
                .collect();
            if let Some(hint) = rec.import_module_hint.as_deref() {
                let hint_segs = normalize_module_path_segments(hint);
                if matches.len() > 1 && !hint_segs.is_empty() {
                    let mut best_score = 0usize;
                    let mut filtered = Vec::new();
                    for cand in &matches {
                        let score = trailing_suffix_overlap(&hint_segs, &cand.norm_segments);
                        if score > best_score {
                            best_score = score;
                            filtered.clear();
                            filtered.push(*cand);
                        } else if score == best_score && score >= 1 {
                            filtered.push(*cand);
                        }
                    }
                    if best_score >= 1 {
                        matches = filtered;
                    }
                }
            }
            if matches.len() == 1 {
                return ResolveOutcome::Resolved(matches[0].node_id);
            }
            if matches.len() > 1 {
                return ResolveOutcome::Ambiguous;
            }
        }

        // Tier 4 — unique bare name (only when no conflicting module hint mismatch).
        let tier2_attempted = rec.import_module_hint.is_some();
        if !tier2_attempted || rec.import_module_hint.is_none() {
            // Always allow tier 4 when no import hint; when hint exists but tier 2
            // missed, still allow unique bare fallback per spec.
        }
        if let Some(cands) = self.by_name.get(&simple) {
            let cross: Vec<_> = cands
                .iter()
                .filter(|c| c.repo_idx != src_repo)
                .filter(|c| node_type_compatible(rec.relation_type, c.node_type))
                .collect();
            // Spec: only when import_module_hint is none OR no tier-2 match, and
            // exactly one candidate. If ≥2 and no module hint → ambiguous.
            if cross.len() == 1 && rec.import_module_hint.is_none() {
                return ResolveOutcome::Resolved(cross[0].node_id);
            }
            if cross.len() >= 2 && rec.import_module_hint.is_none() {
                return ResolveOutcome::Ambiguous;
            }
            // Hint present but tiers 1–3 missed: do not guess among multiples.
            if cross.len() >= 2 {
                return ResolveOutcome::Ambiguous;
            }
            if cross.len() == 1 {
                // Unique after failed tier-2 — still resolve (spec tier 4 "or no candidate matched Tier 2").
                return ResolveOutcome::Resolved(cross[0].node_id);
            }
        }

        ResolveOutcome::Unresolved
    }
}

impl ExportCandidate {
    fn name_matches_owner(&self, type_hint: &str) -> bool {
        let q = self.qualified_name.to_ascii_lowercase();
        let hint = type_hint.to_ascii_lowercase();
        q.contains(&hint) || self.file_path.to_ascii_lowercase().contains(&hint)
    }

    fn name_is(&self, simple: &str) -> bool {
        self.qualified_name
            .rsplit(['.', ':', '#'])
            .next()
            .map(|s| s.eq_ignore_ascii_case(simple))
            .unwrap_or(false)
            || simple_name(self.qualified_name.as_str()).eq_ignore_ascii_case(simple)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveOutcome {
    Resolved(Uuid),
    Ambiguous,
    Unresolved,
}

/// Merge multiple discovered repositories into one columnar snapshot under `out`.
pub fn merge_repositories(repos: &[PathBuf], out: &Path) -> Result<MergeReport> {
    if repos.is_empty() {
        return Err(Error::GraphError(
            "rgctl merge requires at least one repository path".into(),
        ));
    }

    let mut repo_names = Vec::with_capacity(repos.len());
    let mut per_repo_nodes: Vec<Vec<Node>> = Vec::with_capacity(repos.len());
    let mut per_repo_edges: Vec<Vec<Edge>> = Vec::with_capacity(repos.len());
    let mut per_repo_parked: Vec<Vec<ParkedRelationRecord>> = Vec::with_capacity(repos.len());

    for repo in repos {
        let rgctl = resolve_repo_rgctl_dir(repo)?;
        let name = repo_display_name(repo, &rgctl);
        let snap = rgctl.join(SNAPSHOT_FILE);
        let backend = MmappedGraphSnapshot::open(&snap)?.hydrate_backend()?;
        let nodes = backend.all_nodes()?;
        let edges = backend.all_edges()?;
        let parked_path = rgctl.join(PARKED_RELATIONS_FILE);
        let parked = if parked_path.is_file() {
            load_parked_relations(&parked_path)?
        } else {
            Vec::new()
        };
        repo_names.push(name);
        per_repo_nodes.push(nodes);
        per_repo_edges.push(edges);
        per_repo_parked.push(parked);
    }

    // Namespace nodes; route dedup across repos.
    let mut merged_nodes: Vec<Node> = Vec::new();
    let mut node_repo: HashMap<Uuid, usize> = HashMap::new();
    let mut route_canonical: HashMap<String, Uuid> = HashMap::new();
    let mut id_remap: HashMap<Uuid, Uuid> = HashMap::new();

    for (repo_idx, nodes) in per_repo_nodes.into_iter().enumerate() {
        let repo_name = &repo_names[repo_idx];
        for node in nodes {
            let (old_id, node) = namespace_node(node, repo_name);
            if old_id != node.id {
                id_remap.insert(old_id, node.id);
            }
            if is_route_node(&node) {
                let route_key = node
                    .qualified_name
                    .as_deref()
                    .unwrap_or(node.name.as_str())
                    .to_string();
                if let Some(&canon) = route_canonical.get(&route_key) {
                    id_remap.insert(old_id, canon);
                    id_remap.insert(node.id, canon);
                    continue;
                }
                route_canonical.insert(route_key, node.id);
            }
            node_repo.insert(node.id, repo_idx);
            merged_nodes.push(node);
        }
    }

    let remap = |id: Uuid| {
        let mut cur = id;
        // Follow at most a short remap chain (namespace → route canon).
        for _ in 0..4 {
            match id_remap.get(&cur) {
                Some(&next) if next != cur => cur = next,
                _ => break,
            }
        }
        cur
    };

    let mut merged_edges: Vec<Edge> = Vec::new();
    let mut seen_edges: HashSet<(Uuid, Uuid, EdgeType)> = HashSet::new();
    for edges in per_repo_edges {
        for mut edge in edges {
            edge.from = remap(edge.from);
            edge.to = remap(edge.to);
            let key = (edge.from, edge.to, edge.edge_type);
            if seen_edges.insert(key) {
                merged_edges.push(edge);
            }
        }
    }

    // Remap parked from/stub ids for route dedup.
    let mut parked_with_repo: Vec<(usize, ParkedRelationRecord)> = Vec::new();
    let mut parked_relations_evaluated = 0usize;
    for (repo_idx, parked) in per_repo_parked.into_iter().enumerate() {
        for mut rec in parked {
            parked_relations_evaluated += 1;
            rec.from_node_id = remap(rec.from_node_id);
            if let Some(stub) = rec.stub_node_id.as_mut() {
                *stub = remap(*stub);
            }
            parked_with_repo.push((repo_idx, rec));
        }
    }

    let export_index = CrossRepoExportIndex::build(&merged_nodes, &node_repo, &repo_names);

    let mut cross_repo_edges_resolved = 0usize;
    let mut stubs_rewired = 0usize;
    let mut ambiguous_skipped = 0usize;
    let mut stubs_to_prune: HashSet<Uuid> = HashSet::new();
    let mut stub_remaining_degree: HashMap<Uuid, usize> = HashMap::new();
    let stub_ids: FxHashSet<Uuid> = merged_nodes
        .iter()
        .filter(|n| is_external_stub(n))
        .map(|n| n.id)
        .collect();

    // Count non-containment incoming edges for stubs in O(E).
    // (`Contains` from `<external>` File node is structural and must not prevent stub pruning.)
    for edge in &merged_edges {
        if !matches!(edge.edge_type, EdgeType::Contains | EdgeType::DefinedIn)
            && stub_ids.contains(&edge.to)
        {
            *stub_remaining_degree.entry(edge.to).or_default() += 1;
        }
    }

    for (src_repo, rec) in &parked_with_repo {
        match export_index.resolve_parked(rec, *src_repo) {
            ResolveOutcome::Resolved(target) => {
                let edge_type = relation_type_to_edge_type(rec.relation_type);
                if let Some(stub) = rec.stub_node_id {
                    // Remove any existing `from -> stub` edges before inserting `from -> target`
                    // so we never create duplicate `(from, target, edge_type)` edges.
                    let before_len = merged_edges.len();
                    merged_edges.retain(|e| {
                        let drop_it = e.from == rec.from_node_id && e.to == stub;
                        if drop_it {
                            seen_edges.remove(&(e.from, e.to, e.edge_type));
                        }
                        !drop_it
                    });
                    let removed = before_len - merged_edges.len();
                    if removed > 0 {
                        stubs_rewired += 1;
                        if let Some(deg) = stub_remaining_degree.get_mut(&stub) {
                            *deg = deg.saturating_sub(removed);
                            if *deg == 0 {
                                stubs_to_prune.insert(stub);
                            }
                        } else {
                            stubs_to_prune.insert(stub);
                        }
                    } else if stub_remaining_degree.get(&stub).copied().unwrap_or(0) == 0 {
                        stubs_to_prune.insert(stub);
                    }
                }

                let edge = Edge::new(rec.from_node_id, target, edge_type)
                    .with_property("cross_repo".into(), "true".into())
                    .with_property("call_site_line".into(), rec.call_site_line.to_string());
                let key = (edge.from, edge.to, edge.edge_type);
                if seen_edges.insert(key) {
                    merged_edges.push(edge);
                    cross_repo_edges_resolved += 1;
                }
            }
            ResolveOutcome::Ambiguous => ambiguous_skipped += 1,
            ResolveOutcome::Unresolved => {}
        }
    }

    if !stubs_to_prune.is_empty() {
        merged_nodes.retain(|n| !stubs_to_prune.contains(&n.id));
        merged_edges
            .retain(|e| !stubs_to_prune.contains(&e.from) && !stubs_to_prune.contains(&e.to));
        let remaining_stubs = merged_nodes.iter().any(is_external_stub);
        if !remaining_stubs {
            merged_nodes.retain(|n| {
                !(n.node_type == NodeType::File
                    && (n.file_path.as_deref() == Some("<external>")
                        || n.name.as_str() == "<external>"))
            });
        }
    }

    let mut backend = MemoryBackend::new();
    backend.insert_nodes_batch(merged_nodes)?;
    backend.insert_edges_batch(merged_edges)?;

    let out_rgctl = artifact_dir(out);
    std::fs::create_dir_all(&out_rgctl)?;
    let out_snap = out_rgctl.join(SNAPSHOT_FILE);
    write_columnar_from_backend(&backend, &out_snap)?;

    let total_nodes = backend.all_nodes()?.len();
    let total_edges = backend.all_edges()?.len();

    Ok(MergeReport {
        repos_merged: repos.len(),
        total_nodes,
        total_edges,
        parked_relations_evaluated,
        cross_repo_edges_resolved,
        stubs_rewired,
        ambiguous_skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parked_relations::save_parked_relations;
    use rgctl_graph::schema::Node;

    #[test]
    fn normalize_acme_billing_user() {
        assert_eq!(
            normalize_module_path_segments("@acme/billing/user"),
            vec!["acme", "billing", "user"]
        );
    }

    #[test]
    fn normalize_billing_src_user_ts() {
        assert_eq!(
            normalize_module_path_segments("billing/src/user.ts"),
            vec!["billing", "user"]
        );
    }

    #[test]
    fn normalize_java_package() {
        assert_eq!(
            normalize_module_path_segments("com.acme.billing.UserService"),
            vec!["com", "acme", "billing", "userservice"]
        );
    }

    #[test]
    fn normalize_strips_index_and_relative() {
        assert_eq!(
            normalize_module_path_segments("./src/lib/foo/index.ts"),
            vec!["foo"]
        );
    }

    #[test]
    fn trailing_overlap_prefers_billing() {
        let hint = normalize_module_path_segments("@acme/billing/user");
        let billing = normalize_module_path_segments("billing/src/user.ts");
        let auth = normalize_module_path_segments("auth/src/user.ts");
        assert!(trailing_suffix_overlap(&hint, &billing) > trailing_suffix_overlap(&hint, &auth));
        assert_eq!(trailing_suffix_overlap(&hint, &billing), 2);
        assert_eq!(trailing_suffix_overlap(&hint, &auth), 1);
    }

    #[test]
    fn merge_three_repos_tier2_and_ambiguous() {
        let tmp = tempfile::tempdir().unwrap();
        // Repo directory names must participate in module-path suffix scoring
        // (`billing/src/user.ts` → ["billing","user"] vs `@acme/billing/user`).
        let gateway = tmp.path().join("gateway");
        let billing = tmp.path().join("billing");
        let auth = tmp.path().join("auth");
        for p in [&gateway, &billing, &auth] {
            std::fs::create_dir_all(p.join(".rgctl")).unwrap();
        }

        // --- billing ---
        let mut billing_be = MemoryBackend::new();
        let billing_file = Node::new(NodeType::File, "src/user.ts").with_file_path("src/user.ts");
        let billing_us = Node::new(NodeType::Class, "UserService")
            .with_file_path("src/user.ts")
            .with_qualified_name("UserService");
        let billing_charge = Node::new(NodeType::Function, "charge")
            .with_file_path("src/user.ts")
            .with_qualified_name("UserService.charge");
        let billing_fmt = Node::new(NodeType::Function, "formatCurrency")
            .with_file_path("src/user.ts")
            .with_qualified_name("formatCurrency");
        billing_be
            .insert_nodes_batch(vec![billing_file, billing_us, billing_charge, billing_fmt])
            .unwrap();
        write_columnar_from_backend(&billing_be, &billing.join(".rgctl").join(SNAPSHOT_FILE))
            .unwrap();

        // --- auth ---
        let mut auth_be = MemoryBackend::new();
        let auth_file = Node::new(NodeType::File, "src/user.ts").with_file_path("src/user.ts");
        let auth_us = Node::new(NodeType::Class, "UserService")
            .with_file_path("src/user.ts")
            .with_qualified_name("UserService");
        let auth_authn = Node::new(NodeType::Function, "authenticate")
            .with_file_path("src/user.ts")
            .with_qualified_name("UserService.authenticate");
        let auth_fmt = Node::new(NodeType::Function, "formatCurrency")
            .with_file_path("src/user.ts")
            .with_qualified_name("formatCurrency");
        auth_be
            .insert_nodes_batch(vec![auth_file, auth_us, auth_authn, auth_fmt])
            .unwrap();
        write_columnar_from_backend(&auth_be, &auth.join(".rgctl").join(SNAPSHOT_FILE)).unwrap();

        // --- gateway with stub + parked ---
        let mut gw_be = MemoryBackend::new();
        let gw_file =
            Node::new(NodeType::File, "src/checkout.ts").with_file_path("src/checkout.ts");
        let checkout = Node::new(NodeType::Function, "checkout")
            .with_file_path("src/checkout.ts")
            .with_qualified_name("checkout");
        let stub = Node::new(NodeType::Class, "UserService")
            .with_file_path("<external>")
            .with_qualified_name("UserService")
            .with_property("is_external_stub".into(), "true".into());
        let external_file = Node::new(NodeType::File, "<external>").with_file_path("<external>");
        let from_id_orig = checkout.id;
        let stub_id = stub.id;
        let ext_file_id = external_file.id;
        gw_be
            .insert_nodes_batch(vec![gw_file, checkout, stub, external_file])
            .unwrap();
        // Include realistic `<external> -> stub` Contains and `stub -> <external>` DefinedIn
        // edges exactly as GraphBuilder::ensure_external_stub emits during discover.
        gw_be
            .insert_edges_batch(vec![
                Edge::new(from_id_orig, stub_id, EdgeType::Uses),
                Edge::new(ext_file_id, stub_id, EdgeType::Contains),
                Edge::new(stub_id, ext_file_id, EdgeType::DefinedIn),
            ])
            .unwrap();
        write_columnar_from_backend(&gw_be, &gateway.join(".rgctl").join(SNAPSHOT_FILE)).unwrap();

        let parked = vec![
            ParkedRelationRecord {
                from_node_id: from_id_orig,
                from_qualified_name: "checkout".into(),
                from_file: "src/checkout.ts".into(),
                to_name: "UserService".into(),
                to_qualified_hint: None,
                to_type_hint: Some("UserService".into()),
                import_module_hint: Some("@acme/billing/user".into()),
                relation_type: RelationType::Uses,
                call_site_line: 10,
                stub_node_id: Some(stub_id),
            },
            ParkedRelationRecord {
                from_node_id: from_id_orig,
                from_qualified_name: "checkout".into(),
                from_file: "src/checkout.ts".into(),
                to_name: "UserService.charge".into(),
                to_qualified_hint: None,
                to_type_hint: Some("UserService".into()),
                import_module_hint: Some("@acme/billing/user".into()),
                relation_type: RelationType::Calls,
                call_site_line: 11,
                stub_node_id: None,
            },
            ParkedRelationRecord {
                from_node_id: from_id_orig,
                from_qualified_name: "checkout".into(),
                from_file: "src/checkout.ts".into(),
                to_name: "formatCurrency".into(),
                to_qualified_hint: None,
                to_type_hint: None,
                import_module_hint: None,
                relation_type: RelationType::Calls,
                call_site_line: 12,
                stub_node_id: None,
            },
        ];
        save_parked_relations(&gateway.join(".rgctl").join(PARKED_RELATIONS_FILE), &parked)
            .unwrap();

        let out = tmp.path().join("merged");
        let report = merge_repositories(&[gateway, billing, auth], &out).unwrap();
        assert_eq!(report.repos_merged, 3);
        assert_eq!(
            report.ambiguous_skipped, 1,
            "formatCurrency must be skipped"
        );
        assert!(
            report.cross_repo_edges_resolved >= 1,
            "expected Tier-2 UserService/charge resolve, got {report:?}"
        );
        assert!(
            report.stubs_rewired >= 1,
            "expected stub rewire, got {report:?}"
        );

        let merged = MmappedGraphSnapshot::open(&out.join(".rgctl").join(SNAPSHOT_FILE))
            .unwrap()
            .hydrate_backend()
            .unwrap();
        let nodes = merged.all_nodes().unwrap();
        let edges = merged.all_edges().unwrap();

        // Stub pruned.
        assert!(
            !nodes.iter().any(|n| {
                n.properties.get("is_external_stub").map(String::as_str) == Some("true")
                    && n.name.as_str() == "UserService"
            }),
            "external UserService stub should be pruned"
        );

        let from_id = nodes
            .iter()
            .find(|n| {
                n.name.as_str() == "checkout"
                    && n.file_path
                        .as_deref()
                        .is_some_and(|p| p.starts_with("gateway/"))
            })
            .map(|n| n.id)
            .expect("namespaced checkout node");

        // Cross-repo edge to billing UserService (namespaced file).
        let user_services: Vec<_> = nodes
            .iter()
            .filter(|n| n.name.as_str() == "UserService")
            .map(|n| {
                (
                    n.id,
                    n.file_path.as_deref().unwrap_or("").to_string(),
                    n.properties.get("repo_name").cloned(),
                    n.node_type,
                )
            })
            .collect();
        let billing_us_id = user_services
            .iter()
            .find(|(_, fp, repo, _)| {
                fp.starts_with("billing/") || repo.as_deref() == Some("billing")
            })
            .map(|(id, _, _, _)| *id)
            .unwrap_or_else(|| {
                panic!("billing UserService; found UserService nodes: {user_services:?}")
            });
        // Verify cross-repo edge to billing UserService AND preserved `cross_repo == "true"` property.
        let matching_edges: Vec<_> = edges
            .iter()
            .filter(|e| e.from == from_id && e.to == billing_us_id)
            .collect();
        assert_eq!(
            matching_edges.len(),
            1,
            "expected exactly 1 deduplicated edge checkout→billing UserService; got {matching_edges:?}"
        );
        assert_eq!(
            matching_edges[0]
                .properties
                .get("cross_repo")
                .map(String::as_str),
            Some("true"),
            "expected cross_repo=true property on merged edge"
        );

        // Must not link to auth UserService for the parked Uses/Calls with billing hint.
        let auth_us_id = nodes
            .iter()
            .find(|n| {
                n.name.as_str() == "UserService"
                    && n.file_path
                        .as_deref()
                        .is_some_and(|p| p.starts_with("auth/"))
            })
            .map(|n| n.id);
        if let Some(auth_id) = auth_us_id {
            assert!(
                !edges.iter().any(|e| e.from == from_id && e.to == auth_id),
                "must not resolve to auth UserService"
            );
        }
    }

    #[test]
    fn end_to_end_graph_builder_java_and_rust_imports_park_and_merge() {
        use crate::GraphBuilder;
        use rgctl_graph::write_columnar_from_nodes_edges;
        use rgctl_plugin_api::{Relation, SourceLocation, Symbol, SymbolType};
        use serde_json::json;

        let tmp = tempfile::tempdir().unwrap();
        let gateway = tmp.path().join("gateway");
        let billing = tmp.path().join("billing");
        let auth = tmp.path().join("auth");
        for p in [&gateway, &billing, &auth] {
            std::fs::create_dir_all(p.join(".rgctl")).unwrap();
        }

        let loc = |file: &str, line: usize| SourceLocation {
            file: file.to_string(),
            start_line: line,
            end_line: line + 5,
            start_column: 0,
            end_column: 0,
        };

        // 1. Build `billing` via GraphBuilder (`src/main/java/com/acme/billing/UserService.java`)
        let b_file = "src/main/java/com/acme/billing/UserService.java";
        let mut billing_gb = GraphBuilder::new();
        let b_fid = billing_gb.ensure_file_node(Path::new(b_file));
        billing_gb.add_symbol(
            &Symbol {
                name: "UserService".into(),
                symbol_type: SymbolType::Class,
                qualified_name: Some("com.acme.billing.UserService".into()),
                location: loc(b_file, 1),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({}),
                ..Default::default()
            },
            b_fid,
        );
        let (b_nodes, b_edges) = billing_gb.into_graph();
        write_columnar_from_nodes_edges(
            b_nodes,
            b_edges,
            &billing.join(".rgctl").join(SNAPSHOT_FILE),
        )
        .unwrap();

        // 2. Build `auth` via GraphBuilder (`src/main/java/com/acme/auth/UserService.java`)
        let a_file = "src/main/java/com/acme/auth/UserService.java";
        let mut auth_gb = GraphBuilder::new();
        let a_fid = auth_gb.ensure_file_node(Path::new(a_file));
        auth_gb.add_symbol(
            &Symbol {
                name: "UserService".into(),
                symbol_type: SymbolType::Class,
                qualified_name: Some("com.acme.auth.UserService".into()),
                location: loc(a_file, 1),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({}),
                ..Default::default()
            },
            a_fid,
        );
        let (a_nodes, a_edges) = auth_gb.into_graph();
        write_columnar_from_nodes_edges(a_nodes, a_edges, &auth.join(".rgctl").join(SNAPSHOT_FILE))
            .unwrap();

        // 3. Build `gateway` via GraphBuilder with a real Java-style `import com.acme.billing.UserService;`
        //    and an unresolved `Injects` relation (`CheckoutController -> UserService`).
        let gw_file = "src/main/java/com/acme/gateway/CheckoutController.java";
        let mut gw_gb = GraphBuilder::new();
        let gw_fid = gw_gb.ensure_file_node(Path::new(gw_file));
        gw_gb.add_symbol(
            &Symbol {
                name: "import com.acme.billing.UserService;".into(),
                symbol_type: SymbolType::Import,
                qualified_name: None,
                location: loc(gw_file, 1),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({}),
                ..Default::default()
            },
            gw_fid,
        );
        gw_gb.add_symbol(
            &Symbol {
                name: "CheckoutController".into(),
                symbol_type: SymbolType::Class,
                qualified_name: Some("com.acme.gateway.CheckoutController".into()),
                location: loc(gw_file, 5),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({}),
                ..Default::default()
            },
            gw_fid,
        );
        gw_gb.build_resolution_indexes();
        gw_gb
            .add_relation(&Relation {
                from: "com.acme.gateway.CheckoutController".into(),
                to: "UserService".into(),
                relation_type: RelationType::Injects,
                location: loc(gw_file, 8),
                metadata: json!({}),
                to_qualified_hint: None,
                to_type_hint: Some("UserService".into()),
                ..Default::default()
            })
            .unwrap();

        let parked = gw_gb.take_parked_relations();
        assert_eq!(parked.len(), 1, "expected 1 parked Injects relation");
        assert_eq!(
            parked[0].import_module_hint.as_deref(),
            Some("com.acme.billing.UserService"),
            "Java import statement must strip `import ` and `;` to bind UserService"
        );
        assert!(
            parked[0].stub_node_id.is_some(),
            "Injects creates an <external> stub that must be recorded on ParkedRelationRecord"
        );

        let (gw_nodes, gw_edges) = gw_gb.into_graph();
        write_columnar_from_nodes_edges(
            gw_nodes,
            gw_edges,
            &gateway.join(".rgctl").join(SNAPSHOT_FILE),
        )
        .unwrap();
        save_parked_relations(&gateway.join(".rgctl").join(PARKED_RELATIONS_FILE), &parked)
            .unwrap();

        let out = tmp.path().join("merged-java");
        let report = merge_repositories(&[gateway, billing, auth], &out).unwrap();
        assert_eq!(report.cross_repo_edges_resolved, 1);
        assert_eq!(report.stubs_rewired, 1);
        assert_eq!(report.ambiguous_skipped, 0);
    }
}
