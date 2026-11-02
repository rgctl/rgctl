//! Golden correctness + unconditional AST coverage exercise for C++.

use rgctl_lang_cpp::CppPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn namespace_alias_concept_enum_union_preproc() {
    let source = br#"
namespace N {
  void f();
}

using IntVec = std::vector<int>;
namespace NA = std;

template<typename T>
concept Addable = requires(T a, T b) { a + b; };

enum Color { Red, Green = 2 };

union U { int i; float f; };

#define MAX 10
#define SQR(x) ((x)*(x))

using std::vector;

class Base {};
class Derived : public Base {
  void run() {
    auto* p = new Derived();
    auto x = foo<int>();
  }
};
"#;
    let plugin = CppPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.cpp"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "N" && s.symbol_type == SymbolType::Module),
        "namespace N missing"
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| { s.name == "IntVec" && s.symbol_type == SymbolType::TypeAlias }),
        "alias IntVec missing"
    );
    assert!(
        out.symbols.iter().any(|s| {
            s.name == "NA"
                && s.metadata
                    .get("kind")
                    .and_then(|v| v.as_str())
                    == Some("namespace_alias_definition")
        }),
        "namespace alias NA missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols.iter().any(|s| {
            s.name == "Addable"
                && s.metadata.get("kind").and_then(|v| v.as_str()) == Some("concept_definition")
        }),
        "concept Addable missing"
    );
    let color = out
        .symbols
        .iter()
        .find(|s| s.name == "Color" && s.symbol_type == SymbolType::Enum)
        .expect("Color");
    assert!(color.fields.iter().any(|f| f.name == "Red"));
    assert!(color.fields.iter().any(|f| f.name == "Green"));
    assert!(
        out.symbols.iter().any(|s| {
            s.name == "U"
                && s.metadata.get("kind").and_then(|v| v.as_str()) == Some("union_specifier")
        }),
        "union U missing"
    );
    // `#define` macros are not indexed as graph Macro nodes (linux cold node cap).
    assert!(
        out.symbols
            .iter()
            .all(|s| s.symbol_type != SymbolType::Macro),
        "unexpected Macro symbols"
    );
    assert!(
        out.relations.iter().any(|r| {
            r.relation_type == RelationType::Uses
                && r.metadata.get("kind").and_then(|v| v.as_str()) == Some("using_declaration")
        }),
        "using Uses missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Extends && r.to.contains("Base")),
        "Extends Base missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Instantiates && r.to.contains("Derived")),
        "Instantiates Derived missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| { r.relation_type == RelationType::Calls && r.to.contains("foo") }),
        "template_function call foo missing: {:?}",
        out.relations
    );
}

#[test]
fn enumerator_as_symbol_kind_visited() {
    // Coverage marks enumerator as Symbol; fields on the enum cover the semantic extract.
    let source = b"enum E { A, B };\n";
    let plugin = CppPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("e.cpp"), source)
        .expect("extract_all");
    let e = out.symbols.iter().find(|s| s.name == "E").expect("E");
    assert_eq!(e.fields.len(), 2);
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../cpp-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("cpp", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Corpus must exercise every active (non-passive) kind in cpp-ast-coverage.json.
    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.cpp",
            br#"
#include <vector>
#include <cstddef>
#pragma once
[[nodiscard]] int marked();

namespace ns {
  using Alias = int;
  namespace NA = std;
  concept Integral = true;
  enum Color { Red };
  union U { int i; float f; };
  typedef struct TS { int x; } TSAlias;
  struct S {
    int a : 3;
    virtual void pure() = 0;
    void defed() = default;
    void deleted() = delete;
    friend class Color;
    S() {}
    virtual ~S() = 0;
  };
  template<template<typename> class TT, typename T = int, typename... Ts>
  requires true
  class Widget : public S {
    template<typename U>
    void tm();
    void run(int p = 1, ...) {
      int x = 0;
      int *ptr = &x;
      x = 1;
      x = (x);
      x = (int)ptr;
      x = x + 1;
      x = -x + !x;
      x = x ? 1 : 2;
      x = (1, 2);
      x = sizeof(x);
      x = alignof(int);
      x = offsetof(TS, x);
      x = *ptr;
      x = ptr[0];
      x = this->a;
      x++;
      --x;
      [[nodiscard]] return;
    label:
      goto label;
      if (int y = 1; y) {} else {}
      for (int i = 0; i < 1; i++) { break; }
      for (auto& v : std::vector<int>{}) { continue; }
      while (0) { break; }
      do { break; } while (0);
      switch (x) { case 0: break; }
      try { throw 1; } catch (...) {}
      co_await 1;
      co_yield 1;
      co_return;
      auto* p = new int;
      delete p;
      delete[] p;
      auto lam = [=, &x, a = 1](int n) { return n + x + a; };
      auto fold = []<typename... Args>(Args... args) { return (... + args); };
      asm("nop");
      x = __extension__ ({ 1; });
      this->tm<int>();
      return;
    }
  };
  template<typename T>
  requires requires(T t) { t + t; }
  T add(T a, T b) { return a + b; }
}

static_assert(true);

#if 0
int dead = 1;
#else
int live = 1;
#endif

#ifdef FOO
int fooed = 1;
#endif

#line 1

void seh_demo() {
  __try {
    __leave;
  } __except(1) {
  }
  __try {
  } __finally {
  }
}
"#,
        ),
        (
            "ast_coverage_c_generics.cpp",
            // tree-sitter-cpp still emits C-family kinds used in mixed headers.
            br#"
int g(int x) {
  return _Generic(x, int: 1, default: 0);
}
"#,
        ),
    ];

    let plugin = CppPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active C++ AST kind in cpp-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("cpp unhandled hotspots: {hotspots:?}");
}
