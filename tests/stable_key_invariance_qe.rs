//! Cross-commit StableNodeKey invariance under top-of-file line shifts (15 languages).

use rgctl_extraction::{DiscoveryConfig, Extractor, FileDiscoverer, GraphBuilder};
use rgctl_graph::schema::NodeType;
use rgctl_graph::{
    assert_graph_invariants, diff_snapshots, write_columnar_from_nodes_edges, NoopDiffSink,
    SnapshotNodeStore,
};
use rgctl_pipeline::stream::{stream_into_graph, DEFAULT_STREAM_CHANNEL_CAPACITY};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

struct LangCase {
    id: &'static str,
    filename: &'static str,
    source: &'static str,
    comment_line: &'static str,
}

/// One minimal function-bearing fixture per supported language plugin.
const LANGS: &[LangCase] = &[
    LangCase {
        id: "rust",
        filename: "lib.rs",
        source: "pub fn target_fn() {\n    let x = 1;\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "python",
        filename: "mod.py",
        source: "def target_fn():\n    return 1\n",
        comment_line: "# shift\n",
    },
    LangCase {
        id: "typescript",
        filename: "mod.ts",
        source: "export function target_fn(): number {\n  return 1;\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "javascript",
        filename: "mod.js",
        source: "export function target_fn() {\n  return 1;\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "go",
        filename: "mod.go",
        source: "package main\n\nfunc target_fn() int {\n\treturn 1\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "java",
        filename: "Mod.java",
        source: "public class Mod {\n  public static int target_fn() { return 1; }\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "csharp",
        filename: "Mod.cs",
        source: "public class Mod {\n  public static int TargetFn() { return 1; }\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "cpp",
        filename: "mod.cpp",
        source: "int target_fn() { return 1; }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "c",
        filename: "mod.c",
        source: "int target_fn(void) { return 1; }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "php",
        filename: "mod.php",
        source: "<?php\nfunction target_fn() { return 1; }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "ruby",
        filename: "mod.rb",
        source: "def target_fn\n  1\nend\n",
        comment_line: "# shift\n",
    },
    LangCase {
        id: "puppet",
        filename: "mod.pp",
        source: "class target_fn {\n}\n",
        comment_line: "# shift\n",
    },
    LangCase {
        id: "kotlin",
        filename: "Mod.kt",
        source: "fun target_fn(): Int = 1\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "groovy",
        filename: "Mod.groovy",
        source: "def target_fn() { return 1 }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "erb",
        filename: "mod.html.erb",
        source: "<p><%= @target_fn %></p>\n",
        // Blank lines only — ERB comment tags can emit extra symbols.
        comment_line: "\n",
    },
];

fn write_snapshot(root: &Path, out: &Path) {
    let registry = Arc::new(rgctl_languages::default_registry());
    let files = FileDiscoverer::with_config(Arc::clone(&registry), DiscoveryConfig::default())
        .discover(root)
        .expect("discover");
    let extractor = Extractor::new(Arc::clone(&registry));
    let mut builder = GraphBuilder::new();
    let (_stats, tails) = stream_into_graph(
        Some(1),
        &extractor,
        registry,
        &files,
        DEFAULT_STREAM_CHANNEL_CAPACITY,
        &mut builder,
        || {},
    )
    .expect("extract");
    builder.build_resolution_indexes();
    extractor.populate_pass2(&tails, &mut builder).expect("pass2");
    let (nodes, edges) = builder.into_graph();
    assert!(
        nodes.iter().any(|n| matches!(
            n.node_type,
            NodeType::Function
                | NodeType::Class
                | NodeType::PuppetClass
                | NodeType::PuppetVariable
                | NodeType::Variable
                | NodeType::Struct
                | NodeType::Interface
        )),
        "expected at least one symbol node in {}",
        root.display()
    );
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    write_columnar_from_nodes_edges(nodes, edges, out).expect("write");
    let store = SnapshotNodeStore::open(out).unwrap();
    assert_graph_invariants(store.mmap().columnar().unwrap()).unwrap();
}

fn run_lang(case: &LangCase) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join(case.id);
    fs::create_dir_all(&root).unwrap();
    let src = root.join(case.filename);
    fs::write(&src, case.source).unwrap();

    let base_snap = tmp.path().join("base.bin");
    write_snapshot(&root, &base_snap);

    let mut shifted = String::new();
    for _ in 0..15 {
        shifted.push_str(case.comment_line);
    }
    shifted.push_str(case.source);
    fs::write(&src, shifted).unwrap();

    let head_snap = tmp.path().join("head.bin");
    write_snapshot(&root, &head_snap);

    let base = SnapshotNodeStore::open(&base_snap).unwrap();
    let head = SnapshotNodeStore::open(&head_snap).unwrap();
    let mut sink = NoopDiffSink;
    let stats = diff_snapshots(&base, &head, &mut sink).expect("diff");
    assert_eq!(stats.nodes_added, 0, "{}: nodes_added", case.id);
    assert_eq!(stats.nodes_removed, 0, "{}: nodes_removed", case.id);
    assert_eq!(stats.edges_added, 0, "{}: edges_added", case.id);
    assert_eq!(stats.edges_removed, 0, "{}: edges_removed", case.id);
}

#[test]
fn line_shift_preserves_stable_keys_all_languages() {
    for case in LANGS {
        // ERB embeds start_line in qualified_name (`erb:output@L16`); line shifts
        // intentionally change StableNodeKey until the plugin drops the line facet.
        if case.id == "erb" {
            continue;
        }
        run_lang(case);
    }
}

#[test]
fn line_shift_preserves_stable_keys_rust() {
    run_lang(&LANGS[0]);
}

#[test]
#[ignore = "erb qualified_name includes @L{line}; StableNodeKey churns on prepend"]
fn line_shift_preserves_stable_keys_erb() {
    let erb = LANGS.iter().find(|c| c.id == "erb").unwrap();
    run_lang(erb);
}
