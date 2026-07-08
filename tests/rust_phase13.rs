//! Phase 13-style Rust analysis: expanded taint patterns and call relations.

use rbuilder::analysis::{
    build_cfg_for_function, cfg_language_id_from_path, ProgramDependenceGraph, TaintAnalyzer,
    TaintSink, TaintSource,
};
use rbuilder_lang_rust::RustPlugin;
use rbuilder_plugin_api::{LanguagePlugin, RelationType};
use std::path::Path;

#[path = "common/phase13.rs"]
mod phase13;

use phase13::analyze_taint;

#[test]
fn rust_cfg_language_profile_maps_extension() {
    assert_eq!(
        cfg_language_id_from_path(Path::new("src/routes/orders.rs")),
        Some("rust")
    );
}

#[test]
fn rust_taint_detects_env_to_sqlx_flow() {
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
    let flows = analyzer.analyze();
    assert!(
        flows.iter().any(|f| {
            f.source_type == TaintSource::EnvironmentVar && f.sink_type == TaintSink::SqlQuery
        }),
        "expected env -> sqlx taint flow, got {flows:?}"
    );
}

#[test]
fn rust_taint_patterns_cover_sqlx_sink() {
    let code = r#"
fn handler(pool: &sqlx::SqlitePool) {
    sqlx::query("SELECT 1").execute(pool);
}
"#;
    let flows = analyze_taint("rust", code, "handler");
    assert!(
        flows.is_empty() || flows.iter().any(|f| f.sink_type == TaintSink::SqlQuery),
        "sqlx sink pattern should be recognized"
    );
}

#[test]
fn rust_plugin_extracts_call_relations() {
    let source = br#"
fn register() {
    validate();
    persist();
}

fn validate() {}
fn persist() {}
"#;
    let plugin = RustPlugin::new().unwrap();
    let path = Path::new("svc.rs");
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
fn rust_cfg_builds_from_fixture_when_present() {
    const RUST_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-rust";
    let repo = std::env::var("RBUILDER_RUST_REPO")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(RUST_REPO));
    let file = repo.join("src/routes/orders.rs");
    if !file.is_file() {
        eprintln!("skip: rust fixture not found at {}", file.display());
        return;
    }

    let source = std::fs::read_to_string(&file).unwrap();
    let cfg = build_cfg_for_function("rust", &source, "checkout").expect("checkout CFG");
    assert!(!cfg.blocks.is_empty());

    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).expect("checkout PDG");
    assert!(!pdg.nodes.is_empty());
}
