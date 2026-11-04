use rgctl_graph::{
    schema::{EdgeType, Node, NodeType},
    snapshot::SnapshotNodeStore,
};
use rgctl_pipeline::{PipelineConfig, ProcessingPipeline};
use std::{fs, path::Path, sync::Arc};
use tempfile::TempDir;
fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn nodes(store: &SnapshotNodeStore) -> Vec<Node> {
    let col = store.columnar().unwrap();
    (0..col.node_count())
        .map(|i| col.materialize_node_at(i).unwrap())
        .collect()
}
#[test]
fn exact_role_template_handlers_and_macro_edges_survive_spill() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "deploy.yaml", "- hosts: all\n  roles: [web, db]\n");
    for role in ["web", "db"] {
        write(
            root,
            &format!("roles/{role}/tasks/main.yml"),
            "- name: Render config\n  template:\n    src: config.j2\n    dest: /etc/config\n  notify: reload\n- name: Missing\n  template:\n    src: missing.j2\n    dest: /etc/missing\n- name: Custom syntax\n  template:\n    src: config.j2\n    variable_start_string: '[['\n    variable_end_string: ']]'\n",
        );
        write(
            root,
            &format!("roles/{role}/handlers/main.yml"),
            "- name: Restart one\n  listen: reload\n  debug: {msg: ok}\n- name: Restart two\n  listen: reload\n  debug: {msg: ok}\n",
        );
        write(
            root,
            &format!("roles/{role}/templates/config.j2"),
            "{% import 'macros.jinja' as fmt %}{{ fmt.label(service) }}\n{% include ['missing.j2','fragment.conf'] %}\n",
        );
        write(
            root,
            &format!("roles/{role}/templates/macros.jinja"),
            "{% macro label(value) %}{{ value }}{% endmacro %}",
        );
        write(
            root,
            &format!("roles/{role}/templates/fragment.conf"),
            "port={{ port }}",
        );
        write(
            root,
            &format!("roles/{role}/defaults/main.yml"),
            "port: 80\n",
        );
    }
    write(root, "inventory.yaml", "all:\n  hosts:\n    localhost:\n");
    let registry = Arc::new(rgctl_languages::default_registry());
    let path = root.join(".rgctl/graph.snapshot.bin");
    let pipeline = ProcessingPipeline::with_config(
        registry.clone(),
        PipelineConfig {
            thread_count: Some(1),
            show_progress: false,
            ..Default::default()
        },
    );
    let (_, digest) = pipeline
        .process_repository_to_snapshot(root, &path, None)
        .unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();
    let col = store.columnar().unwrap();
    rgctl_graph::invariants::assert_graph_invariants(col).unwrap();
    let all = nodes(&store);
    let edges = col.edge_topology_typed().unwrap();
    let render_edges: Vec<_> = edges
        .iter()
        .filter(|(_, _, kind)| *kind == EdgeType::RendersTemplate)
        .collect();
    assert_eq!(render_edges.len(), 4);
    for (from, to, _) in render_edges {
        let source = all.iter().find(|n| n.id == *from).unwrap();
        let target = all.iter().find(|n| n.id == *to).unwrap();
        assert_eq!(target.node_type, NodeType::File);
        let role = source.get_property("role_root").unwrap();
        assert!(target.file_path.as_deref().unwrap().starts_with(role));
        if source.name.as_str() == "Custom syntax" {
            assert!(
                source
                    .get_property("source_references_json")
                    .unwrap()
                    .contains("external_profile_unresolved")
            );
        }
    }
    assert_eq!(
        edges
            .iter()
            .filter(|(_, _, kind)| *kind == EdgeType::NotifiesHandler)
            .count(),
        4
    );
    assert_eq!(
        edges
            .iter()
            .filter(|(_, _, kind)| *kind == EdgeType::Calls)
            .count(),
        2
    );
    assert_eq!(
        edges
            .iter()
            .filter(|(_, _, kind)| *kind == EdgeType::IncludesRole)
            .count(),
        2
    );
    assert!(all.iter().any(|n| {
        n.name.as_str() == "Missing"
            && n.get_property("source_references_json")
                .unwrap_or("")
                .contains("missing_or_filtered_target")
    }));
    assert!(!all.iter().any(|n| n.node_type == NodeType::Class));
    let (_, warm) = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            thread_count: Some(4),
            show_progress: false,
            ..Default::default()
        },
    )
    .process_repository_to_snapshot(root, &path, None)
    .unwrap();
    assert_eq!(digest, warm);
}

#[test]
fn workers_apply_filters_after_yaml_classification() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "deploy.yaml",
        "- hosts: all\n  tasks:\n    - debug: {msg: '{{ value }}'}\n",
    );
    write(root, "config.yaml", "value: 1\n");
    write(root, "base.j2", "{{ value }}");
    for (selected, expected) in [
        (vec!["ansible"], 1),
        (vec!["yaml"], 1),
        (vec!["jinja"], 1),
        (vec!["ansible", "jinja"], 2),
    ] {
        let mut config = PipelineConfig {
            show_progress: false,
            ..Default::default()
        };
        config.discovery.languages = Some(selected.iter().map(|s| s.to_string()).collect());
        let path = root.join(".rgctl/graph.snapshot.bin");
        let (stats, _) =
            ProcessingPipeline::with_config(Arc::new(rgctl_languages::default_registry()), config)
                .process_repository_to_snapshot(root, &path, None)
                .unwrap();
        assert_eq!(stats.files_processed, expected, "{selected:?}");
        let store = SnapshotNodeStore::open(&path).unwrap();
        let all = nodes(&store);
        assert_eq!(
            all.iter().filter(|n| n.node_type == NodeType::File).count(),
            expected
        );
    }
}

#[test]
fn literal_targets_reuse_walk_eligibility_and_preserve_other_template_routes() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "deploy.yaml",
        "- hosts: all\n  tasks:\n    - template: {src: nginx.conf, dest: /etc/nginx.conf}\n    - template: {src: nginx.conf, dest: /etc/second}\n    - template: {src: ignored.conf, dest: /etc/ignored}\n    - template: {src: binary.conf, dest: /etc/binary}\n    - template: {src: large.conf, dest: /etc/large}\n    - template: {src: link.conf, dest: /etc/link}\n",
    );
    write(
        root,
        "templates/nginx.conf",
        "server={{ server }}\n{% include 'tail' %}",
    );
    write(root, "templates/tail", "tail={{ tail }}");
    write(root, "templates/ignored.conf", "{{ ignored }}");
    write(root, "templates/binary.conf", "bad\0{{ binary }}");
    write(root, "templates/large.conf", &"x".repeat(2048));
    write(root, "templates/plain.html", "<h1>plain html</h1>");
    write(root, ".gitignore", "templates/ignored.conf\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        root.join("templates/nginx.conf"),
        root.join("templates/link.conf"),
    )
    .unwrap();
    write(
        root,
        "rgctl.toml",
        "[languages.jinja]\ntemplate_roots = ['templates']\n",
    );
    let mut config = PipelineConfig {
        show_progress: false,
        ..Default::default()
    };
    config.discovery.max_file_size = 1024;
    let path = root.join(".rgctl/graph.snapshot.bin");
    let (stats, _) =
        ProcessingPipeline::with_config(Arc::new(rgctl_languages::default_registry()), config)
            .process_repository_to_snapshot(root, &path, None)
            .unwrap();
    assert_eq!(stats.files_failed, 0);
    let store = SnapshotNodeStore::open(&path).unwrap();
    let all = nodes(&store);
    let col = store.columnar().unwrap();
    rgctl_graph::invariants::assert_graph_invariants(col).unwrap();
    assert_eq!(
        all.iter()
            .filter(|node| node.node_type == NodeType::File
                && node
                    .file_path
                    .as_deref()
                    .unwrap_or("")
                    .ends_with("nginx.conf"))
            .count(),
        1
    );
    assert_eq!(
        col.edge_topology_typed()
            .unwrap()
            .iter()
            .filter(|(_, _, kind)| *kind == EdgeType::RendersTemplate)
            .count(),
        2
    );
    assert!(all.iter().any(|node| {
        node.node_type == NodeType::Module
            && node
                .file_path
                .as_deref()
                .unwrap_or("")
                .ends_with("/templates/tail")
    }));
    let plain = all
        .iter()
        .find(|node| {
            node.node_type == NodeType::File
                && node
                    .file_path
                    .as_deref()
                    .unwrap_or("")
                    .ends_with("plain.html")
        })
        .unwrap();
    assert_eq!(plain.get_property("language"), Some("html"));
    for excluded in ["ignored.conf", "binary.conf", "large.conf", "link.conf"] {
        assert!(
            !all.iter()
                .any(|node| node.file_path.as_deref().unwrap_or("").ends_with(excluded)),
            "{excluded}"
        );
    }
}

#[test]
fn reused_task_contexts_and_notification_collisions_stay_unresolved() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "rgctl.toml",
        "[languages.ansible]\ninclude=['shared.yml']\n",
    );
    for side in ["left", "right"] {
        write(
            root,
            &format!("{side}/deploy.yml"),
            "- hosts: all\n  tasks:\n    - import_tasks: ../shared.yml\n",
        );
        write(root, &format!("{side}/templates/config.j2"), "{{ value }}");
    }
    write(
        root,
        "shared.yml",
        "- name: Shared render\n  template: {src: config.j2, dest: /etc/config}\n",
    );
    write(
        root,
        "roles/web/tasks/main.yml",
        "- name: Notify duplicate\n  debug: {msg: ok}\n  notify: restart\n- name: Notify missing\n  debug: {msg: ok}\n  notify: absent\n- name: Notify dynamic\n  debug: {msg: ok}\n  notify: '{{ selected_handler }}'\n",
    );
    write(
        root,
        "roles/web/handlers/main.yml",
        "- name: restart\n  debug: {msg: first}\n- name: restart\n  debug: {msg: second}\n",
    );
    let path = root.join(".rgctl/graph.snapshot.bin");
    ProcessingPipeline::with_config(
        Arc::new(rgctl_languages::default_registry()),
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    )
    .process_repository_to_snapshot(root, &path, None)
    .unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();
    let all = nodes(&store);
    let edges = store.columnar().unwrap().edge_topology_typed().unwrap();
    assert_eq!(
        edges
            .iter()
            .filter(|(_, _, kind)| *kind == EdgeType::RendersTemplate
                || *kind == EdgeType::NotifiesHandler)
            .count(),
        0
    );
    for (name, reason) in [
        ("Shared render", "unknown_or_ambiguous_search_context"),
        ("Notify duplicate", "ambiguous_handler"),
        ("Notify missing", "missing_handler"),
        ("Notify dynamic", "dynamic_target"),
    ] {
        assert!(
            all.iter().any(|node| node.name.as_str() == name
                && node
                    .get_property("source_references_json")
                    .unwrap_or("")
                    .contains(reason)),
            "{name}: {reason}"
        );
    }
    assert_eq!(
        edges
            .iter()
            .filter(|(_, _, kind)| *kind == EdgeType::DependsOn)
            .count(),
        2
    );
}

#[test]
fn jinja_exact_calls_respect_exports_lexical_scope_and_loader_roots() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "rgctl.toml",
        "[languages.jinja]\ntemplate_roots=['templates']\n",
    );
    write(root, "templates/base.j2", "{% block body %}{% endblock %}");
    write(
        root,
        "templates/helpers.j2",
        "{% macro label(value) %}{{ value }}{% endmacro %}{% macro _secret() %}secret{% endmacro %}",
    );
    write(
        root,
        "templates/main.j2",
        "{% extends 'base.j2' %}{% import 'helpers.j2' as f %}{% macro local() %}local{% endmacro %}{% block body %}{{ local() }}{{ f.label(value) }}{{ f._secret() }}{{ missing() }}{% endblock %}",
    );
    write(
        root,
        "templates/unrelated.j2",
        "{% macro local() %}wrong target{% endmacro %}",
    );
    let path = root.join(".rgctl/graph.snapshot.bin");
    let pipeline = ProcessingPipeline::with_config(
        Arc::new(rgctl_languages::default_registry()),
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    );
    let (_, digest) = pipeline
        .process_repository_to_snapshot(root, &path, None)
        .unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();
    let col = store.columnar().unwrap();
    let all = nodes(&store);
    rgctl_graph::invariants::assert_graph_invariants(col).unwrap();
    let edges = col.edge_topology_typed().unwrap();
    assert_eq!(
        edges
            .iter()
            .filter(|(_, _, kind)| *kind == EdgeType::Calls)
            .count(),
        2
    );
    assert_eq!(
        edges
            .iter()
            .filter(|(_, _, kind)| *kind == EdgeType::Extends)
            .count(),
        1
    );
    for (_, target, kind) in edges {
        if kind == EdgeType::Calls {
            let target = all.iter().find(|node| node.id == target).unwrap();
            assert!(
                !target
                    .file_path
                    .as_deref()
                    .unwrap()
                    .ends_with("unrelated.j2")
            );
            assert_ne!(target.name.as_str(), "_secret");
        }
    }
    assert!(all.iter().any(|node| {
        node.get_property("source_references_json")
            .unwrap_or("")
            .contains("missing_or_inaccessible_macro")
    }));
    let dependencies =
        rgctl_extraction::TemplateDependencies::load(root, &digest).expect("sidecar roundtrip");
    assert!(dependencies.files().any(|file| file.ends_with("main.j2")));
    assert!(rgctl_extraction::TemplateDependencies::load(root, "wrong digest").is_none());
    std::fs::remove_file(root.join("rgctl.toml")).unwrap();
    pipeline
        .process_repository_to_snapshot(root, &path, None)
        .unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();
    let all = nodes(&store);
    assert!(all.iter().any(|node| {
        node.get_property("source_references_json")
            .unwrap_or("")
            .contains("unknown_loader_root")
    }));
}

#[test]
fn duplicate_macro_bindings_do_not_choose_a_confident_call_target() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "same.j2",
        "{% macro same() %}one{% endmacro %}{% macro same() %}two{% endmacro %}{{ same() }}",
    );
    let path = root.join(".rgctl/graph.snapshot.bin");
    ProcessingPipeline::new(Arc::new(rgctl_languages::default_registry()))
        .process_repository_to_snapshot(root, &path, None)
        .unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();
    assert!(
        !store
            .columnar()
            .unwrap()
            .edge_topology_typed()
            .unwrap()
            .iter()
            .any(|(_, _, kind)| *kind == EdgeType::Calls)
    );
    assert!(nodes(&store).iter().any(|node| {
        node.get_property("source_references_json")
            .unwrap_or("")
            .contains("ambiguous_macro")
    }));
}
