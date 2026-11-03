use rgctl_graph::{CodeGraph, snapshot::SnapshotNodeStore};
use rgctl_incremental::{FileTracker, IncrementalUpdater, UpdateOptions};
use rgctl_pipeline::{PipelineConfig, ProcessingPipeline};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::TempDir;
fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn pipeline() -> ProcessingPipeline {
    ProcessingPipeline::with_config(
        Arc::new(rgctl_languages::default_registry()),
        PipelineConfig {
            show_progress: false,
            ..Default::default()
        },
    )
}
fn setup(root: &Path) -> CodeGraph {
    let snapshot = root.join(".rgctl/graph.snapshot.bin");
    let (stats, _) = pipeline()
        .process_repository_to_snapshot(root, &snapshot, None)
        .unwrap();
    let mut tracker = FileTracker::new(root);
    let files: Vec<_> = stats.file_hashes.keys().map(PathBuf::from).collect();
    tracker
        .index_files_with_mapping(&files, stats.node_path_mapping, Some(&stats.file_hashes))
        .unwrap();
    tracker.save().unwrap();
    CodeGraph::open_snapshot(&snapshot).unwrap()
}
fn assert_fresh(root: &Path) {
    let snapshot = root.join(".rgctl/graph.snapshot.bin");
    let fresh = root.join(".rgctl/fresh.snapshot.bin");
    let old = SnapshotNodeStore::open(&snapshot).unwrap();
    rgctl_graph::invariants::assert_graph_invariants(old.columnar().unwrap()).unwrap();
    let (_, digest) = pipeline()
        .process_repository_to_snapshot(root, &fresh, None)
        .unwrap();
    if old.content_digest().unwrap() != digest {
        let fresh = SnapshotNodeStore::open(&fresh).unwrap();
        let collect = |store: &SnapshotNodeStore| -> std::collections::BTreeMap<_, _> {
            let col = store.columnar().unwrap();
            (0..col.node_count())
                .map(|i| {
                    let node = col.materialize_node_at(i).unwrap();
                    (node.id, serde_json::to_value(node).unwrap())
                })
                .collect()
        };
        let a = collect(&old);
        let b = collect(&fresh);
        for id in a
            .keys()
            .chain(b.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            if a.get(id) != b.get(id) {
                eprintln!("NODE DIFF old={:?} new={:?}", a.get(id), b.get(id));
            }
        }
        let a: std::collections::HashSet<_> = old
            .columnar()
            .unwrap()
            .edge_topology_typed()
            .unwrap()
            .into_iter()
            .collect();
        let b: std::collections::HashSet<_> = fresh
            .columnar()
            .unwrap()
            .edge_topology_typed()
            .unwrap()
            .into_iter()
            .collect();
        eprintln!(
            "EDGES ONLY OLD {:?} ONLY NEW {:?}",
            a.difference(&b).collect::<Vec<_>>(),
            b.difference(&a).collect::<Vec<_>>()
        );
    }
    assert_eq!(
        old.content_digest().unwrap(),
        digest,
        "updated graph must equal fresh discover"
    );
}
#[test]
fn add_delete_and_higher_priority_targets_rebind_unchanged_referrers() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "rgctl.toml",
        "[languages.jinja]\ntemplate_roots=['templates']\n",
    );
    write(
        root,
        "templates/base.j2",
        "{% include ['priority.j2','fallback.j2'] %}\n",
    );
    write(root, "templates/fallback.j2", "{{ fallback }}\n");
    write(
        root,
        "deploy.yaml",
        "- hosts: all\n  tasks:\n    - template: {src: plain.conf, dest: /etc/plain}\n",
    );
    write(root, "templates/plain.conf", "{{ value }}\n");
    write(root, "src/main.rs", "fn main() {}\n");
    let mut graph = setup(root);
    let updater = IncrementalUpdater::new(Arc::new(rgctl_languages::default_registry()));
    write(root, "templates/priority.j2", "{{ priority }}\n");
    updater
        .update_files(&mut graph, root, &["templates/priority.j2".into()])
        .unwrap();
    assert_fresh(root);
    fs::remove_file(root.join("templates/priority.j2")).unwrap();
    updater
        .update_files(&mut graph, root, &["templates/priority.j2".into()])
        .unwrap();
    assert_fresh(root);
    fs::rename(
        root.join("templates/plain.conf"),
        root.join("templates/renamed.conf"),
    )
    .unwrap();
    updater
        .update_files(
            &mut graph,
            root,
            &[
                "templates/plain.conf".into(),
                "templates/renamed.conf".into(),
            ],
        )
        .unwrap();
    assert_fresh(root);
}
#[test]
fn role_scopes_replay_and_sidecars_recover_without_source_reparse() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "deploy.yml", "- hosts: all\n  roles: [web]\n");
    write(
        root,
        "roles/web/tasks/main.yml",
        "- name: Render\n  template: {src: config.j2, dest: /etc/config}\n  notify: reload\n",
    );
    write(root, "roles/web/templates/config.j2", "{{ value }}\n");
    write(
        root,
        "roles/web/handlers/main.yml",
        "- name: Restart\n  listen: reload\n  debug: {msg: ok}\n",
    );
    let mut graph = setup(root);
    let updater = IncrementalUpdater::new(Arc::new(rgctl_languages::default_registry()));
    for contents in [None, Some(b"corrupt".as_slice())] {
        let sidecar = root.join(".rgctl/template-dependencies.bin");
        if let Some(contents) = contents {
            fs::write(sidecar, contents).unwrap();
        } else {
            fs::remove_file(sidecar).unwrap();
        }
        write(
            root,
            "roles/web/handlers/main.yml",
            "- name: Restart\n  listen: reload\n  debug: {msg: updated}\n- name: Restart second\n  listen: reload\n  debug: {msg: ok}\n",
        );
        updater
            .update_files(&mut graph, root, &["roles/web/handlers/main.yml".into()])
            .unwrap();
        assert_fresh(root);
    }
}
#[test]
fn config_changes_and_source_reclassification_match_fresh_discover() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "deploy.yaml",
        "- hosts: all\n  tasks:\n    - debug: {msg: '{{ value }}'}\n",
    );
    write(root, "page.html", "<p>[[ value ]]</p>");
    let mut graph = setup(root);
    let updater = IncrementalUpdater::new(Arc::new(rgctl_languages::default_registry()));
    write(
        root,
        "rgctl.toml",
        "[languages.jinja]\ninclude=['page.html']\n[languages.jinja.syntax]\nvariable_start_string='[['\nvariable_end_string=']]'\n",
    );
    updater
        .update_files(&mut graph, root, &["rgctl.toml".into()])
        .unwrap();
    assert_fresh(root);
    write(root, "deploy.yaml", "value: ordinary config\n");
    updater
        .update_files(&mut graph, root, &["deploy.yaml".into()])
        .unwrap();
    assert_fresh(root);
    let filtered = IncrementalUpdater::with_options(
        Arc::new(rgctl_languages::default_registry()),
        UpdateOptions {
            discovery: rgctl_extraction::discovery::DiscoveryConfig {
                languages: Some(vec!["ansible".into()]),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    // Explicit config changes refresh even when the requested source bytes did not change.
    write(root, "rgctl.toml", "[languages.jinja]\ninclude=[]\n");
    filtered
        .update_files(&mut graph, root, &["rgctl.toml".into()])
        .unwrap();
}

#[test]
fn in_memory_updates_recover_exact_indexes_and_match_cold_discovery() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "roles/web/tasks/main.yml",
        "- template: {src: config.j2, dest: /etc/config}\n",
    );
    write(root, "roles/web/templates/config.j2", "{{ before }}\n");
    let (mut graph, stats) = pipeline().process_repository(root).unwrap();
    let files: Vec<_> = stats.file_hashes.keys().map(PathBuf::from).collect();
    let mut tracker = FileTracker::new(root);
    tracker
        .index_files_with_mapping(&files, stats.node_path_mapping, Some(&stats.file_hashes))
        .unwrap();
    tracker.save().unwrap();
    assert!(!root.join(".rgctl/graph.snapshot.bin").exists());
    write(root, "roles/web/templates/config.j2", "{{ after }}\n");
    IncrementalUpdater::new(Arc::new(rgctl_languages::default_registry()))
        .update_files(&mut graph, root, &["roles/web/templates/config.j2".into()])
        .unwrap();
    assert_fresh(root);
}

#[test]
fn automatic_update_tracks_referenced_unsuffixed_targets() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "deploy.yml",
        "- hosts: all\n  tasks:\n    - template: {src: config.conf, dest: /etc/config}\n",
    );
    let mut graph = setup(root);
    let updater = IncrementalUpdater::new(Arc::new(rgctl_languages::default_registry()));
    write(root, "templates/config.conf", "{{ added }}\n");
    updater.update(&mut graph, root).unwrap();
    assert_fresh(root);
    assert!(
        FileTracker::load(root)
            .unwrap()
            .file_hashes()
            .contains_key("templates/config.conf")
    );
    write(root, "templates/config.conf", "{{ modified }}\n");
    updater.update(&mut graph, root).unwrap();
    assert_fresh(root);
    fs::remove_file(root.join("templates/config.conf")).unwrap();
    updater.update(&mut graph, root).unwrap();
    assert_fresh(root);
}

#[test]
fn snapshot_synthesis_replays_missing_candidates_without_changing_live_artifacts() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "rgctl.toml",
        "[languages.jinja]\ntemplate_roots=['templates']\n",
    );
    write(root, "base.j2", "{% include 'missing.j2' %}\n");
    let _graph = setup(root);
    let live = root.join(".rgctl/graph.snapshot.bin");
    let bytes = fs::read(root.join(".rgctl/template-dependencies.bin")).unwrap();
    write(root, "templates/missing.j2", "{{ arrived }}\n");
    let work = TempDir::new().unwrap();
    let output = work.path().join("head.snapshot.bin");
    IncrementalUpdater::new(Arc::new(rgctl_languages::default_registry()))
        .apply_change_set_to_snapshot(
            &live,
            &output,
            work.path(),
            root,
            rgctl_incremental::ChangeSet {
                added: vec!["templates/missing.j2".into()],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        bytes,
        fs::read(root.join(".rgctl/template-dependencies.bin")).unwrap()
    );
    let actual = SnapshotNodeStore::open(&output).unwrap();
    rgctl_graph::invariants::assert_graph_invariants(actual.columnar().unwrap()).unwrap();
    let fresh = root.join(".rgctl/fresh.snapshot.bin");
    let (_, digest) = pipeline()
        .process_repository_to_snapshot(root, &fresh, None)
        .unwrap();
    assert_eq!(actual.content_digest().unwrap(), digest);
}

#[test]
fn loader_root_changes_without_source_edits_rebuild_exact_edges() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "rgctl.toml",
        "[languages.jinja]\ntemplate_roots=['a']\n",
    );
    write(root, "page.j2", "{% extends 'base.j2' %}\n");
    write(root, "a/base.j2", "{{ first }}\n");
    write(root, "b/base.j2", "{{ second }}\n");
    let mut graph = setup(root);
    write(
        root,
        "rgctl.toml",
        "[languages.jinja]\ntemplate_roots=['b']\n",
    );
    IncrementalUpdater::new(Arc::new(rgctl_languages::default_registry()))
        .update_files(&mut graph, root, &["rgctl.toml".into()])
        .unwrap();
    assert_fresh(root);
}

#[test]
fn role_meta_import_rebinding_and_stale_sidecar_recovery_match_fresh() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "rgctl.toml",
        "[languages.jinja]\ntemplate_roots=['templates']\n",
    );
    write(root, "deploy.yml", "- hosts: all\n  roles: [web]\n");
    write(root, "roles/web/tasks/main.yml", "- debug: {msg: ok}\n");
    write(root, "roles/db/tasks/main.yml", "- debug: {msg: db}\n");
    write(root, "roles/web/meta/main.yml", "dependencies: [db]\n");
    write(
        root,
        "templates/main.j2",
        "{% from 'helpers.j2' import label %}{{ label(value) }}\n",
    );
    write(
        root,
        "templates/helpers.j2",
        "{% macro label(value) %}{{ value }}{% endmacro %}\n",
    );
    write(
        root,
        "templates/other.j2",
        "{% macro label(value) %}{{ value | upper }}{% endmacro %}\n",
    );
    let mut graph = setup(root);
    let stale = fs::read(root.join(".rgctl/template-dependencies.bin")).unwrap();
    let updater = IncrementalUpdater::new(Arc::new(rgctl_languages::default_registry()));
    write(root, "roles/web/meta/main.yml", "dependencies: []\n");
    updater
        .update_files(&mut graph, root, &["roles/web/meta/main.yml".into()])
        .unwrap();
    assert_fresh(root);
    fs::write(root.join(".rgctl/template-dependencies.bin"), stale).unwrap();
    write(
        root,
        "templates/main.j2",
        "{% from 'other.j2' import label %}{{ label(value) }}\n",
    );
    updater
        .update_files(&mut graph, root, &["templates/main.j2".into()])
        .unwrap();
    assert_fresh(root);
}
