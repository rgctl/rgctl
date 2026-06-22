//! Ansible-specific CLI commands (Phase 16).

use crate::analysis::{RoleDependencyAnalyzer, RoleDependencyGraph};
use crate::plugin as ansible;
use crate::security::{AnsibleSecurityFinding, AnsibleSecurityScanner, AnsibleSeverity};
use clap::{Args, Subcommand};
use rbuilder_error::{Error, Result};
use rbuilder_graph::CodeGraph;
use rbuilder_plugin_api::SymbolType;
use std::path::{Path, PathBuf};

/// Ansible subcommand arguments.
#[derive(Debug, Args)]
pub struct AnsibleArgs {
    #[command(subcommand)]
    /// Ansible operation
    pub command: AnsibleCommand,
}

/// Ansible CLI operations.
#[derive(Debug, Subcommand)]
pub enum AnsibleCommand {
    /// Analyze Ansible roles and show dependencies
    Roles {
        /// Path to roles directory (or repo root to use graph)
        #[arg(default_value = "./roles")]
        path: PathBuf,
        /// Show dependency graph details
        #[arg(long)]
        show_deps: bool,
        /// Output format: text, json, mermaid
        #[arg(long, default_value = "text")]
        format: String,
        /// Load from indexed graph instead of filesystem
        #[arg(long)]
        from_graph: bool,
    },
    /// Validate Ansible playbooks
    Validate {
        /// Path to playbook or directory
        path: PathBuf,
    },
    /// Run security scan on Ansible tasks
    SecurityScan {
        /// Path to repo (uses graph) or playbook directory
        path: PathBuf,
        /// Minimum severity: low, medium, high, critical
        #[arg(long, default_value = "medium")]
        min_severity: String,
        /// Output format: text, json
        #[arg(long, default_value = "text")]
        format: String,
        /// Scan indexed graph (default when .rbuilder exists)
        #[arg(long)]
        from_graph: bool,
    },
}

/// Run ansible subcommand.
pub fn run_ansible_command(repo: &Path, args: AnsibleArgs) -> Result<()> {
    match args.command {
        AnsibleCommand::Roles {
            path,
            show_deps,
            format,
            from_graph,
        } => {
            let graph = if from_graph || repo.join(".rbuilder").exists() {
                let cg = CodeGraph::load_from_repo(repo)?;
                RoleDependencyGraph::from_graph(cg.backend())?
            } else {
                RoleDependencyAnalyzer::new().analyze_roles_dir(&path)?
            };
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&graph.roles)?),
                "mermaid" => print_roles_mermaid(&graph),
                _ => print_roles_text(&graph, show_deps),
            }
            Ok(())
        }
        AnsibleCommand::Validate { path } => validate_ansible_path(&path),
        AnsibleCommand::SecurityScan {
            path,
            min_severity,
            format,
            from_graph,
        } => {
            let min = parse_severity(&min_severity)?;
            let findings = if from_graph || repo.join(".rbuilder").exists() {
                let graph = CodeGraph::load_from_repo(repo)?;
                AnsibleSecurityScanner::new().scan_graph(graph.backend())
            } else {
                scan_playbooks_on_disk(&path)?
            };
            let filtered = AnsibleSecurityScanner::filter_by_severity(findings, min);
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&filtered)?),
                _ => print_findings_text(&filtered),
            }
            Ok(())
        }
    }
}

fn print_roles_text(graph: &RoleDependencyGraph, show_deps: bool) {
    println!("Ansible Roles: {}", graph.roles.len());
    println!();
    for (name, node) in &graph.roles {
        println!("Role: {name}");
        if !node.path.is_empty() {
            println!("  Path: {}", node.path);
        }
        if show_deps {
            if !node.dependencies.is_empty() {
                println!("  Dependencies:");
                for dep in &node.dependencies {
                    println!("    - {dep}");
                }
            }
            if !node.dependents.is_empty() {
                println!("  Dependents:");
                for dep in &node.dependents {
                    println!("    - {dep}");
                }
            }
        }
        println!();
    }
    if let Ok(sorted) = graph.topological_sort() {
        println!("Dependency order:");
        for (i, role) in sorted.iter().enumerate() {
            println!("  {}. {role}", i + 1);
        }
    }
}

fn print_roles_mermaid(graph: &RoleDependencyGraph) {
    println!("graph TD");
    for (name, node) in &graph.roles {
        for dep in &node.dependencies {
            println!("    {name}[{name}] --> {dep}[{dep}]");
        }
    }
}

fn validate_ansible_path(path: &Path) -> Result<()> {
    if path.is_file() {
        let content = std::fs::read_to_string(path)?;
        let value: serde_yaml::Value = serde_yaml::from_str(&content)?;
        let (symbols, _) = ansible::parse_content(&path.to_string_lossy(), &value, &content);
        let plays = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::AnsiblePlay)
            .count();
        if plays == 0 && symbols.is_empty() {
            return Err(Error::ParseError {
                file: path.to_path_buf(),
                line: 1,
                message: "No Ansible plays found".into(),
            });
        }
        println!("Valid playbook: {} plays", plays);
        return Ok(());
    }
    if path.is_dir() {
        walk_yaml_files(path, &mut |file_path| {
            if !ansible::matches_path(&file_path.to_string_lossy()) {
                return Ok(());
            }
            let content = std::fs::read_to_string(&file_path)?;
            let value: serde_yaml::Value = serde_yaml::from_str(&content).unwrap_or_default();
            let (symbols, _) =
                ansible::parse_content(&file_path.to_string_lossy(), &value, &content);
            let plays = symbols
                .iter()
                .filter(|s| s.symbol_type == rbuilder_plugin_api::SymbolType::AnsiblePlay)
                .count();
            if plays > 0 {
                println!("{}: {plays} play(s)", file_path.display());
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn scan_playbooks_on_disk(path: &Path) -> Result<Vec<AnsibleSecurityFinding>> {
    let scanner = AnsibleSecurityScanner::new();
    let mut findings = Vec::new();
    let files: Vec<PathBuf> = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        let mut collected = Vec::new();
        walk_yaml_files(path, &mut |p| {
            if ansible::matches_path(&p.to_string_lossy()) {
                collected.push(p);
            }
            Ok(())
        })?;
        collected
    };
    for file_path in files {
        let content = std::fs::read_to_string(&file_path)?;
        let value: serde_yaml::Value = serde_yaml::from_str(&content).unwrap_or_default();
        let (symbols, _) = ansible::parse_content(&file_path.to_string_lossy(), &value, &content);
        for sym in symbols {
            if sym.symbol_type != SymbolType::AnsibleTask
                && sym.symbol_type != SymbolType::AnsibleHandler
            {
                continue;
            }
            let mut node = rbuilder_graph::schema::Node::new(
                if sym.symbol_type == SymbolType::AnsibleHandler {
                    rbuilder_graph::schema::NodeType::AnsibleHandler
                } else {
                    rbuilder_graph::schema::NodeType::AnsibleTask
                },
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
    }
    Ok(findings)
}

fn walk_yaml_files(path: &Path, f: &mut dyn FnMut(PathBuf) -> Result<()>) -> Result<()> {
    if path.is_file() {
        return f(path.to_path_buf());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            walk_yaml_files(&p, f)?;
        } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if ext == "yml" || ext == "yaml" || ext == "j2" {
                f(p)?;
            }
        }
    }
    Ok(())
}

fn parse_severity(value: &str) -> Result<AnsibleSeverity> {
    match value.to_ascii_lowercase().as_str() {
        "low" => Ok(AnsibleSeverity::Low),
        "medium" => Ok(AnsibleSeverity::Medium),
        "high" => Ok(AnsibleSeverity::High),
        "critical" => Ok(AnsibleSeverity::Critical),
        other => Err(Error::InvalidQuery(format!("Unknown severity: {other}"))),
    }
}

fn print_findings_text(findings: &[AnsibleSecurityFinding]) {
    if findings.is_empty() {
        println!("No security findings.");
        return;
    }
    for f in findings {
        println!("[{:?}] {} — {}", f.severity, f.location, f.message);
        if let Some(cwe) = &f.cwe {
            println!("  CWE: {cwe}");
        }
        if let Some(rem) = &f.remediation {
            println!("  Fix: {rem}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_severity() {
        assert_eq!(parse_severity("high").unwrap(), AnsibleSeverity::High);
    }
}
