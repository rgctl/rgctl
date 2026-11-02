//! Golden correctness + unconditional AST coverage exercise for Scala.

use rgctl_lang_scala::Plugin;
use rgctl_plugin_api::{LanguagePlugin, SymbolType};
use std::path::Path;

#[test]
fn class_function_and_calls() {
    let source = br#"
class Foo {
  def bar(x: Int): Int = {
    if (true) baz(x)
    return 1
  }
}
"#;
    let plugin = Plugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("Foo.scala"), source)
        .expect("extract_all");
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "Foo" && s.symbol_type == SymbolType::Class),
        "Foo class missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "bar" && s.symbol_type == SymbolType::Function),
        "bar function missing"
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../scala-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_scala::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("scala", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let corpus = include_bytes!("fixtures/ast_coverage_corpus.scala");
    let plugin = Plugin::new().expect("plugin");
    plugin
        .extract_all(Path::new("ast_coverage_corpus.scala"), corpus)
        .expect("extract_all");

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Scala AST kind in scala-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("scala unhandled hotspots: {hotspots:?}");
}
