//! `rbuilder export` — graph serialization.

use super::args::ExportFormat;
use super::context::CliContext;
use anyhow::Result;
use crate::export::{export_graphml, export_html_dashboard, generate_dot, generate_mermaid};
use crate::export::{GraphvizOptions, MermaidOptions};

pub struct ExportArgs {
    pub format: ExportFormat,
    pub output: String,
    pub query: String,
}

pub fn run(ctx: &CliContext, args: ExportArgs) -> Result<()> {
    let graph = ctx.load_graph()?;
    let backend = graph.backend();

    match args.format {
        ExportFormat::Html => {
            let analysis_dir = ctx.repo.join(".rbuilder/analysis");
            export_html_dashboard(
                backend,
                if analysis_dir.exists() {
                    Some(&analysis_dir)
                } else {
                    None
                },
                std::path::Path::new(&args.output),
            )
            .map_err(|e| anyhow::anyhow!(e))?;
        }
        ExportFormat::Graphml => {
            let content = export_graphml(backend, &args.query)?;
            std::fs::write(&args.output, content)?;
        }
        ExportFormat::Json => {
            std::fs::write(&args.output, graph.export_json()?)?;
        }
        ExportFormat::Graphviz => {
            let dot = generate_dot(
                backend,
                &args.query,
                GraphvizOptions::default(),
                None,
            )
            .map_err(|e| anyhow::anyhow!(e))?;
            std::fs::write(&args.output, dot)?;
        }
        ExportFormat::Mermaid => {
            let mmd = generate_mermaid(backend, &args.query, MermaidOptions::default())
                .map_err(|e| anyhow::anyhow!(e))?;
            std::fs::write(&args.output, mmd)?;
        }
    }

    if ctx.output.is_none() {
        println!(
            "Exported {} nodes, {} edges -> {}",
            graph.node_count(),
            graph.edge_count(),
            args.output
        );
    }
    Ok(())
}
