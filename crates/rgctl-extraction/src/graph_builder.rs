//! Maps extracted symbols and relations into graph nodes and edges.

use crate::extractor::SymbolPass1Prep;
use rgctl_error::{Error, Result};
use rgctl_graph::code_index::{CodeIndex, hash_code};
use rgctl_graph::content_store::{ContentStore, INLINE_BODY_MAX_BYTES, hash_bytes};
use rgctl_graph::migration::graph_parameter_from_plugin;
use rgctl_graph::normalize_path_str;
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::segmented_spill::{FinishedSpill, SegmentedSpill};
use rgctl_graph::structural_sketch::build_token_bloom;
use rgctl_plugin_api::{
    ComplexityMetrics, ConfigKey, Relation, RelationType, Symbol, SymbolType,
};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
struct LineSpan {
    start: usize,
    end: usize,
    id: Uuid,
}

/// Builds graph nodes and edges from extracted data.
#[derive(Default)]
pub struct GraphBuilder {
    symbol_index: HashMap<String, Uuid>,
    file_nodes: HashMap<String, Uuid>,
    /// Normalized path + component-boundary suffixes → file node id (O(1) lookup).
    file_path_lookup: HashMap<String, Uuid>,
    config_key_nodes: HashMap<String, Uuid>,
    env_nodes: HashMap<String, Uuid>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// Optional disk spill — when set, nodes/edges are not kept in `Vec`s.
    spill: Option<SegmentedSpill>,
    spill_error: Option<String>,
    spilled_nodes: usize,
    spilled_edges: usize,
    /// Line ranges for config-usage resolution when nodes are spilled.
    file_line_spans: HashMap<String, Vec<LineSpan>>,
    code_index: Option<CodeIndex>,
    content_store: Option<ContentStore>,
    // Resolution performance tracking
    resolution_stats: ResolutionStats,
    // Fast resolution indexes (built on demand)
    /// Qualified name → candidate UUIDs (may be ambiguous when FQNs collide).
    symbols_by_qualified: HashMap<String, Vec<Uuid>>,
    symbols_by_suffix: HashMap<String, Vec<Uuid>>,
    indexes_built: bool,
    /// `OwnerType.field` → simple field type (for late Go selector resolution; spill-safe).
    field_type_index: HashMap<String, String>,
    /// Symbol node id → defining file path (for C header/.c disambiguation).
    symbol_files: HashMap<Uuid, String>,
    /// Node id → kind (spill-safe; used to reject non-callable `Calls` targets).
    node_types: HashMap<Uuid, NodeType>,
    /// Precomputed bare-name → `.c` implementation when header/translation-unit pairs collide.
    c_preferred_impl: HashMap<String, Uuid>,
    /// Lazy cache for bare suffix lookups during pass 2 (positive hits only).
    suffix_resolve_cache: HashMap<String, Uuid>,
    /// When false (default discover), `Symbol.fields` stay on symbols only — no Variable nodes.
    materialize_fields: bool,
    /// Path → node ids accumulated during commit (feeds FileTracker without a full mmap scan).
    tracker_mapping: HashMap<String, Vec<Uuid>>,
    /// Normalized path for the current pass-1 file (avoids re-normalize + re-hash per symbol).
    active_tracker_key: Option<String>,
    /// Node ids for the active file batch — flushed once in [`Self::end_file_batch`].
    active_tracker_ids: Vec<Uuid>,
    /// AnnotatedWith argument text keyed for sidecar (columnar edges drop properties).
    annotation_args: Vec<AnnotationArgEntry>,
}

/// One annotation usage with argument text (written to `.rgctl/annotation_args.json`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AnnotationArgEntry {
    pub source_id: String,
    pub annotation: String,
    pub arguments: String,
}

#[derive(Debug, Default)]
struct ResolutionStats {
    total_calls: usize,
    hashmap_hits: usize,
    qualified_hint_scans: usize,
    qualified_hint_hits: usize, // O(1) index hits
    type_hint_scans: usize,
    type_hint_hits: usize, // O(1) index hits
    fuzzy_scans: usize,
    fuzzy_hits: usize, // O(1) index hits
    total_time: std::time::Duration,
    line_lookups: usize,
    line_lookup_time: std::time::Duration,
}

impl GraphBuilder {
    /// Create an empty graph builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder that spills nodes/edges to `spill_dir` instead of retaining Vecs.
    pub fn with_spill(spill_dir: impl AsRef<Path>) -> Result<Self> {
        let mut builder = Self::new();
        builder.spill = Some(SegmentedSpill::create(spill_dir)?);
        Ok(builder)
    }

    /// When true, materialize `Symbol.fields` as `Variable` graph nodes (CPG / `--with-cfg`).
    pub fn set_materialize_fields(&mut self, materialize: bool) {
        self.materialize_fields = materialize;
    }

    /// Whether field members are emitted as graph nodes.
    pub fn materialize_fields(&self) -> bool {
        self.materialize_fields
    }

    /// Whether this builder is spilling to disk.
    pub fn is_spilling(&self) -> bool {
        self.spill.is_some()
    }

    /// Number of nodes built so far.
    pub fn node_count(&self) -> usize {
        if self.spill.is_some() {
            self.spilled_nodes
        } else {
            self.nodes.len()
        }
    }

    /// Number of edges built so far.
    pub fn edge_count(&self) -> usize {
        if self.spill.is_some() {
            self.spilled_edges
        } else {
            self.edges.len()
        }
    }

    /// Pin tracker mapping key for the duration of one file's pass-1 insert.
    pub fn begin_file_batch(&mut self, path: &Path) {
        if self.active_tracker_key.is_some() {
            self.end_file_batch();
        }
        let key = normalize_path_str(&path.to_string_lossy()).into_owned();
        self.active_tracker_key = Some(key);
        self.active_tracker_ids.clear();
    }

    /// Flush active file node ids into `tracker_mapping` (once per file, not per symbol).
    pub fn end_file_batch(&mut self) {
        if let Some(key) = self.active_tracker_key.take() {
            if !self.active_tracker_ids.is_empty() {
                let ids = std::mem::take(&mut self.active_tracker_ids);
                self.tracker_mapping.entry(key).or_default().extend(ids);
            }
        } else {
            self.active_tracker_ids.clear();
        }
    }

    fn record_line_span(&mut self, node: &Node) {
        let Some(file) = node.file_path.as_deref() else {
            return;
        };
        let Some(start) = node.start_line else {
            return;
        };
        let end = node.end_line.unwrap_or(start);
        let span = LineSpan {
            start,
            end,
            id: node.id,
        };
        if let Some(spans) = self.file_line_spans.get_mut(file) {
            spans.push(span);
        } else {
            self.file_line_spans.insert(file.to_string(), vec![span]);
        }
    }

    fn index_symbol_resolution(&mut self, key: &str, node: &Node) {
        if let Some(qualified) = &node.qualified_name {
            self.symbols_by_qualified
                .entry(qualified.to_string())
                .or_default()
                .push(node.id);
            // Package-qualified suffixes (Java/Go): `com.foo.Bar.baz` → `Bar.baz`, `baz`.
            // Skip for C struct field nodes (`Struct.field`, label `field`) — bare names
            // like `data` would collide across the whole kernel.
            let is_field_member = node.has_label("field");
            let parts: Vec<&str> = qualified.split('.').collect();
            if !is_field_member && parts.len() >= 3 {
                for i in 0..parts.len() {
                    let suffix = parts[i..].join(".");
                    self.symbols_by_suffix
                        .entry(suffix)
                        .or_default()
                        .push(node.id);
                }
            }
            // Ruby method QN uses `#` / `.` (e.g. `OrderDTO#mark_processed`, `OrderService.build`).
            if !is_field_member {
                if let Some((_, method)) = qualified.rsplit_once('#')
                    && !method.is_empty()
                {
                    self.symbols_by_suffix
                        .entry(method.to_string())
                        .or_default()
                        .push(node.id);
                } else if let Some((_, method)) = qualified.rsplit_once('.')
                    && !method.is_empty()
                {
                    self.symbols_by_suffix
                        .entry(method.to_string())
                        .or_default()
                        .push(node.id);
                }
            }
        } else {
            self.symbols_by_suffix
                .entry(node.name.to_string())
                .or_default()
                .push(node.id);
        }
        // `symbol_key` is `file::name` or `file::a::b::…` / `file::Qualified.Name`.
        // For unqualified `file::name` the bare-name insert above already covers
        // resolution — skip to avoid a duplicate push + alloc per C-like symbol.
        match key.matches("::").count() {
            0 => {}
            1 => {
                if let Some((_, tail)) = key.split_once("::") {
                    let duplicate_bare =
                        node.qualified_name.is_none() && tail == node.name.as_str();
                    if !duplicate_bare {
                        self.symbols_by_suffix
                            .entry(tail.to_string())
                            .or_default()
                            .push(node.id);
                    }
                }
            }
            _ => {
                let parts: Vec<&str> = key.split("::").collect();
                for i in 1..parts.len() {
                    let suffix = parts[i..].join("::");
                    self.symbols_by_suffix
                        .entry(suffix)
                        .or_default()
                        .push(node.id);
                }
            }
        }
    }

    fn commit_node(&mut self, node: Node) {
        self.node_types.insert(node.id, node.node_type);
        self.record_line_span(&node);
        self.record_tracker_mapping(&node);
        if let Some(spill) = self.spill.as_mut() {
            if let Err(e) = spill.append_node(&node) {
                self.spill_error = Some(e.to_string());
            } else {
                self.spilled_nodes += 1;
            }
        } else {
            self.nodes.push(node);
        }
    }

    fn record_tracker_mapping(&mut self, node: &Node) {
        if self.active_tracker_key.is_some() {
            self.active_tracker_ids.push(node.id);
            return;
        }
        let path = node.file_path.as_deref().or_else(|| {
            if matches!(node.node_type, NodeType::File) {
                Some(node.name.as_str())
            } else {
                None
            }
        });
        let Some(path) = path else {
            return;
        };
        let key = normalize_path_str(path).into_owned();
        self.tracker_mapping.entry(key).or_default().push(node.id);
    }

    fn commit_edge(&mut self, edge: Edge) {
        if let Some(spill) = self.spill.as_mut() {
            if let Err(e) = spill.append_edge(&edge) {
                self.spill_error = Some(e.to_string());
            } else {
                self.spilled_edges += 1;
            }
        } else {
            self.edges.push(edge);
        }
    }

    /// Ensure a file node exists and return its ID.
    pub fn ensure_file_node(&mut self, path: &Path) -> Uuid {
        self.ensure_file_node_with_source(path, None)
    }

    /// Ensure a file node exists, optionally recording full-file content hash / blob ref.
    pub fn ensure_file_node_with_source(&mut self, path: &Path, source: Option<&[u8]>) -> Uuid {
        let file_path = path.to_string_lossy().to_string();
        if let Some(id) = self.file_nodes.get(&file_path) {
            return *id;
        }

        let mut node =
            Node::new(NodeType::File, file_path.clone()).with_file_path(file_path.clone());
        if let Some(bytes) = source {
            let content_hash = hash_bytes(bytes);
            node = node.with_property("content_hash".to_string(), content_hash.clone());
            if bytes.len() > INLINE_BODY_MAX_BYTES
                && let Some(store) = self.content_store.as_mut()
            {
                store.insert_bytes(&content_hash, bytes.to_vec());
                node = node.with_property("blob_ref".to_string(), content_hash);
            }
        }
        let id = node.id;
        let norm = normalize_file_key(&file_path);
        self.file_nodes.insert(file_path, id);
        register_file_lookup_keys(&mut self.file_path_lookup, &norm, id);
        self.commit_node(node);
        id
    }

    /// Attach a code index for body hashing during symbol insertion.
    pub fn set_code_index(&mut self, index: CodeIndex) {
        self.code_index = Some(index);
    }

    /// Mutable access to the optional code index.
    pub fn code_index_mut(&mut self) -> Option<&mut CodeIndex> {
        self.code_index.as_mut()
    }

    /// Attach a content blob store for truncated markdown bodies and large files.
    pub fn set_content_store(&mut self, store: ContentStore) {
        self.content_store = Some(store);
    }

    /// Mutable access to the optional content store.
    pub fn content_store_mut(&mut self) -> Option<&mut ContentStore> {
        self.content_store.as_mut()
    }

    /// Merge out-of-line content blobs from extraction.
    pub fn merge_content_blobs(&mut self, blobs: &std::collections::HashMap<String, String>) {
        if let Some(store) = self.content_store.as_mut() {
            store.merge_text_blobs(blobs);
        }
    }

    /// Take the accumulated content store after graph build.
    pub fn take_content_store(&mut self) -> Option<ContentStore> {
        self.content_store.take()
    }

    /// Take the accumulated code index after graph build.
    pub fn take_code_index(&mut self) -> Option<CodeIndex> {
        self.code_index.take()
    }

    /// Take the path → node-id mapping accumulated during commit (for FileTracker).
    pub fn take_tracker_mapping(&mut self) -> HashMap<String, Vec<Uuid>> {
        self.end_file_batch();
        std::mem::take(&mut self.tracker_mapping)
    }

    /// Add a symbol node linked to its file.
    pub fn add_symbol(&mut self, symbol: &Symbol, file_id: Uuid) -> Uuid {
        self.add_symbol_with_prep(symbol, file_id, None, None)
    }

    /// Add a symbol node and optionally hash its body for change detection.
    pub fn add_symbol_with_body(
        &mut self,
        symbol: &Symbol,
        file_id: Uuid,
        body: Option<&str>,
    ) -> Uuid {
        self.add_symbol_with_prep(symbol, file_id, body, None)
    }

    /// Add a symbol, preferring worker-precomputed hash/bloom when `prep` is set.
    pub fn add_symbol_with_prep(
        &mut self,
        symbol: &Symbol,
        file_id: Uuid,
        body: Option<&str>,
        prep: Option<&SymbolPass1Prep>,
    ) -> Uuid {
        let mut key = symbol_key(
            &symbol.location.file,
            &symbol.name,
            symbol.qualified_name.as_deref(),
        );
        // C `#include` paths are per-translation-unit; dedupe globally by path string
        // so kernel-scale repos do not materialize one Import node per occurrence.
        if symbol.symbol_type == SymbolType::Import
            && symbol
                .metadata
                .get("kind")
                .and_then(|v| v.as_str())
                == Some("include")
        {
            key = format!("c_include::{}", symbol.name);
            if let Some(&id) = self.symbol_index.get(&key) {
                self.add_edge(file_id, id, EdgeType::Uses);
                return id;
            }
        }
        if let Some(id) = self.symbol_index.get(&key) {
            return *id;
        }

        let mut node = Node::new(
            symbol_type_to_node_type(symbol.symbol_type),
            symbol.name.clone(),
        )
        .with_file_path(symbol.location.file.clone())
        .with_location(symbol.location.start_line, symbol.location.end_line);

        if let Some(qn) = &symbol.qualified_name {
            node = node.with_qualified_name(qn.clone());
        }
        if let Some(sig) = &symbol.signature {
            node = node.with_signature(sig.clone());
        }
        if let Some(ret) = &symbol.return_type {
            node = node.with_return_type(ret.clone());
        }
        if !symbol.parameters.is_empty() {
            node = node.with_parameters(
                symbol
                    .parameters
                    .iter()
                    .cloned()
                    .map(graph_parameter_from_plugin)
                    .collect(),
            );
        }

        let code_hash = prep
            .and_then(|p| p.code_hash.clone())
            .or_else(|| {
                body.map(|b| {
                    if let Some(index) = self.code_index.as_mut() {
                        index.add_code(b, &symbol.location)
                    } else {
                        hash_code(b)
                    }
                })
            });
        if let Some(code_hash) = code_hash {
            node = node.with_code_hash(code_hash);
        }

        if let Some(bloom) = prep.and_then(|p| p.token_bloom) {
            node = node.with_token_bloom(bloom);
        } else if should_sketch_symbol(symbol.symbol_type) {
            let bloom = build_token_bloom(
                &symbol.name,
                symbol.qualified_name.as_deref(),
                symbol.signature.as_deref(),
                body,
            );
            node = node.with_token_bloom(bloom);
        }
        if !symbol.modifiers.is_empty() {
            node = node.with_property("modifiers".to_string(), symbol.modifiers.join(" "));
        }
        if let Some(doc) = &symbol.documentation {
            node = node.with_property("documentation".to_string(), doc.clone());
        }
        if let Some(obj) = symbol.metadata.as_object() {
            for (k, v) in obj {
                let prop_val = match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Bool(b) => Some(b.to_string()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                };
                if let Some(s) = prop_val {
                    node = node.with_property(k.clone(), s);
                }
            }
        }

        let id = node.id;
        self.symbol_index.insert(key.clone(), id);
        self.symbol_files
            .insert(id, symbol.location.file.clone());
        self.index_symbol_resolution(&key, &node);
        self.commit_node(node);
        self.add_edge(id, file_id, EdgeType::DefinedIn);
        self.add_edge(file_id, id, EdgeType::Contains);
        if self.materialize_fields {
            self.add_field_members(id, symbol, file_id);
        }
        id
    }

    /// Materialize `Symbol.fields` as Variable nodes contained by the owning type.
    ///
    /// Enables hybrid CPG member queries without waiting for per-language Variable
    /// symbols. Idempotent via `symbol_index` keys (`Owner.field` FQN).
    /// Only runs when [`Self::materialize_fields`] is true (`--with-cfg` discover).
    fn add_field_members(&mut self, owner_id: Uuid, symbol: &Symbol, file_id: Uuid) {
        if symbol.fields.is_empty() {
            return;
        }
        let owner_qn = symbol
            .qualified_name
            .clone()
            .unwrap_or_else(|| symbol.name.clone());
        for field in &symbol.fields {
            let field_qn = format!("{owner_qn}.{}", field.name);
            let key = symbol_key(&symbol.location.file, &field.name, Some(field_qn.as_str()));
            if self.symbol_index.contains_key(&key) {
                continue;
            }
            let mut node = Node::new(NodeType::Variable, field.name.clone())
                .with_file_path(symbol.location.file.clone())
                .with_location(symbol.location.start_line, symbol.location.end_line)
                .with_qualified_name(field_qn.clone())
                .with_label("field".to_string())
                .with_property("member_of".to_string(), symbol.name.clone())
                .with_property("owner_qualified_name".to_string(), owner_qn.clone());
            if let Some(ty) = &field.field_type {
                node = node.with_property("field_type".to_string(), ty.clone());
                let simple = go_simple_type_from_str(ty);
                self.field_type_index
                    .insert(format!("{owner_qn}.{}", field.name), simple.clone());
                // Also index by bare owner name when owner_qn == struct name.
                self.field_type_index
                    .insert(format!("{}.{}", symbol.name, field.name), simple);
            }
            if let Some(vis) = &field.visibility {
                node = node.with_property("visibility".to_string(), vis.clone());
            }
            let field_id = node.id;
            self.symbol_index.insert(key.clone(), field_id);
            self.index_symbol_resolution(&key, &node);
            self.commit_node(node);
            self.add_edge(field_id, file_id, EdgeType::DefinedIn);
            self.add_edge(file_id, field_id, EdgeType::Contains);
            self.add_edge(owner_id, field_id, EdgeType::Contains);
        }
    }

    /// Attach complexity metrics to an existing symbol node.
    ///
    /// Only supported in in-memory mode (not when spilling).
    pub fn add_complexity(&mut self, symbol: &Symbol, metrics: &ComplexityMetrics) {
        if self.spill.is_some() {
            return;
        }
        let key = symbol_key(
            &symbol.location.file,
            &symbol.name,
            symbol.qualified_name.as_deref(),
        );
        if let Some(id) = self.symbol_index.get(&key)
            && let Some(node) = self.nodes.iter_mut().find(|n| n.id == *id)
        {
            node.properties
                .insert("cyclomatic".to_string(), metrics.cyclomatic.to_string());
            node.properties
                .insert("cognitive".to_string(), metrics.cognitive.to_string());
            node.properties
                .insert("loc".to_string(), metrics.loc.to_string());
            node.properties.insert(
                "nesting_depth".to_string(),
                metrics.nesting_depth.to_string(),
            );
        }
    }

    /// Build reverse indexes for fast symbol resolution.
    ///
    /// Call this after all symbols are added but before processing relations.
    /// Indexes are maintained incrementally at insert; this finalizes the flag.
    pub fn build_resolution_indexes(&mut self) {
        use tracing::info;

        if self.indexes_built {
            return;
        }

        // Suffix / qualified indexes are filled in `index_symbol_resolution` at insert.
        // Rebuild suffix from symbol_index only if somehow empty (legacy / partial path).
        if self.symbols_by_suffix.is_empty() && !self.symbol_index.is_empty() {
            for key in self.symbol_index.keys() {
                let parts: Vec<&str> = key.split("::").collect();
                for i in 1..parts.len() {
                    let suffix = parts[i..].join("::");
                    if let Some(uuid) = self.symbol_index.get(key) {
                        let entry = self.symbols_by_suffix.entry(suffix).or_default();
                        if !entry.contains(uuid) {
                            entry.push(*uuid);
                        }
                    }
                }
            }
        }

        self.build_c_preferred_impl();

        self.indexes_built = true;

        info!(
            symbol_count = self.symbol_index.len(),
            qualified_count = self.symbols_by_qualified.len(),
            suffix_count = self.symbols_by_suffix.len(),
            c_preferred_impl = self.c_preferred_impl.len(),
            "built resolution indexes"
        );
    }

    /// Precompute `.c` preference for header/implementation name collisions (C Tier 1).
    fn build_c_preferred_impl(&mut self) {
        self.c_preferred_impl.clear();
        self.suffix_resolve_cache.clear();
        for (suffix, ids) in &self.symbols_by_suffix {
            if unique_resolved(ids).is_some() {
                continue;
            }
            if let Some(id) = single_c_preferred_impl(ids, &self.symbol_files) {
                self.c_preferred_impl.insert(suffix.clone(), id);
            }
        }
    }

    /// Add a configuration key node linked to its file.
    pub fn add_config_key(&mut self, key: &ConfigKey, file_id: Uuid) -> Uuid {
        let lookup = format!("{}::{}", key.location.file, key.key_path);
        if let Some(id) = self.config_key_nodes.get(&lookup) {
            return *id;
        }

        let node = Node::new(NodeType::ConfigKey, key.key_path.clone())
            .with_file_path(key.location.file.clone())
            .with_property("value".to_string(), key.value.clone())
            .with_property("value_type".to_string(), format!("{:?}", key.value_type));

        let id = node.id;
        self.config_key_nodes.insert(lookup, id);
        self.commit_node(node);
        self.add_edge(id, file_id, EdgeType::DefinedIn);
        self.add_edge(file_id, id, EdgeType::Contains);
        id
    }

    /// Add a relation between symbols when endpoints resolve.
    ///
    /// For allowlisted relation kinds, missing endpoints become deduplicated
    /// external stub nodes (`is_external_stub`) so Instantiates / JPMS /
    /// AnnotatedWith edges survive into GQL. Field-like `References` are not
    /// stubbed (they must not invent Class nodes named like fields).
    pub fn add_relation(&mut self, relation: &Relation) -> Result<()> {
        // Fast reject: unresolved cross-file C calls (no stub policy for Calls).
        if relation.relation_type == RelationType::Calls
            && relation.metadata.get("language").and_then(|v| v.as_str()) == Some("c")
            && !self.c_preferred_impl.contains_key(&relation.to)
            && !self.suffix_resolve_cache.contains_key(&relation.to)
        {
            let can_resolve = self
                .symbols_by_suffix
                .get(&relation.to)
                .is_some_and(|ids| {
                    unique_resolved(ids).is_some()
                        || single_c_preferred_impl(ids, &self.symbol_files).is_some()
                });
            if !can_resolve {
                return Ok(());
            }
        }

        let mut from_id =
            self.resolve_symbol_tracked(&relation.from, &relation.location.file, None, None);
        if from_id.is_none() {
            from_id = self
                .symbols_by_qualified
                .get(&relation.from)
                .and_then(|ids| unique_resolved(ids));
        }
        if from_id.is_none() {
            from_id = self.lookup_file_node(&relation.from, &relation.location.file);
        }

        let (to_type_hint, to_qualified_hint) = self.enrich_go_type_hints(relation);

        let mut to_id = self.resolve_symbol_tracked(
            &relation.to,
            &relation.location.file,
            to_qualified_hint
                .as_deref()
                .or(relation.to_qualified_hint.as_deref()),
            to_type_hint.as_deref().or(relation.to_type_hint.as_deref()),
        );
        if to_id.is_none() {
            if let Some(id) = self.lookup_file_node(&relation.to, &relation.location.file) {
                to_id = Some(id);
            } else if relation
                .to_type_hint
                .as_deref()
                .is_some_and(|hint| hint.eq_ignore_ascii_case("file"))
            {
                return Ok(());
            }
        }

        if relation_allows_external_stub(relation.relation_type) {
            // Only stub unresolved *targets*. Stubbing missing `from` when
            // resolution is ambiguous invents phantom callers and inflates
            // Function counts on large Java repos.
            if from_id.is_some() && to_id.is_none() {
                let qn = to_qualified_hint
                    .as_deref()
                    .or(relation.to_qualified_hint.as_deref());
                to_id = Some(self.ensure_external_stub(
                    &relation.to,
                    qn,
                    stub_node_type_for_target(relation),
                ));
            }
        }

        // Calls must target callables only — never Class field stubs via fuzzy suffix.
        if relation.relation_type == RelationType::Calls
            && let Some(to) = to_id
            && self.node_types.get(&to).copied() != Some(NodeType::Function)
        {
            to_id = None;
        }

        if let (Some(from), Some(to)) = (from_id, to_id) {
            let edge_type = relation_type_to_edge_type(relation.relation_type);
            let mut edge = Edge::new(from, to, edge_type);
            if relation.relation_type == RelationType::Calls {
                edge = edge.with_property(
                    "call_site_line".to_string(),
                    relation.location.start_line.to_string(),
                );
            }
            if relation.relation_type == RelationType::AnnotatedWith
                && let Some(args) = relation
                    .metadata
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            {
                let ann = relation
                    .to
                    .rsplit(['.', '/', ':'])
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(relation.to.as_str());
                edge = edge.with_property("arguments".into(), args.to_string());
                self.annotation_args.push(AnnotationArgEntry {
                    source_id: from.to_string(),
                    annotation: ann.to_string(),
                    arguments: args.to_string(),
                });
            }
            self.commit_edge(edge);
        }
        Ok(())
    }

    /// Resolve a file path string to a registered File node (absolute/relative tolerant).
    fn lookup_file_node(&self, path_str: &str, anchor_file: &str) -> Option<Uuid> {
        let target = normalize_file_key(path_str);
        if !target.is_empty()
            && let Some(id) = self.file_path_lookup.get(&target)
        {
            return Some(*id);
        }
        let anchor = Path::new(anchor_file);
        if let Some(parent) = anchor.parent() {
            let joined = normalize_file_key(&join_path_normalized(parent, path_str));
            if !joined.is_empty() {
                return self.file_path_lookup.get(&joined).copied();
            }
        }
        None
    }

    /// Create or reuse a stub node for an unresolved relation endpoint.
    fn ensure_external_stub(
        &mut self,
        name: &str,
        qualified_hint: Option<&str>,
        node_type: NodeType,
    ) -> Uuid {
        let simple = name
            .rsplit(['.', '/', ':'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(name);
        let qn = qualified_hint
            .filter(|s| !s.is_empty())
            .unwrap_or(name)
            .to_string();

        // Prefer an existing unique resolution (another stub or late symbol).
        if let Some(id) = self
            .symbols_by_qualified
            .get(&qn)
            .and_then(|ids| unique_resolved(ids))
        {
            return id;
        }
        if let Some(id) = self
            .symbols_by_suffix
            .get(name)
            .and_then(|ids| unique_resolved(ids))
        {
            return id;
        }
        if name != simple
            && let Some(id) = self
                .symbols_by_suffix
                .get(simple)
                .and_then(|ids| unique_resolved(ids))
        {
            return id;
        }

        const STUB_FILE: &str = "<external>";
        let key = symbol_key(STUB_FILE, simple, Some(qn.as_str()));
        if let Some(id) = self.symbol_index.get(&key) {
            return *id;
        }

        let file_id = self.ensure_file_node(Path::new(STUB_FILE));
        // JPMS / dotted module ids keep the full name; types use the last segment.
        let display_name = if node_type == NodeType::Module {
            qn.clone()
        } else {
            simple.to_string()
        };
        let node = Node::new(node_type, display_name)
            .with_qualified_name(qn.clone())
            .with_file_path(STUB_FILE.to_string())
            .with_property("is_external_stub".to_string(), "true".to_string());
        let id = node.id;
        self.symbol_index.insert(key.clone(), id);
        self.index_symbol_resolution(&key, &node);
        self.commit_node(node);
        self.add_edge(id, file_id, EdgeType::DefinedIn);
        self.add_edge(file_id, id, EdgeType::Contains);
        id
    }

    /// Late-bind Go `recv.field.Method` using field Variable nodes from Pass 1.
    fn enrich_go_type_hints(&self, relation: &Relation) -> (Option<String>, Option<String>) {
        if relation.to_type_hint.is_some() {
            return (None, None);
        }
        let lang = relation.metadata.get("language").and_then(|v| v.as_str());
        if lang != Some("go") {
            return (None, None);
        }
        let Some(recv_ty) = relation
            .metadata
            .get("go_recv_type")
            .and_then(|v| v.as_str())
        else {
            return (None, None);
        };
        let Some(field) = relation.metadata.get("go_field").and_then(|v| v.as_str()) else {
            return (None, None);
        };
        let callee = relation
            .metadata
            .get("go_callee")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                relation
                    .to
                    .split('.')
                    .next_back()
                    .unwrap_or(relation.to.as_str())
            });

        let Some(ty) = self
            .field_type_index
            .get(&format!("{recv_ty}.{field}"))
            .cloned()
        else {
            return (None, None);
        };
        let qh = format!("{ty}.{callee}");
        (Some(ty), Some(qh))
    }

    /// Link code to a configuration key or environment variable usage.
    pub fn link_config_usage(
        &mut self,
        file_path: &str,
        line: usize,
        key: &str,
        usage_type: ConfigUsageKind,
    ) {
        let file_id = self.file_nodes.get(file_path).copied();
        let code_node = self
            .find_symbol_at_line_tracked(file_path, line)
            .or(file_id);

        let Some(from_id) = code_node else {
            return;
        };

        let target_id = match usage_type {
            ConfigUsageKind::EnvVar => Some(self.ensure_env_node(key)),
            // v1: only link when a ConfigKey already exists — do not invent stubs.
            ConfigUsageKind::ConfigKey => self.find_existing_config_key(key),
        };

        let Some(target_id) = target_id else {
            return;
        };

        self.add_edge(from_id, target_id, EdgeType::UsesConfig);
    }

    /// Resolve an already-ingested ConfigKey by exact or normalized key path.
    fn find_existing_config_key(&self, key: &str) -> Option<Uuid> {
        let suffix = format!("::{key}");
        for (lookup, id) in &self.config_key_nodes {
            if lookup.ends_with(&suffix) || lookup.rsplit("::").next() == Some(key) {
                return Some(*id);
            }
        }
        let norm = crate::usage_detector::ConfigUsageDetector::normalize_key(key);
        for (lookup, id) in &self.config_key_nodes {
            let existing = lookup.rsplit("::").next().unwrap_or(lookup);
            if crate::usage_detector::ConfigUsageDetector::normalize_key(existing) == norm {
                return Some(*id);
            }
        }
        None
    }

    fn ensure_env_node(&mut self, key: &str) -> Uuid {
        if let Some(id) = self.env_nodes.get(key) {
            return *id;
        }

        let node = Node::new(NodeType::Variable, key.to_string())
            .with_label("env".to_string())
            .with_property("env_var".to_string(), key.to_string());

        let id = node.id;
        self.env_nodes.insert(key.to_string(), id);
        self.commit_node(node);
        id
    }

    #[allow(dead_code)] // retained for future stub policy / tests
    fn ensure_config_key_node(&mut self, key: &str, file_path: &str) -> Uuid {
        let lookup = format!("{file_path}::{key}");
        if let Some(id) = self.config_key_nodes.get(&lookup) {
            return *id;
        }

        let node =
            Node::new(NodeType::ConfigKey, key.to_string()).with_file_path(file_path.to_string());
        let id = node.id;
        self.config_key_nodes.insert(lookup, id);
        self.commit_node(node);
        id
    }

    fn find_symbol_at_line_tracked(&mut self, file_path: &str, line: usize) -> Option<Uuid> {
        use std::time::Instant;

        let start = Instant::now();
        self.resolution_stats.line_lookups += 1;

        let result = if self.spill.is_some() {
            self.file_line_spans.get(file_path).and_then(|spans| {
                spans
                    .iter()
                    .filter(|s| s.start <= line && s.end >= line)
                    .max_by_key(|s| s.start)
                    .map(|s| s.id)
            })
        } else {
            self.nodes
                .iter()
                .filter(|n| n.file_path.as_deref() == Some(file_path))
                .filter(|n| {
                    n.start_line
                        .map(|start| start <= line && n.end_line.unwrap_or(start) >= line)
                        .unwrap_or(false)
                })
                .max_by_key(|n| n.start_line.unwrap_or(0))
                .map(|n| n.id)
        };

        self.resolution_stats.line_lookup_time += start.elapsed();
        result
    }

    #[allow(dead_code)]
    fn find_symbol_at_line(&self, file_path: &str, line: usize) -> Option<Uuid> {
        if self.spill.is_some() {
            return self.file_line_spans.get(file_path).and_then(|spans| {
                spans
                    .iter()
                    .filter(|s| s.start <= line && s.end >= line)
                    .max_by_key(|s| s.start)
                    .map(|s| s.id)
            });
        }
        self.nodes
            .iter()
            .filter(|n| n.file_path.as_deref() == Some(file_path))
            .filter(|n| {
                n.start_line
                    .map(|start| start <= line && n.end_line.unwrap_or(start) >= line)
                    .unwrap_or(false)
            })
            .max_by_key(|n| n.start_line.unwrap_or(0))
            .map(|n| n.id)
    }

    /// Resolve a symbol name to its UUID with performance tracking.
    ///
    /// Ambiguous qualified-name or suffix matches return `None` (do not pick arbitrarily).
    fn resolve_symbol_tracked(
        &mut self,
        name: &str,
        file: &str,
        qualified_hint: Option<&str>,
        type_hint: Option<&str>,
    ) -> Option<Uuid> {
        use std::time::Instant;

        let start = Instant::now();
        self.resolution_stats.total_calls += 1;

        // 1. Try exact match in current file
        let qualified = format!("{file}::{name}");
        if let Some(id) = self.symbol_index.get(&qualified) {
            self.resolution_stats.hashmap_hits += 1;
            self.resolution_stats.total_time += start.elapsed();
            return Some(*id);
        }

        // 2. Try qualified hint direct lookup (O(1)); only if uniquely resolved
        if let Some(hint) = qualified_hint {
            self.resolution_stats.qualified_hint_scans += 1;

            if let Some(id) = self
                .symbols_by_qualified
                .get(hint)
                .and_then(|ids| unique_resolved(ids))
            {
                self.resolution_stats.qualified_hint_hits += 1;
                self.resolution_stats.total_time += start.elapsed();
                return Some(id);
            }

            if let Some(id) = self
                .symbols_by_suffix
                .get(hint)
                .and_then(|ids| unique_resolved(ids))
            {
                self.resolution_stats.qualified_hint_hits += 1;
                self.resolution_stats.total_time += start.elapsed();
                return Some(id);
            }
        }

        // 3. Try type hint + simple name (O(1)); only if uniquely resolved
        if let Some(type_name) = type_hint {
            self.resolution_stats.type_hint_scans += 1;
            let simple_name = name.split('.').next_back().unwrap_or(name);
            let type_qualified = format!("{type_name}.{simple_name}");

            if let Some(id) = self
                .symbols_by_suffix
                .get(&type_qualified)
                .and_then(|ids| unique_resolved(ids))
            {
                self.resolution_stats.type_hint_hits += 1;
                self.resolution_stats.total_time += start.elapsed();
                return Some(id);
            }
        }

        // 4. Fallback: suffix index — `.c` preference map, then lazy cache, then live lookup.
        self.resolution_stats.fuzzy_scans += 1;
        let result = if qualified_hint.is_none() && type_hint.is_none() {
            if let Some(id) = self.c_preferred_impl.get(name) {
                Some(*id)
            } else if let Some(id) = self.suffix_resolve_cache.get(name) {
                Some(*id)
            } else {
                let resolved = self.symbols_by_suffix.get(name).and_then(|ids| {
                    unique_resolved(ids)
                        .or_else(|| single_c_preferred_impl(ids, &self.symbol_files))
                });
                if let Some(id) = resolved {
                    self.suffix_resolve_cache.insert(name.to_string(), id);
                }
                resolved
            }
        } else {
            self.symbols_by_suffix
                .get(name)
                .and_then(|ids| unique_resolved(ids))
        };

        if result.is_some() {
            self.resolution_stats.fuzzy_hits += 1;
        }

        self.resolution_stats.total_time += start.elapsed();
        result
    }

    /// Resolve a symbol name to its UUID (internal use without tracking).
    ///
    /// Resolution strategy (in order):
    /// 1. Try exact match in current file: `{file}::{name}`
    /// 2. If qualified_hint provided, try unique match on qualified / suffix indexes
    /// 3. If type_hint provided, try unique suffix match for `{type}.{name}`
    /// 4. Fallback to unique fuzzy suffix match on `name`
    ///
    /// Ambiguous matches return `None`.
    #[allow(dead_code)]
    fn resolve_symbol(
        &self,
        name: &str,
        file: &str,
        qualified_hint: Option<&str>,
        type_hint: Option<&str>,
    ) -> Option<Uuid> {
        let qualified = format!("{file}::{name}");
        if let Some(id) = self.symbol_index.get(&qualified) {
            return Some(*id);
        }

        if let Some(hint) = qualified_hint
            && let Some(id) = self
                .symbols_by_qualified
                .get(hint)
                .and_then(|ids| unique_resolved(ids))
        {
            return Some(id);
        }
        if let Some(hint) = qualified_hint
            && let Some(id) = self
                .symbols_by_suffix
                .get(hint)
                .and_then(|ids| unique_resolved(ids))
        {
            return Some(id);
        }

        if let Some(type_name) = type_hint
            && let simple_name = name.split('.').next_back().unwrap_or(name)
            && let type_qualified = format!("{type_name}.{simple_name}")
            && let Some(id) = self
                .symbols_by_suffix
                .get(&type_qualified)
                .and_then(|ids| unique_resolved(ids))
        {
            return Some(id);
        }

        self.symbols_by_suffix
            .get(name)
            .and_then(|ids| unique_resolved(ids))
    }

    /// Log resolution performance statistics.
    pub fn log_resolution_stats(&self) {
        use tracing::info;

        let stats = &self.resolution_stats;
        let avg_time_micros = if stats.total_calls > 0 {
            stats.total_time.as_micros() / stats.total_calls as u128
        } else {
            0
        };

        let avg_line_lookup_micros = if stats.line_lookups > 0 {
            stats.line_lookup_time.as_micros() / stats.line_lookups as u128
        } else {
            0
        };

        let scan_calls = stats.qualified_hint_scans + stats.type_hint_scans + stats.fuzzy_scans;
        let index_hits = stats.qualified_hint_hits + stats.type_hint_hits + stats.fuzzy_hits;
        let total_index_lookups = scan_calls;
        let index_hit_rate = if total_index_lookups > 0 {
            (index_hits as f64 / total_index_lookups as f64) * 100.0
        } else {
            0.0
        };

        info!(
            total_calls = stats.total_calls,
            hashmap_hits = stats.hashmap_hits,
            qualified_hint_scans = stats.qualified_hint_scans,
            qualified_hint_hits = stats.qualified_hint_hits,
            type_hint_scans = stats.type_hint_scans,
            type_hint_hits = stats.type_hint_hits,
            fuzzy_scans = stats.fuzzy_scans,
            fuzzy_hits = stats.fuzzy_hits,
            total_scan_calls = scan_calls,
            index_hits,
            index_hit_rate_percent = format!("{:.1}", index_hit_rate),
            total_time_secs = stats.total_time.as_secs_f64(),
            avg_time_micros,
            line_lookups = stats.line_lookups,
            line_lookup_time_secs = stats.line_lookup_time.as_secs_f64(),
            avg_line_lookup_micros,
            "symbol resolution statistics"
        );
    }

    fn add_edge(&mut self, from: Uuid, to: Uuid, edge_type: EdgeType) {
        self.commit_edge(Edge::new(from, to, edge_type));
    }

    /// Borrow built nodes (testing / inspection). Empty when spilling.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Consume the builder and return all nodes and edges.
    ///
    /// Panics if the builder was created with [`Self::with_spill`] — use
    /// [`Self::finish_spill`] instead.
    pub fn into_graph(self) -> (Vec<Node>, Vec<Edge>) {
        assert!(
            self.spill.is_none(),
            "into_graph called on spilling GraphBuilder; use finish_spill"
        );
        (self.nodes, self.edges)
    }

    /// Take collected annotation argument entries (for `.rgctl/annotation_args.json`).
    pub fn take_annotation_args(&mut self) -> Vec<AnnotationArgEntry> {
        std::mem::take(&mut self.annotation_args)
    }

    /// Finish spill writers and return a [`FinishedSpill`] for columnar compile.
    pub fn finish_spill(mut self) -> Result<FinishedSpill> {
        if let Some(err) = self.spill_error.take() {
            return Err(Error::SerdeError(err));
        }
        let spill = self
            .spill
            .take()
            .ok_or_else(|| Error::SerdeError("finish_spill called without spill mode".into()))?;
        spill.finish()
    }
}

/// Kind of configuration reference detected in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigUsageKind {
    /// Environment variable reference
    EnvVar,
    /// Configuration key reference
    ConfigKey,
}

fn symbol_key(file: &str, name: &str, qualified: Option<&str>) -> String {
    format!("{file}::{}", qualified.unwrap_or(name))
}

/// Return the sole candidate UUID, or `None` when zero or multiple distinct IDs.
fn unique_resolved(ids: &[Uuid]) -> Option<Uuid> {
    match ids {
        [id] => Some(*id),
        [first, rest @ ..] if rest.iter().all(|id| id == first) => Some(*first),
        _ => None,
    }
}

/// When a C symbol appears in both `.h` and `.c`, prefer the sole `.c` definition.
fn single_c_preferred_impl(ids: &[Uuid], symbol_files: &HashMap<Uuid, String>) -> Option<Uuid> {
    let mut c_impl = None;
    for id in ids {
        if symbol_files
            .get(id)
            .is_some_and(|path| path.ends_with(".c"))
        {
            if c_impl.is_some() {
                return None;
            }
            c_impl = Some(*id);
        }
    }
    c_impl
}

fn should_sketch_symbol(symbol_type: SymbolType) -> bool {
    matches!(symbol_type, SymbolType::Function)
}

fn symbol_type_to_node_type(symbol_type: SymbolType) -> NodeType {
    match symbol_type {
        SymbolType::Function => NodeType::Function,
        SymbolType::Class => NodeType::Class,
        SymbolType::Struct => NodeType::Struct,
        SymbolType::Enum => NodeType::Enum,
        SymbolType::Interface => NodeType::Interface,
        SymbolType::Annotation => NodeType::Annotation,
        SymbolType::Module => NodeType::Module,
        SymbolType::Variable => NodeType::Variable,
        SymbolType::TypeAlias => NodeType::TypeAlias,
        SymbolType::Macro => NodeType::Macro,
        SymbolType::Import => NodeType::Import,
        SymbolType::Table => NodeType::Table,
        SymbolType::Dependency => NodeType::Dependency,
        SymbolType::Job => NodeType::Job,
        SymbolType::BuildStep => NodeType::BuildStep,
        SymbolType::AnsiblePlaybook => NodeType::AnsiblePlaybook,
        SymbolType::AnsiblePlay => NodeType::AnsiblePlay,
        SymbolType::AnsibleTask => NodeType::AnsibleTask,
        SymbolType::AnsibleRole => NodeType::AnsibleRole,
        SymbolType::AnsibleHandler => NodeType::AnsibleHandler,
        SymbolType::AnsibleVariable => NodeType::AnsibleVariable,
        SymbolType::AnsibleTemplate => NodeType::AnsibleTemplate,
        SymbolType::ChefCookbook => NodeType::ChefCookbook,
        SymbolType::ChefRecipe => NodeType::ChefRecipe,
        SymbolType::ChefResource => NodeType::ChefResource,
        SymbolType::ChefAttribute => NodeType::ChefAttribute,
        SymbolType::ChefTemplate => NodeType::ChefTemplate,
        SymbolType::ChefCustomResource => NodeType::ChefCustomResource,
        SymbolType::PuppetModule => NodeType::PuppetModule,
        SymbolType::PuppetClass => NodeType::PuppetClass,
        SymbolType::PuppetDefinedType => NodeType::PuppetDefinedType,
        SymbolType::PuppetResource => NodeType::PuppetResource,
        SymbolType::PuppetVariable => NodeType::PuppetVariable,
        SymbolType::PuppetFact => NodeType::PuppetFact,
        SymbolType::PuppetNode => NodeType::PuppetNode,
    }
}

const MAX_FILE_LOOKUP_SUFFIX_KEYS: usize = 8;

fn register_file_lookup_keys(index: &mut HashMap<String, Uuid>, norm_path: &str, id: Uuid) {
    if norm_path.is_empty() {
        return;
    }
    index.entry(norm_path.to_string()).or_insert(id);
    let slash_positions: Vec<usize> = norm_path.match_indices('/').map(|(i, _)| i).collect();
    let start = slash_positions
        .len()
        .saturating_sub(MAX_FILE_LOOKUP_SUFFIX_KEYS);
    for i in slash_positions.into_iter().skip(start) {
        if i + 1 < norm_path.len() {
            let suffix = &norm_path[i + 1..];
            index.entry(suffix.to_string()).or_insert(id);
        }
    }
}

fn normalize_file_key(path: &str) -> String {
    let mut out = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Prefix(prefix) => out = PathBuf::from(prefix.as_os_str()),
            Component::RootDir => out.push(std::path::MAIN_SEPARATOR_STR),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    normalize_path_str(out.to_string_lossy().as_ref()).into_owned()
}

fn join_path_normalized(base: &Path, rel: &str) -> String {
    let mut out = base.to_path_buf();
    for component in Path::new(rel).components() {
        match component {
            Component::Prefix(prefix) => out = PathBuf::from(prefix.as_os_str()),
            Component::RootDir => out = PathBuf::from(std::path::MAIN_SEPARATOR_STR),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    normalize_file_key(out.to_string_lossy().as_ref())
}

fn relation_allows_external_stub(relation_type: RelationType) -> bool {
    // `References` is intentionally omitted: field access must not invent Class
    // stubs named like `email` / `password` that pollute suffix resolution (#125).
    matches!(
        relation_type,
        RelationType::Instantiates
            | RelationType::DependsOn
            | RelationType::Uses
            | RelationType::AnnotatedWith
            | RelationType::Extends
            | RelationType::Implements
            | RelationType::Permits
            | RelationType::UsesVariable
            | RelationType::UsesFact
    )
}

fn stub_node_type_for_target(relation: &Relation) -> NodeType {
    if let Some(hint) = relation.to_type_hint.as_deref() {
        match hint.to_ascii_lowercase().as_str() {
            "module" => return NodeType::Module,
            "annotation" => return NodeType::Annotation,
            "interface" => return NodeType::Interface,
            "enum" => return NodeType::Enum,
            "function" | "method" => return NodeType::Function,
            "class" | "struct" => return NodeType::Class,
            "dependency" => return NodeType::Dependency,
            "puppetvariable" => return NodeType::PuppetVariable,
            "puppetfact" => return NodeType::PuppetFact,
            "puppetresource" => return NodeType::PuppetResource,
            "puppetclass" => return NodeType::PuppetClass,
            _ => {}
        }
    }
    match relation.relation_type {
        RelationType::DependsOn => NodeType::Module,
        RelationType::AnnotatedWith => NodeType::Annotation,
        RelationType::Calls => NodeType::Function,
        RelationType::Implements => NodeType::Interface,
        RelationType::Extends
        | RelationType::Instantiates
        | RelationType::Permits
        | RelationType::Uses
        | RelationType::References => NodeType::Class,
        _ => NodeType::Class,
    }
}

fn relation_type_to_edge_type(relation_type: RelationType) -> EdgeType {
    match relation_type {
        RelationType::Calls => EdgeType::Calls,
        RelationType::Uses => EdgeType::Uses,
        RelationType::Implements => EdgeType::Implements,
        RelationType::Extends => EdgeType::Extends,
        RelationType::AnnotatedWith => EdgeType::AnnotatedWith,
        RelationType::Permits => EdgeType::Permits,
        RelationType::Defines => EdgeType::Contains,
        RelationType::References => EdgeType::References,
        RelationType::Instantiates => EdgeType::Instantiates,
        RelationType::Modifies => EdgeType::Modifies,
        RelationType::DependsOn => EdgeType::DependsOn,
        RelationType::IncludesRole => EdgeType::IncludesRole,
        RelationType::DependsOnRole => EdgeType::DependsOnRole,
        RelationType::ExecutesTask => EdgeType::ExecutesTask,
        RelationType::NotifiesHandler => EdgeType::NotifiesHandler,
        RelationType::IncludesPlaybook => EdgeType::IncludesPlaybook,
        RelationType::UsesVariable => EdgeType::Uses,
        RelationType::RendersTemplate => EdgeType::RendersTemplate,
        RelationType::DependsOnCookbook => EdgeType::DependsOnCookbook,
        RelationType::IncludesRecipe => EdgeType::IncludesRecipe,
        RelationType::DeclaresResource => EdgeType::DeclaresResource,
        RelationType::UsesTemplate => EdgeType::UsesTemplate,
        RelationType::DefinesAttribute => EdgeType::DefinesAttribute,
        RelationType::NotifiesResource => EdgeType::NotifiesResource,
        RelationType::DependsOnModule => EdgeType::DependsOnModule,
        RelationType::IncludesClass => EdgeType::IncludesClass,
        RelationType::InheritsClass => EdgeType::InheritsClass,
        RelationType::RequiresResource => EdgeType::RequiresResource,
        RelationType::UsesFact => EdgeType::UsesFact,
    }
}

fn go_simple_type_from_str(ty: &str) -> String {
    ty.trim_start_matches('*')
        .rsplit(['.', '/'])
        .next()
        .unwrap_or(ty)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_plugin_api::{ConfigValueType, SourceLocation, SymbolType};

    fn sample_symbol() -> Symbol {
        Symbol {
            name: "main".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: None,
            location: SourceLocation {
                file: "src/main.rs".to_string(),
                start_line: 1,
                end_line: 3,
                start_column: 0,
                end_column: 1,
            },
            signature: Some("fn main()".to_string()),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn java_annotated_with_resolves_package_qualified_annotation() {
        let mut builder = GraphBuilder::new();
        let marker_file = builder.ensure_file_node(Path::new("Marker.java"));
        let foo_file = builder.ensure_file_node(Path::new("Foo.java"));

        let annotation = Symbol {
            name: "Marker".to_string(),
            symbol_type: SymbolType::Annotation,
            qualified_name: Some("demo.Marker".to_string()),
            location: SourceLocation {
                file: "Marker.java".to_string(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec!["public".into()],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        let method = Symbol {
            name: "bar".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("demo.Foo.bar".to_string()),
            location: SourceLocation {
                file: "Foo.java".to_string(),
                start_line: 3,
                end_line: 4,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec!["public".into()],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        builder.add_symbol(&annotation, marker_file);
        builder.add_symbol(&method, foo_file);
        builder
            .add_relation(&Relation {
                from: "demo.Foo.bar".to_string(),
                to: "Marker".to_string(),
                relation_type: RelationType::AnnotatedWith,
                location: SourceLocation {
                    file: "Foo.java".to_string(),
                    start_line: 3,
                    end_line: 3,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: None,
                to_type_hint: None,
            })
            .unwrap();

        assert!(
            builder
                .edges
                .iter()
                .any(|e| e.edge_type == EdgeType::AnnotatedWith),
            "AnnotatedWith edge must resolve Marker via dotted QN suffix index"
        );
    }

    #[test]
    fn instantiates_unresolved_type_creates_stub_and_edge() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("C.java"));
        let method = Symbol {
            name: "m".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("C.m".to_string()),
            location: SourceLocation {
                file: "C.java".to_string(),
                start_line: 2,
                end_line: 3,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        builder.add_symbol(&method, file_id);
        builder
            .add_relation(&Relation {
                from: "C.m".to_string(),
                to: "String".to_string(),
                relation_type: RelationType::Instantiates,
                location: SourceLocation {
                    file: "C.java".to_string(),
                    start_line: 2,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: Some("java.lang.String".to_string()),
                to_type_hint: None,
            })
            .unwrap();

        let stub = builder
            .nodes()
            .iter()
            .find(|n| {
                n.name == "String"
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            })
            .expect("String stub");
        assert_eq!(stub.node_type, NodeType::Class);
        assert!(
            builder
                .edges
                .iter()
                .any(|e| { e.edge_type == EdgeType::Instantiates && e.to == stub.id })
        );
    }

    #[test]
    fn extends_unresolved_superclass_creates_stub_and_edge() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("App.java"));
        let class = Symbol {
            name: "App".to_string(),
            symbol_type: SymbolType::Class,
            qualified_name: Some("com.example.App".to_string()),
            location: SourceLocation {
                file: "App.java".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        builder.add_symbol(&class, file_id);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "com.example.App".to_string(),
                to: "RuntimeException".to_string(),
                relation_type: RelationType::Extends,
                location: SourceLocation {
                    file: "App.java".to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: None,
                to_type_hint: None,
            })
            .unwrap();

        let stub = builder
            .nodes()
            .iter()
            .find(|n| {
                n.name == "RuntimeException"
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            })
            .expect("RuntimeException stub");
        assert_eq!(stub.node_type, NodeType::Class);
        assert!(
            builder
                .edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Extends && e.to == stub.id)
        );
    }

    #[test]
    fn implements_unresolved_interface_creates_stub_and_edge() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("Svc.java"));
        let class = Symbol {
            name: "Svc".to_string(),
            symbol_type: SymbolType::Class,
            qualified_name: Some("com.example.Svc".to_string()),
            location: SourceLocation {
                file: "Svc.java".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        builder.add_symbol(&class, file_id);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "com.example.Svc".to_string(),
                to: "UserDetailsService".to_string(),
                relation_type: RelationType::Implements,
                location: SourceLocation {
                    file: "Svc.java".to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: Some(
                    "org.springframework.security.core.userdetails.UserDetailsService".to_string(),
                ),
                to_type_hint: None,
            })
            .unwrap();

        let stub = builder
            .nodes()
            .iter()
            .find(|n| {
                n.name == "UserDetailsService"
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            })
            .expect("UserDetailsService stub");
        assert_eq!(stub.node_type, NodeType::Interface);
        assert!(
            builder
                .edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Implements && e.to == stub.id)
        );
    }

    #[test]
    fn permits_unresolved_type_creates_stub_and_edge() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("Shape.java"));
        let class = Symbol {
            name: "Shape".to_string(),
            symbol_type: SymbolType::Class,
            qualified_name: Some("com.example.Shape".to_string()),
            location: SourceLocation {
                file: "Shape.java".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        builder.add_symbol(&class, file_id);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "com.example.Shape".to_string(),
                to: "Circle".to_string(),
                relation_type: RelationType::Permits,
                location: SourceLocation {
                    file: "Shape.java".to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: None,
                to_type_hint: None,
            })
            .unwrap();

        let stub = builder
            .nodes()
            .iter()
            .find(|n| {
                n.name == "Circle"
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            })
            .expect("Circle stub");
        assert!(
            builder
                .edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Permits && e.to == stub.id)
        );
    }

    #[test]
    fn depends_on_unresolved_module_creates_module_stub() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("module-info.java"));
        let module = Symbol {
            name: "M".to_string(),
            symbol_type: SymbolType::Module,
            qualified_name: Some("M".to_string()),
            location: SourceLocation {
                file: "module-info.java".to_string(),
                start_line: 1,
                end_line: 4,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        builder.add_symbol(&module, file_id);
        builder
            .add_relation(&Relation {
                from: "M".to_string(),
                to: "java.base".to_string(),
                relation_type: RelationType::DependsOn,
                location: SourceLocation {
                    file: "module-info.java".to_string(),
                    start_line: 2,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: Some("java.base".to_string()),
                to_type_hint: None,
            })
            .unwrap();

        let stub = builder
            .nodes()
            .iter()
            .find(|n| {
                n.qualified_name.as_deref() == Some("java.base")
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            })
            .expect("java.base stub");
        assert_eq!(stub.node_type, NodeType::Module);
        assert!(
            builder
                .edges
                .iter()
                .any(|e| { e.edge_type == EdgeType::DependsOn && e.to == stub.id })
        );
    }

    #[test]
    fn annotated_with_unresolved_annotation_creates_stub() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("C.java"));
        let method = Symbol {
            name: "m".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("C.m".to_string()),
            location: SourceLocation {
                file: "C.java".to_string(),
                start_line: 2,
                end_line: 3,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        builder.add_symbol(&method, file_id);
        builder
            .add_relation(&Relation {
                from: "C.m".to_string(),
                to: "NonNull".to_string(),
                relation_type: RelationType::AnnotatedWith,
                location: SourceLocation {
                    file: "C.java".to_string(),
                    start_line: 2,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: None,
                to_type_hint: None,
            })
            .unwrap();

        let stub = builder
            .nodes()
            .iter()
            .find(|n| n.name == "NonNull" && n.node_type == NodeType::Annotation)
            .expect("NonNull stub");
        assert_eq!(
            stub.properties.get("is_external_stub").map(String::as_str),
            Some("true")
        );
        assert!(
            builder
                .edges
                .iter()
                .any(|e| { e.edge_type == EdgeType::AnnotatedWith && e.to == stub.id })
        );
    }

    #[test]
    fn c_header_and_source_disambiguate_to_dot_c() {
        let mut builder = GraphBuilder::new();
        let header = builder.ensure_file_node(Path::new("cart_service.h"));
        let source = builder.ensure_file_node(Path::new("cart_service.c"));
        let proto = Symbol {
            name: "cart_add_item".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("cart_service::cart_add_item".to_string()),
            location: SourceLocation {
                file: "cart_service.h".to_string(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "c" }),
        };
        let defn = Symbol {
            location: SourceLocation {
                file: "cart_service.c".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 1,
            },
            ..proto.clone()
        };
        builder.add_symbol(&proto, header);
        builder.add_symbol(&defn, source);
        builder.build_resolution_indexes();

        let rel = Relation {
            from: "cart_service::cart_add_item".to_string(),
            to: "cart_add_item".to_string(),
            relation_type: RelationType::Calls,
            location: SourceLocation {
                file: "cart_service.c".to_string(),
                start_line: 2,
                end_line: 2,
                start_column: 0,
                end_column: 1,
            },
            metadata: serde_json::json!({ "language": "c" }),
            to_qualified_hint: None,
            to_type_hint: None,
        };
        let before = builder.edge_count();
        builder.add_relation(&rel).unwrap();
        assert_eq!(builder.edge_count(), before + 1);
    }

    #[test]
    fn cpp_cross_namespace_call_resolves_with_qualified_hint() {
        let mut builder = GraphBuilder::new();
        let checkout_file = builder.ensure_file_node(Path::new("order_service.cpp"));
        let repo_file = builder.ensure_file_node(Path::new("order_repository.cpp"));
        let checkout = Symbol {
            name: "checkout".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("ecommerce::services::checkout".to_string()),
            location: SourceLocation {
                file: "order_service.cpp".to_string(),
                start_line: 2,
                end_line: 6,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "cpp" }),
        };
        let order_create = Symbol {
            name: "order_create".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("ecommerce::repositories::order_create".to_string()),
            location: SourceLocation {
                file: "order_repository.cpp".to_string(),
                start_line: 2,
                end_line: 4,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "cpp" }),
        };
        builder.add_symbol(&checkout, checkout_file);
        builder.add_symbol(&order_create, repo_file);
        builder.build_resolution_indexes();

        let rel = Relation {
            from: "ecommerce::services::checkout".to_string(),
            to: "repositories::order_create".to_string(),
            relation_type: RelationType::Calls,
            location: SourceLocation {
                file: "order_service.cpp".to_string(),
                start_line: 4,
                end_line: 4,
                start_column: 0,
                end_column: 1,
            },
            metadata: serde_json::json!({ "language": "cpp" }),
            to_qualified_hint: Some("ecommerce::repositories::order_create".to_string()),
            to_type_hint: None,
        };
        let before = builder.edge_count();
        builder.add_relation(&rel).unwrap();
        assert_eq!(
            builder.edge_count(),
            before + 1,
            "expected CALLS edge for cross-namespace C++ hint"
        );
    }

    fn auth_register_symbol() -> Symbol {
        Symbol {
            name: "register".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("AuthService.register".to_string()),
            location: SourceLocation {
                file: "AuthService.java".to_string(),
                start_line: 1,
                end_line: 10,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        }
    }

    fn auth_loc(line: usize) -> SourceLocation {
        SourceLocation {
            file: "AuthService.java".to_string(),
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: 1,
        }
    }

    /// Issue #125: field References must not invent Class stubs that bare Calls can bind.
    #[test]
    fn references_to_field_does_not_create_class_stub() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("AuthService.java"));
        builder.add_symbol(&auth_register_symbol(), file_id);
        builder.build_resolution_indexes();
        let before_nodes = builder.node_count();
        builder
            .add_relation(&Relation {
                from: "AuthService.register".to_string(),
                to: "email".to_string(),
                relation_type: RelationType::References,
                location: auth_loc(2),
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: Some("User.email".to_string()),
                to_type_hint: None,
            })
            .unwrap();
        assert_eq!(
            builder.node_count(),
            before_nodes,
            "unresolved field References must not invent nodes"
        );
        assert!(
            !builder.nodes().iter().any(|n| {
                n.name == "email"
                    && n.node_type == NodeType::Class
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            }),
            "no Class stub named email"
        );
    }

    /// Issue #125: bare Calls never bind Class stubs, regardless of relation order.
    #[test]
    fn calls_never_bind_class_stub_regardless_of_order() {
        let references = Relation {
            from: "AuthService.register".to_string(),
            to: "email".to_string(),
            relation_type: RelationType::References,
            location: auth_loc(2),
            metadata: serde_json::json!({ "language": "java" }),
            to_qualified_hint: Some("User.email".to_string()),
            to_type_hint: None,
        };
        let calls = Relation {
            from: "AuthService.register".to_string(),
            to: "email".to_string(),
            relation_type: RelationType::Calls,
            location: auth_loc(3),
            metadata: serde_json::json!({ "language": "java" }),
            to_qualified_hint: None,
            to_type_hint: None,
        };

        for order_refs_first in [true, false] {
            let mut builder = GraphBuilder::new();
            let file_id = builder.ensure_file_node(Path::new("AuthService.java"));
            builder.add_symbol(&auth_register_symbol(), file_id);
            // Plant a Class stub via Instantiates (still allowlisted) to prove Calls rejects it.
            builder
                .add_relation(&Relation {
                    from: "AuthService.register".to_string(),
                    to: "email".to_string(),
                    relation_type: RelationType::Instantiates,
                    location: auth_loc(1),
                    metadata: serde_json::json!({ "language": "java" }),
                    to_qualified_hint: Some("User.email".to_string()),
                    to_type_hint: None,
                })
                .unwrap();
            let stub_id = builder
                .nodes()
                .iter()
                .find(|n| {
                    n.name == "email"
                        && n.node_type == NodeType::Class
                        && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
                })
                .map(|n| n.id)
                .expect("Class stub via Instantiates");
            builder.build_resolution_indexes();
            if order_refs_first {
                builder.add_relation(&references).unwrap();
                builder.add_relation(&calls).unwrap();
            } else {
                builder.add_relation(&calls).unwrap();
                builder.add_relation(&references).unwrap();
            }
            assert!(
                !builder
                    .edges
                    .iter()
                    .any(|e| e.edge_type == EdgeType::Calls && e.to == stub_id),
                "Calls must not target Class stub (refs_first={order_refs_first})"
            );
        }
    }

    #[test]
    fn calls_to_existing_function_suffix_still_commits() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("AuthService.java"));
        builder.add_symbol(&auth_register_symbol(), file_id);
        let callee = Symbol {
            name: "encode".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("PasswordEncoder.encode".to_string()),
            location: SourceLocation {
                file: "PasswordEncoder.java".to_string(),
                start_line: 1,
                end_line: 3,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        let enc_file = builder.ensure_file_node(Path::new("PasswordEncoder.java"));
        builder.add_symbol(&callee, enc_file);
        builder.build_resolution_indexes();
        let before = builder.edge_count();
        builder
            .add_relation(&Relation {
                from: "AuthService.register".to_string(),
                to: "encode".to_string(),
                relation_type: RelationType::Calls,
                location: auth_loc(5),
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: None,
                to_type_hint: None,
            })
            .unwrap();
        assert_eq!(builder.edge_count(), before + 1);
        assert!(builder.edges.iter().any(|e| e.edge_type == EdgeType::Calls));
    }

    #[test]
    fn same_file_lambdas_with_different_qn_keep_distinct_ids() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("AuthService.java"));
        let login_lambda = Symbol {
            name: "$lambda$0".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("AuthService.login.$lambda$0".to_string()),
            location: SourceLocation {
                file: "AuthService.java".to_string(),
                start_line: 63,
                end_line: 63,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java", "is_lambda": true }),
        };
        let current_lambda = Symbol {
            name: "$lambda$0".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("AuthService.currentUser.$lambda$0".to_string()),
            location: SourceLocation {
                file: "AuthService.java".to_string(),
                start_line: 77,
                end_line: 77,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java", "is_lambda": true }),
        };
        let id_a = builder.add_symbol(&login_lambda, file_id);
        let id_b = builder.add_symbol(&current_lambda, file_id);
        assert_ne!(id_a, id_b);
        let qns: Vec<_> = builder
            .nodes()
            .iter()
            .filter(|n| n.name == "$lambda$0")
            .filter_map(|n| n.qualified_name.as_deref())
            .collect();
        assert!(qns.contains(&"AuthService.login.$lambda$0"));
        assert!(qns.contains(&"AuthService.currentUser.$lambda$0"));
    }

    #[test]
    fn external_stub_dedupes_by_qualified_name() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("C.java"));
        let method = Symbol {
            name: "m".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("C.m".to_string()),
            location: SourceLocation {
                file: "C.java".to_string(),
                start_line: 2,
                end_line: 5,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        builder.add_symbol(&method, file_id);
        for _ in 0..2 {
            builder
                .add_relation(&Relation {
                    from: "C.m".to_string(),
                    to: "String".to_string(),
                    relation_type: RelationType::Instantiates,
                    location: SourceLocation {
                        file: "C.java".to_string(),
                        start_line: 3,
                        end_line: 3,
                        start_column: 0,
                        end_column: 1,
                    },
                    metadata: serde_json::json!({ "language": "java" }),
                    to_qualified_hint: Some("java.lang.String".to_string()),
                    to_type_hint: None,
                })
                .unwrap();
        }
        let stubs: Vec<_> = builder
            .nodes()
            .iter()
            .filter(|n| {
                n.qualified_name.as_deref() == Some("java.lang.String")
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            })
            .collect();
        assert_eq!(stubs.len(), 1, "expected one stub for java.lang.String");
    }

    #[test]
    fn references_file_target_resolves_to_file_node() {
        let mut builder = GraphBuilder::new();
        let guide_id = builder.ensure_file_node(Path::new("docs/guide.md"));
        let adr_id = builder.ensure_file_node(Path::new("docs/adr.md"));
        let heading = Symbol {
            name: "Checkout Flow".to_string(),
            symbol_type: SymbolType::Module,
            qualified_name: Some("docs/guide.md#checkout-flow".to_string()),
            location: SourceLocation {
                file: "docs/guide.md".to_string(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "kind": "heading" }),
        };
        builder.add_symbol(&heading, guide_id);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "docs/guide.md#checkout-flow".to_string(),
                to: "docs/adr.md".to_string(),
                relation_type: RelationType::References,
                location: SourceLocation {
                    file: "docs/guide.md".to_string(),
                    start_line: 2,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "kind": "markdown_link" }),
                to_qualified_hint: Some("docs/adr.md".to_string()),
                to_type_hint: Some("file".to_string()),
            })
            .unwrap();

        let heading_id = builder
            .nodes()
            .iter()
            .find(|n| n.qualified_name.as_deref() == Some("docs/guide.md#checkout-flow"))
            .expect("heading node")
            .id;
        assert!(builder.edges.iter().any(|e| {
            e.edge_type == EdgeType::References && e.from == heading_id && e.to == adr_id
        }));
        assert!(
            !builder.nodes().iter().any(|n| {
                n.node_type == NodeType::Class
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
                    && n.name.contains("adr")
            }),
            "file target must not create Class stub"
        );
    }

    #[test]
    fn references_from_document_path_resolves_to_file_node() {
        let mut builder = GraphBuilder::new();
        let guide_id = builder.ensure_file_node(Path::new("docs/guide.md"));
        let adr_id = builder.ensure_file_node(Path::new("docs/adr.md"));
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "docs/guide.md".to_string(),
                to: "docs/adr.md".to_string(),
                relation_type: RelationType::References,
                location: SourceLocation {
                    file: "docs/guide.md".to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "kind": "markdown_link" }),
                to_qualified_hint: Some("docs/adr.md".to_string()),
                to_type_hint: Some("file".to_string()),
            })
            .unwrap();

        assert!(builder.edges.iter().any(|e| {
            e.edge_type == EdgeType::References && e.from == guide_id && e.to == adr_id
        }));
    }

    #[test]
    fn references_missing_file_target_skips_edge_and_stub() {
        let mut builder = GraphBuilder::new();
        let guide_id = builder.ensure_file_node(Path::new("docs/guide.md"));
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "docs/guide.md".to_string(),
                to: "docs/missing.md".to_string(),
                relation_type: RelationType::References,
                location: SourceLocation {
                    file: "docs/guide.md".to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "kind": "markdown_link" }),
                to_qualified_hint: Some("docs/missing.md".to_string()),
                to_type_hint: Some("file".to_string()),
            })
            .unwrap();

        assert!(
            !builder
                .edges
                .iter()
                .any(|e| e.edge_type == EdgeType::References),
            "missing file href must not create an edge"
        );
        assert_eq!(builder.node_count(), 1, "only guide file node");
        assert!(
            !builder.nodes().iter().any(|n| n
                .properties
                .get("is_external_stub")
                .map(String::as_str)
                == Some("true")),
            "missing file href must not stub"
        );
        let _ = guide_id;
    }

    #[test]
    fn references_file_target_resolves_with_absolute_discover_paths() {
        let mut builder = GraphBuilder::new();
        let guide_abs = "/tmp/fixture/./docs/guide.md";
        let java_abs = "/tmp/fixture/src/CheckoutService.java";
        let guide_id = builder.ensure_file_node(Path::new(guide_abs));
        let java_id = builder.ensure_file_node(Path::new(java_abs));
        let checkout = Symbol {
            name: "Checkout Flow".to_string(),
            symbol_type: SymbolType::Module,
            qualified_name: Some(format!("{guide_abs}#checkout-flow")),
            location: SourceLocation {
                file: guide_abs.to_string(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "kind": "heading", "level": 1 }),
        };
        let checkout_id = builder.add_symbol(&checkout, guide_id);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: format!("{guide_abs}#checkout-flow"),
                to: "src/CheckoutService.java".to_string(),
                relation_type: RelationType::References,
                location: SourceLocation {
                    file: guide_abs.to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "kind": "markdown_link" }),
                to_qualified_hint: Some("src/CheckoutService.java".to_string()),
                to_type_hint: Some("file".to_string()),
            })
            .unwrap();

        assert!(
            builder.edges.iter().any(|e| {
                e.edge_type == EdgeType::References && e.from == checkout_id && e.to == java_id
            }),
            "relative file href must resolve against absolute discover paths"
        );
    }

    #[test]
    fn references_unresolved_type_name_does_not_stub_class() {
        // #125: References is not stub-allowlisted (field names must not become Class stubs).
        // Type stubs remain available via Instantiates / Uses / etc.
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("Demo.java"));
        let method = Symbol {
            name: "run".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("Demo.run".to_string()),
            location: SourceLocation {
                file: "Demo.java".to_string(),
                start_line: 2,
                end_line: 4,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        };
        builder.add_symbol(&method, file_id);
        builder.build_resolution_indexes();
        let before = builder.node_count();
        builder
            .add_relation(&Relation {
                from: "Demo.run".to_string(),
                to: "ArrayList".to_string(),
                relation_type: RelationType::References,
                location: SourceLocation {
                    file: "Demo.java".to_string(),
                    start_line: 3,
                    end_line: 3,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: None,
                to_type_hint: None,
            })
            .unwrap();
        assert_eq!(builder.node_count(), before);
        assert!(
            !builder.nodes().iter().any(|n| {
                n.name == "ArrayList"
                    && n.node_type == NodeType::Class
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            }),
            "References must not invent Class stubs"
        );
    }

    #[test]
    fn add_symbol_with_body_sets_token_bloom() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("src/main.rs"));
        let symbol = sample_symbol();
        builder.add_symbol_with_body(&symbol, file_id, Some("let port = ntohs(raw);"));
        let node = builder
            .nodes()
            .iter()
            .find(|n| n.name == symbol.name)
            .unwrap();
        let bloom = node.token_bloom.expect("token bloom");
        assert!(rgctl_graph::keyword_in_bloom("ntohs", &bloom));
    }

    #[test]
    fn test_add_symbol_creates_file_and_symbol_nodes() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("src/main.rs"));
        builder.add_symbol(&sample_symbol(), file_id);

        assert_eq!(builder.node_count(), 2);
        assert_eq!(builder.edge_count(), 2);
    }

    #[test]
    fn test_add_symbol_materializes_fields() {
        let mut builder = GraphBuilder::new();
        builder.set_materialize_fields(true);
        let file_id = builder.ensure_file_node(Path::new("OrderDTO.java"));
        let mut symbol = sample_symbol();
        symbol.name = "OrderDTO".to_string();
        symbol.symbol_type = SymbolType::Class;
        symbol.location.file = "OrderDTO.java".to_string();
        symbol.fields = vec![rgctl_plugin_api::Field {
            name: "status".to_string(),
            field_type: Some("String".to_string()),
            visibility: Some("private".to_string()),
        }];
        builder.add_symbol(&symbol, file_id);

        let field = builder
            .nodes()
            .iter()
            .find(|n| n.name == "status" && n.node_type == NodeType::Variable)
            .expect("field variable node");
        assert_eq!(
            field.properties.get("member_of").map(String::as_str),
            Some("OrderDTO")
        );
        assert_eq!(
            field.properties.get("field_type").map(String::as_str),
            Some("String")
        );
        let owner = builder
            .nodes()
            .iter()
            .find(|n| n.name == "OrderDTO")
            .unwrap();
        assert!(builder.edges.iter().any(|e| {
            e.from == owner.id && e.to == field.id && e.edge_type == EdgeType::Contains
        }));
    }

    #[test]
    fn test_add_config_key() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("config.yaml"));
        let key = ConfigKey {
            key_path: "database.host".to_string(),
            value: "localhost".to_string(),
            value_type: ConfigValueType::String,
            location: SourceLocation {
                file: "config.yaml".to_string(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 0,
            },
        };

        builder.add_config_key(&key, file_id);
        assert_eq!(builder.node_count(), 2);
    }

    #[test]
    fn test_link_config_usage_env_var() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("src/main.rs"));
        builder.add_symbol(&sample_symbol(), file_id);

        builder.link_config_usage("src/main.rs", 1, "DB_HOST", ConfigUsageKind::EnvVar);

        assert!(builder.node_count() >= 3);
        assert!(
            builder
                .edges
                .iter()
                .any(|e| e.edge_type == EdgeType::UsesConfig)
        );
    }

    fn function_symbol(file: &str, name: &str, qualified: &str) -> Symbol {
        Symbol {
            name: name.to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some(qualified.to_string()),
            location: SourceLocation {
                file: file.to_string(),
                start_line: 1,
                end_line: 3,
                start_column: 0,
                end_column: 1,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
        }
    }

    /// Ambiguous FQN must not resolve as a single definitive UUID (#27).
    #[test]
    fn qe_duplicate_qualified_name_must_not_collapse_index() {
        let mut builder = GraphBuilder::new();
        let a = builder.ensure_file_node(Path::new("a.rs"));
        let b = builder.ensure_file_node(Path::new("b.rs"));
        builder.add_symbol(&function_symbol("a.rs", "transform", "Helper.transform"), a);
        builder.add_symbol(&function_symbol("b.rs", "transform", "Helper.transform"), b);
        builder.build_resolution_indexes();

        let nodes_with_fqn = builder
            .nodes()
            .iter()
            .filter(|n| n.qualified_name.as_deref() == Some("Helper.transform"))
            .count();
        assert_eq!(nodes_with_fqn, 2, "both nodes must survive ingest");

        let suffix_n = builder
            .symbols_by_suffix
            .get("Helper.transform")
            .map(|v| v.len())
            .unwrap_or(0);
        assert!(
            suffix_n >= 2,
            "suffix index should retain both UUIDs (got {suffix_n})"
        );

        let qn_n = builder
            .symbols_by_qualified
            .get("Helper.transform")
            .map(|v| v.len())
            .unwrap_or(0);
        assert!(
            qn_n >= 2,
            "qualified index should retain both UUIDs (got {qn_n})"
        );

        let resolved = builder.resolve_symbol_tracked(
            "transform",
            "caller.rs",
            Some("Helper.transform"),
            None,
        );
        assert!(
            resolved.is_none(),
            "QE: duplicate FQN must not resolve to a single UUID via qualified hint \
             (got {resolved:?}); see rgctl-tests/correctness/QE.md"
        );
    }

    /// Suffix multi-match must not silently pick `.first()` (#27).
    #[test]
    fn qe_suffix_multimatch_must_not_pick_first_silently() {
        let mut builder = GraphBuilder::new();
        let a = builder.ensure_file_node(Path::new("pkg/a.rs"));
        let b = builder.ensure_file_node(Path::new("pkg/b.rs"));
        builder.add_symbol(&function_symbol("pkg/a.rs", "twin", "alpha::twin"), a);
        builder.add_symbol(&function_symbol("pkg/b.rs", "twin", "beta::twin"), b);
        builder.build_resolution_indexes();

        let candidates = builder
            .symbols_by_suffix
            .get("twin")
            .map(|v| v.len())
            .unwrap_or(0);
        assert!(
            candidates >= 2,
            "expected ≥2 suffix candidates for twin, got {candidates}"
        );

        let resolved = builder.resolve_symbol_tracked("twin", "other.rs", None, None);
        assert!(
            resolved.is_none(),
            "QE: fuzzy suffix multi-match must not return Some(uuid) without signaling ambiguity \
             (got {resolved:?}); see rgctl-tests/correctness/QE.md"
        );
    }

    #[test]
    fn test_c_struct_fields_not_materialized_by_default() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("cart.c"));
        let symbol = Symbol {
            name: "Cart".to_string(),
            symbol_type: SymbolType::Class,
            location: SourceLocation {
                file: "cart.c".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 0,
            },
            qualified_name: None,
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![rgctl_plugin_api::Field {
                name: "total".to_string(),
                field_type: Some("int".to_string()),
                visibility: None,
            }],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
        };
        builder.add_symbol(&symbol, file_id);
        assert!(
            !builder
                .nodes()
                .iter()
                .any(|n| n.name == "total" && n.node_type == NodeType::Variable),
            "default discover must not emit field Variable nodes"
        );
    }

    #[test]
    fn test_field_suffix_not_indexed_for_c_struct_field() {
        let mut builder = GraphBuilder::new();
        builder.set_materialize_fields(true);
        let file_id = builder.ensure_file_node(Path::new("cart.c"));
        let symbol = Symbol {
            name: "Cart".to_string(),
            symbol_type: SymbolType::Class,
            location: SourceLocation {
                file: "cart.c".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 0,
            },
            qualified_name: None,
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![rgctl_plugin_api::Field {
                name: "total".to_string(),
                field_type: Some("int".to_string()),
                visibility: None,
            }],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
        };
        builder.add_symbol(&symbol, file_id);
        builder.build_resolution_indexes();
        let bare = builder.symbols_by_suffix.get("total");
        assert!(
            bare.is_none() || bare.is_some_and(|v| v.is_empty()),
            "bare field suffix must not be indexed for C struct fields"
        );
    }

    #[test]
    fn test_file_lookup_suffix_keys_are_bounded() {
        let mut index = HashMap::new();
        let id = Uuid::new_v4();
        let deep = "a/b/c/d/e/f/g/h/i/j/k/l/m.rs";
        register_file_lookup_keys(&mut index, deep, id);

        assert_eq!(index.get(deep), Some(&id));
        assert_eq!(
            index.get("m.rs"),
            Some(&id),
            "shortest suffix should still resolve"
        );
        assert_eq!(
            index.get("l/m.rs"),
            Some(&id),
            "near-tail multi-segment suffix should resolve"
        );
        assert!(
            !index.contains_key("b/c/d/e/f/g/h/i/j/k/l/m.rs"),
            "far-prefix suffix should be skipped when depth exceeds bound"
        );
    }
}
