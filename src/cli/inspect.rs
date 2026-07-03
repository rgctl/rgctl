//! `rbuilder inspect` — raw CFG / PDG / dominance debugging.

use super::args::{InspectLayer, OutputFormat, PdgEdgeLayer};
use super::context::{language_from_path, CliContext};
use super::inspect_output::{inspect_cfg_json, inspect_dom_json, inspect_pdg_json};
use anyhow::Result;
use crate::analysis::{
    build_cfg_for_function, DominatorTree, ProgramDependenceGraph,
};
use std::path::Path;

pub struct InspectArgs {
    pub symbol: String,
    pub layer: InspectLayer,
}

pub fn run(ctx: &CliContext, args: InspectArgs) -> Result<()> {
    let (node, source) = resolve_symbol_function(ctx, &args.symbol)?;
    let file = node.file_path.as_deref().unwrap_or(".");
    let lang = language_from_path(Path::new(file));
    let mut cfg = build_cfg_for_function(&lang, &source, &node.name)?;
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
    let dom = DominatorTree::build(&cfg);

    match args.layer {
        InspectLayer::Cfg { prune } => {
            if prune {
                cfg.prune_unreachable_blocks();
            }
            match ctx.format {
                OutputFormat::Json => {
                    let response = inspect_cfg_json(&args.symbol, &cfg, prune);
                    ctx.emit_json_value(&serde_json::to_value(&response)?)?;
                }
                OutputFormat::Mermaid => {
                    ctx.emit(&cfg_to_mermaid(&cfg))?;
                }
                OutputFormat::Graphviz => {
                    ctx.emit(&cfg_to_dot(&cfg))?;
                }
                OutputFormat::Text => {
                    println!(
                        "CFG for {}: {} blocks, {} edges",
                        args.symbol,
                        cfg.blocks.len(),
                        cfg.edges.len()
                    );
                }
            }
        }
        InspectLayer::Pdg { edge_layer, def_use } => {
            let (data, control) = match edge_layer {
                PdgEdgeLayer::All => (pdg.data_deps.len(), pdg.control_deps.len()),
                PdgEdgeLayer::Data => (pdg.data_deps.len(), 0),
                PdgEdgeLayer::Control => (0, pdg.control_deps.len()),
            };
            if ctx.format == OutputFormat::Json {
                let response = inspect_pdg_json(&args.symbol, &pdg, def_use, data, control);
                ctx.emit_json_value(&serde_json::to_value(&response)?)?;
            } else {
                println!(
                    "PDG for {}: {} nodes, {} data deps, {} control deps",
                    args.symbol,
                    pdg.nodes.len(),
                    data,
                    control
                );
            }
        }
        InspectLayer::Dom { frontiers } => {
            if ctx.format == OutputFormat::Json {
                let response = inspect_dom_json(&args.symbol, &cfg, &dom, frontiers);
                ctx.emit_json_value(&serde_json::to_value(&response)?)?;
            } else if ctx.format == OutputFormat::Mermaid {
                ctx.emit(&dom_to_mermaid(&dom))?;
            } else {
                println!("Dominators for {}: {} blocks", args.symbol, dom.idom.len());
                if frontiers {
                    for (block, frontier) in &dom.frontiers {
                        if !frontier.is_empty() {
                            println!("  DF({block:?}): {frontier:?}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn cfg_to_dot(cfg: &crate::analysis::ControlFlowGraph) -> String {
    let mut out = String::from("digraph cfg {\n");
    for edge in &cfg.edges {
        out.push_str(&format!("  {:?} -> {:?};\n", edge.from, edge.to));
    }
    out.push_str("}\n");
    out
}

fn cfg_to_mermaid(cfg: &crate::analysis::ControlFlowGraph) -> String {
    let mut out = String::from("flowchart TD\n");
    for edge in &cfg.edges {
        out.push_str(&format!("  {:?} --> {:?}\n", edge.from, edge.to));
    }
    out
}

fn dom_to_mermaid(dom: &DominatorTree) -> String {
    let mut out = String::from("flowchart TD\n");
    for (child, parent) in &dom.idom {
        out.push_str(&format!("  {:?} --> {:?}\n", parent, child));
    }
    out
}

fn resolve_symbol_function(
    ctx: &CliContext,
    symbol: &str,
) -> Result<(rbuilder_graph::schema::Node, String)> {
    use rbuilder_graph::schema::NodeType;
    use std::fs;

    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let matches = backend.find_nodes_by_name(symbol)?;
    let node = matches
        .into_iter()
        .find(|n| n.node_type == NodeType::Function)
        .or_else(|| {
            backend
                .all_nodes()
                .ok()?
                .into_iter()
                .find(|n| n.name == symbol || n.name.ends_with(symbol))
        })
        .ok_or_else(|| anyhow::anyhow!("function symbol not found: {symbol}"))?;
    let file = node
        .file_path
        .clone()
        .ok_or_else(|| anyhow::anyhow!("function has no file path"))?;
    let source = fs::read_to_string(&file)?;
    Ok((node, source))
}
