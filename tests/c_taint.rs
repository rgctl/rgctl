//! Phase 13-style C analysis: CFG, taint, and call relations.

use rbuilder::analysis::{
    build_cfg_for_function, canonical_language_id, cfg_language_id_from_path,
    ProgramDependenceGraph, TaintAnalyzer, TaintSink, TaintSource,
};
use rbuilder_lang_c::CPlugin;
use rbuilder_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn c_canonical_language_id_from_profile() {
    assert_eq!(canonical_language_id("c"), Some("c"));
    assert_eq!(
        cfg_language_id_from_path(Path::new("src/cart_service.c")),
        Some("c")
    );
    assert_eq!(
        cfg_language_id_from_path(Path::new("include/types.h")),
        Some("c")
    );
}

#[test]
fn c_taint_detects_env_to_sql_flow() {
    let code = r#"
#include <sqlite3.h>
#include <stdlib.h>

void bad_handler(void) {
    char *id = getenv("QUERY_STRING");
    char query[256];
    sprintf(query, "SELECT * FROM users WHERE id = %s", id);
    sqlite3_exec(db, query, NULL, NULL, NULL);
}
"#;
    let cfg = build_cfg_for_function("c", code, "bad_handler").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("c");
    let flows = analyzer.analyze();
    assert!(
        flows.iter().any(|f| {
            f.source_type == TaintSource::HttpParameter && f.sink_type == TaintSink::SqlQuery
        }),
        "expected env/HTTP -> SQL taint flow, got {flows:?}"
    );
}

#[test]
fn c_plugin_extracts_call_relations() {
    let source = br#"
void register_user(void) {
    validate();
    persist();
}

void validate(void) {}
void persist(void) {}
"#;
    let plugin = CPlugin::new().unwrap();
    let path = Path::new("service.c");
    let symbols = plugin.extract_symbols(path, source).unwrap();
    let relations = plugin.extract_relations(path, source, &symbols).unwrap();
    let calls: Vec<_> = relations
        .iter()
        .filter(|r| matches!(r.relation_type, RelationType::Calls))
        .collect();
    assert!(calls.len() >= 2, "expected at least 2 calls, got {calls:?}");
    assert!(
        symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .count()
            >= 3
    );
}

#[test]
fn c_if_cfg_has_true_false_edges() {
    let code = r#"
int abs_val(int x) {
    if (x > 0) {
        return x;
    }
    return -x;
}
"#;
    let cfg = build_cfg_for_function("c", code, "abs_val").unwrap();
    assert!(cfg.blocks.len() >= 4);
}
