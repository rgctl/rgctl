//! `rbuilder check` — CI policy gateway.

use super::args::OutputFormat;
use super::check_output::{build_check_response, violations_from_json_values};
use super::context::CliContext;
use super::policy_file::PolicyFile;
use anyhow::Result;
use crate::analysis::{
    BlastRadiusEngine, CentralityAnalyzer, PetGraphView, PolicyViolation,
};
use rbuilder_graph::schema::NodeType;
use serde_json::json;
use std::path::Path;
use std::process::Command;

pub struct CheckArgs {
    pub policy_file: String,
}

pub fn run(ctx: &CliContext, args: CheckArgs) -> Result<()> {
    let registry = PolicyFile::load(Path::new(&args.policy_file))?.into_registry();
    let centrality_threshold = registry.centrality_alert_threshold;
    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let view = PetGraphView::from_backend(backend)?;
    let centrality = CentralityAnalyzer::new().analyze_with_view(&view)?.scores;
    let engine = BlastRadiusEngine::build(backend)?;

    let symbols = changed_function_symbols(&ctx.repo, backend)?;
    let symbol_count = symbols.len();
    let mut violation_rows = Vec::new();

    for symbol in symbols {
        let Ok((id, _)) = crate::analysis::resolve_unique_symbol(backend, &symbol) else {
            continue;
        };
        if let Err(err) = engine.analyze_with_policy(
            id,
            Some(backend),
            Some(&registry),
            Some(&centrality),
        ) {
            violation_rows.push(json!({
                "symbol": symbol,
                "error": err.to_string(),
            }));
            continue;
        }
        if let Ok(result) = engine.analyze(id) {
            for node_id in &result.impact_zone_ids {
                if let Some(score) = centrality.get(node_id) {
                    if score.betweenness > centrality_threshold {
                        violation_rows.push(json!({
                            "symbol": symbol,
                            "violation": format!("{}", PolicyViolation::CascadeHazard {
                                node: *node_id,
                                betweenness: score.betweenness,
                                threshold: centrality_threshold,
                            }),
                        }));
                    }
                }
            }
        }
    }

    let response = build_check_response(
        &args.policy_file,
        violations_from_json_values(&violation_rows),
    );

    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(&response)?)?;
    } else if response.passed {
        println!("Policy check passed ({} symbols)", symbol_count);
    } else {
        println!("Policy violations: {}", response.violations.len());
        for v in &response.violations {
            println!("  {v:?}");
        }
    }

    if !response.passed {
        std::process::exit(1);
    }
    Ok(())
}

fn changed_function_symbols(
    repo: &Path,
    backend: &rbuilder_graph::backend::MemoryBackend,
) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(repo)
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let files = String::from_utf8_lossy(&out.stdout);
            let paths: Vec<String> = files
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if !paths.is_empty() {
                let mut symbols = Vec::new();
                for node in backend.all_nodes()? {
                    if node.node_type != NodeType::Function {
                        continue;
                    }
                    if let Some(ref fp) = node.file_path {
                        if paths.iter().any(|p| fp.ends_with(p) || p.ends_with(fp)) {
                            symbols.push(node.name.clone());
                        }
                    }
                }
                if !symbols.is_empty() {
                    return Ok(symbols);
                }
            }
        }
    }

    Ok(backend
        .collect_nodes_by_type(NodeType::Function)?
        .into_iter()
        .map(|n| n.name)
        .collect())
}
