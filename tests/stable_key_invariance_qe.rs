//! Cross-commit StableNodeKey invariance under top-of-file line shifts (24 Tier-1 languages).

use rgctl_extraction::{DiscoveryConfig, Extractor, FileDiscoverer, GraphBuilder};
use rgctl_graph::schema::NodeType;
use rgctl_graph::{
    NoopDiffSink, SnapshotNodeStore, assert_graph_invariants, diff_snapshots,
    write_columnar_from_nodes_edges,
};
use rgctl_pipeline::stream::{DEFAULT_STREAM_CHANNEL_CAPACITY, stream_into_graph};
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

/// Two sibling callables (or two sibling symbols for DSLs) plus UTF-8 trivia.
const LANGS: &[LangCase] = &[
    LangCase {
        id: "rust",
        filename: "lib.rs",
        source: "// café\npub fn target_fn() {\n    let x = 1;\n}\npub fn sibling_fn() {\n    let y = 2;\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "python",
        filename: "mod.py",
        source: "# café\ndef target_fn():\n    return 1\n\ndef sibling_fn():\n    return 2\n",
        comment_line: "# shift\n",
    },
    LangCase {
        id: "typescript",
        filename: "mod.ts",
        source: "// café\nexport function target_fn(): number {\n  return 1;\n}\nexport function sibling_fn(): number {\n  return 2;\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "javascript",
        filename: "mod.js",
        source: "// café\nexport function target_fn() {\n  return 1;\n}\nexport function sibling_fn() {\n  return 2;\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "go",
        filename: "mod.go",
        source: "package main\n\n// café\nfunc target_fn() int {\n\treturn 1\n}\n\nfunc sibling_fn() int {\n\treturn 2\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "java",
        filename: "Mod.java",
        source: "public class Mod {\n  // café\n  public static int target_fn() { return 1; }\n  public static int sibling_fn() { return 2; }\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "csharp",
        filename: "Mod.cs",
        source: "public class Mod {\n  // café\n  public static int TargetFn() { return 1; }\n  public static int SiblingFn() { return 2; }\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "cpp",
        filename: "mod.cpp",
        source: "// café\nint target_fn() { return 1; }\nint sibling_fn() { return 2; }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "c",
        filename: "mod.c",
        source: "/* café */\nint target_fn(void) { return 1; }\nint sibling_fn(void) { return 2; }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "php",
        filename: "mod.php",
        source: "<?php\n// café\nfunction target_fn() { return 1; }\nfunction sibling_fn() { return 2; }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "ruby",
        filename: "mod.rb",
        source: "# café\ndef target_fn\n  1\nend\ndef sibling_fn\n  2\nend\n",
        comment_line: "# shift\n",
    },
    LangCase {
        id: "puppet",
        filename: "mod.pp",
        source: "# café\nfunction target_fn() {\n  1\n}\nfunction sibling_fn() {\n  2\n}\n",
        comment_line: "# shift\n",
    },
    LangCase {
        id: "kotlin",
        filename: "Mod.kt",
        source: "// café\nfun target_fn(): Int = 1\nfun sibling_fn(): Int = 2\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "groovy",
        filename: "Mod.groovy",
        source: "// café\ndef target_fn() { return 1 }\ndef sibling_fn() { return 2 }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "erb",
        filename: "mod.html.erb",
        source: "<p><%= @target_fn %></p>\n<% sibling_fn = 1 %>\n<!-- café -->\n",
        // Blank lines only — ERB comment tags can emit extra symbols.
        comment_line: "\n",
    },
    LangCase {
        id: "swift",
        filename: "Mod.swift",
        source: "// café\nfunc target_fn() -> Int { return 1 }\nfunc sibling_fn() -> Int { return 2 }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "scala",
        filename: "Mod.scala",
        source: "// café\nclass Mod {\n  def target_fn(): Int = 1\n  def sibling_fn(): Int = 2\n}\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "dart",
        filename: "mod.dart",
        source: "// café\nint target_fn() { return 1; }\nint sibling_fn() { return 2; }\n",
        comment_line: "// shift\n",
    },
    LangCase {
        id: "elixir",
        filename: "mod.ex",
        source: "# café\ndefmodule Mod do\n  def target_fn, do: 1\n  def sibling_fn, do: 2\nend\n",
        comment_line: "# shift\n",
    },
    LangCase {
        id: "lua",
        filename: "mod.lua",
        source: "-- café\nfunction target_fn()\n  return 1\nend\nfunction sibling_fn()\n  return 2\nend\n",
        comment_line: "-- shift\n",
    },
    LangCase {
        id: "bash",
        filename: "mod.sh",
        source: "# café\ntarget_fn() { echo 1; }\nsibling_fn() { echo 2; }\n",
        comment_line: "# shift\n",
    },
    LangCase {
        id: "sql",
        filename: "mod.sql",
        source: "-- café\nCREATE FUNCTION target_fn() RETURNS INT LANGUAGE sql AS $$ SELECT 1; $$;\nCREATE FUNCTION sibling_fn() RETURNS INT LANGUAGE sql AS $$ SELECT 2; $$;\n",
        comment_line: "-- shift\n",
    },
    LangCase {
        id: "hcl",
        filename: "mod.tf",
        source: "# café\nresource \"null_resource\" \"target_fn\" {\n}\nresource \"null_resource\" \"sibling_fn\" {\n}\n",
        comment_line: "# shift\n",
    },
    LangCase {
        id: "markdown",
        filename: "doc.md",
        source: "# Target Fn\n\nParagraph with café.\n\n# Sibling Fn\n\nMore.\n",
        // A new heading would add a symbol; blank lines only shift spans.
        comment_line: "\n",
    },
];

fn shared_registry() -> Arc<rgctl_registry::LanguageRegistry> {
    static REGISTRY: std::sync::OnceLock<Arc<rgctl_registry::LanguageRegistry>> =
        std::sync::OnceLock::new();
    Arc::clone(REGISTRY.get_or_init(|| Arc::new(rgctl_languages::default_registry())))
}

fn write_snapshot(root: &Path, out: &Path) {
    let registry = shared_registry();
    let files = FileDiscoverer::with_config(Arc::clone(&registry), DiscoveryConfig::default())
        .discover(root)
        .expect("discover");
    let extractor = Extractor::new(Arc::clone(&registry));
    let mut builder = GraphBuilder::new();
    let (_stats, tails, _lossless_sidecars) = stream_into_graph(
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
    extractor
        .populate_pass2(&tails, &mut builder)
        .expect("pass2");
    let (nodes, edges) = builder.into_graph();
    let symbol_count = nodes
        .iter()
        .filter(|n| {
            !matches!(
                n.node_type,
                NodeType::File | NodeType::Import | NodeType::Dependency
            )
        })
        .count();
    assert!(
        symbol_count >= 2,
        "expected ≥2 sibling symbols in {}, got {symbol_count}",
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
        run_lang(case);
    }
}

#[test]
fn line_shift_preserves_stable_keys_rust() {
    run_lang(&LANGS[0]);
}

#[test]
fn line_shift_preserves_stable_keys_erb_multiblock() {
    let erb = LANGS.iter().find(|c| c.id == "erb").expect("erb fixture");
    run_lang(erb);
}
