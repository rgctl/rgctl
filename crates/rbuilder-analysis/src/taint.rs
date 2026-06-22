//! Forward taint analysis from sources to sinks (Phase 13.0).

use crate::cfg::ControlFlowGraph;
use crate::pdg::{PdgNodeId, ProgramDependenceGraph};
use crate::type_inference::{InferredType, TypeInferenceEngine};
use std::collections::{HashMap, HashSet, VecDeque};

/// Classification of taint sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaintSource {
    /// HTTP request parameter.
    HttpParameter,
    /// File read.
    FileInput,
    /// Network socket read.
    NetworkInput,
    /// CLI argument.
    CommandLineArg,
    /// Environment variable.
    EnvironmentVar,
    /// Database query result.
    DatabaseResult,
}

/// Classification of taint sinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaintSink {
    /// SQL execution.
    SqlQuery,
    /// Shell command.
    ShellCommand,
    /// File write.
    FileWrite,
    /// Network send.
    NetworkOutput,
    /// Log output.
    LogOutput,
    /// HTML render (XSS).
    HtmlRender,
    /// eval/exec.
    CodeEval,
}

/// Sanitizer that may break taint flow.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Sanitizer {
    /// Prepared statement / parameter binding.
    SqlParameterize,
    /// HTML escape.
    HtmlEscape,
    /// Shell escape.
    ShellEscape,
    /// Regex validation.
    Validation(String),
    /// Type cast (numeric, etc.).
    TypeCast(String),
}

/// A taint flow path from source to sink.
#[derive(Debug, Clone)]
pub struct TaintFlow {
    /// Source PDG node.
    pub source: PdgNodeId,
    /// Source kind.
    pub source_type: TaintSource,
    /// Sink PDG node.
    pub sink: PdgNodeId,
    /// Sink kind.
    pub sink_type: TaintSink,
    /// Tainted variable name.
    pub variable: String,
    /// PDG nodes on the path.
    pub path: Vec<PdgNodeId>,
    /// Sanitizers encountered.
    pub sanitizers: Vec<Sanitizer>,
    /// Severity 1–10.
    pub severity: u8,
}

impl TaintFlow {
    /// True when no sanitizer was applied on the path.
    pub fn is_vulnerable(&self) -> bool {
        self.sanitizers.is_empty()
    }

    /// Compute severity from source/sink pair.
    pub fn compute_severity(&mut self) {
        self.severity = match (self.source_type, self.sink_type) {
            (TaintSource::HttpParameter, TaintSink::SqlQuery) => 10,
            (TaintSource::HttpParameter, TaintSink::ShellCommand) => 10,
            (TaintSource::HttpParameter, TaintSink::HtmlRender) => 9,
            (TaintSource::HttpParameter, TaintSink::CodeEval) => 10,
            (TaintSource::FileInput, TaintSink::ShellCommand) => 8,
            (TaintSource::FileInput, TaintSink::SqlQuery) => 7,
            (TaintSource::DatabaseResult, TaintSink::HtmlRender) => 6,
            (TaintSource::EnvironmentVar, TaintSink::ShellCommand) => 7,
            _ => 5,
        };
    }
}

/// Forward taint analyzer over a function PDG.
pub struct TaintAnalyzer<'a> {
    pdg: &'a ProgramDependenceGraph,
    _cfg: &'a ControlFlowGraph,
    sources: HashMap<PdgNodeId, TaintSource>,
    sinks: HashMap<PdgNodeId, TaintSink>,
    sanitizers: HashMap<PdgNodeId, Sanitizer>,
    type_inference: Option<TypeInferenceEngine<'a>>,
}

impl<'a> TaintAnalyzer<'a> {
    /// Create analyzer for a function.
    pub fn new(pdg: &'a ProgramDependenceGraph, cfg: &'a ControlFlowGraph) -> Self {
        Self {
            pdg,
            _cfg: cfg,
            sources: HashMap::new(),
            sinks: HashMap::new(),
            sanitizers: HashMap::new(),
            type_inference: None,
        }
    }

    /// Attach type inference for sanitizer detection (Phase 13.3).
    pub fn with_type_inference(mut self, engine: TypeInferenceEngine<'a>) -> Self {
        self.type_inference = Some(engine);
        self
    }

    /// Detect sources, sinks, and sanitizers from statement patterns.
    pub fn detect_patterns(&mut self, language: &str) {
        match language {
            "python" | "py" => self.detect_python_patterns(),
            "javascript" | "js" | "typescript" | "ts" => self.detect_js_patterns(),
            "rust" | "rs" => self.detect_rust_patterns(),
            _ => {}
        }
    }

    fn detect_python_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;
            if text.contains("request.GET") || text.contains("request.POST") {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            } else if text.contains("open(") {
                self.sources.insert(*node_id, TaintSource::FileInput);
            } else if text.contains("sys.argv") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("os.environ") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            }
            if text.contains("execute(") || text.contains("executemany(") {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            } else if text.contains("os.system(") || text.contains("subprocess.") {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("eval(") || text.contains("exec(") {
                self.sinks.insert(*node_id, TaintSink::CodeEval);
            } else if text.contains("render(") || text.contains(".html") {
                self.sinks.insert(*node_id, TaintSink::HtmlRender);
            }
            if text.contains("int(") || text.contains("float(") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::TypeCast("numeric".into()));
            } else if text.contains("escape(") || text.contains("html.escape") {
                self.sanitizers.insert(*node_id, Sanitizer::HtmlEscape);
            } else if text.contains("shlex.quote") {
                self.sanitizers.insert(*node_id, Sanitizer::ShellEscape);
            }
        }
    }

    fn detect_js_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;
            if text.contains("req.query")
                || text.contains("req.body")
                || text.contains("req.params")
            {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            } else if text.contains("fs.readFile") {
                self.sources.insert(*node_id, TaintSource::FileInput);
            } else if text.contains("process.argv") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("process.env") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            }
            if text.contains(".query(") || text.contains(".execute(") {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            } else if text.contains("exec(") || text.contains("spawn(") {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("eval(") || text.contains("Function(") {
                self.sinks.insert(*node_id, TaintSink::CodeEval);
            } else if text.contains("innerHTML") || text.contains("document.write") {
                self.sinks.insert(*node_id, TaintSink::HtmlRender);
            }
            if text.contains("parseInt(") || text.contains("parseFloat(") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::TypeCast("numeric".into()));
            } else if text.contains("escapeHtml(") || text.contains("sanitize(") {
                self.sanitizers.insert(*node_id, Sanitizer::HtmlEscape);
            }
        }
    }

    fn detect_rust_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;
            if text.contains("env::var") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            } else if text.contains("env::args") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("File::open") || text.contains("read_to_string") {
                self.sources.insert(*node_id, TaintSource::FileInput);
            }
            if text.contains("Command::new") {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("query(") || text.contains("execute(") {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            }
            if text.contains(".parse::<") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::TypeCast("typed".into()));
            }
        }
    }

    /// Run forward taint analysis.
    pub fn analyze(&self) -> Vec<TaintFlow> {
        let mut flows = Vec::new();
        for (source_id, source_type) in &self.sources {
            for (sink_id, path) in self.find_reachable_sinks_from_source(*source_id) {
                let Some(sink_type) = self.sinks.get(&sink_id).copied() else {
                    continue;
                };
                let variable = self
                    .pdg
                    .nodes
                    .get(source_id)
                    .and_then(|n| n.defined_vars.iter().next())
                    .cloned()
                    .unwrap_or_default();
                let sanitizers = self.find_sanitizers_on_path(&path, &variable);
                let mut flow = TaintFlow {
                    source: *source_id,
                    source_type: *source_type,
                    sink: sink_id,
                    sink_type,
                    variable,
                    path,
                    sanitizers,
                    severity: 0,
                };
                flow.compute_severity();
                flows.push(flow);
            }
        }
        flows
    }

    /// Vulnerable flows only (no sanitizers).
    pub fn vulnerable_flows(&self) -> Vec<TaintFlow> {
        self.analyze()
            .into_iter()
            .filter(|f| f.is_vulnerable())
            .collect()
    }

    fn find_reachable_sinks_from_source(
        &self,
        source: PdgNodeId,
    ) -> Vec<(PdgNodeId, Vec<PdgNodeId>)> {
        let mut reachable = Vec::new();
        let mut visited = HashSet::new();
        let mut queue: VecDeque<(PdgNodeId, Vec<PdgNodeId>)> = VecDeque::new();
        queue.push_back((source, vec![source]));

        while let Some((current, path)) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }
            if self.sinks.contains_key(&current) {
                reachable.push((current, path.clone()));
            }
            for dep in self.pdg.data_deps.iter().filter(|d| d.from == current) {
                let mut new_path = path.clone();
                new_path.push(dep.to);
                queue.push_back((dep.to, new_path));
            }
        }
        reachable
    }

    fn find_sanitizers_on_path(&self, path: &[PdgNodeId], variable: &str) -> Vec<Sanitizer> {
        let mut sanitizers = Vec::new();
        for node_id in path {
            if let Some(san) = self.sanitizers.get(node_id) {
                sanitizers.push(san.clone());
            }
            if let Some(ref engine) = self.type_inference {
                if let Some(typ) = engine.get_type(*node_id, variable) {
                    match typ {
                        InferredType::Int | InferredType::Float => {
                            sanitizers.push(Sanitizer::TypeCast("numeric".into()));
                        }
                        InferredType::Bool => {
                            sanitizers.push(Sanitizer::TypeCast("boolean".into()));
                        }
                        _ => {}
                    }
                }
            }
        }
        sanitizers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_builder::build_cfg_for_function;
    use crate::pdg::ProgramDependenceGraph;
    use crate::type_inference::TypeInferenceEngine;

    #[test]
    fn test_taint_sql_injection_python() {
        let code = r#"
def handle_request(request):
    username = request.GET['username']
    query = f"SELECT * FROM users WHERE name = '{username}'"
    cursor.execute(query)
"#;
        let cfg = build_cfg_for_function("python", code, "handle_request").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
        analyzer.detect_patterns("python");
        let flows = analyzer.vulnerable_flows();
        assert!(!flows.is_empty(), "expected vulnerable SQL flow");
        assert_eq!(flows[0].source_type, TaintSource::HttpParameter);
        assert_eq!(flows[0].sink_type, TaintSink::SqlQuery);
    }

    #[test]
    fn test_taint_sanitized_flow_python() {
        let code = r#"
def handle_request(request):
    user_id = request.GET['id']
    safe_id = int(user_id)
    query = f"SELECT * FROM users WHERE id = {safe_id}"
    cursor.execute(query)
"#;
        let cfg = build_cfg_for_function("python", code, "handle_request").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let mut type_engine = TypeInferenceEngine::new(&pdg, &cfg, "python");
        type_engine.infer();
        let mut analyzer = TaintAnalyzer::new(&pdg, &cfg).with_type_inference(type_engine);
        analyzer.detect_patterns("python");
        let vulnerable = analyzer.vulnerable_flows();
        let all = analyzer.analyze();
        assert!(
            vulnerable.len() < all.len() || vulnerable.is_empty(),
            "sanitized flow should reduce vulnerabilities"
        );
    }
}
