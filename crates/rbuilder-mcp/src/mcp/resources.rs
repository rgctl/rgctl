//! MCP resource providers

use rbuilder_analysis::community::CommunityDetector;
use rbuilder_analysis::complexity::ComplexityAnalyzer;
use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::NodeType;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// MCP resource descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDefinition {
    /// Resource URI
    pub uri: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// MIME type
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

/// Provides read-only MCP resources from the graph.
pub struct ResourceProvider;

impl ResourceProvider {
    /// List available resources.
    pub fn list_resources() -> Vec<ResourceDefinition> {
        vec![
            ResourceDefinition {
                uri: "rbuilder://graph/schema".into(),
                name: "Graph Schema".into(),
                description: "Overview of graph node types, edge types, and query DSL".into(),
                mime_type: "application/json".into(),
            },
            ResourceDefinition {
                uri: "rbuilder://graph/stats".into(),
                name: "Graph Statistics".into(),
                description: "Overall codebase statistics and metrics".into(),
                mime_type: "application/json".into(),
            },
            ResourceDefinition {
                uri: "rbuilder://communities".into(),
                name: "Architectural Communities".into(),
                description: "Detected architectural modules and their boundaries".into(),
                mime_type: "application/json".into(),
            },
        ]
    }

    /// Read a resource by URI.
    pub fn read(backend: &MemoryBackend, uri: &str) -> Result<Value> {
        match uri {
            "rbuilder://graph/schema" => Ok(graph_schema()),
            "rbuilder://graph/stats" => graph_stats(backend),
            "rbuilder://communities" => community_summary(backend),
            other => Err(rbuilder_error::Error::NotFound(format!(
                "Unknown resource: {other}"
            ))),
        }
    }
}

fn graph_schema() -> Value {
    json!({
        "node_types": [
            "Function", "Class", "Struct", "Enum", "Interface", "Module",
            "Variable", "File", "ConfigKey", "TypeAlias", "Macro", "Import"
        ],
        "edge_types": [
            "Calls", "Contains", "Uses", "Implements", "Extends",
            "References", "Instantiates", "Modifies", "UsesConfig", "DefinedIn"
        ],
        "query_dsl": [
            "type:Function", "name:symbol", "label:soa:service",
            "functions", "classes", "files", "config", "all"
        ],
    })
}

fn graph_stats(backend: &MemoryBackend) -> Result<Value> {
    let nodes = backend.all_nodes()?;
    let edges = backend.all_edges()?;

    let mut by_type = std::collections::HashMap::new();
    for node in &nodes {
        *by_type
            .entry(format!("{:?}", node.node_type))
            .or_insert(0usize) += 1;
    }

    let mut by_edge = std::collections::HashMap::new();
    for edge in &edges {
        *by_edge
            .entry(format!("{:?}", edge.edge_type))
            .or_insert(0usize) += 1;
    }

    let complexity = ComplexityAnalyzer::analyze(backend).ok();

    Ok(json!({
        "node_count": nodes.len(),
        "edge_count": edges.len(),
        "nodes_by_type": by_type,
        "edges_by_type": by_edge,
        "function_count": backend.find_nodes_by_type(NodeType::Function)?.len(),
        "avg_complexity": complexity.as_ref().map(|c| c.avg_cyclomatic),
        "max_complexity": complexity.as_ref().map(|c| c.max_cyclomatic),
    }))
}

fn community_summary(backend: &MemoryBackend) -> Result<Value> {
    let result = CommunityDetector::new().detect(backend)?;
    let communities: Vec<Value> = result
        .communities
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "member_count": c.members.len(),
            })
        })
        .collect();

    Ok(json!({
        "modularity": result.modularity,
        "community_count": communities.len(),
        "communities": communities,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::Node;

    #[test]
    fn test_graph_stats_resource() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(Node::new(NodeType::Function, "main".into()))
            .unwrap();
        let stats = graph_stats(&backend).unwrap();
        assert_eq!(stats["node_count"], 1);
    }
}
