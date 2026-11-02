//! Graph-based dead-code detection via reachability analysis.

use crate::centrality::{NodeRole, classify_node_role};
use crate::graph_utils::PetGraphView;
use petgraph::graph::NodeIndex;
use rgctl_error::Result;
use rgctl_graph::SnapshotNodeStore;
use rgctl_graph::schema::{EdgeType, Node, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

/// JSON output schema version for dead-code reports.
pub const DEAD_CODE_SCHEMA_VERSION: u32 = 1;

const REFERENCE_EDGE_TYPES: &[EdgeType] = &[
    EdgeType::Calls,
    EdgeType::Uses,
    EdgeType::References,
    EdgeType::Instantiates,
    EdgeType::Extends,
    EdgeType::Implements,
    EdgeType::Overrides,
    EdgeType::Injects,
    EdgeType::AnnotatedWith,
    EdgeType::ExposesRoute,
    EdgeType::Modifies,
];

const ANALYZABLE_NODE_TYPES: &[NodeType] = &[
    NodeType::Function,
    NodeType::Class,
    NodeType::Struct,
    NodeType::Enum,
    NodeType::Interface,
    NodeType::Variable,
    NodeType::Module,
    NodeType::Import,
    NodeType::TypeAlias,
];

/// Confidence tier for a dead-code finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadCodeConfidence {
    /// Definite dead code (unreferenced locals, unused imports).
    High,
    /// Likely dead but may be an intentional seam.
    Medium,
    /// Possibly dead — public API with no internal callers.
    Low,
}

/// Classification of the dead-code finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadCodeKind {
    /// Import with no references from other symbols.
    UnusedImport,
    /// Function with no callers or references.
    UnreferencedFunction,
    /// Class/struct/enum/interface with no references.
    UnreferencedClass,
    /// Variable/constant with no references.
    UnreferencedVariable,
    /// Module with no imports or references.
    UnreferencedModule,
    /// Type alias with no references.
    UnreferencedType,
}

/// A single dead-code finding with evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeFinding {
    /// Symbol display name.
    pub symbol_name: String,
    /// Fully qualified name when available.
    pub qualified_name: Option<String>,
    /// Source file path.
    pub file_path: Option<String>,
    /// Definition start line (1-based).
    pub start_line: Option<usize>,
    /// Definition end line (1-based).
    pub end_line: Option<usize>,
    /// Node type label (e.g. "Function", "Import").
    pub node_type: String,
    /// Finding classification.
    pub kind: DeadCodeKind,
    /// Confidence tier.
    pub confidence: DeadCodeConfidence,
    /// Human-readable explanation.
    pub reason: String,
    /// Graph node UUID.
    pub node_id: Uuid,
}

/// Machine-readable dead-code analysis report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeReport {
    /// Output schema version.
    pub schema_version: u32,
    /// Total analyzable symbols in the graph.
    pub total_symbols: usize,
    /// Symbols reachable from entry points.
    pub reachable_symbols: usize,
    /// Symbols classified as dead code.
    pub dead_symbols: usize,
    /// Number of identified entry points.
    pub entry_points: usize,
    /// Ordered findings (high confidence first).
    pub findings: Vec<DeadCodeFinding>,
}

/// Graph-based dead-code analyzer over a snapshot store.
pub struct DeadCodeAnalyzer<'a> {
    store: &'a SnapshotNodeStore,
    view: PetGraphView,
    entry_points: Vec<String>,
    exclude_patterns: Vec<String>,
    include_tests: bool,
}

impl<'a> DeadCodeAnalyzer<'a> {
    /// Build a new analyzer over the given snapshot store.
    pub fn new(store: &'a SnapshotNodeStore) -> Result<Self> {
        let view = PetGraphView::from_snapshot_store(store)?;
        Ok(Self {
            store,
            view,
            entry_points: Vec::new(),
            exclude_patterns: Vec::new(),
            include_tests: false,
        })
    }

    /// Set explicit entry-point patterns (name, qualified_name, or file glob).
    pub fn with_entry_points(mut self, patterns: Vec<String>) -> Self {
        self.entry_points = patterns;
        self
    }

    /// Set exclusion patterns (path or name substrings).
    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    /// Whether to include test symbols in the analysis scope.
    pub fn with_include_tests(mut self, include: bool) -> Self {
        self.include_tests = include;
        self
    }

    /// Run the dead-code analysis and return a report.
    pub fn analyze(&self) -> Result<DeadCodeReport> {
        let mut analyzable_ids: HashSet<Uuid> = HashSet::new();
        let mut root_ids: HashSet<Uuid> = HashSet::new();

        let route_sources = self.collect_route_source_indices();

        for &nt in ANALYZABLE_NODE_TYPES {
            self.store.for_each_node_of_type(nt, |node| {
                let id = node.id;
                let name = node.name.as_str();
                let file_path = node.file_path.as_ref().map(|p| p.as_str());

                if self.should_exclude(name, file_path) {
                    return;
                }

                let role =
                    classify_node_role(name, file_path, node.labels.iter().any(|l| l == "stub"));

                if role == NodeRole::ExternalIntrinsic {
                    return;
                }
                if !self.include_tests && role == NodeRole::Test {
                    return;
                }

                analyzable_ids.insert(id);

                if self.is_entry_point(node, &route_sources) {
                    root_ids.insert(id);
                }
            })?;
        }

        let total_symbols = analyzable_ids.len();
        let entry_point_count = root_ids.len();

        let reachable = self.bfs_reachable(&root_ids);

        let reachable_symbols = analyzable_ids.intersection(&reachable).count();

        let mut findings: Vec<DeadCodeFinding> = Vec::new();
        for &id in &analyzable_ids {
            if reachable.contains(&id) {
                continue;
            }
            if let Ok(Some(node)) = self.store.get_node(id) {
                if let Some(finding) = self.classify_finding(&node) {
                    findings.push(finding);
                }
            }
        }

        findings.sort_by(|a, b| {
            confidence_order(&a.confidence)
                .cmp(&confidence_order(&b.confidence))
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.start_line.cmp(&b.start_line))
        });

        Ok(DeadCodeReport {
            schema_version: DEAD_CODE_SCHEMA_VERSION,
            total_symbols,
            reachable_symbols,
            dead_symbols: findings.len(),
            entry_points: entry_point_count,
            findings,
        })
    }

    fn is_entry_point(&self, node: &Node, route_source_indices: &HashSet<NodeIndex>) -> bool {
        if self.is_configured_entry_point(node) {
            return true;
        }

        let name = node.name.as_str();
        if name == "main" || name.ends_with("::main") || name == "__init__" {
            return true;
        }

        if let Some(idx) = self.view.get_index(node.id) {
            if route_source_indices.contains(&idx) {
                return true;
            }
        }

        if self.has_framework_entry_annotation(node.id) {
            return true;
        }

        if self.is_public_api_root(node) {
            return true;
        }

        false
    }

    fn collect_route_source_indices(&self) -> HashSet<NodeIndex> {
        let mut sources = HashSet::new();
        let _ = self.view.for_each_edge(|from, _to, edge_type| {
            if edge_type == EdgeType::ExposesRoute {
                sources.insert(from);
            }
        });
        sources
    }

    fn is_configured_entry_point(&self, node: &Node) -> bool {
        if self.entry_points.is_empty() {
            return false;
        }
        let name = node.name.as_str();
        let qn = node
            .qualified_name
            .as_ref()
            .map(|q| q.as_str())
            .unwrap_or("");
        let fp = node.file_path.as_ref().map(|f| f.as_str()).unwrap_or("");
        self.entry_points.iter().any(|pat| {
            name == pat
                || qn.contains(pat)
                || fp.contains(pat)
                || glob_matches(pat, name)
                || glob_matches(pat, fp)
        })
    }

    fn has_framework_entry_annotation(&self, id: Uuid) -> bool {
        let Some(idx) = self.view.get_index(id) else {
            return false;
        };
        for neighbor in self.view.outgoing_filtered(idx, &[EdgeType::AnnotatedWith]) {
            if let Some(uid) = self.view.get_uuid(neighbor) {
                if let Ok(Some(ann)) = self.store.get_node(uid) {
                    let ann_name = ann.name.as_str().to_lowercase();
                    if ann_name.contains("endpoint")
                        || ann_name.contains("controller")
                        || ann_name.contains("service")
                        || ann_name.contains("component")
                        || ann_name.contains("bean")
                        || ann_name.contains("inject")
                        || ann_name.contains("requestmapping")
                        || ann_name.contains("getmapping")
                        || ann_name.contains("postmapping")
                        || ann_name.contains("putmapping")
                        || ann_name.contains("deletemapping")
                        || ann_name.contains("restcontroller")
                        || ann_name.contains("springbootapplication")
                        || ann_name.contains("test")
                        || ann_name.contains("export")
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn is_public_api_root(&self, node: &Node) -> bool {
        if node.node_type == NodeType::Import {
            return false;
        }
        let vis = node.properties.get("visibility");
        match vis {
            Some(v) if v == "public" || v == "pub" => {
                let has_inbound_ref = self
                    .view
                    .get_index(node.id)
                    .map(|idx| {
                        self.view
                            .incoming_filtered(idx, REFERENCE_EDGE_TYPES)
                            .next()
                            .is_some()
                    })
                    .unwrap_or(false);
                !has_inbound_ref
            }
            _ => false,
        }
    }

    fn bfs_reachable(&self, roots: &HashSet<Uuid>) -> HashSet<Uuid> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        for &root_id in roots {
            if let Some(idx) = self.view.get_index(root_id) {
                visited.insert(root_id);
                queue.push_back(idx);
            }
        }

        while let Some(current) = queue.pop_front() {
            for neighbor in self.view.outgoing_filtered(current, REFERENCE_EDGE_TYPES) {
                if let Some(uid) = self.view.get_uuid(neighbor) {
                    if visited.insert(uid) {
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        visited
    }

    /// Classify an unreachable node into a finding using CSR for inbound-edge checks.
    fn classify_finding(&self, node: &Node) -> Option<DeadCodeFinding> {
        let idx = self.view.get_index(node.id)?;

        let has_non_structural_inbound = self
            .view
            .incoming_filtered(idx, REFERENCE_EDGE_TYPES)
            .next()
            .is_some();

        if has_non_structural_inbound {
            return None;
        }

        let has_any_inbound = self
            .view
            .incoming_filtered(idx, &[EdgeType::Contains, EdgeType::DefinedIn])
            .next()
            .is_some();

        let (kind, confidence, reason) = match node.node_type {
            NodeType::Import => {
                let outgoing_uses = self
                    .view
                    .outgoing_filtered(
                        idx,
                        &[EdgeType::Calls, EdgeType::Uses, EdgeType::References],
                    )
                    .count();
                if !has_non_structural_inbound && (!has_any_inbound || outgoing_uses == 0) {
                    (
                        DeadCodeKind::UnusedImport,
                        DeadCodeConfidence::High,
                        "Import has no references from other symbols".to_string(),
                    )
                } else {
                    return None;
                }
            }
            NodeType::Function => {
                let vis = node.properties.get("visibility");
                let is_public = matches!(vis, Some(v) if v == "public" || v == "pub");
                if is_public {
                    (
                        DeadCodeKind::UnreferencedFunction,
                        DeadCodeConfidence::Low,
                        "Public function with no internal callers — may be an intentional API seam"
                            .to_string(),
                    )
                } else {
                    (
                        DeadCodeKind::UnreferencedFunction,
                        DeadCodeConfidence::High,
                        "Function has no callers or references".to_string(),
                    )
                }
            }
            NodeType::Class | NodeType::Struct | NodeType::Interface | NodeType::Enum => {
                let vis = node.properties.get("visibility");
                let is_public = matches!(vis, Some(v) if v == "public" || v == "pub");
                let conf = if is_public {
                    DeadCodeConfidence::Low
                } else {
                    DeadCodeConfidence::Medium
                };
                (
                    DeadCodeKind::UnreferencedClass,
                    conf,
                    format!(
                        "{:?} has no references, instantiations, or extensions",
                        node.node_type
                    ),
                )
            }
            NodeType::Variable => (
                DeadCodeKind::UnreferencedVariable,
                DeadCodeConfidence::High,
                "Variable/constant has no references".to_string(),
            ),
            NodeType::Module => (
                DeadCodeKind::UnreferencedModule,
                DeadCodeConfidence::Medium,
                "Module has no imports or references from other modules".to_string(),
            ),
            NodeType::TypeAlias => (
                DeadCodeKind::UnreferencedType,
                DeadCodeConfidence::Medium,
                "Type alias has no references".to_string(),
            ),
            _ => return None,
        };

        Some(DeadCodeFinding {
            symbol_name: node.name.as_str().to_string(),
            qualified_name: node.qualified_name.as_ref().map(|q| q.as_str().to_string()),
            file_path: node.file_path.as_ref().map(|p| p.as_str().to_string()),
            start_line: node.start_line,
            end_line: node.end_line,
            node_type: format!("{:?}", node.node_type),
            kind,
            confidence,
            reason,
            node_id: node.id,
        })
    }

    fn should_exclude(&self, name: &str, file_path: Option<&str>) -> bool {
        let fp = file_path.unwrap_or("");
        self.exclude_patterns.iter().any(|pat| {
            fp.contains(pat)
                || name.contains(pat)
                || glob_matches(pat, fp)
                || glob_matches(pat, name)
        })
    }
}

fn confidence_order(c: &DeadCodeConfidence) -> u8 {
    match c {
        DeadCodeConfidence::High => 0,
        DeadCodeConfidence::Medium => 1,
        DeadCodeConfidence::Low => 2,
    }
}

fn glob_matches(pattern: &str, text: &str) -> bool {
    if !pattern.contains('*') {
        return text == pattern;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 2 {
        let (prefix, suffix) = (parts[0], parts[1]);
        return text.starts_with(prefix) && text.ends_with(suffix);
    }
    text.contains(pattern.trim_matches('*'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::{GraphBackend, MemoryBackend};
    use rgctl_graph::schema::{Edge, Node};

    fn temp_snapshot(backend: &MemoryBackend) -> (SnapshotNodeStore, std::path::PathBuf) {
        // `write_columnar_from_backend` (not `PreparedGraphSnapshot::write_to_path`) so node
        // rows are written sorted by id: `SnapshotNodeStore::get_node` binary-searches the
        // node column, and `write_to_path` currently emits `from_backend` HashMap order
        // (unsorted), which makes lookups miss nodes nondeterministically. Tracked in a
        // separate issue; once `write_columnar_to_path` sorts internally this can revert.
        let path = std::env::temp_dir().join(format!("dc_{}.bin", Uuid::new_v4()));
        rgctl_graph::write_columnar_from_backend(backend, &path).unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        (store, path)
    }

    fn build_test_backend() -> MemoryBackend {
        let mut backend = MemoryBackend::new();

        let main_fn = Node::new(NodeType::Function, "main")
            .with_file_path("src/main.rs")
            .with_location(1, 10);
        let used_fn = Node::new(NodeType::Function, "process")
            .with_file_path("src/lib.rs")
            .with_location(1, 5);
        let dead_fn = Node::new(NodeType::Function, "unused_helper")
            .with_file_path("src/lib.rs")
            .with_location(10, 15);
        let used_import = Node::new(NodeType::Import, "std::io")
            .with_file_path("src/main.rs")
            .with_location(1, 1);
        let unused_import = Node::new(NodeType::Import, "std::collections::HashMap")
            .with_file_path("src/main.rs")
            .with_location(2, 2);

        let main_id = main_fn.id;
        let used_id = used_fn.id;
        let _dead_id = dead_fn.id;
        let used_imp_id = used_import.id;
        let _unused_imp_id = unused_import.id;

        backend.insert_node(main_fn).unwrap();
        backend.insert_node(used_fn).unwrap();
        backend.insert_node(dead_fn).unwrap();
        backend.insert_node(used_import).unwrap();
        backend.insert_node(unused_import).unwrap();

        backend
            .insert_edge(Edge::new(main_id, used_id, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(main_id, used_imp_id, EdgeType::Uses))
            .unwrap();

        backend
    }

    #[test]
    fn detects_unused_import() {
        let backend = build_test_backend();
        let (store, path) = temp_snapshot(&backend);

        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();

        let unused_imports: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.kind == DeadCodeKind::UnusedImport)
            .collect();
        assert!(
            !unused_imports.is_empty(),
            "should detect at least one unused import"
        );
        assert!(
            unused_imports
                .iter()
                .any(|f| f.symbol_name.contains("HashMap")),
            "should detect unused HashMap import"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn detects_unreferenced_function() {
        let backend = build_test_backend();
        let (store, path) = temp_snapshot(&backend);

        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();

        let dead_fns: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.kind == DeadCodeKind::UnreferencedFunction)
            .collect();
        assert!(
            dead_fns.iter().any(|f| f.symbol_name == "unused_helper"),
            "should detect unused_helper as dead code"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn main_is_entry_point() {
        let backend = build_test_backend();
        let (store, path) = temp_snapshot(&backend);

        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();

        assert!(
            !report.findings.iter().any(|f| f.symbol_name == "main"),
            "main should not be reported as dead code"
        );
        assert!(
            !report.findings.iter().any(|f| f.symbol_name == "process"),
            "process (called from main) should not be dead code"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn public_symbol_is_entry_point_root() {
        let mut backend = MemoryBackend::new();
        let public_fn = Node::new(NodeType::Function, "api_handler")
            .with_file_path("src/api.rs")
            .with_location(1, 10)
            .with_property("visibility".to_string(), "pub".to_string());
        backend.insert_node(public_fn).unwrap();

        let (store, path) = temp_snapshot(&backend);

        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.symbol_name == "api_handler"),
            "public symbol with no inbound refs is an entry-point root, not dead code"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_only_symbols_excluded_by_default() {
        let mut backend = MemoryBackend::new();
        let test_fn = Node::new(NodeType::Function, "test_something")
            .with_file_path("tests/test_api.rs")
            .with_location(1, 10);
        backend.insert_node(test_fn).unwrap();

        let (store, path) = temp_snapshot(&backend);

        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.symbol_name == "test_something"),
            "test symbols should be excluded by default"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn glob_matching_works() {
        assert!(glob_matches("*.rs", "main.rs"));
        assert!(glob_matches("test_*", "test_foo"));
        assert!(!glob_matches("*.rs", "main.py"));
        assert!(glob_matches("foo", "foo"));
        assert!(!glob_matches("foo", "bar"));
    }
}
