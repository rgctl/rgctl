//! `rgctl inspect` — raw CFG / PDG / dominance debugging.

use super::args::{InspectLayer, OutputFormat, PdgEdgeLayer};
use super::context::{CliContext, language_from_path};
use super::inspect_output::{inspect_cfg_json, inspect_dom_json, inspect_pdg_json};
use super::markup::markup_context_unsupported;
use crate::analysis::{DominatorTree, ProgramDependenceGraph, build_cfg_for_function};
use anyhow::Result;
use rgctl_error::SymbolCandidate;
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Node, NodeType};
use std::path::Path;

pub struct InspectArgs {
    pub symbol: String,
    pub file: Option<String>,
    pub class: Option<String>,
    pub line: Option<usize>,
    pub layer: InspectLayer,
}

pub fn run(ctx: &CliContext, args: InspectArgs) -> Result<()> {
    let (node, source) = resolve_symbol_function(
        ctx,
        &args.symbol,
        args.file.as_deref(),
        args.class.as_deref(),
        args.line,
    )?;
    let file = node.file_path.as_deref().unwrap_or(".");
    if let Some(msg) = markup_context_unsupported("inspect", Path::new(file)) {
        anyhow::bail!(msg);
    }
    let lang = language_from_path(Path::new(file));
    let display_name = node.name.as_str();
    let mut cfg = build_cfg_for_function(&lang, &source, display_name)?;
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
    let dom = DominatorTree::build(&cfg);

    match args.layer {
        InspectLayer::Cfg { prune } => {
            if prune {
                cfg.prune_unreachable_blocks();
            }
            match ctx.format {
                OutputFormat::Json => {
                    let response = inspect_cfg_json(display_name, &cfg, prune);
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
                        display_name,
                        cfg.blocks.len(),
                        cfg.edges.len()
                    );
                    print_cfg_text_summary(&cfg);
                }
            }
        }
        InspectLayer::Pdg {
            edge_layer,
            def_use,
        } => {
            let (data, control) = match edge_layer {
                PdgEdgeLayer::All => (pdg.data_deps.len(), pdg.control_deps.len()),
                PdgEdgeLayer::Data => (pdg.data_deps.len(), 0),
                PdgEdgeLayer::Control => (0, pdg.control_deps.len()),
            };
            if ctx.format == OutputFormat::Json {
                let response = inspect_pdg_json(display_name, &pdg, def_use, data, control);
                ctx.emit_json_value(&serde_json::to_value(&response)?)?;
            } else {
                println!(
                    "PDG for {}: {} nodes, {} data deps, {} control deps",
                    display_name,
                    pdg.nodes.len(),
                    data,
                    control
                );
                print_pdg_text_summary(&pdg, edge_layer);
            }
        }
        InspectLayer::Dom { frontiers } => {
            if ctx.format == OutputFormat::Json {
                let response = inspect_dom_json(display_name, &cfg, &dom, frontiers);
                ctx.emit_json_value(&serde_json::to_value(&response)?)?;
            } else if ctx.format == OutputFormat::Mermaid {
                ctx.emit(&dom_to_mermaid(&dom))?;
            } else {
                println!("Dominators for {}: {} blocks", display_name, dom.idom.len());
                print_dom_text_summary(&dom, frontiers);
            }
        }
    }
    Ok(())
}

fn print_cfg_text_summary(cfg: &crate::analysis::ControlFlowGraph) {
    let mut blocks: Vec<_> = cfg.blocks.values().collect();
    blocks.sort_by_key(|b| b.id.0);
    for b in blocks.iter().take(40) {
        let kind = if b.id == cfg.entry {
            "entry"
        } else if cfg.exits.contains(&b.id) {
            "exit"
        } else {
            "block"
        };
        println!(
            "  B{:<4} {kind:<5} L{}-{}  {} stmts",
            b.id.0,
            b.start_line,
            b.end_line,
            b.statements.len()
        );
    }
    if blocks.len() > 40 {
        println!("  ... {} more blocks", blocks.len() - 40);
    }
    for e in cfg.edges.iter().take(60) {
        println!("  B{} -> B{}  {:?}", e.from.0, e.to.0, e.edge_type);
    }
    if cfg.edges.len() > 60 {
        println!("  ... {} more edges", cfg.edges.len() - 60);
    }
}

fn print_pdg_text_summary(pdg: &ProgramDependenceGraph, layer: PdgEdgeLayer) {
    let show_data = matches!(layer, PdgEdgeLayer::All | PdgEdgeLayer::Data);
    let show_ctrl = matches!(layer, PdgEdgeLayer::All | PdgEdgeLayer::Control);
    if show_data {
        for d in pdg.data_deps.iter().take(30) {
            println!(
                "  data  {} -> {}  {:?}",
                short_id(&d.from),
                short_id(&d.to),
                d.dep_type
            );
        }
        if pdg.data_deps.len() > 30 {
            println!("  ... {} more data deps", pdg.data_deps.len() - 30);
        }
    }
    if show_ctrl {
        for c in pdg.control_deps.iter().take(30) {
            println!(
                "  ctrl  {} -> {}",
                short_id(&c.controller),
                short_id(&c.dependent)
            );
        }
        if pdg.control_deps.len() > 30 {
            println!("  ... {} more control deps", pdg.control_deps.len() - 30);
        }
    }
}

fn print_dom_text_summary(dom: &DominatorTree, frontiers: bool) {
    let mut ids: Vec<_> = dom.idom.keys().copied().collect();
    ids.sort_by_key(|id| id.0);
    for id in ids.iter().take(40) {
        let parent = dom
            .idom
            .get(id)
            .map(|p| format!("B{}", p.0))
            .unwrap_or_else(|| "-".into());
        println!("  B{} idom {parent}", id.0);
        if frontiers {
            if let Some(df) = dom.frontiers.get(id) {
                if !df.is_empty() {
                    let list: Vec<_> = df.iter().map(|b| format!("B{}", b.0)).collect();
                    println!("       DF {{{}}}", list.join(", "));
                }
            }
        }
    }
    if ids.len() > 40 {
        println!("  ... {} more blocks", ids.len() - 40);
    }
}

fn short_id(id: &uuid::Uuid) -> String {
    id.to_string()[..8].to_string()
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

fn node_to_symbol_candidate(n: &Node) -> SymbolCandidate {
    SymbolCandidate {
        id: n.id.to_string(),
        name: n.name.to_string(),
        qualified_name: n.qualified_name.as_ref().map(|s| s.to_string()),
        node_type: "function".into(),
        file: n.file_path.as_ref().map(|s| s.to_string()),
        line: n.start_line,
    }
}

/// Fuzzy name suggestions for NotFound (substring / length distance; capped).
pub(crate) fn suggest_function_names(backend: &MemoryBackend, needle: &str, limit: usize) -> Vec<String> {
    let needle_l = needle.to_ascii_lowercase();
    let Ok(nodes) = backend.all_nodes() else {
        return Vec::new();
    };
    let mut scored: Vec<(usize, String)> = nodes
        .into_iter()
        .filter(|n| n.node_type == NodeType::Function)
        .filter_map(|n| {
            let name = n.name.to_string();
            let lower = name.to_ascii_lowercase();
            if lower.contains(&needle_l) || needle_l.contains(&lower) {
                let dist = lower.len().abs_diff(needle_l.len());
                Some((dist, name))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by_key(|(d, _)| *d);
    scored.dedup_by(|a, b| a.1 == b.1);
    scored.into_iter().take(limit).map(|(_, n)| n).collect()
}

fn filter_nodes_by_line(nodes: &mut Vec<Node>, line: Option<usize>) {
    if let Some(line) = line {
        nodes.retain(|n| n.start_line == Some(line));
    }
}

fn filter_nodes_by_file(nodes: &mut Vec<Node>, file: Option<&str>) {
    if let Some(file) = file {
        nodes.retain(|n| {
            n.file_path.as_deref().is_some_and(|p| {
                let p = p.replace('\\', "/");
                let f = file.replace('\\', "/");
                p.contains(&f) || p.ends_with(&f)
            })
        });
    }
}

fn filter_nodes_by_class(nodes: &mut Vec<Node>, class: Option<&str>) {
    if let Some(class) = class {
        nodes.retain(|n| {
            n.qualified_name
                .as_deref()
                .is_some_and(|q| q.contains(class))
                || n.name.contains(class)
        });
    }
}

fn emit_ambiguous(
    ctx: &CliContext,
    name: &str,
    count: usize,
    nodes: &[Node],
) -> Result<(Node, String)> {
    let rows: Vec<SymbolCandidate> = nodes.iter().map(node_to_symbol_candidate).collect();
    let recovery = format!("rgctl inspect {name} cfg --file <PATH> --class <CLASS> --line <N>");
    if ctx.format == OutputFormat::Json {
        let _ = ctx.emit_json_error(
            "ambiguous_symbol",
            "AMBIGUOUS_SYMBOL",
            &format!("Ambiguous symbol '{name}': {count} matches"),
            Some(&rows),
            None,
            Some(&recovery),
        );
    } else {
        eprintln!("Ambiguous symbol '{name}': {count} matches. Candidates:");
        for c in rows.iter().take(20) {
            eprintln!(
                "  - {} file={}:{} {}",
                c.id,
                c.file.as_deref().unwrap_or("?"),
                c.line.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
                c.qualified_name.as_deref().unwrap_or(&c.name)
            );
        }
        eprintln!("Disambiguate with --file, --class, and/or --line.");
    }
    anyhow::bail!(
        "Symbol '{name}' is ambiguous ({count} matches). Disambiguate with --file / --class / --line."
    )
}

fn emit_not_found(
    ctx: &CliContext,
    backend: &MemoryBackend,
    symbol: &str,
    target_name: &str,
) -> Result<(Node, String)> {
    let suggestions = suggest_function_names(backend, target_name, 5);
    let msg = format!("function symbol not found: {symbol}");
    if ctx.format == OutputFormat::Json {
        let _ = ctx.emit_json_error(
            "not_found",
            "NOT_FOUND",
            &msg,
            None,
            Some(&suggestions),
            Some("rgctl find <PATTERN> --type function"),
        );
    } else if !suggestions.is_empty() {
        eprintln!("Did you mean: {}?", suggestions.join(", "));
    }
    anyhow::bail!("{msg}")
}

fn load_source(node: Node) -> Result<(Node, String)> {
    let file = node
        .file_path
        .clone()
        .ok_or_else(|| anyhow::anyhow!("function has no file path"))?;
    let source = std::fs::read_to_string(&file)?;
    Ok((node, source))
}

fn resolve_symbol_function(
    ctx: &CliContext,
    symbol: &str,
    file: Option<&str>,
    class: Option<&str>,
    line: Option<usize>,
) -> Result<(Node, String)> {
    use rgctl_analysis::{candidates_from_backend, parse_fqn_symbol, resolve_symbol_uuid};

    let parsed = parse_fqn_symbol(
        symbol,
        class.map(str::to_string),
        file.map(str::to_string),
    );
    let graph = ctx.load_graph()?;
    let backend = graph.backend();

    let candidates = candidates_from_backend(backend, &parsed.target_name)?;
    if !candidates.is_empty() {
        match resolve_symbol_uuid(&candidates, &parsed) {
            Ok(id) => {
                let node = backend
                    .get_node(id)?
                    .ok_or_else(|| anyhow::anyhow!("function symbol not found: {symbol}"))?;
                if let Some(line) = line {
                    if node.start_line != Some(line) {
                        return emit_not_found(ctx, backend, symbol, &parsed.target_name);
                    }
                }
                return load_source(node);
            }
            Err(rgctl_error::Error::AmbiguousSymbol { name, count, .. }) => {
                let mut nodes: Vec<Node> = candidates
                    .iter()
                    .filter_map(|c| backend.get_node(c.id).ok().flatten())
                    .collect();
                filter_nodes_by_line(&mut nodes, line);
                filter_nodes_by_file(&mut nodes, file);
                filter_nodes_by_class(&mut nodes, class);
                match nodes.len() {
                    1 => return load_source(nodes.remove(0)),
                    0 => return emit_not_found(ctx, backend, symbol, &parsed.target_name),
                    n => return emit_ambiguous(ctx, &name, n.max(count), &nodes),
                }
            }
            Err(rgctl_error::Error::NotFound(_)) => {}
            Err(e) => return Err(e.into()),
        }
    }

    // Exact bare-name Function matches only (no silent first-suffix fallback).
    let mut funcs: Vec<Node> = backend
        .find_nodes_by_name(&parsed.target_name)?
        .into_iter()
        .filter(|n| n.node_type == NodeType::Function)
        .collect();
    filter_nodes_by_file(&mut funcs, parsed.file_filter.as_deref().or(file));
    filter_nodes_by_class(&mut funcs, parsed.class_filter.as_deref().or(class));
    filter_nodes_by_line(&mut funcs, line);

    match funcs.len() {
        1 => load_source(funcs.remove(0)),
        0 => emit_not_found(ctx, backend, symbol, &parsed.target_name),
        count => emit_ambiguous(ctx, symbol, count, &funcs),
    }
}
