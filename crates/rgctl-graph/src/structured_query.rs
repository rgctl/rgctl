//! Structured query primitives over mmap snapshots (no `MemoryBackend` hydrate).
//!
//! # Index complexity (honest)
//! - Exact name: O(1) hash via shared Arc `name_index` ([`ColumnarGraphMmap::indexes_shared`]).
//! - Prefix / suffix / contains name patterns: O(|name keys|) scan of the HashMap keys.
//! - `--scope` on `qualified_name`: per-candidate node filter (no dedicated prefix index yet).
//! - Seedless edge scan: sequential typed-edge walk via [`SnapshotNodeStore::for_each_edge`].
//! - Seeded multi-hop: builds a typed adjacency map once per invocation (O(E_type)).

use crate::schema::{EdgeType, Node, NodeType, SharedStr};
use crate::snapshot::SnapshotNodeStore;
use rgctl_error::{Error, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use uuid::Uuid;

/// JSON schema version for structured-query envelopes.
///
/// v2: relations rows carry `occurrences`; `total` is distinct `(source,target,edge)` count.
pub const STRUCTURED_QUERY_SCHEMA_VERSION: u32 = 2;

/// All [`NodeType`] variants for inventory zero-count emission.
pub const ALL_NODE_TYPES: &[NodeType] = &[
    NodeType::Function,
    NodeType::Class,
    NodeType::Struct,
    NodeType::Enum,
    NodeType::Interface,
    NodeType::Annotation,
    NodeType::Module,
    NodeType::Variable,
    NodeType::File,
    NodeType::ConfigKey,
    NodeType::TypeAlias,
    NodeType::Macro,
    NodeType::Import,
    NodeType::Table,
    NodeType::Dependency,
    NodeType::Job,
    NodeType::BuildStep,
    NodeType::AnsiblePlaybook,
    NodeType::AnsiblePlay,
    NodeType::AnsibleTask,
    NodeType::AnsibleRole,
    NodeType::AnsibleHandler,
    NodeType::AnsibleVariable,
    NodeType::AnsibleTemplate,
    NodeType::ChefCookbook,
    NodeType::ChefRecipe,
    NodeType::ChefResource,
    NodeType::ChefAttribute,
    NodeType::ChefTemplate,
    NodeType::ChefCustomResource,
    NodeType::PuppetModule,
    NodeType::PuppetClass,
    NodeType::PuppetDefinedType,
    NodeType::PuppetResource,
    NodeType::PuppetVariable,
    NodeType::PuppetFact,
    NodeType::PuppetNode,
];

/// All [`EdgeType`] variants except [`EdgeType::Unknown`] for inventory zeros.
pub const ALL_EDGE_TYPES: &[EdgeType] = &[
    EdgeType::Calls,
    EdgeType::Contains,
    EdgeType::Uses,
    EdgeType::Implements,
    EdgeType::Extends,
    EdgeType::References,
    EdgeType::Instantiates,
    EdgeType::Modifies,
    EdgeType::UsesConfig,
    EdgeType::DefinedIn,
    EdgeType::DependsOn,
    EdgeType::IncludesRole,
    EdgeType::DependsOnRole,
    EdgeType::ExecutesTask,
    EdgeType::NotifiesHandler,
    EdgeType::IncludesPlaybook,
    EdgeType::RendersTemplate,
    EdgeType::DependsOnCookbook,
    EdgeType::IncludesRecipe,
    EdgeType::DeclaresResource,
    EdgeType::UsesTemplate,
    EdgeType::DefinesAttribute,
    EdgeType::NotifiesResource,
    EdgeType::DependsOnModule,
    EdgeType::IncludesClass,
    EdgeType::InheritsClass,
    EdgeType::RequiresResource,
    EdgeType::UsesFact,
    EdgeType::AnnotatedWith,
    EdgeType::Permits,
    EdgeType::Overrides,
    EdgeType::ExposesRoute,
    EdgeType::Injects,
    EdgeType::CoChangesWith,
];

/// How `--scope` filters `qualified_name` prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScopeMode {
    /// Keep nodes/endpoints whose FQN starts with the prefix.
    #[default]
    Inside,
    /// Keep nodes/endpoints whose FQN does not start with the prefix.
    Outside,
    /// For edges: keep pairs that straddle the prefix boundary.
    Crossing,
}

impl ScopeMode {
    /// Parse CLI token.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "inside" | "in" => Ok(Self::Inside),
            "outside" | "out" => Ok(Self::Outside),
            "crossing" | "cross" => Ok(Self::Crossing),
            other => Err(Error::InvalidQuery(format!(
                "unknown scope-mode '{other}' (expected inside|outside|crossing)"
            ))),
        }
    }
}

/// Lean entity row for JSON.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EntityRow {
    /// Bare name
    pub name: String,
    /// Fully qualified name when present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    /// Node type (lowercase CLI form)
    #[serde(rename = "type")]
    pub node_type: String,
    /// Source file path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Definition line
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Declaration start byte (Red-Green), when known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_byte: Option<u32>,
    /// Declaration exclusive end byte, when known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_byte: Option<u32>,
    /// Identifier token start byte, when known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_start_byte: Option<u32>,
    /// Identifier token exclusive end byte, when known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_end_byte: Option<u32>,
    /// Node UUID (string)
    pub id: String,
    /// Annotation argument text when `--show-attributes` and args are indexed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<String>,
}

/// Keyed edge row (direction-stable; never positional).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EdgeRow {
    /// Edge origin
    pub source: EntityRow,
    /// Edge type (lowercase CLI form)
    pub edge: String,
    /// Traversal direction for this emission
    pub direction: String,
    /// Edge destination
    pub target: EntityRow,
    /// Hop distance from seed (1 for seedless scans)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hops: Option<usize>,
    /// How many stored edges collapsed into this distinct relationship
    pub occurrences: usize,
}

/// Find / list result envelope.
#[derive(Debug, Clone, Serialize)]
pub struct FindResult {
    /// Schema version
    pub schema_version: u32,
    /// Emitted row count
    pub returned: usize,
    /// Matches before limit when known
    pub total: usize,
    /// Entity rows (empty when count_only)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityRow>,
}

/// Callers / callees envelope.
#[derive(Debug, Clone, Serialize)]
pub struct CallNeighborsResult {
    /// Schema version
    pub schema_version: u32,
    /// Resolved seed
    pub target: EntityRow,
    /// Neighbor rows
    pub neighbors: Vec<EntityRow>,
    /// Hop labels aligned with neighbors
    pub hops: Vec<usize>,
    /// Emitted neighbor count
    pub returned: usize,
    /// Total neighbors before limit
    pub total: usize,
    /// Requested depth
    pub depth: usize,
    /// `callers` or `callees`
    pub direction: String,
}

/// Relations envelope.
#[derive(Debug, Clone, Serialize)]
pub struct RelationsResult {
    /// Schema version
    pub schema_version: u32,
    /// Optional resolved seed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<EntityRow>,
    /// Keyed edges
    pub edges: Vec<EdgeRow>,
    /// Emitted count
    pub returned: usize,
    /// Total before limit
    pub total: usize,
}

/// Inventory envelope.
#[derive(Debug, Clone, Serialize)]
pub struct InventoryResult {
    /// Schema version
    pub schema_version: u32,
    /// Aggregation dimension
    pub by: String,
    /// Counts (includes zeros for type/edge)
    pub counts: Vec<InventoryCount>,
}

/// One inventory bucket.
#[derive(Debug, Clone, Serialize)]
pub struct InventoryCount {
    /// Bucket key (type/edge/lang/file/community)
    pub key: String,
    /// Primary count. For `--by edge`: distinct `(source,target)` relationships
    /// (aligned with `relations.total`). For other dimensions: entity count.
    pub count: usize,
    /// Raw stored edge instances when `count` is distinct (`--by edge` only).
    /// Omitted for non-edge inventories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurrences: Option<usize>,
}

/// Filters shared by find / resolve.
#[derive(Debug, Clone, Default)]
pub struct QueryFilters {
    /// Node type filter
    pub node_type: Option<NodeType>,
    /// Path glob (`*` wildcards; `?` single char)
    pub file_glob: Option<String>,
    /// Language id (property `language` or file extension heuristic)
    pub lang: Option<String>,
    /// qualified_name prefix
    pub scope: Option<String>,
    /// Scope mode
    pub scope_mode: ScopeMode,
    /// Enclosing class/type name filter
    pub class: Option<String>,
    /// Definition start line (disambiguates same-class overloads)
    pub line: Option<usize>,
    /// Max rows (None = unbounded)
    pub limit: Option<usize>,
    /// Count only
    pub count_only: bool,
    /// Exact name match (no glob)
    pub exact: bool,
    /// Annotation invert: simple names / `@Name` / FQNs (OR). When set, `find` returns
    /// AnnotatedWith **sources** matching any listed annotation.
    pub annotation_names: Option<Vec<String>>,
    /// Request annotation argument payloads when indexed (`--show-attributes`).
    pub show_attributes: bool,
    /// Optional map `source_id\0annotation_simple` → arguments (from sidecar).
    pub annotation_arg_index: Option<HashMap<String, String>>,
}

/// Session over an open snapshot store.
pub struct StructuredQuery<'a> {
    store: &'a SnapshotNodeStore,
}

impl<'a> StructuredQuery<'a> {
    /// Borrow a snapshot store (must already be open; no hydrate).
    pub fn new(store: &'a SnapshotNodeStore) -> Self {
        Self { store }
    }

    /// Documented serve offload contract: call heavy methods from `spawn_blocking`.
    pub fn serve_offload_note() -> &'static str {
        "When invoking StructuredQuery from rgctl serve, run find/callers/relations/inventory on spawn_blocking"
    }

    fn project(node: &Node) -> EntityRow {
        let (name_start_byte, name_end_byte) = match (node.start_byte, node.name_rel_offset) {
            (Some(start), Some(rel)) => {
                let ns = start.saturating_add(u32::from(rel));
                let len = node
                    .name_token_len
                    .map(u32::from)
                    .unwrap_or_else(|| node.name.len() as u32);
                (Some(ns), Some(ns.saturating_add(len)))
            }
            (Some(start), None) => {
                if let Some((rel, len)) = node.name_span_overflow {
                    let ns = start.saturating_add(rel);
                    (Some(ns), Some(ns.saturating_add(len)))
                } else {
                    (None, None)
                }
            }
            _ => (None, None),
        };
        EntityRow {
            name: node.name.to_string(),
            qualified_name: node.qualified_name.as_ref().map(|s| s.to_string()),
            node_type: node_type_cli(node.node_type),
            file: node.file_path.as_ref().map(|s| s.to_string()),
            line: node.start_line,
            start_byte: node.start_byte,
            end_byte: node.end_byte,
            name_start_byte,
            name_end_byte,
            id: node.id.to_string(),
            attributes: None,
        }
    }

    fn project_with_annotation_attrs(
        node: &Node,
        annots: &[String],
        arg_index: Option<&HashMap<String, String>>,
    ) -> EntityRow {
        let mut row = Self::project(node);
        if let Some(idx) = arg_index {
            let mut parts = Vec::new();
            for a in annots {
                let key = format!("{}\0{a}", node.id);
                if let Some(args) = idx.get(&key) {
                    parts.push(format!("@{a}{args}"));
                } else {
                    // Also try bare annotation key variants
                    let simple = normalize_annotation_name(a);
                    let key2 = format!("{}\0{simple}", node.id);
                    if let Some(args) = idx.get(&key2) {
                        parts.push(format!("@{simple}{args}"));
                    }
                }
            }
            if !parts.is_empty() {
                row.attributes = Some(parts.join("; "));
            }
        }
        row
    }

    fn matches_scope(node: &Node, scope: Option<&str>, mode: ScopeMode) -> bool {
        let Some(prefix) = scope else {
            return mode != ScopeMode::Crossing;
        };
        let inside = node_in_scope(node, prefix);
        match mode {
            ScopeMode::Inside => inside,
            ScopeMode::Outside => !inside,
            // Node-level crossing is meaningless; treat as inside for node filters.
            ScopeMode::Crossing => inside,
        }
    }

    fn edge_scope_ok(src: &Node, dst: &Node, scope: Option<&str>, mode: ScopeMode) -> bool {
        let Some(prefix) = scope else {
            return true;
        };
        let s = node_in_scope(src, prefix);
        let d = node_in_scope(dst, prefix);
        match mode {
            // Migration-friendly: keep edges whose *source* is in-scope (annotations /
            // base types are often external). Use Crossing for true boundary edges.
            ScopeMode::Inside => s,
            ScopeMode::Outside => !s,
            ScopeMode::Crossing => s != d,
        }
    }

    fn matches_file(node: &Node, glob: Option<&str>) -> bool {
        let Some(raw) = glob else {
            return true;
        };
        let pat = normalize_file_glob(raw);
        let path = node.file_path.as_deref().unwrap_or("");
        if glob_match(&pat, path) {
            return true;
        }
        // Basename fallback when pattern has no path separator.
        if !raw.contains('/') && !raw.contains('\\') {
            let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
            return glob_match(&pat, base) || glob_match(raw, base);
        }
        false
    }

    fn matches_lang(node: &Node, lang: Option<&str>) -> bool {
        let Some(want) = lang else {
            return true;
        };
        let want = want.to_ascii_lowercase();
        if let Some(v) = node.get_property("language") {
            return v.eq_ignore_ascii_case(&want);
        }
        let ext = node
            .file_path
            .as_deref()
            .and_then(|p| p.rsplit('.').next())
            .unwrap_or("");
        lang_from_ext(ext).eq_ignore_ascii_case(&want)
    }

    fn matches_class(node: &Node, class: Option<&str>) -> bool {
        let Some(c) = class else {
            return true;
        };
        if let Some(qn) = node.qualified_name.as_deref()
            && qn.contains(c)
        {
            return true;
        }
        if let Some(v) = node.get_property("class") {
            return v == c;
        }
        if let Some(v) = node.get_property("enclosing_type") {
            return v == c;
        }
        false
    }

    fn node_passes(&self, node: &Node, f: &QueryFilters) -> bool {
        if let Some(t) = f.node_type
            && node.node_type != t
        {
            return false;
        }
        if !Self::matches_file(node, f.file_glob.as_deref()) {
            return false;
        }
        if !Self::matches_lang(node, f.lang.as_deref()) {
            return false;
        }
        if !Self::matches_scope(node, f.scope.as_deref(), f.scope_mode) {
            return false;
        }
        if !Self::matches_class(node, f.class.as_deref()) {
            return false;
        }
        if let Some(want_line) = f.line
            && node.start_line != Some(want_line)
        {
            return false;
        }
        true
    }

    #[allow(clippy::type_complexity)]
    fn indexes(
        &self,
    ) -> Result<Option<Arc<(HashMap<String, Vec<Uuid>>, HashMap<NodeType, Vec<Uuid>>)>>> {
        self.store
            .columnar()
            .map(|c| c.indexes_shared())
            .transpose()
    }

    /// Resolve a unique symbol or return ambiguity / not-found errors.
    pub fn resolve_symbol(&self, symbol: &str, filters: &QueryFilters) -> Result<Node> {
        let mut matches = self.lookup_name_candidates(symbol, filters.exact)?;
        matches.retain(|n| self.node_passes(n, filters));
        match matches.len() {
            0 => Err(Error::NodeNotFound(symbol.to_string())),
            1 => Ok(matches.remove(0)),
            n => {
                let candidates = matches
                    .into_iter()
                    .take(50)
                    .map(|node| rgctl_error::SymbolCandidate {
                        id: node.id.to_string(),
                        name: node.name.to_string(),
                        qualified_name: node.qualified_name.as_ref().map(|s| s.to_string()),
                        node_type: node_type_cli(node.node_type),
                        file: node.file_path.as_ref().map(|s| s.to_string()),
                        line: node.start_line,
                    })
                    .collect();
                Err(Error::AmbiguousSymbol {
                    name: symbol.to_string(),
                    count: n,
                    candidates,
                })
            }
        }
    }

    fn lookup_name_candidates(&self, symbol: &str, exact: bool) -> Result<Vec<Node>> {
        if let Ok(uuid) = Uuid::parse_str(symbol)
            && let Some(n) = self.store.get_node(uuid)?
        {
            return Ok(vec![n]);
        }
        if exact || !is_glob_pattern(symbol) {
            if let Some(indexes) = self.indexes()? {
                if let Some(ids) = indexes.0.get(symbol) {
                    let mut out = Vec::with_capacity(ids.len());
                    for id in ids {
                        if let Some(n) = self.store.get_node(*id)? {
                            out.push(n);
                        }
                    }
                    if !out.is_empty() {
                        return Ok(out);
                    }
                }
            } else {
                return self.store.find_nodes_by_name(symbol);
            }
            // Fall through: also try qualified_name exact via type scan is expensive; return empty.
            return Ok(Vec::new());
        }
        // Glob over name keys (O(|keys|)).
        let Some(indexes) = self.indexes()? else {
            return Err(Error::GraphError(
                "name glob requires columnar snapshot indexes".into(),
            ));
        };
        let mut out = Vec::new();
        for (name, ids) in indexes.0.iter() {
            if glob_match(symbol, name) {
                for id in ids {
                    if let Some(n) = self.store.get_node(*id)? {
                        out.push(n);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Entity search (`rgctl find`).
    pub fn find(&self, pattern: Option<&str>, filters: &QueryFilters) -> Result<FindResult> {
        if filters.show_attributes {
            let has_annots = filters
                .annotation_names
                .as_ref()
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            if !has_annots {
                return Err(Error::InvalidQuery(
                    "--show-attributes requires --annotation".into(),
                ));
            }
            if filters
                .annotation_arg_index
                .as_ref()
                .map(|m| m.is_empty())
                .unwrap_or(true)
            {
                return Err(Error::InvalidQuery(
                    "annotation attributes are not indexed yet; omit --show-attributes or re-discover after a Java index that writes .rgctl/annotation_args.json"
                        .into(),
                ));
            }
        }

        if let Some(annots) = filters.annotation_names.as_ref()
            && !annots.is_empty()
        {
            return self.find_by_annotation(annots, pattern, filters);
        }

        let candidates = if let Some(pat) = pattern {
            let mut c = self.lookup_name_candidates(pat, filters.exact)?;
            c.retain(|n| self.node_passes(n, filters));
            c
        } else {
            // Linear hot-column scan: filter on type/path/line/scope without N× cold_ext
            // deserializations; materialize cold only when class/property filters require it.
            self.collect_nodes_hot_scan(filters.node_type, filters)?
        };

        Self::finish_find(candidates, filters)
    }

    /// Whether query filters require cold `properties` / labels (not hot columns + qn).
    fn filters_need_cold_ext(filters: &QueryFilters) -> bool {
        filters.class.is_some()
    }

    /// Sequential columnar scan with hot-column pre-filter; cold_ext only when needed.
    fn collect_nodes_hot_scan(
        &self,
        type_filter: Option<NodeType>,
        filters: &QueryFilters,
    ) -> Result<Vec<Node>> {
        if let Some(col) = self.store.columnar() {
            let need_cold = Self::filters_need_cold_ext(filters);
            let mut out = Vec::new();
            for idx in 0..col.node_count() {
                if let Some(want) = type_filter
                    && col.node_type_at(idx)? != want
                {
                    continue;
                }
                if let Some(want_line) = filters.line {
                    let row = col.node_row_at(idx)?;
                    if row.start_line == 0 || row.start_line as usize != want_line {
                        continue;
                    }
                }
                if let Some(glob) = filters.file_glob.as_deref() {
                    let path = col.node_file_path_str(idx)?.unwrap_or("");
                    let pat = normalize_file_glob(glob);
                    let ok = glob_match(&pat, path)
                        || (!glob.contains('/') && !glob.contains('\\') && {
                            let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
                            glob_match(&pat, base) || glob_match(glob, base)
                        });
                    if !ok {
                        continue;
                    }
                }
                let node = if need_cold {
                    col.materialize_node_at(idx)?
                } else {
                    let mut n = col.materialize_node_hot_at(idx)?;
                    if let Some(qn) = col.node_qualified_name_str(idx)? {
                        n.qualified_name = Some(SharedStr::from(qn.to_string()));
                    }
                    n
                };
                if self.node_passes(&node, filters) {
                    out.push(node);
                }
            }
            return Ok(out);
        }

        let mut out = Vec::new();
        for id in self.store.all_node_ids() {
            let Some(n) = self.store.get_node(id)? else {
                continue;
            };
            if let Some(t) = type_filter
                && n.node_type != t
            {
                continue;
            }
            if self.node_passes(&n, filters) {
                out.push(n);
            }
        }
        Ok(out)
    }

    /// Invert AnnotatedWith: return sources carrying any of the listed annotations (OR).
    fn find_by_annotation(
        &self,
        annots: &[String],
        pattern: Option<&str>,
        filters: &QueryFilters,
    ) -> Result<FindResult> {
        let normalized: Vec<String> = annots
            .iter()
            .map(|a| normalize_annotation_name(a))
            .filter(|a| !a.is_empty())
            .collect();
        if normalized.is_empty() {
            return Err(Error::InvalidQuery(
                "--annotation requires at least one name (e.g. @MessageDriven)".into(),
            ));
        }

        let mut seen = HashSet::new();
        let mut candidates: Vec<Node> = Vec::new();
        self.store.for_each_edge(|from, to, et| {
            if et != EdgeType::AnnotatedWith {
                return Ok(());
            }
            let Some(ann) = self.store.get_node(to)? else {
                return Ok(());
            };
            if !normalized
                .iter()
                .any(|want| annotation_name_matches(&ann, want))
            {
                return Ok(());
            }
            if !seen.insert(from) {
                return Ok(());
            }
            let Some(src) = self.store.get_node(from)? else {
                return Ok(());
            };
            if let Some(pat) = pattern {
                let ok = if filters.exact || !is_glob_pattern(pat) {
                    src.name == pat
                } else {
                    glob_match(pat, &src.name)
                };
                if !ok {
                    return Ok(());
                }
            }
            if self.node_passes(&src, filters) {
                candidates.push(src);
            }
            Ok(())
        })?;

        Self::finish_find_annotated(candidates, filters, &normalized)
    }

    fn finish_find_annotated(
        candidates: Vec<Node>,
        filters: &QueryFilters,
        annots: &[String],
    ) -> Result<FindResult> {
        let total = candidates.len();
        let limit = filters.limit.unwrap_or(total);
        let arg_index = filters.annotation_arg_index.as_ref();
        let entities: Vec<EntityRow> = if filters.count_only {
            Vec::new()
        } else {
            candidates
                .into_iter()
                .take(limit)
                .map(|n| {
                    if filters.show_attributes {
                        Self::project_with_annotation_attrs(&n, annots, arg_index)
                    } else {
                        Self::project(&n)
                    }
                })
                .collect()
        };
        let returned = if filters.count_only {
            total.min(limit)
        } else {
            entities.len()
        };
        Ok(FindResult {
            schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
            returned,
            total,
            entities,
        })
    }

    fn finish_find(candidates: Vec<Node>, filters: &QueryFilters) -> Result<FindResult> {
        let total = candidates.len();
        let limit = filters.limit.unwrap_or(total);
        let entities: Vec<EntityRow> = if filters.count_only {
            Vec::new()
        } else {
            candidates
                .into_iter()
                .take(limit)
                .map(|n| Self::project(&n))
                .collect()
        };
        let returned = if filters.count_only {
            total.min(limit)
        } else {
            entities.len()
        };
        Ok(FindResult {
            schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
            returned,
            total,
            entities,
        })
    }

    /// Incoming (`callers`) or outgoing (`callees`) CALLS.
    pub fn call_neighbors(
        &self,
        symbol: &str,
        incoming: bool,
        depth: usize,
        filters: &QueryFilters,
    ) -> Result<CallNeighborsResult> {
        let seed = self.resolve_symbol(symbol, filters)?;
        let depth = depth.max(1);
        let adj = self.build_typed_adjacency(EdgeType::Calls)?;
        let mut seen = HashSet::from([seed.id]);
        let mut q = VecDeque::from([(seed.id, 0usize)]);
        let mut found: Vec<(Uuid, usize)> = Vec::new();
        while let Some((id, d)) = q.pop_front() {
            if d >= depth {
                continue;
            }
            let nexts = if incoming {
                adj.incoming.get(&id)
            } else {
                adj.outgoing.get(&id)
            };
            let Some(nexts) = nexts else { continue };
            for &nid in nexts {
                if !seen.insert(nid) {
                    continue;
                }
                let hop = d + 1;
                found.push((nid, hop));
                if hop < depth {
                    q.push_back((nid, hop));
                }
            }
        }

        let mut neighbors = Vec::new();
        let mut hops = Vec::new();
        for (id, hop) in &found {
            let Some(n) = self.store.get_node(*id)? else {
                continue;
            };
            if !self.node_passes(&n, filters) && filters.scope.is_some() {
                // For callers, scope typically filters the *neighbor* side.
                if !Self::matches_scope(&n, filters.scope.as_deref(), filters.scope_mode) {
                    continue;
                }
            }
            neighbors.push(Self::project(&n));
            hops.push(*hop);
        }
        let total = neighbors.len();
        let limit = filters.limit.unwrap_or(total);
        if neighbors.len() > limit {
            neighbors.truncate(limit);
            hops.truncate(limit);
        }
        Ok(CallNeighborsResult {
            schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
            target: Self::project(&seed),
            returned: neighbors.len(),
            total,
            neighbors,
            hops,
            depth,
            direction: if incoming {
                "callers".into()
            } else {
                "callees".into()
            },
        })
    }

    /// Seeded or seedless relations.
    #[allow(clippy::too_many_arguments)]
    pub fn relations(
        &self,
        symbol: Option<&str>,
        edge: EdgeType,
        direction: RelationDirection,
        from_type: Option<NodeType>,
        to_type: Option<NodeType>,
        depth: usize,
        filters: &QueryFilters,
    ) -> Result<RelationsResult> {
        if let Some(sym) = symbol {
            return self.relations_seeded(sym, edge, direction, from_type, to_type, depth, filters);
        }
        self.relations_seedless(edge, direction, from_type, to_type, filters)
    }

    fn relations_seedless(
        &self,
        edge: EdgeType,
        direction: RelationDirection,
        from_type: Option<NodeType>,
        to_type: Option<NodeType>,
        filters: &QueryFilters,
    ) -> Result<RelationsResult> {
        // Aggregate duplicate stored edges into distinct (source,target) with occurrences.
        let mut agg: HashMap<(Uuid, Uuid), EdgeRow> = HashMap::new();
        self.store.for_each_edge(|from, to, et| {
            if et != edge {
                return Ok(());
            }
            let Some(src) = self.store.get_node(from)? else {
                return Ok(());
            };
            let Some(dst) = self.store.get_node(to)? else {
                return Ok(());
            };
            if let Some(ft) = from_type
                && src.node_type != ft
            {
                return Ok(());
            }
            if let Some(tt) = to_type
                && dst.node_type != tt
            {
                return Ok(());
            }
            if !Self::edge_scope_ok(&src, &dst, filters.scope.as_deref(), filters.scope_mode) {
                return Ok(());
            }
            let (source, target, dir_label, key) = match direction {
                RelationDirection::In => {
                    (Self::project(&dst), Self::project(&src), "in", (to, from))
                }
                RelationDirection::Out | RelationDirection::Both => {
                    (Self::project(&src), Self::project(&dst), "out", (from, to))
                }
            };
            agg.entry(key)
                .and_modify(|row| row.occurrences = row.occurrences.saturating_add(1))
                .or_insert(EdgeRow {
                    source,
                    edge: edge_type_cli(edge),
                    direction: dir_label.into(),
                    target,
                    hops: Some(1),
                    occurrences: 1,
                });
            Ok(())
        })?;
        let total = agg.len();
        let limit = filters.limit.unwrap_or(usize::MAX);
        let mut edges: Vec<EdgeRow> = agg.into_values().collect();
        // Stable order for agents / goldens: source name, then target name.
        edges.sort_by(|a, b| {
            (&a.source.name, &a.target.name, &a.source.id, &a.target.id).cmp(&(
                &b.source.name,
                &b.target.name,
                &b.source.id,
                &b.target.id,
            ))
        });
        edges.truncate(limit);
        Ok(RelationsResult {
            schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
            target: None,
            returned: edges.len(),
            total,
            edges,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn relations_seeded(
        &self,
        symbol: &str,
        edge: EdgeType,
        direction: RelationDirection,
        from_type: Option<NodeType>,
        to_type: Option<NodeType>,
        depth: usize,
        filters: &QueryFilters,
    ) -> Result<RelationsResult> {
        let seed = self.resolve_symbol(symbol, filters)?;
        let depth = depth.max(1);
        let adj = self.build_typed_adjacency(edge)?;
        let mut agg: HashMap<(Uuid, Uuid, bool), EdgeRow> = HashMap::new();

        let walk = |incoming: bool, agg: &mut HashMap<(Uuid, Uuid, bool), EdgeRow>| -> Result<()> {
            let mut seen = HashSet::from([seed.id]);
            let mut q = VecDeque::from([(seed.id, 0usize)]);
            while let Some((id, d)) = q.pop_front() {
                if d >= depth {
                    continue;
                }
                let nexts = if incoming {
                    adj.incoming.get(&id)
                } else {
                    adj.outgoing.get(&id)
                };
                let Some(nexts) = nexts else { continue };
                for &nid in nexts {
                    let hop = d + 1;
                    let (src_id, dst_id) = if incoming { (nid, id) } else { (id, nid) };
                    let Some(src) = self.store.get_node(src_id)? else {
                        continue;
                    };
                    let Some(dst) = self.store.get_node(dst_id)? else {
                        continue;
                    };
                    if let Some(ft) = from_type
                        && src.node_type != ft
                    {
                        continue;
                    }
                    if let Some(tt) = to_type
                        && dst.node_type != tt
                    {
                        continue;
                    }
                    if !Self::edge_scope_ok(
                        &src,
                        &dst,
                        filters.scope.as_deref(),
                        filters.scope_mode,
                    ) {
                        continue;
                    }
                    let key = (src_id, dst_id, incoming);
                    agg.entry(key)
                        .and_modify(|row| {
                            row.occurrences = row.occurrences.saturating_add(1);
                            // Keep the shortest hop when collapsing duplicates.
                            if let (Some(h), Some(prev)) = (Some(hop), row.hops)
                                && h < prev
                            {
                                row.hops = Some(h);
                            }
                        })
                        .or_insert(EdgeRow {
                            source: Self::project(&src),
                            edge: edge_type_cli(edge),
                            direction: if incoming { "in" } else { "out" }.into(),
                            target: Self::project(&dst),
                            hops: Some(hop),
                            occurrences: 1,
                        });
                    if seen.insert(nid) && hop < depth {
                        q.push_back((nid, hop));
                    }
                }
            }
            Ok(())
        };

        match direction {
            RelationDirection::Out => walk(false, &mut agg)?,
            RelationDirection::In => walk(true, &mut agg)?,
            RelationDirection::Both => {
                walk(false, &mut agg)?;
                walk(true, &mut agg)?;
            }
        }

        let total = agg.len();
        let limit = filters.limit.unwrap_or(usize::MAX);
        let mut rows: Vec<EdgeRow> = agg.into_values().collect();
        rows.sort_by(|a, b| {
            (&a.source.name, &a.target.name, &a.source.id, &a.target.id).cmp(&(
                &b.source.name,
                &b.target.name,
                &b.source.id,
                &b.target.id,
            ))
        });
        rows.truncate(limit);

        Ok(RelationsResult {
            schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
            target: Some(Self::project(&seed)),
            returned: rows.len(),
            total,
            edges: rows,
        })
    }

    /// Inventory with zero-count enums for type/edge.
    pub fn inventory(&self, by: InventoryBy, filters: &QueryFilters) -> Result<InventoryResult> {
        match by {
            InventoryBy::Type => {
                let mut map: HashMap<NodeType, usize> =
                    ALL_NODE_TYPES.iter().map(|t| (*t, 0usize)).collect();
                for n in self.collect_nodes_hot_scan(None, filters)? {
                    *map.entry(n.node_type).or_insert(0) += 1;
                }
                let mut counts: Vec<InventoryCount> = ALL_NODE_TYPES
                    .iter()
                    .map(|t| InventoryCount {
                        key: node_type_cli(*t),
                        count: *map.get(t).unwrap_or(&0),
                        occurrences: None,
                    })
                    .collect();
                // Include any unexpected types not in ALL_NODE_TYPES.
                for (t, c) in map {
                    if !ALL_NODE_TYPES.contains(&t) {
                        counts.push(InventoryCount {
                            key: node_type_cli(t),
                            count: c,
                            occurrences: None,
                        });
                    }
                }
                Ok(InventoryResult {
                    schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
                    by: "type".into(),
                    counts,
                })
            }
            InventoryBy::Edge => {
                // `count` = distinct (from,to) pairs; `occurrences` = raw stored edges.
                let mut distinct: HashMap<EdgeType, HashSet<(Uuid, Uuid)>> = ALL_EDGE_TYPES
                    .iter()
                    .map(|t| (*t, HashSet::new()))
                    .collect();
                let mut occurrences: HashMap<EdgeType, usize> =
                    ALL_EDGE_TYPES.iter().map(|t| (*t, 0usize)).collect();
                self.store.for_each_edge(|from, to, et| {
                    if et == EdgeType::Unknown {
                        return Ok(());
                    }
                    if filters.scope.is_some() {
                        let Some(src) = self.store.get_node(from)? else {
                            return Ok(());
                        };
                        let Some(dst) = self.store.get_node(to)? else {
                            return Ok(());
                        };
                        if !Self::edge_scope_ok(
                            &src,
                            &dst,
                            filters.scope.as_deref(),
                            filters.scope_mode,
                        ) {
                            return Ok(());
                        }
                    }
                    *occurrences.entry(et).or_insert(0) += 1;
                    distinct.entry(et).or_default().insert((from, to));
                    Ok(())
                })?;
                let counts = ALL_EDGE_TYPES
                    .iter()
                    .map(|t| {
                        let occ = *occurrences.get(t).unwrap_or(&0);
                        let dist = distinct.get(t).map(|s| s.len()).unwrap_or(0);
                        InventoryCount {
                            key: edge_type_cli(*t),
                            count: dist,
                            occurrences: Some(occ),
                        }
                    })
                    .collect();
                Ok(InventoryResult {
                    schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
                    by: "edge".into(),
                    counts,
                })
            }
            InventoryBy::Lang => {
                let mut map: HashMap<String, usize> = HashMap::new();
                // Prefer hot file-extension heuristic (0 cold_ext). Property
                // `language` is only consulted when class filters already force cold.
                for n in self.collect_nodes_hot_scan(None, filters)? {
                    let lang = n
                        .get_property("language")
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            let ext = n
                                .file_path
                                .as_deref()
                                .and_then(|p| p.rsplit('.').next())
                                .unwrap_or("");
                            lang_from_ext(ext)
                        });
                    *map.entry(lang).or_insert(0) += 1;
                }
                let mut counts: Vec<_> = map
                    .into_iter()
                    .map(|(key, count)| InventoryCount {
                        key,
                        count,
                        occurrences: None,
                    })
                    .collect();
                counts.sort_by(|a, b| a.key.cmp(&b.key));
                Ok(InventoryResult {
                    schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
                    by: "lang".into(),
                    counts,
                })
            }
            InventoryBy::File => {
                let mut map: HashMap<String, usize> = HashMap::new();
                for n in self.collect_nodes_hot_scan(None, filters)? {
                    let key = n.file_path.as_deref().unwrap_or("<unknown>").to_string();
                    *map.entry(key).or_insert(0) += 1;
                }
                let mut counts: Vec<_> = map
                    .into_iter()
                    .map(|(key, count)| InventoryCount {
                        key,
                        count,
                        occurrences: None,
                    })
                    .collect();
                counts.sort_by(|a, b| a.key.cmp(&b.key));
                Ok(InventoryResult {
                    schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
                    by: "file".into(),
                    counts,
                })
            }
            InventoryBy::Community => {
                // `community_id` lives in cold properties — full materialize required.
                let mut map: HashMap<String, usize> = HashMap::new();
                for id in self.store.all_node_ids() {
                    let Some(n) = self.store.get_node(id)? else {
                        continue;
                    };
                    if !self.node_passes(&n, filters) {
                        continue;
                    }
                    let key = n
                        .get_property("community_id")
                        .unwrap_or("<none>")
                        .to_string();
                    *map.entry(key).or_insert(0) += 1;
                }
                let mut counts: Vec<_> = map
                    .into_iter()
                    .map(|(key, count)| InventoryCount {
                        key,
                        count,
                        occurrences: None,
                    })
                    .collect();
                counts.sort_by(|a, b| a.key.cmp(&b.key));
                Ok(InventoryResult {
                    schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
                    by: "community".into(),
                    counts,
                })
            }
            InventoryBy::ImportPrefix => {
                // Observed prefixes only (no zero-fill). Default depth = 2 dotted segments.
                const DEPTH: usize = 2;
                let mut map: HashMap<String, usize> = HashMap::new();
                for n in self.collect_nodes_hot_scan(Some(NodeType::Import), filters)? {
                    let key = import_package_prefix(&n.name, DEPTH);
                    *map.entry(key).or_insert(0) += 1;
                }
                let mut counts: Vec<_> = map
                    .into_iter()
                    .map(|(key, count)| InventoryCount {
                        key,
                        count,
                        occurrences: None,
                    })
                    .collect();
                counts.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
                Ok(InventoryResult {
                    schema_version: STRUCTURED_QUERY_SCHEMA_VERSION,
                    by: "import-prefix".into(),
                    counts,
                })
            }
        }
    }

    fn build_typed_adjacency(&self, edge: EdgeType) -> Result<TypedAdj> {
        let mut outgoing: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        let mut incoming: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        self.store.for_each_edge(|from, to, et| {
            if et != edge {
                return Ok(());
            }
            outgoing.entry(from).or_default().push(to);
            incoming.entry(to).or_default().push(from);
            Ok(())
        })?;
        Ok(TypedAdj { outgoing, incoming })
    }
}

struct TypedAdj {
    outgoing: HashMap<Uuid, Vec<Uuid>>,
    incoming: HashMap<Uuid, Vec<Uuid>>,
}

/// Relation walk direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    /// Follow edge as stored (from → to)
    Out,
    /// Reverse (to → from)
    In,
    /// Both
    Both,
}

impl RelationDirection {
    /// Parse CLI token.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "out" | "outgoing" => Ok(Self::Out),
            "in" | "incoming" => Ok(Self::In),
            "both" => Ok(Self::Both),
            other => Err(Error::InvalidQuery(format!(
                "unknown direction '{other}' (expected in|out|both)"
            ))),
        }
    }
}

/// Inventory dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryBy {
    /// Node types
    Type,
    /// Edge types
    Edge,
    /// Language
    Lang,
    /// File path
    File,
    /// Community id property
    Community,
    /// Import package prefix (first N dotted segments; observed only)
    ImportPrefix,
}

impl InventoryBy {
    /// Parse CLI token.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "type" => Ok(Self::Type),
            "edge" => Ok(Self::Edge),
            "lang" | "language" => Ok(Self::Lang),
            "file" => Ok(Self::File),
            "community" => Ok(Self::Community),
            "import-prefix" | "importprefix" | "import_prefix" => Ok(Self::ImportPrefix),
            other => Err(Error::InvalidQuery(format!(
                "unknown inventory --by '{other}' (expected type|edge|lang|file|community|import-prefix)"
            ))),
        }
    }
}

/// Parse CLI node type string.
pub fn parse_node_type(s: &str) -> Result<NodeType> {
    let key = s.to_ascii_lowercase().replace(['-', '_'], "");
    for t in ALL_NODE_TYPES {
        if node_type_cli(*t).replace('_', "") == key {
            return Ok(*t);
        }
    }
    // Common aliases
    match key.as_str() {
        "config" => Ok(NodeType::ConfigKey),
        "fn" | "method" => Ok(NodeType::Function),
        "trait" => Ok(NodeType::Interface),
        _ => Err(Error::InvalidQuery(format!(
            "unknown node type '{s}' (e.g. function, class, import, annotation)"
        ))),
    }
}

/// Parse CLI edge type string.
pub fn parse_edge_type(s: &str) -> Result<EdgeType> {
    let key = s.to_ascii_lowercase().replace(['-', '_'], "");
    if matches!(key.as_str(), "cochange" | "cochangeswith" | "cochanges") {
        return Ok(EdgeType::CoChangesWith);
    }
    for t in ALL_EDGE_TYPES {
        if edge_type_cli(*t).replace('_', "") == key {
            return Ok(*t);
        }
    }
    Err(Error::InvalidQuery(format!(
        "unknown edge type '{s}' (e.g. calls, uses, extends, implements, annotatedwith)"
    )))
}

fn node_type_cli(t: NodeType) -> String {
    format!("{t:?}").to_ascii_lowercase()
}

fn edge_type_cli(t: EdgeType) -> String {
    format!("{t:?}").to_ascii_lowercase()
}

fn is_glob_pattern(s: &str) -> bool {
    s.contains('*') || s.contains('?')
}

/// Normalize scope keys so `/`, `\`, and `.` compare equivalently (PHP vs POSIX).
pub fn normalize_scope_key(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' | '/' | '.' => '/',
            other => other,
        })
        .collect()
}

/// True when node's FQN or (if missing) file_path matches the scope prefix.
///
/// Go/TS often leave `qualified_name` unset on types; package identity lives in `file_path`.
fn node_in_scope(node: &Node, scope_prefix: &str) -> bool {
    let want = normalize_scope_key(scope_prefix);
    if want.is_empty() {
        return true;
    }
    if let Some(qn) = node.qualified_name.as_deref().filter(|s| !s.is_empty()) {
        let hay = normalize_scope_key(qn);
        return hay.starts_with(&want);
    }
    // Fallback when qualified_name is absent (common for Go/TS types).
    let path = node.file_path.as_deref().unwrap_or("");
    if path.is_empty() {
        return false;
    }
    let hay = normalize_scope_key(path);
    path_contains_scope_segment(&hay, &want)
}

/// Match `want` as a path segment / prefix inside a normalized file path.
fn path_contains_scope_segment(hay: &str, want: &str) -> bool {
    if hay == want || hay.starts_with(&format!("{want}/")) || hay.ends_with(&format!("/{want}")) {
        return true;
    }
    hay.contains(&format!("/{want}/"))
}

/// If `--file` has no directory separator and no leading `*`, treat as basename glob.
pub fn normalize_file_glob(raw: &str) -> String {
    if raw.is_empty() {
        return raw.to_string();
    }
    if raw.contains('/') || raw.contains('\\') || raw.starts_with('*') {
        return raw.to_string();
    }
    format!("*{raw}")
}

/// Glob match with `*` and `?` (not full regex).
pub fn glob_match(pattern: &str, value: &str) -> bool {
    glob_match_rec(pattern.as_bytes(), value.as_bytes())
}

fn glob_match_rec(pat: &[u8], val: &[u8]) -> bool {
    let mut pi = 0usize;
    let mut vi = 0usize;
    let mut star_p = None;
    let mut star_v = 0usize;
    while vi < val.len() {
        if pi < pat.len() && (pat[pi] == b'?' || pat[pi] == val[vi]) {
            pi += 1;
            vi += 1;
        } else if pi < pat.len() && pat[pi] == b'*' {
            star_p = Some(pi);
            star_v = vi;
            pi += 1;
        } else if let Some(sp) = star_p {
            pi = sp + 1;
            star_v += 1;
            vi = star_v;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }
    pi == pat.len()
}

fn lang_from_ext(ext: &str) -> String {
    match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust".into(),
        "java" => "java".into(),
        "c" | "h" => "c".into(),
        "cc" | "cpp" | "cxx" | "hpp" => "cpp".into(),
        "py" => "python".into(),
        "go" => "go".into(),
        "js" | "mjs" | "cjs" => "javascript".into(),
        "ts" | "tsx" => "typescript".into(),
        "cs" => "csharp".into(),
        "rb" => "ruby".into(),
        "php" => "php".into(),
        "kt" | "kts" => "kotlin".into(),
        "groovy" => "groovy".into(),
        "pp" => "puppet".into(),
        "erb" => "erb".into(),
        other => other.into(),
    }
}

/// Strip `@` and whitespace from an annotation CLI token.
pub fn normalize_annotation_name(raw: &str) -> String {
    raw.trim().trim_start_matches('@').trim().to_string()
}

/// Parse `--annotation @A,@B` into normalized names (empty tokens dropped).
pub fn parse_annotation_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(normalize_annotation_name)
        .filter(|s| !s.is_empty())
        .collect()
}

/// True when an Annotation node matches a normalized want (simple name or FQN).
fn annotation_name_matches(node: &Node, want: &str) -> bool {
    if want.is_empty() {
        return false;
    }
    if node.name == want {
        return true;
    }
    if let Some(simple) = node.name.rsplit('.').next()
        && simple == want
    {
        return true;
    }
    if let Some(qn) = node.qualified_name.as_deref() {
        if qn == want {
            return true;
        }
        if let Some(simple) = qn.rsplit('.').next()
            && simple == want
        {
            return true;
        }
    }
    false
}

/// Derive import package prefix: strip `import` / `static` / `.*` / `;`, take first `depth` segments.
pub fn import_package_prefix(import_name: &str, depth: usize) -> String {
    let mut s = import_name.trim();
    if let Some(rest) = s.strip_prefix("import ") {
        s = rest.trim();
    }
    if let Some(rest) = s.strip_prefix("static ") {
        s = rest.trim();
    }
    s = s.trim_end_matches(';').trim();
    if let Some(rest) = s.strip_suffix(".*") {
        s = rest.trim();
    }
    let parts: Vec<&str> = s.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return "<unknown>".into();
    }
    let take = depth.max(1).min(parts.len());
    parts[..take].join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{GraphBackend, MemoryBackend};
    use crate::columnar_snapshot::write_columnar_from_backend;
    use crate::schema::Edge;
    use crate::snapshot::SnapshotNodeStore;
    use tempfile::tempdir;

    fn sample_store() -> (tempfile::TempDir, SnapshotNodeStore) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("graph.snapshot.bin");
        let mut backend = MemoryBackend::new();
        let f1 = Node::new(NodeType::Function, "getNextPrintPackage")
            .with_qualified_name("de.metas.printing.esb.Svc.getNextPrintPackage")
            .with_file_path("src/Svc.java")
            .with_location(10, 20);
        let a1 = Node::new(NodeType::Annotation, "Path")
            .with_qualified_name("javax.ws.rs.Path")
            .with_file_path("<external>");
        let c1 = Node::new(NodeType::Class, "PRTRestServiceRoute")
            .with_qualified_name("de.metas.printing.esb.camel.PRTRestServiceRoute")
            .with_file_path("src/PRTRestServiceRoute.java")
            .with_location(1, 50);
        let base = Node::new(NodeType::Class, "RouteBuilder")
            .with_qualified_name("org.apache.camel.builder.RouteBuilder")
            .with_file_path("<external>");
        let imp =
            Node::new(NodeType::Import, "import javax.ws.rs.Path;").with_file_path("src/Svc.java");
        let imp_ejb = Node::new(NodeType::Import, "import javax.ejb.MessageDriven;")
            .with_file_path("src/OrderMDB.java");
        let imp_jms = Node::new(NodeType::Import, "import javax.jms.Topic;")
            .with_file_path("src/OrderMDB.java");
        let imp_eclipselink = Node::new(
            NodeType::Import,
            "import org.eclipse.persistence.sessions.Session;",
        )
        .with_file_path("src/Jpa.java");
        let mdb_ann = Node::new(NodeType::Annotation, "MessageDriven")
            .with_qualified_name("javax.ejb.MessageDriven")
            .with_file_path("<external>");
        let scoped_ann = Node::new(NodeType::Annotation, "SessionScoped")
            .with_qualified_name("javax.enterprise.context.SessionScoped")
            .with_file_path("<external>");
        let mdb = Node::new(NodeType::Class, "OrderMDB")
            .with_qualified_name("com.coolstore.OrderMDB")
            .with_file_path("src/OrderMDB.java")
            .with_location(1, 80);
        let cart = Node::new(NodeType::Class, "CartResource")
            .with_qualified_name("com.coolstore.CartResource")
            .with_file_path("src/CartResource.java")
            .with_location(1, 40);
        let caller = Node::new(NodeType::Function, "handle")
            .with_qualified_name("de.metas.printing.esb.Svc.handle")
            .with_file_path("src/Svc.java")
            .with_location(30, 40);
        let f1_id = f1.id;
        let a1_id = a1.id;
        let c1_id = c1.id;
        let base_id = base.id;
        let caller_id = caller.id;
        let mdb_ann_id = mdb_ann.id;
        let scoped_ann_id = scoped_ann.id;
        let mdb_id = mdb.id;
        let cart_id = cart.id;
        backend.insert_node(f1).unwrap();
        backend.insert_node(a1).unwrap();
        backend.insert_node(c1).unwrap();
        backend.insert_node(base).unwrap();
        backend.insert_node(imp).unwrap();
        backend.insert_node(imp_ejb).unwrap();
        backend.insert_node(imp_jms).unwrap();
        backend.insert_node(imp_eclipselink).unwrap();
        backend.insert_node(mdb_ann).unwrap();
        backend.insert_node(scoped_ann).unwrap();
        backend.insert_node(mdb).unwrap();
        backend.insert_node(cart).unwrap();
        backend.insert_node(caller).unwrap();
        backend
            .insert_edge(Edge::new(f1_id, a1_id, EdgeType::AnnotatedWith))
            .unwrap();
        backend
            .insert_edge(Edge::new(c1_id, base_id, EdgeType::Extends))
            .unwrap();
        backend
            .insert_edge(Edge::new(caller_id, f1_id, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(mdb_id, mdb_ann_id, EdgeType::AnnotatedWith))
            .unwrap();
        backend
            .insert_edge(Edge::new(cart_id, scoped_ann_id, EdgeType::AnnotatedWith))
            .unwrap();
        write_columnar_from_backend(&backend, &path).unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        (dir, store)
    }

    #[test]
    fn seedless_annotatedwith_keyed_and_typed() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .relations(
                None,
                EdgeType::AnnotatedWith,
                RelationDirection::Out,
                Some(NodeType::Function),
                Some(NodeType::Annotation),
                1,
                &QueryFilters {
                    scope: Some("de.metas.printing.esb".into()),
                    scope_mode: ScopeMode::Inside,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.edges[0].source.node_type, "function");
        assert_eq!(res.edges[0].target.node_type, "annotation");
        assert_eq!(res.edges[0].source.name, "getNextPrintPackage");
        assert_eq!(res.edges[0].target.name, "Path");
    }

    #[test]
    fn extends_keyed_source_is_subtype() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .relations(
                None,
                EdgeType::Extends,
                RelationDirection::Out,
                Some(NodeType::Class),
                None,
                1,
                &QueryFilters::default(),
            )
            .unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.edges[0].source.name, "PRTRestServiceRoute");
        assert_eq!(res.edges[0].target.name, "RouteBuilder");
    }

    #[test]
    fn inventory_type_includes_zeros() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .inventory(InventoryBy::Type, &QueryFilters::default())
            .unwrap();
        let config = res
            .counts
            .iter()
            .find(|c| c.key == "configkey")
            .expect("configkey present");
        assert_eq!(config.count, 0);
        let functions = res.counts.iter().find(|c| c.key == "function").unwrap();
        assert!(functions.count >= 2);
    }

    #[test]
    fn find_import_prefix() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .find(
                Some("import javax*"),
                &QueryFilters {
                    node_type: Some(NodeType::Import),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(res.total >= 1);
        assert!(
            res.entities
                .iter()
                .any(|e| e.name.starts_with("import javax"))
        );
    }

    #[test]
    fn find_by_annotation_message_driven() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .find(
                None,
                &QueryFilters {
                    annotation_names: Some(vec!["MessageDriven".into()]),
                    node_type: Some(NodeType::Class),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.entities[0].name, "OrderMDB");
    }

    #[test]
    fn find_by_annotation_or_list() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .find(
                None,
                &QueryFilters {
                    annotation_names: Some(vec!["MessageDriven".into(), "SessionScoped".into()]),
                    node_type: Some(NodeType::Class),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(res.total, 2);
        let names: HashSet<_> = res.entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains("OrderMDB"));
        assert!(names.contains("CartResource"));
    }

    #[test]
    fn find_by_annotation_at_prefix_and_fqn() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let list = parse_annotation_list("@SessionScoped,javax.ejb.MessageDriven");
        let res = q
            .find(
                None,
                &QueryFilters {
                    annotation_names: Some(list),
                    node_type: Some(NodeType::Class),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(res.total, 2);
    }

    #[test]
    fn show_attributes_errors_when_not_indexed() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let err = q
            .find(
                None,
                &QueryFilters {
                    annotation_names: Some(vec!["Resource".into()]),
                    show_attributes: true,
                    ..Default::default()
                },
            )
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not indexed") || msg.contains("attributes"));
    }

    #[test]
    fn show_attributes_attaches_from_index() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        // Resolve OrderMDB id from a plain find first.
        let base = q
            .find(
                None,
                &QueryFilters {
                    annotation_names: Some(vec!["MessageDriven".into()]),
                    node_type: Some(NodeType::Class),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(base.total, 1);
        let id = base.entities[0].id.clone();
        let mut idx = HashMap::new();
        idx.insert(
            format!("{id}\0MessageDriven"),
            "(mappedName=\"jms/orders\")".into(),
        );
        let res = q
            .find(
                None,
                &QueryFilters {
                    annotation_names: Some(vec!["MessageDriven".into()]),
                    node_type: Some(NodeType::Class),
                    show_attributes: true,
                    annotation_arg_index: Some(idx),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(
            res.entities[0]
                .attributes
                .as_deref()
                .unwrap_or("")
                .contains("jms/orders")
        );
    }

    #[test]
    fn inventory_import_prefix_census() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .inventory(InventoryBy::ImportPrefix, &QueryFilters::default())
            .unwrap();
        assert_eq!(res.by, "import-prefix");
        let keys: HashMap<_, _> = res
            .counts
            .iter()
            .map(|c| (c.key.as_str(), c.count))
            .collect();
        assert!(keys.get("javax.ws").copied().unwrap_or(0) >= 1);
        assert!(keys.get("javax.ejb").copied().unwrap_or(0) >= 1);
        assert!(keys.get("javax.jms").copied().unwrap_or(0) >= 1);
        assert!(keys.get("org.eclipse").copied().unwrap_or(0) >= 1);
    }

    #[test]
    fn import_package_prefix_heuristic() {
        assert_eq!(
            import_package_prefix("import javax.ejb.MessageDriven;", 2),
            "javax.ejb"
        );
        assert_eq!(
            import_package_prefix("import static org.junit.Assert.*;", 2),
            "org.junit"
        );
        assert_eq!(
            import_package_prefix("com.fasterxml.jackson.databind.ObjectMapper", 2),
            "com.fasterxml"
        );
    }

    #[test]
    fn find_mdb_suffix_glob() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .find(
                Some("*MDB*"),
                &QueryFilters {
                    node_type: Some(NodeType::Class),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.entities[0].name, "OrderMDB");
    }

    #[test]
    fn callers_depth_one() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .call_neighbors("getNextPrintPackage", true, 1, &QueryFilters::default())
            .unwrap();
        assert_eq!(res.returned, 1);
        assert_eq!(res.neighbors[0].name, "handle");
    }

    #[test]
    fn seedless_calls_dedupes_with_occurrences() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("graph.snapshot.bin");
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "assumeNotEmpty")
            .with_file_path("Check.java")
            .with_location(290, 300);
        let b = Node::new(NodeType::Function, "assume")
            .with_file_path("Check.java")
            .with_location(138, 150);
        let a_id = a.id;
        let b_id = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        for _ in 0..4 {
            backend
                .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
                .unwrap();
        }
        write_columnar_from_backend(&backend, &path).unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        let q = StructuredQuery::new(&store);
        let res = q
            .relations(
                None,
                EdgeType::Calls,
                RelationDirection::Out,
                None,
                None,
                1,
                &QueryFilters::default(),
            )
            .unwrap();
        assert_eq!(res.total, 1, "distinct relationships");
        assert_eq!(res.returned, 1);
        // Columnar write collapses identical (from,to,type) rows (INV-2); multiplicity
        // is not retained on disk. Aggregation still reports one occurrence per row.
        assert_eq!(res.edges[0].occurrences, 1);
        assert_eq!(res.schema_version, STRUCTURED_QUERY_SCHEMA_VERSION);
        assert_eq!(res.edges[0].source.name, "assumeNotEmpty");
        assert_eq!(res.edges[0].target.name, "assume");
    }

    #[test]
    fn ambiguous_symbol_includes_candidates() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("graph.snapshot.bin");
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "assume")
            .with_file_path("A.java")
            .with_location(10, 20);
        let b = Node::new(NodeType::Function, "assume")
            .with_file_path("B.java")
            .with_location(30, 40);
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        write_columnar_from_backend(&backend, &path).unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        let q = StructuredQuery::new(&store);
        let err = q
            .resolve_symbol("assume", &QueryFilters::default())
            .unwrap_err();
        match err {
            Error::AmbiguousSymbol {
                count, candidates, ..
            } => {
                assert_eq!(count, 2);
                assert_eq!(candidates.len(), 2);
                assert!(
                    candidates
                        .iter()
                        .any(|c| c.file.as_deref() == Some("A.java"))
                );
            }
            other => panic!("expected AmbiguousSymbol, got {other:?}"),
        }
        let ok = q
            .resolve_symbol(
                "assume",
                &QueryFilters {
                    line: Some(30),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(ok.start_line, Some(30));
    }

    #[test]
    fn inventory_edge_distinct_matches_relations_total() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("graph.snapshot.bin");
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "assumeNotEmpty")
            .with_file_path("Check.java")
            .with_location(290, 300);
        let b = Node::new(NodeType::Function, "assume")
            .with_file_path("Check.java")
            .with_location(138, 150);
        let a_id = a.id;
        let b_id = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        for _ in 0..4 {
            backend
                .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
                .unwrap();
        }
        write_columnar_from_backend(&backend, &path).unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        let q = StructuredQuery::new(&store);
        let inv = q
            .inventory(InventoryBy::Edge, &QueryFilters::default())
            .unwrap();
        let calls = inv
            .counts
            .iter()
            .find(|c| c.key == "calls")
            .expect("calls bucket");
        assert_eq!(calls.count, 1, "distinct relationships");
        // Writer dedupes identical edges; on-disk occurrences == distinct count.
        assert_eq!(calls.occurrences, Some(1));
        let rel = q
            .relations(
                None,
                EdgeType::Calls,
                RelationDirection::Out,
                None,
                None,
                1,
                &QueryFilters::default(),
            )
            .unwrap();
        assert_eq!(rel.total, calls.count);
        assert_eq!(rel.edges[0].occurrences, calls.occurrences.unwrap());
    }

    #[test]
    fn glob_contains() {
        assert!(glob_match("*Service*", "PRTRestServiceRoute"));
        assert!(!glob_match("*Service*", "Path"));
    }

    #[test]
    fn normalize_scope_separators() {
        assert_eq!(normalize_scope_key(r"App\Http"), "App/Http");
        assert_eq!(normalize_scope_key("App.Http"), "App/Http");
        assert_eq!(normalize_scope_key("App/Http"), "App/Http");
    }

    #[test]
    fn normalize_file_basename_glob() {
        assert_eq!(
            normalize_file_glob("PRTRestServiceRoute.java"),
            "*PRTRestServiceRoute.java"
        );
        assert_eq!(
            normalize_file_glob("*/printing/**/*.java"),
            "*/printing/**/*.java"
        );
        assert_eq!(normalize_file_glob("*Route.java"), "*Route.java");
    }

    #[test]
    fn scope_falls_back_to_file_path_when_qn_absent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("snap.bin");
        let mut backend = MemoryBackend::new();
        let n =
            Node::new(NodeType::Function, "ServeHTTP").with_file_path("/tmp/pkg/util/handler.go");
        // No qualified_name — Go/TS style.
        backend.insert_node(n).unwrap();
        write_columnar_from_backend(&backend, &path).unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        let q = StructuredQuery::new(&store);
        let hit = q
            .find(
                Some("ServeHTTP"),
                &QueryFilters {
                    scope: Some("pkg/util".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(hit.total, 1);
        let miss = q
            .find(
                Some("ServeHTTP"),
                &QueryFilters {
                    scope: Some("pkg/other".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(miss.total, 0);
    }

    #[test]
    fn scope_php_backslash_matches_slash_qn() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("snap.bin");
        let mut backend = MemoryBackend::new();
        let n = Node::new(NodeType::Class, "Controller")
            .with_qualified_name(r"App\Http\Controllers\Controller");
        backend.insert_node(n).unwrap();
        write_columnar_from_backend(&backend, &path).unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        let q = StructuredQuery::new(&store);
        let hit = q
            .find(
                Some("Controller"),
                &QueryFilters {
                    scope: Some("App/Http".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(hit.total, 1);
    }

    #[test]
    fn file_filter_accepts_basename() {
        let (_dir, store) = sample_store();
        let q = StructuredQuery::new(&store);
        let res = q
            .find(
                Some("*Service*"),
                &QueryFilters {
                    file_glob: Some("PRTRestServiceRoute.java".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.entities[0].name, "PRTRestServiceRoute");
    }

    #[test]
    fn reject_unknown_edge_and_type() {
        assert!(parse_edge_type("not_a_real_edge").is_err());
        assert!(parse_node_type("not_a_type").is_err());
        assert!(parse_edge_type("annotatedwith").is_ok());
        assert!(parse_node_type("import").is_ok());
    }
}
