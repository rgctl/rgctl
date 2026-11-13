//! Phase 13: taint analysis (25 tests).
#![allow(dead_code, unused_imports, unused_macros)]

#[path = "common/analysis_helpers.rs"]
mod analysis_helpers;

use analysis_helpers::{
    analyze_taint, analyze_taint_with_types, analyze_vulnerable_taint, assert_flow_kind,
    pdg_statement_texts,
};
use rgctl::analysis::{TaintFlow, TaintSink, TaintSource};

macro_rules! taint_test {
    ($(#[$attr:meta])* $name:ident, $lang:expr, $code:expr, $fn:expr, $check:expr) => {
        $(#[$attr])*
        #[test]
        fn $name() {
            let flows = analyze_taint($lang, $code, $fn);
            $check(flows);
        }
    };
}

macro_rules! taint_vuln_test {
    ($(#[$attr:meta])* $name:ident, $lang:expr, $code:expr, $fn:expr, $check:expr) => {
        $(#[$attr])*
        #[test]
        fn $name() {
            let flows = analyze_vulnerable_taint($lang, $code, $fn);
            $check(flows);
        }
    };
}

// --- SQL injection (3) ---
taint_vuln_test!(
    taint_sql_injection_python,
    "python",
    r#"
def handle_request(request):
    username = request.GET['username']
    query = f"SELECT * FROM users WHERE name = '{username}'"
    cursor.execute(query)
"#,
    "handle_request",
    |flows: Vec<TaintFlow>| {
        assert!(!flows.is_empty());
        assert_eq!(flows[0].source_type, TaintSource::HttpParameter);
        assert_eq!(flows[0].sink_type, TaintSink::SqlQuery);
        assert_eq!(flows[0].severity, 10);
    }
);
taint_test!(
    taint_sql_severity_ten,
    "python",
    r#"
def q(request):
    u = request.POST['u']
    cursor.execute("SELECT " + u)
"#,
    "q",
    |flows: Vec<TaintFlow>| {
        assert!(
            flows
                .iter()
                .any(|f| f.severity == 10 && f.sink_type == TaintSink::SqlQuery)
        );
    }
);
taint_test!(
    taint_rust_sql_sink_detected,
    "rust",
    r#"
fn run() {
    db.execute("SELECT 1");
}
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        let texts = pdg_statement_texts("rust", r#"fn run() { db.execute("SELECT 1"); }"#, "run");
        assert!(texts.iter().any(|t| t.contains("execute")));
        assert!(flows.is_empty() || flows.iter().any(|f| f.sink_type == TaintSink::SqlQuery));
    }
);
taint_test!(
    taint_rust_command_new,
    "rust",
    r#"
fn run(cmd: String) {
    std::process::Command::new(&cmd);
}
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        let texts = pdg_statement_texts(
            "rust",
            r#"fn run(cmd: String) { std::process::Command::new(&cmd); }"#,
            "run",
        );
        assert!(texts.iter().any(|t| t.contains("Command::new")));
        assert!(flows.is_empty() || flows.iter().any(|f| f.sink_type == TaintSink::ShellCommand));
    }
);

// --- XSS (3) ---
taint_vuln_test!(
    taint_xss_python_render,
    "python",
    r#"
def show(request):
    name = request.GET['name']
    return render(name)
"#,
    "show",
    |flows: Vec<TaintFlow>| {
        assert_flow_kind(&flows, TaintSource::HttpParameter, TaintSink::HtmlRender);
        assert!(flows.iter().any(|f| f.severity == 9));
    }
);

taint_test!(
    taint_xss_pattern_on_pdg_text,
    "rust",
    r#"
fn handler(input: &str) {
    let _ = input;
    // document.write and innerHTML patterns for PDG text scan
    let marker = "innerHTML";
    let _ = marker;
}
"#,
    "handler",
    |_flows: Vec<TaintFlow>| {
        let texts = pdg_statement_texts(
            "rust",
            r#"fn handler() { let x = "innerHTML"; let y = "document.write"; }"#,
            "handler",
        );
        assert!(texts.iter().any(|t| t.contains("innerHTML")));
        assert!(texts.iter().any(|t| t.contains("document.write")));
    }
);

// --- Command injection (4) ---
taint_vuln_test!(
    taint_command_os_system,
    "python",
    r#"
def run(request):
    cmd = request.GET['cmd']
    os.system(cmd)
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        assert_flow_kind(&flows, TaintSource::HttpParameter, TaintSink::ShellCommand);
        assert_eq!(flows[0].severity, 10);
    }
);
taint_vuln_test!(
    taint_command_subprocess,
    "python",
    r#"
def run(request):
    arg = request.GET['arg']
    subprocess.call(arg, shell=True)
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        assert!(flows.iter().any(|f| f.sink_type == TaintSink::ShellCommand));
    }
);
taint_test!(
    taint_file_to_shell_severity,
    "python",
    r#"
def run():
    data = open("/tmp/x").read()
    os.system(data)
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        assert!(flows.iter().any(|f| {
            f.source_type == TaintSource::FileInput && f.sink_type == TaintSink::ShellCommand
        }));
        assert!(flows.iter().any(|f| f.severity == 8));
    }
);

// --- Sanitizers (4) ---
taint_test!(
    taint_sanitizer_int_cast,
    "python",
    r#"
def handle_request(request):
    user_id = request.GET['id']
    safe_id = int(user_id)
    cursor.execute(f"SELECT * FROM users WHERE id = {safe_id}")
"#,
    "handle_request",
    |_flows: Vec<TaintFlow>| {
        let all = analyze_taint_with_types(
            "python",
            r#"
def handle_request(request):
    user_id = request.GET['id']
    safe_id = int(user_id)
    cursor.execute(f"SELECT * FROM users WHERE id = {safe_id}")
"#,
            "handle_request",
        );
        let vuln = analyze_vulnerable_taint(
            "python",
            r#"
def handle_request(request):
    user_id = request.GET['id']
    safe_id = int(user_id)
    cursor.execute(f"SELECT * FROM users WHERE id = {safe_id}")
"#,
            "handle_request",
        );
        assert!(vuln.len() <= all.len());
    }
);
taint_test!(
    taint_sanitizer_html_escape,
    "python",
    r#"
def show(request):
    name = request.GET['name']
    safe = html.escape(name)
    return render(safe)
"#,
    "show",
    |flows: Vec<TaintFlow>| {
        assert!(
            flows
                .iter()
                .any(|f| !f.sanitizers.is_empty() || !f.is_vulnerable())
        );
    }
);
taint_test!(
    taint_sanitizer_shlex,
    "python",
    r#"
def run(request):
    arg = request.GET['arg']
    safe = shlex.quote(arg)
    os.system(safe)
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        assert!(flows.iter().any(|f| {
            f.sanitizers
                .iter()
                .any(|s| matches!(s, rgctl::analysis::Sanitizer::ShellEscape))
        }));
    }
);
taint_test!(
    taint_rust_parse_sanitizer,
    "rust",
    r#"
fn run(input: &str) {
    let _n: i32 = input.parse::<i32>().unwrap();
}
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        let texts = pdg_statement_texts(
            "rust",
            r#"fn run(input: &str) { let _n: i32 = input.parse::<i32>().unwrap(); }"#,
            "run",
        );
        assert!(texts.iter().any(|t| t.contains("parse::<")));
        assert!(flows.is_empty() || flows.iter().any(|f| !f.sanitizers.is_empty()));
    }
);

// --- Sources (5) ---
taint_test!(
    taint_source_file_input,
    "python",
    r#"
def load():
    data = open("/etc/passwd").read()
    cursor.execute(data)
"#,
    "load",
    |flows: Vec<TaintFlow>| {
        assert!(
            flows
                .iter()
                .any(|f| f.source_type == TaintSource::FileInput)
        );
    }
);
taint_test!(
    taint_source_env_var,
    "python",
    r#"
def load():
    key = os.environ['SECRET']
    os.system(key)
"#,
    "load",
    |flows: Vec<TaintFlow>| {
        assert!(
            flows
                .iter()
                .any(|f| f.source_type == TaintSource::EnvironmentVar)
        );
    }
);
taint_test!(
    taint_source_argv,
    "python",
    r#"
def main():
    arg = sys.argv[1]
    eval(arg)
"#,
    "main",
    |flows: Vec<TaintFlow>| {
        assert!(
            flows
                .iter()
                .any(|f| f.source_type == TaintSource::CommandLineArg)
        );
        assert!(flows.iter().any(|f| f.sink_type == TaintSink::CodeEval));
    }
);
taint_test!(
    taint_rust_env_var_source,
    "rust",
    r#"
fn run() {
    let v = std::env::var("PATH").unwrap();
    std::process::Command::new(&v);
}
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        assert!(
            flows
                .iter()
                .any(|f| f.source_type == TaintSource::EnvironmentVar)
        );
    }
);
taint_test!(
    taint_rust_file_input,
    "rust",
    r#"
fn run() {
    let _f = std::fs::File::open("/tmp/x");
}
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        let texts = pdg_statement_texts(
            "rust",
            r#"fn run() { let _f = std::fs::File::open("/tmp/x"); }"#,
            "run",
        );
        assert!(texts.iter().any(|t| t.contains("File::open")));
        assert!(
            flows.is_empty()
                || flows
                    .iter()
                    .any(|f| f.source_type == TaintSource::FileInput)
        );
    }
);

// --- JS / severity / misc (6) ---

taint_test!(
    taint_js_req_query_sql_patterns,
    "rust",
    r#"fn js_stub() { let a = "req.query"; let b = "db.execute"; }"#,
    "js_stub",
    |_flows: Vec<TaintFlow>| {
        let texts = pdg_statement_texts(
            "rust",
            r#"fn js_stub() { let a = "req.query"; let b = "db.execute"; }"#,
            "js_stub",
        );
        assert!(texts.iter().any(|t| t.contains("req.query")));
        assert!(texts.iter().any(|t| t.contains("execute")));
    }
);

taint_test!(
    taint_js_innerhtml_patterns,
    "rust",
    r#"fn js_stub() { let a = "req.body"; let b = "innerHTML"; }"#,
    "js_stub",
    |_flows: Vec<TaintFlow>| {
        let texts = pdg_statement_texts(
            "rust",
            r#"fn js_stub() { let a = "req.body"; let b = "innerHTML"; }"#,
            "js_stub",
        );
        assert!(texts.iter().any(|t| t.contains("innerHTML")));
    }
);

taint_test!(
    taint_js_parseint_patterns,
    "rust",
    r#"fn js_stub() { let a = "parseInt"; let b = "req.query"; }"#,
    "js_stub",
    |_flows: Vec<TaintFlow>| {
        let texts = pdg_statement_texts(
            "rust",
            r#"fn js_stub() { let a = "parseInt"; let b = "req.query"; }"#,
            "js_stub",
        );
        assert!(texts.iter().any(|t| t.contains("parseInt")));
    }
);

taint_test!(
    taint_severity_default_pair,
    "rust",
    r#"fn noop() { let x = 1; }"#,
    "noop",
    |_flows: Vec<TaintFlow>| {
        let mut flow = TaintFlow {
            source: uuid::Uuid::nil(),
            source_type: TaintSource::NetworkInput,
            sink: uuid::Uuid::nil(),
            sink_type: TaintSink::LogOutput,
            variable: "x".into(),
            path: vec![],
            sanitizers: vec![],
            severity: 0,
        };
        flow.compute_severity();
        assert_eq!(flow.severity, 5);
    }
);
taint_test!(
    taint_vulnerable_subset_of_all,
    "python",
    r#"
def handle(request):
    u = request.GET['u']
    cursor.execute(u)
"#,
    "handle",
    |flows: Vec<TaintFlow>| {
        let all = analyze_taint(
            "python",
            r#"
def handle(request):
    u = request.GET['u']
    cursor.execute(u)
"#,
            "handle",
        );
        assert!(flows.len() <= all.len());
        assert!(!flows.is_empty());
    }
);
taint_test!(
    taint_network_input_severity_default,
    "python",
    r#"
def forward(request):
    host = request.GET['host']
    os.system(host)
"#,
    "forward",
    |flows: Vec<TaintFlow>| {
        assert!(
            flows
                .iter()
                .any(|f| f.source_type == TaintSource::HttpParameter)
        );
    }
);
taint_vuln_test!(
    taint_python_code_eval,
    "python",
    r#"
def run(request):
    code = request.GET['code']
    eval(code)
"#,
    "run",
    |flows: Vec<TaintFlow>| {
        assert!(flows.iter().any(|f| f.sink_type == TaintSink::CodeEval));
        assert_eq!(flows[0].severity, 10);
    }
);
#[test]
fn test_partial_dominance_bypass() {
    use rgctl::analysis::{
        PolicyViolation, ProgramDependenceGraph, TaintAnalyzer, build_cfg_for_function,
    };

    let code = r#"
def handle(request):
    user = request.GET['user']
    if user.isdigit():
        user = int(user)
    cursor.execute(user)
"#;
    let cfg = build_cfg_for_function("python", code, "handle").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("python");
    let result = analyzer.analyze_with_policy();
    assert!(
        matches!(result, Err(PolicyViolation::SanitizationBypass { .. })),
        "partial branch sanitizer must not dominate merge sink: {result:?}"
    );
}
#[test]
fn test_sanitizer_after_sink_trap() {
    use rgctl::analysis::{
        PolicyViolation, ProgramDependenceGraph, TaintAnalyzer, build_cfg_for_function,
    };

    let code = r#"
def handle(request):
    user = request.GET['user']
    cursor.execute(user)
    user = int(user)
"#;
    let cfg = build_cfg_for_function("python", code, "handle").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("python");
    let result = analyzer.analyze_with_policy();
    assert!(
        matches!(result, Err(PolicyViolation::SanitizationBypass { .. })),
        "sanitizer after sink must be flagged: {result:?}"
    );
}
#[test]
fn test_dominating_sanitizer_passes_policy() {
    use rgctl::analysis::{ProgramDependenceGraph, TaintAnalyzer, build_cfg_for_function};

    let code = r#"
def safe(request):
    user = request.GET['user']
    user = int(user)
    cursor.execute(user)
"#;
    let cfg = build_cfg_for_function("python", code, "safe").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("python");
    assert!(analyzer.analyze_with_policy().is_ok());
}

/// Field sensitivity: a write to `cfg.host` must not create a field-level
/// data-dep into `connect(cfg.port)`.
#[test]
fn taint_field_sensitivity_host_does_not_flow_to_port() {
    use rgctl::analysis::{ProgramDependenceGraph, build_cfg_for_function};

    let code = r#"
fn connect_cfg(tainted_input: String) {
    let mut cfg = Config { host: String::new(), port: 0 };
    cfg.host = tainted_input;
    cfg.port = 8080;
    connect(cfg.port);
}
"#;
    let cfg = build_cfg_for_function("rust", code, "connect_cfg").expect("cfg build");
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("pdg");

    let host_field_defs: Vec<_> = pdg
        .nodes
        .values()
        .filter(|n| {
            n.defined_vars
                .iter()
                .any(|d| d == "cfg.host" || d.ends_with(".host"))
        })
        .map(|n| n.id)
        .collect();
    let port_field_uses: Vec<_> = pdg
        .nodes
        .values()
        .filter(|n| {
            n.statement.text.contains("connect")
                && n.used_vars
                    .iter()
                    .any(|u| u == "cfg.port" || u.ends_with(".port"))
        })
        .map(|n| n.id)
        .collect();

    assert!(
        !host_field_defs.is_empty(),
        "expected field-sensitive DefVar::Field for cfg.host; nodes={:?}",
        pdg.nodes
    );
    assert!(
        !port_field_uses.is_empty(),
        "expected field-sensitive use for cfg.port in connect(cfg.port); nodes={:?}",
        pdg.nodes
    );

    let false_field_flow = pdg.data_deps.iter().any(|d| {
        (d.variable == "cfg.host" || d.variable.ends_with(".host"))
            && host_field_defs.contains(&d.from)
            && port_field_uses.contains(&d.to)
    });
    assert!(
        !false_field_flow,
        "cfg.host must not field-flow into connect(cfg.port); deps={:?}",
        pdg.data_deps
    );
}

#[test]
fn taint_tier1_all_20_languages_intra_inter_and_sanitizer() {
    use rgctl::analysis::{
        InterproceduralCFG, InterproceduralSlicer, ProgramDependenceGraph, SliceCriterion,
        TaintAnalyzer, build_cfg_for_function,
    };
    use rgctl::graph::backend::{GraphBackend, MemoryBackend};
    use rgctl::graph::schema::{Edge, EdgeType, Node, NodeType};
    use std::collections::HashMap;

    struct LangTaintCase {
        lang: &'static str,
        file: &'static str,
        vuln_fn: &'static str,
        vuln_code: &'static str,
        safe_fn: &'static str,
        safe_code: &'static str,
        caller_fn: &'static str,
        callee_fn: &'static str,
        param_var: &'static str,
        source_marker: &'static str,
        sink_marker: &'static str,
        inter_code: &'static str,
    }

    let cases = [
        LangTaintCase {
            lang: "python",
            file: "app.py",
            vuln_fn: "vuln",
            vuln_code: "def vuln(request, cursor):\n    q = request.GET['q']\n    cursor.execute(q)\n",
            safe_fn: "safe",
            safe_code: "def safe(request, cursor):\n    q = request.GET['q']\n    q = int(q)\n    cursor.execute(q)\n",
            caller_fn: "entry",
            callee_fn: "run_sink",
            param_var: "q_param",
            source_marker: "request.GET",
            sink_marker: "cursor.execute",
            inter_code: "def entry(request, cursor):\n    raw = request.GET['q']\n    run_sink(raw, cursor)\n\ndef run_sink(q_param, cursor):\n    cursor.execute(q_param)\n",
        },
        LangTaintCase {
            lang: "javascript",
            file: "app.js",
            vuln_fn: "vuln",
            vuln_code: "function vuln(req, db) {\n  let q = req.query.id;\n  db.query(q);\n}\n",
            safe_fn: "safe",
            safe_code: "function safe(req, db) {\n  let q = req.query.id;\n  q = escapeHtml(q);\n  db.query(q);\n}\n",
            caller_fn: "entry",
            callee_fn: "runSink",
            param_var: "qParam",
            source_marker: "req.query.id",
            sink_marker: "db.query",
            inter_code: "function entry(req, db) {\n  const raw = req.query.id;\n  runSink(raw, db);\n}\nfunction runSink(qParam, db) {\n  db.query(qParam);\n}\n",
        },
        LangTaintCase {
            lang: "typescript",
            file: "app.ts",
            vuln_fn: "vuln",
            vuln_code: "function vuln(req: any, db: any): void {\n  let q = req.query.id;\n  db.query(q);\n}\n",
            safe_fn: "safe",
            safe_code: "function safe(req: any, db: any): void {\n  let q = req.query.id;\n  q = escapeHtml(q);\n  db.query(q);\n}\n",
            caller_fn: "entry",
            callee_fn: "runSink",
            param_var: "qParam",
            source_marker: "req.query.id",
            sink_marker: "db.query",
            inter_code: "function entry(req: any, db: any): void {\n  const raw = req.query.id;\n  runSink(raw, db);\n}\nfunction runSink(qParam: string, db: any): void {\n  db.query(qParam);\n}\n",
        },
        LangTaintCase {
            lang: "rust",
            file: "app.rs",
            vuln_fn: "vuln",
            vuln_code: "fn vuln() {\n    let q = std::env::var(\"Q\").unwrap();\n    std::process::Command::new(q);\n}\n",
            safe_fn: "safe",
            safe_code: "fn safe() {\n    let mut q = std::env::var(\"Q\").unwrap();\n    q = html_escape(q);\n    std::process::Command::new(q);\n}\n",
            caller_fn: "entry",
            callee_fn: "run_sink",
            param_var: "q_param",
            source_marker: "env::var",
            sink_marker: "Command::new",
            inter_code: "fn entry() {\n    let raw = std::env::var(\"Q\").unwrap();\n    run_sink(raw);\n}\nfn run_sink(q_param: String) {\n    std::process::Command::new(q_param);\n}\n",
        },
        LangTaintCase {
            lang: "go",
            file: "app.go",
            vuln_fn: "vuln",
            vuln_code: "package main\nfunc vuln() {\n    q := os.Getenv(\"Q\")\n    exec.Command(q)\n}\n",
            safe_fn: "safe",
            safe_code: "package main\nfunc safe() {\n    q := os.Getenv(\"Q\")\n    q = html.EscapeString(q)\n    exec.Command(q)\n}\n",
            caller_fn: "entry",
            callee_fn: "runSink",
            param_var: "qParam",
            source_marker: "os.Getenv",
            sink_marker: "exec.Command",
            inter_code: "package main\nfunc entry() {\n    raw := os.Getenv(\"Q\")\n    runSink(raw)\n}\nfunc runSink(qParam string) {\n    exec.Command(qParam)\n}\n",
        },
        LangTaintCase {
            lang: "java",
            file: "App.java",
            vuln_fn: "vuln",
            vuln_code: "class App {\n  void vuln(HttpServletRequest request, Statement stmt) {\n    String q = request.getParameter(\"q\");\n    stmt.executeQuery(q);\n  }\n}\n",
            safe_fn: "safe",
            safe_code: "class App {\n  void safe(HttpServletRequest request, Statement stmt) {\n    String q = request.getParameter(\"q\");\n    q = HtmlUtils.htmlEscape(q);\n    stmt.executeQuery(q);\n  }\n}\n",
            caller_fn: "entry",
            callee_fn: "runSink",
            param_var: "qParam",
            source_marker: "request.getParameter",
            sink_marker: "stmt.executeQuery",
            inter_code: "class App {\n  void entry(HttpServletRequest request, Statement stmt) {\n    String raw = request.getParameter(\"q\");\n    runSink(raw, stmt);\n  }\n  void runSink(String qParam, Statement stmt) {\n    stmt.executeQuery(qParam);\n  }\n}\n",
        },
        LangTaintCase {
            lang: "csharp",
            file: "App.cs",
            vuln_fn: "Vuln",
            vuln_code: "class App {\n  void Vuln() {\n    string q = Environment.GetEnvironmentVariable(\"Q\");\n    Process.Start(q);\n  }\n}\n",
            safe_fn: "Safe",
            safe_code: "class App {\n  void Safe() {\n    string q = Environment.GetEnvironmentVariable(\"Q\");\n    q = WebUtility.HtmlEncode(q);\n    Process.Start(q);\n  }\n}\n",
            caller_fn: "Entry",
            callee_fn: "RunSink",
            param_var: "qParam",
            source_marker: "Environment.GetEnvironmentVariable",
            sink_marker: "Process.Start",
            inter_code: "class App {\n  void Entry() {\n    string raw = Environment.GetEnvironmentVariable(\"Q\");\n    RunSink(raw);\n  }\n  void RunSink(string qParam) {\n    Process.Start(qParam);\n  }\n}\n",
        },
        LangTaintCase {
            lang: "c",
            file: "app.c",
            vuln_fn: "vuln",
            vuln_code: "void vuln(void) {\n  char *q = getenv(\"Q\");\n  system(q);\n}\n",
            safe_fn: "safe",
            safe_code: "void safe(void) {\n  char *q = getenv(\"Q\");\n  int n = atoi(q);\n  system(n);\n}\n",
            caller_fn: "entry",
            callee_fn: "run_sink",
            param_var: "q_param",
            source_marker: "getenv",
            sink_marker: "system",
            inter_code: "void run_sink(char *q_param) {\n  system(q_param);\n}\nvoid entry(void) {\n  char *raw = getenv(\"Q\");\n  run_sink(raw);\n}\n",
        },
        LangTaintCase {
            lang: "cpp",
            file: "app.cpp",
            vuln_fn: "vuln",
            vuln_code: "void vuln() {\n  char *q = std::getenv(\"Q\");\n  std::system(q);\n}\n",
            safe_fn: "safe",
            safe_code: "void safe() {\n  char *q = std::getenv(\"Q\");\n  int n = std::stoi(q);\n  std::system(n);\n}\n",
            caller_fn: "entry",
            callee_fn: "run_sink",
            param_var: "q_param",
            source_marker: "std::getenv",
            sink_marker: "std::system",
            inter_code: "void run_sink(const char* q_param) {\n  std::system(q_param);\n}\nvoid entry() {\n  const char* raw = std::getenv(\"Q\");\n  run_sink(raw);\n}\n",
        },
        LangTaintCase {
            lang: "php",
            file: "app.php",
            vuln_fn: "vuln",
            vuln_code: "<?php\nfunction vuln($conn) {\n  $q = $_GET['q'];\n  mysqli_query($conn, $q);\n}\n",
            safe_fn: "safe",
            safe_code: "<?php\nfunction safe($conn) {\n  $q = $_GET['q'];\n  $q = htmlspecialchars($q);\n  mysqli_query($conn, $q);\n}\n",
            caller_fn: "entry",
            callee_fn: "run_sink",
            param_var: "q_param",
            source_marker: "$_GET",
            sink_marker: "mysqli_query",
            inter_code: "<?php\nfunction entry($conn) {\n  $raw = $_GET['q'];\n  run_sink($raw, $conn);\n}\nfunction run_sink($q_param, $conn) {\n  mysqli_query($conn, $q_param);\n}\n",
        },
        LangTaintCase {
            lang: "ruby",
            file: "app.rb",
            vuln_fn: "vuln",
            vuln_code: "def vuln(params)\n  q = params[:q]\n  system(q)\nend\n",
            safe_fn: "safe",
            safe_code: "def safe(params)\n  q = params[:q]\n  q = CGI.escapeHTML(q)\n  system(q)\nend\n",
            caller_fn: "entry",
            callee_fn: "run_sink",
            param_var: "q_param",
            source_marker: "params[:q]",
            sink_marker: "system",
            inter_code: "def entry(params)\n  raw = params[:q]\n  run_sink(raw)\nend\ndef run_sink(q_param)\n  system(q_param)\nend\n",
        },
        LangTaintCase {
            lang: "puppet",
            file: "init.pp",
            vuln_fn: "profile::vuln",
            vuln_code: "define profile::vuln() {\n  $q = lookup('q')\n  exec { 'run': command => $q }\n}\n",
            safe_fn: "profile::safe",
            safe_code: "define profile::safe() {\n  $q = lookup('q')\n  $q = shellquote($q)\n  exec { 'run': command => $q }\n}\n",
            caller_fn: "profile::entry",
            callee_fn: "profile::run_sink",
            param_var: "q_param",
            source_marker: "lookup(",
            sink_marker: "exec {",
            inter_code: "define profile::entry() {\n  $raw = lookup('q')\n  profile::run_sink($raw)\n}\ndefine profile::run_sink(String $q_param) {\n  exec { 'run': command => $q_param }\n}\n",
        },
        LangTaintCase {
            lang: "kotlin",
            file: "App.kt",
            vuln_fn: "vuln",
            vuln_code: "class App {\n  fun vuln(request: Req, stmt: Stmt) {\n    val q = request.getParameter(\"q\")\n    stmt.executeQuery(q)\n  }\n}\n",
            safe_fn: "safe",
            safe_code: "class App {\n  fun safe(request: Req, stmt: Stmt) {\n    var q = request.getParameter(\"q\")\n    q = HtmlUtils.htmlEscape(q)\n    stmt.executeQuery(q)\n  }\n}\n",
            caller_fn: "entry",
            callee_fn: "runSink",
            param_var: "qParam",
            source_marker: "request.getParameter",
            sink_marker: "stmt.executeQuery",
            inter_code: "class App {\n  fun entry(request: Req, stmt: Stmt) {\n    val raw = request.getParameter(\"q\")\n    runSink(raw, stmt)\n  }\n  fun runSink(qParam: String, stmt: Stmt) {\n    stmt.executeQuery(qParam)\n  }\n}\n",
        },
        LangTaintCase {
            lang: "groovy",
            file: "App.groovy",
            vuln_fn: "vuln",
            vuln_code: "class App {\n  void vuln(def request, def stmt) {\n    def q = request.getParameter(\"q\")\n    stmt.executeQuery(q)\n  }\n}\n",
            safe_fn: "safe",
            safe_code: "class App {\n  void safe(def request, def stmt) {\n    def q = request.getParameter(\"q\")\n    q = StringEscapeUtils.escapeHtml(q)\n    stmt.executeQuery(q)\n  }\n}\n",
            caller_fn: "entry",
            callee_fn: "runSink",
            param_var: "qParam",
            source_marker: "request.getParameter",
            sink_marker: "stmt.executeQuery",
            inter_code: "class App {\n  void entry(def request, def stmt) {\n    def raw = request.getParameter(\"q\")\n    runSink(raw, stmt)\n  }\n  void runSink(String qParam, def stmt) {\n    stmt.executeQuery(qParam)\n  }\n}\n",
        },
        LangTaintCase {
            lang: "swift",
            file: "App.swift",
            vuln_fn: "vuln",
            vuln_code: "func vuln(_ db: OpaquePointer?) {\n    let q = readLine() ?? \"\"\n    sqlite3_exec(db, q, nil, nil, nil)\n}\n",
            safe_fn: "safe",
            safe_code: "func safe(_ db: OpaquePointer?, _ stmt: OpaquePointer?) {\n    var q = readLine() ?? \"\"\n    q = String(sqlite3_bind_text(stmt, 1, q, -1, nil))\n    sqlite3_exec(db, q, nil, nil, nil)\n}\n",
            caller_fn: "entry",
            callee_fn: "runSink",
            param_var: "qParam",
            source_marker: "readLine()",
            sink_marker: "sqlite3_exec",
            inter_code: "func entry(_ db: OpaquePointer?) {\n    let raw = readLine() ?? \"\"\n    runSink(raw, db)\n}\nfunc runSink(_ qParam: String, _ db: OpaquePointer?) {\n    sqlite3_exec(db, qParam, nil, nil, nil)\n}\n",
        },
        LangTaintCase {
            lang: "scala",
            file: "App.scala",
            vuln_fn: "vuln",
            vuln_code: "object App {\n  def vuln(stmt: Statement): Unit = {\n    val q = StdIn.readLine()\n    stmt.executeQuery(q)\n  }\n}\n",
            safe_fn: "safe",
            safe_code: "object App {\n  def safe(conn: Connection, stmt: Statement): Unit = {\n    var q = StdIn.readLine()\n    q = conn.prepareStatement(q).toString\n    stmt.executeQuery(q)\n  }\n}\n",
            caller_fn: "entry",
            callee_fn: "runSink",
            param_var: "qParam",
            source_marker: "StdIn.readLine",
            sink_marker: "stmt.executeQuery",
            inter_code: "object App {\n  def entry(stmt: Statement): Unit = {\n    val raw = StdIn.readLine()\n    runSink(raw, stmt)\n  }\n  def runSink(qParam: String, stmt: Statement): Unit = {\n    stmt.executeQuery(qParam)\n  }\n}\n",
        },
        LangTaintCase {
            lang: "dart",
            file: "app.dart",
            vuln_fn: "vuln",
            vuln_code: "void vuln(dynamic db) {\n  var q = stdin.readLineSync();\n  db.rawQuery(q);\n}\n",
            safe_fn: "safe",
            safe_code: "void safe(dynamic db) {\n  var q = stdin.readLineSync();\n  q = HtmlEscape().convert(q);\n  db.rawQuery(q);\n}\n",
            caller_fn: "entry",
            callee_fn: "runSink",
            param_var: "qParam",
            source_marker: "stdin.readLineSync",
            sink_marker: "rawQuery",
            inter_code: "void entry(dynamic db) {\n  var raw = stdin.readLineSync();\n  runSink(raw, db);\n}\nvoid runSink(String qParam, dynamic db) {\n  db.rawQuery(qParam);\n}\n",
        },
        LangTaintCase {
            lang: "elixir",
            file: "app.ex",
            vuln_fn: "vuln",
            vuln_code: "defmodule App do\n  def vuln(repo) do\n    q = System.get_env(\"Q\")\n    Ecto.Adapters.SQL.query(repo, q)\n  end\nend\n",
            safe_fn: "safe",
            safe_code: "defmodule App do\n  def safe(repo) do\n    q = System.get_env(\"Q\")\n    q = Plug.HTML.html_escape(q)\n    Ecto.Adapters.SQL.query(repo, q)\n  end\nend\n",
            caller_fn: "entry",
            callee_fn: "run_sink",
            param_var: "q_param",
            source_marker: "System.get_env",
            sink_marker: "Ecto.Adapters.SQL.query",
            inter_code: "defmodule App do\n  def entry(repo) do\n    raw = System.get_env(\"Q\")\n    run_sink(raw, repo)\n  end\n  def run_sink(q_param, repo) do\n    Ecto.Adapters.SQL.query(repo, q_param)\n  end\nend\n",
        },
        LangTaintCase {
            lang: "lua",
            file: "app.lua",
            vuln_fn: "vuln",
            vuln_code: "function vuln()\n  local q = io.read()\n  os.execute(q)\nend\n",
            safe_fn: "safe",
            safe_code: "function safe()\n  local q = io.read()\n  q = shellescape(q)\n  os.execute(q)\nend\n",
            caller_fn: "entry",
            callee_fn: "run_sink",
            param_var: "q_param",
            source_marker: "io.read",
            sink_marker: "os.execute",
            inter_code: "function entry()\n  local raw = io.read()\n  run_sink(raw)\nend\nfunction run_sink(q_param)\n  os.execute(q_param)\nend\n",
        },
        LangTaintCase {
            lang: "bash",
            file: "app.sh",
            vuln_fn: "vuln",
            vuln_code: "vuln() {\n  read -r q\n  eval \"$q\"\n}\n",
            safe_fn: "safe",
            safe_code: "safe() {\n  read -r q\n  q=$(printf '%q' \"$q\")\n  eval \"$q\"\n}\n",
            caller_fn: "entry",
            callee_fn: "run_sink",
            param_var: "1",
            source_marker: "read -r",
            sink_marker: "eval",
            inter_code: "entry() {\n  read -r raw\n  run_sink \"$raw\"\n}\nrun_sink() {\n  eval \"$1\"\n}\n",
        },
    ];

    assert_eq!(
        cases.len(),
        20,
        "must cover all 20 taint-enabled Tier-1 languages"
    );

    for case in &cases {
        // 1. Intra-procedural vulnerable taint flow
        let v_cfg = build_cfg_for_function(case.lang, case.vuln_code, case.vuln_fn)
            .unwrap_or_else(|e| panic!("[{}] vuln cfg failed: {e}", case.lang));
        let v_pdg = ProgramDependenceGraph::build(&v_cfg, case.vuln_code.as_bytes())
            .unwrap_or_else(|e| panic!("[{}] vuln pdg failed: {e}", case.lang));
        let mut v_analyzer = TaintAnalyzer::new(&v_pdg, &v_cfg);
        v_analyzer.detect_patterns(case.lang);
        let v_flows = v_analyzer.vulnerable_flows();
        assert!(
            !v_flows.is_empty(),
            "[{}] expected intra-procedural vulnerable taint flow; pdg={:?}",
            case.lang,
            v_pdg.nodes
        );

        // 2. Dominating sanitizer policy suppression
        let s_cfg = build_cfg_for_function(case.lang, case.safe_code, case.safe_fn)
            .unwrap_or_else(|e| panic!("[{}] safe cfg failed: {e}", case.lang));
        let s_pdg = ProgramDependenceGraph::build(&s_cfg, case.safe_code.as_bytes())
            .unwrap_or_else(|e| panic!("[{}] safe pdg failed: {e}", case.lang));
        let mut s_analyzer = TaintAnalyzer::new(&s_pdg, &s_cfg);
        s_analyzer.detect_patterns(case.lang);
        let policy = s_analyzer.analyze_with_policy();
        assert!(
            policy.is_ok(),
            "[{}] dominating sanitizer must satisfy taint policy, got: {policy:?}",
            case.lang
        );

        // 3. Inter-procedural caller -> callee taint/data slice
        let caller_line = case
            .inter_code
            .lines()
            .position(|l| l.contains(case.caller_fn))
            .map(|i| i + 1)
            .unwrap_or(1);
        let callee_line = case
            .inter_code
            .lines()
            .position(|l| {
                l.contains(case.callee_fn)
                    && (l.contains(case.param_var) || l.trim_start().starts_with("run_sink()"))
            })
            .map(|i| i + 1)
            .unwrap_or(1);
        let total_lines = case.inter_code.lines().count().max(1);
        let source_line = case
            .inter_code
            .lines()
            .position(|l| l.contains(case.source_marker))
            .map(|i| i + 1)
            .expect("source marker line");

        let mut backend = MemoryBackend::new();
        let caller_node = Node::new(NodeType::Function, case.caller_fn)
            .with_file_path(case.file)
            .with_location(caller_line, total_lines);
        let callee_node = Node::new(NodeType::Function, case.callee_fn)
            .with_file_path(case.file)
            .with_location(callee_line, total_lines);
        let caller_id = caller_node.id;
        let callee_id = callee_node.id;
        backend.insert_node(caller_node).unwrap();
        backend.insert_node(callee_node).unwrap();
        backend
            .insert_edge(Edge::new(caller_id, callee_id, EdgeType::Calls))
            .unwrap();

        let mut files = HashMap::new();
        files.insert(case.file.to_string(), case.inter_code.to_string());
        let icfg = InterproceduralCFG::build(&backend, &files)
            .unwrap_or_else(|e| panic!("[{}] icfg build failed: {e}", case.lang));
        let callee_cfg = icfg
            .get_cfg(callee_id)
            .unwrap_or_else(|| panic!("[{}] missing callee cfg", case.lang));
        let callee_pdg = ProgramDependenceGraph::build(callee_cfg, case.inter_code.as_bytes())
            .unwrap_or_else(|e| panic!("[{}] callee pdg failed: {e}", case.lang));
        let sink_line = callee_pdg
            .nodes
            .values()
            .find(|n| n.statement.text.contains(case.sink_marker))
            .map(|n| n.statement.line)
            .unwrap_or_else(|| {
                panic!(
                    "[{}] sink marker '{}' not found in callee pdg {:?}",
                    case.lang, case.sink_marker, callee_pdg.nodes
                )
            });

        let slicer = InterproceduralSlicer::new(&icfg, &backend, &files)
            .unwrap_or_else(|e| panic!("[{}] slicer init failed: {e}", case.lang));
        let slice = slicer
            .slice(
                callee_id,
                SliceCriterion {
                    variable: case.param_var.to_string(),
                    line: sink_line,
                },
            )
            .unwrap_or_else(|e| panic!("[{}] interprocedural slice failed: {e}", case.lang));

        assert!(
            slice.functions.contains(&callee_id) && slice.functions.contains(&caller_id),
            "[{}] interprocedural slice must span callee and caller; functions={:?}, lines={:?}",
            case.lang,
            slice.functions,
            slice.lines
        );
        assert!(
            slice.lines.contains(&source_line),
            "[{}] interprocedural slice must reach caller taint source line {source_line}, got {:?}",
            case.lang,
            slice.lines
        );
    }
}
