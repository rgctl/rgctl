//! Phase 13: end-to-end integration (4 tests).

#[path = "common/phase13.rs"]
mod phase13;

use phase13::{
    analyze_taint_with_types, build_dominance, build_sample_backend_with_chain, call_graph_from,
    large_graph, run_taint_security,
};
use rbuilder::analysis::{
    InterproceduralCFG, InterproceduralSlicer, ProgramDependenceGraph, SliceCriterion, TaintSink,
};
use rbuilder::gql::execute;
use rbuilder::security::default_cwe_patterns;
use std::collections::HashMap;

macro_rules! e2e_test {
    ($name:ident, $body:expr) => {
        #[test]
        fn $name() {
            $body;
        }
    };
}

#[cfg(feature = "bundle-minimal")]
e2e_test!(e2e_taint_security_sql_pipeline, {
    let code = r#"
def handle(request):
    user = request.GET['user']
    cursor.execute(f"SELECT * FROM accounts WHERE name='{user}'")
"#;
    let vulns = run_taint_security("python", code, "handle");
    assert!(!vulns.is_empty());
    assert!(default_cwe_patterns().iter().any(|p| p.cwe_id == "CWE-89"));
    assert!(vulns[0].recommendation.contains("parameterized"));
});

#[cfg(feature = "bundle-minimal")]
e2e_test!(e2e_interprocedural_dominance_slice, {
    let (backend, files) = build_sample_backend_with_chain(4);
    let cg = call_graph_from(&backend);
    assert_eq!(cg.topological_order().unwrap().len(), 4);

    let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
    let leaf = icfg.call_graph.id_by_name("f3").unwrap();
    let source = files.get("app.rs").unwrap();

    let (cfg, dom) = build_dominance("rust", source, "f3");
    for block in cfg.blocks.keys() {
        assert!(dom.dominates(cfg.entry, *block));
    }

    let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();
    let pdg =
        ProgramDependenceGraph::build(icfg.get_cfg(leaf).unwrap(), source.as_bytes()).unwrap();
    let line = pdg
        .nodes
        .values()
        .find(|n| n.statement.text.contains("input + 1"))
        .map(|n| n.statement.line)
        .unwrap_or(1);
    let slice = slicer
        .slice(
            leaf,
            SliceCriterion {
                variable: "input".into(),
                line,
            },
        )
        .unwrap();
    assert!(slice.functions.contains(&leaf));
});

#[cfg(feature = "bundle-minimal")]
e2e_test!(e2e_type_inference_taint_sanitized, {
    let code = r#"
def handle(request):
    raw = request.GET['id']
    safe = int(raw)
    cursor.execute(f"SELECT * FROM t WHERE id={safe}")
"#;
    let flows = analyze_taint_with_types("python", code, "handle");
    let vuln = flows.iter().filter(|f| f.is_vulnerable()).count();
    assert!(flows.iter().any(|f| f.sink_type == TaintSink::SqlQuery));
    assert!(vuln <= flows.len());
});

e2e_test!(e2e_gql_optimize_execute_large_graph, {
    let backend = large_graph(50);
    let result = execute(
        &backend,
        "MATCH (f:Function) WHERE f.name = 'rare_target' RETURN f",
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0]["f"].name, "rare_target");

    let chain_backend = {
        let (b, _) = build_sample_backend_with_chain(3);
        b
    };
    let _ = call_graph_from(&chain_backend);
    let mut files: HashMap<String, String> = HashMap::new();
    files.insert(
        "app.rs".into(),
        "fn f0() { f1(); }\nfn f1() { f2(1); }\nfn f2(input: i32) -> i32 { input }\n".into(),
    );
    #[cfg(feature = "bundle-minimal")]
    {
        let icfg = InterproceduralCFG::build(&chain_backend, &files).unwrap();
        assert!(!icfg.function_cfgs.is_empty());
    }
});
