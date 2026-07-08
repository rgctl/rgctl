//! Phase 13: performance smoke tests (4 tests, generous CI limits).

#[path = "common/phase13.rs"]
mod phase13;

use phase13::{analyze_taint, build_dominance, build_sample_backend_with_chain, large_graph};
use rbuilder::analysis::CallGraph;
use rbuilder::gql::execute;
use std::time::Instant;

macro_rules! perf_test {
    ($name:ident, $limit_ms:expr, $body:expr) => {
        #[test]
        fn $name() {
            let start = Instant::now();
            $body;
            let elapsed = start.elapsed();
            assert!(
                elapsed.as_millis() < $limit_ms,
                "perf smoke exceeded {}ms: {:?}",
                $limit_ms,
                elapsed
            );
        }
    };
}

perf_test!(perf_taint_large_function, 5000, {
    let mut body = String::from("def big(request):\n    x = request.GET['a']\n");
    for i in 0..200 {
        body.push_str(&format!("    v{i} = x + {i}\n"));
    }
    body.push_str("    cursor.execute(x)\n");
    let flows = analyze_taint("python", &body, "big");
    assert!(!flows.is_empty());
});

perf_test!(perf_dominance_large_cfg, 3000, {
    let mut code = String::from("fn big(mut x: i32) -> i32 {\n");
    for i in 0..100 {
        code.push_str(&format!("    if x > {i} {{ x += {i}; }}\n"));
    }
    code.push_str("    x\n}\n");
    let (_cfg, dom) = build_dominance("rust", &code, "big");
    assert!(!dom.idom.is_empty());
});

perf_test!(perf_call_graph_large_chain, 2000, {
    let (backend, _) = build_sample_backend_with_chain(100);
    let cg = CallGraph::from_backend(&backend).unwrap();
    let order = cg.topological_order().unwrap();
    assert_eq!(order.len(), 100);
});

perf_test!(perf_gql_large_graph_query, 3000, {
    let backend = large_graph(500);
    let result = execute(
        &backend,
        "MATCH (f:Function) WHERE f.name = 'fn_250' RETURN f",
    )
    .unwrap();
    assert_eq!(result.rows.len(), 1);
});
