//! Maps extracted symbols and relations into graph nodes and edges.

use rbuilder_error::Result;
use rbuilder_graph::code_index::{hash_code, CodeIndex};
use rbuilder_graph::migration::graph_parameter_from_plugin;
use rbuilder_graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder_plugin_api::{
    ComplexityMetrics, ConfigKey, Relation, RelationType, Symbol, SymbolType,
};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// Builds graph nodes and edges from extracted data.
#[derive(Debug)]
pub struct GraphBuilder {
    symbol_index: HashMap<String, Uuid>,
    file_nodes: HashMap<String, Uuid>,
    config_key_nodes: HashMap<String, Uuid>,
    env_nodes: HashMap<String, Uuid>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    code_index: Option<CodeIndex>,
    // Resolution performance tracking
    resolution_stats: ResolutionStats,
    // Fast resolution indexes (built on demand)
    symbols_by_qualified: HashMap<String, Uuid>,
    symbols_by_suffix: HashMap<String, Vec<Uuid>>,
    indexes_built: bool,
}

#[derive(Debug, Default)]
struct ResolutionStats {
    total_calls: usize,
    hashmap_hits: usize,
    qualified_hint_scans: usize,
    qualified_hint_hits: usize,      // O(1) index hits
    type_hint_scans: usize,
    type_hint_hits: usize,           // O(1) index hits
    fuzzy_scans: usize,
    fuzzy_hits: usize,               // O(1) index hits
    total_time: std::time::Duration,
    line_lookups: usize,
    line_lookup_time: std::time::Duration,
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self {
            symbol_index: HashMap::default(),
            file_nodes: HashMap::default(),
            config_key_nodes: HashMap::default(),
            env_nodes: HashMap::default(),
            nodes: Vec::default(),
            edges: Vec::default(),
            code_index: None,
            resolution_stats: ResolutionStats::default(),
            symbols_by_qualified: HashMap::default(),
            symbols_by_suffix: HashMap::default(),
            indexes_built: false,
        }
    }
}

impl GraphBuilder {
    /// Create an empty graph builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of nodes built so far.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges built so far.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Ensure a file node exists and return its ID.
    pub fn ensure_file_node(&mut self, path: &Path) -> Uuid {
        let file_path = path.to_string_lossy().to_string();
        if let Some(id) = self.file_nodes.get(&file_path) {
            return *id;
        }

        let node = Node::new(NodeType::File, file_path.clone()).with_file_path(file_path.clone());
        let id = node.id;
        self.file_nodes.insert(file_path, id);
        self.nodes.push(node);
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

    /// Add a symbol node linked to its file.
    pub fn add_symbol(&mut self, symbol: &Symbol, file_id: Uuid) -> Uuid {
        self.add_symbol_with_body(symbol, file_id, None)
    }

    /// Add a symbol node and optionally hash its body for change detection.
    pub fn add_symbol_with_body(
        &mut self,
        symbol: &Symbol,
        file_id: Uuid,
        body: Option<&str>,
    ) -> Uuid {
        let key = symbol_key(
            &symbol.location.file,
            &symbol.name,
            symbol.qualified_name.as_deref(),
        );
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
        if let Some(body) = body {
            let code_hash = if let Some(index) = self.code_index.as_mut() {
                index.add_code(body, &symbol.location)
            } else {
                hash_code(body)
            };
            node = node.with_code_hash(code_hash);
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
        self.symbol_index.insert(key, id);
        self.nodes.push(node);
        self.add_edge(id, file_id, EdgeType::DefinedIn);
        self.add_edge(file_id, id, EdgeType::Contains);
        id
    }

    /// Attach complexity metrics to an existing symbol node.
    pub fn add_complexity(&mut self, symbol: &Symbol, metrics: &ComplexityMetrics) {
        let key = symbol_key(
            &symbol.location.file,
            &symbol.name,
            symbol.qualified_name.as_deref(),
        );
        if let Some(id) = self.symbol_index.get(&key) {
            if let Some(node) = self.nodes.iter_mut().find(|n| n.id == *id) {
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
    }

    /// Build reverse indexes for fast symbol resolution.
    ///
    /// Call this after all symbols are added but before processing relations.
    /// This converts O(n) linear scans into O(1) HashMap lookups.
    pub fn build_resolution_indexes(&mut self) {
        use std::time::Instant;
        use tracing::info;

        if self.indexes_built {
            return; // Already built
        }

        let start = Instant::now();
        let symbol_count = self.symbol_index.len();

        // Build UUID → Node index for O(1) lookups (eliminates O(n²) nested loop)
        let uuid_to_node: HashMap<Uuid, &Node> = self.nodes.iter()
            .map(|n| (n.id, n))
            .collect();

        // Build qualified name index and suffix index
        for (key, uuid) in &self.symbol_index {
            // Find the node to get its qualified_name (now O(1) instead of O(n))
            if let Some(node) = uuid_to_node.get(uuid) {
                // Index by qualified_name if present
                if let Some(qualified) = &node.qualified_name {
                    self.symbols_by_qualified.insert(qualified.clone(), *uuid);
                }
            }

            // Build suffix index for "ends_with" queries
            // Split "file.rs::Module::function" into suffixes:
            // - "Module::function"
            // - "function"
            let parts: Vec<&str> = key.split("::").collect();
            for i in 1..parts.len() {
                let suffix = parts[i..].join("::");
                self.symbols_by_suffix
                    .entry(suffix)
                    .or_default()
                    .push(*uuid);
            }
        }

        self.indexes_built = true;

        info!(
            symbol_count,
            qualified_count = self.symbols_by_qualified.len(),
            suffix_count = self.symbols_by_suffix.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "built resolution indexes"
        );
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
        self.nodes.push(node);
        self.add_edge(id, file_id, EdgeType::DefinedIn);
        self.add_edge(file_id, id, EdgeType::Contains);
        id
    }

    /// Add a relation between symbols if both endpoints exist.
    pub fn add_relation(&mut self, relation: &Relation) -> Result<()> {
        let from_id = self.resolve_symbol_tracked(&relation.from, &relation.location.file, None, None);

        // Use hints for cross-file resolution (best-effort)
        // Hints are language plugin's best guess at the qualified name based on local context
        let to_id = self.resolve_symbol_tracked(
            &relation.to,
            &relation.location.file,
            relation.to_qualified_hint.as_deref(),
            relation.to_type_hint.as_deref(),
        );

        if let (Some(from), Some(to)) = (from_id, to_id) {
            self.add_edge(from, to, relation_type_to_edge_type(relation.relation_type));
        }
        Ok(())
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
        let code_node = self.find_symbol_at_line_tracked(file_path, line).or(file_id);

        let Some(from_id) = code_node else {
            return;
        };

        let target_id = match usage_type {
            ConfigUsageKind::EnvVar => self.ensure_env_node(key),
            ConfigUsageKind::ConfigKey => self.ensure_config_key_node(key, file_path),
        };

        self.add_edge(from_id, target_id, EdgeType::UsesConfig);
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
        self.nodes.push(node);
        id
    }

    fn ensure_config_key_node(&mut self, key: &str, file_path: &str) -> Uuid {
        let lookup = format!("{file_path}::{key}");
        if let Some(id) = self.config_key_nodes.get(&lookup) {
            return *id;
        }

        let node =
            Node::new(NodeType::ConfigKey, key.to_string()).with_file_path(file_path.to_string());
        let id = node.id;
        self.config_key_nodes.insert(lookup, id);
        self.nodes.push(node);
        id
    }

    fn find_symbol_at_line_tracked(&mut self, file_path: &str, line: usize) -> Option<Uuid> {
        use std::time::Instant;

        let start = Instant::now();
        self.resolution_stats.line_lookups += 1;

        let result = self.nodes
            .iter()
            .filter(|n| n.file_path.as_deref() == Some(file_path))
            .filter(|n| {
                n.start_line
                    .map(|start| start <= line && n.end_line.unwrap_or(start) >= line)
                    .unwrap_or(false)
            })
            .max_by_key(|n| n.start_line.unwrap_or(0))
            .map(|n| n.id);

        self.resolution_stats.line_lookup_time += start.elapsed();
        result
    }

    #[allow(dead_code)]
    fn find_symbol_at_line(&self, file_path: &str, line: usize) -> Option<Uuid> {
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

        // 2. Try qualified hint direct lookup (O(1))
        if let Some(hint) = qualified_hint {
            self.resolution_stats.qualified_hint_scans += 1;

            // First try direct qualified name lookup
            if let Some(id) = self.symbols_by_qualified.get(hint) {
                self.resolution_stats.qualified_hint_hits += 1;
                self.resolution_stats.total_time += start.elapsed();
                return Some(*id);
            }

            // Then try suffix index
            if let Some(ids) = self.symbols_by_suffix.get(hint) {
                if let Some(id) = ids.first() {
                    self.resolution_stats.qualified_hint_hits += 1;
                    self.resolution_stats.total_time += start.elapsed();
                    return Some(*id);
                }
            }
        }

        // 3. Try type hint + simple name (O(1))
        if let Some(type_name) = type_hint {
            self.resolution_stats.type_hint_scans += 1;
            // Extract simple name from qualified name if needed
            let simple_name = name.split('.').next_back().unwrap_or(name);
            let type_qualified = format!("{type_name}.{simple_name}");

            // Try suffix index lookup
            if let Some(ids) = self.symbols_by_suffix.get(&type_qualified) {
                if let Some(id) = ids.first() {
                    self.resolution_stats.type_hint_hits += 1;
                    self.resolution_stats.total_time += start.elapsed();
                    return Some(*id);
                }
            }
        }

        // 4. Fallback: suffix index lookup (O(1))
        self.resolution_stats.fuzzy_scans += 1;
        let result = self.symbols_by_suffix
            .get(name)
            .and_then(|ids| ids.first())
            .copied();

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
    /// 2. If qualified_hint provided, try: `*::{qualified_hint}` (e.g., "Helper.transform")
    /// 3. If type_hint provided, try: `*::{type_hint}.{simple_name}` (e.g., "Helper.transform")
    /// 4. Fallback to fuzzy match: any key ending with `::{name}`
    ///
    /// Hints are best-effort guesses from language plugins based on local context
    /// (variable types, field declarations, etc.) and may not always be accurate.
    #[allow(dead_code)]
    fn resolve_symbol(
        &self,
        name: &str,
        file: &str,
        qualified_hint: Option<&str>,
        type_hint: Option<&str>,
    ) -> Option<Uuid> {
        // 1. Try exact match in current file
        let qualified = format!("{file}::{name}");
        if let Some(id) = self.symbol_index.get(&qualified) {
            return Some(*id);
        }

        // 2. Try qualified hint (e.g., "Helper.transform")
        if let Some(hint) = qualified_hint {
            // Look for any key ending with the hint
            let hint_suffix = format!("::{hint}");
            if let Some((_, id)) = self
                .symbol_index
                .iter()
                .find(|(k, _)| k.ends_with(&hint_suffix))
            {
                return Some(*id);
            }
        }

        // 3. Try type hint + simple name
        if let Some(type_name) = type_hint {
            // Extract simple name from qualified name if needed
            let simple_name = name.split('.').next_back().unwrap_or(name);
            let type_qualified = format!("{type_name}.{simple_name}");
            let hint_suffix = format!("::{type_qualified}");
            if let Some((_, id)) = self
                .symbol_index
                .iter()
                .find(|(k, _)| k.ends_with(&hint_suffix))
            {
                return Some(*id);
            }
        }

        // 4. Fallback: fuzzy match any key ending with the name
        let fuzzy_suffix = format!("::{name}");
        self.symbol_index
            .iter()
            .find(|(k, _)| k.ends_with(&fuzzy_suffix))
            .map(|(_, id)| *id)
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
        self.edges.push(Edge::new(from, to, edge_type));
    }

    /// Borrow built nodes (testing / inspection).
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Consume the builder and return all nodes and edges.
    pub fn into_graph(self) -> (Vec<Node>, Vec<Edge>) {
        (self.nodes, self.edges)
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

fn symbol_type_to_node_type(symbol_type: SymbolType) -> NodeType {
    match symbol_type {
        SymbolType::Function => NodeType::Function,
        SymbolType::Class => NodeType::Class,
        SymbolType::Struct => NodeType::Struct,
        SymbolType::Enum => NodeType::Enum,
        SymbolType::Interface => NodeType::Interface,
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
    }
}

fn relation_type_to_edge_type(relation_type: RelationType) -> EdgeType {
    match relation_type {
        RelationType::Calls => EdgeType::Calls,
        RelationType::Uses => EdgeType::Uses,
        RelationType::Implements => EdgeType::Implements,
        RelationType::Extends => EdgeType::Extends,
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

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_plugin_api::{ConfigValueType, SourceLocation, SymbolType};

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
    fn test_add_symbol_creates_file_and_symbol_nodes() {
        let mut builder = GraphBuilder::new();
        let file_id = builder.ensure_file_node(Path::new("src/main.rs"));
        builder.add_symbol(&sample_symbol(), file_id);

        assert_eq!(builder.node_count(), 2);
        assert_eq!(builder.edge_count(), 2);
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
        assert!(builder
            .edges
            .iter()
            .any(|e| e.edge_type == EdgeType::UsesConfig));
    }
}
