//! Phase 13 advanced analysis benchmarks.
//!
//! Run: `cargo bench --features bundle-minimal --bench phase13_analysis`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rbuilder::analysis::{
    build_cfg_for_function, CallGraph, DominatorTree, InterproceduralCFG, InterproceduralSlicer,
    ProgramDependenceGraph, SliceCriterion, TaintAnalyzer, TypeInferenceEngine,
};
use rbuilder::gql::{execute, parse, QueryExecutor};
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use std::collections::HashMap;
use std::time::Duration;

fn python_1k_loc() -> String {
    let mut code = String::from("def big(request):\n    x = request.GET['id']\n");
    for i in 0..500 {
        code.push_str(&format!("    v{i} = x + {i}\n"));
    }
    code.push_str("    cursor.execute(f\"SELECT * FROM t WHERE id = {x}\")\n");
    code
}

fn bench_taint_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("phase13_taint");
    group.measurement_time(Duration::from_secs(8));
    let code = python_1k_loc();
    group.bench_function("python_1k_loc", |b| {
        b.iter(|| {
            let cfg = build_cfg_for_function("python", &code, "big").unwrap();
            let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
            let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
            analyzer.detect_patterns("python");
            black_box(analyzer.analyze())
        });
    });
    group.finish();
}

fn bench_type_inference(c: &mut Criterion) {
    let code = python_1k_loc();
    c.bench_function("phase13_type_inference_1k_loc", |b| {
        b.iter(|| {
            let cfg = build_cfg_for_function("python", &code, "big").unwrap();
            let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
            let mut engine = TypeInferenceEngine::new(&pdg, &cfg, "python");
            black_box(engine.infer())
        });
    });
}

fn build_python_chain(depth: usize) -> (MemoryBackend, HashMap<String, String>) {
    assert!(depth >= 2, "chain depth must be >= 2");
    let mut backend = MemoryBackend::new();
    let mut ids = Vec::with_capacity(depth);
    for i in 0..depth {
        let name = format!("f{i}");
        let node = Node::new(NodeType::Function, name.clone()).with_file_path("chain.py".into());
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for i in 0..depth - 1 {
        backend
            .insert_edge(Edge::new(ids[i], ids[i + 1], EdgeType::Calls))
            .unwrap();
    }

    let mut source = String::new();
    for i in 0..depth {
        let name = format!("f{i}");
        if i == 0 {
            source.push_str(&format!("def {name}():\n    f1(0)\n"));
        } else if i < depth - 1 {
            let next = format!("f{}", i + 1);
            source.push_str(&format!("def {name}(data):\n    return {next}(data)\n"));
        } else {
            source.push_str(&format!("def {name}(data):\n    return data\n"));
        }
    }
    let mut files = HashMap::new();
    files.insert("chain.py".into(), source);
    (backend, files)
}

fn bench_interprocedural_slice(c: &mut Criterion) {
    let depth = 10;
    let (backend, files) = build_python_chain(depth);
    let source = files.get("chain.py").unwrap().clone();
    let leaf_name = format!("f{}", depth - 1);

    c.bench_function("phase13_interprocedural_10_fn_chain", |b| {
        b.iter(|| {
            let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
            let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();
            let leaf_id = icfg.call_graph.id_by_name(&leaf_name).unwrap();
            let pdg =
                ProgramDependenceGraph::build(icfg.get_cfg(leaf_id).unwrap(), source.as_bytes())
                    .unwrap();
            let line = pdg
                .nodes
                .values()
                .find(|n| n.statement.text.contains("return data"))
                .map(|n| n.statement.line)
                .unwrap_or(1);
            black_box(
                slicer
                    .slice(
                        leaf_id,
                        SliceCriterion {
                            variable: "data".into(),
                            line,
                        },
                    )
                    .unwrap(),
            )
        });
    });
}

fn large_backend(n: usize) -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    for i in 0..n {
        backend
            .insert_node(Node::new(NodeType::Function, format!("fn_{i}")))
            .unwrap();
    }
    backend
        .insert_node(Node::new(NodeType::Function, "target".into()))
        .unwrap();
    backend
}

fn bench_gql_optimizer_speedup(c: &mut Criterion) {
    let mut group = c.benchmark_group("phase13_gql");
    for size in [100usize, 500] {
        let backend = large_backend(size);
        let query = "MATCH (f:Function) WHERE f.name = 'target' RETURN f";
        group.bench_with_input(
            BenchmarkId::new("unoptimized", size),
            &backend,
            |b, backend| {
                let parsed = parse(query).unwrap();
                b.iter(|| {
                    black_box(QueryExecutor::new(backend).execute(&parsed).unwrap());
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("optimized", size),
            &backend,
            |b, backend| {
                b.iter(|| {
                    black_box(execute(backend, query).unwrap());
                });
            },
        );
    }
    group.finish();
}

fn bench_call_graph(c: &mut Criterion) {
    let backend = large_backend(200);
    c.bench_function("phase13_call_graph_200_nodes", |b| {
        b.iter(|| black_box(CallGraph::from_backend(&backend).unwrap()));
    });
}

fn bench_dominance_1000_blocks(c: &mut Criterion) {
    let mut code = String::from("fn nested(mut x: i32) -> i32 {\n");
    for i in 0..500 {
        code.push_str(&format!("    if x > {i} {{ x += {i}; }}\n"));
    }
    code.push_str("    x\n}\n");
    let cfg = build_cfg_for_function("rust", &code, "nested").unwrap();
    assert!(cfg.blocks.len() >= 500, "expected large CFG, got {}", cfg.blocks.len());

    c.bench_function("phase13_dominance_1000_blocks", |b| {
        b.iter(|| {
            let start = std::time::Instant::now();
            let dom = DominatorTree::build(&cfg);
            let elapsed = start.elapsed();
            assert!(
                elapsed < Duration::from_millis(15),
                "idom+DF exceeded 15ms: {elapsed:?} for {} blocks",
                cfg.blocks.len()
            );
            black_box(dom)
        });
    });
}

criterion_group!(
    benches,
    bench_taint_analysis,
    bench_type_inference,
    bench_interprocedural_slice,
    bench_gql_optimizer_speedup,
    bench_call_graph,
    bench_dominance_1000_blocks
);
criterion_main!(benches);
