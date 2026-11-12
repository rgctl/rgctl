//! Maps extracted symbols and relations into graph nodes and edges.

use crate::extractor::SymbolPass1Prep;
use crate::parked_relations::ParkedRelationRecord;
use rgctl_error::{Error, Result};
use rgctl_graph::code_index::{CodeIndex, hash_code};
use rgctl_graph::content_store::{ContentStore, INLINE_BODY_MAX_BYTES, hash_bytes};
use rgctl_graph::migration::graph_parameter_from_plugin;
use rgctl_graph::normalize_path_str;
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::segmented_spill::{FinishedSpill, SegmentedSpill};
use rgctl_graph::structural_sketch::build_token_bloom;
use rgctl_plugin_api::{ComplexityMetrics, ConfigKey, Relation, RelationType, Symbol, SymbolType};
use rustc_hash::{FxHashMap, FxHashSet};
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
    symbol_index: HashMap<(String, NodeType), Uuid>,
    file_nodes: HashMap<String, Uuid>,
    /// Normalized path + component-boundary suffixes → file node id (O(1) lookup).
    file_path_lookup: HashMap<String, Uuid>,
    config_key_nodes: HashMap<String, Uuid>,
    env_nodes: HashMap<String, Uuid>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// Lazily populated `Uuid -> self.nodes index` map when `append_target_vocab` runs.
    node_index_by_id: HashMap<Uuid, usize>,
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
    /// Child type simple name → direct bases `(base_simple_name, is_interface)`.
    /// Populated from Extends / Implements / IncludesClass / InheritsClass in pass 2.
    inheritance_bases: HashMap<String, Vec<(String, bool)>>,
    /// Owner simple name → `(method_simple_name, function_node_id)` declared on that type.
    type_methods: HashMap<String, Vec<(String, Uuid)>>,
    /// Cached MRO chains (simple type names, self first).
    mro_cache: HashMap<String, Vec<String>>,
    /// Unresolved / external-stub relations parked for cross-repo merge.
    parked_relations: Vec<ParkedRelationRecord>,
    declaration_ids: FxHashSet<Uuid>,
    callable_parameters: HashMap<Uuid, String>,
    /// Shared dispatch contracts, available even when graph nodes spill to disk.
    method_identities: HashMap<Uuid, String>,
    /// Type members cannot be selected as free functions by lexical fallback.
    member_method_ids: FxHashSet<Uuid>,
    method_override_owners: HashMap<Uuid, String>,
    signature_families: FxHashSet<Uuid>,
    unresolved_calls: usize,
    callable_resolution_samples: Vec<Relation>,
    /// `file_path → (imported_symbol_or_alias → module_source_path)` from Import symbols.
    file_imports_index: FxHashMap<String, FxHashMap<String, String>>,
    /// External stub node UUIDs (`O(1)` membership test in `park_relation_if_needed`).
    external_stub_ids: FxHashSet<Uuid>,
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
            // Namespace-qualified callables also need bare suffixes without
            // the discriminator appended to symbol_index's logical key.
            // Without a discriminator, the key suffix loop below already adds them.
            if !is_field_member && node.identity_discriminator().is_some() {
                for (offset, _) in qualified.match_indices("::") {
                    let suffix = &qualified[offset + 2..];
                    if !suffix.is_empty() {
                        self.symbols_by_suffix
                            .entry(suffix.to_owned())
                            .or_default()
                            .push(node.id);
                    }
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
            if !self.node_index_by_id.is_empty() {
                self.node_index_by_id.insert(node.id, self.nodes.len());
            }
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
        let mut key = (
            symbol_key(
                &symbol.location.file,
                &symbol.name,
                symbol.qualified_name.as_deref(),
            ),
            symbol_type_to_node_type(symbol.symbol_type),
        );
        if let Some(discriminator) = symbol
            .metadata
            .get("identity_discriminator")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
        {
            key.0.push('\u{1f}');
            key.0.push_str(discriminator);
        }
        // C `#include` paths are per-translation-unit; dedupe globally by path string
        // so kernel-scale repos do not materialize one Import node per occurrence.
        if symbol.symbol_type == SymbolType::Import
            && symbol.metadata.get("kind").and_then(|v| v.as_str()) == Some("include")
        {
            key.0 = format!("c_include::{}", symbol.name);
            if let Some(&id) = self.symbol_index.get(&key) {
                self.add_edge(file_id, id, EdgeType::Uses);
                return id;
            }
        }
        if let Some(id) = self.symbol_index.get(&key) {
            return *id;
        }

        let start_byte = prep.and_then(|p| p.start_byte).or(symbol.start_byte);
        let end_byte = prep.and_then(|p| p.end_byte).or(symbol.end_byte);
        let name_rel_offset = prep
            .and_then(|p| p.name_rel_offset)
            .or(symbol.name_rel_offset);
        let name_token_len = prep
            .and_then(|p| p.name_token_len)
            .or(symbol.name_token_len);
        let name_span_overflow = prep
            .and_then(|p| p.name_span_overflow)
            .or(symbol.name_span_overflow);

        let mut node = Node::new(
            symbol_type_to_node_type(symbol.symbol_type),
            symbol.name.clone(),
        )
        .with_file_path(symbol.location.file.clone())
        .with_location(symbol.location.start_line, symbol.location.end_line)
        .with_byte_spans(
            start_byte,
            end_byte,
            name_rel_offset,
            name_token_len,
            name_span_overflow,
        );

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

        let code_hash = prep.and_then(|p| p.code_hash.clone()).or_else(|| {
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

        let semantic_tags = symbol
            .metadata
            .get("semantic_tags")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        if let Some(bloom) = prep.and_then(|p| p.token_bloom) {
            node = node.with_token_bloom(bloom);
        } else if should_sketch_symbol(symbol.symbol_type, semantic_tags.is_some()) {
            let bloom = build_token_bloom(
                &symbol.name,
                symbol.qualified_name.as_deref(),
                symbol.signature.as_deref(),
                body,
                semantic_tags,
            );
            node = node.with_token_bloom(bloom);
        }
        if !symbol.modifiers.is_empty() {
            node = node.with_property("modifiers".to_string(), symbol.modifiers.join(" "));
        }
        if let Some(doc) = &symbol.documentation {
            node = node.with_property("documentation".to_string(), doc.clone());
        }
        if let Some(metrics) = rgctl_plugin_api::complexity_from_metadata(symbol) {
            node = node
                .with_property("cyclomatic".to_string(), metrics.cyclomatic.to_string())
                .with_property("cognitive".to_string(), metrics.cognitive.to_string())
                .with_property("loc".to_string(), metrics.loc.to_string())
                .with_property(
                    "nesting_depth".to_string(),
                    metrics.nesting_depth.to_string(),
                )
                .with_property("returns".to_string(), metrics.returns.to_string());
        }
        if let Some(obj) = symbol.metadata.as_object() {
            for (k, v) in obj {
                if k == rgctl_plugin_api::COMPLEXITY_META_KEY
                    || matches!(k.as_str(), "source_start_byte" | "source_end_byte")
                {
                    continue;
                }
                let prop_val = match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Bool(b) => Some(b.to_string()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                };
                if let Some(s) = prop_val {
                    if k == "kind" && s == "field" {
                        node = node.with_label("field".to_string());
                    }
                    node = node.with_property(k.clone(), s);
                }
            }
        }

        if let Some(discriminator) = symbol
            .metadata
            .get("identity_discriminator")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
        {
            node = node.with_identity_discriminator(discriminator);
        }
        let id = node.id;
        if symbol.metadata["callable_role"] == "declaration" {
            self.declaration_ids.insert(id);
        }
        if symbol.metadata["callable_role"] == "implementation"
            && symbol.metadata["superseded_definition"] != true
            && let Some(parameters) = symbol
                .metadata
                .get("parameter_identity")
                .and_then(|v| v.as_str())
        {
            self.callable_parameters.insert(id, parameters.to_owned());
        }
        self.symbol_index.insert(key.clone(), id);
        self.symbol_files.insert(id, symbol.location.file.clone());
        self.index_symbol_resolution(&key.0, &node);
        self.index_type_method(symbol, id);
        self.index_import_binding(symbol);
        self.commit_node(node);
        self.add_edge(id, file_id, EdgeType::DefinedIn);
        self.add_edge(file_id, id, EdgeType::Contains);
        if self.materialize_fields {
            self.add_field_members(id, symbol, file_id);
        }
        id
    }

    /// Record Import symbol bindings for parked-relation module hints.
    fn index_import_binding(&mut self, symbol: &Symbol) {
        if symbol.symbol_type != SymbolType::Import {
            return;
        }
        let raw_module = symbol
            .metadata
            .get("module")
            .or_else(|| symbol.metadata.get("module_path"))
            .and_then(|v| v.as_str())
            .or_else(|| symbol.qualified_name.as_deref().filter(|q| *q != "*"))
            .or(symbol.signature.as_deref())
            .map(clean_import_module_text)
            .filter(|m| !m.is_empty())
            .or_else(|| infer_import_module_from_name(&symbol.name));
        let Some(module) = raw_module.filter(|m| !m.is_empty()) else {
            return;
        };

        let explicit_binding = symbol
            .metadata
            .get("binding")
            .or_else(|| symbol.metadata.get("local"))
            .or_else(|| symbol.metadata.get("alias"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| infer_import_binding_from_name(&symbol.name, &module));

        let by_file = self
            .file_imports_index
            .entry(symbol.location.file.clone())
            .or_default();

        if let Some(binding) = explicit_binding {
            by_file.insert(binding, module.clone());
        } else {
            by_file.insert("*".to_string(), module.clone());
        }

        // Always also bind the trailing identifier of the module path (e.g. `UserService`
        // from `com.acme.billing.UserService` or `acme_billing::user::UserService`) if it
        // looks like a symbol/package name.
        let stripped_wildcard = module
            .strip_suffix(".*")
            .or_else(|| module.strip_suffix("::*"))
            .unwrap_or(&module);
        if stripped_wildcard.len() < module.len() {
            by_file.insert("*".to_string(), stripped_wildcard.to_string());
        } else if let Some(tail) = module.rsplit(['.', '/', ':']).next() {
            let tail = tail.trim();
            if !tail.is_empty() && tail != "*" {
                by_file
                    .entry(tail.to_string())
                    .or_insert_with(|| module.clone());
            }
        }
    }

    /// Index `Owner.method` / `Owner::method` / `Owner#method` for MRO Overrides.
    fn index_type_method(&mut self, symbol: &Symbol, id: Uuid) {
        if !matches!(
            symbol.symbol_type,
            SymbolType::Function | SymbolType::Method
        ) {
            return;
        }
        if symbol.metadata["superseded_definition"] == true {
            return;
        }
        if let Some(identity) = symbol
            .metadata
            .get("override_identity")
            .or_else(|| symbol.metadata.get("identity_discriminator"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            self.method_identities.insert(id, identity.to_owned());
        }
        if let Some(owner) = symbol
            .metadata
            .get("override_owner")
            .and_then(|v| v.as_str())
        {
            self.method_override_owners.insert(id, owner.to_owned());
        }
        if symbol.metadata["language"] == "typescript" {
            self.signature_families.insert(id);
        }
        let Some(qn) = symbol.qualified_name.as_deref() else {
            return;
        };
        let Some((owner, method)) = split_owner_method(qn) else {
            return;
        };
        let method = method.to_string();
        let owners = [owner, type_simple_name(owner)];
        for o in owners {
            if o.is_empty() {
                continue;
            }
            let entry = self.type_methods.entry(o.to_string()).or_default();
            if !entry.iter().any(|(m, uid)| m == &method && *uid == id) {
                entry.push((method.clone(), id));
            }
        }
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
            let key = (
                symbol_key(&symbol.location.file, &field.name, Some(field_qn.as_str())),
                NodeType::Variable,
            );
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
            if let Some(fqn) = symbol
                .metadata
                .get("field_type_fqns")
                .and_then(|v| v.get(&field.name))
                .and_then(|v| v.as_str())
            {
                node = node.with_property("field_type_fqn".into(), fqn.to_owned());
            }
            if let Some(span) = field.source_span {
                node = node
                    .with_byte_spans(
                        Some(span.start_byte),
                        Some(span.end_byte),
                        span.name_rel_offset,
                        span.name_token_len,
                        span.name_span_overflow,
                    )
                    .with_location(span.start_line as usize, span.end_line as usize);
            }
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
            self.index_symbol_resolution(&key.0, &node);
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
        let mut key = (
            symbol_key(
                &symbol.location.file,
                &symbol.name,
                symbol.qualified_name.as_deref(),
            ),
            symbol_type_to_node_type(symbol.symbol_type),
        );
        if let Some(discriminator) = symbol
            .metadata
            .get("identity_discriminator")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
        {
            key.0.push('\u{1f}');
            key.0.push_str(discriminator);
        }
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
                let parts: Vec<&str> = key.0.split("::").collect();
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

        self.member_method_ids.clear();
        for (owner, methods) in &self.type_methods {
            if self
                .symbols_by_qualified
                .get(owner)
                .or_else(|| self.symbols_by_suffix.get(owner))
                .is_some_and(|ids| {
                    ids.iter().any(|id| {
                        matches!(
                            self.node_types.get(id),
                            Some(
                                NodeType::Class
                                    | NodeType::Struct
                                    | NodeType::Interface
                                    | NodeType::Enum
                                    | NodeType::Annotation
                            )
                        )
                    })
                })
            {
                self.member_method_ids
                    .extend(methods.iter().map(|(_, id)| *id));
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
            if let Some(id) =
                single_c_preferred_impl(ids, &self.symbol_files, &self.declaration_ids)
            {
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

    /// Resolve only the supplied canonical identity. Do not fall back to a
    /// different overload if the exact identity is absent or ambiguous.
    fn resolve_callable_identity(
        &self,
        name: &str,
        file: &str,
        discriminator: &str,
    ) -> Option<Uuid> {
        let key = if discriminator.is_empty() {
            format!("{file}::{name}")
        } else {
            format!("{file}::{name}\u{1f}{discriminator}")
        };
        let lookup = (key, NodeType::Function);
        self.symbol_index.get(&lookup).copied().or_else(|| {
            self.symbols_by_suffix
                .get(&lookup.0[file.len() + 2..])
                .and_then(|ids| self.unique_typed(ids, Some(NodeType::Function)))
        })
    }

    // Argument tuples are evidence, not canonical identities: qualifiers,
    // generic arity and implementation roles may distinguish the same tuple.
    fn resolve_argument_identity(
        &self,
        name: &str,
        file: &str,
        arguments: &str,
        free_only: bool,
    ) -> Option<Option<Uuid>> {
        let candidates = self
            .symbols_by_qualified
            .get(name)
            .filter(|ids| {
                ids.iter().any(|id| {
                    self.node_types.get(id) == Some(&NodeType::Function)
                        && (!free_only || !self.member_method_ids.contains(id))
                })
            })
            .or_else(|| self.symbols_by_suffix.get(name))?;
        let mut known = false;
        let mut global = None;
        let mut local = None;
        let mut global_ambiguous = false;
        let mut local_ambiguous = false;
        for id in candidates {
            if free_only && self.member_method_ids.contains(id) {
                continue;
            }
            if let Some(parameters) = self.callable_parameters.get(id) {
                known = true;
                if parameters != arguments {
                    continue;
                }
                if global.is_some_and(|old| old != *id) {
                    global_ambiguous = true;
                } else {
                    global = Some(*id);
                }
                if self.symbol_files.get(id).is_some_and(|path| path == file) {
                    if local.is_some_and(|old| old != *id) {
                        local_ambiguous = true;
                    } else {
                        local = Some(*id);
                    }
                }
            }
        }
        known.then_some(if local_ambiguous {
            None
        } else if local.is_some() {
            local
        } else if global_ambiguous {
            None
        } else {
            global
        })
    }

    /// Resolve a signature-aware call within its receiver/lexical owner before
    /// considering any speculative bare-name fallback. `Some(None)` means a
    /// known family was ambiguous or incompatible and must not be bypassed.
    fn resolve_scoped_callable(
        &self,
        name: &str,
        file: &str,
        identity: Option<&str>,
        arguments: Option<&str>,
        inherited_overloads: bool,
        free_only: bool,
    ) -> Option<Option<Uuid>> {
        let direct = |name: &str| {
            let candidates = self
                .symbols_by_qualified
                .get(name)
                .filter(|ids| {
                    ids.iter().any(|id| {
                        self.node_types.get(id) == Some(&NodeType::Function)
                            && (!free_only || !self.member_method_ids.contains(id))
                    })
                })
                .or_else(|| self.symbols_by_suffix.get(name))?;
            let eligible = |id: &Uuid| {
                self.node_types.get(id) == Some(&NodeType::Function)
                    && (!free_only || !self.member_method_ids.contains(id))
            };
            if !candidates.iter().any(eligible) {
                return None;
            }
            if let Some(identity) = identity {
                return Some(
                    self.resolve_callable_identity(name, file, identity)
                        .filter(eligible),
                );
            }
            if let Some(arguments) = arguments {
                // Non-overloaded declarations need no argument-type proof.
                // Literal spellings (e.g. Rust int vs i32) are only hints.
                if let Some(id) = unique_matching(candidates, eligible)
                    && !self.method_identities.contains_key(&id)
                    && !self.declaration_ids.contains(&id)
                {
                    return Some(Some(id));
                }
                return Some(
                    self.resolve_argument_identity(name, file, arguments, free_only)
                        .unwrap_or_else(|| {
                            self.resolve_callable_identity(name, file, arguments)
                                .filter(eligible)
                        }),
                );
            }
            Some(unique_matching(candidates, eligible))
        };
        let local = direct(name);
        if local.is_some_and(|found| found.is_some())
            || local.is_some()
                && !(inherited_overloads && arguments.is_some() && identity.is_none())
        {
            return local;
        }
        let Some((owner, method)) = split_owner_method(name) else {
            return local;
        };
        if !self.inheritance_bases.contains_key(type_simple_name(owner)) {
            return local;
        }
        let chain = compute_mro_chain(owner, &self.inheritance_bases);
        let mut key = String::with_capacity(name.len());
        for ancestor in chain.iter().skip(1) {
            for separator in [".", "::", "#"] {
                key.clear();
                key.push_str(ancestor);
                key.push_str(separator);
                key.push_str(method);
                if let Some(found) = direct(&key) {
                    if found.is_some()
                        || !inherited_overloads
                        || arguments.is_none()
                        || identity.is_some()
                    {
                        return Some(found);
                    }
                }
            }
        }
        local
    }

    /// Add a relation between symbols when endpoints resolve.
    ///
    /// For allowlisted relation kinds, missing endpoints become deduplicated
    /// external stub nodes (`is_external_stub`) so Instantiates / JPMS /
    /// AnnotatedWith edges survive into GQL. Field-like `References` are not
    /// stubbed (they must not invent Class nodes named like fields).
    ///
    /// Unresolved or stubbed `Calls`/`Uses`/`Extends`/`Implements`/`Injects`
    /// from in-repo nodes are also parked for cross-repo `rgctl merge`.
    pub fn add_relation(&mut self, relation: &Relation) -> Result<()> {
        // Fast reject: unresolved cross-file C calls (no stub policy for Calls).
        // Still park when `from` resolves so cross-repo merge can recover.
        if relation.relation_type == RelationType::Calls
            && relation.metadata.get("language").and_then(|v| v.as_str()) == Some("c")
            && !self.c_preferred_impl.contains_key(&relation.to)
            && !self.suffix_resolve_cache.contains_key(&relation.to)
        {
            let can_resolve = self.symbols_by_suffix.get(&relation.to).is_some_and(|ids| {
                self.unique_typed(ids, Some(NodeType::Function)).is_some()
                    || single_c_preferred_impl(ids, &self.symbol_files, &self.declaration_ids)
                        .is_some()
            });
            if !can_resolve {
                let mut from_id = self.resolve_symbol_tracked_typed(
                    &relation.from,
                    &relation.location.file,
                    None,
                    None,
                    Some(NodeType::Function),
                );
                if from_id.is_none() {
                    from_id = self
                        .symbols_by_qualified
                        .get(&relation.from)
                        .and_then(|ids| self.unique_typed(ids, Some(NodeType::Function)));
                }
                if let Some(from) = from_id {
                    self.park_relation_if_needed(
                        relation,
                        from,
                        None,
                        relation.to_type_hint.clone(),
                        relation.to_qualified_hint.clone(),
                    );
                }
                return Ok(());
            }
        }

        let exact_from = relation
            .metadata
            .get("from_identity_discriminator")
            .and_then(|v| v.as_str());
        let mut from_id = if let Some(identity) = exact_from {
            self.resolve_callable_identity(&relation.from, &relation.location.file, identity)
        } else {
            self.resolve_symbol_tracked_typed(
                &relation.from,
                &relation.location.file,
                None,
                None,
                (relation.relation_type == RelationType::Calls).then_some(NodeType::Function),
            )
        };
        if from_id.is_none() && exact_from.is_none() {
            from_id = self
                .symbols_by_qualified
                .get(&relation.from)
                .and_then(|ids| {
                    self.unique_typed(
                        ids,
                        (relation.relation_type == RelationType::Calls)
                            .then_some(NodeType::Function),
                    )
                });
        }
        if from_id.is_none() && exact_from.is_none() {
            from_id = self.lookup_file_node(&relation.from, &relation.location.file);
        }

        let (to_type_hint, to_qualified_hint) = self.enrich_go_type_hints(relation);

        let java_call = relation.relation_type == RelationType::Calls
            && relation.metadata.get("language").and_then(|v| v.as_str()) == Some("java");
        let target = to_qualified_hint
            .as_deref()
            .or(relation.to_qualified_hint.as_deref())
            .unwrap_or(&relation.to);
        let explicit_to = relation
            .metadata
            .get("to_identity_discriminator")
            .and_then(|v| v.as_str());
        let argument_to = relation
            .metadata
            .get("argument_identity_discriminator")
            .and_then(|v| v.as_str());
        let unresolved_generic =
            relation.metadata.get("explicit_generic_arity").is_some() && explicit_to.is_none();
        let mut scoped = if relation.relation_type == RelationType::Calls {
            self.resolve_scoped_callable(
                target,
                &relation.location.file,
                explicit_to,
                argument_to,
                java_call,
                false,
            )
        } else {
            None
        };
        // Only a helper-inferred owner may fall back to the unqualified name.
        // Explicit receiver hints and known-but-incompatible families are final.
        let inferred_owner = relation.metadata["inferred_callable_owner"] == true;
        if scoped.is_none() && inferred_owner && explicit_to.is_none() {
            scoped = self.resolve_scoped_callable(
                &relation.to,
                &relation.location.file,
                None,
                argument_to,
                java_call,
                true,
            );
        }
        let exact_id = if unresolved_generic {
            None
        } else {
            scoped.flatten()
        };
        let signature_call = java_call
            || relation.relation_type == RelationType::Calls
                && relation
                    .metadata
                    .get("exact_callable_resolution")
                    .and_then(|v| v.as_bool())
                    == Some(true);
        let ambiguous_java = signature_call
            && self
                .symbols_by_qualified
                .get(target)
                .or_else(|| self.symbols_by_suffix.get(target))
                .is_some_and(|ids| self.unique_typed(ids, Some(NodeType::Function)).is_none());
        let mut to_id = if exact_id.is_some() {
            exact_id
        } else if explicit_to.is_some()
            || ambiguous_java
            || signature_call && (argument_to.is_some() || scoped == Some(None))
        {
            None
        } else {
            self.resolve_symbol_tracked_typed(
                &relation.to,
                &relation.location.file,
                to_qualified_hint
                    .as_deref()
                    .or(relation.to_qualified_hint.as_deref()),
                to_type_hint.as_deref().or(relation.to_type_hint.as_deref()),
                (relation.relation_type == RelationType::Calls).then_some(NodeType::Function),
            )
        };
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

        // Signature text can contain namespace separators (for example Rust
        // trait names). A suffix candidate must still satisfy the typed edge.
        if is_inheritance_relation(relation.relation_type)
            && let Some(to) = to_id
            && !matches!(
                self.node_types.get(&to),
                Some(
                    NodeType::Class
                        | NodeType::Struct
                        | NodeType::Enum
                        | NodeType::Interface
                        | NodeType::Module
                        | NodeType::Annotation
                        | NodeType::TypeAlias
                )
            )
        {
            to_id = None;
        }

        if relation_allows_external_stub(relation) {
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

        if let Some(from) = from_id {
            self.park_relation_if_needed(
                relation,
                from,
                to_id,
                to_type_hint
                    .clone()
                    .or_else(|| relation.to_type_hint.clone())
                    .or_else(|| {
                        relation
                            .metadata
                            .get("to_type_hint")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                    }),
                to_qualified_hint
                    .clone()
                    .or_else(|| relation.to_qualified_hint.clone()),
            );
        }

        if signature_call && (from_id.is_none() || to_id.is_none()) {
            self.unresolved_calls += 1;
            if self.callable_resolution_samples.len() < crate::callable_resolution::DIAGNOSTIC_LIMIT
            {
                let mut sample = relation.clone();
                sample.metadata["resolution_reason"]="No unique canonical callable matched the available source evidence; no name-only overload fallback was applied".into();
                self.callable_resolution_samples.push(sample);
            }
        }

        if let (Some(from), Some(to)) = (from_id, to_id) {
            let edge_type = relation_type_to_edge_type(relation.relation_type);
            let mut edge = Edge::new(from, to, edge_type);
            if relation.relation_type == RelationType::Calls {
                edge = edge.with_property(
                    "call_site_line".to_string(),
                    relation.location.start_line.to_string(),
                );
                // Persist receiver type / callee for post-ingest MRO retargeting.
                let hint = to_type_hint
                    .as_deref()
                    .or(relation.to_type_hint.as_deref())
                    .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("file"));
                if let Some(h) = hint {
                    edge = edge.with_property("to_type_hint".to_string(), h.to_string());
                }
                let callee = relation
                    .to
                    .rsplit(['.', ':', '#'])
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(relation.to.as_str());
                if !callee.is_empty() {
                    edge = edge.with_property("callee".to_string(), callee.to_string());
                }
            }
            if is_inheritance_relation(relation.relation_type) {
                let child = type_simple_name(&relation.from);
                let parent = type_simple_name(
                    relation
                        .to_qualified_hint
                        .as_deref()
                        .unwrap_or(relation.to.as_str()),
                );
                let is_iface = matches!(
                    relation.relation_type,
                    RelationType::Implements | RelationType::IncludesClass
                ) || relation.to_type_hint.as_deref().is_some_and(|h| {
                    matches!(
                        h.to_ascii_lowercase().as_str(),
                        "interface" | "trait" | "module" | "annotation"
                    )
                });
                let prepend_front = relation
                    .metadata
                    .get("mixin")
                    .and_then(|v| v.as_str())
                    .is_some_and(|m| matches!(m, "include" | "prepend"));
                self.record_inheritance(child, parent, is_iface, prepend_front);
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
            if matches!(
                relation.relation_type,
                RelationType::ExposesRoute | RelationType::Injects
            ) {
                for key in [
                    "http_method",
                    "route_path",
                    "framework",
                    "rule_id",
                    "call_context",
                ] {
                    if let Some(v) = relation
                        .metadata
                        .get(key)
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        edge = edge.with_property(key.to_string(), v.to_string());
                    }
                }
            }
            if relation.relation_type == RelationType::Injects
                && let Some(target_vocab) = relation
                    .metadata
                    .get("target_vocab")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            {
                self.append_target_vocab(to, target_vocab);
            }
            self.commit_edge(edge);
        }
        Ok(())
    }

    /// Append `target_vocab` tokens onto an in-repo Injects target's `semantic_tags` in O(1).
    fn append_target_vocab(&mut self, to: Uuid, target_vocab: &str) {
        if self.spill.is_some() {
            return;
        }
        if self.node_index_by_id.is_empty() && !self.nodes.is_empty() {
            self.node_index_by_id.reserve(self.nodes.len());
            for (idx, n) in self.nodes.iter().enumerate() {
                self.node_index_by_id.insert(n.id, idx);
            }
        }
        let Some(&idx) = self.node_index_by_id.get(&to) else {
            return;
        };
        let Some(node) = self.nodes.get_mut(idx) else {
            return;
        };
        if node.properties.get("is_external_stub").map(String::as_str) == Some("true") {
            return;
        }
        let existing = node
            .properties
            .get("semantic_tags")
            .cloned()
            .unwrap_or_default();
        let mut parts: Vec<&str> = existing
            .split_whitespace()
            .chain(target_vocab.split_whitespace())
            .filter(|t| !t.is_empty())
            .collect();
        parts.sort_unstable();
        parts.dedup();
        let merged = parts.join(" ");
        node.properties
            .insert("semantic_tags".to_string(), merged.clone());
        let bloom = build_token_bloom(
            &node.name,
            node.qualified_name.as_deref(),
            node.signature_text(),
            None,
            Some(merged.as_str()),
        );
        let mut cur = node.token_bloom.unwrap_or([0; 4]);
        for (slot, b) in cur.iter_mut().zip(bloom) {
            *slot |= b;
        }
        node.token_bloom = Some(cur);
    }

    fn record_inheritance(
        &mut self,
        child: &str,
        parent: &str,
        is_interface: bool,
        prepend_front: bool,
    ) {
        if child.is_empty() || parent.is_empty() || child == parent {
            return;
        }
        let entry = self.inheritance_bases.entry(child.to_string()).or_default();
        if !entry.iter().any(|(p, _)| p == parent) {
            if prepend_front {
                entry.insert(0, (parent.to_string(), is_interface));
            } else {
                entry.push((parent.to_string(), is_interface));
            }
            self.mro_cache.clear();
        }
    }

    /// Class-over-interface MRO: `self` → non-interface bases (DFS) → interfaces.
    /// Good default for Java/TS/Kotlin/PHP and a sound ancestor walk for call resolve.
    fn mro_chain_cached(&mut self, type_name: &str) -> Vec<String> {
        let key = type_simple_name(type_name).to_string();
        if let Some(cached) = self.mro_cache.get(key.as_str()) {
            return cached.clone();
        }
        let out = compute_mro_chain(type_name, &self.inheritance_bases);
        self.mro_cache.insert(key, out.clone());
        out
    }

    /// Resolve `method` on `type_name` by walking the MRO (`sep` ∈ `.` / `::` / `#`).
    fn resolve_via_mro(&mut self, type_name: &str, method: &str, skip_self: bool) -> Option<Uuid> {
        if method.is_empty() || type_name.is_empty() {
            return None;
        }
        // C / non-OOP corpora: skip String + Vec allocs when no inheritance graph.
        if self.inheritance_bases.is_empty()
            || !self
                .inheritance_bases
                .contains_key(type_simple_name(type_name))
        {
            return None;
        }
        let chain = self.mro_chain_cached(type_name);
        self.lookup_method_on_chain(&chain, method, skip_self)
    }

    fn resolve_via_mro_ref(&self, type_name: &str, method: &str, skip_self: bool) -> Option<Uuid> {
        if method.is_empty() || type_name.is_empty() {
            return None;
        }
        if self.inheritance_bases.is_empty()
            || !self
                .inheritance_bases
                .contains_key(type_simple_name(type_name))
        {
            return None;
        }
        let chain = self
            .mro_cache
            .get(type_simple_name(type_name))
            .cloned()
            .unwrap_or_else(|| compute_mro_chain(type_name, &self.inheritance_bases));
        self.lookup_method_on_chain(&chain, method, skip_self)
    }

    fn lookup_method_on_chain(
        &self,
        chain: &[String],
        method: &str,
        skip_self: bool,
    ) -> Option<Uuid> {
        // Always skip chain[0] (self): Step 2/3 already probed the receiver type
        // via qualified/suffix indexes. Re-walking self through `type_methods`
        // would collapse ambiguous FQNs (QE duplicate-qualified-name invariant).
        // `skip_self` is retained for API clarity (super.* callers pass true).
        let _ = skip_self;
        // Reuse one buffer across separator probes — avoid `format!` per hop on
        // the sequential merge / relation-resolution thread (AGENTS.md hot path).
        let mut key = String::with_capacity(64);
        for ancestor in chain.iter().skip(1) {
            for sep in [".", "::", "#"] {
                key.clear();
                key.push_str(ancestor);
                key.push_str(sep);
                key.push_str(method);
                if let Some(id) = self
                    .symbols_by_qualified
                    .get(&key)
                    .and_then(|ids| unique_resolved(ids))
                {
                    return Some(id);
                }
                if let Some(id) = self
                    .symbols_by_suffix
                    .get(&key)
                    .and_then(|ids| unique_resolved(ids))
                {
                    return Some(id);
                }
            }
            if let Some(id) = self
                .type_methods
                .get(ancestor)
                .and_then(|ms| unique_method_id(ms, method))
            {
                return Some(id);
            }
        }
        None
    }

    /// Emit `EdgeType::Overrides` for every subtype method that shadows an ancestor.
    ///
    /// Call after inheritance relations are committed (end of pass 2).
    pub fn emit_mro_overrides(&mut self) {
        if self.inheritance_bases.is_empty() || self.type_methods.is_empty() {
            return;
        }
        let owners: Vec<String> = self.type_methods.keys().cloned().collect();
        let mut edges = Vec::new();
        for owner in owners {
            let methods = match self.type_methods.get(&owner) {
                Some(m) => m.clone(),
                None => continue,
            };
            let chain = self.mro_chain_cached(&owner);
            if chain.len() < 2 {
                continue;
            }
            for (method, child_id) in methods {
                for ancestor in chain.iter().skip(1) {
                    if self
                        .method_override_owners
                        .get(&child_id)
                        .is_some_and(|owner| {
                            owner != ancestor && type_simple_name(owner) != ancestor
                        })
                    {
                        continue;
                    }
                    let Some(methods) = self.type_methods.get(ancestor) else {
                        continue;
                    };
                    let identity = self.method_identities.get(&child_id);
                    let mut candidates: Vec<_> = methods
                        .iter()
                        .filter(|(name, id)| {
                            name == &method && self.method_identities.get(id) == identity
                        })
                        .map(|(_, id)| *id)
                        .collect();
                    // A paired declaration must not hide its runtime body.
                    if candidates
                        .iter()
                        .any(|id| !self.declaration_ids.contains(id))
                    {
                        candidates.retain(|id| !self.declaration_ids.contains(id));
                    }
                    if candidates.len() != 1
                        && !(self.signature_families.contains(&child_id)
                            && !candidates.is_empty()
                            && candidates
                                .iter()
                                .all(|id| self.signature_families.contains(id)))
                    {
                        continue;
                    }
                    for ancestor_id in candidates {
                        if ancestor_id != child_id {
                            edges.push(Edge::new(child_id, ancestor_id, EdgeType::Overrides));
                        }
                    }
                    break; // nearest ancestor with a matching dispatch contract
                }
            }
        }
        for edge in edges {
            self.commit_edge(edge);
        }
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
    ///
    /// External stubs are indexed under `symbols_by_qualified` / `symbol_index` only —
    /// never `symbols_by_suffix` — so a stub cannot hijack bare simple-name lookups (#125).
    fn ensure_external_stub(
        &mut self,
        name: &str,
        qualified_hint: Option<&str>,
        node_type: NodeType,
    ) -> Uuid {
        // Preserve full HTTP route names — `rsplit` on `/` would turn
        // `"GET /api/users/{id}"` into `"{id}"`.
        let is_route_stub = qualified_hint.is_some_and(|h| h.starts_with("route::"))
            || name.starts_with('/')
            || [
                "GET ", "POST ", "PUT ", "DELETE ", "PATCH ", "OPTIONS ", "HEAD ", "ANY ",
            ]
            .iter()
            .any(|p| name.starts_with(p));
        let simple = if is_route_stub {
            name
        } else {
            name.rsplit(['.', '/', ':'])
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(name)
        };
        let qn = qualified_hint
            .filter(|s| !s.is_empty())
            .unwrap_or(name)
            .to_string();

        let compatible = |id: &Uuid| {
            let Some(kind) = self.node_types.get(id) else {
                return false;
            };
            if matches!(
                node_type,
                NodeType::Class | NodeType::Interface | NodeType::Annotation
            ) {
                matches!(
                    kind,
                    NodeType::Class
                        | NodeType::Struct
                        | NodeType::Enum
                        | NodeType::Interface
                        | NodeType::Module
                        | NodeType::Annotation
                        | NodeType::TypeAlias
                )
            } else {
                *kind == node_type
            }
        };
        // Prefer an existing unique resolution (another stub or late symbol).
        if let Some(id) = self
            .symbols_by_qualified
            .get(&qn)
            .and_then(|ids| unique_matching(ids, compatible))
        {
            return id;
        }
        // Only coalesce via bare suffix when the caller did not provide a distinct QN.
        // Otherwise `com.foo.Builder` and `org.bar.Builder` would collapse.
        if qualified_hint.is_none() {
            if let Some(id) = self
                .symbols_by_suffix
                .get(name)
                .and_then(|ids| unique_matching(ids, compatible))
            {
                return id;
            }
            if name != simple
                && let Some(id) = self
                    .symbols_by_suffix
                    .get(simple)
                    .and_then(|ids| unique_matching(ids, compatible))
            {
                return id;
            }
        }

        const STUB_FILE: &str = "<external>";
        let key = (symbol_key(STUB_FILE, simple, Some(qn.as_str())), node_type);
        if let Some(&id) = self.symbol_index.get(&key) {
            self.external_stub_ids.insert(id);
            return id;
        }

        let file_id = self.ensure_file_node(Path::new(STUB_FILE));
        // JPMS / dotted module ids keep the full name; types use the last segment.
        // Route stubs keep the full `"METHOD /path"` display name.
        let display_name = if node_type == NodeType::Module {
            qn.clone()
        } else {
            simple.to_string()
        };
        let mut node = Node::new(node_type, display_name)
            .with_qualified_name(qn.clone())
            .with_file_path(STUB_FILE.to_string())
            .with_property("is_external_stub".to_string(), "true".to_string());
        if is_route_stub {
            node = node.with_label("route".to_string());
        }
        let id = node.id;
        self.symbol_index.insert(key, id);
        self.external_stub_ids.insert(id);
        self.symbols_by_qualified.entry(qn).or_default().push(id);
        self.commit_node(node);
        self.add_edge(id, file_id, EdgeType::DefinedIn);
        self.add_edge(file_id, id, EdgeType::Contains);
        id
    }

    /// Park unresolved / external-stub targets for cross-repo merge when applicable.
    fn park_relation_if_needed(
        &mut self,
        relation: &Relation,
        from_id: Uuid,
        to_id: Option<Uuid>,
        to_type_hint: Option<String>,
        to_qualified_hint: Option<String>,
    ) {
        if !is_parkable_relation(relation.relation_type) {
            return;
        }
        let stub_node_id = to_id.filter(|id| self.is_external_stub_id(*id));
        let unresolved = to_id.is_none();
        if !unresolved && stub_node_id.is_none() {
            return;
        }

        let file_path = &relation.location.file;
        let import_module_hint =
            self.lookup_import_module_hint(file_path, &relation.to, to_type_hint.as_deref());

        let identity = relation
            .metadata
            .get("to_identity_discriminator")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .or_else(|| {
                relation
                    .metadata
                    .get("argument_identity_discriminator")
                    .and_then(|v| v.as_str())
                    .map(|arguments| format!("args:{arguments}"))
            });
        let to_qualified_hint = identity
            .map(|identity| {
                format!(
                    "{}\u{1f}{identity}",
                    to_qualified_hint.as_deref().unwrap_or(&relation.to)
                )
            })
            .or(to_qualified_hint);

        self.parked_relations.push(ParkedRelationRecord {
            from_node_id: from_id,
            from_qualified_name: relation.from.clone(),
            from_file: file_path.clone(),
            to_name: relation.to.clone(),
            to_qualified_hint,
            to_type_hint,
            import_module_hint,
            relation_type: relation.relation_type,
            call_site_line: relation.location.start_line as u32,
            stub_node_id,
        });
    }

    #[inline]
    fn is_external_stub_id(&self, id: Uuid) -> bool {
        self.external_stub_ids.contains(&id)
    }

    fn lookup_import_module_hint(
        &self,
        file_path: &str,
        to_name: &str,
        to_type_hint: Option<&str>,
    ) -> Option<String> {
        let by_file = self.file_imports_index.get(file_path)?;
        if let Some(module) = by_file.get(to_name) {
            return Some(module.clone());
        }
        if let Some(hint) = to_type_hint
            && let Some(module) = by_file.get(hint)
        {
            return Some(module.clone());
        }
        // `UserService.charge` / `UserService::charge` → bind on receiver prefix.
        let prefix = to_name
            .split(['.', ':'])
            .next()
            .filter(|s| !s.is_empty() && *s != to_name);
        if let Some(prefix) = prefix
            && let Some(module) = by_file.get(prefix)
        {
            return Some(module.clone());
        }
        // Wildcard import fallback.
        by_file.get("*").cloned()
    }

    /// Bounded diagnostic evidence for missing or ambiguous canonical calls.
    pub fn take_callable_resolution_report(
        &mut self,
    ) -> crate::callable_resolution::CallableResolutionReport {
        let unresolved_calls = std::mem::take(&mut self.unresolved_calls);
        let mut samples = std::mem::take(&mut self.callable_resolution_samples);
        samples.sort_by(|a, b| {
            (
                &a.location.file,
                a.location.start_line,
                a.location.start_column,
                &a.from,
                &a.to,
            )
                .cmp(&(
                    &b.location.file,
                    b.location.start_line,
                    b.location.start_column,
                    &b.from,
                    &b.to,
                ))
        });
        crate::callable_resolution::CallableResolutionReport {
            schema_version: 1,
            unresolved_calls,
            samples_truncated: unresolved_calls > samples.len(),
            samples,
        }
    }

    pub fn take_parked_relations(&mut self) -> Vec<ParkedRelationRecord> {
        std::mem::take(&mut self.parked_relations)
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
        // Match on the key-path tail without allocating `format!("::{key}")` per probe.
        for (lookup, id) in &self.config_key_nodes {
            if lookup.rsplit("::").next() == Some(key) {
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

        let mut node = Node::new(NodeType::Variable, key.to_string())
            .with_label("env".to_string())
            .with_property("env_var".to_string(), key.to_string());
        node.id = rgctl_graph::stable_key::deterministic_node_id(
            Some("<env>"),
            key,
            NodeType::Variable,
            Some(&format!("env::{key}")),
        );

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

    fn unique_typed(&self, ids: &[Uuid], expected: Option<NodeType>) -> Option<Uuid> {
        let mut result = None;
        for id in ids {
            if expected.is_some_and(|kind| self.node_types.get(id) != Some(&kind)) {
                continue;
            }
            if result.is_some_and(|old| old != *id) {
                return None;
            }
            result = Some(*id);
        }
        result
    }

    fn exact_symbol(&self, name: &str, file: &str, expected: Option<NodeType>) -> Option<Uuid> {
        let mut lookup = (
            format!("{file}::{name}"),
            expected.unwrap_or(NodeType::Function),
        );
        if expected.is_some() {
            return self.symbol_index.get(&lookup).copied();
        }
        let candidates = self
            .symbols_by_qualified
            .get(name)
            .or_else(|| self.symbols_by_suffix.get(name))?;
        let mut result = None;
        for id in candidates {
            lookup.1 = *self.node_types.get(id)?;
            if let Some(id) = self.symbol_index.get(&lookup) {
                if result.is_some_and(|old| old != *id) {
                    return None;
                }
                result = Some(*id);
            }
        }
        result
    }

    /// Resolve a symbol name to its UUID with performance tracking.
    ///
    /// Ambiguous qualified-name or suffix matches return `None` (do not pick arbitrarily).
    #[cfg(test)]
    fn resolve_symbol_tracked(
        &mut self,
        name: &str,
        file: &str,
        qualified_hint: Option<&str>,
        type_hint: Option<&str>,
    ) -> Option<Uuid> {
        self.resolve_symbol_tracked_typed(name, file, qualified_hint, type_hint, None)
    }

    fn resolve_symbol_tracked_typed(
        &mut self,
        name: &str,
        file: &str,
        qualified_hint: Option<&str>,
        type_hint: Option<&str>,
        expected: Option<NodeType>,
    ) -> Option<Uuid> {
        use std::time::Instant;

        let start = Instant::now();
        self.resolution_stats.total_calls += 1;

        // 1. Try exact match in current file
        if let Some(id) = self.exact_symbol(name, file, expected) {
            self.resolution_stats.hashmap_hits += 1;
            self.resolution_stats.total_time += start.elapsed();
            return Some(id);
        }

        // 2. Try qualified hint direct lookup (O(1)); only if uniquely resolved
        if let Some(hint) = qualified_hint {
            self.resolution_stats.qualified_hint_scans += 1;

            if let Some(id) = self
                .symbols_by_qualified
                .get(hint)
                .and_then(|ids| self.unique_typed(ids, expected))
            {
                self.resolution_stats.qualified_hint_hits += 1;
                self.resolution_stats.total_time += start.elapsed();
                return Some(id);
            }

            if let Some(id) = self
                .symbols_by_suffix
                .get(hint)
                .and_then(|ids| self.unique_typed(ids, expected))
            {
                self.resolution_stats.qualified_hint_hits += 1;
                self.resolution_stats.total_time += start.elapsed();
                return Some(id);
            }
        }

        // 3. Try type hint + simple name (O(1)); walk MRO when the subtype
        //    did not declare the method (inherited Calls would otherwise drop).
        if let Some(type_name) = type_hint {
            self.resolution_stats.type_hint_scans += 1;
            let simple_name = method_simple_name(name, qualified_hint);
            let type_name = type_simple_name(type_name);
            let skip_self = is_super_call(name, qualified_hint, type_name);

            let mut type_qualified = String::with_capacity(type_name.len() + simple_name.len() + 2);
            for sep in [".", "::", "#"] {
                type_qualified.clear();
                type_qualified.push_str(type_name);
                type_qualified.push_str(sep);
                type_qualified.push_str(simple_name);
                if let Some(id) = self
                    .symbols_by_suffix
                    .get(&type_qualified)
                    .and_then(|ids| self.unique_typed(ids, expected))
                    .or_else(|| {
                        self.symbols_by_qualified
                            .get(&type_qualified)
                            .and_then(|ids| self.unique_typed(ids, expected))
                    })
                {
                    self.resolution_stats.type_hint_hits += 1;
                    self.resolution_stats.total_time += start.elapsed();
                    return Some(id);
                }
            }

            if let Some(id) = self.resolve_via_mro(type_name, simple_name, skip_self) {
                self.resolution_stats.type_hint_hits += 1;
                self.resolution_stats.total_time += start.elapsed();
                return Some(id);
            }
        } else if let Some(hint) = qualified_hint
            && let Some((owner, method)) = split_owner_method(hint)
        {
            // Qualified hint alone (e.g. `Child.run`) — walk MRO when Child has no `run`.
            self.resolution_stats.type_hint_scans += 1;
            let skip_self = is_super_call(name, Some(hint), owner);
            if let Some(id) = self.resolve_via_mro(owner, method, skip_self) {
                self.resolution_stats.type_hint_hits += 1;
                self.resolution_stats.total_time += start.elapsed();
                return Some(id);
            }
        }

        // 4. Fallback: suffix index — `.c` preference map, then lazy cache, then live lookup.
        self.resolution_stats.fuzzy_scans += 1;
        let result = if qualified_hint.is_none() && type_hint.is_none() {
            if let Some(id) = self
                .c_preferred_impl
                .get(name)
                .filter(|id| expected.is_none_or(|kind| self.node_types.get(id) == Some(&kind)))
            {
                Some(*id)
            } else if let Some(id) = self
                .suffix_resolve_cache
                .get(name)
                .filter(|_| expected.is_none())
            {
                Some(*id)
            } else {
                let resolved = self.symbols_by_suffix.get(name).and_then(|ids| {
                    self.unique_typed(ids, expected).or_else(|| {
                        single_c_preferred_impl(ids, &self.symbol_files, &self.declaration_ids)
                            .filter(|id| {
                                expected.is_none_or(|kind| self.node_types.get(id) == Some(&kind))
                            })
                    })
                });
                if let Some(id) = resolved.filter(|_| expected.is_none()) {
                    self.suffix_resolve_cache.insert(name.to_string(), id);
                }
                resolved
            }
        } else {
            self.symbols_by_suffix
                .get(name)
                .and_then(|ids| self.unique_typed(ids, expected))
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
        if let Some(id) = self.exact_symbol(name, file, None) {
            return Some(id);
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

        if let Some(type_name) = type_hint {
            let simple_name = method_simple_name(name, qualified_hint);
            let type_name = type_simple_name(type_name);
            let skip_self = is_super_call(name, qualified_hint, type_name);
            for sep in [".", "::", "#"] {
                let type_qualified = format!("{type_name}{sep}{simple_name}");
                if let Some(id) = self
                    .symbols_by_suffix
                    .get(&type_qualified)
                    .and_then(|ids| unique_resolved(ids))
                    .or_else(|| {
                        self.symbols_by_qualified
                            .get(&type_qualified)
                            .and_then(|ids| unique_resolved(ids))
                    })
                {
                    return Some(id);
                }
            }
            if let Some(id) = self.resolve_via_mro_ref(type_name, simple_name, skip_self) {
                return Some(id);
            }
        } else if let Some(hint) = qualified_hint
            && let Some((owner, method)) = split_owner_method(hint)
        {
            let skip_self = is_super_call(name, Some(hint), owner);
            if let Some(id) = self.resolve_via_mro_ref(owner, method, skip_self) {
                return Some(id);
            }
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

fn unique_matching(ids: &[Uuid], matches: impl Fn(&Uuid) -> bool) -> Option<Uuid> {
    let mut found = None;
    for id in ids.iter().filter(|id| matches(id)) {
        if found.is_some_and(|old| old != *id) {
            return None;
        }
        found = Some(*id);
    }
    found
}

/// Return the sole candidate UUID, or `None` when zero or multiple distinct IDs.
fn unique_resolved(ids: &[Uuid]) -> Option<Uuid> {
    match ids {
        [id] => Some(*id),
        [first, rest @ ..] if rest.iter().all(|id| id == first) => Some(*first),
        _ => None,
    }
}

/// Return the sole method UUID for `method` on an owner, or `None` if ambiguous.
fn unique_method_id(ms: &[(String, Uuid)], method: &str) -> Option<Uuid> {
    let mut found: Option<Uuid> = None;
    for (m, id) in ms {
        if m != method {
            continue;
        }
        match found {
            None => found = Some(*id),
            Some(prev) if prev == *id => {}
            Some(_) => return None,
        }
    }
    found
}

/// When a C symbol appears in both `.h` and `.c`, prefer the sole `.c` definition.
fn single_c_preferred_impl(
    ids: &[Uuid],
    symbol_files: &HashMap<Uuid, String>,
    declarations: &FxHashSet<Uuid>,
) -> Option<Uuid> {
    let mut c_impl = None;
    for id in ids {
        if !declarations.contains(id)
            && symbol_files
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

fn should_sketch_symbol(symbol_type: SymbolType, has_semantic_tags: bool) -> bool {
    matches!(symbol_type, SymbolType::Function | SymbolType::Method)
        || (has_semantic_tags
            && matches!(
                symbol_type,
                SymbolType::Class | SymbolType::Struct | SymbolType::Interface
            ))
}

fn symbol_type_to_node_type(symbol_type: SymbolType) -> NodeType {
    match symbol_type {
        SymbolType::Function | SymbolType::Method => NodeType::Function,
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

fn is_parkable_relation(relation_type: RelationType) -> bool {
    matches!(
        relation_type,
        RelationType::Calls
            | RelationType::Uses
            | RelationType::Extends
            | RelationType::Implements
            | RelationType::Injects
    )
}

fn clean_import_module_text(raw: &str) -> String {
    let mut s = raw.trim().trim_end_matches(';').trim();
    for prefix in [
        "import static ",
        "import ",
        "using static ",
        "using ",
        "use ",
        "#include ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim();
            break;
        }
    }
    // C# alias `using Alias = Namespace.Type` → extract RHS module path
    if let Some((_, rhs)) = s.split_once('=') {
        s = rhs.trim();
    }
    // `module:binding` (ECMAScript/Python helper convention, not Rust `::`)
    if !s.contains("::")
        && let Some((module, _)) = s.split_once(':')
        && !module.is_empty()
    {
        s = module.trim();
    }
    s.trim_matches(['"', '\'', '<', '>']).trim().to_string()
}

fn infer_import_module_from_name(name: &str) -> Option<String> {
    let cleaned = clean_import_module_text(name);
    if cleaned.is_empty() {
        return None;
    }
    if cleaned.contains('/')
        || cleaned.contains('.')
        || cleaned.contains("::")
        || cleaned.starts_with('@')
        || name.trim_start().starts_with("import ")
        || name.trim_start().starts_with("using ")
    {
        return Some(cleaned);
    }
    None
}

fn infer_import_binding_from_name(name: &str, cleaned_module: &str) -> Option<String> {
    let trimmed = name.trim().trim_end_matches(';').trim();
    // C# alias: `using Alias = Namespace.Type`
    if let Some(rest) = trimmed.strip_prefix("using ")
        && let Some((lhs, _)) = rest.split_once('=')
    {
        let alias = lhs.trim();
        if !alias.is_empty() && !alias.contains(' ') {
            return Some(alias.to_string());
        }
    }
    // ECMAScript/Python `module:binding` / `module:* as alias` (skip Rust `::`)
    if !trimmed.contains("::")
        && let Some((_, rest)) = trimmed.split_once(':')
    {
        let rest = rest.trim();
        if rest.is_empty() {
            return None;
        }
        if rest == "*" {
            return Some("*".to_string());
        }
        if let Some(alias) = rest.strip_prefix("* as ") {
            let alias = alias.trim();
            if !alias.is_empty() {
                return Some(alias.to_string());
            }
        }
        return Some(rest.to_string());
    }
    // Rust / Go / Zig style: `symbol.name` is a bare identifier (`"UserService"`, `"tu"`, `"*"`)
    // while `cleaned_module` has the full module path (`"acme_billing::user::UserService"`).
    if trimmed == "*" {
        return Some("*".to_string());
    }
    if !trimmed.is_empty()
        && trimmed != cleaned_module
        && !trimmed.contains(['/', '.', ':', ' ', ';', '"', '\''])
    {
        return Some(trimmed.to_string());
    }
    None
}

fn relation_allows_external_stub(relation: &Relation) -> bool {
    match relation.relation_type {
        RelationType::Instantiates
        | RelationType::DependsOn
        | RelationType::Uses
        | RelationType::AnnotatedWith
        | RelationType::Extends
        | RelationType::Implements
        | RelationType::Permits
        | RelationType::UsesVariable
        | RelationType::UsesFact
        | RelationType::ExposesRoute
        | RelationType::Injects => true,
        // Bare field `References` (no type hint / type metadata) must not invent
        // Class stubs that pollute suffix resolution (#125). Typed References
        // (throws, class literals, Puppet resources, explicit type hints) still stub.
        RelationType::References => references_allows_external_stub(relation),
        _ => false,
    }
}

fn references_allows_external_stub(relation: &Relation) -> bool {
    if relation
        .metadata
        .get("throws")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || relation
            .metadata
            .get("class_literal")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    {
        return true;
    }
    matches!(
        relation
            .to_type_hint
            .as_deref()
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("puppetresource")
            | Some("puppetclass")
            | Some("class")
            | Some("interface")
            | Some("enum")
            | Some("annotation")
            | Some("module")
            | Some("struct")
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
            "route" => return NodeType::Function,
            _ => {}
        }
    }
    match relation.relation_type {
        RelationType::DependsOn => NodeType::Module,
        RelationType::AnnotatedWith => NodeType::Annotation,
        RelationType::Calls | RelationType::ExposesRoute => NodeType::Function,
        RelationType::Implements => NodeType::Interface,
        RelationType::Injects => NodeType::Class,
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
        RelationType::ExposesRoute => EdgeType::ExposesRoute,
        RelationType::Injects => EdgeType::Injects,
    }
}

fn go_simple_type_from_str(ty: &str) -> String {
    ty.trim_start_matches('*')
        .rsplit(['.', '/'])
        .next()
        .unwrap_or(ty)
        .to_string()
}

pub(crate) fn is_inheritance_relation(rt: RelationType) -> bool {
    matches!(
        rt,
        RelationType::Extends
            | RelationType::Implements
            | RelationType::IncludesClass
            | RelationType::InheritsClass
    )
}

fn compute_mro_chain(type_name: &str, bases: &HashMap<String, Vec<(String, bool)>>) -> Vec<String> {
    let key = type_simple_name(type_name);
    let mut visiting = std::collections::HashSet::new();
    let mut memo: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(c3) = compute_c3_mro(key, bases, &mut visiting, &mut memo) {
        return c3;
    }

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    fn walk(
        name: &str,
        bases: &HashMap<String, Vec<(String, bool)>>,
        out: &mut Vec<String>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        if !seen.insert(name.to_string()) {
            return;
        }
        out.push(name.to_string());
        let Some(direct) = bases.get(name) else {
            return;
        };
        let (classes, ifaces): (Vec<_>, Vec<_>) =
            direct.iter().cloned().partition(|(_, iface)| !*iface);
        for (base, _) in classes.into_iter().chain(ifaces) {
            walk(&base, bases, out, seen);
        }
    }
    walk(key, bases, &mut out, &mut seen);
    out
}

fn compute_c3_mro(
    name: &str,
    bases: &HashMap<String, Vec<(String, bool)>>,
    visiting: &mut std::collections::HashSet<String>,
    memo: &mut HashMap<String, Vec<String>>,
) -> Option<Vec<String>> {
    if let Some(cached) = memo.get(name) {
        return Some(cached.clone());
    }
    if !visiting.insert(name.to_string()) {
        return None;
    }
    let Some(direct) = bases.get(name) else {
        visiting.remove(name);
        let single = vec![name.to_string()];
        memo.insert(name.to_string(), single.clone());
        return Some(single);
    };
    let (classes, ifaces): (Vec<_>, Vec<_>) =
        direct.iter().cloned().partition(|(_, iface)| !*iface);
    let ordered_parents: Vec<String> = classes.into_iter().chain(ifaces).map(|(p, _)| p).collect();
    if ordered_parents.is_empty() {
        visiting.remove(name);
        let single = vec![name.to_string()];
        memo.insert(name.to_string(), single.clone());
        return Some(single);
    }

    let mut sequences: Vec<std::collections::VecDeque<String>> =
        Vec::with_capacity(ordered_parents.len() + 1);
    for parent in &ordered_parents {
        let parent_mro = compute_c3_mro(parent, bases, visiting, memo)?;
        sequences.push(parent_mro.into_iter().collect());
    }
    sequences.push(ordered_parents.into_iter().collect());

    let mut result = vec![name.to_string()];
    loop {
        sequences.retain(|seq| !seq.is_empty());
        if sequences.is_empty() {
            break;
        }
        let mut chosen: Option<String> = None;
        for seq in &sequences {
            let candidate = &seq[0];
            let in_any_tail = sequences
                .iter()
                .any(|other| other.iter().skip(1).any(|item| item == candidate));
            if !in_any_tail {
                chosen = Some(candidate.clone());
                break;
            }
        }
        let head = chosen?;
        result.push(head.clone());
        for seq in &mut sequences {
            if seq.front() == Some(&head) {
                seq.pop_front();
            }
        }
    }

    visiting.remove(name);
    memo.insert(name.to_string(), result.clone());
    Some(result)
}

/// Rightmost type segment: `com.foo.Child` → `Child`, `pkg::Child` → `Child`.
fn type_simple_name(name: &str) -> &str {
    name.split('<')
        .next()
        .unwrap_or(name)
        .rsplit(['.', '/', ':'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(name)
}

/// Split `Owner.method` / `Owner::method` / Ruby `Owner#method`.
fn split_owner_method(qn: &str) -> Option<(&str, &str)> {
    if let Some((owner, method)) = qn.rsplit_once("::")
        && !owner.is_empty()
        && !method.is_empty()
    {
        let method = method.split('<').next().unwrap_or(method);
        let owner = owner.split('<').next().unwrap_or(owner);
        return Some((owner, method));
    }
    if let Some((owner, method)) = qn.rsplit_once('#')
        && !owner.is_empty()
        && !method.is_empty()
    {
        return Some((owner, method));
    }
    if let Some((owner, method)) = qn.rsplit_once('.')
        && !owner.is_empty()
        && !method.is_empty()
    {
        return Some((owner, method));
    }
    None
}

fn method_simple_name<'a>(name: &'a str, qualified_hint: Option<&'a str>) -> &'a str {
    if let Some(hint) = qualified_hint
        && let Some((_, method)) = split_owner_method(hint)
    {
        return method;
    }
    name.rsplit(['.', ':', '#'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(name)
}

fn is_super_call(name: &str, qualified_hint: Option<&str>, type_name: &str) -> bool {
    if type_name.eq_ignore_ascii_case("super") {
        return true;
    }
    let probe = qualified_hint.unwrap_or(name);
    probe.starts_with("super.")
        || probe.starts_with("super::")
        || probe.starts_with("super#")
        || name == "super"
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
            ..Default::default()
        }
    }

    #[test]
    fn java_overloaded_callers_and_literal_targets_are_independent() {
        let registry = std::sync::Arc::new(rgctl_languages::default_registry());
        let extractor = crate::Extractor::new(registry);
        let source = br#"package demo;
class Library {
 static int intHelper() { return 1; }
 static int stringHelper() { return 2; }
 static int quote(int value) { return intHelper(); }
 static int quote(String value) { return stringHelper(); }
 static int runInt() { return quote(7); }
 static int runString() { return quote("hello"); }
}"#;
        let extraction = extractor
            .extract_file_with_source(Path::new("Library.java"), source.to_vec())
            .unwrap();
        let mut builder = GraphBuilder::new();
        extractor
            .populate_graph(&[extraction], &mut builder)
            .unwrap();
        let (nodes, edges) = builder.into_graph();
        let find = |name: &str, identity: Option<&str>| {
            nodes
                .iter()
                .find(|n| {
                    n.name == name && identity.is_none_or(|i| n.identity_discriminator() == Some(i))
                })
                .unwrap()
                .id
        };
        let calls = |from, to| {
            edges
                .iter()
                .any(|e| e.from == from && e.to == to && e.edge_type == EdgeType::Calls)
        };
        let int = find("quote", Some("(int)"));
        let string = find("quote", Some("(String)"));
        assert!(calls(int, find("intHelper", None)));
        assert!(calls(string, find("stringHelper", None)));
        assert!(!calls(int, find("stringHelper", None)));
        assert!(calls(find("runInt", None), int));
        assert!(!calls(find("runInt", None), string));
        assert!(calls(find("runString", None), string));
    }

    #[test]
    fn java_overloads_survive_snapshots_body_changes_line_shifts_and_removal() {
        use crate::Extractor;
        use rgctl_graph::{
            SnapshotNodeStore, stable_key_from_node, stable_key_from_row,
            write_columnar_from_nodes_edges,
        };
        let registry = std::sync::Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let baseline = include_bytes!(
            "../../../tests/fixtures/tier1_identity_dependencies/java/dependencies/library/baseline/demo/Library.java"
        );
        let candidate = include_bytes!(
            "../../../tests/fixtures/tier1_identity_dependencies/java/dependencies/library/candidate/demo/Library.java"
        );
        let build = |source: &[u8]| {
            let extraction = extractor
                .extract_file_with_source(Path::new("Library.java"), source.to_vec())
                .unwrap();
            let mut builder = GraphBuilder::new();
            extractor
                .populate_graph(&[extraction], &mut builder)
                .unwrap();
            let (nodes, edges) = builder.into_graph();
            let temporary = tempfile::tempdir().unwrap();
            let path = temporary.path().join("graph.snapshot.bin");
            write_columnar_from_nodes_edges(nodes.clone(), edges, &path).unwrap();
            let store = SnapshotNodeStore::open(&path).unwrap();
            let col = store.columnar().unwrap();
            for index in 0..col.node_count() {
                let node = col.materialize_node_at(index).unwrap();
                assert_eq!(
                    stable_key_from_row(col, index).unwrap(),
                    stable_key_from_node(&node)
                );
            }
            nodes
                .into_iter()
                .filter(|node| node.name == "quote")
                .map(|node| {
                    let restored = store.get_node(node.id).unwrap().unwrap();
                    assert_eq!(
                        restored.identity_discriminator(),
                        node.identity_discriminator()
                    );
                    (
                        node.identity_discriminator().unwrap().to_owned(),
                        (node.id, node.code_hash.clone()),
                    )
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let before = build(baseline);
        let after = build(candidate);
        assert_eq!(before.len(), 2);
        assert_eq!(after.len(), 2);
        assert_ne!(before["(int)"].0, before["(String)"].0);
        assert_eq!(before["(int)"], after["(int)"]);
        assert_eq!(before["(String)"].0, after["(String)"].0);
        assert_ne!(before["(String)"].1, after["(String)"].1);
        let shifted = format!("\n\n{}", std::str::from_utf8(baseline).unwrap());
        assert_eq!(before, build(shifted.as_bytes()));
        let removed = std::str::from_utf8(baseline).unwrap().replace(
            "    public static int quote(String value) {\n        return value.length();\n    }\n",
            "",
        );
        let remaining = build(removed.as_bytes());
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining["(int)"], before["(int)"]);
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
            ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
        };
        let calls = Relation {
            from: "AuthService.register".to_string(),
            to: "email".to_string(),
            relation_type: RelationType::Calls,
            location: auth_loc(3),
            metadata: serde_json::json!({ "language": "java" }),
            to_qualified_hint: None,
            to_type_hint: None,
            ..Default::default()
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
                    ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
                    ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
        // #125: bare field-like References (no type hint / type metadata) must not stub.
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
            ..Default::default()
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
                ..Default::default()
            })
            .unwrap();
        assert_eq!(builder.node_count(), before);
        assert!(
            !builder.nodes().iter().any(|n| {
                n.name == "ArrayList"
                    && n.node_type == NodeType::Class
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            }),
            "bare References must not invent Class stubs"
        );
    }

    #[test]
    fn references_throws_still_stubs_external_exception() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("Demo.java"));
        builder.add_symbol(
            &Symbol {
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
                ..Default::default()
            },
            file_id,
        );
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "Demo.run".to_string(),
                to: "IOException".to_string(),
                relation_type: RelationType::References,
                location: SourceLocation {
                    file: "Demo.java".to_string(),
                    start_line: 2,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java", "throws": true }),
                to_qualified_hint: Some("java.io.IOException".to_string()),
                to_type_hint: None,
                ..Default::default()
            })
            .unwrap();
        assert!(
            builder.nodes().iter().any(|n| {
                n.qualified_name.as_deref() == Some("java.io.IOException")
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            }),
            "throws References must still create external stubs"
        );
        assert!(
            !builder.symbols_by_suffix.contains_key("IOException"),
            "external stubs must not enter symbols_by_suffix"
        );
    }

    #[test]
    fn references_puppetresource_hint_still_stubs() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("site.pp"));
        builder.add_symbol(
            &Symbol {
                name: "main".to_string(),
                symbol_type: SymbolType::Function,
                qualified_name: Some("main".to_string()),
                location: SourceLocation {
                    file: "site.pp".to_string(),
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
                metadata: serde_json::json!({ "language": "puppet" }),
                ..Default::default()
            },
            file_id,
        );
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "main".to_string(),
                to: "nginx".to_string(),
                relation_type: RelationType::References,
                location: SourceLocation {
                    file: "site.pp".to_string(),
                    start_line: 2,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "puppet" }),
                to_qualified_hint: Some("Package[nginx]".to_string()),
                to_type_hint: Some("puppetresource".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert!(
            builder.nodes().iter().any(|n| {
                n.node_type == NodeType::PuppetResource
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            }),
            "PuppetResource References must still stub"
        );
    }

    #[test]
    fn external_stubs_with_distinct_qualified_hints_do_not_coalesce() {
        let mut builder = GraphBuilder::new();
        let a = builder.ensure_file_node(Path::new("FileA.java"));
        let b = builder.ensure_file_node(Path::new("FileB.java"));
        builder.add_symbol(&function_symbol("FileA.java", "a", "FileA.a"), a);
        builder.add_symbol(&function_symbol("FileB.java", "b", "FileB.b"), b);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "FileA.a".to_string(),
                to: "Builder".to_string(),
                relation_type: RelationType::Instantiates,
                location: SourceLocation {
                    file: "FileA.java".to_string(),
                    start_line: 2,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: Some("com.foo.Builder".to_string()),
                to_type_hint: None,
                ..Default::default()
            })
            .unwrap();
        builder
            .add_relation(&Relation {
                from: "FileB.b".to_string(),
                to: "Builder".to_string(),
                relation_type: RelationType::Instantiates,
                location: SourceLocation {
                    file: "FileB.java".to_string(),
                    start_line: 2,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint: Some("org.bar.Builder".to_string()),
                to_type_hint: None,
                ..Default::default()
            })
            .unwrap();
        let stubs: Vec<_> = builder
            .nodes()
            .iter()
            .filter(|n| {
                n.name == "Builder"
                    && n.properties.get("is_external_stub").map(String::as_str) == Some("true")
            })
            .map(|n| n.qualified_name.clone())
            .collect();
        assert_eq!(
            stubs.len(),
            2,
            "expected distinct external Builders, got {stubs:?}"
        );
        assert!(
            stubs
                .iter()
                .any(|q| q.as_deref() == Some("com.foo.Builder"))
        );
        assert!(
            stubs
                .iter()
                .any(|q| q.as_deref() == Some("org.bar.Builder"))
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
            source_span: None,
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
            ..Default::default()
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
                source_span: None,
                name: "total".to_string(),
                field_type: Some("int".to_string()),
                visibility: None,
            }],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
            ..Default::default()
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
                source_span: None,
                name: "total".to_string(),
                field_type: Some("int".to_string()),
                visibility: None,
            }],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
            ..Default::default()
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

    fn type_symbol(file: &str, name: &str, st: SymbolType) -> Symbol {
        Symbol {
            name: name.to_string(),
            symbol_type: st,
            location: SourceLocation {
                file: file.to_string(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 1,
            },
            qualified_name: Some(name.to_string()),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
            ..Default::default()
        }
    }

    #[test]
    fn mro_resolves_inherited_call_and_emits_overrides() {
        let mut builder = GraphBuilder::new();
        let base_file = builder.ensure_file_node(Path::new("base.py"));
        let child_file = builder.ensure_file_node(Path::new("child.py"));
        builder.add_symbol(
            &type_symbol("base.py", "Base", SymbolType::Class),
            base_file,
        );
        builder.add_symbol(&function_symbol("base.py", "run", "Base.run"), base_file);
        builder.add_symbol(
            &type_symbol("child.py", "Child", SymbolType::Class),
            child_file,
        );
        // Child overrides run — should emit Overrides → Base.run
        let child_run =
            builder.add_symbol(&function_symbol("child.py", "run", "Child.run"), child_file);
        let caller = builder.add_symbol(&function_symbol("child.py", "main", "main"), child_file);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "Child".to_string(),
                to: "Base".to_string(),
                relation_type: RelationType::Extends,
                location: SourceLocation {
                    file: "child.py".to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "python" }),
                to_qualified_hint: Some("Base".to_string()),
                to_type_hint: Some("class".to_string()),
                ..Default::default()
            })
            .unwrap();
        builder.emit_mro_overrides();
        let (_, edges) = builder.into_graph();
        assert!(
            edges
                .iter()
                .any(|e| { e.edge_type == EdgeType::Overrides && e.from == child_run }),
            "expected Overrides from Child.run, edges={edges:?}"
        );
        let _ = caller;
    }

    #[test]
    fn mro_inherited_call_targets_ancestor_when_child_has_no_method() {
        let mut builder = GraphBuilder::new();
        let base_file = builder.ensure_file_node(Path::new("base.py"));
        let child_file = builder.ensure_file_node(Path::new("child.py"));
        builder.add_symbol(
            &type_symbol("base.py", "Base", SymbolType::Class),
            base_file,
        );
        let base_run =
            builder.add_symbol(&function_symbol("base.py", "run", "Base.run"), base_file);
        builder.add_symbol(
            &type_symbol("child.py", "Child", SymbolType::Class),
            child_file,
        );
        builder.add_symbol(
            &function_symbol("child.py", "entry", "Child.entry"),
            child_file,
        );
        builder.build_resolution_indexes();
        // Inheritance before Calls (mirrors populate_pass2 ordering).
        builder
            .add_relation(&Relation {
                from: "Child".to_string(),
                to: "Base".to_string(),
                relation_type: RelationType::Extends,
                location: SourceLocation {
                    file: "child.py".to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "python" }),
                to_qualified_hint: Some("Base".to_string()),
                to_type_hint: Some("class".to_string()),
                ..Default::default()
            })
            .unwrap();
        builder
            .add_relation(&Relation {
                from: "Child.entry".to_string(),
                to: "Child.run".to_string(),
                relation_type: RelationType::Calls,
                location: SourceLocation {
                    file: "child.py".to_string(),
                    start_line: 5,
                    end_line: 5,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "python" }),
                to_qualified_hint: Some("Child.run".to_string()),
                to_type_hint: Some("Child".to_string()),
                ..Default::default()
            })
            .unwrap();
        let (_, edges) = builder.into_graph();
        assert!(
            edges
                .iter()
                .any(|e| { e.edge_type == EdgeType::Calls && e.to == base_run }),
            "inherited Child.run call must resolve to Base.run, edges={edges:?}"
        );
    }

    #[test]
    fn mro_ruby_hash_qn_resolves_via_include() {
        let mut builder = GraphBuilder::new();
        let mod_file = builder.ensure_file_node(Path::new("walkable.rb"));
        let dog_file = builder.ensure_file_node(Path::new("dog.rb"));
        builder.add_symbol(
            &type_symbol("walkable.rb", "Walkable", SymbolType::Module),
            mod_file,
        );
        let bark = builder.add_symbol(
            &Symbol {
                name: "bark".to_string(),
                symbol_type: SymbolType::Method,
                location: SourceLocation {
                    file: "walkable.rb".to_string(),
                    start_line: 2,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                qualified_name: Some("Walkable#bark".to_string()),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: serde_json::json!({}),
                ..Default::default()
            },
            mod_file,
        );
        builder.add_symbol(&type_symbol("dog.rb", "Dog", SymbolType::Class), dog_file);
        builder.add_symbol(&function_symbol("dog.rb", "speak", "Dog#speak"), dog_file);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "Dog".to_string(),
                to: "Walkable".to_string(),
                relation_type: RelationType::IncludesClass,
                location: SourceLocation {
                    file: "dog.rb".to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "ruby" }),
                to_qualified_hint: Some("Walkable".to_string()),
                to_type_hint: Some("module".to_string()),
                ..Default::default()
            })
            .unwrap();
        builder
            .add_relation(&Relation {
                from: "Dog#speak".to_string(),
                to: "Dog#bark".to_string(),
                relation_type: RelationType::Calls,
                location: SourceLocation {
                    file: "dog.rb".to_string(),
                    start_line: 4,
                    end_line: 4,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({ "language": "ruby" }),
                to_qualified_hint: Some("Dog#bark".to_string()),
                to_type_hint: Some("Dog".to_string()),
                ..Default::default()
            })
            .unwrap();
        let (_, edges) = builder.into_graph();
        assert!(
            edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Calls && e.to == bark),
            "Dog#bark via include must resolve to Walkable#bark, edges={edges:?}"
        );
    }

    #[test]
    fn exposes_route_preserves_full_route_name_not_brace_segment() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("users.py"));
        let handler = Symbol {
            name: "get_user".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name: Some("get_user".to_string()),
            location: SourceLocation {
                file: "users.py".to_string(),
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
            metadata: serde_json::json!({ "language": "python" }),
            ..Default::default()
        };
        builder.add_symbol(&handler, file_id);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "get_user".to_string(),
                to: "GET /api/users/{id}".to_string(),
                relation_type: RelationType::ExposesRoute,
                location: SourceLocation {
                    file: "users.py".to_string(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({
                    "http_method": "GET",
                    "route_path": "/api/users/{id}",
                    "framework": "fastapi",
                    "rule_id": "fastapi.flask.route_decorator"
                }),
                to_qualified_hint: Some("route::GET /api/users/{id}".to_string()),
                to_type_hint: Some("route".to_string()),
                ..Default::default()
            })
            .unwrap();

        let route = builder
            .nodes()
            .iter()
            .find(|n| n.name == "GET /api/users/{id}")
            .expect("route node must keep full METHOD /path name, not {id}");
        assert_ne!(route.name, "{id}");
        assert!(
            builder
                .edges
                .iter()
                .any(|e| { e.edge_type == EdgeType::ExposesRoute && e.to == route.id }),
            "ExposesRoute edge missing"
        );
    }

    #[test]
    fn injects_links_to_in_repo_class_and_appends_target_vocab() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("OrderController.java"));
        let controller = Symbol {
            name: "OrderController".to_string(),
            symbol_type: SymbolType::Class,
            qualified_name: Some("OrderController".to_string()),
            location: SourceLocation {
                file: "OrderController.java".to_string(),
                start_line: 1,
                end_line: 20,
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
            ..Default::default()
        };
        let service = Symbol {
            name: "OrderService".to_string(),
            symbol_type: SymbolType::Class,
            qualified_name: Some("OrderService".to_string()),
            location: SourceLocation {
                file: "OrderController.java".to_string(),
                start_line: 22,
                end_line: 40,
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
            ..Default::default()
        };
        builder.add_symbol(&controller, file_id);
        builder.add_symbol(&service, file_id);
        builder.build_resolution_indexes();
        builder
            .add_relation(&Relation {
                from: "OrderController".to_string(),
                to: "OrderService".to_string(),
                relation_type: RelationType::Injects,
                location: SourceLocation {
                    file: "OrderController.java".to_string(),
                    start_line: 5,
                    end_line: 5,
                    start_column: 0,
                    end_column: 1,
                },
                metadata: serde_json::json!({
                    "rule_id": "spring.jakarta.field_di",
                    "target_vocab": "dependency injection provider service"
                }),
                to_qualified_hint: None,
                to_type_hint: Some("class".to_string()),
                ..Default::default()
            })
            .unwrap();

        assert!(
            builder
                .edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Injects),
            "Injects edge missing"
        );
        let svc = builder
            .nodes()
            .iter()
            .find(|n| n.name == "OrderService")
            .expect("OrderService");
        assert!(svc.properties.get("is_external_stub").map(String::as_str) != Some("true"));
        let tags = svc
            .properties
            .get("semantic_tags")
            .map(String::as_str)
            .unwrap_or("");
        assert!(
            tags.contains("dependency") && tags.contains("service"),
            "target_vocab should land on in-repo Injects target, got {tags:?}"
        );
    }
    #[test]
    fn materialized_fields_keep_qualified_types_and_byte_spans() {
        let mut builder = GraphBuilder::new();
        builder.set_materialize_fields(true);
        let file = builder.ensure_file_node(Path::new("Order.java"));
        let mut symbol = sample_symbol();
        symbol.name = "Order".into();
        symbol.symbol_type = SymbolType::Class;
        symbol.fields = vec![rgctl_plugin_api::Field {
            source_span: None,
            name: "status".into(),
            field_type: Some("Status".into()),
            visibility: None,
        }];
        symbol.fields[0].source_span = Some(rgctl_plugin_api::FieldSourceSpan {
            start_byte: 100,
            end_byte: 120,
            name_rel_offset: Some(4),
            name_token_len: Some(6),
            name_span_overflow: None,
            start_line: 10,
            end_line: 10,
        });
        symbol.metadata = serde_json::json!({"field_type_fqns":{"status":"demo.Status"}});
        builder.add_symbol(&symbol, file);
        let field = builder
            .nodes()
            .iter()
            .find(|n| n.name == "status")
            .expect("field");
        assert_eq!(field.get_property("field_type_fqn"), Some("demo.Status"));
        assert_eq!(field.start_line, Some(10));
        assert_eq!(field.start_byte, Some(100));
        assert_eq!(field.end_byte, Some(120));
        assert_eq!(field.name_rel_offset, Some(4));
        assert_eq!(field.name_token_len, Some(6));
    }
}
