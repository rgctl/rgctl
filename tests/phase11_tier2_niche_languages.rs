//! Phase 11.1.2 — Tier 2 niche language plugin tests (bundle-extra)

use rbuilder::languages::generic::{RegexLanguagePlugin, TreeSitterLanguagePlugin};
use rbuilder::languages::plugin_trait::LanguagePlugin;
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

#[cfg(feature = "lang-ocaml")]
#[test]
fn test_ocaml_plugin() {
    assert_extracts_function(
        "ocaml",
        || tree_sitter_ocaml::LANGUAGE_OCAML.into(),
        "test.ml",
        "let add x y = x + y",
        "add",
    );
}

#[cfg(feature = "lang-perl")]
#[test]
fn test_perl_plugin() {
    assert_extracts_function(
        "perl",
        || ts_parser_perl::LANGUAGE.into(),
        "test.pl",
        "sub add { my ($a, $b) = @_; return $a + $b; }",
        "add",
    );
}

#[cfg(feature = "lang-fortran")]
#[test]
fn test_fortran_plugin() {
    assert_extracts_function(
        "fortran",
        || tree_sitter_fortran::LANGUAGE.into(),
        "test.f90",
        "function add(a, b)\n  integer :: a, b\n  add = a + b\nend function add",
        "add",
    );
}

#[cfg(feature = "lang-verilog")]
#[test]
fn test_verilog_plugin() {
    assert_extracts_function(
        "verilog",
        || tree_sitter_verilog::LANGUAGE.into(),
        "test.v",
        "module counter;\n  function integer add;\n    input a, b;\n    add = a + b;\n  endfunction\nendmodule",
        "add",
    );
}

#[cfg(feature = "lang-vhdl")]
#[test]
fn test_vhdl_plugin() {
    assert_extracts_function(
        "vhdl",
        || tree_sitter_vhdl::LANGUAGE.into(),
        "test.vhd",
        "entity top is\nend top;",
        "top",
    );
}

#[cfg(feature = "lang-pascal")]
#[test]
fn test_pascal_plugin() {
    assert_extracts_function(
        "pascal",
        || tree_sitter_pascal::LANGUAGE.into(),
        "test.pas",
        "function Add(a, b: Integer): Integer;\nbegin\n  Add := a + b;\nend;",
        "Add",
    );
}

#[cfg(feature = "lang-scheme")]
#[test]
fn test_scheme_plugin() {
    let plugin = RegexLanguagePlugin::new("scheme").unwrap();
    let source = "(define (add a b) (+ a b))";
    let symbols = plugin
        .extract_symbols(Path::new("test.scm"), source.as_bytes())
        .unwrap();
    assert!(symbols.iter().any(|s| s.name == "add"));
}

#[cfg(feature = "lang-zig")]
#[test]
fn test_zig_plugin() {
    assert_extracts_function(
        "zig",
        || tree_sitter_zig::LANGUAGE.into(),
        "test.zig",
        "fn add(a: i32, b: i32) i32 { return a + b; }",
        "add",
    );
}

#[cfg(feature = "lang-fsharp")]
#[test]
fn test_fsharp_plugin() {
    assert_extracts_function(
        "fsharp",
        || tree_sitter_fsharp::LANGUAGE_FSHARP.into(),
        "test.fs",
        "let add a b = a + b",
        "add",
    );
}

#[cfg(feature = "lang-crystal")]
#[test]
fn test_crystal_plugin() {
    assert_extracts_function(
        "crystal",
        || tree_sitter_crystal::LANGUAGE.into(),
        "test.cr",
        "def add(a : Int32, b : Int32)\n  a + b\nend",
        "add",
    );
}

#[cfg(feature = "lang-clojure")]
#[test]
fn test_clojure_plugin() {
    let plugin = RegexLanguagePlugin::new("clojure").unwrap();
    let source = "(defn add [a b] (+ a b))";
    let symbols = plugin
        .extract_symbols(Path::new("test.clj"), source.as_bytes())
        .unwrap();
    assert!(symbols.iter().any(|s| s.name == "add"));
}

#[cfg(feature = "lang-cobol")]
#[test]
fn test_cobol_plugin() {
    let plugin = RegexLanguagePlugin::new("cobol").unwrap();
    let source = "       MAIN-PARA.\n           DISPLAY 'HELLO'.\n       END-PARA.";
    let symbols = plugin
        .extract_symbols(Path::new("test.cob"), source.as_bytes())
        .unwrap();
    assert!(symbols.iter().any(|s| s.name == "MAIN-PARA"));
}

#[cfg(feature = "lang-assembly")]
#[test]
fn test_assembly_plugin() {
    let plugin = RegexLanguagePlugin::new("assembly").unwrap();
    let source = "_start:\n    mov rax, 1\n    ret\n";
    let symbols = plugin
        .extract_symbols(Path::new("test.s"), source.as_bytes())
        .unwrap();
    assert!(symbols.iter().any(|s| s.name == "_start"));
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_extra_bundle_registers_niche_languages() {
    use rbuilder::languages::registry::LanguageRegistry;

    let registry = LanguageRegistry::new();
    let ids: Vec<_> = registry
        .supported_languages()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    for lang in [
        "ocaml",
        "perl",
        "fortran",
        "verilog",
        "vhdl",
        "cobol",
        "pascal",
        "scheme",
        "zig",
        "fsharp",
        "crystal",
        "clojure",
        "assembly",
    ] {
        assert!(ids.contains(&lang.to_string()), "missing language: {lang}");
    }
}
