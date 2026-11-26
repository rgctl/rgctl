//! Graph-based dead-code detection via reachability analysis.

use crate::centrality::{NodeRole, classify_node_role};
use crate::hash_maps::FxHashSet;
use crate::structural_topology::StructuralTopology;
use rgctl_error::Result;
use rgctl_graph::SnapshotNodeStore;
use rgctl_graph::schema::{EdgeType, Node, NodeType};
use rgctl_plugin_api::SymbolVisibility;
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
    /// Symbols classified as dead code (after confidence filtering, before
    /// `--limit` truncation).
    pub dead_symbols: usize,
    /// Number of identified entry points.
    pub entry_points: usize,
    /// Findings actually returned after `--limit` truncation. Equal to
    /// `dead_symbols` unless a limit was applied, which keeps the total
    /// dead count visible alongside the truncated payload.
    pub returned: usize,
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
    /// Typed CSR topology (one UUID map; no `PetGraphView` duplicate index).
    topo: StructuralTopology,
    entry_points: Vec<String>,
    exclude_patterns: Vec<String>,
    include_tests: bool,
    /// Optional confidence tier filter (`[high, medium, low]` keep-set).
    /// Applied *before* finding allocation so excluded tiers never pay
    /// string formatting or cold decodes.
    confidence_filter: Option<Vec<DeadCodeConfidence>>,
}

impl<'a> DeadCodeAnalyzer<'a> {
    /// Build a new analyzer over the given snapshot store.
    pub fn new(store: &'a SnapshotNodeStore) -> Result<Self> {
        let topo = StructuralTopology::from_snapshot_store(store)?;
        Ok(Self {
            store,
            topo,
            entry_points: Vec::new(),
            exclude_patterns: Vec::new(),
            include_tests: false,
            confidence_filter: None,
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

    /// Restrict findings to the given confidence tiers (empty = all).
    pub fn with_confidence_filter(mut self, tiers: Vec<DeadCodeConfidence>) -> Self {
        if tiers.is_empty() || tiers.len() == 3 {
            self.confidence_filter = None;
        } else {
            self.confidence_filter = Some(tiers);
        }
        self
    }

    fn keeps_confidence(&self, c: DeadCodeConfidence) -> bool {
        self.confidence_filter
            .as_ref()
            .is_none_or(|keep| keep.contains(&c))
    }

    /// Run the dead-code analysis and return a report.
    pub fn analyze(&self) -> Result<DeadCodeReport> {
        let n = self.topo.node_count();
        let mut analyzable = vec![false; n];
        let mut candidate_roots = vec![false; n];
        let mut framework_annotations: Vec<usize> = Vec::new();
        let mut app_trait_indices: Vec<usize> = Vec::new();
        // Scala-file indices — collected only on legacy v1 snapshots (the
        // columnar path checks the source file directly at scan time).
        let mut scala_file_indices: Vec<usize> = Vec::new();
        // Elixir OTP callback candidates (`(idx, module prefix)`) — resolved
        // against `use GenServer` / `@behaviour` edges after Pass 1 so a
        // plain `.ex` function that merely shares a callback name is not
        // seeded as a root.
        let mut otp_callback_candidates: Vec<(usize, String)> = Vec::new();
        let mut behaviour_targets: Vec<usize> = Vec::new();

        // Pass 1: single scan collecting analyzable nodes, root candidates,
        // and framework annotation indices. On columnar v2+ snapshots this is
        // zero-cold-touch (hot `NodeRow` columns only — no `NodeExtension`
        // bincode deserializations; the dense CSR index equals the columnar
        // row index). Legacy v1 snapshots fall back to `for_each_node_seq`.
        if let Some(col) = self.store.columnar() {
            for i in 0..col.node_count() {
                let nt = col.node_type_at(i)?;
                let name = col.node_name_str(i)?;
                if nt == NodeType::Annotation {
                    if is_framework_annotation(name) {
                        framework_annotations.push(i);
                    }
                    continue;
                }
                if is_type_kind(nt) && simple_name_segment(name) == "App" {
                    // Scala `object Main extends App` candidates (also matches
                    // qualified `scala.App`) — resolved against incoming
                    // `Extends`/`Implements` edges after Pass 1.
                    app_trait_indices.push(i);
                }
                if is_otp_behaviour_target(name) {
                    // Elixir behaviour targets (`use GenServer` resolves to a
                    // Class/Import node — matched by simple name, not type).
                    behaviour_targets.push(i);
                }
                if !is_analyzable_type(nt) {
                    continue;
                }
                let file_path = col.node_file_path_str(i)?;
                if self.should_exclude(name, file_path) {
                    continue;
                }
                // External stubs carry `file_path == "<external>"` (INV-7).
                let role = classify_node_role(name, file_path, file_path == Some("<external>"));
                if role == NodeRole::ExternalIntrinsic {
                    continue;
                }
                if !self.include_tests && role == NodeRole::Test {
                    continue;
                }
                let is_elixir = is_elixir_source(file_path);
                let needs_qualified_name = !self.entry_points.is_empty()
                    || (is_elixir
                        && (name == "start" || is_otp_callback_name(simple_name_segment(name))));
                let qualified_name = if needs_qualified_name {
                    col.node_qualified_name_str(i)?
                } else {
                    None
                };
                if is_elixir
                    && is_otp_callback_name(name)
                    && let Some(module) = qualified_name.and_then(elixir_module_prefix)
                {
                    // Deferred: only a root once the enclosing module is
                    // known to adopt a behaviour — but explicit
                    // `--entry-point` patterns always win.
                    if self.is_configured_entry_point(name, qualified_name, file_path) {
                        candidate_roots[i] = true;
                    } else {
                        otp_callback_candidates.push((i, module.to_string()));
                    }
                    analyzable[i] = true;
                    continue;
                }
                analyzable[i] = true;
                candidate_roots[i] = self.is_root_candidate(name, qualified_name, file_path);
            }
        } else {
            self.store.for_each_node_seq(|node| {
                if node.node_type == NodeType::Annotation {
                    if is_framework_annotation(node.name.as_str()) {
                        if let Some(idx) = self.legacy_index(node.id) {
                            framework_annotations.push(idx);
                        }
                    }
                    return;
                }
                if is_type_kind(node.node_type) && simple_name_segment(node.name.as_str()) == "App"
                {
                    if let Some(idx) = self.legacy_index(node.id) {
                        app_trait_indices.push(idx);
                    }
                }
                if is_otp_behaviour_target(node.name.as_str()) {
                    if let Some(idx) = self.legacy_index(node.id) {
                        behaviour_targets.push(idx);
                    }
                }
                if !is_analyzable_type(node.node_type) {
                    return;
                }
                let name = node.name.as_str();
                let file_path = node.file_path.as_ref().map(|p| p.as_str());
                if self.should_exclude(name, file_path) {
                    return;
                }
                // External stubs carry `file_path == "<external>"`, plus the
                // legacy `is_external_stub` property marker (INV-7).
                let is_external_stub = file_path == Some("<external>")
                    || node.get_property("is_external_stub") == Some("true");
                let role = classify_node_role(name, file_path, is_external_stub);
                if role == NodeRole::ExternalIntrinsic {
                    return;
                }
                if !self.include_tests && role == NodeRole::Test {
                    return;
                }
                let Some(idx) = self.legacy_index(node.id) else {
                    return;
                };
                if file_path.is_some_and(is_scala_source) {
                    scala_file_indices.push(idx);
                }
                let is_elixir = is_elixir_source(file_path);
                if is_elixir
                    && is_otp_callback_name(name)
                    && let Some(module) = node
                        .qualified_name
                        .as_deref()
                        .and_then(elixir_module_prefix)
                {
                    // Deferred: only a root once the enclosing module is
                    // known to adopt a behaviour — but explicit
                    // `--entry-point` patterns always win.
                    if self.is_configured_entry_point(
                        name,
                        node.qualified_name.as_deref(),
                        file_path,
                    ) {
                        candidate_roots[idx] = true;
                    } else {
                        otp_callback_candidates.push((idx, module.to_string()));
                    }
                    analyzable[idx] = true;
                    return;
                }
                analyzable[idx] = true;
                candidate_roots[idx] =
                    self.is_root_candidate(name, node.qualified_name.as_deref(), file_path);
            })?;
        }

        // Elixir OTP callback seeding: resolve the deferred candidates
        // against modules that actually adopt a behaviour (`use GenServer`
        // emits an `Extends` edge, `@behaviour GenServer` an `Implements`
        // edge from the module node). Enclosing modules are matched by the
        // qualified-name prefix recorded in Pass 1.
        if !otp_callback_candidates.is_empty() {
            let mut otp_module_indices: Vec<u32> = Vec::new();
            for target in &behaviour_targets {
                for edge_type in [EdgeType::Extends, EdgeType::Implements] {
                    for src in self.topo.in_filtered(*target as u32, &[edge_type]) {
                        otp_module_indices.push(src);
                    }
                }
            }
            if !otp_module_indices.is_empty() {
                let mut otp_modules: FxHashSet<String> = FxHashSet::default();
                if let Some(col) = self.store.columnar() {
                    // Index directly by the resolved module rows — no full
                    // `node_count` re-scan, no linear `contains`.
                    for &i in &otp_module_indices {
                        let i = i as usize;
                        if i < n
                            && col.node_type_at(i)? == NodeType::Module
                            && is_elixir_source(col.node_file_path_str(i)?)
                            && let Some(qn) = col.node_qualified_name_str(i)?
                        {
                            otp_modules.insert(qn.to_string());
                        }
                    }
                } else {
                    self.store.for_each_node_seq(|node| {
                        if node.node_type != NodeType::Module
                            || !is_elixir_source(node.file_path.as_deref())
                        {
                            return;
                        }
                        let Some(idx) = self.legacy_index(node.id) else {
                            return;
                        };
                        if otp_module_indices.contains(&(idx as u32)) {
                            if let Some(qn) = node.qualified_name.as_deref() {
                                otp_modules.insert(qn.to_string());
                            }
                        }
                    })?;
                }
                for (i, module) in &otp_callback_candidates {
                    if otp_modules.contains(module.as_str()) {
                        candidate_roots[*i] = true;
                    }
                }
            }
        }

        // Framework entry points: nodes with an incoming `AnnotatedWith` edge
        // from a framework annotation.
        let mut is_framework_entry = vec![false; n];
        for ann in framework_annotations {
            for neighbor in self
                .topo
                .in_filtered(ann as u32, &[EdgeType::AnnotatedWith])
            {
                let ni = neighbor as usize;
                if ni < n {
                    is_framework_entry[ni] = true;
                }
            }
        }

        // Scala `extends App` (and `extends Base with App`, which emits
        // `Implements`): an object subclassing the `App` trait executes its
        // body at startup, so it (and everything it calls) is reachable.
        let col = self.store.columnar();
        for app in app_trait_indices {
            for edge_type in [EdgeType::Extends, EdgeType::Implements] {
                for neighbor in self.topo.in_filtered(app as u32, &[edge_type]) {
                    let ni = neighbor as usize;
                    if ni >= n {
                        continue;
                    }
                    let is_scala = match col {
                        Some(col) => is_scala_source(col.node_file_path_str(ni)?.unwrap_or("")),
                        None => scala_file_indices.contains(&ni),
                    };
                    if is_scala {
                        is_framework_entry[ni] = true;
                    }
                }
            }
        }

        let mut root_indices: Vec<u32> = Vec::new();

        for i in 0..n {
            if !analyzable[i] {
                continue;
            }
            if candidate_roots[i]
                || is_framework_entry[i]
                || self
                    .topo
                    .out_filtered(i as u32, &[EdgeType::ExposesRoute])
                    .next()
                    .is_some()
            {
                root_indices.push(i as u32);
            }
        }

        // Type-root → member propagation. Real `discover` graphs carry
        // `Contains` edges only for File→Symbol (never Type→Method), so a
        // root *type* (annotated class, `@main` struct, `extends App`
        // object) must propagate reachability to its member symbols by
        // qualified-name prefix within the same file. Propagated members
        // seed the BFS but are not counted as entry points, and *private*
        // members are not propagated — DI containers never invoke
        // unannotated private methods directly (annotated lifecycle methods
        // are already seeded in Pass 1 via their own `AnnotatedWith` edge,
        // and called helpers are reached via `Calls` BFS).
        let mut propagated_roots: Vec<u32> = Vec::new();
        self.propagate_type_roots_to_members(
            &analyzable,
            &candidate_roots,
            &is_framework_entry,
            &mut propagated_roots,
        )?;
        // Entry-point count is captured *before* seeding the BFS with
        // propagated members so `summary.entry_points` reflects real
        // entry points only.
        let entry_point_count = root_indices.len();
        root_indices.extend_from_slice(&propagated_roots);

        let total_symbols = analyzable.iter().filter(|&&a| a).count();

        let reachable = self.bfs_reachable(&root_indices);

        let reachable_symbols = (0..n).filter(|&i| analyzable[i] && reachable[i]).count();

        // Pass 3: classify unreachable nodes. All prechecks (inbound
        // reference edges, import outbound references, declaration role,
        // confidence tier) run on hot columns / the CSR *before* any
        // allocation, so excluded tiers and bodiless declarations never pay
        // string formatting — and the columnar path never decodes cold
        // `NodeExtension` payloads at all. Isolated dead cycles (e.g.
        // `dead_a <-> dead_b` mutual recursion with zero external callers)
        // are reported as root SCCs while downstream callees of a dead root
        // stay suppressed.
        let (inbound_members, cycle_members) = self.isolated_dead_cycles(&analyzable, &reachable);
        let mut findings: Vec<DeadCodeFinding> = Vec::new();
        for (i, &is_analyzable) in analyzable.iter().enumerate() {
            if !is_analyzable || reachable[i] {
                continue;
            }
            let has_inbound = inbound_members.binary_search(&i).is_ok();
            let in_cycle = cycle_members.binary_search(&i).is_ok();
            if has_inbound && !in_cycle {
                continue;
            }
            if let Some(col) = col {
                if col.node_type_at(i)? == NodeType::Import
                    && !self.has_outbound_reference(i as u32)
                {
                    continue;
                }
                // Bodiless declarations (`.h` prototypes, interface
                // signatures) are API surface, not dead code.
                if col.node_is_declaration_at(i)? {
                    continue;
                }
                let nt = col.node_type_at(i)?;
                let is_public = col.node_visibility_at(i)? == SymbolVisibility::Public;
                let Some((kind, confidence)) = Self::classify_tier_of(nt, is_public) else {
                    continue;
                };
                if !self.keeps_confidence(confidence) {
                    continue;
                }
                // Hot-only construction: every finding field (name,
                // qualified name, file, lines, id, type, visibility) comes
                // from direct row + zero-copy string reads — no
                // `SharedStr` interning, no signature / span-tail decode.
                let (start_line, end_line) = col.node_lines_at(i)?;
                findings.push(DeadCodeFinding {
                    symbol_name: col.node_name_str(i)?.to_string(),
                    qualified_name: col.node_qualified_name_str(i)?.map(str::to_string),
                    file_path: col.node_file_path_str(i)?.map(str::to_string),
                    start_line,
                    end_line,
                    node_type: node_type_label(nt).to_string(),
                    kind,
                    confidence,
                    reason: finding_reason(kind, confidence, nt, in_cycle),
                    node_id: col.node_id_at(i)?,
                });
            } else if let Some(idx) = self.legacy_index_of(i) {
                if let Ok(Some(node)) = self.store.get_node(idx) {
                    if node.node_type == NodeType::Import && !self.has_outbound_reference(i as u32)
                    {
                        continue;
                    }
                    if node.get_property("callable_role") == Some("declaration") {
                        continue;
                    }
                    let Some((kind, confidence)) =
                        Self::classify_tier_of(node.node_type, is_public_symbol(&node))
                    else {
                        continue;
                    };
                    if !self.keeps_confidence(confidence) {
                        continue;
                    }
                    findings.push(self.build_finding(
                        &node,
                        node.qualified_name.as_deref(),
                        kind,
                        confidence,
                        in_cycle,
                    ));
                }
            }
        }

        findings.sort_by(|a, b| {
            confidence_order(&a.confidence)
                .cmp(&confidence_order(&b.confidence))
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.start_line.cmp(&b.start_line))
        });

        let dead_symbols = findings.len();
        Ok(DeadCodeReport {
            schema_version: DEAD_CODE_SCHEMA_VERSION,
            summary: DeadCodeSummary {
                total_symbols,
                reachable_symbols,
                dead_symbols,
                entry_points: entry_point_count,
                returned: dead_symbols,
            },
            findings,
        })
    }

    /// Dense CSR index for a node UUID (legacy v1 snapshots only).
    fn legacy_index(&self, id: Uuid) -> Option<usize> {
        self.topo.get_index(id).map(|i| i as usize)
    }

    /// UUID for a dense CSR index (legacy v1 snapshots only).
    fn legacy_index_of(&self, idx: usize) -> Option<Uuid> {
        self.topo.index_to_uuid.get(idx).copied()
    }

    /// Propagate root status from root *types* (Class/Struct/Interface/Enum/
    /// Module) to their member symbols. Real graphs have no Type→Method
    /// `Contains` edges, so membership is matched by qualified-name prefix
    /// within the same source file. Propagated members are appended to
    /// `propagated_roots` (BFS seeds, not entry-point counts).
    #[allow(clippy::too_many_arguments)]
    fn propagate_type_roots_to_members(
        &self,
        analyzable: &[bool],
        candidate_roots: &[bool],
        is_framework_entry: &[bool],
        propagated_roots: &mut Vec<u32>,
    ) -> Result<()> {
        let n = analyzable.len();
        // (qualified name, file) of root types — same-file + prefix match.
        let mut root_types: Vec<(String, String)> = Vec::new();
        if let Some(col) = self.store.columnar() {
            for i in 0..n {
                if !analyzable[i] || !(candidate_roots[i] || is_framework_entry[i]) {
                    continue;
                }
                if !is_type_kind(col.node_type_at(i)?) {
                    continue;
                }
                if let (Some(qn), Some(fp)) =
                    (col.node_qualified_name_str(i)?, col.node_file_path_str(i)?)
                {
                    root_types.push((qn.to_string(), fp.to_string()));
                }
            }
        } else {
            self.store.for_each_node_seq(|node| {
                let Some(idx) = self.legacy_index(node.id) else {
                    return;
                };
                if idx >= n
                    || !analyzable[idx]
                    || !(candidate_roots[idx] || is_framework_entry[idx])
                    || !is_type_kind(node.node_type)
                {
                    return;
                }
                if let (Some(qn), Some(fp)) =
                    (node.qualified_name.as_deref(), node.file_path.as_deref())
                {
                    root_types.push((qn.to_string(), fp.to_string()));
                }
            })?;
        }
        if root_types.is_empty() {
            return Ok(());
        }
        let roots: FxHashSet<(&str, &str)> = root_types
            .iter()
            .map(|(qn, fp)| (qn.as_str(), fp.as_str()))
            .collect();
        // One scan: mark member symbols whose qualified name has a root-type
        // ancestor in the same file. Private members are NOT propagated —
        // framework containers never invoke unannotated private methods
        // directly, so a dead `private` helper inside a `@Service` class
        // must stay reportable.
        if let Some(col) = self.store.columnar() {
            for i in 0..n {
                if !analyzable[i] || candidate_roots[i] || is_framework_entry[i] {
                    continue;
                }
                if col.node_visibility_at(i)? == SymbolVisibility::Private {
                    continue;
                }
                let (Some(qn), Some(fp)) =
                    (col.node_qualified_name_str(i)?, col.node_file_path_str(i)?)
                else {
                    continue;
                };
                if has_root_ancestor_in_file(qn, fp, &roots) {
                    propagated_roots.push(i as u32);
                }
            }
        } else {
            self.store.for_each_node_seq(|node| {
                let Some(idx) = self.legacy_index(node.id) else {
                    return;
                };
                if idx >= n || !analyzable[idx] || candidate_roots[idx] || is_framework_entry[idx] {
                    return;
                }
                if node.visibility == SymbolVisibility::Private {
                    return;
                }
                let (Some(qn), Some(fp)) =
                    (node.qualified_name.as_deref(), node.file_path.as_deref())
                else {
                    return;
                };
                if has_root_ancestor_in_file(qn, fp, &roots) {
                    propagated_roots.push(idx as u32);
                }
            })?;
        }
        Ok(())
    }

    /// Cheap root predicates evaluated during Pass 1: language-level
    /// entry-point names and user-configured entry-point patterns.
    fn is_root_candidate(
        &self,
        name: &str,
        qualified_name: Option<&str>,
        file_path: Option<&str>,
    ) -> bool {
        // Language-level entry points, matched on the simple (last) name
        // segment: lowercase `main` is the near-universal convention (Rust
        // `main` / `crate::main`, Go, C/C++, Bash/Lua/Dart `main`).
        let simple = simple_name_segment(name);
        if simple == "main" || simple == "__init__" {
            return true;
        }
        // C# / F# `Program.Main` — PascalCase `Main`, gated to `.cs`/`.fs`
        // sources so a dead class named `Main` in Java/Kotlin/etc. stays
        // reportable.
        if simple == "Main" && is_csharp_like_source(file_path) {
            return true;
        }
        if is_elixir_source(file_path) {
            // `Application.start/2` — the supervisor tree entry of an OTP
            // app (mix wires the module named `…Application` by convention).
            // GenServer/Supervisor behaviour callbacks (`init`,
            // `handle_call`, …) are seeded separately in Pass 1, gated on
            // the enclosing module actually adopting the behaviour.
            if simple == "start"
                && qualified_name.is_some_and(|qn| qn.ends_with("Application.start"))
            {
                return true;
            }
        }
        self.is_configured_entry_point(name, qualified_name, file_path)
    }

    /// Members of isolated dead cycles among the unreachable analyzable
    /// nodes: SCCs over `REFERENCE_EDGE_TYPES` edges with **zero** inbound
    /// reference edges from outside the SCC (e.g. `dead_a <-> dead_b` mutual
    /// recursion, or a self-recursive function nobody calls). Cycles that a
    /// dead root calls into stay suppressed (downstream callees), matching
    /// the root-only reporting semantics.
    ///
    /// Runs entirely on the CSR (no node materialization). Allocations are
    /// bounded by the dead subgraph — the returned member list and the
    /// internal dense arrays are sized by the number of unreachable symbols
    /// with inbound reference edges, never by the full node count.
    fn isolated_dead_cycles(
        &self,
        analyzable: &[bool],
        reachable: &[bool],
    ) -> (Vec<usize>, Vec<usize>) {
        let n = analyzable.len();
        // Dead nodes that have at least one inbound reference edge — the only
        // candidates for cycle membership (dead roots are reported directly).
        // Ascending by construction, so binary search maps graph index →
        // dense member index without an O(n) side table. Returned to the
        // caller so Pass 3 reuses the same inbound scan instead of paying a
        // second CSR pass.
        let members: Vec<usize> = (0..n)
            .filter(|&i| {
                analyzable[i]
                    && !reachable[i]
                    && self
                        .topo
                        .in_filtered(i as u32, REFERENCE_EDGE_TYPES)
                        .next()
                        .is_some()
            })
            .collect();
        let m = members.len();
        if m == 0 {
            return (Vec::new(), Vec::new());
        }
        let member_pos =
            |graph_idx: usize| -> Option<usize> { members.binary_search(&graph_idx).ok() };

        let mut adj: Vec<Vec<u32>> = vec![Vec::new(); m];
        let mut self_loop = vec![false; m];
        for (ci, &i) in members.iter().enumerate() {
            for nb in self.topo.out_filtered(i as u32, REFERENCE_EDGE_TYPES) {
                let nj = nb as usize;
                if nj == i {
                    self_loop[ci] = true;
                }
                if let Some(cj) = member_pos(nj) {
                    adj[ci].push(cj as u32);
                }
            }
        }

        // Iterative Tarjan SCC (bounded, no recursion).
        let mut index_counter: u32 = 0;
        let mut indices = vec![u32::MAX; m];
        let mut lowlink = vec![u32::MAX; m];
        let mut on_stack = vec![false; m];
        let mut scc_stack: Vec<u32> = Vec::new();
        let mut scc_of: Vec<i64> = vec![-1; m];
        let mut scc_count: usize = 0;
        let mut return_members: Vec<usize> = Vec::new();
        // (member, next-neighbor position) work stack.
        let mut work: Vec<(u32, usize)> = Vec::new();
        for root in 0..m {
            if indices[root] != u32::MAX {
                continue;
            }
            work.push((root as u32, 0));
            while let Some(&mut (v, ref mut next)) = work.last_mut() {
                if *next == 0 {
                    indices[v as usize] = index_counter;
                    lowlink[v as usize] = index_counter;
                    index_counter += 1;
                    scc_stack.push(v);
                    on_stack[v as usize] = true;
                }
                if *next < adj[v as usize].len() {
                    let w = adj[v as usize][*next];
                    *next += 1;
                    if indices[w as usize] == u32::MAX {
                        work.push((w, 0));
                    } else if on_stack[w as usize] {
                        lowlink[v as usize] = lowlink[v as usize].min(indices[w as usize]);
                    }
                } else {
                    work.pop();
                    if let Some(&(parent, _)) = work.last() {
                        lowlink[parent as usize] =
                            lowlink[parent as usize].min(lowlink[v as usize]);
                    }
                    if lowlink[v as usize] == indices[v as usize] {
                        // Pop one SCC off the Tarjan stack.
                        let mut scc: Vec<usize> = Vec::new();
                        while let Some(w) = scc_stack.pop() {
                            on_stack[w as usize] = false;
                            scc_of[w as usize] = scc_count as i64;
                            scc.push(w as usize);
                            if w == v {
                                break;
                            }
                        }
                        // Root SCC: ≥2 members, or a self-recursive singleton,
                        // with no inbound reference edge from outside the SCC
                        // (edges from live, excluded, or non-member nodes all
                        // count as external and suppress the cycle).
                        let cyclic = scc.len() > 1 || self_loop[scc[0]];
                        if cyclic {
                            let external_inbound = scc.iter().any(|&ci| {
                                self.topo
                                    .in_filtered(members[ci] as u32, REFERENCE_EDGE_TYPES)
                                    .any(|src| {
                                        // `scc_of` is only assigned when an SCC
                                        // is popped: a source in a not-yet-
                                        // popped SCC still carries `-1`, which
                                        // casts to a value != `scc_count` and
                                        // correctly counts as external.
                                        member_pos(src as usize)
                                            .is_none_or(|cs| scc_of[cs] as usize != scc_count)
                                    })
                            });
                            if !external_inbound {
                                return_members.extend(scc.iter().map(|&ci| members[ci]));
                            }
                        }
                        scc_count += 1;
                    }
                }
            }
        }
        // Ascending order so callers can use binary search for membership.
        return_members.sort_unstable();
        (members, return_members)
    }

    fn is_configured_entry_point(
        &self,
        name: &str,
        qualified_name: Option<&str>,
        file_path: Option<&str>,
    ) -> bool {
        if self.entry_points.is_empty() {
            return false;
        }
        let qn = qualified_name.unwrap_or("");
        let fp = file_path.unwrap_or("");
        self.entry_points.iter().any(|pat| {
            name == pat
                || qn.contains(pat)
                || fp.contains(pat)
                || glob_matches(pat, name)
                || glob_matches(pat, fp)
        })
    }

    fn bfs_reachable(&self, root_indices: &[u32]) -> Vec<bool> {
        let n = self.topo.node_count();
        let mut visited = vec![false; n];
        let mut queue = VecDeque::with_capacity(root_indices.len());

        for &root in root_indices {
            if !visited[root as usize] {
                visited[root as usize] = true;
                queue.push_back(root);
            }
        }

        while let Some(current) = queue.pop_front() {
            for neighbor in self.topo.out_filtered(current, BFS_EDGE_TYPES) {
                let ni = neighbor as usize;
                if !visited[ni] {
                    visited[ni] = true;
                    queue.push_back(neighbor);
                }
            }
        }

        visited
    }

    /// Whether an import node has at least one outbound
    /// `Uses`/`References`/`Calls` edge (i.e. it resolves to an internal
    /// symbol). Checked against the CSR before any cold node materialization.
    fn has_outbound_reference(&self, idx: u32) -> bool {
        self.topo
            .out_filtered(
                idx,
                &[EdgeType::Uses, EdgeType::References, EdgeType::Calls],
            )
            .next()
            .is_some()
    }

    /// Classify an unreachable node's kind and confidence tier from hot
    /// data only (node type + typed visibility). Returns `None` for
    /// non-analyzable leftovers. Called *before* any finding allocation so
    /// filtered tiers cost nothing.
    fn classify_tier_of(
        node_type: NodeType,
        is_public: bool,
    ) -> Option<(DeadCodeKind, DeadCodeConfidence)> {
        Some(match node_type {
            NodeType::Import => (DeadCodeKind::UnusedImport, DeadCodeConfidence::High),
            NodeType::Function => (
                DeadCodeKind::UnreferencedFunction,
                if is_public {
                    DeadCodeConfidence::Low
                } else {
                    DeadCodeConfidence::High
                },
            ),
            NodeType::Class | NodeType::Struct | NodeType::Interface | NodeType::Enum => (
                DeadCodeKind::UnreferencedClass,
                if is_public {
                    DeadCodeConfidence::Low
                } else {
                    DeadCodeConfidence::Medium
                },
            ),
            NodeType::Variable => (
                DeadCodeKind::UnreferencedVariable,
                if is_public {
                    DeadCodeConfidence::Low
                } else {
                    DeadCodeConfidence::High
                },
            ),
            NodeType::Module => (DeadCodeKind::UnreferencedModule, DeadCodeConfidence::Medium),
            NodeType::TypeAlias => (
                DeadCodeKind::UnreferencedType,
                if is_public {
                    DeadCodeConfidence::Low
                } else {
                    DeadCodeConfidence::Medium
                },
            ),
            _ => return None,
        })
    }

    /// Build the finding payload once kind/confidence are known (legacy v1
    /// snapshots: full `Node` materialization; the columnar path builds
    /// findings inline from hot columns).
    fn build_finding(
        &self,
        node: &Node,
        qualified_name: Option<&str>,
        kind: DeadCodeKind,
        confidence: DeadCodeConfidence,
        in_isolated_cycle: bool,
    ) -> DeadCodeFinding {
        DeadCodeFinding {
            symbol_name: node.name.as_str().to_string(),
            qualified_name: qualified_name.map(str::to_string),
            file_path: node.file_path.as_ref().map(|p| p.as_str().to_string()),
            start_line: node.start_line,
            end_line: node.end_line,
            node_type: node_type_label(node.node_type).to_string(),
            kind,
            confidence,
            reason: finding_reason(kind, confidence, node.node_type, in_isolated_cycle),
            node_id: node.id,
        }
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

/// Framework / lifecycle annotation names that mark a symbol as externally
/// invoked (by a DI container, web framework, scheduler, message broker, or
/// test runner) and therefore a reachability root.
///
/// Matching is anchored on the annotation's *simple* name (the last segment
/// of a possibly qualified name, with any leading `@` stripped) and is ASCII
/// case-insensitive without allocating, so look-alike annotations
/// (e.g. `@TestVisible`, `@BeanFactory`) do not accidentally mark symbols as
/// framework entry points.
const FRAMEWORK_ANNOTATIONS: &[&str] = &[
    // Spring
    "restcontroller",
    "controller",
    "controlleradvice",
    "service",
    "component",
    "bean",
    "repository",
    "configuration",
    "springbootapplication",
    "autowired",
    "scheduled",
    "eventlistener",
    // Jakarta / Java EE
    "messagedriven",
    "stateless",
    "stateful",
    "singleton",
    // JAX-RS HTTP verbs + path
    "path",
    "get",
    "post",
    "put",
    "delete",
    "patch",
    // Swift `@main` / Scala 3 `@main` program entry attributes
    "main",
    // DI / generic entry points
    "inject",
    "export",
];

/// Last segment of a possibly qualified symbol name (`a::b.c#d` → `d`),
/// borrowed with no allocation.
fn simple_name_segment(name: &str) -> &str {
    match name.rfind(['.', ':', '#', '\\']) {
        Some(pos) => &name[pos + 1..],
        None => name,
    }
}

/// ASCII case-insensitive suffix check on a borrowed slice.
fn ends_with_ignore_ascii_case(haystack: &str, suffix: &str) -> bool {
    haystack.len() >= suffix.len()
        && haystack[haystack.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

fn is_elixir_source(file_path: Option<&str>) -> bool {
    file_path.is_some_and(|p| p.ends_with(".ex") || p.ends_with(".exs"))
}

fn is_scala_source(file_path: &str) -> bool {
    file_path.ends_with(".scala")
}

/// C# / F# source files — the ecosystems where the program entry point is
/// the PascalCase `Main` method (`Program.Main`).
fn is_csharp_like_source(file_path: Option<&str>) -> bool {
    file_path.is_some_and(|p| p.ends_with(".cs") || p.ends_with(".fs"))
}

/// Elixir OTP behaviour callbacks invoked by the BEAM (GenServer /
/// Supervisor), never by intra-repo callers. Seeded only when the enclosing
/// module adopts the behaviour (see `analyze`).
fn is_otp_callback_name(name: &str) -> bool {
    matches!(
        name,
        "init"
            | "handle_call"
            | "handle_cast"
            | "handle_info"
            | "handle_continue"
            | "terminate"
            | "code_change"
            | "format_status"
            | "child_spec"
    )
}

/// Standard OTP behaviour modules whose callbacks the BEAM invokes. Matched
/// on the simple name segment so qualified aliases (`Elixir.GenServer`)
/// resolve too.
fn is_otp_behaviour_target(name: &str) -> bool {
    matches!(
        simple_name_segment(name),
        "GenServer"
            | "Supervisor"
            | "DynamicSupervisor"
            | "Application"
            | "Agent"
            | "Task"
            | "gen_statem"
    )
}

/// Type-like nodes whose root status propagates to member symbols
/// (`Contains` edges are File→Symbol only in real discover graphs).
fn is_type_kind(nt: NodeType) -> bool {
    matches!(
        nt,
        NodeType::Class
            | NodeType::Struct
            | NodeType::Interface
            | NodeType::Enum
            | NodeType::Module
    )
}

/// Whether `qn` has an ancestor segment chain present in `roots` for the
/// same file. Ancestors are `qn` substrings (any separator depth), so
/// `MyApp.Inner.method` matches the root type `MyApp`.
fn has_root_ancestor_in_file(qn: &str, file: &str, roots: &FxHashSet<(&str, &str)>) -> bool {
    let mut rest = qn;
    while let Some(pos) = rest.rfind(['.', ':', '#']) {
        rest = &rest[..pos];
        if roots.contains(&(rest, file)) {
            return true;
        }
    }
    false
}

/// Module segment of an Elixir qualified name (`Server.handle_call` →
/// `Server`). Returns `None` for top-level (non-module) functions.
fn elixir_module_prefix(qualified_name: &str) -> Option<&str> {
    let (prefix, last) = qualified_name.rsplit_once('.')?;
    (!prefix.is_empty() && !last.is_empty()).then_some(prefix)
}

fn is_framework_annotation(name: &str) -> bool {
    // Strip a leading `@` and any qualifier (`org.springframework…`,
    // `Module::sub`, `#member`) to get the simple annotation name.
    let stripped = name.trim_start_matches('@');
    let simple = match stripped.rfind(['.', ':', '#', '/']) {
        Some(pos) => &stripped[pos + 1..],
        None => stripped,
    };
    if simple.is_empty() {
        return false;
    }
    FRAMEWORK_ANNOTATIONS
        .iter()
        .any(|a| simple.eq_ignore_ascii_case(a))
        // Spring mapping verbs: `@RequestMapping`, `@GetMapping`,
        // `@PatchMapping`, …
        || ends_with_ignore_ascii_case(simple, "mapping")
        // JUnit family: `@Test`, `@ParameterizedTest`, `@RepeatedTest`, …
        || ends_with_ignore_ascii_case(simple, "test")
        // Framework endpoint annotations: `@Endpoint`,
        // `@ControllerEndpoint`, `@RestControllerEndpoint`, …
        || ends_with_ignore_ascii_case(simple, "endpoint")
}

/// Human-readable explanation for a finding tier (shared by the hot
/// columnar path and the legacy materialized path).
fn finding_reason(
    kind: DeadCodeKind,
    confidence: DeadCodeConfidence,
    node_type: NodeType,
    in_isolated_cycle: bool,
) -> String {
    let cycle_note = if in_isolated_cycle {
        " (isolated dead cycle — no callers outside the cycle)"
    } else {
        ""
    };
    match kind {
        DeadCodeKind::UnusedImport => {
            "Import resolves to an internal symbol but has no references".to_string()
        }
        DeadCodeKind::UnreferencedFunction => {
            if confidence == DeadCodeConfidence::Low {
                format!(
                    "Public/exported function with no internal callers — may be an \
                     intentional API seam{cycle_note}"
                )
            } else {
                format!("Function has no callers or references{cycle_note}")
            }
        }
        DeadCodeKind::UnreferencedClass => format!(
            "{} has no references, instantiations, or extensions{cycle_note}",
            node_type_label(node_type)
        ),
        DeadCodeKind::UnreferencedVariable => {
            if confidence == DeadCodeConfidence::Low {
                format!(
                    "Public/exported variable/constant with no references — may be an \
                     intentional API seam{cycle_note}"
                )
            } else {
                format!("Variable/constant has no references{cycle_note}")
            }
        }
        DeadCodeKind::UnreferencedModule => {
            format!("Module has no imports or references from other modules{cycle_note}")
        }
        DeadCodeKind::UnreferencedType => {
            if confidence == DeadCodeConfidence::Low {
                format!(
                    "Public/exported type alias with no references — may be an \
                     intentional API seam{cycle_note}"
                )
            } else {
                format!("Type alias has no references{cycle_note}")
            }
        }
    }
}

/// Human-readable label for a node type (matches the JSON `node_type`
/// field's `Debug` spelling without relying on `{:?}` formatting).
fn node_type_label(nt: NodeType) -> &'static str {
    match nt {
        NodeType::Function => "Function",
        NodeType::Class => "Class",
        NodeType::Struct => "Struct",
        NodeType::Enum => "Enum",
        NodeType::Interface => "Interface",
        NodeType::Variable => "Variable",
        NodeType::Module => "Module",
        NodeType::Import => "Import",
        NodeType::TypeAlias => "TypeAlias",
        NodeType::Annotation => "Annotation",
        _ => "Symbol",
    }
}

/// Whether a symbol is public / exported in its language's terms.
///
/// Normalizes the `visibility` property across plugins and adds
/// language-specific export rules for plugins that do not set the property:
///
/// - Go: identifiers starting with an ASCII uppercase letter are exported.
/// - Elixir: `def` / `defmacro` (public) vs `defp` / `defmacrop` (private),
///   recorded in the `kind` property.
/// - C: functions without the `static` storage class have external linkage.
fn is_public_symbol(node: &Node) -> bool {
    match node.visibility {
        SymbolVisibility::Public => return true,
        SymbolVisibility::Private | SymbolVisibility::Internal => return false,
        // Unknown: fall through to legacy token inspection so mixed-version
        // (pre-typed-visibility) snapshots still classify sanely.
        SymbolVisibility::Unknown => {}
    }
    if let Some(vis) = node.get_property("visibility") {
        match vis {
            "public" | "pub" | "exported" | "open" | "global" => return true,
            "private" | "internal" | "fileprivate" | "protected" | "package"
            | "package-private" => return false,
            _ => {
                tracing::trace!(
                    symbol = %node.name.as_str(),
                    visibility = vis,
                    "unrecognized visibility marker; using modifier tokens"
                );
            }
        }
    }
    // `GraphBuilder` writes visibility keywords into `properties["modifiers"]`
    // (joined tokens) — scan them before the language rules.
    if let Some(mods) = node.get_property("modifiers") {
        for token in mods.split(' ') {
            match token.trim() {
                "pub" | "public" | "export" | "exported" | "open" | "global" => return true,
                "private" | "protected" | "fileprivate" | "internal" | "pub(crate)"
                | "pub(super)" | "package" | "package-private" => return false,
                _ => {}
            }
        }
    }
    match node.get_property("language") {
        Some("go") => node
            .name
            .as_str()
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase()),
        Some("elixir") => matches!(node.get_property("kind"), Some("def") | Some("defmacro")),
        _ => false,
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
    let Some((prefix, suffix)) = pattern.split_once('*') else {
        return text == pattern;
    };
    if !suffix.contains('*') {
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
            .with_visibility(SymbolVisibility::Public);
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
    fn entry_point_class_methods_reachable_via_qn_prefix() {
        // Real `discover` graphs carry `Contains` only for File→Symbol (never
        // Type→Method), so methods of entry-point types are reached through
        // qualified-name prefix propagation.
        let mut backend = MemoryBackend::new();
        let class = Node::new(NodeType::Class, "MyController")
            .with_file_path("src/controller.rs")
            .with_qualified_name("MyController")
            .with_visibility(SymbolVisibility::Public);
        let annotation = Node::new(NodeType::Annotation, "RestController");
        let method = Node::new(NodeType::Function, "handle_request")
            .with_file_path("src/controller.rs")
            .with_qualified_name("MyController.handle_request");

        let class_id = class.id;
        let ann_id = annotation.id;

        backend.insert_node(class).unwrap();
        backend.insert_node(annotation).unwrap();
        backend.insert_node(method).unwrap();
        backend
            .insert_edge(Edge::new(class_id, ann_id, EdgeType::AnnotatedWith))
            .unwrap();

        let (store, path) = temp_snapshot(&backend);

        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.symbol_name == "handle_request"),
            "methods on entry-point classes should be reachable via qn prefix propagation"
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

    #[test]
    fn framework_annotation_matching_is_anchored() {
        // Exact simple names (case-insensitive).
        assert!(is_framework_annotation("RestController"));
        assert!(is_framework_annotation("Service"));
        assert!(is_framework_annotation("Bean"));
        assert!(is_framework_annotation("Inject"));
        assert!(is_framework_annotation("Export"));
        // Qualified names resolve to their last segment.
        assert!(is_framework_annotation(
            "org.springframework.stereotype.Service"
        ));
        assert!(is_framework_annotation("jakarta.inject.Inject"));
        // Mapping verbs via the `mapping` suffix rule.
        assert!(is_framework_annotation("RequestMapping"));
        assert!(is_framework_annotation("GetMapping"));
        assert!(is_framework_annotation("PatchMapping"));
        // JUnit family via the `test` suffix rule.
        assert!(is_framework_annotation("Test"));
        assert!(is_framework_annotation("ParameterizedTest"));
        // Endpoint annotations via the `endpoint` suffix rule.
        assert!(is_framework_annotation("Endpoint"));
        assert!(is_framework_annotation("RestControllerEndpoint"));
        // Lifecycle / DI annotations from the expanded list.
        assert!(is_framework_annotation("Autowired"));
        assert!(is_framework_annotation("Scheduled"));
        assert!(is_framework_annotation("EventListener"));
        assert!(is_framework_annotation("MessageDriven"));
        assert!(is_framework_annotation("Stateless"));
    }

    #[test]
    fn lookalike_annotations_are_not_framework_entries() {
        // Substring look-alikes must NOT match (anchored simple-name check).
        assert!(!is_framework_annotation("TestVisible"));
        assert!(!is_framework_annotation("TestConfiguration"));
        assert!(!is_framework_annotation("BeanFactory"));
        assert!(!is_framework_annotation("ServiceLocator"));
        assert!(!is_framework_annotation("ExportedResource"));
        assert!(!is_framework_annotation("InjectionUtils"));
        assert!(!is_framework_annotation("EndpointProvider"));
        assert!(!is_framework_annotation(""));
        assert!(!is_framework_annotation("@"));
    }

    #[test]
    fn lookalike_annotation_does_not_create_entry_point() {
        // `@TestVisible` shares a substring with `@Test` but is not a test
        // runner entry point: the annotated class and its method must stay
        // in the dead-code report.
        let mut backend = MemoryBackend::new();
        let class = Node::new(NodeType::Class, "MyInternal").with_file_path("src/internal.rs");
        let annotation = Node::new(NodeType::Annotation, "TestVisible");
        let method = Node::new(NodeType::Function, "helper").with_file_path("src/internal.rs");

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
            report.findings.iter().any(|f| f.symbol_name == "helper"),
            "@TestVisible must not make the class a framework entry point"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pascal_case_main_is_entry_point() {
        // C# `Program.Main` — PascalCase `Main` in a `.cs` source must seed
        // reachability.
        let mut backend = MemoryBackend::new();
        let main_fn = Node::new(NodeType::Function, "Main").with_file_path("src/Program.cs");
        let helper = Node::new(NodeType::Function, "Run").with_file_path("src/Program.cs");
        let main_id = main_fn.id;
        let helper_id = helper.id;
        backend.insert_node(main_fn).unwrap();
        backend.insert_node(helper).unwrap();
        backend
            .insert_edge(Edge::new(main_id, helper_id, EdgeType::Calls))
            .unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report.findings.iter().any(|f| f.symbol_name == "Main"),
            "PascalCase Main in .cs must be an entry point"
        );
        assert!(
            !report.findings.iter().any(|f| f.symbol_name == "Run"),
            "callees of Main must be reachable"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pascal_case_main_outside_csharp_is_not_entry_point() {
        // A dead class named `Main` in Java (or any non-C#/F# language) must
        // stay reportable — the PascalCase gate is `.cs`/`.fs`-only.
        let mut backend = MemoryBackend::new();
        let main_class = Node::new(NodeType::Class, "Main").with_file_path("src/Main.java");
        backend.insert_node(main_class).unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            report.findings.iter().any(|f| f.symbol_name == "Main"),
            "a Java class named Main is not a program entry point"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn elixir_otp_callbacks_seeded_only_for_behaviour_modules() {
        // GenServer behaviour callbacks are invoked by the BEAM — but only
        // in modules that actually adopt the behaviour (`use GenServer`
        // emits an `Extends` edge from the module).
        let mut backend = MemoryBackend::new();
        let gen_server = Node::new(NodeType::Module, "GenServer").with_file_path("<external>");
        let server_mod = Node::new(NodeType::Module, "Server")
            .with_file_path("lib/server.ex")
            .with_qualified_name("Server");
        let handle_call = Node::new(NodeType::Function, "handle_call")
            .with_file_path("lib/server.ex")
            .with_qualified_name("Server.handle_call");
        let plain_mod = Node::new(NodeType::Module, "Plain")
            .with_file_path("lib/plain.ex")
            .with_qualified_name("Plain");
        let plain_handle = Node::new(NodeType::Function, "handle_call")
            .with_file_path("lib/plain.ex")
            .with_qualified_name("Plain.handle_call");

        let gs_id = gen_server.id;
        let server_id = server_mod.id;
        backend.insert_node(gen_server).unwrap();
        backend.insert_node(server_mod).unwrap();
        backend.insert_node(handle_call).unwrap();
        backend.insert_node(plain_mod).unwrap();
        backend.insert_node(plain_handle).unwrap();
        backend
            .insert_edge(Edge::new(server_id, gs_id, EdgeType::Extends))
            .unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.symbol_name == "handle_call"
                    && f.qualified_name.as_deref() == Some("Server.handle_call")),
            "handle_call in a `use GenServer` module must be an entry point"
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.symbol_name == "handle_call"
                    && f.qualified_name.as_deref() == Some("Plain.handle_call")),
            "handle_call in a module without the behaviour stays dead-code candidate"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn elixir_application_start_is_entry_point() {
        let mut backend = MemoryBackend::new();
        let start = Node::new(NodeType::Function, "start")
            .with_file_path("lib/my_app.ex")
            .with_qualified_name("MyApp.Application.start");
        backend.insert_node(start).unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report.findings.iter().any(|f| f.symbol_name == "start"),
            "Application.start/2 must be an entry point"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scala_extends_app_is_entry_point() {
        // `object Main extends App` — the subclass executes its body at
        // startup, so it and its callees are reachable.
        let mut backend = MemoryBackend::new();
        let app_trait = Node::new(NodeType::Class, "App").with_file_path("<external>");
        let main_obj = Node::new(NodeType::Class, "Main")
            .with_file_path("src/Main.scala")
            .with_qualified_name("Main");
        let method = Node::new(NodeType::Function, "run")
            .with_file_path("src/Main.scala")
            .with_qualified_name("Main.run");
        let app_id = app_trait.id;
        let main_id = main_obj.id;
        backend.insert_node(app_trait).unwrap();
        backend.insert_node(main_obj).unwrap();
        backend.insert_node(method).unwrap();
        backend
            .insert_edge(Edge::new(main_id, app_id, EdgeType::Extends))
            .unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report.findings.iter().any(|f| f.symbol_name == "run"),
            "methods of `extends App` objects must be reachable"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn main_annotation_creates_entry_point() {
        // Swift / Scala 3 `@main` — the annotated declaration is the
        // program entry point.
        let mut backend = MemoryBackend::new();
        let annotation = Node::new(NodeType::Annotation, "main");
        let app_struct = Node::new(NodeType::Struct, "MyApp")
            .with_file_path("src/Main.swift")
            .with_qualified_name("MyApp");
        let method = Node::new(NodeType::Function, "start")
            .with_file_path("src/Main.swift")
            .with_qualified_name("MyApp.start");
        let ann_id = annotation.id;
        let struct_id = app_struct.id;
        backend.insert_node(annotation).unwrap();
        backend.insert_node(app_struct).unwrap();
        backend.insert_node(method).unwrap();
        backend
            .insert_edge(Edge::new(struct_id, ann_id, EdgeType::AnnotatedWith))
            .unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            !report.findings.iter().any(|f| f.symbol_name == "start"),
            "@main-annotated declarations must be framework entry points"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn go_exported_function_is_low_confidence() {
        // Go exports are encoded as uppercase-initial identifiers.
        let mut backend = MemoryBackend::new();
        let exported = Node::new(NodeType::Function, "CorrectnessRoot")
            .with_file_path("src/harness.go")
            .with_visibility(SymbolVisibility::Public);
        backend.insert_node(exported).unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        let finding = report
            .findings
            .iter()
            .find(|f| f.symbol_name == "CorrectnessRoot")
            .expect("exported Go function still reported");
        assert_eq!(
            finding.confidence,
            DeadCodeConfidence::Low,
            "unreferenced Go exports classify as Low"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn elixir_defp_is_high_confidence() {
        // Elixir `def` (public) vs `defp` (private), recorded in `kind`.
        let mut backend = MemoryBackend::new();
        let public = Node::new(NodeType::Function, "public_fn")
            .with_file_path("lib/mod.ex")
            .with_visibility(SymbolVisibility::Public);
        let private = Node::new(NodeType::Function, "private_fn")
            .with_file_path("lib/mod.ex")
            .with_visibility(SymbolVisibility::Private);
        backend.insert_node(public).unwrap();
        backend.insert_node(private).unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        let pub_f = report
            .findings
            .iter()
            .find(|f| f.symbol_name == "public_fn")
            .expect("def function reported");
        let priv_f = report
            .findings
            .iter()
            .find(|f| f.symbol_name == "private_fn")
            .expect("defp function reported");
        assert_eq!(pub_f.confidence, DeadCodeConfidence::Low);
        assert_eq!(priv_f.confidence, DeadCodeConfidence::High);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn c_static_vs_external_linkage_confidence() {
        // C `static` functions have internal linkage (High); non-static
        // definitions may be called from other translation units (Low).
        let mut backend = MemoryBackend::new();
        let static_fn = Node::new(NodeType::Function, "helper")
            .with_file_path("src/a.c")
            .with_visibility(SymbolVisibility::Private);
        let extern_fn = Node::new(NodeType::Function, "api")
            .with_file_path("src/a.c")
            .with_visibility(SymbolVisibility::Public);
        backend.insert_node(static_fn).unwrap();
        backend.insert_node(extern_fn).unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        let s = report
            .findings
            .iter()
            .find(|f| f.symbol_name == "helper")
            .expect("static fn reported");
        let e = report
            .findings
            .iter()
            .find(|f| f.symbol_name == "api")
            .expect("extern fn reported");
        assert_eq!(s.confidence, DeadCodeConfidence::High);
        assert_eq!(e.confidence, DeadCodeConfidence::Low);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn isolated_mutual_recursion_cycle_is_reported() {
        // `dead_a <-> dead_b` with zero external callers: both members are
        // reported as an isolated dead cycle.
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "dead_a").with_file_path("src/lib.rs");
        let b = Node::new(NodeType::Function, "dead_b").with_file_path("src/lib.rs");
        let a_id = a.id;
        let b_id = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend
            .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(b_id, a_id, EdgeType::Calls))
            .unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        let names: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.symbol_name.as_str())
            .collect();
        assert!(
            names.contains(&"dead_a") && names.contains(&"dead_b"),
            "isolated cycle members must be reported, got {names:?}"
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.symbol_name == "dead_a" && f.reason.contains("isolated dead cycle")),
            "cycle findings must explain the isolated cycle"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn self_recursive_dead_function_is_reported() {
        let mut backend = MemoryBackend::new();
        let f = Node::new(NodeType::Function, "dead_rec").with_file_path("src/lib.rs");
        let f_id = f.id;
        backend.insert_node(f).unwrap();
        backend
            .insert_edge(Edge::new(f_id, f_id, EdgeType::Calls))
            .unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|x| x.symbol_name == "dead_rec" && x.reason.contains("isolated dead cycle")),
            "self-recursive dead functions must be reported as isolated cycles"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cycle_reachable_from_dead_root_stays_suppressed() {
        // dead_root -> dead_a <-> dead_b: the cycle is downstream of a dead
        // root and must stay suppressed (root-only semantics).
        let mut backend = MemoryBackend::new();
        let root = Node::new(NodeType::Function, "dead_root").with_file_path("src/lib.rs");
        let a = Node::new(NodeType::Function, "dead_a").with_file_path("src/lib.rs");
        let b = Node::new(NodeType::Function, "dead_b").with_file_path("src/lib.rs");
        let root_id = root.id;
        let a_id = a.id;
        let b_id = b.id;
        backend.insert_node(root).unwrap();
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend
            .insert_edge(Edge::new(root_id, a_id, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(b_id, a_id, EdgeType::Calls))
            .unwrap();

        let (store, path) = temp_snapshot(&backend);
        let report = DeadCodeAnalyzer::new(&store).unwrap().analyze().unwrap();
        let names: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.symbol_name.as_str())
            .collect();
        assert_eq!(names, vec!["dead_root"], "only the dead root is reported");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn jaxrs_annotations_are_framework_entries() {
        assert!(is_framework_annotation("Path"));
        assert!(is_framework_annotation("GET"));
        assert!(is_framework_annotation("POST"));
        assert!(is_framework_annotation("Put"));
        assert!(is_framework_annotation("DELETE"));
        assert!(is_framework_annotation("Patch"));
        assert!(is_framework_annotation("main"));
        // Anchored: look-alikes still rejected.
        assert!(!is_framework_annotation("PathProvider"));
        assert!(!is_framework_annotation("Postpone"));
        assert!(!is_framework_annotation("Getter"));
    }

    #[test]
    fn simple_name_segments() {
        assert_eq!(simple_name_segment("main"), "main");
        assert_eq!(simple_name_segment("crate::main"), "main");
        assert_eq!(simple_name_segment("Program.Main"), "Main");
        assert_eq!(simple_name_segment("Mod#inner"), "inner");
        assert_eq!(simple_name_segment("__init__"), "__init__");
    }
}
