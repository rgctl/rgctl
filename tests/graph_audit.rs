//! Synthetic graph builders for Strategy 2 audit tests.
#![allow(dead_code, unused_imports, unused_macros)]

use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};

/// Linear call chain of `n` functions: f0 → f1 → … → f{n-1}.
pub fn deep_chain(n: usize) -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    let mut ids = Vec::new();
    for i in 0..n {
        let node = Node::new(NodeType::Function, format!("f{i}"));
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for i in 0..n.saturating_sub(1) {
        backend
            .insert_edge(Edge::new(ids[i], ids[i + 1], EdgeType::Calls))
            .unwrap();
    }
    backend
}

/// Star: `leaves` callers → hub.
pub fn star(leaves: usize) -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    let hub = Node::new(NodeType::Function, "hub".into());
    let hub_id = hub.id;
    backend.insert_node(hub).unwrap();
    for i in 0..leaves {
        let leaf = Node::new(NodeType::Function, format!("leaf{i}"));
        let id = leaf.id;
        backend.insert_node(leaf).unwrap();
        backend
            .insert_edge(Edge::new(id, hub_id, EdgeType::Calls))
            .unwrap();
    }
    backend
}

/// Cycle mesh: a → b → c → … with back-edge closing the loop.
pub fn mesh_cycle(size: usize) -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    let mut ids = Vec::new();
    for i in 0..size {
        let name = char::from(b'a' + i as u8).to_string();
        let node = Node::new(NodeType::Function, name);
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }
    for i in 0..size {
        let next = (i + 1) % size;
        backend
            .insert_edge(Edge::new(ids[i], ids[next], EdgeType::Calls))
            .unwrap();
    }
    if size > 2 {
        backend
            .insert_edge(Edge::new(ids[size - 1], ids[1], EdgeType::Calls))
            .unwrap();
    }
    backend
}

pub fn structural_topology() -> (MemoryBackend, uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let mut backend = MemoryBackend::new();
    let module = Node::new(NodeType::Module, "rbuilder-analysis".to_string());
    let main_fn = Node::new(NodeType::Function, "main".to_string());
    let init_fn = Node::new(NodeType::Function, "init".to_string());
    let module_id = module.id;
    let main_id = main_fn.id;
    let init_id = init_fn.id;
    backend.insert_node(module).unwrap();
    backend.insert_node(main_fn).unwrap();
    backend.insert_node(init_fn).unwrap();
    backend
        .insert_edge(Edge::new(module_id, main_id, EdgeType::Contains))
        .unwrap();
    backend
        .insert_edge(Edge::new(main_id, init_id, EdgeType::Calls))
        .unwrap();
    (backend, module_id, main_id, init_id)
}

/// Hub with one incoming edge of each type Calls, Contains, Uses.
pub fn mixed_edge_hub() -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    let target = Node::new(NodeType::Function, "target".into());
    let caller = Node::new(NodeType::Function, "caller".into());
    let module = Node::new(NodeType::Module, "module".into());
    let importer = Node::new(NodeType::Function, "importer".into());
    let id_t = target.id;
    let id_c = caller.id;
    let id_m = module.id;
    let id_i = importer.id;
    for n in [target, caller, module, importer] {
        backend.insert_node(n).unwrap();
    }
    backend
        .insert_edge(Edge::new(id_c, id_t, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_m, id_t, EdgeType::Contains))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_i, id_t, EdgeType::Uses))
        .unwrap();
    backend
}

/// Deterministic pseudo-random call graph for parity fuzzing.
pub fn random_call_graph(seed: u64, nodes: usize) -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    let mut state = seed.wrapping_add(1);
    let mut rng = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        state
    };

    let mut ids = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let node = Node::new(NodeType::Function, format!("fn_{seed}_{i}"));
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }

    for i in 0..nodes {
        for j in (i + 1)..nodes {
            if rng() % 3 == 0 {
                backend
                    .insert_edge(Edge::new(ids[i], ids[j], EdgeType::Calls))
                    .unwrap();
            }
        }
    }
    backend
}

/// Large mixed-type graph for manual memory audits.
pub fn large_mixed_graph(nodes: usize, edges: usize) -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    let mut ids = Vec::with_capacity(nodes);
    let types = [
        NodeType::Function,
        NodeType::Module,
        NodeType::Class,
        NodeType::File,
    ];
    for i in 0..nodes {
        let node = Node::new(types[i % types.len()], format!("n{i}"));
        ids.push(node.id);
        backend.insert_node(node).unwrap();
    }

    let edge_types = [
        EdgeType::Calls,
        EdgeType::Contains,
        EdgeType::Uses,
        EdgeType::References,
        EdgeType::Unknown,
    ];
    for e in 0..edges {
        let from = ids[e % nodes];
        let to = ids[(e * 7 + 3) % nodes];
        let edge_type = edge_types[e % edge_types.len()];
        let _ = backend.insert_edge(Edge::new(from, to, edge_type));
    }
    backend
}
