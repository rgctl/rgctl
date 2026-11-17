//! `rgctl metrics` — PageRank, betweenness, communities, and git churn hotspots.

use super::args::OutputFormat;
use super::context::CliContext;
use anyhow::Result;
use rgctl_graph::backend::GraphBackend;
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_service::command::{Command, MetricsArgs as SvcMetrics};
use rgctl_service::{Session, execute};
use serde_json::json;
use std::path::Path;
use uuid::Uuid;

pub struct MetricsArgs {
    pub pagerank: bool,
    pub betweenness: bool,
    pub communities: bool,
    pub churn: bool,
    pub iterations: Option<usize>,
    /// Include test / intrinsic nodes in PageRank hotspots (default: production only).
    pub include_tests: bool,
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
            .or_else(|| row.get("file_path").and_then(|v| v.as_str()))
            .unwrap_or("<unknown>");
        let kind = row.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
        let file = row.get("file_path").and_then(|v| v.as_str()).unwrap_or("-");
        let score = row.get(score_key).and_then(|v| v.as_f64()).unwrap_or(0.0);
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

fn git_churn_json_exists(repo: &Path) -> bool {
    repo.join(".rgctl")
        .join("analysis")
        .join("git_churn.json")
        .is_file()
}

fn backend_has_git_hotspot(backend: &rgctl_graph::backend::MemoryBackend) -> bool {
    backend
        .all_nodes()
        .ok()
        .map(|nodes| {
            nodes
                .iter()
                .any(|n| n.get_property("git_hotspot_score").is_some())
        })
        .unwrap_or(false)
}

fn collect_churn_hotspots(
    backend: &rgctl_graph::backend::MemoryBackend,
    limit: usize,
) -> Vec<serde_json::Value> {
    let Ok(nodes) = backend.all_nodes() else {
        return Vec::new();
    };
    let Ok(co_edges) = backend.find_edges_by_type(EdgeType::CoChangesWith) else {
        return Vec::new();
    };

    // Pre-index file path -> max cyclomatic and node id -> file path in O(N).
    let mut max_cyclo_by_file: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    let mut path_by_id: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::with_capacity(nodes.len());
    for n in &nodes {
        if n.node_type == NodeType::File {
            let fp = n
                .file_path
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| n.name.to_string());
            path_by_id.insert(n.id, fp);
        } else if n.node_type == NodeType::Function
            && let Some(fp) = n.file_path.as_deref()
            && let Some(cyclo) = n
                .get_property("cyclomatic")
                .and_then(|s| s.parse::<u32>().ok())
        {
            let entry = max_cyclo_by_file.entry(fp.to_string()).or_insert(1);
            if cyclo > *entry {
                *entry = cyclo;
            }
        }
    }

    // Pre-group CoChangesWith edges by source node in O(E).
    let mut co_by_from: std::collections::HashMap<Uuid, Vec<(String, u32, f32)>> =
        std::collections::HashMap::new();
    for e in &co_edges {
        let count = e
            .get_property("cochange_count")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        let jaccard = e
            .get_property("jaccard")
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0);
        let peer_path = path_by_id
            .get(&e.to)
            .cloned()
            .unwrap_or_else(|| e.to.to_string());
        co_by_from
            .entry(e.from)
            .or_default()
            .push((peer_path, count, jaccard));
    }

    let mut file_rows: Vec<serde_json::Value> = Vec::new();
    for n in &nodes {
        if n.node_type != NodeType::File {
            continue;
        }
        let Some(hot_s) = n.get_property("git_hotspot_score") else {
            continue;
        };
        let Ok(hotspot) = hot_s.parse::<f64>() else {
            continue;
        };
        let commit_count = n
            .get_property("git_commits")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        let churn_lines = n
            .get_property("git_churn_lines")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let recency = n
            .get_property("git_churn_score")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let file_path = n
            .file_path
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| n.name.to_string());
        let max_cyclo = max_cyclo_by_file.get(&file_path).copied().unwrap_or(1);

        let mut co_changing = co_by_from.remove(&n.id).unwrap_or_default();
        co_changing.sort_by(|a, b| {
            b.2.total_cmp(&a.2)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        co_changing.truncate(5);

        file_rows.push(json!({
            "node": n.id.to_string(),
            "kind": "file",
            "name": n.name.to_string(),
            "file_path": file_path,
            "commit_count": commit_count,
            "churn_lines": churn_lines,
            "recency_churn_score": recency,
            "max_cyclomatic": max_cyclo,
            "hotspot_score": hotspot,
            "co_changing_files": co_changing.iter().map(|(p, c, j)| json!({
                "file_path": p,
                "cochange_count": c,
                "jaccard": format!("{j:.4}"),
            })).collect::<Vec<_>>(),
        }));
    }

    file_rows.sort_by(|a, b| {
        let sa = a
            .get("hotspot_score")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let sb = b
            .get("hotspot_score")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        sb.total_cmp(&sa)
    });
    file_rows.truncate(limit);

    // Also surface top functions by git_hotspot_score.
    let mut fn_rows: Vec<serde_json::Value> = Vec::new();
    for n in &nodes {
        if n.node_type != NodeType::Function {
            continue;
        }
        let Some(hot_s) = n.get_property("git_hotspot_score") else {
            continue;
        };
        let Ok(hotspot) = hot_s.parse::<f64>() else {
            continue;
        };
        fn_rows.push(json!({
            "node": n.id.to_string(),
            "kind": format!("{:?}", n.node_type).to_lowercase(),
            "name": n.name.to_string(),
            "file_path": n.file_path.as_ref().map(|s| s.to_string()).unwrap_or_default(),
            "hotspot_score": hotspot,
        }));
    }
    fn_rows.sort_by(|a, b| {
        let sa = a
            .get("hotspot_score")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let sb = b
            .get("hotspot_score")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        sb.total_cmp(&sa)
    });
    fn_rows.truncate(limit.min(10));

    file_rows.extend(fn_rows);
    file_rows
}

pub fn run(ctx: &CliContext, args: MetricsArgs) -> Result<()> {
    let run_all = !args.pagerank && !args.betweenness && !args.communities && !args.churn;

    if ctx.format == OutputFormat::Json {
        let mut value = if args.pagerank || args.betweenness || args.communities || run_all {
            let mut session = Session::new(&ctx.repo);
            if !session.graph_ready() {
                anyhow::bail!("Graph not found (run `rgctl discover` first)");
            }
            execute(
                &mut session,
                Command::Metrics(SvcMetrics {
                    pagerank: args.pagerank || run_all,
                    betweenness: args.betweenness || run_all,
                    communities: args.communities || run_all,
                    include_tests: args.include_tests,
                    iterations: args.iterations,
                }),
            )?
        } else {
            json!({
                "schema_version": 1,
            })
        };
        if args.churn || (run_all && git_churn_json_exists(&ctx.repo)) {
            let graph = ctx.load_graph()?;
            let hotspots = collect_churn_hotspots(graph.backend(), 20);
            if let Some(obj) = value.as_object_mut() {
                obj.insert("churn_hotspots".into(), json!(hotspots));
            }
        }
        return ctx.emit_json_value(&value);
    }

    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let churn_available = git_churn_json_exists(&ctx.repo) || backend_has_git_hotspot(backend);
    let emit_churn = args.churn || (run_all && churn_available);

    use super::metrics_output::{
        MetricsCommunitiesSection, MetricsPagerankSection, build_metrics_response_with_churn,
    };
    use crate::analysis::{
        CommunityDetector, FastPageRank, PetGraphView, SampledBetweenness, classify_node_role,
        community_edge_types_for_backend, default_behavioral_edges, filter_scores_by_role,
    };

    let view = PetGraphView::from_backend(backend)?;
    let iterations = args.iterations.unwrap_or(20);
    let allowed = default_behavioral_edges();

    let mut pagerank = None;
    let mut betweenness = None;
    let mut communities = None;
    let mut churn_hotspots = None;

    if args.pagerank || run_all {
        let engine = FastPageRank::new(iterations, 0.85);
        let (scores, stats) = engine.compute(&view, allowed);
        let mut top = filter_scores_by_role(&scores, args.include_tests, |id| {
            backend
                .get_node(id)
                .ok()
                .flatten()
                .map(|n| {
                    let stub = n.get_property("is_external_stub") == Some("true");
                    let name = n.qualified_name.as_deref().unwrap_or(n.name.as_str());
                    classify_node_role(name, n.file_path.as_deref(), stub)
                })
                .unwrap_or(crate::analysis::NodeRole::Production)
        });
        if top.is_empty() {
            top = scores.iter().map(|(id, score)| (*id, *score)).collect();
            top.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        }
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
        let bc = SampledBetweenness::compute_adaptive(&view, allowed);
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

    if emit_churn {
        let rows = collect_churn_hotspots(backend, 20);
        if rows.is_empty() {
            println!("Churn hotspots: (none — run `rgctl discover --with-git`)");
        } else {
            print_hotspot_rows("Churn hotspots", &rows, "hotspot_score");
        }
        churn_hotspots = Some(rows);
    }

    let _ = build_metrics_response_with_churn(pagerank, betweenness, communities, churn_hotspots);
    Ok(())
}
