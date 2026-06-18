//! Chef-specific CLI commands (Phase 17).

use crate::analysis::chef_cookbooks::{
    CookbookDependencyAnalyzer, CookbookDependencyGraph,
};
use crate::error::{Error, Result};
use crate::graph::CodeGraph;
use crate::languages::multimodal::chef::parser::ChefParser;
use crate::security::chef::{ChefSecurityFinding, ChefSecurityScanner, ChefSeverity};
use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};

/// Chef subcommand arguments.
#[derive(Debug, Args)]
pub struct ChefArgs {
    #[command(subcommand)]
    /// Chef operation
    pub command: ChefCommand,
}

/// Chef CLI operations.
#[derive(Debug, Subcommand)]
pub enum ChefCommand {
    /// Analyze Chef cookbooks and show dependencies
    Cookbooks {
        /// Path to cookbooks directory
        #[arg(default_value = "./cookbooks")]
        path: PathBuf,
        #[arg(long)]
        show_deps: bool,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        from_graph: bool,
    },
    /// Validate Chef recipes
    Validate {
        path: PathBuf,
    },
    /// Run security scan on Chef cookbooks
    SecurityScan {
        path: PathBuf,
        #[arg(long, default_value = "medium")]
        min_severity: String,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        from_graph: bool,
    },
}

/// Run chef subcommand.
pub fn run_chef_command(repo: &Path, args: ChefArgs) -> Result<()> {
    match args.command {
        ChefCommand::Cookbooks {
            path,
            show_deps,
            format,
            from_graph,
        } => {
            let graph = if from_graph || repo.join(".rbuilder").exists() {
                let cg = CodeGraph::load_from_repo(repo)?;
                CookbookDependencyGraph::from_graph(cg.backend())?
            } else {
                CookbookDependencyAnalyzer::new().analyze_cookbooks_dir(&path)?
            };
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&graph.cookbooks)?),
                "mermaid" => print_cookbooks_mermaid(&graph),
                _ => print_cookbooks_text(&graph, show_deps),
            }
            Ok(())
        }
        ChefCommand::Validate { path } => validate_chef_path(&path),
        ChefCommand::SecurityScan {
            path,
            min_severity,
            format,
            from_graph,
        } => {
            let min = parse_severity(&min_severity)?;
            let findings = if from_graph || repo.join(".rbuilder").exists() {
                let graph = CodeGraph::load_from_repo(repo)?;
                ChefSecurityScanner::new().scan_graph(graph.backend())
            } else {
                scan_cookbooks_on_disk(&path)?
            };
            let filtered = ChefSecurityScanner::filter_by_severity(findings, min);
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&filtered)?),
                _ => print_findings_text(&filtered),
            }
            Ok(())
        }
    }
}

fn print_cookbooks_text(graph: &CookbookDependencyGraph, show_deps: bool) {
    println!("Chef Cookbooks: {}", graph.cookbooks.len());
    for (name, node) in &graph.cookbooks {
        println!("Cookbook: {name} (v{})", node.version);
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
        for (i, cb) in sorted.iter().enumerate() {
            println!("  {}. {cb}", i + 1);
        }
    }
}

fn print_cookbooks_mermaid(graph: &CookbookDependencyGraph) {
    println!("graph TD");
    for (name, node) in &graph.cookbooks {
        for dep in &node.dependencies {
            println!("    {name}[{name}] --> {dep}[{dep}]");
        }
    }
}

fn validate_chef_path(path: &Path) -> Result<()> {
    let parser = ChefParser::new();
    if path.extension().and_then(|e| e.to_str()) == Some("rb")
        && ChefParser::is_chef_path(&path.to_string_lossy())
    {
        let content = std::fs::read_to_string(path)?;
        let (symbols, _) = parser.parse(&path.to_string_lossy(), &content);
        let recipes = symbols
            .iter()
            .filter(|s| {
                s.symbol_type == crate::languages::plugin_trait::SymbolType::ChefRecipe
            })
            .count();
        if recipes == 0 && !path.to_string_lossy().contains("metadata.rb") {
            return Err(Error::ParseError {
                file: path.to_path_buf(),
                line: 1,
                message: "No Chef recipe content found".into(),
            });
        }
        println!("Valid Chef file: {recipes} recipe symbol(s)");
        return Ok(());
    }
    if path.is_dir() {
        walk_rb_files(path, &mut |file_path| {
            if !ChefParser::is_chef_path(&file_path.to_string_lossy()) {
                return Ok(());
            }
            let content = std::fs::read_to_string(&file_path)?;
            let (symbols, _) = parser.parse(&file_path.to_string_lossy(), &content);
            let count = symbols.len();
            if count > 0 {
                println!("{}: {count} symbol(s)", file_path.display());
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn scan_cookbooks_on_disk(path: &Path) -> Result<Vec<ChefSecurityFinding>> {
    let parser = ChefParser::new();
    let scanner = ChefSecurityScanner::new();
    let mut findings = Vec::new();
    walk_rb_files(path, &mut |file_path| {
        if !ChefParser::is_chef_path(&file_path.to_string_lossy()) {
            return Ok(());
        }
        let content = std::fs::read_to_string(&file_path)?;
        let (symbols, _) = parser.parse(&file_path.to_string_lossy(), &content);
        for sym in symbols {
            if sym.symbol_type != crate::languages::plugin_trait::SymbolType::ChefResource {
                continue;
            }
            let mut node = crate::graph::schema::Node::new(
                crate::graph::schema::NodeType::ChefResource,
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

fn walk_rb_files(path: &Path, f: &mut dyn FnMut(PathBuf) -> Result<()>) -> Result<()> {
    if path.is_file() {
        return f(path.to_path_buf());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            walk_rb_files(&p, f)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("rb")
            || p.extension().and_then(|e| e.to_str()) == Some("erb")
        {
            f(p)?;
        }
    }
    Ok(())
}

fn parse_severity(value: &str) -> Result<ChefSeverity> {
    match value.to_ascii_lowercase().as_str() {
        "low" => Ok(ChefSeverity::Low),
        "medium" => Ok(ChefSeverity::Medium),
        "high" => Ok(ChefSeverity::High),
        "critical" => Ok(ChefSeverity::Critical),
        other => Err(Error::InvalidQuery(format!("Unknown severity: {other}"))),
    }
}

fn print_findings_text(findings: &[ChefSecurityFinding]) {
    if findings.is_empty() {
        println!("No security findings.");
        return;
    }
    for f in findings {
        println!("[{:?}] {} — {}", f.severity, f.location, f.message);
        if let Some(cwe) = &f.cwe {
            println!("  CWE: {cwe}");
        }
    }
}
