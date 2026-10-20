//! Universal structural invariants for [`ColumnarGraphMmap`] snapshots.

use crate::columnar_snapshot::{
    append_node_columnar, edge_digest_bytes, ColumnarGraphMmap, StringPool,
};
use crate::csr::edge_type_to_u8;
use crate::schema::{Edge, EdgeType, Node, NodeType};
use crate::stable_key::{stable_key_from_node, stable_key_from_row};
use rgctl_error::{Error, Result};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Assert that a columnar snapshot satisfies INV-1…INV-7.
pub fn assert_graph_invariants(col: &ColumnarGraphMmap) -> Result<()> {
    check_inv1_sorted_unique_nodes(col)?;
    check_inv2_sorted_unique_edges(col)?;
    check_inv3_endpoint_legality(col)?;
    check_inv4_stable_key_consistency(col)?;
    check_inv5_index_bijection(col)?;
    check_inv6_content_digest(col)?;
    check_inv7_external_stubs(col)?;
    Ok(())
}

fn inv_err(msg: impl Into<String>) -> Error {
    Error::SerdeError(msg.into())
}

fn check_inv1_sorted_unique_nodes(col: &ColumnarGraphMmap) -> Result<()> {
    let mut prev: Option<Uuid> = None;
    for (idx, id) in col.node_ids_by_index() {
        if let Some(p) = prev {
            if id <= p {
                return Err(inv_err(format!(
                    "INV-1: node_rows not strictly sorted/unique at index {idx}: {id} <= {p}"
                )));
            }
        }
        prev = Some(id);
    }
    Ok(())
}

fn check_inv2_sorted_unique_edges(col: &ColumnarGraphMmap) -> Result<()> {
    let mut node_ids: HashSet<Uuid> = HashSet::with_capacity(col.node_count());
    for (_, id) in col.node_ids_by_index() {
        node_ids.insert(id);
    }

    let mut prev: Option<(Uuid, Uuid, u8)> = None;
    for idx in 0..col.edge_count() {
        let (from, to, edge_type) = col.edge_at(idx)?;
        let key = (from, to, edge_type_to_u8(edge_type));
        if !node_ids.contains(&from) {
            return Err(inv_err(format!(
                "INV-2: dangling edge from={from} at index {idx}"
            )));
        }
        if !node_ids.contains(&to) {
            return Err(inv_err(format!(
                "INV-2: dangling edge to={to} at index {idx}"
            )));
        }
        if let Some(p) = prev {
            if key <= p {
                return Err(inv_err(format!(
                    "INV-2: edge_rows not strictly sorted/unique at index {idx}: {:?} <= {:?}",
                    key, p
                )));
            }
        }
        prev = Some(key);
    }
    Ok(())
}

fn is_type_declaration(nt: NodeType) -> bool {
    matches!(
        nt,
        NodeType::Class
            | NodeType::Struct
            | NodeType::Enum
            | NodeType::Interface
            | NodeType::Annotation
            | NodeType::TypeAlias
            | NodeType::PuppetClass
            | NodeType::PuppetDefinedType
            | NodeType::PuppetNode
            | NodeType::ChefCustomResource
    )
}

/// Inheritance endpoints may resolve to `Import` / `Module` when the trait/type is external.
fn is_inheritance_endpoint(nt: NodeType) -> bool {
    is_type_declaration(nt)
        || matches!(
            nt,
            NodeType::Import | NodeType::Module | NodeType::Dependency
        )
}

fn is_container(nt: NodeType) -> bool {
    matches!(
        nt,
        NodeType::File
            | NodeType::Module
            | NodeType::Class
            | NodeType::Struct
            | NodeType::Enum
            | NodeType::Interface
            | NodeType::Annotation
            | NodeType::PuppetModule
            | NodeType::PuppetClass
            | NodeType::PuppetDefinedType
            | NodeType::PuppetNode
            | NodeType::KantraRuleset
            | NodeType::ChefCookbook
            | NodeType::AnsiblePlaybook
            | NodeType::AnsiblePlay
            | NodeType::AnsibleRole
    )
}

fn check_inv3_endpoint_legality(col: &ColumnarGraphMmap) -> Result<()> {
    for idx in 0..col.edge_count() {
        let (from, to, edge_type) = col.edge_at(idx)?;
        let from_idx = col.find_node_index(from).ok_or_else(|| {
            inv_err(format!("INV-3: missing from node {from}"))
        })?;
        let to_idx = col
            .find_node_index(to)
            .ok_or_else(|| inv_err(format!("INV-3: missing to node {to}")))?;
        let from_node = col.materialize_node_at(from_idx)?;
        let to_node = col.materialize_node_at(to_idx)?;
        match edge_type {
            EdgeType::Calls => {
                // Puppet/Chef callable containers may emit Calls edges from class-like nodes.
                let from_ok = matches!(
                    from_node.node_type,
                    NodeType::Function
                        | NodeType::PuppetClass
                        | NodeType::PuppetDefinedType
                        | NodeType::PuppetNode
                        | NodeType::ChefRecipe
                        | NodeType::ChefResource
                        | NodeType::AnsibleTask
                        | NodeType::AnsibleHandler
                );
                let to_ok = matches!(
                    to_node.node_type,
                    NodeType::Function
                        | NodeType::PuppetClass
                        | NodeType::PuppetDefinedType
                        | NodeType::PuppetNode
                        | NodeType::ChefRecipe
                        | NodeType::ChefResource
                        | NodeType::AnsibleTask
                        | NodeType::AnsibleHandler
                );
                if !from_ok || !to_ok {
                    return Err(inv_err(format!(
                        "INV-3: Calls requires callable endpoints, got {:?}→{:?} ({from}→{to})",
                        from_node.node_type, to_node.node_type
                    )));
                }
            }
            EdgeType::Extends | EdgeType::Implements | EdgeType::Permits => {
                if !is_inheritance_endpoint(from_node.node_type)
                    || !is_inheritance_endpoint(to_node.node_type)
                {
                    return Err(inv_err(format!(
                        "INV-3: {edge_type:?} requires type/import endpoints, got {:?}→{:?} ({from}→{to})",
                        from_node.node_type, to_node.node_type
                    )));
                }
            }
            EdgeType::Contains => {
                if !is_container(from_node.node_type) {
                    return Err(inv_err(format!(
                        "INV-3: Contains source must be a container, got {:?} ({from})",
                        from_node.node_type
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_inv4_stable_key_consistency(col: &ColumnarGraphMmap) -> Result<()> {
    for idx in 0..col.node_count() {
        let from_row = stable_key_from_row(col, idx)?;
        let node = col.materialize_node_at(idx)?;
        let from_node = stable_key_from_node(&node);
        if from_row != from_node {
            return Err(inv_err(format!(
                "INV-4: stable_key_from_row != stable_key_from_node at index {idx} (id={})",
                node.id
            )));
        }
    }
    Ok(())
}

fn check_inv5_index_bijection(col: &ColumnarGraphMmap) -> Result<()> {
    let mut expected_name: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut expected_type: HashMap<NodeType, Vec<Uuid>> = HashMap::new();
    for idx in 0..col.node_count() {
        let node = col.materialize_node_at(idx)?;
        expected_name
            .entry(node.name.to_string())
            .or_default()
            .push(node.id);
        expected_type.entry(node.node_type).or_default().push(node.id);
    }
    for ids in expected_name.values_mut() {
        ids.sort_unstable();
    }
    for ids in expected_type.values_mut() {
        ids.sort_unstable();
    }

    let indexes = col.indexes_shared()?;
    let mut actual_name = indexes.0.clone();
    let mut actual_type = indexes.1.clone();
    for ids in actual_name.values_mut() {
        ids.sort_unstable();
    }
    for ids in actual_type.values_mut() {
        ids.sort_unstable();
    }

    if actual_name != expected_name {
        return Err(inv_err(
            "INV-5: name_index does not match linear scan of node_rows",
        ));
    }
    if actual_type != expected_type {
        return Err(inv_err(
            "INV-5: type_index does not match linear scan of node_rows",
        ));
    }
    Ok(())
}

fn recompute_content_digest(nodes: &[Node], edges: &[Edge]) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut strings = StringPool::new();
    let mut node_rows = Vec::with_capacity(nodes.len());
    let mut extensions_blob = Vec::new();
    let mut name_index = HashMap::new();
    let mut type_index = HashMap::new();
    let mut scratch = Vec::with_capacity(512);
    for node in nodes {
        append_node_columnar(
            node,
            &mut hasher,
            &mut strings,
            &mut extensions_blob,
            &mut name_index,
            &mut type_index,
            &mut node_rows,
            &mut scratch,
        )?;
    }
    for edge in edges {
        hasher.update(&edge_digest_bytes(edge)?);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn check_inv6_content_digest(col: &ColumnarGraphMmap) -> Result<()> {
    let mut nodes = Vec::with_capacity(col.node_count());
    for idx in 0..col.node_count() {
        nodes.push(col.materialize_node_at(idx)?);
    }
    // Writer digests in id order; rematerialized rows are already sorted (INV-1).
    let mut edges = Vec::with_capacity(col.edge_count());
    for idx in 0..col.edge_count() {
        let (from, to, edge_type) = col.edge_at(idx)?;
        edges.push(Edge::new(from, to, edge_type));
    }
    let recomputed = recompute_content_digest(&nodes, &edges)?;
    if recomputed != col.content_digest() {
        return Err(inv_err(format!(
            "INV-6: content_digest mismatch: header={} recomputed={}",
            col.content_digest(),
            recomputed
        )));
    }
    Ok(())
}

fn is_external_stub(node: &Node) -> bool {
    let path_external = node.file_path.as_deref() == Some("<external>");
    let prop_external = node
        .properties
        .get("is_external_stub")
        .is_some_and(|v| v == "true");
    path_external || prop_external
}

fn check_inv7_external_stubs(col: &ColumnarGraphMmap) -> Result<()> {
    let mut stubs: Vec<(String, NodeType, Uuid)> = Vec::new();
    let mut in_repo: HashSet<(String, NodeType)> = HashSet::new();

    for idx in 0..col.node_count() {
        let node = col.materialize_node_at(idx)?;
        if is_external_stub(&node) {
            if node.file_path.as_deref() != Some("<external>") {
                return Err(inv_err(format!(
                    "INV-7: external stub {} missing file_path=<external>",
                    node.id
                )));
            }
            // Target type legality vs incoming edges is covered by INV-3; stubs may be
            // Function / type / Import / Dependency / File depending on the relation.
            stubs.push((node.name.to_string(), node.node_type, node.id));
        } else {
            in_repo.insert((node.name.to_string(), node.node_type));
        }
    }

    for (name, nt, id) in &stubs {
        // Dependency/Import stubs commonly coexist with in-repo package nodes of the
        // same simple name; shadowing is only a correctness hazard for call/type targets.
        if matches!(
            nt,
            NodeType::Dependency | NodeType::Import | NodeType::File | NodeType::Module
        ) {
            continue;
        }
        if in_repo.contains(&(name.clone(), *nt)) {
            return Err(inv_err(format!(
                "INV-7: external stub {id} shadows in-repo symbol name={name} type={nt:?}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Edge, EdgeType, Node, NodeType};
    use crate::write_columnar_from_nodes_edges;
    use memmap2::Mmap;
    use std::fs::File;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    fn open_col(path: &Path) -> ColumnarGraphMmap {
        let file = File::open(path).unwrap();
        let mmap = unsafe { Mmap::map(&file).unwrap() };
        ColumnarGraphMmap::open(Arc::new(mmap)).unwrap()
    }

    fn good_graph(path: &Path) -> String {
        let a = Node::new(NodeType::Function, "a")
            .with_file_path("a.rs")
            .with_qualified_name("a")
            .with_location(1, 2);
        let b = Node::new(NodeType::Function, "b")
            .with_file_path("b.rs")
            .with_qualified_name("b")
            .with_location(1, 2);
        let file = Node::new(NodeType::File, "a.rs").with_file_path("a.rs");
        let a_id = a.id;
        let b_id = b.id;
        let file_id = file.id;
        write_columnar_from_nodes_edges(
            vec![a, b, file],
            vec![
                Edge::new(a_id, b_id, EdgeType::Calls),
                Edge::new(file_id, a_id, EdgeType::Contains),
            ],
            path,
        )
        .unwrap()
    }

    #[test]
    fn good_graph_passes_all_invariants() {
        let tmp = NamedTempFile::new().unwrap();
        good_graph(tmp.path());
        let col = open_col(tmp.path());
        assert_graph_invariants(&col).expect("good graph must pass");
    }

    #[test]
    fn rejects_calls_to_class() {
        let tmp = NamedTempFile::new().unwrap();
        let f = Node::new(NodeType::Function, "f").with_file_path("f.rs");
        let c = Node::new(NodeType::Class, "C").with_file_path("c.rs");
        let f_id = f.id;
        let c_id = c.id;
        write_columnar_from_nodes_edges(
            vec![f, c],
            vec![Edge::new(f_id, c_id, EdgeType::Calls)],
            tmp.path(),
        )
        .unwrap();
        let col = open_col(tmp.path());
        let err = assert_graph_invariants(&col).unwrap_err().to_string();
        assert!(err.contains("INV-3"), "{err}");
    }

    #[test]
    fn stable_key_row_matches_node() {
        let tmp = NamedTempFile::new().unwrap();
        good_graph(tmp.path());
        let col = open_col(tmp.path());
        for idx in 0..col.node_count() {
            let row = stable_key_from_row(&col, idx).unwrap();
            let node = col.materialize_node_at(idx).unwrap();
            assert_eq!(row, stable_key_from_node(&node));
        }
    }

    #[test]
    fn rejects_duplicate_node_ids() {
        let tmp = NamedTempFile::new().unwrap();
        let mut a = Node::new(NodeType::Function, "a").with_file_path("a.rs");
        let mut b = Node::new(NodeType::Function, "b").with_file_path("b.rs");
        let shared = Uuid::new_v4();
        a.id = shared;
        b.id = shared;
        write_columnar_from_nodes_edges(vec![a, b], vec![], tmp.path()).unwrap();
        let col = open_col(tmp.path());
        let err = assert_graph_invariants(&col).unwrap_err().to_string();
        assert!(err.contains("INV-1"), "{err}");
    }

    #[test]
    fn rejects_external_stub_shadowing() {
        let tmp = NamedTempFile::new().unwrap();
        let local = Node::new(NodeType::Function, "printf")
            .with_file_path("main.c")
            .with_qualified_name("main::printf");
        let mut stub = Node::new(NodeType::Function, "printf").with_file_path("<external>");
        stub.properties
            .insert("is_external_stub".into(), "true".into());
        write_columnar_from_nodes_edges(vec![local, stub], vec![], tmp.path()).unwrap();
        let col = open_col(tmp.path());
        let err = assert_graph_invariants(&col).unwrap_err().to_string();
        assert!(err.contains("INV-7"), "{err}");
    }

    #[test]
    fn content_digest_reproducible() {
        let tmp = NamedTempFile::new().unwrap();
        let digest = good_graph(tmp.path());
        let col = open_col(tmp.path());
        assert_eq!(col.content_digest(), digest);
        assert_graph_invariants(&col).unwrap();
    }
}
