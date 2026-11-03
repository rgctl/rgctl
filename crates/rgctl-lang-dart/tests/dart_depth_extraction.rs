//! Golden correctness + unconditional AST coverage exercise for Dart.

use rgctl_lang_dart::Plugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn class_method_call_and_import() {
    let source = br#"
import 'dart:core';

class Foo {
  void bar() {
    baz();
  }
}

void baz() {}
"#;
    let plugin = Plugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.dart"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "Foo" && s.symbol_type == SymbolType::Class),
        "class Foo missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "bar" && s.symbol_type == SymbolType::Function),
        "method bar missing"
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Calls),
        "Calls missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../dart-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_dart::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("dart", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Corpus must exercise every non-Literal active kind in dart-ast-coverage.json.
    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.dart",
            br#"
/// docs
import 'dart:async' as async show Future;
export 'dart:core' show Object;
part 'part.dart';

typedef IntList = List<int>;
int topLevel = 1;
int get topGetter => 1;
void set topSetter(int v) {}

external void externalFn();
external int get externalGetter;
external set externalSetter(int v);
external int externalField;

enum Color { red, green }

mixin M on Object {
  void m() {}
}

class Base<T extends Object> {
  final int x;
  Base(this.x);
  Base.named(this.x);
  const Base.constNamed(this.x);
  factory Base.fact(int x) => Base.named(x);
  factory Base.redirect(int x) = Other;
  int operator +(Base o) => x + o.x;
  int get value => x;
  set value(int v) {}
  static final int k = 1;
}

class Other extends Base with M implements Comparable<Other> {
  Other(super.x);
  @Deprecated('x')
  void run(List<int>? xs, {required int n, int m = 0}) {
    ;
    label: while (false) {
      break label;
    }
    do {
      continue;
    } while (false);
    for (var i = 0; i < 1; i++) {
      if (i == 0 && n > 0 || m != 0) {
      } else {
        empty();
      }
    }
    try {
      assert(n > 0);
      throw Exception('x');
    } on Exception catch (e) {
      rethrow;
    } finally {
      n;
    }
    switch (n) {
      case 0:
        break;
      default:
        break;
    }
    final sw = switch (n) {
      1 => 'one',
      _ => 'other',
    };
    final a = 1 + 2 * 3;
    final b = 1 & 2 | 3 ^ 4;
    final c = 1 << 2;
    final d = a == b;
    final e = a < b;
    final f = true ? a : b;
    final g = a ?? b;
    final h = -a;
    final i2 = a!;
    a++;
    final j = xs?[0];
    final k2 = xs?.length;
    final mem = this.x;
    final idx = xs![0];
    final lit = const Object();
    final neu = new Object();
    final tear = Object.new;
    Object();
    print(List<int>);
    final casc = StringBuffer()
      ..write('a')
      ..write('b');
    dynamic dyn;
    dyn
      ..b.c
      ..b[0]
      ..b?.c
      ..b?[0]
      ..b.c()
      ..len!;
    (a);
    var local = 1;
    local = 2;
    var (p1, p2) = (1, 2);
    (p1, p2) = (3, 4);
    final listPat = switch ([1, 2, 3]) {
      [var head, ...var rest] => head,
      _ => 0,
    };
    final mapPat = switch ({'a': 1}) {
      {'a': var v} => v,
      _ => 0,
    };
    final objPat = switch (Base(1)) {
      Base(x: var xx) => xx,
      _ => 0,
    };
    final recPat = switch ((1, y: 2)) {
      (var x, y: var y) => x + y,
      _ => 0,
    };
    final castPat = switch (1 as Object) {
      var z as int => z,
      _ => 0,
    };
    final nullChk = switch (xs) {
      List<int> l? => l.length,
      _ => 0,
    };
    final nullAssert = switch (xs) {
      List<int> l! => l.length,
      _ => 0,
    };
    final constPat = switch (1) {
      1 => true,
      _ => false,
    };
    if (a is int) {}
    if (a as int > 0) {}
    void localFn(int z) {
      return;
    }
    localFn(1);
    final lambda = (int z) => z;
    lambda(2);
    await Future.value(1);
  }

  Stream<int> gen() async* {
    yield 1;
    yield* Stream.empty();
  }
}

class C = Object with M;

extension E on int {
  int doubled() => this * 2;
}

extension type Id(int value) {
  int get v => value;
}

void empty() {}
"#,
        ),
        (
            "part.dart",
            br#"
part of 'ast_coverage_corpus.dart';

void partHelper() {}
"#,
        ),
    ];

    let plugin = Plugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Dart AST kind in dart-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("dart unhandled hotspots: {hotspots:?}");
}
