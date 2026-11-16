//! Cross-language Method Resolution Order (MRO) and Overrides emission.
//!
//! Builds a dense (`0..V_types`) inheritance subgraph from `Extends` /
//! `Implements` edges, computes per-type MRO chains, emits
//! [`EdgeType::Overrides`], and retargets typed Calls via the MRO.

use rgctl_error::Result;
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType, SharedStr};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn dispatch_identity(node: &Node) -> Option<&str> {
    node.get_property("override_identity")
        .or_else(|| node.identity_discriminator())
        .filter(|s| !s.is_empty())
}

/// Language-specific MRO strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MroStrategy {
    /// Strict C3 superclass linearization (Python).
    #[default]
    PythonC3,
    /// Depth-first leftmost-base walk (C++).
    CppLeftmost,
    /// Class inheritance chain first, then interfaces (Java / C# / Kotlin / TS / PHP).
    ClassOverInterface,
    /// Class + included modules in reverse-include order (Ruby).
    RubyModules,
    /// Inherent type methods before trait methods (Rust).
    RustInherentOverTrait,
}

impl MroStrategy {
    /// Infer strategy from a language id string.
    pub fn from_language(lang: &str) -> Self {
        match lang {
            "python" => Self::PythonC3,
            "cpp" | "c++" | "cxx" => Self::CppLeftmost,
            "java" | "csharp" | "c#" | "kotlin" | "typescript" | "javascript" | "php" => {
                Self::ClassOverInterface
            }
            "ruby" => Self::RubyModules,
            "rust" => Self::RustInherentOverTrait,
            _ => Self::ClassOverInterface,
        }
    }
}

/// Dense inheritance graph over type nodes.
#[derive(Debug, Clone)]
pub struct InheritanceGraph {
    /// Dense id → graph Uuid.
    pub type_ids: Vec<Uuid>,
    /// Uuid → dense id.
    pub id_of: HashMap<Uuid, u32>,
    /// Unique simple / qualified name → dense id; ambiguous names are excluded.
    pub name_of: HashMap<SharedStr, u32>,
    /// Direct bases in declaration order: `(base_dense_id, is_interface)`.
    pub bases: Vec<SmallVec<[(u32, bool); 4]>>,
    /// True when the type node is an interface/trait.
    pub is_interface: Vec<bool>,
}

impl InheritanceGraph {
    /// Collect Class / Struct / Enum / Interface / Module type nodes and inheritance edges.
    pub fn from_backend(backend: &MemoryBackend) -> Result<Self> {
        let mut type_ids = Vec::new();
        let mut id_of = HashMap::new();
        let mut name_of: HashMap<SharedStr, u32> = HashMap::new();
        let mut is_interface = Vec::new();
        let mut ambiguous_names = HashSet::new();

        for &nt in &[
            NodeType::Class,
            NodeType::Struct,
            NodeType::Enum,
            NodeType::Interface,
            NodeType::Module,
            NodeType::Annotation,
        ] {
            for node in backend.find_nodes_by_type(nt)? {
                let dense = type_ids.len() as u32;
                id_of.insert(node.id, dense);
                for name in std::iter::once(&node.name).chain(node.qualified_name.as_ref()) {
                    if ambiguous_names.contains(name) {
                        continue;
                    }
                    if name_of.get(name).is_some_and(|&previous| previous != dense) {
                        name_of.remove(name);
                        ambiguous_names.insert(name.clone());
                    } else {
                        name_of.insert(name.clone(), dense);
                    }
                }
                type_ids.push(node.id);
                is_interface.push(matches!(
                    node.node_type,
                    NodeType::Interface | NodeType::Annotation
                ));
            }
        }

        let n = type_ids.len();
        let mut bases = vec![SmallVec::new(); n];

        for edge in backend.find_edges_by_type(EdgeType::Extends)? {
            if let (Some(&child), Some(&parent)) = (id_of.get(&edge.from), id_of.get(&edge.to)) {
                let iface = is_interface[parent as usize];
                bases[child as usize].push((parent, iface));
            }
        }
        for edge in backend.find_edges_by_type(EdgeType::Implements)? {
            if let (Some(&child), Some(&parent)) = (id_of.get(&edge.from), id_of.get(&edge.to)) {
                bases[child as usize].push((parent, true));
            }
        }
        // Ruby / Puppet-style include edges also contribute to the chain.
        for edge in backend.find_edges_by_type(EdgeType::IncludesClass)? {
            if let (Some(&child), Some(&parent)) = (id_of.get(&edge.from), id_of.get(&edge.to)) {
                bases[child as usize].push((parent, true));
            }
        }

        Ok(Self {
            type_ids,
            id_of,
            name_of,
            bases,
            is_interface,
        })
    }

    /// Number of types.
    pub fn len(&self) -> usize {
        self.type_ids.len()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.type_ids.is_empty()
    }
}

/// Per-type MRO chains (dense ids, self first).
#[derive(Debug, Clone)]
pub struct MroIndex {
    /// Strategy used to build chains.
    pub strategy: MroStrategy,
    /// Inheritance substrate.
    pub graph: InheritanceGraph,
    /// `mro[t]` = linearization of type `t` (includes `t` at index 0).
    pub mro: Vec<SmallVec<[u32; 8]>>,
    /// Methods defined on each type: `method_name → function Uuid`.
    pub methods: Vec<HashMap<String, Vec<Uuid>>>,
    /// Canonical parameter identity used to avoid inventing overrides between overloads.
    pub method_identities: HashMap<Uuid, Option<String>>,
    /// Trait implementations can override only their declared trait owner.
    pub override_owners: HashMap<Uuid, String>,
    /// One runtime implementation may satisfy several TypeScript overload declarations.
    pub signature_families: HashSet<Uuid>,
}

impl MroIndex {
    /// Build MRO for every type using `strategy`.
    pub fn build(graph: InheritanceGraph, strategy: MroStrategy) -> Result<Self> {
        let n = graph.len();
        let mut mro = vec![SmallVec::new(); n];
        let mut visiting = vec![0u8; n]; // 0 = white, 1 = gray, 2 = black
        for t in 0..n {
            compute_mro(t as u32, &graph, strategy, &mut mro, &mut visiting)?;
        }
        Ok(Self {
            strategy,
            graph,
            mro,
            methods: vec![HashMap::new(); n],
            method_identities: HashMap::new(),
            override_owners: HashMap::new(),
            signature_families: HashSet::new(),
        })
    }

    /// Index Function nodes onto owning types via qualified-name prefix.
    pub fn index_methods_from_backend(&mut self, backend: &MemoryBackend) -> Result<()> {
        let nodes = backend.find_nodes_by_type(NodeType::Function)?;
        let implemented: HashSet<_> = nodes
            .iter()
            .filter(|n| {
                n.get_property("callable_role") != Some("declaration")
                    && n.get_property("superseded_definition") != Some("true")
            })
            .filter_map(|n| {
                split_owner_method(n.qualified_name.as_deref(), &n.name)
                    .map(|(owner, method)| (owner, method, dispatch_identity(n)))
            })
            .collect();
        for node in &nodes {
            if node.get_property("callable_role") == Some("declaration")
                && split_owner_method(node.qualified_name.as_deref(), &node.name).is_some_and(
                    |(owner, method)| {
                        implemented.contains(&(owner, method, dispatch_identity(node)))
                    },
                )
            {
                continue;
            }
            if node.get_property("language") == Some("typescript") {
                self.signature_families.insert(node.id);
            }
            if node
                .get_property("superseded_definition")
                .is_some_and(|role| role == "true")
            {
                continue;
            }
            if let Some(owner) = node.get_property("override_owner") {
                self.override_owners.insert(node.id, owner.to_owned());
            }
            if let Some((owner, method)) =
                split_owner_method(node.qualified_name.as_deref(), &node.name)
                && let Some(&tid) = self.graph.name_of.get(owner)
            {
                self.methods[tid as usize]
                    .entry(method.to_string())
                    .or_default()
                    .push(node.id);
                self.method_identities
                    .insert(node.id, dispatch_identity(node).map(str::to_owned));
            }
        }
        Ok(())
    }

    /// Walk `mro[recv]` for the first type defining `method`.
    pub fn resolve_method(&self, recv_type: u32, method: &str) -> Option<Uuid> {
        let chain = self.mro.get(recv_type as usize)?;
        for &tid in chain {
            if let Some(ids) = self.methods.get(tid as usize)?.get(method) {
                return match ids.as_slice() {
                    [id] => Some(*id),
                    _ => None,
                };
            }
        }
        None
    }

    /// Resolve by type name (simple or qualified) + method name.
    pub fn resolve_method_by_type_name(&self, type_name: &str, method: &str) -> Option<Uuid> {
        let tid = *self.graph.name_of.get(type_name)?;
        self.resolve_method(tid, method)
    }

    /// Emit Overrides edges: subtype method → nearest ancestor method of the same name.
    pub fn overrides_edges(&self) -> Vec<Edge> {
        let mut out = Vec::new();
        for (tid, methods) in self.methods.iter().enumerate() {
            for (name, children) in methods {
                for &child in children {
                    let identity = self
                        .method_identities
                        .get(&child)
                        .and_then(|i| i.as_deref());
                    for &ancestor in self.mro[tid].iter().skip(1) {
                        if self.override_owners.get(&child).is_some_and(|owner| {
                            self.graph.name_of.get(owner.as_str()) != Some(&ancestor)
                        }) {
                            continue;
                        }
                        let Some(candidates) = self.methods[ancestor as usize].get(name) else {
                            continue;
                        };
                        let matches: Vec<_> = candidates
                            .iter()
                            .copied()
                            .filter(|id| {
                                let parent =
                                    self.method_identities.get(id).and_then(|i| i.as_deref());
                                identity == parent
                                    && (identity.is_some()
                                        || candidates.len() == 1 && children.len() == 1
                                        || self.signature_families.contains(&child)
                                            && self.signature_families.contains(id))
                            })
                            .collect();
                        if matches.len() == 1
                            || !matches.is_empty()
                                && self.signature_families.contains(&child)
                                && matches
                                    .iter()
                                    .all(|id| self.signature_families.contains(id))
                        {
                            for parent in matches {
                                if child != parent {
                                    out.push(
                                        Edge::new(child, parent, EdgeType::Overrides)
                                            .with_property("method".into(), name.clone()),
                                    );
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
        out
    }
}

/// C3 merge of linearized lists + the direct-bases list.
///
/// Returns `None` when the hierarchy is inconsistent (no legal merge).
pub fn c3_merge(lists: &[SmallVec<[u32; 8]>]) -> Option<SmallVec<[u32; 8]>> {
    let mut lists: Vec<SmallVec<[u32; 8]>> = lists.to_vec();
    let mut result = SmallVec::new();
    loop {
        lists.retain(|l| !l.is_empty());
        if lists.is_empty() {
            return Some(result);
        }
        let mut candidate: Option<u32> = None;
        for list in &lists {
            let head = list[0];
            let appears_in_tail = lists
                .iter()
                .any(|other| other.iter().skip(1).any(|&x| x == head));
            if !appears_in_tail {
                candidate = Some(head);
                break;
            }
        }
        let head = candidate?;
        result.push(head);
        for list in &mut lists {
            if list.first().copied() == Some(head) {
                list.remove(0);
            }
        }
    }
}

/// Pure C3 linearization for one type given precomputed ancestor MROs.
pub fn c3_linearize(
    self_id: u32,
    base_mros: &[SmallVec<[u32; 8]>],
    bases_in_order: &[u32],
) -> Option<SmallVec<[u32; 8]>> {
    let mut lists: Vec<SmallVec<[u32; 8]>> = base_mros.to_vec();
    lists.push(bases_in_order.iter().copied().collect());
    let mut out = SmallVec::new();
    out.push(self_id);
    let merged = c3_merge(&lists)?;
    out.extend(merged);
    Some(out)
}

fn compute_mro(
    t: u32,
    graph: &InheritanceGraph,
    strategy: MroStrategy,
    mro: &mut [SmallVec<[u32; 8]>],
    visiting: &mut [u8],
) -> Result<()> {
    let ti = t as usize;
    if visiting[ti] == 2 {
        return Ok(());
    }
    if visiting[ti] == 1 {
        // Cycle — fall back to `[self]` only.
        let mut only = SmallVec::new();
        only.push(t);
        mro[ti] = only;
        visiting[ti] = 2;
        return Ok(());
    }
    visiting[ti] = 1;

    for &(base, _) in &graph.bases[ti] {
        compute_mro(base, graph, strategy, mro, visiting)?;
    }

    mro[ti] = match strategy {
        MroStrategy::PythonC3 => linearize_c3(t, graph, mro),
        MroStrategy::CppLeftmost => linearize_leftmost(t, graph, mro),
        MroStrategy::ClassOverInterface => linearize_class_over_interface(t, graph, mro),
        MroStrategy::RubyModules => linearize_ruby(t, graph, mro),
        MroStrategy::RustInherentOverTrait => linearize_rust(t, graph, mro),
    };
    visiting[ti] = 2;
    Ok(())
}

fn linearize_c3(
    t: u32,
    graph: &InheritanceGraph,
    mro: &[SmallVec<[u32; 8]>],
) -> SmallVec<[u32; 8]> {
    let bases: Vec<u32> = graph.bases[t as usize].iter().map(|(b, _)| *b).collect();
    let base_mros: Vec<SmallVec<[u32; 8]>> =
        bases.iter().map(|&b| mro[b as usize].clone()).collect();
    c3_linearize(t, &base_mros, &bases).unwrap_or_else(|| {
        let mut out = SmallVec::new();
        out.push(t);
        out
    })
}

fn linearize_leftmost(
    t: u32,
    graph: &InheritanceGraph,
    mro: &[SmallVec<[u32; 8]>],
) -> SmallVec<[u32; 8]> {
    let mut out = SmallVec::new();
    out.push(t);
    let mut seen = HashSet::from([t]);
    for &(base, _) in &graph.bases[t as usize] {
        for &x in &mro[base as usize] {
            if seen.insert(x) {
                out.push(x);
            }
        }
    }
    out
}

fn linearize_class_over_interface(
    t: u32,
    graph: &InheritanceGraph,
    mro: &[SmallVec<[u32; 8]>],
) -> SmallVec<[u32; 8]> {
    let mut out = SmallVec::new();
    out.push(t);
    let mut seen = HashSet::from([t]);
    // Class bases first (is_interface == false), then interfaces.
    let mut class_bases = Vec::new();
    let mut iface_bases = Vec::new();
    for &(base, iface) in &graph.bases[t as usize] {
        if iface || graph.is_interface[base as usize] {
            iface_bases.push(base);
        } else {
            class_bases.push(base);
        }
    }
    for base in class_bases.into_iter().chain(iface_bases) {
        for &x in &mro[base as usize] {
            if seen.insert(x) {
                out.push(x);
            }
        }
    }
    out
}

fn linearize_ruby(
    t: u32,
    graph: &InheritanceGraph,
    mro: &[SmallVec<[u32; 8]>],
) -> SmallVec<[u32; 8]> {
    // Ruby: self → reverse(included modules) → superclass chain.
    // `from_backend` pushes Extends before IncludesClass; partition so modules
    // (is_interface) are searched before the superclass, last-include first.
    let mut out = SmallVec::new();
    out.push(t);
    let mut seen = HashSet::from([t]);
    let mut modules = Vec::new();
    let mut supers = Vec::new();
    for &(base, iface) in &graph.bases[t as usize] {
        if iface || graph.is_interface[base as usize] {
            modules.push(base);
        } else {
            supers.push(base);
        }
    }
    for &base in modules.iter().rev().chain(supers.iter()) {
        for &x in &mro[base as usize] {
            if seen.insert(x) {
                out.push(x);
            }
        }
    }
    out
}

fn linearize_rust(
    t: u32,
    graph: &InheritanceGraph,
    mro: &[SmallVec<[u32; 8]>],
) -> SmallVec<[u32; 8]> {
    // Inherent type first; trait (interface) bases after.
    linearize_class_over_interface(t, graph, mro)
}

/// Split `Owner.method` / `Owner::method` / Ruby `Owner#method` / bare `method`.
fn split_owner_method<'a>(qn: Option<&'a str>, name: &'a str) -> Option<(&'a str, &'a str)> {
    let qn = qn.unwrap_or(name);
    if let Some((owner, method)) = qn.rsplit_once("::")
        && !owner.is_empty()
        && !method.is_empty()
    {
        // Strip turbofish / generics on owner: `Foo::<init>` → treat as constructor.
        let method = method.split('<').next().unwrap_or(method);
        let owner = owner.split('<').next().unwrap_or(owner);
        return Some((owner, method));
    }
    if let Some((owner, method)) = qn.rsplit_once('#')
        && !owner.is_empty()
        && !method.is_empty()
    {
        return Some((owner, method));
    }
    if let Some((owner, method)) = qn.rsplit_once('.')
        && !owner.is_empty()
        && !method.is_empty()
    {
        return Some((owner, method));
    }
    None
}

/// Full MRO pass: build index, emit Overrides, retarget typed Calls.
pub fn apply_mro_pass(backend: &mut MemoryBackend, strategy: MroStrategy) -> Result<MroPassStats> {
    let graph = InheritanceGraph::from_backend(backend)?;
    if graph.is_empty() {
        return Ok(MroPassStats::default());
    }
    let mut index = MroIndex::build(graph, strategy)?;
    index.index_methods_from_backend(backend)?;

    let overrides = index.overrides_edges();
    let overrides_emitted = overrides.len();
    if !overrides.is_empty() {
        backend.insert_edges_batch(overrides)?;
    }

    let calls_retargeted = retarget_calls_via_mro(backend, &index)?;
    Ok(MroPassStats {
        types: index.graph.len(),
        overrides_emitted,
        calls_retargeted,
    })
}

/// Stats from [`apply_mro_pass`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MroPassStats {
    /// Type nodes considered.
    pub types: usize,
    /// Overrides edges inserted.
    pub overrides_emitted: usize,
    /// Calls edges retargeted to inherited methods.
    pub calls_retargeted: usize,
}

fn retarget_calls_via_mro(backend: &mut MemoryBackend, index: &MroIndex) -> Result<usize> {
    let all = backend.all_edges()?;
    let mut extras = Vec::new();
    let mut seen: HashSet<(Uuid, Uuid)> = HashSet::new();

    for edge in &all {
        if edge.edge_type != EdgeType::Calls {
            continue;
        }
        // A canonical target is already resolved. A name-only MRO pass must not
        // replace it with another overload on a subtype.
        if backend
            .with_node(edge.to, |node| node.identity_discriminator().is_some())?
            .unwrap_or(false)
        {
            continue;
        }
        let Some(type_name) = edge
            .properties
            .get("to_type_hint")
            .or_else(|| edge.properties.get("recv_type"))
            .map(|s| s.as_str())
        else {
            continue;
        };
        let method = match edge.properties.get("callee").map(|s| s.as_str()) {
            Some(m) if !m.is_empty() => m.to_string(),
            _ => match backend.with_node(edge.to, |n| n.name.as_str().to_string())? {
                Some(m) if !m.is_empty() => m,
                _ => continue,
            },
        };
        let Some(resolved) = index.resolve_method_by_type_name(type_name, &method) else {
            continue;
        };
        if resolved == edge.to || !seen.insert((edge.from, resolved)) {
            continue;
        }
        if backend.has_edge(edge.from, resolved, EdgeType::Calls) {
            continue;
        }
        let mut e = Edge::new(edge.from, resolved, EdgeType::Calls)
            .with_property("resolved_via".into(), "mro".into())
            .with_property("callee".into(), method)
            .with_property("to_type_hint".into(), type_name.to_string());
        if let Some(line) = edge.properties.get("call_site_line") {
            e = e.with_property("call_site_line".into(), line.clone());
        }
        extras.push(e);
    }

    let n = extras.len();
    if !extras.is_empty() {
        backend.insert_edges_batch(extras)?;
    }
    Ok(n)
}

/// Persist type / callee hints on Calls edges so the MRO pass can retarget them.
pub fn annotate_call_edge(mut edge: Edge, to_type_hint: Option<&str>, callee: &str) -> Edge {
    if let Some(hint) = to_type_hint
        && !hint.is_empty()
    {
        edge = edge.with_property("to_type_hint".into(), hint.to_string());
    }
    if !callee.is_empty() {
        edge = edge.with_property("callee".into(), callee.to_string());
    }
    edge
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::Node;

    fn type_node(name: &str, nt: NodeType) -> Node {
        Node::new(nt, name.to_string()).with_qualified_name(name.to_string())
    }

    fn fn_node(owner: &str, method: &str) -> Node {
        let qn = format!("{owner}.{method}");
        Node::new(NodeType::Function, method.to_string()).with_qualified_name(qn)
    }

    fn build_diamond_python() -> (MemoryBackend, Uuid, Uuid, Uuid, Uuid) {
        let mut backend = MemoryBackend::new();
        let a = type_node("A", NodeType::Class);
        let b = type_node("B", NodeType::Class);
        let c = type_node("C", NodeType::Class);
        let d = type_node("D", NodeType::Class);
        let a_id = a.id;
        let b_id = b.id;
        let c_id = c.id;
        let d_id = d.id;
        for n in [a, b, c, d] {
            backend.insert_node(n).unwrap();
        }
        // B(A), C(A), D(B, C)
        backend
            .insert_edges_batch(vec![
                Edge::new(b_id, a_id, EdgeType::Extends),
                Edge::new(c_id, a_id, EdgeType::Extends),
                Edge::new(d_id, b_id, EdgeType::Extends),
                Edge::new(d_id, c_id, EdgeType::Extends),
            ])
            .unwrap();
        (backend, a_id, b_id, c_id, d_id)
    }

    #[test]
    fn overloaded_methods_are_not_first_match_and_overrides_match_identity() {
        let mut backend = MemoryBackend::new();
        let base = type_node("Base", NodeType::Class).with_file_path("Base.java");
        let child = type_node("Child", NodeType::Class).with_file_path("Child.java");
        let ids = (base.id, child.id);
        let int = fn_node("Base", "quote")
            .with_file_path("Base.java")
            .with_identity_discriminator("(int)");
        let string = fn_node("Base", "quote")
            .with_file_path("Base.java")
            .with_identity_discriminator("(String)");
        let override_string = fn_node("Child", "quote")
            .with_file_path("Child.java")
            .with_identity_discriminator("(String)");
        let methods = (int.id, string.id, override_string.id);
        for node in [base, child, int, string, override_string] {
            backend.insert_node(node).unwrap();
        }
        backend
            .insert_edges_batch(vec![Edge::new(ids.1, ids.0, EdgeType::Extends)])
            .unwrap();
        let graph = InheritanceGraph::from_backend(&backend).unwrap();
        let mut index = MroIndex::build(graph, MroStrategy::ClassOverInterface).unwrap();
        index.index_methods_from_backend(&backend).unwrap();
        assert_eq!(index.resolve_method_by_type_name("Base", "quote"), None);
        let edges = index.overrides_edges();
        assert_eq!(edges.len(), 1);
        assert_eq!((edges[0].from, edges[0].to), (methods.2, methods.1));
    }

    #[test]
    fn c3_merge_simple() {
        let lists = vec![
            SmallVec::from_slice(&[1u32, 2, 3]),
            SmallVec::from_slice(&[4u32, 2, 3]),
            SmallVec::from_slice(&[1u32, 4]),
        ];
        let merged = c3_merge(&lists).expect("merge");
        assert_eq!(&merged[..], &[1, 4, 2, 3]);
    }

    #[test]
    fn python_diamond_c3_order() {
        let (backend, a, b, c, d) = build_diamond_python();
        let graph = InheritanceGraph::from_backend(&backend).unwrap();
        let index = MroIndex::build(graph, MroStrategy::PythonC3).unwrap();
        let d_dense = *index.graph.id_of.get(&d).unwrap();
        let chain: Vec<Uuid> = index.mro[d_dense as usize]
            .iter()
            .map(|&i| index.graph.type_ids[i as usize])
            .collect();
        // L[D] = [D] + merge(L[B], L[C], [B, C]) = [D, B, C, A]
        assert_eq!(chain, vec![d, b, c, a]);
    }

    #[test]
    fn diamond_resolves_process_on_c() {
        let (mut backend, _a, _b, c, d) = build_diamond_python();
        let c_process = fn_node("C", "process");
        let c_fn = c_process.id;
        backend.insert_node(c_process).unwrap();
        backend.insert_node(fn_node("A", "process")).unwrap();

        let mut index = MroIndex::build(
            InheritanceGraph::from_backend(&backend).unwrap(),
            MroStrategy::PythonC3,
        )
        .unwrap();
        index.index_methods_from_backend(&backend).unwrap();
        let d_dense = *index.graph.id_of.get(&d).unwrap();
        let resolved = index.resolve_method(d_dense, "process").unwrap();
        assert_eq!(resolved, c_fn);
        let _ = c;
    }

    #[test]
    fn overrides_edge_nearest_ancestor() {
        let mut backend = MemoryBackend::new();
        let base = type_node("Base", NodeType::Class);
        let child = type_node("Child", NodeType::Class);
        let base_id = base.id;
        let child_id = child.id;
        backend.insert_node(base).unwrap();
        backend.insert_node(child).unwrap();
        backend
            .insert_edges_batch(vec![Edge::new(child_id, base_id, EdgeType::Extends)])
            .unwrap();

        let base_exec = fn_node("Base", "execute");
        let child_exec = fn_node("Child", "execute");
        let base_fn = base_exec.id;
        let child_fn = child_exec.id;
        backend.insert_node(base_exec).unwrap();
        backend.insert_node(child_exec).unwrap();

        let stats = apply_mro_pass(&mut backend, MroStrategy::ClassOverInterface).unwrap();
        assert_eq!(stats.overrides_emitted, 1);
        let overrides = backend.find_edges_by_type(EdgeType::Overrides).unwrap();
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].from, child_fn);
        assert_eq!(overrides[0].to, base_fn);
    }

    #[test]
    fn class_over_interface_puts_class_before_iface() {
        let mut backend = MemoryBackend::new();
        let cls = type_node("Cls", NodeType::Class);
        let base = type_node("Base", NodeType::Class);
        let iface = type_node("IFace", NodeType::Interface);
        let cls_id = cls.id;
        let base_id = base.id;
        let iface_id = iface.id;
        for n in [cls, base, iface] {
            backend.insert_node(n).unwrap();
        }
        // Declaration order: interface listed before class base — strategy should still
        // put Base (class) ahead of IFace.
        backend
            .insert_edges_batch(vec![
                Edge::new(cls_id, iface_id, EdgeType::Implements),
                Edge::new(cls_id, base_id, EdgeType::Extends),
            ])
            .unwrap();
        let index = MroIndex::build(
            InheritanceGraph::from_backend(&backend).unwrap(),
            MroStrategy::ClassOverInterface,
        )
        .unwrap();
        let tid = *index.graph.id_of.get(&cls_id).unwrap();
        let chain: Vec<Uuid> = index.mro[tid as usize]
            .iter()
            .map(|&i| index.graph.type_ids[i as usize])
            .collect();
        assert_eq!(chain[0], cls_id);
        let base_pos = chain.iter().position(|u| *u == base_id).unwrap();
        let iface_pos = chain.iter().position(|u| *u == iface_id).unwrap();
        assert!(base_pos < iface_pos);
    }

    #[test]
    fn rust_inherent_before_trait() {
        let mut backend = MemoryBackend::new();
        let ty = type_node("Widget", NodeType::Struct);
        let tr = type_node("Drawable", NodeType::Interface);
        let ty_id = ty.id;
        let tr_id = tr.id;
        backend.insert_node(ty).unwrap();
        backend.insert_node(tr).unwrap();
        backend
            .insert_edges_batch(vec![Edge::new(ty_id, tr_id, EdgeType::Implements)])
            .unwrap();
        let inherent = fn_node("Widget", "draw");
        let trait_fn = fn_node("Drawable", "draw");
        let inherent_id = inherent.id;
        backend.insert_node(inherent).unwrap();
        backend.insert_node(trait_fn).unwrap();
        let mut index = MroIndex::build(
            InheritanceGraph::from_backend(&backend).unwrap(),
            MroStrategy::RustInherentOverTrait,
        )
        .unwrap();
        index.index_methods_from_backend(&backend).unwrap();
        let tid = *index.graph.id_of.get(&ty_id).unwrap();
        assert_eq!(index.resolve_method(tid, "draw"), Some(inherent_id));
    }

    #[test]
    fn multi_hop_inherited_call() {
        let mut backend = MemoryBackend::new();
        let a = type_node("A", NodeType::Class);
        let b = type_node("B", NodeType::Class);
        let c = type_node("C", NodeType::Class);
        let a_id = a.id;
        let b_id = b.id;
        let c_id = c.id;
        for n in [a, b, c] {
            backend.insert_node(n).unwrap();
        }
        backend
            .insert_edges_batch(vec![
                Edge::new(b_id, a_id, EdgeType::Extends),
                Edge::new(c_id, b_id, EdgeType::Extends),
            ])
            .unwrap();
        let a_run = fn_node("A", "run");
        let a_fn = a_run.id;
        backend.insert_node(a_run).unwrap();
        let mut index = MroIndex::build(
            InheritanceGraph::from_backend(&backend).unwrap(),
            MroStrategy::ClassOverInterface,
        )
        .unwrap();
        index.index_methods_from_backend(&backend).unwrap();
        let c_dense = *index.graph.id_of.get(&c_id).unwrap();
        assert_eq!(index.resolve_method(c_dense, "run"), Some(a_fn));
    }

    #[test]
    fn retarget_self_call_via_mro() {
        let mut backend = MemoryBackend::new();
        let base = type_node("Base", NodeType::Class);
        let child = type_node("Child", NodeType::Class);
        let base_id = base.id;
        let child_id = child.id;
        backend.insert_node(base).unwrap();
        backend.insert_node(child).unwrap();
        backend
            .insert_edges_batch(vec![Edge::new(child_id, base_id, EdgeType::Extends)])
            .unwrap();

        let base_run = fn_node("Base", "run");
        let child_caller = fn_node("Child", "caller");
        let stub = fn_node("Child", "run_stub");
        let base_fn = base_run.id;
        let caller_fn = child_caller.id;
        let stub_fn = stub.id;
        backend.insert_node(base_run).unwrap();
        backend.insert_node(child_caller).unwrap();
        backend.insert_node(stub).unwrap();

        // Unresolved/stub Calls edge with receiver type hint → retarget to Base.run.
        backend
            .insert_edges_batch(vec![annotate_call_edge(
                Edge::new(caller_fn, stub_fn, EdgeType::Calls),
                Some("Child"),
                "run",
            )])
            .unwrap();

        let stats = apply_mro_pass(&mut backend, MroStrategy::ClassOverInterface).unwrap();
        assert_eq!(stats.calls_retargeted, 1);
        let calls = backend.find_edges_by_type(EdgeType::Calls).unwrap();
        assert!(
            calls.iter().any(|e| e.from == caller_fn
                && e.to == base_fn
                && e.properties.get("resolved_via").map(|s| s.as_str()) == Some("mro")),
            "expected MRO-retargeted Calls to Base.run, got {calls:?}"
        );
    }

    #[test]
    fn ruby_hash_separator_indexes_instance_methods() {
        let mut backend = MemoryBackend::new();
        let dog = type_node("Dog", NodeType::Class);
        let animal = type_node("Animal", NodeType::Class);
        let dog_id = dog.id;
        let animal_id = animal.id;
        backend.insert_node(dog).unwrap();
        backend.insert_node(animal).unwrap();
        backend
            .insert_edges_batch(vec![Edge::new(dog_id, animal_id, EdgeType::Extends)])
            .unwrap();
        let bark = Node::new(NodeType::Function, "bark".to_string())
            .with_qualified_name("Animal#bark".to_string());
        let bark_id = bark.id;
        backend.insert_node(bark).unwrap();
        let mut index = MroIndex::build(
            InheritanceGraph::from_backend(&backend).unwrap(),
            MroStrategy::RubyModules,
        )
        .unwrap();
        index.index_methods_from_backend(&backend).unwrap();
        let tid = *index.graph.id_of.get(&dog_id).unwrap();
        assert_eq!(index.resolve_method(tid, "bark"), Some(bark_id));
    }

    #[test]
    fn ruby_modules_before_superclass() {
        let mut backend = MemoryBackend::new();
        let cls = type_node("Dog", NodeType::Class);
        let parent = type_node("Animal", NodeType::Class);
        let mixin = type_node("Walkable", NodeType::Module);
        let cls_id = cls.id;
        let parent_id = parent.id;
        let mixin_id = mixin.id;
        for n in [cls, parent, mixin] {
            backend.insert_node(n).unwrap();
        }
        // Extends recorded before include (matches from_backend push order).
        backend
            .insert_edges_batch(vec![
                Edge::new(cls_id, parent_id, EdgeType::Extends),
                Edge::new(cls_id, mixin_id, EdgeType::IncludesClass),
            ])
            .unwrap();
        let index = MroIndex::build(
            InheritanceGraph::from_backend(&backend).unwrap(),
            MroStrategy::RubyModules,
        )
        .unwrap();
        let tid = *index.graph.id_of.get(&cls_id).unwrap();
        let chain: Vec<Uuid> = index.mro[tid as usize]
            .iter()
            .map(|&i| index.graph.type_ids[i as usize])
            .collect();
        assert_eq!(chain[0], cls_id);
        let mixin_pos = chain.iter().position(|u| *u == mixin_id).unwrap();
        let parent_pos = chain.iter().position(|u| *u == parent_id).unwrap();
        assert!(
            mixin_pos < parent_pos,
            "module should precede superclass, chain={chain:?}"
        );
    }

    #[test]
    fn interface_default_method_resolution() {
        let mut backend = MemoryBackend::new();
        let cls = type_node("Impl", NodeType::Class);
        let iface = type_node("Runnable", NodeType::Interface);
        let cls_id = cls.id;
        let iface_id = iface.id;
        backend.insert_node(cls).unwrap();
        backend.insert_node(iface).unwrap();
        backend
            .insert_edges_batch(vec![Edge::new(cls_id, iface_id, EdgeType::Implements)])
            .unwrap();
        let default_run = fn_node("Runnable", "run");
        let default_id = default_run.id;
        backend.insert_node(default_run).unwrap();

        let mut index = MroIndex::build(
            InheritanceGraph::from_backend(&backend).unwrap(),
            MroStrategy::ClassOverInterface,
        )
        .unwrap();
        index.index_methods_from_backend(&backend).unwrap();
        let tid = *index.graph.id_of.get(&cls_id).unwrap();
        assert_eq!(index.resolve_method(tid, "run"), Some(default_id));
    }
}
