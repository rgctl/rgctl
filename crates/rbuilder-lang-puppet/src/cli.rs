//! Puppet-specific CLI commands (Phase 18).

use crate::analysis::{ModuleDependencyAnalyzer, ModuleDependencyGraph};
use crate::plugin as puppet;
use crate::security::{PuppetSecurityFinding, PuppetSecurityScanner, PuppetSeverity};
use clap::{Args, Subcommand};
use rbuilder_error::{Error, Result};
use rbuilder_graph::CodeGraph;
use std::path::{Path, PathBuf};

/// Puppet subcommand arguments.
#[derive(Debug, Args)]
pub struct PuppetArgs {
    #[command(subcommand)]
    /// Puppet operation
    pub command: PuppetCommand,
}

/// Puppet CLI operations.
#[derive(Debug, Subcommand)]
pub enum PuppetCommand {
    /// Analyze Puppet modules and show dependencies
    Modules {
        /// Path to modules directory
        #[arg(default_value = "./modules")]
        path: PathBuf,
        /// Show dependency relationships between modules
        #[arg(long)]
        show_deps: bool,
        /// Output format (`text` or `json`)
        #[arg(long, default_value = "text")]
        format: String,
        /// Build dependency graph from indexed knowledge graph instead of filesystem scan
        #[arg(long)]
        from_graph: bool,
    },
    /// Validate Puppet manifests
    Validate {
        /// Path to manifests or modules directory
        path: PathBuf,
    },
    /// Run security scan on Puppet modules
    SecurityScan {
        /// Path to modules directory
        path: PathBuf,
        /// Minimum severity to report (`low`, `medium`, `high`, `critical`)
        #[arg(long, default_value = "medium")]
        min_severity: String,
        /// Output format (`text` or `json`)
        #[arg(long, default_value = "text")]
        format: String,
        /// Scan indexed graph instead of filesystem
        #[arg(long)]
        from_graph: bool,
    },
}

/// Run puppet subcommand.
pub fn run_puppet_command(repo: &Path, args: PuppetArgs) -> Result<()> {
    match args.command {
        PuppetCommand::Modules {
            path,
            show_deps,
            format,
            from_graph,
        } => {
            let graph = if from_graph || repo.join(".rbuilder").exists() {
                let cg = CodeGraph::load_from_repo(repo)?;
                ModuleDependencyGraph::from_graph(cg.backend())?
            } else {
                ModuleDependencyAnalyzer::new().analyze_modules_dir(&path)?
            };
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&graph.modules)?),
                "mermaid" => print_modules_mermaid(&graph),
                _ => print_modules_text(&graph, show_deps),
            }
            Ok(())
        }
        PuppetCommand::Validate { path } => validate_puppet_path(&path),
        PuppetCommand::SecurityScan {
            path,
            min_severity,
            format,
            from_graph,
        } => {
            let min = parse_severity(&min_severity)?;
            let findings = if from_graph || repo.join(".rbuilder").exists() {
                let graph = CodeGraph::load_from_repo(repo)?;
                PuppetSecurityScanner::new().scan_graph(graph.backend())
            } else {
                scan_modules_on_disk(&path)?
            };
            let filtered = PuppetSecurityScanner::filter_by_severity(findings, min);
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&filtered)?),
                _ => print_findings_text(&filtered),
            }
            Ok(())
        }
    }
}

fn print_modules_text(graph: &ModuleDependencyGraph, show_deps: bool) {
    println!("Puppet Modules: {}", graph.modules.len());
    for (name, node) in &graph.modules {
        println!("Module: {name} (v{})", node.version);
        if !node.path.is_empty() {
            println!("  Path: {}", node.path);
        }
        if show_deps && !node.dependencies.is_empty() {
            println!("  Dependencies:");
            for dep in &node.dependencies {
                println!("    - {dep}");
            }
        }
    }
    if let Ok(sorted) = graph.topological_sort() {
        println!("Dependency order:");
        for (i, m) in sorted.iter().enumerate() {
            println!("  {}. {m}", i + 1);
        }
    }
}

fn print_modules_mermaid(graph: &ModuleDependencyGraph) {
    println!("graph TD");
    for (name, node) in &graph.modules {
        for dep in &node.dependencies {
            println!("    {name}[{name}] --> {dep}[{dep}]");
        }
    }
}

fn validate_puppet_path(path: &Path) -> Result<()> {
    if path.extension().and_then(|e| e.to_str()) == Some("pp")
        && puppet::matches_path(&path.to_string_lossy())
    {
        let content = std::fs::read_to_string(path)?;
        let (symbols, _) = puppet::parse_content(&path.to_string_lossy(), &content);
        let classes = symbols
            .iter()
            .filter(|s| s.symbol_type == rbuilder_plugin_api::SymbolType::PuppetClass)
            .count();
        if classes == 0 {
            return Err(Error::ParseError {
                file: path.to_path_buf(),
                line: 1,
                message: "No Puppet class content found".into(),
            });
        }
        println!("Valid Puppet manifest: {classes} class symbol(s)");
        return Ok(());
    }
    if path.is_dir() {
        walk_pp_files(path, &mut |file_path| {
            if !puppet::matches_path(&file_path.to_string_lossy()) {
                return Ok(());
            }
            let content = std::fs::read_to_string(&file_path)?;
            let (symbols, _) = puppet::parse_content(&file_path.to_string_lossy(), &content);
            let count = symbols.len();
            if count > 0 {
                println!("{}: {count} symbol(s)", file_path.display());
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn scan_modules_on_disk(path: &Path) -> Result<Vec<PuppetSecurityFinding>> {
    let scanner = PuppetSecurityScanner::new();
    let mut findings = Vec::new();
    walk_pp_files(path, &mut |file_path| {
        if !puppet::matches_path(&file_path.to_string_lossy()) {
            return Ok(());
        }
        let content = std::fs::read_to_string(&file_path)?;
        let (symbols, _) = puppet::parse_content(&file_path.to_string_lossy(), &content);
        for sym in symbols {
            if sym.symbol_type != rbuilder_plugin_api::SymbolType::PuppetResource {
                continue;
            }
            let mut node = rbuilder_graph::schema::Node::new(
                rbuilder_graph::schema::NodeType::PuppetResource,
                sym.name,
            );
            node.signature = sym.signature;
            if let Some(obj) = sym.metadata.as_object() {
                for (k, v) in obj {
                    if let Some(s) = v.as_str() {
                        node.properties.insert(k.clone(), s.to_string());
                    }
                }
            }
            findings.extend(scanner.scan_node(&node));
        }
        Ok(())
    })?;
    Ok(findings)
}

fn walk_pp_files(path: &Path, f: &mut dyn FnMut(PathBuf) -> Result<()>) -> Result<()> {
    if path.is_file() {
        return f(path.to_path_buf());
    }
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                walk_pp_files(&p, f)?;
            } else if p.extension().and_then(|e| e.to_str()) == Some("pp")
                || p.file_name().and_then(|n| n.to_str()) == Some("metadata.json")
            {
                f(p)?;
            }
        }
    }
    Ok(())
}

fn parse_severity(s: &str) -> Result<PuppetSeverity> {
    match s.to_ascii_lowercase().as_str() {
        "low" => Ok(PuppetSeverity::Low),
        "medium" => Ok(PuppetSeverity::Medium),
        "high" => Ok(PuppetSeverity::High),
        "critical" => Ok(PuppetSeverity::Critical),
        other => Err(Error::InvalidQuery(format!("Unknown severity: {other}"))),
    }
}

fn print_findings_text(findings: &[PuppetSecurityFinding]) {
    if findings.is_empty() {
        println!("No security findings.");
        return;
    }
    for f in findings {
        println!("[{:?}] {}", f.severity, f.message);
        if let Some(cwe) = &f.cwe {
            println!("  CWE: {cwe}");
        }
        if let Some(rem) = &f.remediation {
            println!("  Fix: {rem}");
        }
    }
}
