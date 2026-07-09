//! Phase 17 — sampled betweenness + HyperBall harmonic scale gates.

use rbuilder::analysis::{
    BetweennessMode, CentralityAnalyzer, HarmonicMode, PetGraphView, SampledBetweenness,
    HyperBallHarmonic, FlatGraphIndex, DEFAULT_SAMPLE_PIVOTS,
};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder::graph::{CodeGraph, MmappedGraphSnapshot, SnapshotNodeStore};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn build_mock_graph(nodes: usize, edges: usize) -> CodeGraph {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    let mut ids = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let node = Node::new(NodeType::Function, format!("n{i}"));
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for e in 0..edges {
        let from = ids[e % nodes];
        let to = ids[(e * 13 + 7) % nodes];
        if from != to {
            backend
                .insert_edge(Edge::new(from, to, EdgeType::Calls))
                .unwrap();
        }
    }
    graph
}

fn bench_repo(path: &Path) -> Option<(usize, usize, Duration, Duration, Duration)> {
    let snap_path = MmappedGraphSnapshot::default_path(path);
    let store = SnapshotNodeStore::open(&snap_path).ok()?;
    let view = PetGraphView::from_snapshot_store(&store).ok()?;
    let n = view.directed.node_count();
    let e = view.directed.edge_count();

    let analyzer = CentralityAnalyzer::new().with_exact_limit(500);
    let start = Instant::now();
    let report = analyzer.analyze_with_view(&view).ok()?;
    let total = start.elapsed();

    let bt = Duration::from_millis(report.approx_stats.betweenness_ms);
    let hm = Duration::from_millis(report.approx_stats.harmonic_ms);
    Some((n, e, total, bt, hm))
}

fn repo_or_env(name: &str, default: PathBuf) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RBUILDER_BENCH_REPO") {
        let p = PathBuf::from(p);
        if p.join(".rbuilder/graph.snapshot.bin").is_file()
            || MmappedGraphSnapshot::default_path(&p).is_file()
        {
            return Some(p);
        }
    }
    if default.join(".rbuilder/graph.snapshot.bin").is_file()
        || MmappedGraphSnapshot::default_path(&default).is_file()
    {
        Some(default)
    } else {
        eprintln!("skip {name}: no snapshot at {}", default.display());
        None
    }
}

/// 10k-node mock: approximate centrality must finish within 30s (release).
#[test]
fn approx_centrality_10k_mock_under_30s() {
    let graph = build_mock_graph(10_000, 40_000);
    let view = PetGraphView::from_backend(graph.backend()).unwrap();
    let start = Instant::now();
    let report = CentralityAnalyzer::new()
        .with_exact_limit(500)
        .with_sample_pivots(128)
        .analyze_with_view(&view)
        .unwrap();
    let elapsed = start.elapsed();

    assert!(matches!(
        report.approx_stats.betweenness_mode,
        Some(BetweennessMode::Sampled { .. })
    ));
    assert!(matches!(
        report.approx_stats.harmonic_mode,
        Some(HarmonicMode::HyperBall { .. })
    ));
    assert!(
        report.scores.values().any(|s| s.betweenness > 0.0),
        "expected non-zero sampled betweenness"
    );
    assert!(
        report.scores.values().any(|s| s.harmonic > 0.0),
        "expected non-zero hyperball harmonic"
    );
    assert!(
        elapsed < Duration::from_secs(30),
        "10k approx centrality regression: {elapsed:?} >= 30s"
    );
    eprintln!(
        "10k mock: total={elapsed:?} bt={}ms harm={}ms",
        report.approx_stats.betweenness_ms, report.approx_stats.harmonic_ms
    );
}

/// Flat-index sampled betweenness alone on 50k mock — budget 60s.
#[test]
fn sampled_betweenness_50k_flat_under_60s() {
    let graph = build_mock_graph(50_000, 200_000);
    let view = PetGraphView::from_backend(graph.backend()).unwrap();
    let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
    let start = Instant::now();
    let scores = SampledBetweenness::compute_flat(&index, DEFAULT_SAMPLE_PIVOTS, 42);
    let elapsed = start.elapsed();
    assert_eq!(scores.len(), 50_000);
    assert!(scores.iter().any(|&s| s > 0.0));
    assert!(
        elapsed < Duration::from_secs(60),
        "50k sampled betweenness regression: {elapsed:?} >= 60s"
    );
    eprintln!("50k sampled BC (k={DEFAULT_SAMPLE_PIVOTS}): {elapsed:?}");
}

/// HyperBall harmonic on 50k mock — budget 60s (HLL merges dominate).
#[test]
fn hyperball_harmonic_50k_flat_under_60s() {
    let graph = build_mock_graph(50_000, 200_000);
    let view = PetGraphView::from_backend(graph.backend()).unwrap();
    let index = FlatGraphIndex::from_view(&view, &[EdgeType::Calls]);
    let start = Instant::now();
    let scores = HyperBallHarmonic::compute_flat(&index, 16);
    let elapsed = start.elapsed();
    assert_eq!(scores.len(), 50_000);
    assert!(scores.iter().any(|&s| s > 0.0));
    assert!(
        elapsed < Duration::from_secs(60),
        "50k hyperball harmonic regression: {elapsed:?} >= 60s"
    );
    eprintln!("50k HyperBall harmonic: {elapsed:?}");
}

/// Kafka example repo when snapshot exists.
#[test]
fn approx_centrality_kafka_when_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/kafka");
    if !root.is_dir() {
        eprintln!("skip kafka: example/kafka missing");
        return;
    }
    if let Some((n, e, total, bt, hm)) = bench_repo(&root) {
        eprintln!(
            "kafka approx centrality: nodes={n} edges={e} total={total:?} betweenness={bt:?} harmonic={hm:?}"
        );
        assert!(total < Duration::from_secs(120));
    } else {
        eprintln!("skip kafka: no graph snapshot — run discover first");
    }
}

/// Metasfresh reference repo — manual/CI-soft gate with timing report.
#[test]
#[ignore = "scale gate: run with `cargo test --release --test phase17_centrality_approx_scale -- --ignored --nocapture`"]
fn approx_centrality_metasfresh_timing() {
    let root = repo_or_env(
        "metasfresh",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/metasfresh-4.9.8b"),
    )
    .expect("metasfresh snapshot required for ignored gate");

    let (n, e, total, bt, hm) = bench_repo(&root).expect("centrality analysis failed");
    eprintln!("=== metasfresh approximate centrality ===");
    eprintln!("nodes: {n}");
    eprintln!("edges: {e}");
    eprintln!("total centrality pass: {total:?}");
    eprintln!("betweenness (sampled): {bt:?}");
    eprintln!("harmonic (HyperBall): {hm:?}");
    eprintln!("(compare to full discover ~5-6 min baseline)");

    assert!(
        total < Duration::from_secs(180),
        "metasfresh approx centrality regression: {total:?} >= 180s"
    );
}

/// Gbuilder golden repo when available.
#[test]
#[ignore = "scale gate: run with --ignored when RBUILDER_BENCH_REPO or gbuilder snapshot present"]
fn approx_centrality_gbuilder_timing() {
    let root = repo_or_env(
        "gbuilder",
        PathBuf::from("/Users/sshaaf/git/java/gbuilder"),
    )
    .expect("gbuilder snapshot required");

    let (n, e, total, bt, hm) = bench_repo(&root).expect("centrality failed");
    eprintln!("gbuilder: nodes={n} edges={e} total={total:?} bt={bt:?} hm={hm:?}");
    assert!(total < Duration::from_secs(30));
}
