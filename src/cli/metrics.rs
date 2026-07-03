//! `rbuilder metrics` — PageRank, betweenness, and community detection.

use super::args::OutputFormat;
use super::context::CliContext;
use super::metrics_output::wrap_metrics_payload;
use anyhow::Result;
use crate::analysis::{
    BetweennessCentrality, CommunityDetector, FastPageRank, PetGraphView,
    default_behavioral_edges, default_community_edge_types,
};
use rbuilder_graph::schema::EdgeType;
use serde_json::json;

pub struct MetricsArgs {
    pub pagerank: bool,
    pub betweenness: bool,
    pub communities: bool,
    pub iterations: Option<usize>,
}

pub fn run(ctx: &CliContext, args: MetricsArgs) -> Result<()> {
    let run_all = !args.pagerank && !args.betweenness && !args.communities;
    let graph = ctx.load_graph()?;
    let view = PetGraphView::from_backend(graph.backend())?;
    let iterations = args.iterations.unwrap_or(20);
    let allowed = default_behavioral_edges();

    let mut payload = json!({});

    if args.pagerank || run_all {
        let engine = FastPageRank::new(iterations, 0.85);
        let (scores, stats) = engine.compute(&view, &[EdgeType::Calls]);
        let top: Vec<_> = scores
            .iter()
            .map(|(id, score)| json!({ "node": id.to_string(), "pagerank": score }))
            .take(20)
            .collect();
        payload["pagerank"] = json!({
            "top": top,
            "converged": stats.converged,
            "iterations": stats.iterations_run,
            "max_delta": stats.max_delta,
        });
    }

    if args.betweenness || run_all {
        let bc = BetweennessCentrality::compute_unbounded(&view, &[EdgeType::Calls]);
        let mut top: Vec<_> = bc
            .iter()
            .map(|(id, score)| (id, *score))
            .collect();
        top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top.truncate(20);
        payload["betweenness"] = json!(
            top.iter()
                .map(|(id, s)| json!({ "node": id.to_string(), "score": s }))
                .collect::<Vec<_>>()
        );
    }

    if args.communities || run_all {
        let detector = CommunityDetector::new();
        let result = detector.detect_with_view_filtered(&view, default_community_edge_types())?;
        payload["communities"] = json!({
            "count": result.communities.len(),
            "modularity": result.modularity,
            "assignments": result.assignments.len(),
        });
        let _ = allowed;
    }

    if ctx.format == OutputFormat::Json {
        wrap_metrics_payload(&mut payload);
        ctx.emit_json_value(&payload)?;
    } else {
        if let Some(pr) = payload.get("pagerank") {
            println!("PageRank: {:?}", pr);
        }
        if let Some(bc) = payload.get("betweenness") {
            println!("Betweenness top: {:?}", bc);
        }
        if let Some(cm) = payload.get("communities") {
            println!("Communities: {:?}", cm);
        }
    }
    Ok(())
}
