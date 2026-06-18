//! MCP tool implementations for AI agent integration.

use crate::analysis::blast_radius::BlastRadiusAnalyzer;
use crate::analysis::community::{CommunityDetector, CommunityResult};
use crate::analysis::complexity::{ComplexityAnalyzer, ComplexityReport};
use crate::analysis::dependency::DependencyAnalyzer;
use crate::analysis::graph_utils::PetGraphView;
use crate::analysis::{
    build_cfg_for_function, BackwardSlicer, InterproceduralCFG, InterproceduralSlicer,
    ProgramDependenceGraph, SliceCriterion, TaintAnalyzer, TypeInferenceEngine,
};
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
use crate::gql::{execute, execute_explain, execute_macro, QueryMacroRegistry};
use crate::security::SecurityAnalyzer;
use crate::nlp::dual_agent::DualAgentQuerySystem;
use crate::nlp::pattern_matcher::{PatternMatcher, QueryResult};
use crate::semantic::signature::SignatureExtractor;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
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
            ToolDefinition {
                name: "blast_radius".into(),
                description: "PDG-enhanced blast radius: score, callers, and data-flow depth for a symbol change".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "Function or class name" },
                        "depth": { "type": "integer", "description": "Max transitive caller depth (default 10)" },
                        "include_verbose": { "type": "boolean" }
                    },
                    "required": ["symbol"]
                }),
            },
            ToolDefinition {
                name: "backward_slice".into(),
                description: "Backward program slice: lines that affect a variable at a source line".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file": { "type": "string", "description": "Source file path" },
                        "line": { "type": "integer", "description": "Line number (1-based)" },
                        "variable": { "type": "string", "description": "Variable name" },
                        "function": { "type": "string", "description": "Function name (optional)" },
                        "language": { "type": "string", "description": "rust or python (optional)" },
                        "interprocedural": { "type": "boolean", "description": "Follow call graph across functions (Phase 13.1)" },
                        "include_verbose": { "type": "boolean" }
                    },
                    "required": ["file", "line", "variable"]
                }),
            },
            ToolDefinition {
                name: "gql_query".into(),
                description: "Execute graph query language (GQL) against the code graph".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "GQL query string" },
                        "macro_name": { "type": "string", "description": "Named macro instead of query" },
                        "explain": { "type": "boolean", "description": "Include execution plan" },
                        "include_verbose": { "type": "boolean" }
                    }
                }),
            },
            ToolDefinition {
                name: "taint_analysis".into(),
                description: "Forward taint analysis: track untrusted data from sources to security sinks".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file": { "type": "string", "description": "Source file path" },
                        "function": { "type": "string", "description": "Function name" },
                        "language": { "type": "string", "description": "rust, python, or javascript" },
                        "include_verbose": { "type": "boolean" }
                    },
                    "required": ["file", "function"]
                }),
            },
            ToolDefinition {
                name: "security_scan".into(),
                description: "Scan a function for CWE/OWASP vulnerabilities using taint analysis".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file": { "type": "string", "description": "Source file path" },
                        "function": { "type": "string", "description": "Function name" },
                        "language": { "type": "string", "description": "rust, python, or javascript" },
                        "include_verbose": { "type": "boolean" }
                    },
                    "required": ["file", "function"]
                }),
            },
            ToolDefinition {
                name: "generate_diagram".into(),
                description: "Generate a Mermaid or Graphviz diagram for a graph query".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Graph query DSL (e.g. type:Function)" },
                        "format": {
                            "type": "string",
                            "enum": ["mermaid", "dot", "graphml"],
                            "description": "Output format (default mermaid)"
                        },
                        "diagram_type": {
                            "type": "string",
                            "enum": ["flowchart", "class", "call-graph"],
                            "description": "Mermaid diagram style"
                        },
                        "depth": { "type": "integer", "description": "Neighborhood expansion depth" },
                        "include_verbose": { "type": "boolean" }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "analyze_ansible_playbook".into(),
                description: "Summarize Ansible playbooks, plays, tasks, and roles from the graph".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "playbook": { "type": "string", "description": "Filter by playbook name (optional)" },
                        "include_verbose": { "type": "boolean" }
                    }
                }),
            },
            ToolDefinition {
                name: "find_ansible_roles".into(),
                description: "List Ansible roles and dependency order from the graph".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "role": { "type": "string", "description": "Filter by role name (optional)" },
                        "include_deps": { "type": "boolean", "description": "Include dependency lists" },
                        "include_verbose": { "type": "boolean" }
                    }
                }),
            },
            ToolDefinition {
                name: "ansible_security_scan".into(),
                description: "Scan Ansible tasks in the graph for security issues".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "min_severity": {
                            "type": "string",
                            "enum": ["low", "medium", "high", "critical"],
                            "description": "Minimum severity (default medium)"
                        },
                        "include_verbose": { "type": "boolean" }
                    }
                }),
            },
            ToolDefinition {
                name: "analyze_chef_cookbook".into(),
                description: "Summarize Chef cookbooks, recipes, and resources from the graph".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "cookbook": { "type": "string", "description": "Filter by cookbook name (optional)" },
                        "include_verbose": { "type": "boolean" }
                    }
                }),
            },
            ToolDefinition {
                name: "find_chef_recipes".into(),
                description: "List Chef recipes and resource counts from the graph".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "cookbook": { "type": "string", "description": "Filter by cookbook (optional)" },
                        "include_verbose": { "type": "boolean" }
                    }
                }),
            },
            ToolDefinition {
                name: "chef_security_scan".into(),
                description: "Scan Chef resources in the graph for security issues".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "min_severity": {
                            "type": "string",
                            "enum": ["low", "medium", "high", "critical"],
                            "description": "Minimum severity (default medium)"
                        },
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
            "blast_radius" => {
                let symbol = args
                    .get("symbol")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing symbol".into()))?;
                let depth = args
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;
                self.blast_radius(backend, symbol, depth, verbose)
            }
            "backward_slice" => {
                let file = args
                    .get("file")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing file".into()))?;
                let line = args
                    .get("line")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| Error::InvalidQuery("Missing line".into()))?
                    as usize;
                let variable = args
                    .get("variable")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing variable".into()))?;
                let function = args.get("function").and_then(|v| v.as_str());
                let language = args.get("language").and_then(|v| v.as_str());
                let interprocedural = args
                    .get("interprocedural")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.backward_slice(
                    graph,
                    file,
                    line,
                    variable,
                    function,
                    language,
                    interprocedural,
                    verbose,
                )
            }
            "gql_query" => {
                let query = args.get("query").and_then(|v| v.as_str());
                let macro_name = args.get("macro_name").and_then(|v| v.as_str());
                let explain = args.get("explain").and_then(|v| v.as_bool()).unwrap_or(false);
                self.gql_query(backend, query, macro_name, explain, verbose)
            }
            "taint_analysis" => {
                let file = args
                    .get("file")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing file".into()))?;
                let function = args
                    .get("function")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing function".into()))?;
                let language = args.get("language").and_then(|v| v.as_str());
                self.taint_analysis(file, function, language, verbose)
            }
            "security_scan" => {
                let file = args
                    .get("file")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing file".into()))?;
                let function = args
                    .get("function")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing function".into()))?;
                let language = args.get("language").and_then(|v| v.as_str());
                self.security_scan(file, function, language, verbose)
            }
            "generate_diagram" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidQuery("Missing query".into()))?;
                let format = args
                    .get("format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("mermaid");
                let diagram_type = args
                    .get("diagram_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("flowchart");
                let depth = args.get("depth").and_then(|v| v.as_u64()).map(|d| d as usize);
                self.generate_diagram(backend, query, format, diagram_type, depth, verbose)
            }
            "analyze_ansible_playbook" => {
                let playbook = args.get("playbook").and_then(|v| v.as_str());
                self.analyze_ansible_playbook(backend, playbook, verbose)
            }
            "find_ansible_roles" => {
                let role = args.get("role").and_then(|v| v.as_str());
                let include_deps = args
                    .get("include_deps")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                self.find_ansible_roles(backend, role, include_deps, verbose)
            }
            "ansible_security_scan" => {
                let min_severity = args
                    .get("min_severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("medium");
                self.ansible_security_scan(backend, min_severity, verbose)
            }
            "analyze_chef_cookbook" => {
                let cookbook = args.get("cookbook").and_then(|v| v.as_str());
                self.analyze_chef_cookbook(backend, cookbook, verbose)
            }
            "find_chef_recipes" => {
                let cookbook = args.get("cookbook").and_then(|v| v.as_str());
                self.find_chef_recipes(backend, cookbook, verbose)
            }
            "chef_security_scan" => {
                let min_severity = args
                    .get("min_severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("medium");
                self.chef_security_scan(backend, min_severity, verbose)
            }
            other => Err(Error::InvalidQuery(format!("Unknown tool: {other}"))),
        }
    }

    fn query_codebase(&self, backend: &MemoryBackend, question: &str, verbose: bool) -> Result<Value> {
        let matcher = PatternMatcher::from_graph(backend)?;
        let lower = question.to_lowercase();
        let use_dual = lower.contains(" and ")
            || lower.contains("security")
            || lower.contains("authentication")
            || lower.contains("impact")
            || lower.contains("callers");

        let (answer, confidence, sub_query_count, dual, fallback_results) = if use_dual {
            let dual = DualAgentQuerySystem::new().query(question, backend)?;
            let answer = dual.answer_lines.join("\n");
            (
                answer,
                dual.confidence,
                dual.context.sub_queries.len(),
                Some(dual),
                None,
            )
        } else {
            let translated = matcher.translate(question)?;
            let result = matcher.execute(&translated, backend)?;
            let answer = format_query_result(&result);
            let results = if verbose {
                Some(query_result_to_json(&result, true))
            } else {
                None
            };
            (
                answer,
                translated.confidence,
                0usize,
                None,
                results,
            )
        };

        let mut response = json!({
            "answer": answer,
            "confidence": confidence,
            "sub_queries": sub_query_count,
        });

        if let Some(results) = fallback_results {
            response["results"] = results;
        }

        if let Some(dual) = dual {
            if verbose {
                response["sub_query_details"] = json!(dual
                    .context
                    .sub_queries
                    .iter()
                    .map(|sq| json!({
                        "question": sq.natural_language,
                        "pattern": sq.translated_pattern,
                        "results": sq.results.len(),
                    }))
                    .collect::<Vec<_>>());
                response["nodes"] = json!(dual.nodes.iter().map(|n| json!({
                    "name": n.name,
                    "type": format!("{:?}", n.node_type),
                })).collect::<Vec<_>>());
            }
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

    fn blast_radius(
        &self,
        backend: &MemoryBackend,
        symbol: &str,
        depth: usize,
        verbose: bool,
    ) -> Result<Value> {
        let report = BlastRadiusAnalyzer::new(backend)
            .with_max_depth(depth)
            .analyze(symbol)?;

        let mut response = json!({
            "symbol": symbol,
            "score": report.score,
            "direct_callers": report.direct_callers.len(),
            "impact_zone_size": report.impact_zone.len(),
            "data_flow_depth": report.data_flow_depth,
            "severity": if report.score > 70.0 { "critical" }
                else if report.score > 40.0 { "high" }
                else if report.score > 15.0 { "medium" }
                else { "low" },
        });

        if verbose {
            response["direct_callers"] = json!(report.direct_callers);
            response["impact_zone"] = json!(report.impact_zone);
            response["data_flow_impact"] = json!(report
                .data_flow_impact
                .iter()
                .map(|d| json!({
                    "caller": d.caller_name,
                    "depth": d.depth,
                }))
                .collect::<Vec<_>>());
        } else {
            response["direct_callers"] = json!(report.direct_callers.iter().take(10).collect::<Vec<_>>());
            response["impact_zone_sample"] = json!(report.impact_zone.iter().take(10).collect::<Vec<_>>());
        }

        Ok(response)
    }

    #[allow(clippy::too_many_arguments)]
    fn backward_slice(
        &self,
        graph: &CodeGraph,
        file: &str,
        line: usize,
        variable: &str,
        function: Option<&str>,
        language: Option<&str>,
        interprocedural: bool,
        verbose: bool,
    ) -> Result<Value> {
        let path = if std::path::Path::new(file).is_absolute() {
            std::path::PathBuf::from(file)
        } else {
            self.repo_root.join(file)
        };
        let source = std::fs::read_to_string(&path)?;
        let lang = language.map(str::to_string).unwrap_or_else(|| {
            match path.extension().and_then(|e| e.to_str()) {
                Some("py") => "python".to_string(),
                Some("js") | Some("ts") => "javascript".to_string(),
                _ => "rust".to_string(),
            }
        });
        let fn_name = function.map(str::to_string).unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("main")
                .to_string()
        });

        let (reduction_percent, lines, extra): (f64, Vec<usize>, Value) = if interprocedural {
            let mut files = std::collections::HashMap::new();
            files.insert(path.display().to_string(), source.clone());
            let icfg = InterproceduralCFG::build(graph.backend(), &files)?;
            let func_id = icfg
                .call_graph
                .nodes
                .values()
                .find(|n| n.name == fn_name)
                .map(|n| n.id)
                .ok_or_else(|| Error::NodeNotFound(fn_name.clone()))?;
            let slicer = InterproceduralSlicer::new(&icfg, &files)?;
            let slice = slicer.slice(
                func_id,
                SliceCriterion {
                    variable: variable.to_string(),
                    line,
                },
            )?;
            let mut lines: Vec<usize> = slice.lines.into_iter().collect();
            lines.sort_unstable();
            (
                slice.reduction_percent,
                lines,
                json!({
                    "interprocedural": true,
                    "functions_in_slice": slice.functions.len(),
                }),
            )
        } else {
            let cfg = build_cfg_for_function(&lang, &source, &fn_name)?;
            let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
            let slice = BackwardSlicer::new(&pdg, &cfg).slice(SliceCriterion {
                variable: variable.to_string(),
                line,
            })?;
            let mut lines: Vec<usize> = slice.lines.into_iter().collect();
            lines.sort_unstable();
            let mut extra = json!({ "interprocedural": false });
            if verbose {
                extra["cfg_dot"] = json!(cfg.to_dot());
                extra["statement_count"] = json!(slice.statements.len());
            }
            (slice.reduction_percent, lines, extra)
        };

        let mut response = json!({
            "file": path.display().to_string(),
            "line": line,
            "variable": variable,
            "function": fn_name,
            "language": lang,
            "reduction_percent": reduction_percent,
            "total_lines": lines.len(),
            "slice_lines": lines,
        });
        if let Some(obj) = extra.as_object() {
            for (k, v) in obj {
                response[k] = v.clone();
            }
        }
        Ok(response)
    }

    fn taint_analysis(
        &self,
        file: &str,
        function: &str,
        language: Option<&str>,
        verbose: bool,
    ) -> Result<Value> {
        let (path, source, lang) = self.read_source_file(file, language)?;
        let cfg = build_cfg_for_function(&lang, &source, function)?;
        let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
        let mut type_engine = TypeInferenceEngine::new(&pdg, &cfg, &lang);
        type_engine.infer();
        let mut analyzer = TaintAnalyzer::new(&pdg, &cfg).with_type_inference(type_engine);
        analyzer.detect_patterns(&lang);
        let flows = analyzer.analyze();
        let vulnerable = flows.iter().filter(|f| f.is_vulnerable()).count();

        let flow_json: Vec<Value> = flows
            .iter()
            .map(|f| {
                json!({
                    "variable": f.variable,
                    "severity": f.severity,
                    "vulnerable": f.is_vulnerable(),
                    "source_type": format!("{:?}", f.source_type),
                    "sink_type": format!("{:?}", f.sink_type),
                    "sanitizers": f.sanitizers.len(),
                })
            })
            .collect();

        let response = json!({
            "file": path.display().to_string(),
            "function": function,
            "language": lang,
            "total_flows": flows.len(),
            "vulnerable_flows": vulnerable,
            "flows": if verbose { json!(flow_json) } else { json!(flow_json.iter().take(10).collect::<Vec<_>>()) },
        });
        Ok(response)
    }

    fn security_scan(
        &self,
        file: &str,
        function: &str,
        language: Option<&str>,
        verbose: bool,
    ) -> Result<Value> {
        let (path, source, lang) = self.read_source_file(file, language)?;
        let cfg = build_cfg_for_function(&lang, &source, function)?;
        let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
        let mut type_engine = TypeInferenceEngine::new(&pdg, &cfg, &lang);
        type_engine.infer();
        let mut analyzer = TaintAnalyzer::new(&pdg, &cfg).with_type_inference(type_engine);
        analyzer.detect_patterns(&lang);
        let flows = analyzer.vulnerable_flows();
        let vulns = SecurityAnalyzer::new().analyze(flows, &pdg, &source);

        let vuln_json: Vec<Value> = vulns
            .iter()
            .map(|v| {
                json!({
                    "cwe_id": v.cwe_id,
                    "cwe_name": v.cwe_name,
                    "severity": v.severity,
                    "variable": v.taint_flow.variable,
                    "source_line": v.source_line,
                    "sink_line": v.sink_line,
                    "recommendation": v.recommendation,
                })
            })
            .collect();

        let response = json!({
            "file": path.display().to_string(),
            "function": function,
            "total_vulnerabilities": vulns.len(),
            "critical": vulns.iter().filter(|v| v.severity >= 9).count(),
            "high": vulns.iter().filter(|v| v.severity >= 7 && v.severity < 9).count(),
            "vulnerabilities": if verbose { json!(vuln_json) } else { json!(vuln_json.iter().take(10).collect::<Vec<_>>()) },
        });
        Ok(response)
    }

    fn generate_diagram(
        &self,
        backend: &MemoryBackend,
        query: &str,
        format: &str,
        diagram_type: &str,
        depth: Option<usize>,
        verbose: bool,
    ) -> Result<Value> {
        use crate::export::{
            export_graphml, generate_dot, generate_mermaid, parse_diagram_type, GraphvizOptions,
            MermaidOptions,
        };

        let content = match format.to_ascii_lowercase().as_str() {
            "dot" | "graphviz" => generate_dot(
                backend,
                query,
                GraphvizOptions::default(),
                depth,
            )?,
            "graphml" => export_graphml(backend, query)?,
            _ => generate_mermaid(
                backend,
                query,
                MermaidOptions {
                    diagram_type: parse_diagram_type(diagram_type),
                    max_depth: depth,
                    vertical: true,
                },
            )?,
        };

        let mut response = json!({
            "query": query,
            "format": format,
            "diagram_type": diagram_type,
            "content": content,
        });
        if verbose {
            if let Some(obj) = response.as_object_mut() {
                obj.insert("length".into(), json!(content.len()));
            }
        }
        Ok(response)
    }

    fn analyze_ansible_playbook(
        &self,
        backend: &MemoryBackend,
        playbook_filter: Option<&str>,
        verbose: bool,
    ) -> Result<Value> {
        use crate::graph::schema::NodeType;

        let playbooks: Vec<_> = backend
            .find_nodes_by_type(NodeType::AnsiblePlaybook)?
            .into_iter()
            .filter(|n| {
                playbook_filter.is_none_or(|f| n.name.contains(f))
            })
            .collect();

        let mut summary = Vec::new();
        for pb in &playbooks {
            let plays = backend
                .find_nodes_by_type(NodeType::AnsiblePlay)?
                .into_iter()
                .filter(|p| {
                    playbook_filter.is_none_or(|f| {
                        p.name.contains(f)
                            || p
                                .get_property("playbook")
                                .is_some_and(|pb| pb.contains(f))
                    })
                })
                .count();
            let tasks = backend.find_nodes_by_type(NodeType::AnsibleTask)?.len();
            summary.push(json!({
                "playbook": pb.name,
                "plays": plays,
                "file": pb.file_path,
            }));
            let _ = tasks;
        }

        let total_plays = backend.find_nodes_by_type(NodeType::AnsiblePlay)?.len();
        let total_tasks = backend.find_nodes_by_type(NodeType::AnsibleTask)?.len();
        let total_roles = backend.find_nodes_by_type(NodeType::AnsibleRole)?.len();

        let mut response = json!({
            "playbooks": summary,
            "totals": {
                "playbooks": playbooks.len(),
                "plays": total_plays,
                "tasks": total_tasks,
                "roles": total_roles,
            },
        });
        if verbose {
            if let Some(obj) = response.as_object_mut() {
                obj.insert(
                    "plays".into(),
                    json!(backend.find_nodes_by_type(NodeType::AnsiblePlay)?),
                );
            }
        }
        Ok(response)
    }

    fn find_ansible_roles(
        &self,
        backend: &MemoryBackend,
        role_filter: Option<&str>,
        include_deps: bool,
        verbose: bool,
    ) -> Result<Value> {
        use crate::analysis::ansible_roles::RoleDependencyGraph;

        let graph = RoleDependencyGraph::from_graph(backend)?;
        let roles: Vec<_> = graph
            .roles
            .values()
            .filter(|r| role_filter.is_none_or(|f| r.name.contains(f)))
            .map(|r| {
                if include_deps {
                    json!({
                        "name": r.name,
                        "path": r.path,
                        "dependencies": r.dependencies,
                        "dependents": r.dependents,
                    })
                } else {
                    json!({ "name": r.name, "path": r.path })
                }
            })
            .collect();

        let order = graph.topological_sort().unwrap_or_default();
        let mut response = json!({
            "roles": roles,
            "dependency_order": order,
            "count": roles.len(),
        });
        if verbose {
            if let Some(obj) = response.as_object_mut() {
                obj.insert("graph".into(), json!(graph.roles));
            }
        }
        Ok(response)
    }

    fn ansible_security_scan(
        &self,
        backend: &MemoryBackend,
        min_severity: &str,
        verbose: bool,
    ) -> Result<Value> {
        use crate::security::ansible::{AnsibleSecurityScanner, AnsibleSeverity};

        let min = match min_severity.to_ascii_lowercase().as_str() {
            "low" => AnsibleSeverity::Low,
            "medium" => AnsibleSeverity::Medium,
            "high" => AnsibleSeverity::High,
            "critical" => AnsibleSeverity::Critical,
            other => return Err(Error::InvalidQuery(format!("Unknown severity: {other}"))),
        };

        let findings =
            AnsibleSecurityScanner::filter_by_severity(AnsibleSecurityScanner::new().scan_graph(backend), min);

        let mut response = json!({
            "finding_count": findings.len(),
            "findings": findings.iter().map(|f| json!({
                "severity": format!("{:?}", f.severity),
                "message": f.message,
                "location": f.location,
                "cwe": f.cwe,
            })).collect::<Vec<_>>(),
        });
        if verbose {
            if let Some(obj) = response.as_object_mut() {
                obj.insert("details".into(), json!(findings));
            }
        }
        Ok(response)
    }

    fn analyze_chef_cookbook(
        &self,
        backend: &MemoryBackend,
        cookbook_filter: Option<&str>,
        verbose: bool,
    ) -> Result<Value> {
        use crate::graph::schema::NodeType;

        let cookbooks: Vec<_> = backend
            .find_nodes_by_type(NodeType::ChefCookbook)?
            .into_iter()
            .filter(|n| {
                cookbook_filter.is_none_or(|f| {
                    n.name.contains(f)
                        || n
                            .get_property("cookbook")
                            .is_some_and(|c| c.contains(f))
                })
            })
            .collect();

        let total_recipes = backend.find_nodes_by_type(NodeType::ChefRecipe)?.len();
        let total_resources = backend.find_nodes_by_type(NodeType::ChefResource)?.len();

        let mut response = json!({
            "cookbooks": cookbooks.iter().map(|c| json!({
                "name": c.name,
                "version": c.get_property("version"),
                "file": c.file_path,
            })).collect::<Vec<_>>(),
            "totals": {
                "cookbooks": cookbooks.len(),
                "recipes": total_recipes,
                "resources": total_resources,
            },
        });
        if verbose {
            if let Some(obj) = response.as_object_mut() {
                obj.insert(
                    "recipes".into(),
                    json!(backend.find_nodes_by_type(NodeType::ChefRecipe)?),
                );
            }
        }
        Ok(response)
    }

    fn find_chef_recipes(
        &self,
        backend: &MemoryBackend,
        cookbook_filter: Option<&str>,
        verbose: bool,
    ) -> Result<Value> {
        use crate::analysis::chef_cookbooks::CookbookDependencyGraph;
        use crate::graph::schema::NodeType;

        let recipes: Vec<_> = backend
            .find_nodes_by_type(NodeType::ChefRecipe)?
            .into_iter()
            .filter(|r| {
                cookbook_filter.is_none_or(|f| {
                    r.name.contains(f)
                        || r
                            .get_property("cookbook")
                            .is_some_and(|c| c.contains(f))
                })
            })
            .map(|r| json!({ "name": r.name, "file": r.file_path }))
            .collect();

        let graph = CookbookDependencyGraph::from_graph(backend)?;
        let order = graph.topological_sort().unwrap_or_default();

        let mut response = json!({
            "recipes": recipes,
            "recipe_count": recipes.len(),
            "cookbook_dependency_order": order,
        });
        if verbose {
            if let Some(obj) = response.as_object_mut() {
                obj.insert("cookbooks".into(), json!(graph.cookbooks));
            }
        }
        Ok(response)
    }

    fn chef_security_scan(
        &self,
        backend: &MemoryBackend,
        min_severity: &str,
        verbose: bool,
    ) -> Result<Value> {
        use crate::security::chef::{ChefSecurityScanner, ChefSeverity};

        let min = match min_severity.to_ascii_lowercase().as_str() {
            "low" => ChefSeverity::Low,
            "medium" => ChefSeverity::Medium,
            "high" => ChefSeverity::High,
            "critical" => ChefSeverity::Critical,
            other => return Err(Error::InvalidQuery(format!("Unknown severity: {other}"))),
        };

        let findings =
            ChefSecurityScanner::filter_by_severity(ChefSecurityScanner::new().scan_graph(backend), min);

        let mut response = json!({
            "finding_count": findings.len(),
            "findings": findings.iter().map(|f| json!({
                "severity": format!("{:?}", f.severity),
                "message": f.message,
                "location": f.location,
                "cwe": f.cwe,
            })).collect::<Vec<_>>(),
        });
        if verbose {
            if let Some(obj) = response.as_object_mut() {
                obj.insert("details".into(), json!(findings));
            }
        }
        Ok(response)
    }

    fn read_source_file(
        &self,
        file: &str,
        language: Option<&str>,
    ) -> Result<(std::path::PathBuf, String, String)> {
        let path = if std::path::Path::new(file).is_absolute() {
            std::path::PathBuf::from(file)
        } else {
            self.repo_root.join(file)
        };
        let source = std::fs::read_to_string(&path)?;
        let lang = language.map(str::to_string).unwrap_or_else(|| {
            match path.extension().and_then(|e| e.to_str()) {
                Some("py") => "python".to_string(),
                Some("js") | Some("ts") => "javascript".to_string(),
                _ => "rust".to_string(),
            }
        });
        Ok((path, source, lang))
    }

    fn gql_query(
        &self,
        backend: &MemoryBackend,
        query: Option<&str>,
        macro_name: Option<&str>,
        explain: bool,
        verbose: bool,
    ) -> Result<Value> {
        let registry = QueryMacroRegistry::with_defaults();
        let result = match (query, macro_name) {
            (None, Some(name)) => execute_macro(backend, &registry, name)?,
            (Some(q), None) if explain => execute_explain(backend, q)?,
            (Some(q), None) => execute(backend, q)?,
            (Some(_), Some(_)) => {
                return Err(Error::InvalidQuery(
                    "Provide either query or macro_name, not both".into(),
                ));
            }
            (None, None) => {
                return Err(Error::InvalidQuery(
                    "Missing query or macro_name".into(),
                ));
            }
        };

        let rows: Vec<Value> = result
            .rows
            .iter()
            .map(|row| {
                json!(row
                    .iter()
                    .map(|(var, node)| (var.clone(), node.name.clone()))
                    .collect::<HashMap<_, _>>())
            })
            .collect();

        let mut response = json!({
            "row_count": result.rows.len(),
            "rows": if verbose { json!(rows) } else { json!(rows.iter().take(20).collect::<Vec<_>>()) },
        });

        if explain {
            if let Some(plan) = result.plan {
                response["explain"] = json!(plan
                    .steps
                    .iter()
                    .map(|s| json!({ "operation": s.operation, "detail": s.detail }))
                    .collect::<Vec<_>>());
            }
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
