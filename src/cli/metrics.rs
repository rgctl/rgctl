//! `rgctl metrics` — PageRank, betweenness, and community detection.

use super::args::OutputFormat;
use super::context::CliContext;
use anyhow::Result;
use rgctl_graph::backend::GraphBackend;
use rgctl_service::command::{Command, MetricsArgs as SvcMetrics};
use rgctl_service::{Session, execute};
use serde_json::json;
use uuid::Uuid;

pub struct MetricsArgs {
    pub pagerank: bool,
    pub betweenness: bool,
    pub communities: bool,
    pub iterations: Option<usize>,
}

fn enrich_top_entry(
    backend: &rgctl_graph::backend::MemoryBackend,
    id: Uuid,
    score_key: &str,
    score: f64,
) -> serde_json::Value {
    let mut obj = json!({
        "node": id.to_string(),
        score_key: score,
    });
    if let Ok(Some(n)) = backend.get_node(id) {
        obj["name"] = json!(n.name.to_string());
        obj["kind"] = json!(format!("{:?}", n.node_type).to_lowercase());
        if let Some(qn) = n.qualified_name.as_ref() {
            obj["qualified_name"] = json!(qn.to_string());
        }
        if let Some(fp) = n.file_path.as_ref() {
            obj["file_path"] = json!(fp.to_string());
        }
    }
    obj
}

fn print_hotspot_rows(title: &str, rows: &[serde_json::Value], score_key: &str) {
    println!("{title}:");
    for (rank, row) in rows.iter().enumerate() {
        let name = row
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let kind = row.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
        let file = row
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        let score = row
            .get(score_key)
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        println!(
            "  {:>2}. {:<8} {:<32} {:>10.6}  {}",
            rank + 1,
            kind,
            truncate(name, 32),
            score,
            file
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

pub fn run(ctx: &CliContext, args: MetricsArgs) -> Result<()> {
    let run_all = !args.pagerank && !args.betweenness && !args.communities;
    if ctx.format == OutputFormat::Json {
        let mut session = Session::new(&ctx.repo);
        if !session.graph_ready() {
            anyhow::bail!("Graph not found (run `rgctl discover` first)");
        }
        let value = execute(
            &mut session,
            Command::Metrics(SvcMetrics {
                pagerank: args.pagerank || run_all,
                betweenness: args.betweenness || run_all,
                communities: args.communities || run_all,
            }),
        )?;
        let _ = args.iterations;
        return ctx.emit_json_value(&value);
    }

    use super::metrics_output::{
        MetricsCommunitiesSection, MetricsPagerankSection, build_metrics_response,
    };
    use crate::analysis::{
        BetweennessCentrality, CommunityDetector, FastPageRank, PetGraphView,
        community_edge_types_for_backend, default_behavioral_edges,
    };

    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let view = PetGraphView::from_backend(backend)?;
    let iterations = args.iterations.unwrap_or(20);
    let allowed = default_behavioral_edges();

    let mut pagerank = None;
    let mut betweenness = None;
    let mut communities = None;

    if args.pagerank || run_all {
        let engine = FastPageRank::new(iterations, 0.85);
        let (scores, stats) = engine.compute(&view, allowed);
        let mut top: Vec<_> = scores
            .iter()
            .filter(|(_, score)| **score > 0.0)
            .map(|(id, score)| (*id, *score))
            .collect();
        if top.is_empty() {
            top = scores.iter().map(|(id, score)| (*id, *score)).collect();
        }
        top.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        top.truncate(20);
        let top: Vec<_> = top
            .iter()
            .map(|(id, score)| enrich_top_entry(backend, *id, "pagerank", *score))
            .collect();
        print_hotspot_rows("PageRank", &top, "pagerank");
        pagerank = Some(MetricsPagerankSection {
            top,
            converged: stats.converged,
            iterations: stats.iterations_run,
            max_delta: stats.max_delta,
        });
    }

    if args.betweenness || run_all {
        let bc = BetweennessCentrality::compute_unbounded(&view, allowed);
        let mut top: Vec<_> = bc.iter().map(|(id, score)| (id, *score)).collect();
        top.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        top.truncate(20);
        let top: Vec<_> = top
            .iter()
            .map(|(id, s)| enrich_top_entry(backend, **id, "score", *s))
            .collect();
        print_hotspot_rows("Betweenness", &top, "score");
        betweenness = Some(top);
    }

    if args.communities || run_all {
        let detector = CommunityDetector::new();
        let allowed_comm = community_edge_types_for_backend(backend);
        let result = detector.detect_with_view_filtered(&view, &allowed_comm)?;
        println!(
            "Communities: {} (modularity={:.4}, assignments={})",
            result.communities.len(),
            result.modularity,
            result.assignments.len()
        );
        communities = Some(MetricsCommunitiesSection {
            count: result.communities.len(),
            modularity: result.modularity,
            assignments: result.assignments.len(),
        });
    }

    let _ = build_metrics_response(pagerank, betweenness, communities);
    Ok(())
}
