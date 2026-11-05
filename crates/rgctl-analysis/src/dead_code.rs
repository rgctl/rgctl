//! Graph-based dead-code detection via reachability analysis.

use crate::centrality::{NodeRole, classify_node_role};
use crate::graph_utils::PetGraphView;
use petgraph::graph::NodeIndex;
use rgctl_error::Result;
use rgctl_graph::SnapshotNodeStore;
use rgctl_graph::schema::{EdgeType, Node, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
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

/// Edge types traversed during BFS reachability (includes Contains so that
/// entry-point classes propagate reachability to their methods).
const BFS_EDGE_TYPES: &[EdgeType] = &[
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
    EdgeType::Contains,
];

fn is_analyzable_type(nt: NodeType) -> bool {
    matches!(
        nt,
        NodeType::Function
            | NodeType::Class
            | NodeType::Struct
            | NodeType::Enum
            | NodeType::Interface
            | NodeType::Variable
            | NodeType::Module
            | NodeType::Import
            | NodeType::TypeAlias
    )
}

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

/// Summary statistics for a dead-code report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeSummary {
    /// Total analyzable symbols in the graph.
    pub total_symbols: usize,
    /// Symbols reachable from entry points.
    pub reachable_symbols: usize,
    /// Symbols classified as dead code.
    pub dead_symbols: usize,
    /// Number of identified entry points.
    pub entry_points: usize,
}

/// Machine-readable dead-code analysis report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeReport {
    /// Output schema version.
    pub schema_version: u32,
    /// Summary statistics.
    pub summary: DeadCodeSummary,
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
        let n = self.view.node_count();
        let mut analyzable = vec![false; n];
        let mut root_indices: Vec<NodeIndex> = Vec::new();

        let framework_entry_indices = self.collect_framework_entry_indices();

        // Single pass: collect analyzable nodes and roots.
        self.store.for_each_node_seq(|node| {
            if !is_analyzable_type(node.node_type) {
                return;
            }

            let name = node.name.as_str();
            let file_path = node.file_path.as_ref().map(|p| p.as_str());

            if self.should_exclude(name, file_path) {
                return;
            }

            let role = classify_node_role(name, file_path, node.labels.iter().any(|l| l == "stub"));

            if role == NodeRole::ExternalIntrinsic {
                return;
            }
            if !self.include_tests && role == NodeRole::Test {
                return;
            }

            let Some(idx) = self.view.get_index(node.id) else {
                return;
            };

            analyzable[idx.index()] = true;

            if self.is_entry_point(node, idx, &framework_entry_indices) {
                root_indices.push(idx);
            }
        })?;

        let total_symbols = analyzable.iter().filter(|&&a| a).count();
        let entry_point_count = root_indices.len();

        let reachable = self.bfs_reachable(&root_indices);

        let reachable_symbols = (0..n).filter(|&i| analyzable[i] && reachable[i]).count();

        let mut findings: Vec<DeadCodeFinding> = Vec::new();
        for (i, &is_analyzable) in analyzable.iter().enumerate() {
            if !is_analyzable || reachable[i] {
                continue;
            }
            let idx = NodeIndex::new(i);
            if let Some(uid) = self.view.get_uuid(idx) {
                if let Ok(Some(node)) = self.store.get_node(uid) {
                    if let Some(finding) = self.classify_finding(&node, idx) {
                        findings.push(finding);
                    }
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
            summary: DeadCodeSummary {
                total_symbols,
                reachable_symbols,
                dead_symbols: findings.len(),
                entry_points: entry_point_count,
            },
            findings,
        })
    }

    fn is_entry_point(
        &self,
        node: &Node,
        idx: NodeIndex,
        framework_entry_indices: &[bool],
    ) -> bool {
        if self.is_configured_entry_point(node) {
            return true;
        }

        let name = node.name.as_str();
        if name == "main" || name.ends_with("::main") || name == "__init__" {
            return true;
        }

        if self
            .view
            .outgoing_filtered(idx, &[EdgeType::ExposesRoute])
            .next()
            .is_some()
        {
            return true;
        }

        if idx.index() < framework_entry_indices.len() && framework_entry_indices[idx.index()] {
            return true;
        }

        false
    }

    /// Pre-collect which NodeIndices are framework entry points by scanning
    /// Annotation nodes once, then walking their incoming AnnotatedWith edges.
    fn collect_framework_entry_indices(&self) -> Vec<bool> {
        let n = self.view.node_count();
        let mut is_entry = vec![false; n];

        let mut annotation_indices: Vec<NodeIndex> = Vec::new();
        for (idx, uid) in self.view.index_uuid_iter() {
            if let Ok(Some(node)) = self.store.get_node(uid) {
                if node.node_type == NodeType::Annotation {
                    let ann_name = node.name.as_str().to_lowercase();
                    if is_framework_annotation(&ann_name) {
                        annotation_indices.push(idx);
                    }
                }
            }
        }

        for ann_idx in &annotation_indices {
            for neighbor in self
                .view
                .incoming_filtered(*ann_idx, &[EdgeType::AnnotatedWith])
            {
                if neighbor.index() < n {
                    is_entry[neighbor.index()] = true;
                }
            }
        }

        is_entry
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

    fn bfs_reachable(&self, root_indices: &[NodeIndex]) -> Vec<bool> {
        let n = self.view.node_count();
        let mut visited = vec![false; n];
        let mut queue = VecDeque::with_capacity(root_indices.len());

        for &root in root_indices {
            if !visited[root.index()] {
                visited[root.index()] = true;
                queue.push_back(root);
            }
        }

        while let Some(current) = queue.pop_front() {
            for neighbor in self.view.outgoing_filtered(current, BFS_EDGE_TYPES) {
                let ni = neighbor.index();
                if !visited[ni] {
                    visited[ni] = true;
                    queue.push_back(neighbor);
                }
            }
        }

        visited
    }

    /// Classify an unreachable node into a finding using CSR for inbound-edge checks.
    fn classify_finding(&self, node: &Node, idx: NodeIndex) -> Option<DeadCodeFinding> {
        let has_non_structural_inbound = self
            .view
            .incoming_filtered(idx, REFERENCE_EDGE_TYPES)
            .next()
            .is_some();

        if has_non_structural_inbound {
            return None;
        }

        let (kind, confidence, reason) = match node.node_type {
            NodeType::Import => {
                let has_outbound_ref = self
                    .view
                    .outgoing_filtered(
                        idx,
                        &[EdgeType::Uses, EdgeType::References, EdgeType::Calls],
                    )
                    .next()
                    .is_some();
                if !has_outbound_ref {
                    return None;
                }
                (
                    DeadCodeKind::UnusedImport,
                    DeadCodeConfidence::High,
                    "Import resolves to an internal symbol but has no references".to_string(),
                )
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

fn is_framework_annotation(name: &str) -> bool {
    name.contains("endpoint")
        || name.contains("controller")
        || name.contains("service")
        || name.contains("component")
        || name.contains("bean")
        || name.contains("inject")
        || name.contains("requestmapping")
        || name.contains("getmapping")
        || name.contains("postmapping")
        || name.contains("putmapping")
        || name.contains("deletemapping")
        || name.contains("restcontroller")
        || name.contains("springbootapplication")
        || name.contains("test")
        || name.contains("export")
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
    fn unreferenced_public_fn_reported_as_low_confidence() {
        let mut backend = MemoryBackend::new();
        let public_fn = Node::new(NodeType::Function, "api_handler")
            .with_file_path("src/api.rs")
            .with_location(1, 10)
            .with_property("visibility".to_string(), "pub".to_string());
        backend.insert_node(public_fn).unwrap();

        let (store, path) = temp_snapshot(&backend);

        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        let finding = report
            .findings
            .iter()
            .find(|f| f.symbol_name == "api_handler");
        assert!(
            finding.is_some(),
            "unreferenced public function should be reported as dead code"
        );
        assert_eq!(
            finding.unwrap().confidence,
            DeadCodeConfidence::Low,
            "unreferenced public function should have Low confidence"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn external_import_not_flagged() {
        let mut backend = MemoryBackend::new();
        let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
        let ext_import =
            Node::new(NodeType::Import, "std::collections::HashMap").with_file_path("src/main.rs");
        backend.insert_node(main_fn).unwrap();
        backend.insert_node(ext_import).unwrap();

        let (store, path) = temp_snapshot(&backend);

        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.kind == DeadCodeKind::UnusedImport),
            "external imports without Imports edges should not be flagged"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn entry_point_class_methods_reachable_via_contains() {
        let mut backend = MemoryBackend::new();
        let class = Node::new(NodeType::Class, "MyController")
            .with_file_path("src/controller.rs")
            .with_property("visibility".to_string(), "pub".to_string());
        let annotation = Node::new(NodeType::Annotation, "RestController");
        let method =
            Node::new(NodeType::Function, "handle_request").with_file_path("src/controller.rs");

        let class_id = class.id;
        let ann_id = annotation.id;
        let method_id = method.id;

        backend.insert_node(class).unwrap();
        backend.insert_node(annotation).unwrap();
        backend.insert_node(method).unwrap();
        backend
            .insert_edge(Edge::new(class_id, ann_id, EdgeType::AnnotatedWith))
            .unwrap();
        backend
            .insert_edge(Edge::new(class_id, method_id, EdgeType::Contains))
            .unwrap();

        let (store, path) = temp_snapshot(&backend);

        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.symbol_name == "handle_request"),
            "methods on entry-point classes should be reachable via Contains"
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
