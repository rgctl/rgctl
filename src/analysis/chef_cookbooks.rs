//! Chef cookbook dependency analysis from the knowledge graph.

use crate::error::{Error, Result};
use crate::graph::backend::GraphBackend;
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::{EdgeType, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

/// A cookbook node in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookbookNode {
    /// Cookbook name
    pub name: String,
    /// Version from metadata
    pub version: String,
    /// Filesystem path when discovered from disk
    pub path: String,
    /// Cookbook dependencies
    pub dependencies: Vec<String>,
    /// Cookbooks that depend on this one
    pub dependents: Vec<String>,
}

/// Cookbook dependency graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CookbookDependencyGraph {
    /// Cookbook name → metadata
    pub cookbooks: HashMap<String, CookbookNode>,
}

impl CookbookDependencyGraph {
    /// Build from indexed graph backend.
    pub fn from_graph(backend: &MemoryBackend) -> Result<Self> {
        let mut graph = Self::default();
        let cookbook_nodes: Vec<_> = backend
            .find_nodes_by_type(NodeType::ChefCookbook)?
            .into_iter()
            .filter(|n| n.get_property("referenced").is_none_or(|v| v != "true"))
            .collect();

        for node in &cookbook_nodes {
            let name = node
                .name
                .strip_prefix("cookbook::")
                .unwrap_or(&node.name)
                .to_string();
            graph
                .cookbooks
                .entry(name.clone())
                .or_insert_with(|| CookbookNode {
                    name,
                    version: node
                        .get_property("version")
                        .cloned()
                        .unwrap_or_else(|| "0.0.0".to_string()),
                    path: node
                        .file_path
                        .clone()
                        .or_else(|| node.get_property("cookbook_path").cloned())
                        .unwrap_or_default(),
                    dependencies: vec![],
                    dependents: vec![],
                });
        }

        let edges = backend.all_edges()?;
        for edge in edges {
            if edge.edge_type != EdgeType::DependsOnCookbook {
                continue;
            }
            let from = backend.get_node(edge.from)?.ok_or_else(|| {
                Error::GraphError(format!("Missing node {}", edge.from))
            })?;
            let to = backend.get_node(edge.to)?.ok_or_else(|| {
                Error::GraphError(format!("Missing node {}", edge.to))
            })?;
            let from_name = from
                .name
                .strip_prefix("cookbook::")
                .unwrap_or(&from.name)
                .to_string();
            let to_name = to
                .name
                .strip_prefix("cookbook::")
                .unwrap_or(&to.name)
                .to_string();
            graph
                .cookbooks
                .entry(from_name.clone())
                .or_insert_with(|| CookbookNode {
                    name: from_name.clone(),
                    version: from
                        .get_property("version")
                        .cloned()
                        .unwrap_or_else(|| "0.0.0".to_string()),
                    path: from.file_path.clone().unwrap_or_default(),
                    dependencies: vec![],
                    dependents: vec![],
                });
            graph
                .cookbooks
                .entry(to_name.clone())
                .or_insert_with(|| CookbookNode {
                    name: to_name.clone(),
                    version: to
                        .get_property("version")
                        .cloned()
                        .unwrap_or_else(|| "0.0.0".to_string()),
                    path: to.file_path.clone().unwrap_or_default(),
                    dependencies: vec![],
                    dependents: vec![],
                });
            let from_entry = graph.cookbooks.get_mut(&from_name).unwrap();
            if !from_entry.dependencies.contains(&to_name) {
                from_entry.dependencies.push(to_name.clone());
            }
            let to_entry = graph.cookbooks.get_mut(&to_name).unwrap();
            if !to_entry.dependents.contains(&from_name) {
                to_entry.dependents.push(from_name);
            }
        }

        Ok(graph)
    }

    /// Dependencies for a cookbook.
    pub fn get_dependencies(&self, name: &str) -> Option<Vec<String>> {
        self.cookbooks.get(name).map(|n| n.dependencies.clone())
    }

    /// Detect circular cookbook dependencies.
    pub fn validate_no_cycles(&self) -> Result<()> {
        for name in self.cookbooks.keys() {
            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            if self.has_cycle(name, &mut visited, &mut stack)? {
                return Err(Error::GraphError(format!(
                    "Circular Chef cookbook dependency involving: {name}"
                )));
            }
        }
        Ok(())
    }

    fn has_cycle(
        &self,
        name: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
    ) -> Result<bool> {
        if stack.contains(name) {
            return Ok(true);
        }
        if visited.contains(name) {
            return Ok(false);
        }
        visited.insert(name.to_string());
        stack.insert(name.to_string());
        if let Some(node) = self.cookbooks.get(name) {
            for dep in &node.dependencies {
                if self.has_cycle(dep, visited, stack)? {
                    return Ok(true);
                }
            }
        }
        stack.remove(name);
        Ok(false)
    }

    /// Topological sort (dependencies first).
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = self
            .cookbooks
            .iter()
            .map(|(name, node)| (name.clone(), node.dependencies.len()))
            .collect();

        let mut queue: VecDeque<String> = self
            .cookbooks
            .values()
            .filter(|n| n.dependencies.is_empty())
            .map(|n| n.name.clone())
            .collect();

        let mut result = Vec::new();
        while let Some(name) = queue.pop_front() {
            result.push(name.clone());
            for (cb_name, node) in &self.cookbooks {
                if node.dependencies.contains(&name) {
                    if let Some(deg) = in_degree.get_mut(cb_name) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(cb_name.clone());
                        }
                    }
                }
            }
        }

        if result.len() != self.cookbooks.len() {
            return Err(Error::GraphError(
                "Circular dependency detected in Chef cookbooks".into(),
            ));
        }
        Ok(result)
    }
}

/// Analyze cookbooks from a filesystem directory (CLI helper).
pub struct CookbookDependencyAnalyzer;

impl CookbookDependencyAnalyzer {
    /// Create analyzer.
    pub fn new() -> Self {
        Self
    }

    /// Scan `cookbooks/*/metadata.rb` on disk.
    pub fn analyze_cookbooks_dir(&self, cookbooks_path: &Path) -> Result<CookbookDependencyGraph> {
        use crate::languages::multimodal::chef::parser::ChefParser;

        let mut graph = CookbookDependencyGraph::default();
        if !cookbooks_path.exists() {
            return Ok(graph);
        }

        let parser = ChefParser::new();
        for entry in std::fs::read_dir(cookbooks_path)? {
            let entry = entry?;
            let cb_path = entry.path();
            if !cb_path.is_dir() {
                continue;
            }
            let cb_name = cb_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("cookbook")
                .to_string();
            let meta = cb_path.join("metadata.rb");
            let mut dependencies = Vec::new();
            let mut version = "0.0.0".to_string();
            if meta.exists() {
                let content = std::fs::read_to_string(&meta)?;
                let (symbols, relations) = parser.parse(&meta.to_string_lossy(), &content);
                for sym in symbols {
                    if sym.symbol_type == crate::languages::plugin_trait::SymbolType::ChefCookbook
                        && sym.name == format!("cookbook::{cb_name}")
                    {
                        if let Some(v) = sym.metadata.get("version").and_then(|v| v.as_str()) {
                            version = v.to_string();
                        }
                    }
                }
                for rel in relations {
                    if rel.relation_type
                        == crate::languages::plugin_trait::RelationType::DependsOnCookbook
                    {
                        let dep = rel
                            .to
                            .strip_prefix("cookbook::")
                            .unwrap_or(&rel.to)
                            .to_string();
                        dependencies.push(dep);
                    }
                }
            }
            graph.cookbooks.insert(
                cb_name.clone(),
                CookbookNode {
                    name: cb_name,
                    version,
                    path: cb_path.to_string_lossy().to_string(),
                    dependencies,
                    dependents: vec![],
                },
            );
        }

        let names: Vec<String> = graph.cookbooks.keys().cloned().collect();
        for name in names {
            let deps = graph.cookbooks.get(&name).unwrap().dependencies.clone();
            for dep in deps {
                graph.cookbooks.entry(dep.clone()).or_insert_with(|| CookbookNode {
                    name: dep.clone(),
                    version: "0.0.0".to_string(),
                    path: String::new(),
                    dependencies: vec![],
                    dependents: vec![],
                });
                if let Some(dep_node) = graph.cookbooks.get_mut(&dep) {
                    if !dep_node.dependents.contains(&name) {
                        dep_node.dependents.push(name.clone());
                    }
                }
            }
        }

        graph.validate_no_cycles()?;
        Ok(graph)
    }
}

impl Default for CookbookDependencyAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
