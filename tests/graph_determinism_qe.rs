//! N-way file-order / thread-count determinism for graph extraction.

use rgctl_extraction::{DiscoveryConfig, Extractor, FileDiscoverer, GraphBuilder};
use rgctl_graph::schema::EdgeType;
use rgctl_graph::{
    SNAPSHOT_FILE, SnapshotNodeStore, assert_graph_invariants, edge_type_to_u8,
    write_columnar_from_nodes_edges,
};
use rgctl_pipeline::stream::{DEFAULT_STREAM_CHANNEL_CAPACITY, stream_into_graph};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() == ".rgctl" {
                continue;
            }
            copy_dir_all(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn discover_files(root: &Path) -> Vec<PathBuf> {
    let registry = Arc::new(
        rgctl_languages::default_registry()
            .for_repository(root)
            .expect("routing"),
    );
    let discoverer = FileDiscoverer::with_config(
        registry,
        DiscoveryConfig {
            exclude_patterns: vec![
                ".rgctl/**".into(),
                "target/**".into(),
                "node_modules/**".into(),
            ],
            ..DiscoveryConfig::default()
        },
    );
    discoverer.discover(root).expect("discover files")
}

fn shuffle_with_seed(files: &mut [PathBuf], seed: u64) {
    // Deterministic LCG Fisher–Yates (no rand crate dependency).
    let mut state = seed;
    for i in (1..files.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (state as usize) % (i + 1);
        files.swap(i, j);
    }
}

struct SnapSummary {
    digest: String,
    node_count: usize,
    edge_counts: HashMap<u8, usize>,
}

fn extract_summary(root: &Path, files: &[PathBuf], thread_count: Option<usize>) -> SnapSummary {
    let registry = Arc::new(
        rgctl_languages::default_registry()
            .for_repository(root)
            .expect("routing"),
    );
    let extractor = Extractor::new(Arc::clone(&registry));
    let mut builder = GraphBuilder::new();
    let (_stats, tails) = stream_into_graph(
        thread_count,
        &extractor,
        registry,
        files,
        DEFAULT_STREAM_CHANNEL_CAPACITY,
        &mut builder,
        || {},
    )
    .expect("stream_into_graph");
    builder.build_resolution_indexes();
    extractor
        .populate_pass2(&tails, &mut builder)
        .expect("populate_pass2");
    let (mut nodes, mut edges) = builder.into_graph();
    nodes.sort_by_key(|n| n.id);
    nodes.dedup_by_key(|n| n.id);
    edges.sort_by(|a, b| {
        (a.from, a.to, edge_type_to_u8(a.edge_type)).cmp(&(
            b.from,
            b.to,
            edge_type_to_u8(b.edge_type),
        ))
    });
    edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.edge_type == b.edge_type);
    let node_count = nodes.len();
    let mut edge_counts: HashMap<u8, usize> = HashMap::new();
    for e in &edges {
        *edge_counts.entry(edge_type_to_u8(e.edge_type)).or_default() += 1;
    }
    let out = root.join(".rgctl");
    fs::create_dir_all(&out).unwrap();
    let snap = out.join(SNAPSHOT_FILE);
    let digest = write_columnar_from_nodes_edges(nodes, edges, &snap).expect("write columnar");
    let store = SnapshotNodeStore::open(&snap).expect("open snapshot");
    let col = store.mmap().columnar().expect("columnar v2");
    assert_graph_invariants(col).expect("invariants");
    SnapSummary {
        digest,
        node_count,
        edge_counts,
    }
}

fn assert_same(a: &SnapSummary, b: &SnapSummary, label: &str, require_digest: bool) {
    assert_eq!(a.node_count, b.node_count, "{label}: node_count");
    assert_eq!(a.edge_counts, b.edge_counts, "{label}: per-EdgeType counts");
    let calls = edge_type_to_u8(EdgeType::Calls);
    assert_eq!(
        a.edge_counts.get(&calls).copied().unwrap_or(0),
        b.edge_counts.get(&calls).copied().unwrap_or(0),
        "{label}: Calls count"
    );
    if require_digest {
        assert_eq!(a.digest, b.digest, "{label}: content_digest");
    }
}

fn run_permutation_suite(fixture: &Path, require_digest: bool) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    copy_dir_all(fixture, &root);
    let mut files = discover_files(&root);
    assert!(files.len() >= 2, "fixture needs ≥2 source files");
    files.sort();

    let forward = extract_summary(&root, &files, Some(1));

    let mut reversed = files.clone();
    reversed.reverse();
    let reverse = extract_summary(&root, &reversed, Some(1));
    assert_same(&forward, &reverse, "thread=1 reverse", require_digest);

    for seed in [1u64, 42, 99, 777] {
        let mut shuffled = files.clone();
        shuffle_with_seed(&mut shuffled, seed);
        let summary = extract_summary(&root, &shuffled, Some(8));
        assert_same(
            &forward,
            &summary,
            &format!("thread=8 seed={seed}"),
            require_digest,
        );
    }
}

fn ecommerce_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("rgctl-tests/{name}"))
}

macro_rules! ecommerce_perm_test {
    ($fn_name:ident, $dir:expr) => {
        #[test]
        fn $fn_name() {
            let fixture = ecommerce_fixture($dir);
            if !fixture.is_dir() {
                eprintln!(
                    "skip {}: missing {}",
                    stringify!($fn_name),
                    fixture.display()
                );
                return;
            }
            run_permutation_suite(&fixture, true);
        }
    };
}

ecommerce_perm_test!(
    ecommerce_java_file_order_and_thread_determinism,
    "ecommerce-java"
);
ecommerce_perm_test!(
    ecommerce_python_file_order_and_thread_determinism,
    "ecommerce-python"
);
ecommerce_perm_test!(
    ecommerce_go_file_order_and_thread_determinism,
    "ecommerce-go"
);
ecommerce_perm_test!(
    ecommerce_cpp_file_order_and_thread_determinism,
    "ecommerce-cpp"
);
ecommerce_perm_test!(
    ecommerce_ruby_file_order_and_thread_determinism,
    "ecommerce-ruby"
);
ecommerce_perm_test!(
    ecommerce_rust_file_order_and_thread_determinism,
    "ecommerce-rust"
);
ecommerce_perm_test!(
    ecommerce_typescript_file_order_and_thread_determinism,
    "ecommerce-typescript"
);

#[test]
fn collision_repo_file_order_and_thread_determinism() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collision_repo");
    run_permutation_suite(&fixture, true);
}

/// Same unqualified name in two modules + unhinted call must not flip-flop by worker order.
#[test]
fn duplicate_fqn_never_fabricates_random_winner() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(root.join("mod_a")).unwrap();
    fs::create_dir_all(root.join("mod_b")).unwrap();
    fs::write(root.join("mod_a/dup.rs"), "pub fn shared() {}\n").unwrap();
    fs::write(root.join("mod_b/dup.rs"), "pub fn shared() {}\n").unwrap();
    fs::write(root.join("caller.rs"), "pub fn entry() { shared(); }\n").unwrap();

    let mut files = discover_files(&root);
    files.sort();
    assert_eq!(files.len(), 3);

    let call_targets = |files: &[PathBuf], threads: Option<usize>| -> BTreeSet<String> {
        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(Arc::clone(&registry));
        let mut builder = GraphBuilder::new();
        let (_stats, tails) = stream_into_graph(
            threads,
            &extractor,
            registry,
            files,
            DEFAULT_STREAM_CHANNEL_CAPACITY,
            &mut builder,
            || {},
        )
        .expect("stream_into_graph");
        builder.build_resolution_indexes();
        extractor
            .populate_pass2(&tails, &mut builder)
            .expect("populate_pass2");
        let (nodes, edges) = builder.into_graph();
        let id_to_qn: HashMap<_, String> = nodes
            .iter()
            .map(|n| {
                let qn = n
                    .qualified_name
                    .as_ref()
                    .map(|q| q.to_string())
                    .unwrap_or_else(|| n.name.to_string());
                (n.id, qn)
            })
            .collect();
        let entry_ids: BTreeSet<_> = nodes
            .iter()
            .filter(|n| n.name.as_str() == "entry")
            .map(|n| n.id)
            .collect();
        edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls && entry_ids.contains(&e.from))
            .filter_map(|e| id_to_qn.get(&e.to).cloned())
            .collect()
    };

    let baseline = call_targets(&files, Some(1));
    let mut reversed = files.clone();
    reversed.reverse();
    assert_eq!(
        call_targets(&reversed, Some(1)),
        baseline,
        "reverse file order must not change Calls targets (or absence)"
    );

    for (i, seed) in [1u64, 42, 99, 777, 12345].into_iter().enumerate() {
        let mut shuffled = files.clone();
        shuffle_with_seed(&mut shuffled, seed);
        let threads = if i % 2 == 0 { Some(1) } else { Some(8) };
        assert_eq!(
            call_targets(&shuffled, threads),
            baseline,
            "perm i={i} seed={seed} threads={threads:?} must match baseline {baseline:?}"
        );
    }
}

ecommerce_perm_test!(
    ecommerce_ansible_file_order_and_thread_determinism,
    "ecommerce-ansible"
);
ecommerce_perm_test!(
    ecommerce_jinja_file_order_and_thread_determinism,
    "ecommerce-jinja"
);
