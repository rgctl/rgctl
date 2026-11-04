//! Persisted CLI navigation across the automation graph; each command reopens mmap.
#[path = "support/automation.rs"]
mod automation;
use automation::{cli, fixture};
use rgctl_graph::{
    SnapshotNodeStore, assert_graph_invariants,
    schema::{EdgeType, NodeType},
};

#[test]
fn empty_automation_state_reuses_and_safely_invalidates_snapshots() {
    use std::fs;
    let repo = tempfile::TempDir::new().unwrap();
    let root = repo.path();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("associated.html"), "<p>{{ person }}</p>\n").unwrap();
    let sidecar = root.join(".rgctl/template-dependencies.bin");
    let snapshot = root.join(".rgctl/graph.snapshot.bin");
    let tracker = root.join(".rgctl/file_hashes.json");

    cli(root, &["discover", "."]);
    assert!(!sidecar.exists());
    let original_modified = fs::metadata(&snapshot).unwrap().modified().unwrap();
    let state = rgctl_incremental::FileTracker::load(root).unwrap();
    assert!(state.empty_automation().is_some());
    cli(root, &["discover", "."]);
    assert_eq!(
        original_modified,
        fs::metadata(&snapshot).unwrap().modified().unwrap()
    );

    // A stale proof must not authorize reuse of a different graph snapshot.
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(&tracker).unwrap()).unwrap();
    metadata["empty_automation"]["digest"] = "stale".into();
    fs::write(&tracker, serde_json::to_vec(&metadata).unwrap()).unwrap();
    cli(root, &["discover", "."]);
    let state = rgctl_incremental::FileTracker::load(root).unwrap();
    assert_eq!(
        state.empty_automation().unwrap().digest,
        SnapshotNodeStore::open(&snapshot)
            .unwrap()
            .content_digest()
            .unwrap()
    );

    // A settings change must promote the file even though its bytes did not change.
    fs::write(
        root.join("rgctl.toml"),
        "[languages.jinja]\ninclude=['associated.html']\n",
    )
    .unwrap();
    cli(root, &["discover", "."]);
    assert!(sidecar.is_file());
    assert!(
        cli(root, &["find", "person", "--exact", "--lang", "jinja"])["total"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        rgctl_incremental::FileTracker::load(root)
            .unwrap()
            .empty_automation()
            .is_none()
    );

    fs::remove_file(root.join("rgctl.toml")).unwrap();
    cli(root, &["discover", "."]);
    assert!(!sidecar.exists());
    assert!(
        rgctl_incremental::FileTracker::load(root)
            .unwrap()
            .empty_automation()
            .is_some()
    );
    assert_eq!(
        cli(root, &["find", "person", "--exact", "--lang", "jinja"])["total"],
        0
    );
}

#[test]
fn incremental_update_tracks_transition_to_and_from_empty_automation() {
    use std::fs;
    let repo = tempfile::TempDir::new().unwrap();
    let root = repo.path();
    let deploy = root.join("deploy.yml");
    let sidecar = root.join(".rgctl/template-dependencies.bin");
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(&deploy, "- hosts: all\n  tasks: []\n").unwrap();
    cli(root, &["discover", "."]);
    assert!(sidecar.is_file());

    fs::remove_file(&deploy).unwrap();
    cli(root, &["update", "--files", "deploy.yml"]);
    assert!(!sidecar.exists());
    assert!(
        rgctl_incremental::FileTracker::load(root)
            .unwrap()
            .empty_automation()
            .is_some()
    );
    cli(root, &["discover", "."]);
    assert_eq!(
        cli(root, &["find", "--type", "ansibleplaybook"])["total"],
        0
    );

    fs::write(&deploy, "- hosts: all\n  tasks: []\n").unwrap();
    cli(root, &["update", "--files", "deploy.yml"]);
    assert!(sidecar.is_file());
    assert!(
        rgctl_incremental::FileTracker::load(root)
            .unwrap()
            .empty_automation()
            .is_none()
    );
    assert_eq!(
        cli(root, &["find", "--type", "ansibleplaybook"])["total"],
        1
    );
}

#[test]
fn literal_task_files_role_entrypoints_and_template_lookups_survive_updates() {
    use std::fs;
    let repo = tempfile::TempDir::new().unwrap();
    let root = repo.path();
    for dir in ["tasks", "roles/web/tasks", "roles/web/templates", "other"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    let deploy = "- hosts: all\n  tasks:\n    - name: Include standalone\n      include_tasks: tasks/shared.yml\n    - name: Select role tasks\n      include_role: {name: web, tasks_from: setup}\n";
    fs::write(root.join("deploy.yml"), deploy).unwrap();
    fs::write(root.join("second.yml"), "- hosts: all\n  tasks:\n    - name: Select same role\n      include_role: {name: web, tasks_from: setup}\n    - name: Lookup by relative path\n      debug: {msg: \"{{ lookup('template', 'roles/web/templates/config.j2') }}\"}\n").unwrap();
    fs::write(root.join("other/third.yml"), "- hosts: all\n  tasks:\n    - name: Include same role task from another directory\n      include_tasks: ../roles/web/tasks/setup.yml\n").unwrap();
    fs::write(
        root.join("tasks/shared.yml"),
        "- name: Included task\n  debug: {msg: ok}\n",
    )
    .unwrap();
    fs::write(
        root.join("tasks/unrelated.yml"),
        "- name: unrelated configuration\n  value: one\n",
    )
    .unwrap();
    fs::write(
        root.join("roles/web/tasks/main.yml"),
        "- name: Default role task\n  debug: {msg: default}\n",
    )
    .unwrap();
    fs::write(
        root.join("roles/web/tasks/setup.yml"),
        "- name: Render role lookup\n  debug: {msg: \"{{ lookup('template', 'config.j2') }}\"}\n",
    )
    .unwrap();
    fs::write(
        root.join("roles/web/templates/config.j2"),
        "{{ (computer | default('WIN'))[0:15] }} {{ password }}",
    )
    .unwrap();
    let snapshot = root.join(".rgctl/graph.snapshot.bin");
    let digest = || {
        SnapshotNodeStore::open(&snapshot)
            .unwrap()
            .content_digest()
            .unwrap()
            .to_owned()
    };
    cli(root, &["discover", ".", "-l", "ansible,jinja"]);
    let cold_digest = digest();
    cli(root, &["discover", ".", "-l", "ansible,jinja"]);
    assert_eq!(digest(), cold_digest, "warm discovery changed the graph");
    let inspect = || {
        let store = SnapshotNodeStore::open(&root.join(".rgctl/graph.snapshot.bin")).unwrap();
        let col = store.columnar().unwrap();
        let nodes: Vec<_> = (0..col.node_count())
            .map(|i| col.materialize_node_at(i).unwrap())
            .collect();
        let edges = col.edge_topology_typed().unwrap();
        (nodes, edges)
    };
    let (nodes, edges) = inspect();
    let edge_to = |from: &str, suffix: &str, ty| {
        edges.iter().any(|(a, b, edge)| {
            *edge == ty
                && nodes.iter().any(|n| n.id == *a && n.name == from)
                && nodes.iter().any(|n| {
                    n.id == *b && n.file_path.as_deref().is_some_and(|p| p.ends_with(suffix))
                })
        })
    };
    assert!(edge_to(
        "Include standalone",
        "tasks/shared.yml",
        EdgeType::DependsOn
    ));
    assert!(edge_to(
        "Select role tasks",
        "roles/web/tasks/setup.yml",
        EdgeType::DependsOn
    ));
    assert!(!edge_to(
        "Select role tasks",
        "roles/web/tasks/main.yml",
        EdgeType::DependsOn
    ));
    assert!(edge_to(
        "Render role lookup",
        "roles/web/templates/config.j2",
        EdgeType::RendersTemplate
    ));
    assert!(edge_to(
        "Lookup by relative path",
        "roles/web/templates/config.j2",
        EdgeType::RendersTemplate
    ));
    assert!(nodes.iter().any(|n| n.name == "Included task"));
    assert!(nodes.iter().any(|n| n.name == "password"));
    assert!(
        !nodes
            .iter()
            .any(|n| n.name == "unrelated configuration" && n.node_type == NodeType::AnsibleTask)
    );
    cli(root, &["discover", ".", "-l", "ansible,jinja"]);
    fs::write(root.join("deploy.yml"), "- hosts: all\n  tasks:\n    - name: Select role tasks\n      include_role: {name: web, tasks_from: setup}\n").unwrap();
    cli(
        root,
        &["update", "--files", "deploy.yml", "-l", "ansible,jinja"],
    );
    assert!(!inspect().0.iter().any(|n| n.name == "Included task"));
    fs::write(root.join("deploy.yml"), deploy).unwrap();
    cli(
        root,
        &["update", "--files", "deploy.yml", "-l", "ansible,jinja"],
    );
    assert!(inspect().0.iter().any(|n| n.name == "Included task"));
    let updated_digest = digest();
    fs::remove_dir_all(root.join(".rgctl")).unwrap();
    cli(root, &["discover", ".", "-l", "ansible,jinja"]);
    assert_eq!(digest(), updated_digest, "updated and cold graphs differ");
}

#[test]
fn imported_playbooks_with_the_same_loader_root_resolve_literal_lookups() {
    use std::fs;
    let repo = tempfile::TempDir::new().unwrap();
    let root = repo.path();
    fs::create_dir_all(root.join("templates")).unwrap();
    fs::write(root.join("first.yml"), "- import_playbook: child.yml\n").unwrap();
    fs::write(root.join("second.yml"), "- import_playbook: child.yml\n").unwrap();
    fs::write(root.join("child.yml"), "- hosts: all\n  tasks:\n    - name: Render shared lookup\n      debug: {msg: \"{{ lookup('template', 'config.j2') }}\"}\n").unwrap();
    fs::write(root.join("templates/config.j2"), "{{ input }}").unwrap();
    cli(root, &["discover", ".", "-l", "ansible,jinja"]);
    let renders = cli(
        root,
        &["relations", "--edge", "renderstemplate", "--limit", "100"],
    );
    assert_eq!(renders["edges"].as_array().unwrap().len(), 1, "{renders}");
    fs::create_dir_all(root.join("other/templates")).unwrap();
    fs::write(
        root.join("other/wrapper.yml"),
        "- import_playbook: ../child.yml\n",
    )
    .unwrap();
    fs::write(root.join("other/templates/config.j2"), "{{ other_input }}").unwrap();
    cli(root, &["discover", ".", "-l", "ansible,jinja"]);
    let renders = cli(
        root,
        &["relations", "--edge", "renderstemplate", "--limit", "100"],
    );
    assert!(renders["edges"].as_array().unwrap().is_empty(), "{renders}");
}

#[test]
fn referenced_task_appearance_and_deletion_reclassify_without_bypassing_excludes() {
    use std::fs;
    let repo = tempfile::TempDir::new().unwrap();
    let root = repo.path();
    fs::create_dir_all(root.join("tasks")).unwrap();
    fs::write(root.join("deploy.yml"), "- hosts: all\n  tasks:\n    - name: Include later\n      include_tasks: tasks/later.yml\n    - name: Include excluded\n      include_tasks: tasks/excluded.yml\n").unwrap();
    fs::write(
        root.join("tasks/excluded.yml"),
        "- name: Excluded task\n  debug: {msg: no}\n",
    )
    .unwrap();
    let discover = || {
        cli(
            root,
            &[
                "discover",
                ".",
                "-l",
                "ansible,jinja",
                "-e",
                "tasks/excluded.yml",
            ],
        )
    };
    let total = |name: &str| {
        cli(root, &["find", name, "--exact", "--lang", "ansible"])["total"]
            .as_u64()
            .unwrap()
    };
    discover();
    assert_eq!(total("Excluded task"), 0);
    fs::write(
        root.join("tasks/later.yml"),
        "- name: Later task\n  debug: {msg: yes}\n",
    )
    .unwrap();
    discover();
    assert_eq!(total("Later task"), 1);
    assert_eq!(total("Excluded task"), 0);
    fs::remove_file(root.join("tasks/later.yml")).unwrap();
    discover();
    assert_eq!(total("Later task"), 0);
}
#[test]
fn ansible_cli_persists_roles_templates_variables_and_runtime_evidence() {
    let repo = fixture("ecommerce-ansible");
    let root = repo.path();
    cli(root, &["discover", ".", "-l", "ansible,jinja"]);
    let found = cli(
        root,
        &["find", "Render config", "--exact", "--lang", "ansible"],
    );
    assert_eq!(found["schema_version"], 2);
    assert_eq!(found["entities"].as_array().unwrap().len(), 2);
    let inventory = cli(root, &["inventory", "--by", "lang"]);
    for language in ["ansible", "jinja"] {
        assert!(
            inventory["counts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["key"] == language && entry["count"].as_u64().unwrap() > 0),
            "{inventory}"
        );
    }
    let renders = cli(
        root,
        &["relations", "--edge", "renderstemplate", "--limit", "100"],
    );
    assert_eq!(renders["edges"].as_array().unwrap().len(), 2, "{renders}");
    for edge in ["includesrole", "dependson", "references", "usesfact"] {
        assert!(
            cli(root, &["relations", "--edge", edge])["total"]
                .as_u64()
                .unwrap()
                > 0,
            "{edge}"
        );
    }
    cli(root, &["callers", "webLabel"]);
    cli(root, &["callees", "correctnessRoot"]);
    let graph = SnapshotNodeStore::open(&root.join(".rgctl/graph.snapshot.bin")).unwrap();
    let col = graph.columnar().unwrap();
    assert_graph_invariants(col).unwrap();
    let nodes: Vec<_> = (0..col.node_count())
        .map(|i| col.materialize_node_at(i).unwrap())
        .collect();
    for (from, to, edge) in col.edge_topology_typed().unwrap() {
        let source = nodes.iter().find(|node| node.id == from).unwrap();
        let target = nodes.iter().find(|node| node.id == to).unwrap();
        if edge == EdgeType::RendersTemplate || edge == EdgeType::NotifiesHandler {
            assert!(
                target
                    .file_path
                    .as_deref()
                    .unwrap()
                    .starts_with(source.get_property("role_root").unwrap())
            );
        }
        if edge == EdgeType::RendersTemplate {
            assert_eq!(target.node_type, NodeType::File);
        }
    }
    assert!(nodes.iter().any(|node| {
        node.get_property("source_references_json")
            .unwrap_or("")
            .contains("dynamic_target")
    }));
    assert!(!nodes.iter().any(|node| node.node_type == NodeType::Class));
    // Existing JSON GQL node results retain compact scalar evidence after reopen.
    let evidence = cli(
        root,
        &[
            "gql",
            "MATCH (n:AnsibleTask) WHERE n.source_references_json LIKE '*' RETURN n LIMIT 100",
        ],
    );
    assert!(evidence["count"].as_u64().unwrap_or(0) > 0, "{evidence}");
    cli(root, &["discover", ".", "-l", "ansible,jinja"]);
    let warm = SnapshotNodeStore::open(&root.join(".rgctl/graph.snapshot.bin")).unwrap();
    assert_eq!(
        graph.content_digest().unwrap(),
        warm.content_digest().unwrap()
    );
    cli(root, &["discover", "."]);
    let all = SnapshotNodeStore::open(&root.join(".rgctl/graph.snapshot.bin")).unwrap();
    let col = all.columnar().unwrap();
    assert!(
        (0..col.node_count())
            .map(|i| col.materialize_node_at(i).unwrap())
            .any(|node| node.node_type == NodeType::ConfigKey
                && node
                    .file_path
                    .as_deref()
                    .unwrap_or("")
                    .ends_with("inventory.yml"))
    );
}

#[test]
fn warm_language_filter_changes_reclassify_yaml_and_update_tracker() {
    let repo = tempfile::TempDir::new().unwrap();
    let root = repo.path();
    std::fs::write(
        root.join("deploy.yaml"),
        "- hosts: all\n  tasks:\n    - debug: {msg: '{{ value }}'}\n",
    )
    .unwrap();
    std::fs::write(root.join("base.j2"), "{{ value }}").unwrap();
    cli(root, &["discover", ".", "-l", "ansible"]);
    assert!(
        cli(root, &["find", "--lang", "ansible"])["total"]
            .as_u64()
            .unwrap()
            > 0
    );
    cli(root, &["discover", ".", "-l", "yaml"]);
    assert_eq!(cli(root, &["find", "--lang", "ansible"])["total"], 0);
    let tracker = rgctl_incremental::FileTracker::load(root).unwrap();
    assert!(!tracker.file_hashes().contains_key("deploy.yaml"));
    cli(root, &["discover", ".", "-l", "jinja2"]);
    assert!(
        cli(root, &["find", "--lang", "jinja"])["total"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn discover_recovers_a_missing_sidecar_from_persisted_metadata() {
    let repo = fixture("ecommerce-ansible");
    let root = repo.path();
    cli(root, &["discover", "."]);
    let snapshot = root.join(".rgctl/graph.snapshot.bin");
    let before = SnapshotNodeStore::open(&snapshot)
        .unwrap()
        .content_digest()
        .unwrap()
        .to_owned();
    std::fs::remove_file(root.join(".rgctl/template-dependencies.bin")).unwrap();
    cli(root, &["discover", "."]);
    let after = SnapshotNodeStore::open(&snapshot).unwrap();
    assert_eq!(before, after.content_digest().unwrap());
    assert!(rgctl_extraction::TemplateDependencies::load(root, &before).is_some());
}

#[test]
fn collection_qualified_roles_resolve_to_exact_local_layouts() {
    let repo = tempfile::TempDir::new().unwrap();
    let root = repo.path();
    let role = root.join("ansible_collections/acme/demo/roles/web");
    std::fs::create_dir_all(role.join("tasks")).unwrap();
    std::fs::create_dir_all(role.join("templates")).unwrap();
    std::fs::write(
        root.join("deploy.yml"),
        "- hosts: all\n  roles: [acme.demo.web]\n",
    )
    .unwrap();
    std::fs::write(
        role.join("tasks/main.yml"),
        "- template: {src: config.j2, dest: /etc/config}\n",
    )
    .unwrap();
    std::fs::write(role.join("templates/config.j2"), "{{ service }}").unwrap();
    cli(root, &["discover", ".", "-l", "ansible,jinja"]);
    assert_eq!(
        cli(root, &["relations", "--edge", "includesrole"])["total"],
        1
    );
    assert_eq!(
        cli(root, &["relations", "--edge", "renderstemplate"])["total"],
        1
    );
}

#[test]
fn excluded_yaml_classifications_are_warm_reusable_and_refresh_on_edits() {
    let repo = tempfile::TempDir::new().unwrap();
    let root = repo.path();
    std::fs::write(root.join("deploy.yml"), "- hosts: all\n  tasks: []\n").unwrap();
    std::fs::write(
        root.join("inventory.yml"),
        "all:\n  hosts:\n    example: {}\n",
    )
    .unwrap();
    cli(root, &["discover", ".", "-l", "ansible"]);
    let path = root.join(".rgctl/graph.snapshot.bin");
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    cli(root, &["discover", ".", "-l", "ansible"]);
    assert_eq!(
        modified,
        std::fs::metadata(&path).unwrap().modified().unwrap()
    );
    std::fs::remove_file(root.join(".rgctl/template-dependencies.bin")).unwrap();
    cli(root, &["discover", ".", "-l", "ansible"]);
    assert_eq!(
        modified,
        std::fs::metadata(&path).unwrap().modified().unwrap()
    );
    std::fs::write(
        root.join("inventory.yml"),
        "- hosts: localhost\n  tasks: []\n",
    )
    .unwrap();
    cli(root, &["discover", ".", "-l", "ansible"]);
    assert_eq!(
        cli(root, &["find", "--type", "ansibleplaybook"])["total"],
        2
    );
}

#[test]
fn embedded_macro_language_matches_name_type_and_inventory_queries() {
    let repo = tempfile::TempDir::new().unwrap();
    let root = repo.path();
    std::fs::write(root.join("deploy.yml"),"- hosts: all\n  tasks:\n    - debug:\n        msg: \"{% macro embedded(value) %}{{ value }}{% endmacro %}{{ embedded(input) }}\"\n").unwrap();
    cli(root, &["discover", ".", "-l", "ansible"]);
    assert_eq!(
        cli(root, &["find", "embedded", "--exact", "--lang", "ansible"])["total"],
        1
    );
    assert_eq!(
        cli(root, &["find", "--type", "function", "--lang", "ansible"])["total"],
        1
    );
    assert_eq!(
        cli(root, &["find", "embedded", "--exact", "--lang", "jinja"])["total"],
        0
    );
    assert_eq!(
        cli(root, &["find", "--type", "function", "--lang", "jinja"])["total"],
        0
    );
}
