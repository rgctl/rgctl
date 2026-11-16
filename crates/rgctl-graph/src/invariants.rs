//! Universal structural invariants for [`ColumnarGraphMmap`] snapshots.

use crate::columnar_snapshot::{
    ColumnarGraphMmap, StringPool, append_node_columnar, edge_digest_bytes,
};
use crate::csr::edge_type_to_u8;
use crate::schema::{Edge, EdgeType, Node, NodeType};
use crate::stable_key::{stable_key_from_node, stable_key_from_row};
use rgctl_error::{Error, Result};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Assert that a columnar snapshot satisfies INV-1…INV-9.
pub fn assert_graph_invariants(col: &ColumnarGraphMmap) -> Result<()> {
    check_inv1_sorted_unique_nodes(col)?;
    check_inv2_sorted_unique_edges(col)?;
    check_inv3_endpoint_legality(col)?;
    check_inv4_stable_key_consistency(col)?;
    check_inv5_index_bijection(col)?;
    check_inv6_content_digest(col)?;
    check_inv7_external_stubs(col)?;
    let (nodes, by_id) = load_materialized_nodes(col)?;
    check_inv8_spans(&nodes)?;
    check_inv8_containment(col, &nodes, &by_id)?;
    check_inv9_sibling_spans(col, &nodes, &by_id)?;
    Ok(())
}

fn inv_err(msg: impl Into<String>) -> Error {
    Error::SerdeError(msg.into())
}

fn check_inv1_sorted_unique_nodes(col: &ColumnarGraphMmap) -> Result<()> {
    let mut prev: Option<Uuid> = None;
    for (idx, id) in col.node_ids_by_index() {
        if let Some(p) = prev
            && id <= p
        {
            return Err(inv_err(format!(
                "INV-1: node_rows not strictly sorted/unique at index {idx}: {id} <= {p}"
            )));
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
        if let Some(p) = prev
            && key <= p
        {
            return Err(inv_err(format!(
                "INV-2: edge_rows not strictly sorted/unique at index {idx}: {:?} <= {:?}",
                key, p
            )));
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
            | NodeType::ChefCookbook
            | NodeType::AnsiblePlaybook
            | NodeType::AnsiblePlay
            | NodeType::AnsibleRole
    )
}

fn check_inv3_endpoint_legality(col: &ColumnarGraphMmap) -> Result<()> {
    for idx in 0..col.edge_count() {
        let (from, to, edge_type) = col.edge_at(idx)?;
        let from_idx = col
            .find_node_index(from)
            .ok_or_else(|| inv_err(format!("INV-3: missing from node {from}")))?;
        let to_idx = col
            .find_node_index(to)
            .ok_or_else(|| inv_err(format!("INV-3: missing to node {to}")))?;
        let from_type = col.node_type_at(from_idx)?;
        let to_type = col.node_type_at(to_idx)?;
        match edge_type {
            EdgeType::Calls => {
                // Puppet/Chef callable containers may emit Calls edges from class-like nodes.
                let from_ok = matches!(
                    from_type,
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
                    to_type,
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
                        "INV-3: Calls requires callable endpoints, got {from_type:?}→{to_type:?} ({from}→{to})"
                    )));
                }
            }
            EdgeType::Extends | EdgeType::Implements | EdgeType::Permits => {
                if !is_inheritance_endpoint(from_type) || !is_inheritance_endpoint(to_type) {
                    return Err(inv_err(format!(
                        "INV-3: {edge_type:?} requires type/import endpoints, got {from_type:?}→{to_type:?} ({from}→{to})"
                    )));
                }
            }
            EdgeType::Contains if !is_container(from_type) => {
                return Err(inv_err(format!(
                    "INV-3: Contains source must be a container, got {from_type:?} ({from})"
                )));
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
        let id = Uuid::from_bytes(col.node_row_at(idx)?.id);
        let name = col.node_name_str(idx)?;
        let node_type = col.node_type_at(idx)?;
        expected_name.entry(name.to_string()).or_default().push(id);
        expected_type.entry(node_type).or_default().push(id);
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

fn load_materialized_nodes(col: &ColumnarGraphMmap) -> Result<(Vec<Node>, HashMap<Uuid, usize>)> {
    let mut nodes = Vec::with_capacity(col.node_count());
    let mut by_id = HashMap::with_capacity(col.node_count());
    for idx in 0..col.node_count() {
        let node = col.materialize_node_at(idx)?;
        by_id.insert(node.id, idx);
        nodes.push(node);
    }
    Ok((nodes, by_id))
}

/// Populated declaration span. `end_byte == 0` means "no span" (not a zero-length token at byte 0).
fn decl_span(node: &Node) -> Option<(u32, u32)> {
    let start = node.start_byte?;
    let end = node.end_byte?;
    if end == 0 {
        return None;
    }
    Some((start, end))
}

fn node_label(node: &Node) -> String {
    format!("{:?} {}", node.node_type, node.name)
}

fn property_is_true(node: &Node, key: &str) -> bool {
    node.get_property(key) == Some("true")
}

/// Synthetic / macro / primary-header members are exempt from span-equality checks.
fn is_span_synthetic(node: &Node) -> bool {
    const TRUE_KEYS: &[&str] = &[
        "lombok_synthesized",
        "is_synthetic",
        "primary_header_synthesized",
        "synthesized_accessor",
        "macro_generated",
        "macro_binding",
        "multi_binding",
    ];
    if TRUE_KEYS.iter().any(|key| property_is_true(node, key)) {
        return true;
    }
    matches!(
        node.get_property("kind"),
        Some(
            "primary_constructor"
                | "synthesized_accessor"
                | "property_accessor"
                | "macro_binding"
                | "multi_binding"
        )
    )
}

fn is_field_variable(node: &Node) -> bool {
    node.node_type == NodeType::Variable
        && (node.has_label("field")
            || node.get_property("kind") == Some("field")
            || node.get_property("member_of").is_some())
}

fn is_equality_container(nt: NodeType) -> bool {
    matches!(
        nt,
        NodeType::File
            | NodeType::Class
            | NodeType::Interface
            | NodeType::Struct
            | NodeType::Enum
            | NodeType::Module
    )
}

fn is_type_or_module_container(nt: NodeType) -> bool {
    matches!(
        nt,
        NodeType::Class
            | NodeType::Interface
            | NodeType::Struct
            | NodeType::Enum
            | NodeType::Module
    )
}

fn is_enclosing_owner(nt: NodeType) -> bool {
    nt != NodeType::File && is_container(nt)
}

fn same_source_file(a: &Node, b: &Node) -> bool {
    match (a.file_path.as_deref(), b.file_path.as_deref()) {
        (Some(left), Some(right)) => left == right,
        _ => true,
    }
}

/// File byte length when the `File` row records it (`file_byte_len` or a `[0, end)` span).
fn recorded_file_byte_len(node: &Node) -> Option<u32> {
    if node.node_type != NodeType::File {
        return None;
    }
    if let Some(raw) = node.get_property("file_byte_len")
        && let Ok(len) = raw.parse::<u32>()
    {
        return Some(len);
    }
    match (node.start_byte, node.end_byte) {
        (Some(0), Some(end)) if end > 0 => Some(end),
        _ => None,
    }
}

fn type_containment_issue(member: (u32, u32), container: (u32, u32)) -> Option<&'static str> {
    let (ms, me) = member;
    let (cs, ce) = container;
    // Member encloses the container (includes equal spans).
    if ms <= cs && ce <= me {
        return Some("reverse containment");
    }
    // Starts before the container and crosses its start boundary.
    if ms < cs && cs < me {
        return Some("boundary straddle");
    }
    // Starts inside the container and ends past its end.
    if cs <= ms && ms < ce && me > ce {
        return Some("boundary straddle");
    }
    None
}

fn inv8_pair(
    kind: &str,
    container: &Node,
    container_span: (u32, u32),
    member: &Node,
    member_span: (u32, u32),
) -> Error {
    inv_err(format!(
        "INV-8: {kind}: container {} {}..{} member {} {}..{}",
        node_label(container),
        container_span.0,
        container_span.1,
        node_label(member),
        member_span.0,
        member_span.1
    ))
}

/// INV-8: declaration order, file bounds, and identifier span inside the declaration.
fn check_inv8_spans(nodes: &[Node]) -> Result<()> {
    let mut file_lens: HashMap<&str, u32> = HashMap::new();
    for node in nodes {
        if let Some(len) = recorded_file_byte_len(node) {
            let key = node.file_path.as_deref().unwrap_or(node.name.as_ref());
            file_lens.insert(key, len);
        }
    }

    for node in nodes {
        if is_external_stub(node) {
            continue;
        }
        let Some((start, end)) = decl_span(node) else {
            continue;
        };
        if start > end
            || (node.node_type != NodeType::File && !is_span_synthetic(node) && start == end)
        {
            return Err(inv_err(format!(
                "INV-8: invalid or empty byte span: {} {start}..{end}",
                node_label(node)
            )));
        }
        if start == 0
            && node.node_type != NodeType::File
            && !is_span_synthetic(node)
            && node.start_line.unwrap_or(1) > 1
        {
            return Err(inv_err(format!(
                "INV-8: symbol on line {} cannot start at byte 0: {} {start}..{end}",
                node.start_line.unwrap_or(1),
                node_label(node)
            )));
        }
        if let Some(path) = node.file_path.as_deref()
            && let Some(len) = file_lens.get(path)
            && end > *len
        {
            return Err(inv_err(format!(
                "INV-8: span exceeds file byte length: {} {start}..{end} file {path} len {len}",
                node_label(node)
            )));
        }
        if node.name_rel_offset.is_some() {
            check_name_inside_decl(node, start, end)?;
        }
    }
    Ok(())
}

fn check_name_inside_decl(node: &Node, start: u32, end: u32) -> Result<()> {
    let Some((name_start, name_end)) = node.name_byte_span() else {
        return Err(inv_err(format!(
            "INV-8: identifier span could not be reconstructed: {}",
            node_label(node)
        )));
    };
    if name_start < start || name_end > end {
        return Err(inv_err(format!(
            "INV-8: identifier span outside declaration: {} name {name_start}..{name_end} decl {start}..{end}",
            node_label(node)
        )));
    }
    Ok(())
}

/// INV-8: proper container inequality and out-of-line-safe type containment.
fn check_inv8_containment(
    col: &ColumnarGraphMmap,
    nodes: &[Node],
    by_id: &HashMap<Uuid, usize>,
) -> Result<()> {
    let mut file_children: HashMap<Uuid, Vec<usize>> = HashMap::new();
    for idx in 0..col.edge_count() {
        let (from, to, edge_type) = col.edge_at(idx)?;
        if edge_type != EdgeType::Contains {
            continue;
        }
        if let (Some(&c_idx), Some(&m_idx)) = (by_id.get(&from), by_id.get(&to))
            && nodes[c_idx].node_type == NodeType::File
            && !is_external_stub(&nodes[m_idx])
        {
            file_children.entry(from).or_default().push(m_idx);
        }
    }

    for idx in 0..col.edge_count() {
        let (from, to, edge_type) = col.edge_at(idx)?;
        if edge_type != EdgeType::Contains {
            continue;
        }
        let Some(&container_idx) = by_id.get(&from) else {
            continue;
        };
        let Some(&member_idx) = by_id.get(&to) else {
            continue;
        };
        let container = &nodes[container_idx];
        let member = &nodes[member_idx];
        if is_external_stub(member)
            || is_span_synthetic(member)
            || !same_source_file(container, member)
        {
            continue;
        }
        let Some(member_span) = decl_span(member) else {
            continue;
        };
        let Some(container_span) = decl_span(container) else {
            continue;
        };
        let equality_member = member.node_type == NodeType::Function || is_field_variable(member);
        if equality_member
            && is_equality_container(container.node_type)
            && member_span == container_span
        {
            // A single top-level function on line 1 in a snippet with no trailing newline
            // (`"fn f() {}"`) genuinely spans `0..file_len`. Flag `File` span equality when
            // `file_byte_len` was explicitly set, when `member` starts past line 1, or when
            // any sibling symbol in the file is not nested inside `member` (e.g. a sibling
            // function/import/class or enclosing type).
            let allow_sole_top_level = container.node_type == NodeType::File
                && container.get_property("file_byte_len").is_none()
                && member.start_line == Some(1)
                && member.name_rel_offset.is_some()
                && !member
                    .file_path
                    .as_deref()
                    .is_some_and(|p| p.ends_with(".go"))
                && file_children.get(&from).is_none_or(|kids| {
                    kids.iter().all(|&k_idx| {
                        k_idx == member_idx
                            || (nodes[k_idx].node_type != NodeType::Class
                                && nodes[k_idx].node_type != NodeType::Interface
                                && nodes[k_idx].node_type != NodeType::Struct
                                && nodes[k_idx].node_type != NodeType::Enum
                                && decl_span(&nodes[k_idx]).is_none_or(|(ks, ke)| {
                                    member_span.0 <= ks
                                        && ke <= member_span.1
                                        && (ks, ke) != member_span
                                }))
                    })
                });
            if !allow_sole_top_level {
                return Err(inv8_pair(
                    "member span equals container span",
                    container,
                    container_span,
                    member,
                    member_span,
                ));
            }
        }
        if is_type_or_module_container(container.node_type)
            && let Some(kind) = type_containment_issue(member_span, container_span)
        {
            return Err(inv8_pair(
                kind,
                container,
                container_span,
                member,
                member_span,
            ));
        }
    }
    Ok(())
}

fn owner_scope_key(parents: Option<&[Uuid]>) -> String {
    let Some(parents) = parents else {
        return String::new();
    };
    let mut ids = parents.to_vec();
    ids.sort_unstable();
    ids.dedup();
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// INV-9: source-written sibling functions in one owner scope cannot share a span.
fn check_inv9_sibling_spans(
    col: &ColumnarGraphMmap,
    nodes: &[Node],
    by_id: &HashMap<Uuid, usize>,
) -> Result<()> {
    let mut owners: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for idx in 0..col.edge_count() {
        let (from, to, edge_type) = col.edge_at(idx)?;
        if edge_type != EdgeType::Contains {
            continue;
        }
        let Some(&parent_idx) = by_id.get(&from) else {
            continue;
        };
        if is_enclosing_owner(nodes[parent_idx].node_type) {
            owners.entry(to).or_default().push(from);
        }
    }

    let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        if node.node_type != NodeType::Function || is_external_stub(node) || is_span_synthetic(node)
        {
            continue;
        }
        let Some(span) = decl_span(node) else {
            continue;
        };
        if span.1 <= span.0 {
            continue;
        }
        let file = node.file_path.as_deref().unwrap_or("").to_string();
        let owner = owner_scope_key(owners.get(&node.id).map(Vec::as_slice));
        groups.entry((file, owner)).or_default().push(idx);
    }

    let mut seen: HashMap<(u32, u32), usize> = HashMap::new();
    for members in groups.values() {
        seen.clear();
        for &idx in members {
            let Some(span) = decl_span(&nodes[idx]) else {
                continue;
            };
            if let Some(&prev_idx) = seen.get(&span) {
                let prev = &nodes[prev_idx];
                let current = &nodes[idx];
                if stable_key_from_node(prev) != stable_key_from_node(current) {
                    return Err(inv_err(format!(
                        "INV-9: sibling functions share span {}..{}: {} and {} (file={})",
                        span.0,
                        span.1,
                        node_label(prev),
                        node_label(current),
                        current.file_path.as_deref().unwrap_or("")
                    )));
                }
            } else {
                seen.insert(span, idx);
            }
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
    fn write_columnar_dedupes_duplicate_node_ids() {
        // Writer collapses duplicate ids before assemble (INV-1 is enforced on the
        // on-disk rows). A hand-fed corrupt mmap would still fail INV-1.
        let tmp = NamedTempFile::new().unwrap();
        let mut a = Node::new(NodeType::Function, "a").with_file_path("a.rs");
        let mut b = Node::new(NodeType::Function, "b").with_file_path("b.rs");
        let shared = Uuid::new_v4();
        a.id = shared;
        b.id = shared;
        write_columnar_from_nodes_edges(vec![a, b], vec![], tmp.path()).unwrap();
        let col = open_col(tmp.path());
        assert_eq!(
            col.node_count(),
            1,
            "writer must collapse duplicate node ids"
        );
        assert_graph_invariants(&col).expect("deduped snapshot must pass INV-1");
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

    fn spanned(
        node_type: NodeType,
        file: &str,
        name: &str,
        qn: &str,
        start: u32,
        end: u32,
    ) -> Node {
        Node::new(node_type, name)
            .with_file_path(file)
            .with_qualified_name(qn)
            .with_byte_spans(Some(start), Some(end), None, None, None)
    }

    fn flag(node: Node, key: &str) -> Node {
        node.with_property(key.to_string(), "true".to_string())
    }

    fn contains(parent: &Node, child: &Node) -> Edge {
        Edge::new(parent.id, child.id, EdgeType::Contains)
    }

    fn assert_ok(nodes: Vec<Node>, edges: Vec<Edge>) {
        let tmp = NamedTempFile::new().unwrap();
        write_columnar_from_nodes_edges(nodes, edges, tmp.path()).unwrap();
        let col = open_col(tmp.path());
        assert_graph_invariants(&col).expect("snapshot must satisfy INV-1..INV-9");
    }

    fn assert_err(nodes: Vec<Node>, edges: Vec<Edge>, needle: &str) {
        let tmp = NamedTempFile::new().unwrap();
        write_columnar_from_nodes_edges(nodes, edges, tmp.path()).unwrap();
        let col = open_col(tmp.path());
        let err = assert_graph_invariants(&col).unwrap_err().to_string();
        assert!(err.contains(needle), "{err}");
    }

    #[test]
    fn inv8_rejects_interface_member_assigned_container_span() {
        let iface = spanned(NodeType::Interface, "a.go", "I", "I", 0, 40);
        let method = spanned(NodeType::Function, "a.go", "M", "I.M", 0, 40);
        let err_has_ranges = {
            let tmp = NamedTempFile::new().unwrap();
            write_columnar_from_nodes_edges(
                vec![iface.clone(), method.clone()],
                vec![contains(&iface, &method)],
                tmp.path(),
            )
            .unwrap();
            let err = assert_graph_invariants(&open_col(tmp.path()))
                .unwrap_err()
                .to_string();
            assert!(err.contains("INV-8"), "{err}");
            assert!(err.contains("equals container span"), "{err}");
            assert!(err.contains("Interface I"), "{err}");
            assert!(err.contains("Function M"), "{err}");
            assert!(err.contains("0..40"), "{err}");
            true
        };
        assert!(err_has_ranges);
    }

    #[test]
    fn inv8_rejects_field_span_equal_to_class() {
        let class = spanned(NodeType::Class, "A.java", "C", "C", 0, 40);
        let field =
            spanned(NodeType::Variable, "A.java", "f", "C.f", 0, 40).with_label("field".into());
        assert_err(
            vec![class.clone(), field.clone()],
            vec![contains(&class, &field)],
            "equals container span",
        );
    }

    #[test]
    fn inv8_rejects_whole_file_function_span() {
        let file = spanned(NodeType::File, "only.go", "only.go", "only.go", 0, 24);
        let function = spanned(NodeType::Function, "only.go", "main", "main", 0, 24);
        assert_err(
            vec![file.clone(), function.clone()],
            vec![contains(&file, &function)],
            "equals container span",
        );
    }

    #[test]
    fn inv8_rejects_reverse_containment() {
        let class = spanned(NodeType::Class, "a.rs", "C", "C", 10, 30);
        let method = spanned(NodeType::Function, "a.rs", "m", "C::m", 0, 50);
        assert_err(
            vec![class.clone(), method.clone()],
            vec![contains(&class, &method)],
            "reverse containment",
        );
    }

    #[test]
    fn inv8_rejects_boundary_straddle() {
        let class = spanned(NodeType::Struct, "a.rs", "S", "S", 10, 40);
        let overflows = spanned(NodeType::Function, "a.rs", "m", "S::m", 20, 60);
        assert_err(
            vec![class.clone(), overflows.clone()],
            vec![contains(&class, &overflows)],
            "boundary straddle",
        );

        let class = spanned(NodeType::Class, "a.cpp", "C", "C", 10, 40);
        let overlaps_start = spanned(NodeType::Function, "a.cpp", "n", "C::n", 0, 20);
        assert_err(
            vec![class.clone(), overlaps_start.clone()],
            vec![contains(&class, &overlaps_start)],
            "boundary straddle",
        );
    }

    #[test]
    fn inv8_rejects_identifier_outside_declaration_and_file_bounds() {
        let name_outside = Node::new(NodeType::Function, "reallylongname")
            .with_file_path("a.rs")
            .with_qualified_name("reallylongname")
            .with_byte_spans(Some(0), Some(5), Some(0), Some(20), None);
        assert_err(
            vec![name_outside],
            vec![],
            "identifier span outside declaration",
        );

        let file = Node::new(NodeType::File, "a.rs")
            .with_file_path("a.rs")
            .with_property("file_byte_len".into(), "10".into());
        let past_eof = spanned(NodeType::Function, "a.rs", "a", "a", 0, 25);
        assert_err(
            vec![file, past_eof],
            vec![],
            "span exceeds file byte length",
        );

        let zero_width = spanned(NodeType::Function, "a.rs", "empty", "empty", 10, 10);
        assert_err(vec![zero_width], vec![], "invalid or empty byte span");

        let line2_at_zero =
            spanned(NodeType::Function, "a.rs", "root_bug", "root_bug", 0, 50).with_location(3, 5);
        assert_err(vec![line2_at_zero], vec![], "cannot start at byte 0");
    }

    #[test]
    fn inv9_rejects_sibling_functions_sharing_span() {
        let a = spanned(NodeType::Function, "a.rs", "a", "a", 0, 100);
        let b = spanned(NodeType::Function, "a.rs", "b", "b", 0, 100);
        let file = Node::new(NodeType::File, "a.rs").with_file_path("a.rs");
        let err = {
            let tmp = NamedTempFile::new().unwrap();
            write_columnar_from_nodes_edges(
                vec![file.clone(), a.clone(), b.clone()],
                vec![contains(&file, &a), contains(&file, &b)],
                tmp.path(),
            )
            .unwrap();
            assert_graph_invariants(&open_col(tmp.path()))
                .unwrap_err()
                .to_string()
        };
        assert!(err.contains("INV-9"), "{err}");
        assert!(err.contains("Function a"), "{err}");
        assert!(err.contains("Function b"), "{err}");
        assert!(err.contains("0..100"), "{err}");
    }

    #[test]
    fn inv8_inv9_allow_out_of_line_methods_and_synthetic_accessors() {
        let file = Node::new(NodeType::File, "span.rs")
            .with_file_path("span.rs")
            .with_property("file_byte_len".into(), "400".into());
        let go_s = spanned(NodeType::Struct, "span.rs", "GoS", "GoS", 0, 30);
        let go_inline = spanned(NodeType::Function, "span.rs", "inline", "GoS.inline", 4, 18);
        let go_ool = spanned(NodeType::Function, "span.rs", "M", "GoS.M", 40, 70);
        let rust_s = spanned(NodeType::Struct, "span.rs", "RustS", "RustS", 80, 110);
        let rust_ool = spanned(NodeType::Function, "span.rs", "m", "RustS::m", 120, 150);
        let cpp_c = spanned(NodeType::Class, "span.rs", "CppC", "CppC", 160, 190);
        let cpp_ool = spanned(
            NodeType::Function,
            "span.rs",
            "method",
            "CppC::method",
            200,
            230,
        );
        let data = spanned(NodeType::Class, "span.rs", "Data", "Data", 240, 300);
        let real = spanned(NodeType::Function, "span.rs", "real", "Data.real", 248, 270);
        let getter = flag(
            spanned(NodeType::Function, "span.rs", "getX", "Data.getX", 240, 300),
            "lombok_synthesized",
        );
        let setter = flag(
            spanned(NodeType::Function, "span.rs", "setX", "Data.setX", 240, 300),
            "is_synthetic",
        );
        let macro_a = flag(
            spanned(NodeType::Function, "span.rs", "genA", "genA", 310, 330),
            "multi_binding",
        );
        let macro_b = flag(
            spanned(NodeType::Function, "span.rs", "genB", "genB", 310, 330),
            "primary_header_synthesized",
        );
        let fn_a = spanned(NodeType::Function, "span.rs", "fnA", "fnA", 340, 360);
        let fn_b = spanned(NodeType::Function, "span.rs", "fnB", "fnB", 370, 390);
        let edges = vec![
            contains(&go_s, &go_inline),
            contains(&go_s, &go_ool),
            contains(&rust_s, &rust_ool),
            contains(&cpp_c, &cpp_ool),
            contains(&data, &real),
            contains(&data, &getter),
            contains(&data, &setter),
            contains(&file, &fn_a),
            contains(&file, &fn_b),
        ];
        assert_ok(
            vec![
                file, go_s, go_inline, go_ool, rust_s, rust_ool, cpp_c, cpp_ool, data, real,
                getter, setter, macro_a, macro_b, fn_a, fn_b,
            ],
            edges,
        );
    }
}
