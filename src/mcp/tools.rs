//! MCP tool implementations for AI agent integration.

use crate::analysis::community::{CommunityDetector, CommunityResult};
use crate::analysis::complexity::{ComplexityAnalyzer, ComplexityReport};
use crate::analysis::dependency::DependencyAnalyzer;
use crate::analysis::graph_utils::PetGraphView;
use crate::config::analyzer::ConfigAnalyzer;
use crate::config::secret_detector::SecretDetector;
use crate::discovery::FileDiscoverer;
use crate::error::{Error, Result};
use crate::graph::backend::GraphBackend;
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::Node;
use crate::graph::CodeGraph;
use crate::incremental::file_tracker::{git_changed_files, FileTracker};
use crate::languages::registry::LanguageRegistry;
use crate::nlp::pattern_matcher::{PatternMatcher, QueryResult};
use crate::semantic::signature::SignatureExtractor;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// MCP tool descriptor for tools/list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for input parameters
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Cache for expensive analysis results.
#[derive(Debug, Clone)]
struct AnalysisCache {
    complexity: Option<(ComplexityReport, Instant)>,
    community: Option<(CommunityResult, Instant)>,
    ttl: Duration,
}

impl AnalysisCache {
    fn new(ttl_seconds: u64) -> Self {
        Self {
            complexity: None,
            community: None,
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    fn get_complexity(&mut self, backend: &MemoryBackend) -> Result<ComplexityReport> {
        if let Some((report, time)) = &self.complexity {
            if time.elapsed() < self.ttl {
                return Ok(report.clone());
            }
        }
        let report = ComplexityAnalyzer::analyze(backend)?;
        self.complexity = Some((report.clone(), Instant::now()));
        Ok(report)
    }

    fn get_community(&mut self, backend: &MemoryBackend) -> Result<CommunityResult> {
        if let Some((result, time)) = &self.community {
            if time.elapsed() < self.ttl {
                return Ok(result.clone());
            }
        }
        let result = CommunityDetector::new().detect(backend)?;
        self.community = Some((result.clone(), Instant::now()));
        Ok(result)
    }

    fn invalidate(&mut self) {
        self.complexity = None;
        self.community = None;
    }
}

/// Executes MCP tools against a loaded code graph.
pub struct ToolExecutor {
    repo_root: std::path::PathBuf,
    cache: Arc<Mutex<AnalysisCache>>,
}

impl ToolExecutor {
    /// Create a tool executor for a repository.
    pub fn new(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
            cache: Arc::new(Mutex::new(AnalysisCache::new(300))), // 5 minute TTL
        }
    }

    /// Create a tool executor with custom cache TTL.
    pub fn with_cache_ttl(repo_root: impl AsRef<Path>, ttl_seconds: u64) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
            cache: Arc::new(Mutex::new(AnalysisCache::new(ttl_seconds))),
        }
    }

    /// Invalidate the analysis cache.
    pub fn invalidate_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.invalidate();
        }
    }

    /// List all available MCP tools.
    pub fn list_tools() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "query_codebase".into(),
                description: "Query the codebase knowledge graph using natural language".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "question": { "type": "string", "description": "Natural language question" },
                        "include_verbose": { "type": "boolean", "description": "Include full node details" }
                    },
                    "required": ["question"]
                }),
            },
            ToolDefinition {
                name: "impact_analysis".into(),
                description: "Analyze what would break if a symbol is changed or deleted".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "Function, class, or module name" },
                        "depth": { "type": "integer", "description": "Traversal depth (default 3)" },
                        "include_verbose": { "type": "boolean" }
                    },
                    "required": ["symbol"]
                }),
            },
            ToolDefinition {
                name: "find_by_complexity".into(),
                description: "Find functions by complexity threshold and optional label filters".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "min_complexity": { "type": "integer", "description": "Minimum cyclomatic complexity" },
                        "labels": { "type": "array", "items": { "type": "string" } },
                        "include_verbose": { "type": "boolean" }
                    },
                    "required": ["min_complexity"]
                }),
            },
            ToolDefinition {
                name: "get_community_info".into(),
                description: "Get information about architectural communities/modules".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "community_name": { "type": "string", "description": "Filter by community name (optional)" },
                        "include_verbose": { "type": "boolean" }
                    }
                }),
            },
            ToolDefinition {
                name: "config_analysis".into(),
                description: "Analyze configuration: unused keys, missing env vars, or secrets".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "analysis_type": {
                            "type": "string",
                            "enum": ["unused_keys", "missing_env", "secrets"],
                            "description": "Type of config analysis"
                        },
                        "include_verbose": { "type": "boolean" }
                    },
                    "required": ["analysis_type"]
                }),
            },
            ToolDefinition {
                name: "symbol_info".into(),
                description: "Get detailed information about a function, class, or module".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "symbol_name": { "type": "string" },
                        "include_callers": { "type": "boolean", "default": true },
                        "include_dependencies": { "type": "boolean", "default": false },
                        "include_verbose": { "type": "boolean" }
                    },
                    "required": ["symbol_name"]
                }),
            },
            ToolDefinition {
                name: "diff_analysis".into(),
                description: "Analyze what changed since a git commit".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "since": { "type": "string", "description": "Git commit ref (default: last indexed commit)" },
                        "include_verbose": { "type": "boolean" }
                    }
                }),
            },
        ]
    }

    /// Execute a tool by name with JSON arguments.
    pub fn execute(&self, graph: &CodeGraph, name: &str, args: Value) -> Result<Value> {
        let backend = graph.backend();
        let verbose = args
            .get("include_verbose")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        match name {
            "query_codebase" => {
                let question = args
                    .get("question")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing question".into()))?;
                self.query_codebase(backend, question, verbose)
            }
            "impact_analysis" => {
                let symbol = args
                    .get("symbol")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing symbol".into()))?;
                let depth = args
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(3) as usize;
                self.impact_analysis(backend, symbol, depth, verbose)
            }
            "find_by_complexity" => {
                let min = args
                    .get("min_complexity")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| Error::InvalidQuery("Missing min_complexity".into()))?
                    as usize;
                let labels: Vec<String> = args
                    .get("labels")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                self.find_by_complexity(backend, min, &labels, verbose)
            }
            "get_community_info" => {
                let name_filter = args.get("community_name").and_then(|v| v.as_str());
                self.get_community_info(backend, name_filter, verbose)
            }
            "config_analysis" => {
                let analysis_type = args
                    .get("analysis_type")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing analysis_type".into()))?;
                self.config_analysis(backend, analysis_type, verbose)
            }
            "symbol_info" => {
                let symbol = args
                    .get("symbol_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing symbol_name".into()))?;
                let include_callers = args
                    .get("include_callers")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let include_deps = args
                    .get("include_dependencies")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.symbol_info(backend, symbol, include_callers, include_deps, verbose)
            }
            "diff_analysis" => {
                let since = args.get("since").and_then(|v| v.as_str());
                self.diff_analysis(since, verbose)
            }
            other => Err(Error::InvalidQuery(format!("Unknown tool: {other}"))),
        }
    }

    fn query_codebase(&self, backend: &MemoryBackend, question: &str, verbose: bool) -> Result<Value> {
        let matcher = PatternMatcher::from_graph(backend)?;
        let translated = matcher.translate(question)?;
        let result = matcher.execute(&translated, backend)?;

        let answer = format_query_result(&result);
        let mut response = json!({
            "answer": answer,
            "query": translated.internal_query,
            "confidence": translated.confidence,
            "intent": format!("{:?}", translated.intent),
        });

        if verbose {
            response["results"] = query_result_to_json(&result, true);
        } else {
            response["results"] = query_result_to_json(&result, false);
        }

        Ok(response)
    }

    fn impact_analysis(
        &self,
        backend: &MemoryBackend,
        symbol: &str,
        depth: usize,
        verbose: bool,
    ) -> Result<Value> {
        let (direct, indirect) = impact_at_depth(backend, symbol, depth)?;
        let total = direct.len() + indirect.len();

        let mut response = json!({
            "symbol": symbol,
            "depth": depth,
            "total_affected": total,
            "direct_dependencies": names_to_compact(&direct, verbose),
            "indirect_dependencies": names_to_compact(&indirect, verbose),
            "severity": if total > 20 { "high" } else if total > 5 { "medium" } else { "low" },
        });

        if !verbose {
            response["direct_dependencies"] = json!(direct.iter().take(10).collect::<Vec<_>>());
            response["indirect_dependencies"] = json!(indirect.iter().take(10).collect::<Vec<_>>());
        }

        Ok(response)
    }

    fn find_by_complexity(
        &self,
        backend: &MemoryBackend,
        min_complexity: usize,
        labels: &[String],
        verbose: bool,
    ) -> Result<Value> {
        let report = self.cache.lock().unwrap().get_complexity(backend)?;
        let mut matches: Vec<Value> = Vec::new();

        for fc in &report.functions {
            if fc.cyclomatic < min_complexity {
                continue;
            }
            if !labels.is_empty() && !labels.iter().any(|l| fc.node.has_label(l)) {
                continue;
            }
            matches.push(if verbose {
                json!({
                    "name": fc.node.name,
                    "cyclomatic": fc.cyclomatic,
                    "cognitive": fc.cognitive,
                    "level": format!("{:?}", fc.level),
                    "file": fc.node.file_path,
                    "labels": fc.node.labels,
                })
            } else {
                json!({
                    "name": fc.node.name,
                    "complexity": fc.cyclomatic,
                    "level": format!("{:?}", fc.level),
                    "location": node_location(&fc.node),
                })
            });
        }

        matches.sort_by(|a, b| {
            b["complexity"]
                .as_u64()
                .unwrap_or(0)
                .cmp(&a["complexity"].as_u64().unwrap_or(0))
        });

        Ok(json!({
            "min_complexity": min_complexity,
            "count": matches.len(),
            "functions": matches,
        }))
    }

    fn get_community_info(
        &self,
        backend: &MemoryBackend,
        name_filter: Option<&str>,
        verbose: bool,
    ) -> Result<Value> {
        let result = self.cache.lock().unwrap().get_community(backend)?;
        let mut communities = Vec::new();

        for community in &result.communities {
            let names: Vec<String> = community
                .members
                .iter()
                .filter_map(|id| backend.get_node(*id).ok().flatten().map(|n| n.name.clone()))
                .collect();

            let label = community_label(&names);
            if let Some(filter) = name_filter {
                if !label.to_lowercase().contains(&filter.to_lowercase())
                    && !names.iter().any(|n| n.to_lowercase().contains(&filter.to_lowercase()))
                {
                    continue;
                }
            }

            communities.push(if verbose {
                json!({
                    "id": community.id,
                    "label": label,
                    "member_count": names.len(),
                    "members": names,
                })
            } else {
                json!({
                    "id": community.id,
                    "label": label,
                    "member_count": names.len(),
                    "sample_members": names.iter().take(5).collect::<Vec<_>>(),
                })
            });
        }

        Ok(json!({
            "modularity": result.modularity,
            "community_count": communities.len(),
            "communities": communities,
        }))
    }

    fn config_analysis(&self, backend: &MemoryBackend, analysis_type: &str, verbose: bool) -> Result<Value> {
        match analysis_type {
            "unused_keys" => {
                let keys = ConfigAnalyzer::find_unused_keys(backend)?;
                Ok(json!({
                    "analysis_type": "unused_keys",
                    "count": keys.len(),
                    "keys": if verbose {
                        json!(keys.iter().map(|k| json!({
                            "key": k.key,
                            "file": k.file,
                            "confidence": k.confidence,
                        })).collect::<Vec<_>>())
                    } else {
                        json!(keys.iter().take(20).map(|k| json!({
                            "key": k.key,
                            "file": k.file,
                        })).collect::<Vec<_>>())
                    },
                }))
            }
            "missing_env" => {
                let env_path = self.repo_root.join(".env");
                let missing =
                    ConfigAnalyzer::find_missing_env_vars(backend, &[env_path.as_path()])?;
                Ok(json!({
                    "analysis_type": "missing_env",
                    "count": missing.len(),
                    "variables": if verbose {
                        json!(missing.iter().map(|v| json!({
                            "var": v.var,
                            "files": v.referenced_in,
                        })).collect::<Vec<_>>())
                    } else {
                        json!(missing.iter().map(|v| &v.var).collect::<Vec<_>>())
                    },
                }))
            }
            "secrets" => {
                let discoverer = FileDiscoverer::new(Arc::new(LanguageRegistry::new()));
                let files = discoverer.discover(&self.repo_root)?;
                let detector = SecretDetector::new();
                let mut found = Vec::new();
                for file in files {
                    if let Ok(content) = std::fs::read_to_string(&file) {
                        for secret in detector.scan(&content) {
                            found.push(json!({
                                "file": file.display().to_string(),
                                "line": secret.line,
                                "type": secret.secret_type,
                                "severity": format!("{:?}", secret.severity),
                                "value": if verbose { secret.value.clone() } else { "[redacted]".into() },
                            }));
                        }
                    }
                }
                Ok(json!({
                    "analysis_type": "secrets",
                    "count": found.len(),
                    "findings": found,
                }))
            }
            other => Err(Error::InvalidQuery(format!("Unknown analysis_type: {other}"))),
        }
    }

    fn symbol_info(
        &self,
        backend: &MemoryBackend,
        symbol_name: &str,
        include_callers: bool,
        include_dependencies: bool,
        verbose: bool,
    ) -> Result<Value> {
        let nodes = backend.find_nodes_by_name(symbol_name)?;
        let node = nodes
            .first()
            .ok_or_else(|| Error::NodeNotFound(symbol_name.to_string()))?;

        let cyclomatic = node
            .get_property("cyclomatic")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1usize);
        let level = format!("{:?}", crate::analysis::complexity::classify_complexity(cyclomatic));

        let signature = SignatureExtractor::from_node(node).map(|s| {
            let params: Vec<String> = s
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.type_))
                .collect();
            format!(
                "fn {}({}){}",
                s.name,
                params.join(", "),
                s.return_type
                    .as_ref()
                    .map(|t| format!(" -> {t}"))
                    .unwrap_or_default()
            )
        });

        let mut response = json!({
            "name": node.name,
            "type": format!("{:?}", node.node_type),
            "signature": signature,
            "complexity": cyclomatic,
            "complexity_level": level,
            "location": node_location(node),
            "labels": node.labels,
        });

        if verbose {
            response["properties"] = json!(node.properties);
            response["qualified_name"] = json!(node.qualified_name);
        }

        if include_callers {
            if let Ok(callers) = DependencyAnalyzer::find_callers(backend, symbol_name) {
                response["callers"] = json!(callers);
            }
        }

        if include_dependencies {
            let deps: Vec<String> = backend
                .get_outgoing_edges(node.id)?
                .iter()
                .filter_map(|e| backend.get_node(e.to).ok().flatten().map(|n| n.name.clone()))
                .collect();
            response["dependencies"] = json!(deps);
        }

        Ok(response)
    }

    fn diff_analysis(&self, since: Option<&str>, verbose: bool) -> Result<Value> {
        let tracker = FileTracker::load(&self.repo_root)?;
        let since_ref = since
            .map(String::from)
            .or_else(|| tracker.last_commit().map(String::from));

        let git_files: Vec<String> = if let Some(ref commit) = since_ref {
            git_changed_files(&self.repo_root, commit)?
                .into_iter()
                .filter_map(|p| {
                    crate::incremental::file_tracker::relative_path(&self.repo_root, &p).ok()
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(json!({
            "since": since_ref,
            "changed_files": if verbose {
                json!(git_files)
            } else {
                json!(git_files.iter().take(20).collect::<Vec<_>>())
            },
            "changed_count": git_files.len(),
        }))
    }
}

fn format_query_result(result: &QueryResult) -> String {
    match result {
        QueryResult::Count(n) => format!("Found {n} result(s)"),
        QueryResult::Nodes(nodes) => {
            if nodes.is_empty() {
                "No results found.".into()
            } else {
                format!("Found {} result(s)", nodes.len())
            }
        }
        QueryResult::Text(lines) => lines.join("\n"),
    }
}

fn query_result_to_json(result: &QueryResult, verbose: bool) -> Value {
    match result {
        QueryResult::Count(n) => json!({ "count": n }),
        QueryResult::Nodes(nodes) => {
            let limit = if verbose { nodes.len() } else { nodes.len().min(10) };
            json!(nodes.iter().take(limit).map(|n| compress_node(n, verbose)).collect::<Vec<_>>())
        }
        QueryResult::Text(lines) => json!({ "lines": lines }),
    }
}

/// Compact node representation for token-efficient agent responses.
pub fn compress_node(node: &Node, verbose: bool) -> Value {
    if verbose {
        json!({
            "name": node.name,
            "type": format!("{:?}", node.node_type),
            "file": node.file_path,
            "line": node.start_line,
            "labels": node.labels,
            "properties": node.properties,
        })
    } else {
        json!({
            "name": node.name,
            "type": format!("{:?}", node.node_type),
            "location": node_location(node),
        })
    }
}

fn node_location(node: &Node) -> String {
    match (&node.file_path, node.start_line) {
        (Some(f), Some(line)) => format!("{f}:{line}"),
        (Some(f), None) => f.clone(),
        _ => "?".into(),
    }
}

fn community_label(names: &[String]) -> String {
    if names.is_empty() {
        return "unknown".into();
    }
    if let Some(prefix) = common_path_prefix(names) {
        return prefix;
    }
    names[0].clone()
}

fn common_path_prefix(names: &[String]) -> Option<String> {
    let parts: Vec<Vec<&str>> = names
        .iter()
        .map(|n| n.split("::").collect())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut prefix = parts[0].clone();
    for other in &parts[1..] {
        while prefix.len() > other.len() || prefix != other[..prefix.len()] {
            prefix.pop();
            if prefix.is_empty() {
                return None;
            }
        }
    }
    Some(prefix.join("::"))
}

fn impact_at_depth(
    backend: &MemoryBackend,
    symbol: &str,
    max_depth: usize,
) -> Result<(Vec<String>, Vec<String>)> {
    let view = PetGraphView::from_backend(backend)?;
    let source = view
        .find_uuid_by_name(symbol)
        .ok_or_else(|| Error::NodeNotFound(symbol.to_string()))?;
    let source_idx = view.uuid_to_directed[&source];

    let mut direct = Vec::new();
    let mut indirect = Vec::new();
    let mut seen = HashSet::new();
    seen.insert(source);
    let mut queue = VecDeque::new();
    queue.push_back((source_idx, 0usize));

    while let Some((idx, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for neighbor in view.directed.neighbors_directed(idx, Direction::Incoming) {
            if let Some(uuid) = view.directed_to_uuid.get(&neighbor) {
                if seen.insert(*uuid) {
                    if let Some(name) = view.nodes.iter().find(|n| n.id == *uuid).map(|n| n.name.clone()) {
                        if depth == 0 {
                            direct.push(name);
                        } else {
                            indirect.push(name);
                        }
                        queue.push_back((neighbor, depth + 1));
                    }
                }
            }
        }
    }

    Ok((direct, indirect))
}

fn names_to_compact(names: &[String], verbose: bool) -> Value {
    if verbose {
        json!(names)
    } else {
        json!(names.iter().take(10).collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::{Edge, EdgeType, NodeType};

    fn test_graph() -> CodeGraph {
        let mut graph = CodeGraph::new();
        let backend = graph.backend_mut();
        let caller = Node::new(NodeType::Function, "authenticate_user".into())
            .with_file_path("src/auth.rs".into())
            .with_location(10, 20)
            .with_property("cyclomatic".into(), "5".into());
        let target = Node::new(NodeType::Function, "verify_token".into())
            .with_file_path("src/auth/jwt.rs".into())
            .with_location(89, 120)
            .with_property("cyclomatic".into(), "12".into())
            .with_label("security:critical".into());
        let id_caller = caller.id;
        let id_target = target.id;
        backend.insert_node(caller).unwrap();
        backend.insert_node(target).unwrap();
        backend
            .insert_edge(Edge::new(id_caller, id_target, EdgeType::Calls))
            .unwrap();
        graph
    }

    #[test]
    fn test_mcp_tool_query_codebase() {
        let graph = test_graph();
        let executor = ToolExecutor::new(".");
        let result = executor
            .execute(
                &graph,
                "query_codebase",
                json!({ "question": "how many functions?" }),
            )
            .unwrap();
        assert!(result["answer"].as_str().unwrap().contains("2"));
    }

    #[test]
    fn test_mcp_tool_impact_analysis() {
        let graph = test_graph();
        let executor = ToolExecutor::new(".");
        let result = executor
            .execute(
                &graph,
                "impact_analysis",
                json!({ "symbol": "verify_token", "depth": 3 }),
            )
            .unwrap();
        assert!(result["direct_dependencies"].is_array());
        assert!(result["indirect_dependencies"].is_array());
    }

    #[test]
    fn test_context_efficient_response() {
        let graph = test_graph();
        let executor = ToolExecutor::new(".");
        let result = executor
            .execute(
                &graph,
                "symbol_info",
                json!({ "symbol_name": "verify_token" }),
            )
            .unwrap();
        let json_str = serde_json::to_string(&result).unwrap();
        assert!(json_str.len() < 1024, "Response too verbose: {} bytes", json_str.len());
    }

    #[test]
    fn test_find_by_complexity() {
        let graph = test_graph();
        let executor = ToolExecutor::new(".");
        let result = executor
            .execute(
                &graph,
                "find_by_complexity",
                json!({ "min_complexity": 10, "labels": ["security:critical"] }),
            )
            .unwrap();
        assert_eq!(result["count"], 1);
    }

    #[test]
    fn test_analysis_caching() {
        use std::time::Instant;

        let graph = test_graph();
        let executor = ToolExecutor::with_cache_ttl(".", 60); // 60 second TTL

        // First call - should compute
        let start = Instant::now();
        let result1 = executor
            .execute(
                &graph,
                "find_by_complexity",
                json!({ "min_complexity": 1 }),
            )
            .unwrap();
        let first_duration = start.elapsed();

        // Second call - should use cache
        let start = Instant::now();
        let result2 = executor
            .execute(
                &graph,
                "find_by_complexity",
                json!({ "min_complexity": 1 }),
            )
            .unwrap();
        let second_duration = start.elapsed();

        // Results should be identical
        assert_eq!(result1, result2);

        // Second call should be faster (cached)
        // Note: This might be flaky in CI, but demonstrates caching
        assert!(second_duration < first_duration || second_duration.as_micros() < 1000);
    }

    #[test]
    fn test_cache_invalidation() {
        let graph = test_graph();
        let executor = ToolExecutor::with_cache_ttl(".", 60);

        // Populate cache
        executor
            .execute(
                &graph,
                "find_by_complexity",
                json!({ "min_complexity": 1 }),
            )
            .unwrap();

        // Invalidate cache
        executor.invalidate_cache();

        // Next call should recompute (not crash)
        let result = executor
            .execute(
                &graph,
                "find_by_complexity",
                json!({ "min_complexity": 1 }),
            )
            .unwrap();

        assert!(result["count"].is_number());
    }
}
