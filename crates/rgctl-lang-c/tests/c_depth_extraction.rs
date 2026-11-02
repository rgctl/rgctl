//! Golden correctness + unconditional AST coverage exercise for C.

use rgctl_lang_c::CPlugin;
use rgctl_plugin_api::{LanguagePlugin, SymbolType};
use std::path::Path;

#[test]
fn enum_union_and_preproc() {
    let source = br#"
enum Color { Red, Green = 2 };

union U { int i; float f; };

#define MAX 10
#define SQR(x) ((x)*(x))

struct Point { int x; int y; };
"#;
    let plugin = CPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.c"), source)
        .expect("extract_all");

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
        "union U missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    // `#define` macros are not indexed as graph Macro nodes (linux cold node cap).
    assert!(
        out.symbols
            .iter()
            .all(|s| s.symbol_type != SymbolType::Macro),
        "unexpected Macro symbols: {:?}",
        out.symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Macro)
            .map(|s| &s.name)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../c-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
    let tracker = rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("c", MANIFEST, &lang)
        .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Fixtures intentionally cover every active (non-passive) kind in
    // `c-ast-coverage.json`. Prefer cheap AstSkeleton/CfgStatement hits over
    // extra extract work — single_pass keeps ACTIVE_KINDS for symbols/calls/complexity.
    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.c",
            br#"
#include <stddef.h>
#include "local.h"
#pragma once
#line 1 "ast_coverage_corpus.c"

[[nodiscard]] int g_attr;

enum Color { Red, Green = 2 };
union U { int i; float f; };
struct Point { int x:3; int y; };

typedef int Id;
typedef int (*FP)(int);
typedef int ArrT[4];

void proto(int a, ...);
void __cdecl ms_fn(void);

void foo(int a, int *p, struct Point *pt, ...) {
  int arr[4];
  int (*fp)(int) = 0;
  int x = 0;
  x = 1;
  x++;
  --x;
  +x;
  -x;
  *p = 1;
  p = &x;
  x = x + 1, x = x - 1;
  x = x ? 1 : 2;
  x = (int)x;
  x = __extension__ (1);
  x = sizeof(x);
  x = sizeof(int);
  x = _Alignof(int);
  x = offsetof(struct Point, y);
  x = _Generic(x, int: 1, default: 0);
  x = arr[0];
  x = pt->x;
  x = (*pt).y;
  __asm__("nop");
  __asm__ volatile ("nop" : : : "memory");
  foo(a, p, pt);
  [[likely]] x = 1;
label:
  if (x) {} else {}
  for (;;) { break; }
  while (0) { continue; }
  do { break; } while (0);
  switch (x) { case 0: break; }
  goto label;
  __try { __leave; } __except(1) { }
  __try { } __finally { }
  return;
}

#if 0
#elif defined(FOO)
#elif 1
#else
#endif

#ifdef BAR
#elifdef BAZ
#else
#endif

#if defined(X)
#endif
"#,
        ),
        (
            "ast_coverage_ms_and_attrs.c",
            br#"
[[deprecated]] void attr_stmt_fn(void) {
  [[maybe_unused]] ;
  [[noreturn]] goto done;
done:
  return;
}

void __stdcall stdcall_fn(void) {}
void __fastcall fastcall_fn(void) {}
"#,
        ),
    ];

    let plugin = CPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active C AST kind in c-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("c unhandled hotspots: {hotspots:?}");
}
