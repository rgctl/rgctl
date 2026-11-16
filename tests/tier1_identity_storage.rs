//! Canonical identity storage and its property-only snapshot migration.
use rgctl_extraction::GraphBuilder;
use rgctl_graph::{
    SnapshotNodeStore, VecDiffSink, diff_snapshots,
    schema::{Node, NodeType},
    stable_key_from_node, stable_key_from_row, write_columnar_from_nodes_edges,
};
use rgctl_plugin_api::{SourceLocation, Symbol, SymbolType};
use std::path::Path;

fn build(
    identity: Option<&str>,
    dispatch: Option<&str>,
) -> (Vec<Node>, Vec<rgctl_graph::schema::Edge>) {
    let mut metadata = serde_json::json!({});
    if let Some(identity) = identity {
        metadata["identity_discriminator"] = identity.into();
    }
    if let Some(dispatch) = dispatch {
        metadata["override_identity"] = dispatch.into();
    }
    let symbol = Symbol {
        name: "quote".into(),
        qualified_name: Some("demo.Library.quote".into()),
        symbol_type: SymbolType::Function,
        location: SourceLocation {
            file: "Library.java".into(),
            start_line: 1,
            end_line: 1,
            start_column: 0,
            end_column: 20,
        },
        metadata,
        ..Default::default()
    };
    let mut builder = GraphBuilder::new();
    let file = builder.ensure_file_node(Path::new("Library.java"));
    builder.add_symbol(&symbol, file);
    builder.into_graph()
}

#[test]
fn redundant_identity_properties_are_removed_without_losing_dispatch_contracts() {
    for (identity, dispatch, stored_dispatch) in [
        (Some("(int)"), Some("(int)"), None),
        (Some("declaration:(int)"), Some("(int)"), Some("(int)")),
        (Some("definition:1"), Some(""), Some("")),
        (Some("(int)"), None, None),
        (None, Some("(int)"), Some("(int)")),
        (None, Some(""), Some("")),
    ] {
        let (nodes, _) = build(identity, dispatch);
        let node = nodes
            .iter()
            .find(|node| node.node_type == NodeType::Function)
            .unwrap();
        assert_eq!(node.identity_discriminator(), identity);
        assert_eq!(node.get_property("callable_identity"), identity);
        assert_eq!(node.get_property("identity_discriminator"), None);
        assert_eq!(node.get_property("override_identity"), stored_dispatch);

        let mut expected = Node::new(NodeType::Function, "quote")
            .with_qualified_name("demo.Library.quote")
            .with_file_path("Library.java");
        if let Some(identity) = identity {
            expected = expected.with_identity_discriminator(identity);
        }
        assert_eq!(node.id, expected.id);
    }
}

#[test]
fn older_redundant_properties_remain_readable_and_diff_as_metadata_only() {
    let (nodes, edges) = build(Some("(int)"), Some("(int)"));
    let mut older_nodes = nodes.clone();
    for node in &mut older_nodes {
        if node.node_type == NodeType::Function {
            node.properties
                .insert("identity_discriminator".into(), "(int)".into());
            node.properties
                .insert("override_identity".into(), "(int)".into());
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("before.bin");
    let head_path = dir.path().join("after.bin");
    // The wire layout is unchanged. These nodes reproduce the earlier writer's
    // redundant properties while retaining the canonical prefix/property pair.
    write_columnar_from_nodes_edges(older_nodes, edges.clone(), &base_path).unwrap();
    write_columnar_from_nodes_edges(nodes, edges, &head_path).unwrap();
    let base = SnapshotNodeStore::open(&base_path).unwrap();
    let head = SnapshotNodeStore::open(&head_path).unwrap();
    for store in [&base, &head] {
        let columnar = store.columnar().unwrap();
        for index in 0..columnar.node_count() {
            let node = columnar.materialize_node_at(index).unwrap();
            assert_eq!(
                stable_key_from_row(columnar, index).unwrap(),
                stable_key_from_node(&node)
            );
            if node.node_type == NodeType::Function {
                assert_eq!(node.identity_discriminator(), Some("(int)"));
                assert_eq!(
                    columnar.node_identity_discriminator_str(index).unwrap(),
                    Some("(int)")
                );
            }
        }
    }
    let mut changes = VecDiffSink::default();
    let stats = diff_snapshots(&base, &head, &mut changes).unwrap();
    assert_eq!(stats.nodes_added, 0);
    assert_eq!(stats.nodes_removed, 0);
    assert_eq!(stats.nodes_changed, 1);
    assert_eq!(stats.edges_added, 0);
    assert_eq!(stats.edges_removed, 0);
    assert!(
        std::fs::metadata(&head_path).unwrap().len() < std::fs::metadata(&base_path).unwrap().len()
    );
}
