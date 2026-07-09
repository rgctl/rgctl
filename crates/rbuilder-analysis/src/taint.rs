//! Forward taint analysis from sources to sinks (Phase 13.0).

use crate::cfg::ControlFlowGraph;
use crate::dominance::DominatorTree;
use crate::language_profile::canonical_language_id;
use crate::pdg::{PdgNodeId, ProgramDependenceGraph};
use crate::policy::PolicyViolation;
use crate::type_inference::{InferredType, TypeInferenceEngine};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Classification of taint sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// True when no sanitizer was applied on the path (path-presence only).
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
    dom_tree: DominatorTree,
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
            dom_tree: DominatorTree::build(cfg),
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
        match canonical_language_id(language).unwrap_or(language) {
            "python" => self.detect_python_patterns(),
            "javascript" | "typescript" => self.detect_js_patterns(),
            "rust" => self.detect_rust_patterns(),
            "go" => self.detect_go_patterns(),
            "java" => self.detect_java_patterns(),
            "csharp" => self.detect_csharp_patterns(),
            "c" => self.detect_c_patterns(),
            "cpp" => self.detect_cpp_patterns(),
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

            if text.contains("Path(")
                || text.contains("Query(")
                || text.contains("Json(")
                || text.contains("Form(")
                || text.contains("RawBody(")
                || text.contains("TypedHeader(")
                || text.contains("axum::extract")
                || text.contains("Extension(")
            {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            } else if text.contains("env::var") || text.contains("std::env::var") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            } else if text.contains("env::args") || text.contains("std::env::args") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("File::open")
                || text.contains("read_to_string")
                || text.contains("std::fs::read")
            {
                self.sources.insert(*node_id, TaintSource::FileInput);
            } else if text.contains("reqwest::")
                || text.contains("hyper::")
                || text.contains("TcpStream::connect")
            {
                self.sources.insert(*node_id, TaintSource::NetworkInput);
            }

            if text.contains("sqlx::query")
                || text.contains("query_as")
                || text.contains(".execute(")
                || text.contains("fetch_one(")
                || text.contains("fetch_all(")
                || text.contains("fetch_optional(")
            {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            } else if text.contains("Command::new") || text.contains("std::process::Command") {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("format!") && text.contains("Html") {
                self.sinks.insert(*node_id, TaintSink::HtmlRender);
            } else if text.contains("std::fs::write") || text.contains("write_all(") {
                self.sinks.insert(*node_id, TaintSink::FileWrite);
            }

            if text.contains(".bind(") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::SqlParameterize);
            } else if text.contains(".parse::<") || text.contains("FromStr") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::TypeCast("typed".into()));
            } else if text.contains("html_escape") || text.contains("ammonia::") {
                self.sanitizers.insert(*node_id, Sanitizer::HtmlEscape);
            }
        }
    }

    fn detect_go_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;

            if text.contains("Query(")
                || text.contains("Param(")
                || text.contains("PostForm(")
                || text.contains("ShouldBindJSON")
                || text.contains("BindJSON")
                || text.contains("FormValue(")
                || text.contains("GetHeader(")
                || text.contains("Cookie(")
            {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            } else if text.contains("os.Getenv") || text.contains("os.LookupEnv") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            } else if text.contains("os.Args") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("ReadFile(")
                || text.contains("io.ReadAll")
                || text.contains("ioutil.ReadAll")
                || text.contains("json.Unmarshal")
                || text.contains("io.ReadCloser")
            {
                self.sources.insert(*node_id, TaintSource::FileInput);
            } else if text.contains("http.Get(") || text.contains("http.Post(") {
                self.sources.insert(*node_id, TaintSource::NetworkInput);
            }

            if text.contains(".Exec(")
                || text.contains(".Query(")
                || text.contains(".QueryRow(")
                || text.contains("db.Exec")
                || text.contains("sql.Open")
            {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            } else if text.contains("exec.Command")
                || text.contains("exec.CommandContext")
            {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("template.HTML(")
                || text.contains("c.HTML(")
                || (text.contains("fmt.Fprintf") && text.contains("ResponseWriter"))
            {
                self.sinks.insert(*node_id, TaintSink::HtmlRender);
            } else if text.contains("os.WriteFile") || text.contains("ioutil.WriteFile") {
                self.sinks.insert(*node_id, TaintSink::FileWrite);
            }

            if text.contains("strconv.Atoi")
                || text.contains("strconv.ParseInt")
                || text.contains("strconv.ParseFloat")
            {
                self.sanitizers
                    .insert(*node_id, Sanitizer::TypeCast("numeric".into()));
            } else if text.contains("html.EscapeString") {
                self.sanitizers.insert(*node_id, Sanitizer::HtmlEscape);
            } else if text.contains("Prepare(") || text.contains("PrepareContext(") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::SqlParameterize);
            }
        }
    }

    fn detect_java_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;

            // Sources
            if text.contains("getParameter(")
                || text.contains("getHeader(")
                || text.contains("getQueryString(")
                || text.contains("request.get")
            {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            } else if text.contains("FileInputStream")
                || text.contains("Files.read")
                || text.contains("BufferedReader")
            {
                self.sources.insert(*node_id, TaintSource::FileInput);
            } else if text.contains("System.getenv") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            } else if text.contains("args[") || text.contains("getArgs(") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("ResultSet") || text.contains("executeQuery") {
                self.sources.insert(*node_id, TaintSource::DatabaseResult);
            }

            // Sinks
            if text.contains("createStatement")
                || text.contains("prepareStatement")
                || text.contains("executeQuery")
                || text.contains("executeUpdate")
                || text.contains(".createQuery(")
            {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            } else if text.contains("Runtime.getRuntime().exec")
                || text.contains("ProcessBuilder")
                || text.contains(".exec(")
            {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("FileOutputStream")
                || text.contains("FileWriter")
                || text.contains("Files.write")
            {
                self.sinks.insert(*node_id, TaintSink::FileWrite);
            } else if text.contains("PrintWriter")
                || text.contains("response.getWriter")
                || text.contains("innerHTML")
                || text.contains("innerText")
            {
                self.sinks.insert(*node_id, TaintSink::HtmlRender);
            } else if text.contains("Logger.") || text.contains(".log(") {
                self.sinks.insert(*node_id, TaintSink::LogOutput);
            }

            // Sanitizers
            if text.contains("Integer.parseInt")
                || text.contains("Long.parseLong")
                || text.contains("Double.parseDouble")
            {
                self.sanitizers
                    .insert(*node_id, Sanitizer::TypeCast("numeric".into()));
            } else if text.contains("StringEscapeUtils")
                || text.contains("HtmlUtils.htmlEscape")
                || text.contains("ESAPI.encoder")
            {
                self.sanitizers.insert(*node_id, Sanitizer::HtmlEscape);
            } else if text.contains("prepareStatement") && text.contains("setString") {
                self.sanitizers.insert(*node_id, Sanitizer::SqlParameterize);
            } else if text.contains("Pattern.matches")
                || text.contains(".matches(")
                || text.contains("Validator.")
            {
                self.sanitizers
                    .insert(*node_id, Sanitizer::Validation("pattern".into()));
            }
        }
    }

    fn detect_csharp_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;

            if text.contains("Request.Query")
                || text.contains("Request.Form")
                || text.contains("Request.Headers")
                || text.contains("[FromQuery]")
                || text.contains("[FromBody]")
                || text.contains("HttpContext.Request")
            {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            } else if text.contains("File.ReadAllText")
                || text.contains("File.ReadAllBytes")
                || text.contains("StreamReader")
            {
                self.sources.insert(*node_id, TaintSource::FileInput);
            } else if text.contains("Environment.GetEnvironmentVariable") {
                self.sources.insert(*node_id, TaintSource::EnvironmentVar);
            } else if text.contains("args[") || text.contains("Environment.GetCommandLineArgs") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            } else if text.contains("ExecuteReader") || text.contains("SqlDataReader") {
                self.sources.insert(*node_id, TaintSource::DatabaseResult);
            }

            if text.contains("ExecuteSqlRaw")
                || text.contains("ExecuteSqlInterpolated")
                || text.contains("FromSqlRaw")
                || text.contains("SqlCommand")
                || text.contains("ExecuteNonQuery")
                || text.contains("ExecuteReader")
            {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            } else if text.contains("Process.Start")
                || text.contains("ProcessStartInfo")
            {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("File.WriteAllText")
                || text.contains("File.WriteAllBytes")
                || text.contains("StreamWriter")
            {
                self.sinks.insert(*node_id, TaintSink::FileWrite);
            } else if text.contains("Response.Write")
                || text.contains("Html.Raw")
                || text.contains("InnerHtml")
            {
                self.sinks.insert(*node_id, TaintSink::HtmlRender);
            } else if text.contains("Log.")
                || text.contains("Logger.")
                || text.contains("Console.WriteLine")
            {
                self.sinks.insert(*node_id, TaintSink::LogOutput);
            }

            if text.contains("int.Parse")
                || text.contains("long.Parse")
                || text.contains("double.Parse")
            {
                self.sanitizers
                    .insert(*node_id, Sanitizer::TypeCast("numeric".into()));
            } else if text.contains("HtmlEncode")
                || text.contains("WebUtility.HtmlEncode")
                || text.contains("AntiXss")
            {
                self.sanitizers.insert(*node_id, Sanitizer::HtmlEscape);
            } else if text.contains("AddWithValue") || text.contains("Parameters.Add") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::SqlParameterize);
            }
        }
    }

    fn detect_c_patterns(&mut self) {
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;

            if text.contains("getenv(")
                || text.contains("getenv_s(")
                || text.contains("getchar(")
                || text.contains("fgets(")
                || text.contains("read(")
                || text.contains("recv(")
                || text.contains("QUERY_STRING")
            {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            } else if text.contains("fread(") || text.contains("fopen(") {
                self.sources.insert(*node_id, TaintSource::FileInput);
            } else if text.contains("argv[") {
                self.sources.insert(*node_id, TaintSource::CommandLineArg);
            }

            if text.contains("sqlite3_exec(")
                || text.contains("mysql_query(")
                || text.contains("PQexec(")
                || ((text.contains("sprintf(") || text.contains("snprintf("))
                    && (text.contains("SELECT")
                        || text.contains("INSERT")
                        || text.contains("UPDATE")
                        || text.contains("DELETE")))
            {
                self.sinks.insert(*node_id, TaintSink::SqlQuery);
            } else if text.contains("system(") || text.contains("popen(") {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            } else if text.contains("fprintf(")
                || (text.contains("printf(")
                    && !text.contains("sprintf(")
                    && !text.contains("snprintf("))
            {
                self.sinks.insert(*node_id, TaintSink::HtmlRender);
            } else if text.contains("fwrite(") {
                self.sinks.insert(*node_id, TaintSink::FileWrite);
            }

            if text.contains("atoi(") || text.contains("strtol(") || text.contains("strtoul(") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::TypeCast("numeric".into()));
            } else if text.contains("snprintf(") && text.contains("%") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::SqlParameterize);
            }
        }
    }

    fn detect_cpp_patterns(&mut self) {
        self.detect_c_patterns();
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;

            if text.contains("std::getline") || text.contains("std::cin") {
                self.sources.insert(*node_id, TaintSource::HttpParameter);
            }
            if text.contains("std::system(") {
                self.sinks.insert(*node_id, TaintSink::ShellCommand);
            }
            if text.contains("std::stoi") || text.contains("std::stol") {
                self.sanitizers
                    .insert(*node_id, Sanitizer::TypeCast("numeric".into()));
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
                let sanitizers = self.find_dominating_sanitizers_on_path(&path, sink_id, &variable);
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

    /// Run taint analysis enforcing sanitizer dominance; fails on bypass paths.
    pub fn analyze_with_policy(&self) -> std::result::Result<Vec<TaintFlow>, PolicyViolation> {
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
                self.check_sanitizer_policy(sink_id, &path, &variable)?;
                let sanitizers = self.find_dominating_sanitizers_on_path(&path, sink_id, &variable);
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
        Ok(flows)
    }

    /// Validate sanitizer placement: must dominate sink and precede it in control flow.
    fn check_sanitizer_policy(
        &self,
        sink_id: PdgNodeId,
        path: &[PdgNodeId],
        variable: &str,
    ) -> std::result::Result<(), PolicyViolation> {
        let sink_block = self.pdg.nodes[&sink_id].block;
        let sink_line = self.pdg.nodes[&sink_id].statement.line;

        for node_id in path {
            if self.sanitizers.contains_key(node_id) {
                let san_block = self.pdg.nodes[node_id].block;
                if !self.dom_tree.dominates(san_block, sink_block) {
                    return Err(PolicyViolation::SanitizationBypass {
                        sink_line,
                        path_trace: path.to_vec(),
                        sanitizer_node: *node_id,
                    });
                }
            }
        }

        for &san_id in self.sanitizers.keys() {
            let node = &self.pdg.nodes[&san_id];
            if !self.node_affects_variable(node, variable) {
                continue;
            }
            let san_block = node.block;
            if node.statement.line > sink_line {
                return Err(PolicyViolation::SanitizationBypass {
                    sink_line,
                    path_trace: path.to_vec(),
                    sanitizer_node: san_id,
                });
            }
            if !self.dom_tree.dominates(san_block, sink_block) {
                return Err(PolicyViolation::SanitizationBypass {
                    sink_line,
                    path_trace: path.to_vec(),
                    sanitizer_node: san_id,
                });
            }
        }
        Ok(())
    }

    fn node_affects_variable(&self, node: &crate::pdg::PdgNode, variable: &str) -> bool {
        node.defined_vars.contains(variable)
            || node.used_vars.contains(variable)
            || node.statement.text.contains(&format!("int({variable})"))
            || node.statement.text.contains(&format!("int({variable} "))
    }

    /// Vulnerable flows only (no dominating sanitizers).
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

    fn find_dominating_sanitizers_on_path(
        &self,
        path: &[PdgNodeId],
        sink: PdgNodeId,
        variable: &str,
    ) -> Vec<Sanitizer> {
        let sink_block = self.pdg.nodes[&sink].block;
        let mut sanitizers = Vec::new();
        for node_id in path {
            if let Some(san) = self.sanitizers.get(node_id) {
                let san_block = self.pdg.nodes[node_id].block;
                if self.dom_tree.dominates(san_block, sink_block) {
                    sanitizers.push(san.clone());
                }
            }
            if let Some(ref engine) = self.type_inference {
                if let Some(typ) = engine.get_type(*node_id, variable) {
                    if self
                        .dom_tree
                        .dominates(self.pdg.nodes[node_id].block, sink_block)
                    {
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
    fn test_taint_rust_sqlx_env_flow() {
        let code = r#"
fn bad() {
    let id = std::env::var("ID").unwrap();
    sqlx::query(&format!("SELECT * FROM users WHERE id = {}", id)).execute(pool);
}
"#;
        let cfg = build_cfg_for_function("rust", code, "bad").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
        analyzer.detect_patterns("rust");
        let flows = analyzer.vulnerable_flows();
        assert!(!flows.is_empty(), "expected env -> sqlx SQL flow");
        assert_eq!(flows[0].source_type, TaintSource::EnvironmentVar);
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
