//! In-process 6-law metamorphic correctness gate for all 24 Tier-1 languages.
//!
//! No subprocesses. One shared [`LanguageRegistry`].

use rgctl_analysis::{
    BackwardSlicer, DominatorTree, EnsureCfgPdgOpts, ProgramDependenceGraph, SliceCriterion,
    build_cfg_for_function, ensure_cfg_pdg_for_source, profile_for_language,
};
use rgctl_extraction::{DiscoveryConfig, Extractor, FileDiscoverer, GraphBuilder};
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::stable_key::StableNodeKey;
use rgctl_graph::{
    CodeGraph, MmappedGraphSnapshot, NoopDiffSink, SnapshotNodeStore, assert_graph_invariants,
    diff_snapshots, stable_key_from_node, write_columnar_from_nodes_edges,
};
use rgctl_incremental::{FileTracker, IncrementalUpdater};
use rgctl_pipeline::stream::{DEFAULT_STREAM_CHANNEL_CAPACITY, stream_into_graph};
use rgctl_pipeline::{PipelineConfig, ProcessingPipeline};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;

struct Fix {
    id: &'static str,
    rel: &'static str,
    source: &'static str,
    /// Callable whose bytes sit entirely before [`Self::sibling`].
    target: &'static str,
    sibling: &'static str,
}

const MARKER: &str = "RGCTL_BODY";
/// Same length as [`MARKER`] so a body edit does not shift later byte spans.
const EDITED: &str = "RGCTL_EDIT";

/// Two sibling callables, UTF-8 trivia, and `RGCTL_BODY` only inside the later sibling.
const FIXTURES: &[Fix] = &[
    Fix {
        id: "rust",
        rel: "rust/lib.rs",
        source: "// café\npub fn target_fn() -> i32 {\n    sibling_fn()\n}\npub fn sibling_fn() -> i32 {\n    /* RGCTL_BODY */\n    2\n}\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "python",
        rel: "python/mod.py",
        source: "# café\ndef target_fn():\n    return sibling_fn()\n\ndef sibling_fn():\n    # RGCTL_BODY\n    return 2\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "typescript",
        rel: "typescript/mod.ts",
        source: "// café\nexport class Box {\n  status: number = 0;\n  status(): number { return target_fn(); }\n}\nexport function target_fn(): number {\n  return sibling_fn();\n}\nexport function sibling_fn(): number {\n  /* RGCTL_BODY */\n  return 2;\n}\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "javascript",
        rel: "javascript/mod.js",
        source: "// café\nexport function target_fn() {\n  return sibling_fn();\n}\nexport function sibling_fn() {\n  /* RGCTL_BODY */\n  return 2;\n}\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "go",
        rel: "go/mod.go",
        source: "package main\n\n// café\nfunc target_fn() int {\n\treturn sibling_fn()\n}\n\nfunc sibling_fn() int {\n\t// RGCTL_BODY\n\treturn 2\n}\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "java",
        rel: "java/Mod.java",
        source: "public class Mod {\n  // café\n  public int status;\n  public static int target_fn() { return sibling_fn(); }\n  public int status() { return target_fn(); }\n  public static int sibling_fn() { /* RGCTL_BODY */ return 2; }\n}\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "csharp",
        rel: "csharp/Mod.cs",
        source: "public class Mod {\n  // café\n  public int Status;\n  public static int TargetFn() { return SiblingFn(); }\n  public int Status() { return TargetFn(); }\n  public static int SiblingFn() { /* RGCTL_BODY */ return 2; }\n}\n",
        target: "TargetFn",
        sibling: "SiblingFn",
    },
    Fix {
        id: "cpp",
        rel: "cpp/mod.cpp",
        source: "// café\nclass Holder {\npublic:\n  int status;\n  int status_fn() { return status; }\n};\nint target_fn() { return sibling_fn(); }\nint sibling_fn() { /* RGCTL_BODY */ return 2; }\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "c",
        rel: "c/mod.c",
        source: "/* café */\nint target_fn(void) { return sibling_fn(); }\nint sibling_fn(void) { /* RGCTL_BODY */ return 2; }\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "php",
        rel: "php/mod.php",
        source: "<?php\n// café\nfunction target_fn() { return sibling_fn(); }\nfunction sibling_fn() { /* RGCTL_BODY */ return 2; }\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "ruby",
        rel: "ruby/mod.rb",
        source: "# café\ndef target_fn\n  sibling_fn\nend\ndef sibling_fn\n  # RGCTL_BODY\n  2\nend\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "puppet",
        rel: "puppet/mod.pp",
        source: "# café\nfunction target_fn() {\n  1\n}\nfunction sibling_fn() {\n  # RGCTL_BODY\n  2\n}\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "kotlin",
        rel: "kotlin/Mod.kt",
        source: "// café\nclass Mod {\n  var status: Int = 0\n  fun status(): Int = target_fn()\n}\nfun target_fn(): Int = sibling_fn()\nfun sibling_fn(): Int {\n  // RGCTL_BODY\n  return 2\n}\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "groovy",
        rel: "groovy/Mod.groovy",
        source: "// café\ndef target_fn() { return sibling_fn() }\ndef sibling_fn() { /* RGCTL_BODY */ return 2 }\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "swift",
        rel: "swift/Mod.swift",
        source: "// café\nfunc target_fn() -> Int { return sibling_fn() }\nfunc sibling_fn() -> Int { /* RGCTL_BODY */ return 2 }\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "scala",
        rel: "scala/Mod.scala",
        source: "// café\nclass Mod {\n  var status: Int = 0\n  def status(): Int = target_fn()\n  def target_fn(): Int = sibling_fn()\n  def sibling_fn(): Int = {\n    // RGCTL_BODY\n    2\n  }\n}\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "dart",
        rel: "dart/mod.dart",
        source: "// café\nint target_fn() { return sibling_fn(); }\nint sibling_fn() { /* RGCTL_BODY */ return 2; }\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "elixir",
        rel: "elixir/mod.ex",
        source: "# café\ndefmodule Mod do\n  def target_fn do\n    sibling_fn()\n  end\n  def sibling_fn do\n    # RGCTL_BODY\n    2\n  end\nend\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "lua",
        rel: "lua/mod.lua",
        source: "-- café\nfunction target_fn()\n  return sibling_fn()\nend\nfunction sibling_fn()\n  -- RGCTL_BODY\n  return 2\nend\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "bash",
        rel: "bash/mod.sh",
        source: "# café\ntarget_fn() { sibling_fn; }\nsibling_fn() { echo RGCTL_BODY; }\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "sql",
        rel: "sql/mod.sql",
        source: "-- café\nCREATE FUNCTION target_fn() RETURNS INT LANGUAGE sql AS $$ SELECT 1; $$;\nCREATE FUNCTION sibling_fn() RETURNS INT LANGUAGE sql AS $$ SELECT 2; /* RGCTL_BODY */ $$;\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "hcl",
        rel: "hcl/mod.tf",
        source: "# café\nresource \"null_resource\" \"target_fn\" {\n}\nresource \"null_resource\" \"sibling_fn\" {\n  # RGCTL_BODY\n}\n",
        target: "target_fn",
        sibling: "sibling_fn",
    },
    Fix {
        id: "erb",
        rel: "erb/mod.html.erb",
        source: "<p><%= target_fn %></p>\n<% # RGCTL_BODY %>\n<%= sibling_fn %>\n<!-- café -->\n",
        target: "erb:output#1",
        sibling: "erb:code#1",
    },
    Fix {
        id: "markdown",
        rel: "markdown/doc.md",
        source: "# Target Fn\n\nCafé paragraph.\n\n# Sibling Fn\n\nRGCTL_BODY\n",
        target: "Target Fn",
        sibling: "Sibling Fn",
    },
];

fn shared_registry() -> Arc<rgctl_registry::LanguageRegistry> {
    static REGISTRY: std::sync::OnceLock<Arc<rgctl_registry::LanguageRegistry>> =
        std::sync::OnceLock::new();
    Arc::clone(REGISTRY.get_or_init(|| Arc::new(rgctl_languages::default_registry())))
}

fn write_fixtures(root: &Path) {
    for fix in FIXTURES {
        let path = root.join(fix.rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, fix.source).unwrap();
    }
}

fn discover_sorted(root: &Path) -> Vec<PathBuf> {
    let registry = shared_registry();
    let files = FileDiscoverer::with_config(registry, DiscoveryConfig::default())
        .discover(root)
        .expect("discover");
    let mut files: Vec<_> = files
        .into_iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e != "bin")
        })
        .collect();
    files.sort();
    assert!(
        files.len() >= FIXTURES.len(),
        "expected at least {} sources, got {}",
        FIXTURES.len(),
        files.len()
    );
    files
}

fn extract(
    files: &[PathBuf],
    threads: Option<usize>,
    materialize_fields: bool,
) -> (Vec<Node>, Vec<Edge>) {
    let registry = shared_registry();
    let extractor = Extractor::new(Arc::clone(&registry));
    let mut builder = GraphBuilder::new();
    builder.set_materialize_fields(materialize_fields);
    let (_stats, tails, _lossless) = stream_into_graph(
        threads,
        &extractor,
        registry,
        files,
        DEFAULT_STREAM_CHANNEL_CAPACITY,
        &mut builder,
        || {},
    )
    .expect("extract");
    builder.build_resolution_indexes();
    extractor
        .populate_pass2(&tails, &mut builder)
        .expect("pass2");
    builder.into_graph()
}

fn function_keys(nodes: &[Node]) -> BTreeSet<StableNodeKey> {
    nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Function)
        .map(stable_key_from_node)
        .collect()
}

fn call_pairs(nodes: &[Node], edges: &[Edge]) -> BTreeSet<(StableNodeKey, StableNodeKey)> {
    let by_id: HashMap<_, _> = nodes
        .iter()
        .map(|n| (n.id, stable_key_from_node(n)))
        .collect();
    edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Calls)
        .filter_map(|e| Some((*by_id.get(&e.from)?, *by_id.get(&e.to)?)))
        .collect()
}

fn name_matches(node: &Node, name: &str) -> bool {
    let n = node.name.as_str();
    n == name
        || n.ends_with(&format!(".{name}"))
        || n.ends_with(&format!("::{name}"))
        || n.ends_with(&format!("#{name}"))
}

fn find_named<'a>(nodes: &'a [Node], rel: &str, name: &str) -> Option<&'a Node> {
    nodes.iter().find(|n| {
        n.file_path
            .as_deref()
            .is_some_and(|p| p.replace('\\', "/").ends_with(rel))
            && name_matches(n, name)
    })
}

fn write_snap(nodes: Vec<Node>, edges: Vec<Edge>, path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    write_columnar_from_nodes_edges(nodes, edges, path).expect("write snapshot");
    let store = SnapshotNodeStore::open(path).unwrap();
    assert_graph_invariants(store.columnar().expect("columnar")).expect("invariants");
}

fn open_store(path: &Path) -> SnapshotNodeStore {
    SnapshotNodeStore::open(path).unwrap()
}

fn extract_with_cfg_pipeline(root: &Path, snap: &Path, art: &Path) -> (Vec<Node>, Vec<Edge>) {
    fs::create_dir_all(art).unwrap();
    let pipeline = ProcessingPipeline::with_config(
        shared_registry(),
        PipelineConfig {
            show_progress: false,
            thread_count: Some(1),
            facts_cache: false,
            materialize_fields: true,
            ..PipelineConfig::default()
        },
    );
    pipeline
        .process_repository_to_snapshot(root, snap, Some(art))
        .expect("with-cfg pipeline snapshot");
    let store = SnapshotNodeStore::open(snap).expect("open with-cfg store");
    assert_graph_invariants(store.columnar().expect("columnar")).expect("with-cfg invariants");
    let mut nodes = Vec::new();
    store
        .for_each_node_seq(|n| nodes.push(n.clone()))
        .expect("read nodes");
    let edges = store
        .edge_topology_typed()
        .expect("read edges")
        .into_iter()
        .map(|(from, to, edge_type)| Edge::new(from, to, edge_type))
        .collect();
    // Exercise eager CPG persistence into the `--with-cfg` artifact dir alongside the snapshot.
    for fix in FIXTURES {
        if profile_for_language(fix.id).is_some_and(|p| p.cfg_enabled) {
            let cfg_name = if fix.id == "hcl" {
                "resource.null_resource.target_fn"
            } else {
                fix.target
            };
            let src = fs::read_to_string(root.join(fix.rel)).unwrap();
            ensure_cfg_pdg_for_source(EnsureCfgPdgOpts {
                repo_root: art,
                language: fix.id,
                source: &src,
                function_name: cfg_name,
                file_path: Some(fix.rel),
                start_byte: None,
                end_byte: None,
                function_id: None,
                persist: true,
            })
            .unwrap_or_else(|e| panic!("Law 1 --with-cfg CPG build failed for {}: {e}", fix.id));
        }
    }
    (nodes, edges)
}

fn apply_same_name_overloads(root: &Path) -> usize {
    let edits: &[(&str, &str, &str)] = &[
        (
            "java/Mod.java",
            "public static int target_fn() { return sibling_fn(); }",
            "public static int target_fn() { return sibling_fn(); }\n  public static int target_fn(int x) { return x + 3; }",
        ),
        (
            "csharp/Mod.cs",
            "public static int TargetFn() { return SiblingFn(); }",
            "public static int TargetFn() { return SiblingFn(); }\n  public static int TargetFn(int x) { return x + 3; }",
        ),
        (
            "kotlin/Mod.kt",
            "fun target_fn(): Int = sibling_fn()",
            "fun target_fn(): Int = sibling_fn()\nfun target_fn(x: Int): Int = x + 3",
        ),
        (
            "groovy/Mod.groovy",
            "def target_fn() { return sibling_fn() }",
            "def target_fn() { return sibling_fn() }\ndef target_fn(int x) { return x + 3 }",
        ),
        (
            "scala/Mod.scala",
            "def target_fn(): Int = sibling_fn()",
            "def target_fn(): Int = sibling_fn()\n  def target_fn(x: Int): Int = x + 3",
        ),
        (
            "cpp/mod.cpp",
            "int target_fn() { return sibling_fn(); }",
            "int target_fn() { return sibling_fn(); }\nint target_fn(int x) { return x + 3; }",
        ),
        (
            "swift/Mod.swift",
            "func target_fn() -> Int { return sibling_fn() }",
            "func target_fn() -> Int { return sibling_fn() }\nfunc target_fn(x: Int) -> Int { return x + 3 }",
        ),
    ];
    for (rel, needle, replacement) in edits {
        let path = root.join(rel);
        let src = fs::read_to_string(&path).unwrap();
        assert!(src.contains(needle), "missing overload anchor in {rel}");
        fs::write(&path, src.replacen(needle, replacement, 1)).unwrap();
    }
    edits.len()
}

fn law1_mode_monotonicity(
    root: &Path,
    files: &[PathBuf],
    default_nodes: &[Node],
    default_edges: &[Edge],
) {
    let (field_nodes, field_edges) = extract(files, Some(1), true);
    let cfg_tmp = TempDir::new().unwrap();
    let cfg_snap = cfg_tmp.path().join("cfg.bin");
    let cfg_art = cfg_tmp.path().join("art");
    let (cfg_nodes, cfg_edges) = extract_with_cfg_pipeline(root, &cfg_snap, &cfg_art);
    let default_fns = function_keys(default_nodes);
    let field_fns = function_keys(&field_nodes);
    let cfg_fns = function_keys(&cfg_nodes);
    let describe = |nodes: &[Node], keys: &BTreeSet<StableNodeKey>| -> Vec<String> {
        nodes
            .iter()
            .filter(|n| {
                n.node_type == NodeType::Function && keys.contains(&stable_key_from_node(n))
            })
            .map(|n| {
                format!(
                    "{} {} qn={}",
                    n.file_path.as_deref().unwrap_or(""),
                    n.name.as_str(),
                    n.qualified_name.as_deref().unwrap_or("")
                )
            })
            .collect()
    };
    let only_default: BTreeSet<_> = default_fns.difference(&field_fns).copied().collect();
    let only_fields: BTreeSet<_> = field_fns.difference(&default_fns).copied().collect();
    assert!(
        only_default.is_empty() && only_fields.is_empty(),
        "Law 1: function keys differ between default and --with-fields\nonly default: {:?}\nonly fields: {:?}",
        describe(default_nodes, &only_default),
        describe(&field_nodes, &only_fields)
    );
    let only_default_cfg: BTreeSet<_> = default_fns.difference(&cfg_fns).copied().collect();
    let only_cfg: BTreeSet<_> = cfg_fns.difference(&default_fns).copied().collect();
    assert!(
        only_default_cfg.is_empty() && only_cfg.is_empty(),
        "Law 1: function keys differ between default and --with-cfg\nonly default: {:?}\nonly cfg: {:?}",
        describe(default_nodes, &only_default_cfg),
        describe(&cfg_nodes, &only_cfg)
    );
    assert_eq!(
        call_pairs(default_nodes, default_edges),
        call_pairs(&field_nodes, &field_edges),
        "Law 1: Calls edges differ between default and --with-fields"
    );
    assert_eq!(
        call_pairs(default_nodes, default_edges),
        call_pairs(&cfg_nodes, &cfg_edges),
        "Law 1: Calls edges differ between default and --with-cfg"
    );

    for (rel, member_name) in [
        ("java/Mod.java", "status"),
        ("csharp/Mod.cs", "Status"),
        ("typescript/mod.ts", "status"),
        ("kotlin/Mod.kt", "status"),
        ("scala/Mod.scala", "status"),
    ] {
        let has_fn = field_nodes.iter().any(|n| {
            n.node_type == NodeType::Function
                && n.file_path
                    .as_deref()
                    .is_some_and(|p| p.replace('\\', "/").ends_with(rel))
                && name_matches(n, member_name)
        });
        let has_field = field_nodes.iter().any(|n| {
            n.node_type == NodeType::Variable
                && n.file_path
                    .as_deref()
                    .is_some_and(|p| p.replace('\\', "/").ends_with(rel))
                && name_matches(n, member_name)
        });
        assert!(
            has_fn && has_field,
            "Law 1: {rel} field `{member_name}` (has_field={has_field}) and method `{member_name}()` (has_fn={has_fn}) must both survive --with-fields"
        );
    }

    for fix in FIXTURES {
        let target_node = find_named(default_nodes, fix.rel, fix.target)
            .unwrap_or_else(|| panic!("Law 1: {} missing target symbol {}", fix.id, fix.target));
        let sibling_node = find_named(default_nodes, fix.rel, fix.sibling)
            .unwrap_or_else(|| panic!("Law 1: {} missing sibling symbol {}", fix.id, fix.sibling));
        assert_ne!(
            stable_key_from_node(target_node),
            stable_key_from_node(sibling_node),
            "Law 1: {} {} and {} share a StableNodeKey",
            fix.id,
            fix.target,
            fix.sibling
        );
        if !matches!(fix.id, "hcl" | "erb" | "markdown") {
            let a = target_node
                .code_hash
                .as_deref()
                .unwrap_or_else(|| panic!("Law 1: {} missing code_hash on {}", fix.id, fix.target));
            let b = sibling_node.code_hash.as_deref().unwrap_or_else(|| {
                panic!("Law 1: {} missing code_hash on {}", fix.id, fix.sibling)
            });
            assert_ne!(
                a, b,
                "Law 1: {} {} and {} share a code_hash",
                fix.id, fix.target, fix.sibling
            );
        }
    }
}

fn law2_and_law5_body(v1: &[Node], v2: &[Node], v1_snap: &Path, v2_snap: &Path) {
    let mut hash_errors = Vec::new();
    for fix in FIXTURES {
        let marker = fix.source.find(MARKER).expect("marker") as u32;
        let before = find_named(v1, fix.rel, fix.target)
            .unwrap_or_else(|| panic!("Law 2: {} missing target {} in v1", fix.id, fix.target));
        let after = find_named(v2, fix.rel, fix.target)
            .unwrap_or_else(|| panic!("Law 2: {} missing target {} in v2", fix.id, fix.target));
        let end = before
            .end_byte
            .unwrap_or_else(|| panic!("Law 2: {} {} missing end_byte", fix.id, fix.target));
        assert!(
            end <= marker,
            "Law 2: {} {} end_byte ({end}) bled past sibling marker ({marker})",
            fix.id,
            fix.target
        );
        assert_eq!(
            stable_key_from_node(before),
            stable_key_from_node(after),
            "Law 2: {} {} stable key changed",
            fix.id,
            fix.target
        );
        assert_eq!(before.id, after.id, "Law 2: {} node id changed", fix.id);
        assert_eq!(
            (before.start_byte, before.end_byte),
            (after.start_byte, after.end_byte),
            "Law 2: {} {} span changed",
            fix.id,
            fix.target
        );
        assert_eq!(
            before.code_hash.as_deref(),
            after.code_hash.as_deref(),
            "Law 2: {} {} code_hash changed",
            fix.id,
            fix.target
        );

        if !matches!(fix.id, "hcl" | "erb" | "markdown") {
            let before_sib = find_covering_function(v1, fix.rel, marker).unwrap_or_else(|| {
                panic!(
                    "Law 2: {} no Function span covers sibling marker {marker}",
                    fix.id
                )
            });
            let after_sib = find_covering_function(v2, fix.rel, marker).unwrap_or_else(|| {
                panic!(
                    "Law 2: {} no Function span covers sibling marker {marker} in v2",
                    fix.id
                )
            });
            if before_sib.code_hash.is_none()
                || before_sib.code_hash.as_deref() == after_sib.code_hash.as_deref()
            {
                hash_errors.push(format!(
                    "{} {} span={:?}..{:?} hash={:?}",
                    fix.id,
                    before_sib.name.as_str(),
                    before_sib.start_byte,
                    before_sib.end_byte,
                    before_sib.code_hash
                ));
            }
        }
    }
    assert!(
        hash_errors.is_empty(),
        "Law 2: body edit did not change sibling code_hash:\n{}",
        hash_errors.join("\n")
    );

    let v1_keys: BTreeSet<_> = v1.iter().map(stable_key_from_node).collect();
    let v2_keys: BTreeSet<_> = v2.iter().map(stable_key_from_node).collect();
    let added: Vec<_> = v2
        .iter()
        .filter(|n| !v1_keys.contains(&stable_key_from_node(n)))
        .map(|n| {
            format!(
                "added {:?} {} qn={} file={}",
                n.node_type,
                n.name.as_str(),
                n.qualified_name.as_deref().unwrap_or(""),
                n.file_path.as_deref().unwrap_or("")
            )
        })
        .collect();
    let removed: Vec<_> = v1
        .iter()
        .filter(|n| !v2_keys.contains(&stable_key_from_node(n)))
        .map(|n| {
            format!(
                "removed {:?} {} qn={} file={}",
                n.node_type,
                n.name.as_str(),
                n.qualified_name.as_deref().unwrap_or(""),
                n.file_path.as_deref().unwrap_or("")
            )
        })
        .collect();
    assert!(
        added.is_empty() && removed.is_empty(),
        "Law 5 body edit structural drift:\n{}\n{}",
        added.join("\n"),
        removed.join("\n")
    );

    let base = open_store(v1_snap);
    let head = open_store(v2_snap);
    let mut sink = NoopDiffSink;
    let stats = diff_snapshots(&base, &head, &mut sink).expect("diff body edit");
    assert_eq!(stats.nodes_added, 0, "Law 5 body edit nodes_added");
    assert_eq!(stats.nodes_removed, 0, "Law 5 body edit nodes_removed");
}

fn find_covering_function<'a>(nodes: &'a [Node], rel: &str, marker: u32) -> Option<&'a Node> {
    nodes
        .iter()
        .filter(|n| {
            n.node_type == NodeType::Function
                && n.file_path
                    .as_deref()
                    .is_some_and(|p| p.replace('\\', "/").ends_with(rel))
                && n.start_byte.is_some_and(|start| start < marker)
                && n.end_byte.is_some_and(|end| end > marker)
        })
        .min_by_key(|n| n.end_byte.unwrap_or(u32::MAX) - n.start_byte.unwrap_or(0))
}

fn law5_overload(
    expected_added: usize,
    v1_nodes: &[Node],
    v1_snap: &Path,
    v3_nodes: &[Node],
    v3_snap: &Path,
) {
    let before = function_keys(v1_nodes);
    let after = function_keys(v3_nodes);
    let added: Vec<_> = after.difference(&before).copied().collect();
    let removed: Vec<_> = before.difference(&after).copied().collect();
    let desc = |nodes: &[Node], keys: &[StableNodeKey]| -> Vec<String> {
        nodes
            .iter()
            .filter(|n| {
                n.node_type == NodeType::Function && keys.contains(&stable_key_from_node(n))
            })
            .map(|n| {
                format!(
                    "{} {} qn={:?} sig={:?}",
                    n.file_path.as_deref().unwrap_or(""),
                    n.name.as_str(),
                    n.qualified_name,
                    n.signature
                )
            })
            .collect()
    };
    assert!(
        removed.is_empty(),
        "Law 5: overload addition removed {} existing function(s):\nremoved={:#?}\nadded={:#?}",
        removed.len(),
        desc(v1_nodes, &removed),
        desc(v3_nodes, &added)
    );
    assert_eq!(
        added.len(),
        expected_added,
        "Law 5: adding {expected_added} same-name overloads should introduce {expected_added} function keys, got {}:\nadded={:#?}",
        added.len(),
        desc(v3_nodes, &added)
    );
    let base = open_store(v1_snap);
    let head = open_store(v3_snap);
    let mut sink = NoopDiffSink;
    let stats = diff_snapshots(&base, &head, &mut sink).expect("diff overload");
    assert_eq!(stats.nodes_removed, 0, "Law 5 overload nodes_removed");
    assert_eq!(
        stats.nodes_added, expected_added,
        "Law 5 overload nodes_added, got {}",
        stats.nodes_added
    );
}

fn law6_digest(forward_files: &[PathBuf], forward_nodes: &[Node], forward_edges: &[Edge]) {
    let tmp = TempDir::new().unwrap();
    let fwd = tmp.path().join("forward.bin");
    let rev = tmp.path().join("reverse.bin");
    let digest_fwd =
        write_columnar_from_nodes_edges(forward_nodes.to_vec(), forward_edges.to_vec(), &fwd)
            .unwrap();
    let mut reversed = forward_files.to_vec();
    reversed.reverse();
    let (nodes, edges) = extract(&reversed, Some(4), false);
    let digest_rev = write_columnar_from_nodes_edges(nodes, edges, &rev).unwrap();
    assert_eq!(
        digest_fwd, digest_rev,
        "Law 6: threads=1 forward digest != threads=4 reverse digest"
    );
    for path in [&fwd, &rev] {
        let store = open_store(path);
        assert_graph_invariants(store.columnar().unwrap()).unwrap();
    }
}

fn law4_lazy_vs_upfront(root: &Path, v1_nodes: &[Node]) {
    let mut errors = Vec::new();
    let mut checked = 0usize;
    for fix in FIXTURES {
        let Some(profile) = profile_for_language(fix.id) else {
            continue;
        };
        if !profile.cfg_enabled {
            continue;
        }
        let cfg_name: &str = if fix.id == "hcl" {
            "resource.null_resource.target_fn"
        } else {
            fix.target
        };
        let target_node = find_named(v1_nodes, fix.rel, fix.target)
            .unwrap_or_else(|| panic!("Law 4: {} missing target {}", fix.id, fix.target));
        let source = fs::read_to_string(root.join(fix.rel)).unwrap();
        let direct = match build_cfg_for_function(fix.id, &source, cfg_name) {
            Ok(cfg) => cfg,
            Err(err) => {
                errors.push(format!("{}: upfront cfg: {err}", fix.id));
                continue;
            }
        };
        let lazy = match ensure_cfg_pdg_for_source(EnsureCfgPdgOpts {
            repo_root: root,
            language: fix.id,
            source: &source,
            function_name: cfg_name,
            file_path: Some(fix.rel),
            start_byte: target_node.start_byte.map(|b| b as usize),
            end_byte: target_node.end_byte.map(|b| b as usize),
            function_id: None,
            persist: false,
        }) {
            Ok(record) => record,
            Err(err) => {
                errors.push(format!("{}: lazy cfg: {err}", fix.id));
                continue;
            }
        };
        if direct.blocks.len() != lazy.cfg.blocks.len() {
            errors.push(format!(
                "{}: block count upfront {} != lazy {}",
                fix.id,
                direct.blocks.len(),
                lazy.cfg.blocks.len()
            ));
            continue;
        }
        let upfront_dom = DominatorTree::build(&direct);
        let lazy_dom = DominatorTree::build(&lazy.cfg);
        if upfront_dom.idom != lazy_dom.idom {
            errors.push(format!("{}: dominator trees differ", fix.id));
        }
        let upfront_pdg = ProgramDependenceGraph::build(&direct, source.as_bytes());
        match upfront_pdg {
            Ok(pdg) => {
                if pdg.data_deps.len() != lazy.pdg.data_deps.len()
                    || pdg.control_deps.len() != lazy.pdg.control_deps.len()
                {
                    errors.push(format!("{}: PDG edge counts differ", fix.id));
                }
                if let Some((var, line)) = pdg.nodes.values().find_map(|n| {
                    let var = n
                        .defined_vars
                        .iter()
                        .chain(n.used_vars.iter())
                        .next()?
                        .clone();
                    (n.statement.line > 0).then_some((var, n.statement.line))
                }) {
                    let crit = SliceCriterion {
                        variable: var,
                        line,
                    };
                    let s1 = BackwardSlicer::new(&pdg, &direct).slice(crit.clone());
                    let s2 = BackwardSlicer::new(&lazy.pdg, &lazy.cfg).slice(crit);
                    match (s1, s2) {
                        (Ok(a), Ok(b)) if a.lines != b.lines => {
                            errors.push(format!(
                                "{}: backward slice lines differ: upfront={:?} lazy={:?}",
                                fix.id, a.lines, b.lines
                            ));
                        }
                        (Err(e1), Ok(_)) | (Ok(_), Err(e1)) => {
                            errors.push(format!("{}: backward slice parity error: {e1}", fix.id));
                        }
                        _ => {}
                    }
                }
            }
            Err(err) => errors.push(format!("{}: pdg: {err}", fix.id)),
        }
        checked += 1;
    }
    assert!(
        errors.is_empty(),
        "Law 4 lazy vs upfront CFG mismatches:\n{}",
        errors.join("\n")
    );
    assert_eq!(
        checked, 22,
        "Law 4 must verify all 22 cfg_enabled Tier-1 languages, got {checked}"
    );
}

fn law3_incremental_matches_cold(root: &Path) {
    let registry = shared_registry();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            show_progress: false,
            thread_count: Some(1),
            facts_cache: false,
            materialize_fields: false,
            ..PipelineConfig::default()
        },
    );
    let snap = MmappedGraphSnapshot::default_path(root);
    pipeline
        .process_repository_to_snapshot(root, &snap, Some(root))
        .expect("cold v1");
    let graph = CodeGraph::open_snapshot(&snap).expect("open cold graph");
    let files = discover_sorted(root);
    let mut tracker = FileTracker::new(root);
    tracker.index_files(&files, &graph).expect("index tracker");
    tracker.save().expect("save tracker");

    for fix in FIXTURES {
        let path = root.join(fix.rel);
        let text = fs::read_to_string(&path).unwrap().replace(MARKER, EDITED);
        fs::write(&path, text).unwrap();
    }
    let _ = apply_same_name_overloads(root);

    let mut graph = graph;
    IncrementalUpdater::new(registry)
        .update(&mut graph, root)
        .expect("incremental update");
    let incremental = snap.clone();

    let cold_art = root.join("..").join("cold-art");
    let cold_snap = root.join("..").join("cold.bin");
    fs::create_dir_all(&cold_art).unwrap();
    let pipeline = ProcessingPipeline::with_config(
        shared_registry(),
        PipelineConfig {
            show_progress: false,
            thread_count: Some(1),
            facts_cache: false,
            ..PipelineConfig::default()
        },
    );
    pipeline
        .process_repository_to_snapshot(root, &cold_snap, Some(&cold_art))
        .expect("cold v2");

    let inc_store = open_store(&incremental);
    let cold_store = open_store(&cold_snap);
    let mut sink = NoopDiffSink;
    let stats = diff_snapshots(&inc_store, &cold_store, &mut sink).expect("diff incremental");
    assert_eq!(stats.nodes_added, 0, "Law 3 nodes_added {stats:?}");
    assert_eq!(stats.nodes_removed, 0, "Law 3 nodes_removed {stats:?}");
    assert_eq!(stats.edges_added, 0, "Law 3 edges_added {stats:?}");
    assert_eq!(stats.edges_removed, 0, "Law 3 edges_removed {stats:?}");
}

#[test]
fn tier1_six_law_metamorphic_gate() {
    let started = Instant::now();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    write_fixtures(&root);
    let files = discover_sorted(&root);

    let (v1_nodes, v1_edges) = extract(&files, Some(1), false);
    law6_digest(&files, &v1_nodes, &v1_edges);
    law1_mode_monotonicity(&root, &files, &v1_nodes, &v1_edges);

    let v1_snap = tmp.path().join("v1.bin");
    write_snap(v1_nodes.clone(), v1_edges.clone(), &v1_snap);

    for fix in FIXTURES {
        let path = root.join(fix.rel);
        let text = fs::read_to_string(&path).unwrap().replace(MARKER, EDITED);
        fs::write(path, text).unwrap();
    }
    let edited_files = discover_sorted(&root);
    let (v2_nodes, v2_edges) = extract(&edited_files, Some(1), false);
    let v2_snap = tmp.path().join("v2.bin");
    write_snap(v2_nodes.clone(), v2_edges, &v2_snap);
    law2_and_law5_body(&v1_nodes, &v2_nodes, &v1_snap, &v2_snap);

    // Restore v1, then add real same-name overloads across 7 overloaded OOP languages.
    write_fixtures(&root);
    let expected_overloads = apply_same_name_overloads(&root);
    let overload_files = discover_sorted(&root);
    let (v3_nodes, v3_edges) = extract(&overload_files, Some(1), false);
    let v3_snap = tmp.path().join("v3.bin");
    write_snap(v3_nodes.clone(), v3_edges, &v3_snap);
    law5_overload(expected_overloads, &v1_nodes, &v1_snap, &v3_nodes, &v3_snap);

    write_fixtures(&root);
    law4_lazy_vs_upfront(&root, &v1_nodes);

    let inc_root = tmp.path().join("inc");
    fs::create_dir_all(&inc_root).unwrap();
    write_fixtures(&inc_root);
    law3_incremental_matches_cold(&inc_root);

    let elapsed = started.elapsed().as_secs_f64();
    assert!(
        elapsed < 3.0,
        "metamorphic gate took {elapsed:.2}s; Tier 2 budget is < 3s"
    );
}

/// Two discovered repos: the service defines `serve` overloads, the client calls one.
#[test]
fn merge_stitches_cross_repo_overload_call() {
    let tmp = TempDir::new().unwrap();
    let service = tmp.path().join("service");
    let client = tmp.path().join("client");
    fs::create_dir_all(service.join("src")).unwrap();
    fs::create_dir_all(client.join("src")).unwrap();
    fs::write(
        service.join("src/Service.java"),
        "package svc;\npublic class Service {\n  public int serve(int x) { return x; }\n  public String serve(String s) { return s; }\n}\n",
    )
    .unwrap();
    fs::write(
        client.join("src/Client.java"),
        "package app;\nimport svc.Service;\npublic class Client {\n  public int go() { return Service.serve(1); }\n}\n",
    )
    .unwrap();

    let pipeline = ProcessingPipeline::with_config(
        shared_registry(),
        PipelineConfig {
            show_progress: false,
            thread_count: Some(1),
            facts_cache: false,
            ..PipelineConfig::default()
        },
    );
    pipeline
        .process_repository_to_snapshot(
            &service,
            &MmappedGraphSnapshot::default_path(&service),
            Some(&service),
        )
        .expect("discover service");
    pipeline
        .process_repository_to_snapshot(
            &client,
            &MmappedGraphSnapshot::default_path(&client),
            Some(&client),
        )
        .expect("discover client");

    let out = tmp.path().join("merged");
    fs::create_dir_all(&out).unwrap();
    let report = rgctl_extraction::merge_repositories(&[service.clone(), client.clone()], &out)
        .expect("merge");

    let snap = MmappedGraphSnapshot::default_path(&out);
    let store = SnapshotNodeStore::open(&snap).expect("open merged");
    assert_graph_invariants(store.columnar().expect("columnar")).expect("INV-1..INV-9");

    let mut nodes = Vec::new();
    store
        .for_each_node_seq(|node| nodes.push(node.clone()))
        .unwrap();
    let serves: Vec<_> = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Function && n.name.as_str() == "serve")
        .collect();
    assert_eq!(
        serves.len(),
        2,
        "merge dropped a serve overload; report={report:?} names={:?}",
        nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function)
            .map(|n| format!(
                "{} qn={}",
                n.name.as_str(),
                n.qualified_name.as_deref().unwrap_or("")
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        serves
            .iter()
            .map(|n| stable_key_from_node(n))
            .collect::<BTreeSet<_>>()
            .len(),
        2,
        "serve overloads collapsed to one stable key"
    );

    let go = nodes
        .iter()
        .find(|n| n.node_type == NodeType::Function && n.name.as_str() == "go")
        .expect("client go()");
    let serve_ids: BTreeSet<_> = serves.iter().map(|n| n.id).collect();
    let edges = store.edge_topology_typed().expect("edges");
    let calls: Vec<_> = edges
        .iter()
        .filter(|(_, to, kind)| *kind == EdgeType::Calls && serve_ids.contains(to))
        .filter(|(from, _, _)| *from == go.id)
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "cross-repo Calls to serve: report={report:?} calls={calls:?}"
    );
    let target = nodes
        .iter()
        .find(|n| n.id == calls[0].1)
        .expect("call target");
    let signature = target.signature.as_deref().unwrap_or("");
    assert!(
        signature.contains("int") && !signature.contains("String"),
        "call resolved to the wrong overload: {signature}"
    );
}
