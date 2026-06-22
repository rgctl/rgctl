//! Puppet module dependency analysis from the knowledge graph.
//!
//! # Example
//!
//! ```no_run
//! use rbuilder_lang_puppet::ModuleDependencyGraph;
//! use rbuilder_graph::CodeGraph;
//! use std::path::Path;
//!
//! # fn main() -> rbuilder_error::Result<()> {
//! let graph = CodeGraph::load_from_repo(Path::new("."))?;
//! let module_graph = ModuleDependencyGraph::from_graph(graph.backend())?;
//!
//! let sorted = module_graph.topological_sort()?;
//! println!("Module dependency order: {sorted:?}");
//! # Ok(())
//! # }
//! ```

use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::GraphBackend;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

/// A Puppet module node in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    /// Module name
    pub name: String,
    /// Version from metadata.json
    pub version: String,
    /// Filesystem path when discovered from disk
    pub path: String,
    /// Module dependencies
    pub dependencies: Vec<String>,
    /// Modules that depend on this one
    pub dependents: Vec<String>,
}

/// Puppet module dependency graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModuleDependencyGraph {
    /// Module name → metadata
    pub modules: HashMap<String, ModuleNode>,
}

impl ModuleDependencyGraph {
    /// Build from indexed graph backend.
    pub fn from_graph(backend: &MemoryBackend) -> Result<Self> {
        let mut graph = Self::default();
        let module_nodes: Vec<_> = backend
            .find_nodes_by_type(NodeType::PuppetModule)?
            .into_iter()
            .filter(|n| n.get_property("referenced").is_none_or(|v| v != "true"))
            .collect();

        for node in &module_nodes {
            let name = node
                .name
                .strip_prefix("module::")
                .unwrap_or(&node.name)
                .to_string();
            graph
                .modules
                .entry(name.clone())
                .or_insert_with(|| ModuleNode {
                    name,
                    version: node
                        .get_property("version")
                        .cloned()
                        .unwrap_or_else(|| "0.0.0".to_string()),
                    path: node
                        .file_path
                        .clone()
                        .or_else(|| node.get_property("module_path").cloned())
                        .unwrap_or_default(),
                    dependencies: vec![],
                    dependents: vec![],
                });
        }

        let edges = backend.all_edges()?;
        for edge in edges {
            if edge.edge_type != EdgeType::DependsOnModule {
                continue;
            }
            let from = backend
                .get_node(edge.from)?
                .ok_or_else(|| Error::GraphError(format!("Missing node {}", edge.from)))?;
            let to = backend
                .get_node(edge.to)?
                .ok_or_else(|| Error::GraphError(format!("Missing node {}", edge.to)))?;
            let from_name = from
                .name
                .strip_prefix("module::")
                .unwrap_or(&from.name)
                .to_string();
            let to_name = to
                .name
                .strip_prefix("module::")
                .unwrap_or(&to.name)
                .to_string();
            {
                let from_entry =
                    graph
                        .modules
                        .entry(from_name.clone())
                        .or_insert_with(|| ModuleNode {
                            name: from_name.clone(),
                            version: from
                                .get_property("version")
                                .cloned()
                                .unwrap_or_else(|| "0.0.0".to_string()),
                            path: from.file_path.clone().unwrap_or_default(),
                            dependencies: vec![],
                            dependents: vec![],
                        });
                if !from_entry.dependencies.contains(&to_name) {
                    from_entry.dependencies.push(to_name.clone());
                }
            }
            {
                let to_entry = graph
                    .modules
                    .entry(to_name.clone())
                    .or_insert_with(|| ModuleNode {
                        name: to_name.clone(),
                        version: to
                            .get_property("version")
                            .cloned()
                            .unwrap_or_else(|| "0.0.0".to_string()),
                        path: to.file_path.clone().unwrap_or_default(),
                        dependencies: vec![],
                        dependents: vec![],
                    });
                if !to_entry.dependents.contains(&from_name) {
                    to_entry.dependents.push(from_name);
                }
            }
        }

        Ok(graph)
    }

    /// Detect circular module dependencies.
    pub fn validate_no_cycles(&self) -> Result<()> {
        for name in self.modules.keys() {
            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            if self.has_cycle(name, &mut visited, &mut stack)? {
                return Err(Error::GraphError(format!(
                    "Circular Puppet module dependency involving: {name}"
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
        if let Some(node) = self.modules.get(name) {
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
            .modules
            .iter()
            .map(|(name, node)| (name.clone(), node.dependencies.len()))
            .collect();

        let mut queue: VecDeque<String> = self
            .modules
            .values()
            .filter(|n| n.dependencies.is_empty())
            .map(|n| n.name.clone())
            .collect();

        let mut result = Vec::new();
        while let Some(name) = queue.pop_front() {
            result.push(name.clone());
            for (mod_name, node) in &self.modules {
                if node.dependencies.contains(&name) {
                    if let Some(deg) = in_degree.get_mut(mod_name) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(mod_name.clone());
                        }
                    }
                }
            }
        }

        if result.len() != self.modules.len() {
            return Err(Error::GraphError(
                "Circular dependency detected in Puppet modules".into(),
            ));
        }
        Ok(result)
    }
}

/// Analyze modules from a filesystem directory (CLI helper).
pub struct ModuleDependencyAnalyzer;

impl Default for ModuleDependencyAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleDependencyAnalyzer {
    /// Create analyzer.
    pub fn new() -> Self {
        Self
    }

    /// Scan `modules/*/metadata.json` on disk.
    pub fn analyze_modules_dir(&self, modules_path: &Path) -> Result<ModuleDependencyGraph> {
        let mut graph = ModuleDependencyGraph::default();
        if !modules_path.exists() {
            return Ok(graph);
        }

        for entry in std::fs::read_dir(modules_path)? {
            let entry = entry?;
            let mod_path = entry.path();
            if !mod_path.is_dir() {
                continue;
            }
            let mod_name = mod_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("module")
                .to_string();
            let metadata_path = mod_path.join("metadata.json");
            if metadata_path.exists() {
                let content = std::fs::read_to_string(&metadata_path)?;
                let (symbols, relations) =
                    crate::parse_content(&metadata_path.to_string_lossy(), &content);
                for sym in symbols {
                    if sym.symbol_type == rbuilder_plugin_api::SymbolType::PuppetModule
                        && sym.metadata.get("referenced").is_none()
                    {
                        let version = sym
                            .metadata
                            .get("version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("0.0.0")
                            .to_string();
                        graph.modules.insert(
                            mod_name.clone(),
                            ModuleNode {
                                name: mod_name.clone(),
                                version,
                                path: mod_path.to_string_lossy().to_string(),
                                dependencies: vec![],
                                dependents: vec![],
                            },
                        );
                    }
                }
                for rel in relations {
                    if rel.relation_type == rbuilder_plugin_api::RelationType::DependsOnModule {
                        let dep = rel
                            .to
                            .strip_prefix("module::")
                            .unwrap_or(&rel.to)
                            .to_string();
                        if let Some(node) = graph.modules.get_mut(&mod_name) {
                            if !node.dependencies.contains(&dep) {
                                node.dependencies.push(dep);
                            }
                        }
                    }
                }
            }
        }

        for (name, node) in graph.modules.clone().iter() {
            for dep in &node.dependencies {
                if let Some(dep_node) = graph.modules.get_mut(dep) {
                    if !dep_node.dependents.contains(name) {
                        dep_node.dependents.push(name.clone());
                    }
                } else {
                    graph.modules.insert(
                        dep.clone(),
                        ModuleNode {
                            name: dep.clone(),
                            version: "0.0.0".into(),
                            path: String::new(),
                            dependencies: vec![],
                            dependents: vec![name.clone()],
                        },
                    );
                }
            }
        }

        Ok(graph)
    }
}
