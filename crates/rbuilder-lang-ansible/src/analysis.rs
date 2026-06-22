//! Ansible role dependency analysis from the knowledge graph.
//!
//! # Example
//!
//! ```no_run
//! use rbuilder_lang_ansible::RoleDependencyGraph;
//! use rbuilder_graph::CodeGraph;
//! use std::path::Path;
//!
//! # fn main() -> rbuilder_error::Result<()> {
//! let graph = CodeGraph::load_from_repo(Path::new("."))?;
//! let role_graph = RoleDependencyGraph::from_graph(graph.backend())?;
//!
//! let sorted = role_graph.topological_sort()?;
//! println!("Role execution order: {sorted:?}");
//!
//! role_graph.validate_no_cycles()?;
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

/// A role node in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleNode {
    /// Role name
    pub name: String,
    /// Filesystem path when discovered from disk
    pub path: String,
    /// Roles this role depends on (meta dependencies)
    pub dependencies: Vec<String>,
    /// Roles that depend on this role
    pub dependents: Vec<String>,
}

/// Role dependency graph built from graph nodes or filesystem scan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoleDependencyGraph {
    /// Role name → metadata
    pub roles: HashMap<String, RoleNode>,
}

impl RoleDependencyGraph {
    /// Build from indexed graph backend (preferred integration path).
    pub fn from_graph(backend: &MemoryBackend) -> Result<Self> {
        let mut graph = Self::default();
        let role_nodes: Vec<_> = backend
            .find_nodes_by_type(NodeType::AnsibleRole)?
            .into_iter()
            .filter(|n| n.get_property("referenced").is_none_or(|v| v != "true"))
            .collect();

        for node in &role_nodes {
            graph
                .roles
                .entry(node.name.clone())
                .or_insert_with(|| RoleNode {
                    name: node.name.clone(),
                    path: node
                        .file_path
                        .clone()
                        .or_else(|| node.get_property("role_path").cloned())
                        .unwrap_or_default(),
                    dependencies: vec![],
                    dependents: vec![],
                });
        }

        let edges = backend.all_edges()?;
        for edge in edges {
            if edge.edge_type != EdgeType::DependsOnRole {
                continue;
            }
            let from = backend
                .get_node(edge.from)?
                .ok_or_else(|| Error::GraphError(format!("Missing node {}", edge.from)))?;
            let to = backend
                .get_node(edge.to)?
                .ok_or_else(|| Error::GraphError(format!("Missing node {}", edge.to)))?;
            if from.node_type != NodeType::AnsibleRole || to.node_type != NodeType::AnsibleRole {
                continue;
            }
            {
                let from_entry = graph
                    .roles
                    .entry(from.name.clone())
                    .or_insert_with(|| RoleNode {
                        name: from.name.clone(),
                        path: from.file_path.clone().unwrap_or_default(),
                        dependencies: vec![],
                        dependents: vec![],
                    });
                if !from_entry.dependencies.contains(&to.name) {
                    from_entry.dependencies.push(to.name.clone());
                }
            }
            {
                let to_entry = graph
                    .roles
                    .entry(to.name.clone())
                    .or_insert_with(|| RoleNode {
                        name: to.name.clone(),
                        path: to.file_path.clone().unwrap_or_default(),
                        dependencies: vec![],
                        dependents: vec![],
                    });
                if !to_entry.dependents.contains(&from.name) {
                    to_entry.dependents.push(from.name.clone());
                }
            }
        }

        Ok(graph)
    }

    /// Dependencies for a single role.
    pub fn get_dependencies(&self, role_name: &str) -> Option<Vec<String>> {
        self.roles.get(role_name).map(|n| n.dependencies.clone())
    }

    /// Transitive dependencies in depth-first order.
    pub fn transitive_dependencies(&self, role_name: &str) -> Result<Vec<String>> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.collect_deps(role_name, &mut visited, &mut result)?;
        Ok(result)
    }

    fn collect_deps(
        &self,
        role_name: &str,
        visited: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) -> Result<()> {
        if visited.contains(role_name) {
            return Ok(());
        }
        visited.insert(role_name.to_string());
        if let Some(node) = self.roles.get(role_name) {
            for dep in &node.dependencies {
                result.push(dep.clone());
                self.collect_deps(dep, visited, result)?;
            }
        }
        Ok(())
    }

    /// Detect circular role dependencies.
    pub fn validate_no_cycles(&self) -> Result<()> {
        for role_name in self.roles.keys() {
            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            if self.has_cycle(role_name, &mut visited, &mut stack)? {
                return Err(Error::GraphError(format!(
                    "Circular Ansible role dependency involving: {role_name}"
                )));
            }
        }
        Ok(())
    }

    fn has_cycle(
        &self,
        role_name: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
    ) -> Result<bool> {
        if stack.contains(role_name) {
            return Ok(true);
        }
        if visited.contains(role_name) {
            return Ok(false);
        }
        visited.insert(role_name.to_string());
        stack.insert(role_name.to_string());
        if let Some(node) = self.roles.get(role_name) {
            for dep in &node.dependencies {
                if self.has_cycle(dep, visited, stack)? {
                    return Ok(true);
                }
            }
        }
        stack.remove(role_name);
        Ok(false)
    }

    /// Topological sort of roles (dependencies first).
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = self
            .roles
            .iter()
            .map(|(name, node)| (name.clone(), node.dependencies.len()))
            .collect();

        let mut queue: VecDeque<String> = self
            .roles
            .values()
            .filter(|n| n.dependencies.is_empty())
            .map(|n| n.name.clone())
            .collect();

        let mut result = Vec::new();
        while let Some(role) = queue.pop_front() {
            result.push(role.clone());
            for (name, node) in &self.roles {
                if node.dependencies.contains(&role) {
                    if let Some(deg) = in_degree.get_mut(name) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(name.clone());
                        }
                    }
                }
            }
        }

        if result.len() != self.roles.len() {
            return Err(Error::GraphError(
                "Circular dependency detected in Ansible roles".into(),
            ));
        }
        Ok(result)
    }
}

/// Analyze roles from a filesystem `roles/` directory (CLI helper).
pub struct RoleDependencyAnalyzer;

impl RoleDependencyAnalyzer {
    /// Create analyzer.
    pub fn new() -> Self {
        Self
    }

    /// Scan `roles/*/meta/main.yml` on disk.
    pub fn analyze_roles_dir(&self, roles_path: &Path) -> Result<RoleDependencyGraph> {
        let mut graph = RoleDependencyGraph::default();
        if !roles_path.exists() {
            return Ok(graph);
        }

        for entry in std::fs::read_dir(roles_path)? {
            let entry = entry?;
            let role_path = entry.path();
            if !role_path.is_dir() {
                continue;
            }
            let role_name = role_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("role")
                .to_string();
            let meta = role_path.join("meta/main.yml");
            let mut dependencies = Vec::new();
            if meta.exists() {
                let content = std::fs::read_to_string(&meta)?;
                dependencies =
                    crate::role_dependencies_from_meta(&meta.to_string_lossy(), &content);
            }
            graph.roles.insert(
                role_name.clone(),
                RoleNode {
                    name: role_name,
                    path: role_path.to_string_lossy().to_string(),
                    dependencies,
                    dependents: vec![],
                },
            );
        }

        let names: Vec<String> = graph.roles.keys().cloned().collect();
        for name in names {
            let deps = graph.roles.get(&name).unwrap().dependencies.clone();
            for dep in deps {
                if let Some(dep_node) = graph.roles.get_mut(&dep) {
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

impl Default for RoleDependencyAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::backend::MemoryBackend;
    use rbuilder_graph::schema::Node;

    #[test]
    fn test_topological_sort_from_graph() {
        let mut backend = MemoryBackend::new();
        let nginx = Node::new(NodeType::AnsibleRole, "nginx".into());
        let common = Node::new(NodeType::AnsibleRole, "common".into());
        let nginx_id = nginx.id;
        let common_id = common.id;
        backend.insert_node(nginx).unwrap();
        backend.insert_node(common).unwrap();
        backend
            .insert_edge(rbuilder_graph::schema::Edge::new(
                nginx_id,
                common_id,
                EdgeType::DependsOnRole,
            ))
            .unwrap();

        let graph = RoleDependencyGraph::from_graph(&backend).unwrap();
        let sorted = graph.topological_sort().unwrap();
        assert_eq!(sorted.len(), 2);
        assert!(sorted.contains(&"nginx".to_string()));
        assert!(sorted.contains(&"common".to_string()));
    }
}
