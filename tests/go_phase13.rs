//! Phase 13-style Go analysis: CFG depth, taint, and call relations.

use rbuilder::analysis::{
    build_cfg_for_function, canonical_language_id, cfg_language_id_from_path, ProgramDependenceGraph,
    TaintAnalyzer, TaintSink, TaintSource,
};
use rbuilder_lang_go::GoPlugin;
use rbuilder_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn go_canonical_language_id_from_profile() {
    assert_eq!(canonical_language_id("golang"), Some("go"));
    assert_eq!(
        cfg_language_id_from_path(Path::new("internal/auth.go")),
        Some("go")
    );
}

#[test]
fn go_taint_detects_http_to_sql_flow() {
    let code = r#"
package handler

import "github.com/gin-gonic/gin"

func Bad(c *gin.Context) {
    id := c.Query("id")
    db.Exec("SELECT * FROM users WHERE id = " + id)
}
"#;
    let cfg = build_cfg_for_function("go", code, "Bad").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("go");
    let flows = analyzer.analyze();
    assert!(
        flows.iter().any(|f| {
            f.source_type == TaintSource::HttpParameter && f.sink_type == TaintSink::SqlQuery
        }),
        "expected HTTP -> SQL taint flow, got {flows:?}"
    );
}

#[test]
fn go_plugin_extracts_call_relations() {
    let source = br#"
package svc

func Register() {
    validate()
    persist()
}

func validate() {}
func persist() {}
"#;
    let plugin = GoPlugin::new().unwrap();
    let path = Path::new("svc.go");
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
fn go_switch_cfg_has_multiple_branches() {
    let code = r#"
package demo

func Classify(v int) string {
    switch v {
    case 1:
        return "one"
    case 2:
        return "two"
    default:
        return "other"
    }
}
"#;
    let cfg = build_cfg_for_function("go", code, "Classify").unwrap();
    assert!(cfg.blocks.len() >= 5, "switch should fan out blocks");
}
