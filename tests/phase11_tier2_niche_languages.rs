//! Phase 11.1.2 — Tier 2 niche language plugin tests (bundle-extra)
#![allow(dead_code, unused_imports, unused_macros)]

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

#[cfg(feature = "bundle-extra")]
#[test]
fn test_ocaml_plugin() {
    assert_extracts_function("ocaml", "test.ml", "let add x y = x + y", "add");
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_perl_plugin() {
    assert_extracts_function(
        "perl",
        "test.pl",
        "sub add { my ($a, $b) = @_; return $a + $b; }",
        "add",
    );
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_fortran_plugin() {
    assert_extracts_function(
        "fortran",
        "test.f90",
        "function add(a, b)\n  integer :: a, b\n  add = a + b\nend function add",
        "add",
    );
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_verilog_plugin() {
    assert_extracts_function(
        "verilog",
        "test.v",
        "module counter;\n  function integer add;\n    input a, b;\n    add = a + b;\n  endfunction\nendmodule",
        "add",
    );
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_vhdl_plugin() {
    assert_extracts_function("vhdl", "test.vhd", "entity top is\nend top;", "top");
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_pascal_plugin() {
    assert_extracts_function(
        "pascal",
        "test.pas",
        "function Add(a, b: Integer): Integer;\nbegin\n  Add := a + b;\nend;",
        "Add",
    );
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_scheme_plugin() {
    let registry = LanguageRegistry::new();
    let plugin = registry.get_language_plugin("scheme").unwrap();
    let source = "(define (add a b) (+ a b))";
    let symbols = plugin
        .extract_symbols(Path::new("test.scm"), source.as_bytes())
        .unwrap();
    assert!(symbols.iter().any(|s| s.name == "add"));
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_zig_plugin() {
    assert_extracts_function(
        "zig",
        "test.zig",
        "fn add(a: i32, b: i32) i32 { return a + b; }",
        "add",
    );
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_fsharp_plugin() {
    assert_extracts_function("fsharp", "test.fs", "let add a b = a + b", "add");
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_crystal_plugin() {
    assert_extracts_function(
        "crystal",
        "test.cr",
        "def add(a : Int32, b : Int32)\n  a + b\nend",
        "add",
    );
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_clojure_plugin() {
    let registry = LanguageRegistry::new();
    let plugin = registry.get_language_plugin("clojure").unwrap();
    let source = "(defn add [a b] (+ a b))";
    let symbols = plugin
        .extract_symbols(Path::new("test.clj"), source.as_bytes())
        .unwrap();
    assert!(symbols.iter().any(|s| s.name == "add"));
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_cobol_plugin() {
    let registry = LanguageRegistry::new();
    let plugin = registry.get_language_plugin("cobol").unwrap();
    let source = "       MAIN-PARA.\n           DISPLAY 'HELLO'.\n       END-PARA.";
    let symbols = plugin
        .extract_symbols(Path::new("test.cob"), source.as_bytes())
        .unwrap();
    assert!(symbols.iter().any(|s| s.name == "MAIN-PARA"));
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_assembly_plugin() {
    let registry = LanguageRegistry::new();
    let plugin = registry.get_language_plugin("assembly").unwrap();
    let source = "_start:\n    mov rax, 1\n    ret\n";
    let symbols = plugin
        .extract_symbols(Path::new("test.s"), source.as_bytes())
        .unwrap();
    assert!(symbols.iter().any(|s| s.name == "_start"));
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_extra_bundle_registers_niche_languages() {
    let registry = LanguageRegistry::new();
    let ids: Vec<_> = registry
        .supported_languages()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    for lang in [
        "ocaml", "perl", "fortran", "verilog", "vhdl", "cobol", "pascal", "scheme", "zig",
        "fsharp", "crystal", "clojure", "assembly",
    ] {
        assert!(ids.contains(&lang.to_string()), "missing language: {lang}");
    }
}
