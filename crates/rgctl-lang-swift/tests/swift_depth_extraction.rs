//! Golden correctness + unconditional AST coverage exercise for Swift.

use rgctl_lang_swift::SwiftPlugin;
use rgctl_plugin_api::{LanguagePlugin, SymbolType};
use std::path::Path;

#[test]
fn enum_entries_protocol_and_associatedtype() {
    let source = br#"
enum Color { case red, green }

protocol Named {
  associatedtype ID
  var title: String { get }
  func label() -> String
}

typealias UserId = Int
"#;
    let plugin = SwiftPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.swift"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "Color" && s.symbol_type == SymbolType::Enum),
        "Color enum missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols.iter().any(|s| s.name == "red" || s.name == "green"),
        "enum entries missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "Named" && s.symbol_type == SymbolType::Interface),
        "protocol Named missing"
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "ID" && s.symbol_type == SymbolType::TypeAlias),
        "associatedtype ID missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "UserId" && s.symbol_type == SymbolType::TypeAlias),
        "typealias UserId missing"
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../swift-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_swift::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("swift", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let corpus = include_bytes!("fixtures/ast_coverage_corpus.swift");
    let plugin = SwiftPlugin::new().expect("plugin");
    plugin
        .extract_all(Path::new("ast_coverage_corpus.swift"), corpus)
        .expect("extract_all");

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Swift AST kind in swift-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("swift unhandled hotspots: {hotspots:?}");
}
