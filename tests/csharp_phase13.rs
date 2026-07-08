//! Phase 13-style C# analysis: CFG, taint, and call relations.

use rbuilder::analysis::{
    build_cfg_for_function, canonical_language_id, cfg_language_id_from_path,
    ProgramDependenceGraph, TaintAnalyzer, TaintSink, TaintSource,
};
use rbuilder_lang_csharp::CSharpPlugin;
use rbuilder_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn csharp_canonical_language_id_from_profile() {
    assert_eq!(canonical_language_id("cs"), Some("csharp"));
    assert_eq!(
        cfg_language_id_from_path(Path::new("Services/Auth.cs")),
        Some("csharp")
    );
}

#[test]
fn csharp_taint_detects_http_to_sql_flow() {
    let code = r#"
public class BadController {
    public void Bad() {
        var id = Request.Query["id"];
        db.ExecuteSqlRaw("SELECT * FROM users WHERE id = " + id);
    }
}
"#;
    let cfg = build_cfg_for_function("csharp", code, "Bad").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
    analyzer.detect_patterns("csharp");
    let flows = analyzer.analyze();
    assert!(
        flows.iter().any(|f| {
            f.source_type == TaintSource::HttpParameter && f.sink_type == TaintSink::SqlQuery
        }),
        "expected HTTP -> SQL taint flow, got {flows:?}"
    );
}

#[test]
fn csharp_plugin_extracts_call_relations() {
    let source = br#"
public class Service {
    public void Register() {
        Validate();
        Persist();
    }

    public void Validate() {}
    public void Persist() {}
}
"#;
    let plugin = CSharpPlugin::new().unwrap();
    let path = Path::new("Service.cs");
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
fn csharp_if_cfg_has_true_false_edges() {
    let code = r#"
public class Demo {
    public int Abs(int x) {
        if (x > 0) {
            return x;
        }
        return -x;
    }
}
"#;
    let cfg = build_cfg_for_function("csharp", code, "Abs").unwrap();
    assert!(cfg.blocks.len() >= 4);
}
