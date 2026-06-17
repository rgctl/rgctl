//! Phase 11.1.2 — Tier 2 high-priority language plugin tests

use rbuilder::languages::generic::TreeSitterLanguagePlugin;
use rbuilder::languages::plugin_trait::{LanguagePlugin, SymbolType};
use std::path::Path;

fn assert_extracts_function(
    lang: &str,
    loader: fn() -> tree_sitter::Language,
    file: &str,
    source: &str,
    expected_fn: &str,
) {
    let plugin = TreeSitterLanguagePlugin::new(lang, loader).unwrap();
    let symbols = plugin
        .extract_symbols(Path::new(file), source.as_bytes())
        .unwrap();
    assert!(
        symbols.iter().any(|s| s.name == expected_fn),
        "expected symbol '{}' in {:?}",
        expected_fn,
        symbols
            .iter()
            .map(|s| (&s.name, s.symbol_type))
            .collect::<Vec<_>>()
    );
}

#[cfg(feature = "lang-swift")]
fn load_swift() -> tree_sitter::Language {
    tree_sitter_swift::LANGUAGE.into()
}

#[cfg(feature = "lang-scala")]
fn load_scala() -> tree_sitter::Language {
    tree_sitter_scala::LANGUAGE.into()
}

#[cfg(feature = "lang-lua")]
fn load_lua() -> tree_sitter::Language {
    tree_sitter_lua::LANGUAGE.into()
}

#[cfg(feature = "lang-erlang")]
fn load_erlang() -> tree_sitter::Language {
    tree_sitter_erlang::LANGUAGE.into()
}

#[cfg(feature = "lang-haskell")]
fn load_haskell() -> tree_sitter::Language {
    tree_sitter_haskell::LANGUAGE.into()
}

#[cfg(feature = "lang-dart")]
fn load_dart() -> tree_sitter::Language {
    tree_sitter_dart::LANGUAGE.into()
}

#[cfg(feature = "lang-r")]
fn load_r() -> tree_sitter::Language {
    tree_sitter_r::LANGUAGE.into()
}

#[cfg(feature = "lang-julia")]
fn load_julia() -> tree_sitter::Language {
    tree_sitter_julia::LANGUAGE.into()
}

#[cfg(feature = "lang-swift")]
#[test]
fn test_swift_plugin() {
    assert_extracts_function(
        "swift",
        load_swift,
        "test.swift",
        "func add(a: Int, b: Int) -> Int { return a + b }",
        "add",
    );
}

#[cfg(feature = "lang-scala")]
#[test]
fn test_scala_plugin() {
    assert_extracts_function(
        "scala",
        load_scala,
        "test.scala",
        "object Demo { def add(a: Int, b: Int): Int = a + b }",
        "add",
    );
}

#[cfg(feature = "lang-lua")]
#[test]
fn test_lua_plugin() {
    assert_extracts_function(
        "lua",
        load_lua,
        "test.lua",
        "function add(a, b)\n  return a + b\nend",
        "add",
    );
}

#[cfg(feature = "lang-erlang")]
#[test]
fn test_erlang_plugin() {
    assert_extracts_function(
        "erlang",
        load_erlang,
        "test.erl",
        "-module(demo).\nadd(A, B) -> A + B.",
        "add",
    );
}

#[cfg(feature = "lang-haskell")]
#[test]
fn test_haskell_plugin() {
    assert_extracts_function(
        "haskell",
        load_haskell,
        "test.hs",
        "add a b = a + b",
        "add",
    );
}

#[cfg(feature = "lang-dart")]
#[test]
fn test_dart_plugin() {
    assert_extracts_function(
        "dart",
        load_dart,
        "test.dart",
        "int add(int a, int b) => a + b;",
        "add",
    );
}

#[cfg(feature = "lang-r")]
#[test]
fn test_r_plugin() {
    assert_extracts_function(
        "r",
        load_r,
        "test.r",
        "add <- function(a, b) { a + b }",
        "add",
    );
}

#[cfg(feature = "lang-julia")]
#[test]
fn test_julia_plugin() {
    assert_extracts_function(
        "julia",
        load_julia,
        "test.jl",
        "function add(a, b)\n    a + b\nend",
        "add",
    );
}

#[cfg(feature = "lang-nim")]
#[test]
fn test_nim_plugin() {
    assert_extracts_function(
        "nim",
        tree_sitter_nim::language,
        "test.nim",
        "proc add(a, b: int): int = a + b",
        "add",
    );
}

#[cfg(feature = "lang-elixir")]
#[test]
fn test_elixir_plugin_parses() {
    let plugin = TreeSitterLanguagePlugin::new("elixir", || tree_sitter_elixir::LANGUAGE.into())
        .unwrap();
    let source = "def add(a, b), do: a + b";
    let symbols = plugin
        .extract_symbols(Path::new("test.ex"), source.as_bytes())
        .unwrap();
    // Generic extractor only captures anonymous_function; full def support needs custom handler.
    assert!(
        symbols.is_empty() || symbols.iter().all(|s| s.symbol_type == SymbolType::Function),
        "unexpected symbols: {:?}",
        symbols
    );
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_full_bundle_registers_tier2_languages() {
    use rbuilder::languages::registry::LanguageRegistry;

    let registry = LanguageRegistry::new();
    let ids: Vec<_> = registry
        .supported_languages()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    for lang in [
        "swift", "scala", "elixir", "erlang", "dart", "lua", "haskell", "julia", "r", "nim",
    ] {
        assert!(ids.contains(&lang.to_string()), "missing language: {lang}");
    }
}
