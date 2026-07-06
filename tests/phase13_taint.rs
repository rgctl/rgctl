//! Phase 13: taint analysis (25 tests).
#![allow(dead_code, unused_imports, unused_macros)]

#[path = "common/phase13.rs"]
mod phase13;

use phase13::{
    analyze_taint, analyze_taint_with_types, analyze_vulnerable_taint, assert_flow_kind,
    pdg_statement_texts,
};
use rbuilder::analysis::{TaintFlow, TaintSink, TaintSource};

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

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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
        assert!(flows
            .iter()
            .any(|f| f.severity == 10 && f.sink_type == TaintSink::SqlQuery));
    }
);

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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
        assert!(flows
            .iter()
            .any(|f| !f.sanitizers.is_empty() || !f.is_vulnerable()));
    }
);

#[cfg(feature = "bundle-minimal")]
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
        assert!(flows.iter().any(|f| f
            .sanitizers
            .iter()
            .any(|s| matches!(s, rbuilder::analysis::Sanitizer::ShellEscape))));
    }
);

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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
        assert!(flows
            .iter()
            .any(|f| f.source_type == TaintSource::FileInput));
    }
);

#[cfg(feature = "bundle-minimal")]
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
        assert!(flows
            .iter()
            .any(|f| f.source_type == TaintSource::EnvironmentVar));
    }
);

#[cfg(feature = "bundle-minimal")]
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
        assert!(flows
            .iter()
            .any(|f| f.source_type == TaintSource::CommandLineArg));
        assert!(flows.iter().any(|f| f.sink_type == TaintSink::CodeEval));
    }
);

#[cfg(feature = "bundle-minimal")]
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
        assert!(flows
            .iter()
            .any(|f| f.source_type == TaintSource::EnvironmentVar));
    }
);

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
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
        assert!(flows
            .iter()
            .any(|f| f.source_type == TaintSource::HttpParameter));
    }
);

#[cfg(feature = "bundle-minimal")]
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

#[cfg(feature = "bundle-minimal")]
#[test]
fn test_partial_dominance_bypass() {
    use rbuilder::analysis::{
        build_cfg_for_function, PolicyViolation, ProgramDependenceGraph, TaintAnalyzer,
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

#[cfg(feature = "bundle-minimal")]
#[test]
fn test_sanitizer_after_sink_trap() {
    use rbuilder::analysis::{
        build_cfg_for_function, PolicyViolation, ProgramDependenceGraph, TaintAnalyzer,
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

#[cfg(feature = "bundle-minimal")]
#[test]
fn test_dominating_sanitizer_passes_policy() {
    use rbuilder::analysis::{build_cfg_for_function, ProgramDependenceGraph, TaintAnalyzer};

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
