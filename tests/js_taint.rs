//! Phase 13-style JavaScript/TypeScript analysis: CFG depth, taint, and call relations.

use rbuilder::analysis::{
    build_cfg_for_function, canonical_language_id, cfg_language_id_from_path, ProgramDependenceGraph,
    TaintAnalyzer, TaintSink, TaintSource,
};
use rbuilder_lang_javascript::JavaScriptPlugin;
use rbuilder_lang_typescript::TypeScriptPlugin;
use rbuilder_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn javascript_cfg_language_profile_maps_extension() {
    assert_eq!(canonical_language_id("js"), Some("javascript"));
    assert_eq!(
        cfg_language_id_from_path(Path::new("src/routes/auth.js")),
        Some("javascript")
    );
}

#[test]
fn typescript_cfg_language_profile_maps_extension() {
    assert_eq!(canonical_language_id("ts"), Some("typescript"));
    assert_eq!(
        cfg_language_id_from_path(Path::new("src/routes/auth.ts")),
        Some("typescript")
    );
}

#[test]
fn javascript_taint_detects_http_to_sql_flow() {
    let code = r#"
function bad(req, db) {
    const id = req.params.id;
    db.query("SELECT * FROM users WHERE id = " + id);
}
"#;
    let cfg = build_cfg_for_function("javascript", code, "bad").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("javascript");
    let flows = analyzer.analyze();
    assert!(
        flows.iter().any(|f| {
            f.source_type == TaintSource::HttpParameter && f.sink_type == TaintSink::SqlQuery
        }),
        "expected HTTP -> SQL taint flow, got {flows:?}"
    );
}

#[test]
fn typescript_taint_detects_http_to_sql_flow() {
    let code = r#"
function bad(req: any, db: any) {
    const id = req.body.id;
    db.execute("SELECT * FROM users WHERE id = " + id);
}
"#;
    let cfg = build_cfg_for_function("typescript", code, "bad").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("typescript");
    let flows = analyzer.analyze();
    assert!(
        flows.iter().any(|f| {
            f.source_type == TaintSource::HttpParameter && f.sink_type == TaintSink::SqlQuery
        }),
        "expected HTTP -> SQL taint flow, got {flows:?}"
    );
}

#[test]
fn javascript_plugin_extracts_call_relations() {
    let source = br#"
function register() {
    validate();
    persist();
}

function validate() {}
function persist() {}
"#;
    let plugin = JavaScriptPlugin::new().unwrap();
    let path = Path::new("svc.js");
    let symbols = plugin.extract_symbols(path, source).unwrap();
    let relations = plugin.extract_relations(path, source, &symbols).unwrap();
    let calls: Vec<_> = relations
        .iter()
        .filter(|r| matches!(r.relation_type, RelationType::Calls))
        .collect();
    assert!(calls.len() >= 2, "expected at least 2 calls, got {calls:?}");
    assert!(
        calls.iter().any(|r| r.to == "validate"),
        "missing validate call"
    );
    assert!(calls.iter().any(|r| r.to == "persist"), "missing persist call");
    assert!(
        symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .count()
            >= 3
    );
}

#[test]
fn typescript_plugin_extracts_call_relations() {
    let source = br#"
function register(): void {
    validate();
    persist();
}

function validate(): void {}
function persist(): void {}
"#;
    let plugin = TypeScriptPlugin::new().unwrap();
    let path = Path::new("svc.ts");
    let symbols = plugin.extract_symbols(path, source).unwrap();
    let relations = plugin.extract_relations(path, source, &symbols).unwrap();
    let calls: Vec<_> = relations
        .iter()
        .filter(|r| matches!(r.relation_type, RelationType::Calls))
        .collect();
    assert!(calls.len() >= 2, "expected at least 2 calls, got {calls:?}");
    assert!(
        calls.iter().any(|r| r.to == "validate"),
        "missing validate call"
    );
}

#[test]
fn javascript_switch_cfg_has_multiple_branches() {
    let code = r#"
function classify(v) {
    switch (v) {
        case 1:
            return "one";
        case 2:
            return "two";
        default:
            return "other";
    }
}
"#;
    let cfg = build_cfg_for_function("javascript", code, "classify").unwrap();
    assert!(cfg.blocks.len() >= 5, "switch should fan out blocks");
}
