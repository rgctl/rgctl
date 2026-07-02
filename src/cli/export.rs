//! `rbuilder export` — graph serialization.

use super::args::ExportFormat;
use super::context::CliContext;
use anyhow::Result;
use crate::export::{export_graphml, generate_dot, generate_mermaid};
use crate::export::{GraphvizOptions, MermaidOptions};

pub struct ExportArgs {
    pub export_format: ExportFormat,
    pub export_output: String,
    pub query: String,
}

pub fn run(ctx: &CliContext, args: ExportArgs) -> Result<()> {
    let graph = ctx.load_graph()?;
    let backend = graph.backend();

    match args.export_format {
        ExportFormat::Graphml => {
            let content = export_graphml(backend, &args.query)?;
            std::fs::write(&args.export_output, content)?;
        }
        ExportFormat::Json => {
            std::fs::write(&args.export_output, graph.export_json()?)?;
        }
        ExportFormat::Graphviz => {
            let dot = generate_dot(
                backend,
                &args.query,
                GraphvizOptions::default(),
                None,
            )
            .map_err(|e| anyhow::anyhow!(e))?;
            std::fs::write(&args.export_output, dot)?;
        }
        ExportFormat::Mermaid => {
            let mmd = generate_mermaid(backend, &args.query, MermaidOptions::default())
                .map_err(|e| anyhow::anyhow!(e))?;
            std::fs::write(&args.export_output, mmd)?;
        }
    }

    if ctx.output.is_none() {
        println!(
            "Exported {} nodes, {} edges -> {}",
            graph.node_count(),
            graph.edge_count(),
            args.export_output
        );
    }
    Ok(())
}
