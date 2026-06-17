//! `rbuilder diagram` — export subgraphs as Mermaid, DOT, or images.

use crate::error::{Error, Result};
use crate::export::{
    export_graphml, generate_dot, generate_mermaid, parse_diagram_type, parse_layout,
    render_dot_to_file, DiagramType, GraphvizOptions, ImageFormat, MermaidOptions,
    RankDir,
};
use crate::graph::CodeGraph;
use std::path::{Path, PathBuf};

/// Options for the diagram command.
#[derive(Debug, Clone)]
pub struct DiagramOptions {
    /// Graph query DSL (e.g. `type:Function`, `functions`)
    pub query: String,
    /// Output format: mermaid, dot, graphml, png, svg, pdf
    pub format: String,
    /// Mermaid diagram type: flowchart, class, call-graph
    pub diagram_type: String,
    /// Output path (stdout if None)
    pub output: Option<PathBuf>,
    /// BFS expansion depth for call neighborhoods
    pub depth: Option<usize>,
    /// Graphviz layout engine
    pub layout: String,
    /// Graphviz rank direction: LR or TB
    pub rankdir: String,
}

/// Run diagram export for a repository.
pub fn run_diagram(repo: &Path, options: DiagramOptions) -> Result<()> {
    let graph = CodeGraph::load_from_repo(repo)?;
    let backend = graph.backend();
    let format = options.format.to_ascii_lowercase();

    let content = match format.as_str() {
        "mermaid" | "mmd" => {
            let mermaid = generate_mermaid(
                backend,
                &options.query,
                MermaidOptions {
                    diagram_type: parse_diagram_type(&options.diagram_type),
                    max_depth: options.depth,
                    vertical: !options.rankdir.eq_ignore_ascii_case("lr"),
                },
            )?;
            OutputPayload::Text(mermaid)
        }
        "dot" | "graphviz" => {
            let rankdir = if options.rankdir.eq_ignore_ascii_case("lr") {
                RankDir::Lr
            } else {
                RankDir::Tb
            };
            let dot = generate_dot(
                backend,
                &options.query,
                GraphvizOptions {
                    layout: parse_layout(&options.layout),
                    rankdir,
                },
                options.depth,
            )?;
            OutputPayload::Text(dot)
        }
        "graphml" => OutputPayload::Text(export_graphml(backend, &options.query)?),
        "png" | "svg" | "pdf" => {
            let rankdir = if options.rankdir.eq_ignore_ascii_case("lr") {
                RankDir::Lr
            } else {
                RankDir::Tb
            };
            let dot = generate_dot(
                backend,
                &options.query,
                GraphvizOptions {
                    layout: parse_layout(&options.layout),
                    rankdir,
                },
                options.depth,
            )?;
            let image_format = match format.as_str() {
                "svg" => ImageFormat::Svg,
                "pdf" => ImageFormat::Pdf,
                _ => ImageFormat::Png,
            };
            let output = options
                .output
                .clone()
                .ok_or_else(|| Error::Other("--output required for image formats".into()))?;
            render_dot_to_file(
                &dot,
                &output,
                image_format,
                parse_layout(&options.layout),
            )?;
            println!("Wrote diagram to {}", output.display());
            return Ok(());
        }
        other => {
            return Err(Error::Other(format!(
                "Unsupported format '{other}'. Use mermaid, dot, graphml, png, svg, or pdf."
            )));
        }
    };

    match (content, options.output) {
        (OutputPayload::Text(text), Some(path)) => {
            std::fs::write(&path, &text).map_err(|e| Error::Other(e.to_string()))?;
            println!("Wrote diagram to {}", path.display());
        }
        (OutputPayload::Text(text), None) => print!("{text}"),
    }
    Ok(())
}

enum OutputPayload {
    Text(String),
}

/// Infer diagram type from query shorthand.
pub fn infer_diagram_type(query: &str) -> DiagramType {
    let q = query.to_ascii_lowercase();
    if q.contains("class") || q.starts_with("type:class") {
        DiagramType::ClassDiagram
    } else if q.contains("call") || q == "functions" || q.starts_with("type:function") {
        DiagramType::CallGraph
    } else {
        DiagramType::Flowchart
    }
}
