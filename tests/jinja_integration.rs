//! Standalone Jinja loader roots, callable scopes and persisted language attribution.
#[path = "support/automation.rs"]
mod automation;
use automation::{cli, fixture};
use rgctl_graph::{
    SnapshotNodeStore, assert_graph_invariants,
    schema::{EdgeType, NodeType},
};
#[test]
fn jinja_cli_resolves_loader_dependencies_and_keeps_plain_html_sql_routes() {
    let repo = fixture("ecommerce-jinja");
    let root = repo.path();
    cli(root, &["discover", "."]);
    let macros = cli(
        root,
        &[
            "find",
            "correctness*",
            "--type",
            "function",
            "--lang",
            "jinja",
        ],
    );
    assert_eq!(macros["entities"].as_array().unwrap().len(), 3, "{macros}");
    let aliases = cli(
        root,
        &[
            "find",
            "correctness*",
            "--type",
            "function",
            "--lang",
            "jinja2",
        ],
    );
    assert_eq!(aliases["total"], macros["total"]);
    let callees = cli(root, &["callees", "correctnessRoot"]);
    assert!(callees.to_string().contains("correctnessMid"));
    let callers = cli(root, &["callers", "correctnessLeaf"]);
    assert!(callers.to_string().contains("correctnessMid"));
    let graph = SnapshotNodeStore::open(&root.join(".rgctl/graph.snapshot.bin")).unwrap();
    let col = graph.columnar().unwrap();
    assert_graph_invariants(col).unwrap();
    assert_eq!(
        col.edge_topology_typed()
            .unwrap()
            .iter()
            .filter(|(_, _, edge)| *edge == EdgeType::Extends)
            .count(),
        1
    );
    let nodes: Vec<_> = (0..col.node_count())
        .map(|i| col.materialize_node_at(i).unwrap())
        .collect();
    for (suffix, language) in [("plain.html", "html"), ("schema.sql", "sql")] {
        let file = nodes
            .iter()
            .find(|node| {
                node.node_type == NodeType::File
                    && node.file_path.as_deref().unwrap_or("").ends_with(suffix)
            })
            .unwrap();
        assert_eq!(file.get_property("language"), Some(language));
    }
    std::fs::write(root.join("associated.html"), "[[ user ]]").unwrap();
    std::fs::write(root.join("rgctl.toml"),"[languages.jinja]\ninclude=['associated.html']\ntemplate_roots=['templates']\n[languages.jinja.syntax]\nvariable_start_string='[['\nvariable_end_string=']]'\n").unwrap();
    cli(root, &["discover", "."]);
    let associated = cli(
        root,
        &["find", "*", "--file", "*associated.html", "--lang", "jinja"],
    );
    assert!(associated["total"].as_u64().unwrap() > 0, "{associated}");
    let inventory = cli(root, &["inventory", "--by", "lang"]);
    assert!(
        inventory["counts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["key"] == "jinja")
    );
}
