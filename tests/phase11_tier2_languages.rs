//! Phase 11.1.2 — Tier 2 high-priority language plugin tests

use rbuilder::languages::registry::LanguageRegistry;
use std::path::Path;

fn assert_extracts_function(lang: &str, file: &str, source: &str, expected_fn: &str) {
    let registry = LanguageRegistry::new();
    let plugin = registry
        .get_language_plugin(lang)
        .unwrap_or_else(|| panic!("missing language plugin: {lang}"));
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

#[cfg(feature = "bundle-full")]
#[test]
fn test_swift_plugin() {
    assert_extracts_function(
        "swift",
        "test.swift",
        "func add(a: Int, b: Int) -> Int { return a + b }",
        "add",
    );
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_scala_plugin() {
    assert_extracts_function(
        "scala",
        "test.scala",
        "object Demo { def add(a: Int, b: Int): Int = a + b }",
        "add",
    );
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_lua_plugin() {
    assert_extracts_function(
        "lua",
        "test.lua",
        "function add(a, b)\n  return a + b\nend",
        "add",
    );
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_erlang_plugin() {
    assert_extracts_function(
        "erlang",
        "test.erl",
        "-module(demo).\nadd(A, B) -> A + B.",
        "add",
    );
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_haskell_plugin() {
    assert_extracts_function("haskell", "test.hs", "add a b = a + b", "add");
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_dart_plugin() {
    assert_extracts_function(
        "dart",
        "test.dart",
        "int add(int a, int b) => a + b;",
        "add",
    );
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_r_plugin() {
    assert_extracts_function("r", "test.r", "add <- function(a, b) { a + b }", "add");
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_julia_plugin() {
    assert_extracts_function(
        "julia",
        "test.jl",
        "function add(a, b)\n    a + b\nend",
        "add",
    );
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_nim_plugin() {
    assert_extracts_function("nim", "test.nim", "proc add(a, b: int): int = a + b", "add");
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_elixir_plugin() {
    assert_extracts_function("elixir", "test.ex", "def add(a, b), do: a + b", "add");
}

#[cfg(feature = "bundle-full")]
#[test]
fn test_full_bundle_registers_tier2_languages() {
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
