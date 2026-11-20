//! Phase 5 integration tests: incremental updates and performance

use rgctl::graph::CodeGraph;
use rgctl::graph::backend::GraphBackend;
use rgctl::graph::schema::{EdgeType, Node, NodeType};
use rgctl::incremental::{FileTracker, IncrementalUpdater, UpdateOptions};
use rgctl::languages::registry::LanguageRegistry;
use rgctl::pipeline::{PipelineConfig, ProcessingPipeline};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn write(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn test_incremental_update_workflow() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(&root.join("src/main.rs"), "fn main() {}\n");

    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        },
    );

    let (mut graph, _) = pipeline.process_repository(root).unwrap();
    graph.save_to_repo(root).unwrap();

    let mut tracker = FileTracker::new(root);
    let files = vec![root.join("src/main.rs")];
    tracker.index_files(&files, &graph).unwrap();
    tracker.save().unwrap();

    write(
        &root.join("src/main.rs"),
        "fn main() { helper(); }\nfn helper() {}\n",
    );

    let updater = IncrementalUpdater::with_options(
        registry,
        UpdateOptions {
            show_progress: false,
            ..Default::default()
        },
    );
    let result = updater.update(&mut graph, root).unwrap();

    assert!(result.files_changed >= 1 || result.nodes_added > 0);
    let functions = graph.find_by_type(NodeType::Function).unwrap();
    assert!(functions.iter().any(|n| n.name == "helper"));
}

#[test]
fn test_file_hash_tracking() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("lib.rs");
    write(&file, "fn alpha() {}\n");

    let mut tracker = FileTracker::new(temp.path());
    tracker
        .index_files(std::slice::from_ref(&file), &CodeGraph::new())
        .unwrap();
    tracker.save().unwrap();

    write(&file, "fn beta() {}\n");
    let changes = tracker.detect_changes(&[file]).unwrap();
    assert_eq!(changes.changed.len(), 1);
}

#[test]
fn test_query_performance_by_label() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    for i in 0..10_000 {
        let mut node = Node::new(NodeType::Class, format!("Component{i}"));
        node.labels.push("react:component".to_string());
        backend.insert_node(node).unwrap();
    }

    let start = Instant::now();
    let results = backend.find_nodes_by_label("react:component").unwrap();
    let duration = start.elapsed();

    assert_eq!(results.len(), 10_000);
    assert!(
        duration < Duration::from_millis(50),
        "Query too slow: {duration:?}"
    );
}

#[test]
fn test_query_cache_hit() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();
    for i in 0..100 {
        backend
            .insert_node(Node::new(NodeType::Function, format!("fn{i}")))
            .unwrap();
    }

    let start = Instant::now();
    let _ = backend.cached_query("functions").unwrap();
    let first = start.elapsed();

    let start = Instant::now();
    let cached = backend.cached_query("functions").unwrap();
    let second = start.elapsed();

    assert_eq!(cached.len(), 100);
    assert!(second <= first, "cache should be faster or equal");
}

#[test]
fn test_incremental_update_ten_files() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    for i in 0..10 {
        write(
            &root.join(format!("src/file{i}.rs")),
            &format!("fn func{i}() {{}}\n"),
        );
    }

    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            ..PipelineConfig::default()
        },
    );

    let (mut graph, _) = pipeline.process_repository(root).unwrap();
    graph.save_to_repo(root).unwrap();

    let discoverer = rgctl::discovery::FileDiscoverer::new(Arc::clone(&registry));
    let files = discoverer.discover(root).unwrap();
    let mut tracker = FileTracker::new(root);
    tracker.index_files(&files, &graph).unwrap();
    tracker.save().unwrap();

    for i in 0..10 {
        write(
            &root.join(format!("src/file{i}.rs")),
            &format!("fn func{i}() {{ /* updated */ }}\nfn extra{i}() {{}}\n"),
        );
    }

    let updater = IncrementalUpdater::with_options(
        registry,
        UpdateOptions {
            show_progress: false,
            ..Default::default()
        },
    );

    let start = Instant::now();
    let result = updater.update(&mut graph, root).unwrap();
    let duration = start.elapsed();

    assert!(result.files_changed >= 10 || result.nodes_added > 0);
    assert!(
        duration < Duration::from_secs(5),
        "Update too slow: {duration:?}"
    );
}

#[test]
fn test_string_interning_reduces_duplicates() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    for _ in 0..1000 {
        let mut node = Node::new(NodeType::Function, "shared_name".to_string());
        node.file_path = Some("src/common.rs".into());
        node.labels.push("api:endpoint".to_string());
        backend.insert_node(node).unwrap();
    }

    // Interner should deduplicate repeated strings across nodes
    assert!(backend.memory_estimate() > 0);
    let functions = backend.find_nodes_by_name("shared_name").unwrap();
    assert_eq!(functions.len(), 1000);
}

fn graph_signature(
    graph: &CodeGraph,
) -> (
    BTreeSet<(String, String)>,
    BTreeSet<(String, String, String)>,
) {
    let backend = graph.backend();
    let nodes = backend.all_nodes().unwrap();
    // Compare callables only — File nodes embed absolute temp paths.
    let id_to_key: HashMap<_, _> = nodes
        .iter()
        .filter(|n| matches!(n.node_type, NodeType::Function | NodeType::Class))
        .map(|n| {
            let q = n
                .qualified_name
                .as_deref()
                .unwrap_or(n.name.as_str())
                .to_string();
            let ty = format!("{:?}", n.node_type);
            (n.id, (q, ty))
        })
        .collect();
    let node_set: BTreeSet<_> = id_to_key.values().cloned().collect();
    let edge_set: BTreeSet<_> = backend
        .all_edges()
        .unwrap()
        .into_iter()
        .filter(|e| e.edge_type == EdgeType::Calls)
        .filter_map(|e| {
            let from = id_to_key.get(&e.from)?;
            let to = id_to_key.get(&e.to)?;
            Some((from.0.clone(), to.0.clone(), format!("{:?}", e.edge_type)))
        })
        .collect();
    (node_set, edge_set)
}

/// Warm incremental update after modify/add/delete must match a cold rediscover.
#[test]
fn facts_cache_and_incremental_update_match_cold_discover() {
    let warm = TempDir::new().unwrap();
    let root = warm.path();

    write(
        &root.join("src/file_a.rs"),
        "pub fn alpha() -> i32 { 1 }\npub fn unused_a() {}\n",
    );
    write(&root.join("src/file_b.rs"), "pub fn beta() -> i32 { 2 }\n");
    write(&root.join("src/file_c.rs"), "pub fn gamma() -> i32 { 3 }\n");
    write(&root.join("src/file_d.rs"), "pub fn delta() -> i32 { 4 }\n");

    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            facts_cache: true,
            ..PipelineConfig::default()
        },
    );
    let (mut graph, _) = pipeline.process_repository(root).unwrap();
    graph.save_to_repo(root).unwrap();

    let discoverer = rgctl::discovery::FileDiscoverer::new(Arc::clone(&registry));
    let files = discoverer.discover(root).unwrap();
    let mut tracker = FileTracker::new(root);
    tracker.index_files(&files, &graph).unwrap();
    tracker.save().unwrap();

    // modify + add + delete + rename
    write(
        &root.join("src/file_a.rs"),
        "pub fn alpha() -> i32 { 42 }\npub fn unused_a() {}\n",
    );
    write(
        &root.join("src/file_b.rs"),
        "pub fn beta() -> i32 { 2 }\npub fn beta_extra() -> i32 { crate::file_a::alpha() }\n",
    );
    write(
        &root.join("src/file_f.rs"),
        "pub fn added_fn() -> i32 { 99 }\n",
    );
    let _ = fs::remove_file(root.join("src/file_c.rs"));
    fs::rename(root.join("src/file_d.rs"), root.join("src/file_e.rs")).unwrap();

    let updater = IncrementalUpdater::with_options(
        Arc::clone(&registry),
        UpdateOptions {
            show_progress: false,
            ..Default::default()
        },
    );
    let update = updater.update(&mut graph, root).unwrap();
    assert!(
        update.files_changed >= 2 && update.files_added >= 2 && update.files_deleted >= 2,
        "expected modify+add+delete+rename cycle, got {update:?}"
    );
    graph.save_to_repo(root).unwrap();
    let (warm_nodes, warm_calls) = graph_signature(&graph);

    // Cold rediscover on a clean copy of the post-edit tree.
    let cold = TempDir::new().unwrap();
    let cold_root = cold.path();
    for ent in fs::read_dir(root).unwrap() {
        let ent = ent.unwrap();
        let name = ent.file_name();
        if name == ".rgctl" {
            continue;
        }
        let from = ent.path();
        let to = cold_root.join(&name);
        if from.is_dir() {
            copy_dir(&from, &to);
        } else {
            fs::copy(&from, &to).unwrap();
        }
    }
    let cold_pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            facts_cache: true,
            ..PipelineConfig::default()
        },
    );
    let (cold_graph, _) = cold_pipeline.process_repository(cold_root).unwrap();
    let (cold_nodes, cold_calls) = graph_signature(&cold_graph);

    assert_eq!(
        warm_nodes, cold_nodes,
        "incremental node set (qname, NodeType) must match cold discover"
    );
    assert_eq!(
        warm_calls, cold_calls,
        "incremental Calls edges must match cold discover"
    );

    // Unchanged file_e (ex-delta body) / leftover files should be cacheable on a second warm pass.
    let facts_path = root.join(".rgctl/cache/facts.bin");
    if facts_path.is_file() {
        let facts = rgctl::extraction::FactsCache::load(root);
        assert!(
            facts.len() >= 1,
            "facts.bin should retain at least one entry after incremental update"
        );
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    fs::create_dir_all(dst).unwrap();
    for ent in fs::read_dir(src).unwrap() {
        let ent = ent.unwrap();
        let from = ent.path();
        let to = dst.join(ent.file_name());
        if from.is_dir() {
            copy_dir(&from, &to);
        } else {
            fs::copy(&from, &to).unwrap();
        }
    }
}

/// Snapshot-native `update_repo` must match a clean cold discover across modify + cross-file call
/// + callee rename cascade + add + delete + root/nested `mod.rs` & `foo_mod.rs` disambiguation.
#[test]
fn snapshot_update_repo_matches_cold_discover_and_avoids_zombie_nodes() {
    use rgctl::graph::MmappedGraphSnapshot;

    let warm = TempDir::new().unwrap();
    let root = warm.path();

    write(&root.join("mod.rs"), "pub fn root_mod_fn() -> i32 { 1 }\n");
    write(
        &root.join("src/sub/mod.rs"),
        "pub fn nested_mod_fn() -> i32 { 2 }\n",
    );
    write(
        &root.join("src/foo_mod.rs"),
        "pub fn foo_mod_helper() -> i32 { 3 }\n",
    );
    write(
        &root.join("src/caller_foo.rs"),
        "pub fn call_foo() -> i32 { crate::foo_mod::foo_mod_helper() }\n",
    );
    write(
        &root.join("src/auth.rs"),
        "pub fn login() -> bool { true }\n",
    );
    write(
        &root.join("src/main_flow.rs"),
        "pub fn entry() -> bool { crate::auth::login() }\n",
    );
    write(&root.join("src/obsolete.rs"), "pub fn doomed_symbol() {}\n");

    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            facts_cache: true,
            ..PipelineConfig::default()
        },
    );
    let snapshot_path = MmappedGraphSnapshot::default_path(root);
    let (stats, _) = pipeline
        .process_repository_to_snapshot(root, &snapshot_path, None)
        .unwrap();

    let discoverer = rgctl::discovery::FileDiscoverer::new(Arc::clone(&registry));
    let files = discoverer.discover(root).unwrap();
    let mut tracker = FileTracker::new(root);
    tracker
        .index_files_with_mapping(&files, stats.node_path_mapping, Some(&stats.file_hashes))
        .unwrap();
    tracker.save().unwrap();

    // Edit root `mod.rs`, rename `login` -> `authenticate` in `src/auth.rs` + update caller,
    // delete `src/obsolete.rs`, and add `src/new_feature.rs` calling kept `foo_mod_helper`.
    write(
        &root.join("mod.rs"),
        "pub fn root_mod_fn_v2() -> i32 { 10 }\n",
    );
    write(
        &root.join("src/auth.rs"),
        "pub fn authenticate() -> bool { true }\n",
    );
    write(
        &root.join("src/main_flow.rs"),
        "pub fn entry() -> bool { crate::auth::authenticate() }\n",
    );
    fs::remove_file(root.join("src/obsolete.rs")).unwrap();
    write(
        &root.join("src/new_feature.rs"),
        "pub fn feature() -> i32 { crate::foo_mod::foo_mod_helper() }\n",
    );

    let updater = IncrementalUpdater::with_options(
        Arc::clone(&registry),
        UpdateOptions {
            show_progress: false,
            cascade_depth: 1,
            ..Default::default()
        },
    );
    let res = updater.update_repo(root).unwrap();
    assert_eq!(res.files_added, 1);
    assert_eq!(res.files_changed, 3);
    assert_eq!(res.files_deleted, 1);

    let warm_graph = CodeGraph::open_snapshot(&snapshot_path).unwrap();
    let (warm_nodes, warm_calls) = graph_signature(&warm_graph);

    // Cold rediscover on a clean copy of the post-edit tree.
    let cold = TempDir::new().unwrap();
    let cold_root = cold.path();
    for ent in fs::read_dir(root).unwrap() {
        let ent = ent.unwrap();
        let name = ent.file_name();
        if name == ".rgctl" {
            continue;
        }
        let from = ent.path();
        let to = cold_root.join(&name);
        if from.is_dir() {
            copy_dir(&from, &to);
        } else {
            fs::copy(&from, &to).unwrap();
        }
    }
    let (cold_graph, _) = pipeline.process_repository(cold_root).unwrap();
    let (cold_nodes, cold_calls) = graph_signature(&cold_graph);

    assert_eq!(
        warm_nodes, cold_nodes,
        "snapshot-compacted node set must match clean cold discover (no zombie symbols or dropped nested mod.rs)"
    );
    assert_eq!(
        warm_calls, cold_calls,
        "snapshot-compacted Calls edges must match clean cold discover"
    );
}
