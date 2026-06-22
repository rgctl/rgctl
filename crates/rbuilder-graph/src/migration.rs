//! Graph schema migration (Phase 12.0).

use crate::schema::{Edge, GraphParameter, Node};
use rbuilder_error::Result;
use rbuilder_plugin_api::Parameter;

/// Migrate a graph snapshot to the current schema version.
pub fn migrate_snapshot(
    schema_version: u32,
    nodes: &mut [Node],
    edges: &mut [Edge],
) -> Result<u32> {
    let mut version = schema_version;
    if version < 2 {
        migrate_v1_to_v2(nodes, edges);
        version = 2;
    }
    let _ = edges;
    Ok(version)
}

/// Promote legacy property-bag fields to first-class node/edge metadata.
pub fn migrate_v1_to_v2(nodes: &mut [Node], edges: &mut [Edge]) {
    for node in nodes.iter_mut() {
        if node.signature.is_none() {
            if let Some(sig) = node.properties.get("signature").cloned() {
                node.signature = Some(sig);
            }
        }
        if node.return_type.is_none() {
            if let Some(ret) = node.properties.get("return_type").cloned() {
                node.return_type = Some(ret);
            }
        }
        if node.parameters.is_empty() {
            if let Some(raw) = node.properties.get("parameters") {
                if let Ok(params) = serde_json::from_str::<Vec<Parameter>>(raw) {
                    node.parameters = params
                        .into_iter()
                        .map(graph_parameter_from_plugin)
                        .collect();
                }
            }
        }
        if node.code_hash.is_none() {
            if let Some(hash) = node.properties.get("code_hash").cloned() {
                node.code_hash = Some(hash);
            }
        }
    }

    for edge in edges.iter_mut() {
        if edge.call_type.is_none() {
            if let Some(raw) = edge.properties.get("call_type") {
                edge.call_type = parse_call_type(raw);
            }
        }
        if edge.access_type.is_none() {
            if let Some(raw) = edge.properties.get("access_type") {
                edge.access_type = parse_access_type(raw);
            }
        }
    }
}

fn parse_call_type(raw: &str) -> Option<crate::schema::CallType> {
    use crate::schema::CallType;
    match raw {
        "Direct" | "direct" => Some(CallType::Direct),
        "Indirect" | "indirect" => Some(CallType::Indirect),
        "Virtual" | "virtual" => Some(CallType::Virtual),
        "Macro" | "macro" => Some(CallType::Macro),
        _ => None,
    }
}

fn parse_access_type(raw: &str) -> Option<crate::schema::AccessType> {
    use crate::schema::AccessType;
    match raw {
        "Read" | "read" => Some(AccessType::Read),
        "Write" | "write" => Some(AccessType::Write),
        "ReadWrite" | "readwrite" => Some(AccessType::ReadWrite),
        _ => None,
    }
}

/// Convert a language-plugin parameter into graph storage form.
pub fn graph_parameter_from_plugin(param: Parameter) -> GraphParameter {
    GraphParameter {
        name: param.name,
        param_type: param.param_type,
        default_value: param.default_value,
    }
}

/// Convert graph parameters back to plugin parameters.
pub fn plugin_parameter_from_graph(param: &GraphParameter) -> Parameter {
    Parameter {
        name: param.name.clone(),
        param_type: param.param_type.clone(),
        default_value: param.default_value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{CallType, EdgeType, NodeType, GRAPH_SCHEMA_VERSION};
    use uuid::Uuid;

    #[test]
    fn test_migrate_v1_signature_from_properties() {
        let mut nodes = vec![Node::new(NodeType::Function, "add".to_string())
            .with_property("signature".to_string(), "fn add(a: i32) -> i32".to_string())];
        migrate_v1_to_v2(&mut nodes, &mut []);
        assert_eq!(nodes[0].signature.as_deref(), Some("fn add(a: i32) -> i32"));
    }

    #[test]
    fn test_migrate_snapshot_sets_version() {
        let mut nodes = vec![Node::new(NodeType::Function, "main".to_string())];
        let mut edges = vec![];
        let version = migrate_snapshot(1, &mut nodes, &mut edges).unwrap();
        assert_eq!(version, GRAPH_SCHEMA_VERSION);
    }

    #[test]
    fn test_migrate_edge_call_type_from_property() {
        let mut edges = vec![Edge::new(Uuid::new_v4(), Uuid::new_v4(), EdgeType::Calls)
            .with_property("call_type".to_string(), "Virtual".to_string())];
        migrate_v1_to_v2(&mut [], &mut edges);
        assert_eq!(edges[0].call_type, Some(CallType::Virtual));
    }
}
