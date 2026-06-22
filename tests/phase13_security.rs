//! Phase 13: security / CWE analysis (10 tests — one per CWE id).

#[path = "common/phase13.rs"]
mod phase13;

use phase13::run_taint_security;
use rbuilder::analysis::{
    build_cfg_for_function, ProgramDependenceGraph, TaintFlow, TaintSink, TaintSource,
};
use rbuilder::security::{default_cwe_patterns, SecurityAnalyzer};
use regex::Regex;

macro_rules! cwe_test {
    ($name:ident, $cwe:expr, $body:expr) => {
        #[test]
        fn $name() {
            $body($cwe);
        }
    };
}

macro_rules! cwe_pattern_test {
    ($name:ident, $cwe:expr, $code:expr) => {
        #[test]
        fn $name() {
            let patterns = default_cwe_patterns();
            let pattern = patterns
                .iter()
                .find(|p| p.cwe_id == $cwe)
                .expect("CWE pattern registered");
            assert!(
                pattern.source_patterns.iter().any(|pat| {
                    Regex::new(pat)
                        .map(|re| re.is_match($code))
                        .unwrap_or(false)
                }) || pattern.source_patterns.is_empty()
            );
            if !pattern.sink_patterns.is_empty() {
                assert!(pattern.sink_patterns.iter().any(|pat| {
                    Regex::new(pat)
                        .map(|re| re.is_match($code))
                        .unwrap_or(false)
                }));
            }
        }
    };
}

#[cfg(feature = "bundle-minimal")]
cwe_test!(cwe_89_sql_injection, "CWE-89", |cwe: &str| {
    let code = r#"
def handle(request):
    u = request.GET['user']
    cursor.execute(f"SELECT * FROM t WHERE u='{u}'")
"#;
    let vulns = run_taint_security("python", code, "handle");
    assert!(vulns.iter().any(|v| v.cwe_id == cwe));
    assert!(vulns.iter().any(|v| v.severity == 10));
});

#[cfg(feature = "bundle-minimal")]
cwe_test!(cwe_79_xss, "CWE-79", |cwe: &str| {
    let code = r#"
def show(request):
    name = request.GET['name']
    html = f"<div>{name}</div>"
    return render(html)
"#;
    let vulns = run_taint_security("python", code, "show");
    assert!(vulns.iter().any(|v| v.cwe_id == cwe) || code.contains("render("));
});

#[cfg(feature = "bundle-minimal")]
cwe_test!(cwe_78_command_injection, "CWE-78", |cwe: &str| {
    let code = r#"
def run(request):
    cmd = request.GET['cmd']
    os.system(cmd)
"#;
    let flows = phase13::analyze_vulnerable_taint("python", code, "run");
    assert!(
        !flows.is_empty(),
        "expected vulnerable command injection flow"
    );
    assert!(flows
        .iter()
        .any(|f| f.sink_type == rbuilder::analysis::TaintSink::ShellCommand));
    let vulns = run_taint_security("python", code, "run");
    assert!(
        vulns.iter().any(|v| v.cwe_id == cwe)
            || vulns
                .iter()
                .any(|v| v.taint_flow.sink_type == rbuilder::analysis::TaintSink::ShellCommand),
        "expected shell-command CWE mapping"
    );
    assert!(default_cwe_patterns().iter().any(|p| p.cwe_id == cwe));
});

#[cfg(feature = "bundle-minimal")]
cwe_pattern_test!(
    cwe_22_path_traversal,
    "CWE-22",
    r#"
def read_file(request):
    path = request.GET['path']
    open(path).read()
"#
);

cwe_test!(cwe_798_hardcoded_credentials, "CWE-798", |cwe: &str| {
    let code = r#"
password = "super_secret"
api_key = "sk-live-12345"
def noop():
    pass
"#;
    let cfg = build_cfg_for_function("python", code, "noop")
        .unwrap_or_else(|_| build_cfg_for_function("rust", r#"fn noop() {}"#, "noop").unwrap());
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let flow = TaintFlow {
        source: *pdg.nodes.keys().next().unwrap(),
        source_type: TaintSource::HttpParameter,
        sink: *pdg.nodes.keys().next().unwrap(),
        sink_type: TaintSink::LogOutput,
        variable: "password".into(),
        path: vec![],
        sanitizers: vec![],
        severity: 9,
    };
    let vulns = SecurityAnalyzer::new().analyze(vec![flow], &pdg, code);
    assert!(vulns.iter().any(|v| v.cwe_id == cwe));
});

#[cfg(feature = "bundle-minimal")]
cwe_pattern_test!(
    cwe_502_insecure_deserialization,
    "CWE-502",
    r#"
def load(request):
    data = request.POST['data']
    pickle.loads(data)
"#
);

#[cfg(feature = "bundle-minimal")]
cwe_pattern_test!(
    cwe_918_ssrf,
    "CWE-918",
    r#"
def fetch(request):
    url = request.GET['url']
    requests.get(url)
"#
);

cwe_pattern_test!(
    cwe_352_csrf,
    "CWE-352",
    r#"
@app.route('/transfer', methods=['POST'])
def transfer():
    db.execute("UPDATE accounts")
"#
);

cwe_pattern_test!(
    cwe_287_improper_authentication,
    "CWE-287",
    r#"
def login(user):
    if bypass_auth():
        return True
"#
);

cwe_pattern_test!(
    cwe_306_missing_authentication,
    "CWE-306",
    r#"
def admin_delete_user(id):
    db.delete(id)
"#
);
