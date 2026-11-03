//! Golden correctness + unconditional AST coverage exercise for SQL.

use rgctl_lang_sql::Plugin;
use rgctl_plugin_api::{LanguagePlugin, SymbolType};
use std::path::Path;

#[test]
fn table_function_and_view_symbols() {
    let source = br#"
CREATE TABLE users (id INT);
CREATE FUNCTION foo() RETURNS INT LANGUAGE sql AS $$ SELECT 1; $$;
CREATE VIEW v AS SELECT * FROM users;
"#;
    let plugin = Plugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.sql"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "users" && s.symbol_type == SymbolType::Table),
        "table users missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "foo" && s.symbol_type == SymbolType::Function),
        "function foo missing"
    );
    assert!(out.symbols.iter().any(|s| s.name == "v"), "view v missing");
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../sql-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_sql::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("sql", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let corpus = include_bytes!("fixtures/ast_coverage_corpus.sql");
    let plugin = Plugin::new().expect("plugin");
    plugin
        .extract_all(Path::new("ast_coverage_corpus.sql"), corpus)
        .expect("extract_all");

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active SQL AST kind in sql-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("sql unhandled hotspots: {hotspots:?}");
}
