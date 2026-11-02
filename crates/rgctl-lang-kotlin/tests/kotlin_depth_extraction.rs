//! Golden correctness + Tier-1 Gold AST exercise gate for Kotlin.

use rgctl_lang_kotlin::KotlinPlugin;
use rgctl_plugin_api::{LanguagePlugin, SymbolType};
use std::path::Path;

#[test]
fn enum_entries_typealias_and_companion() {
    let source = br#"
enum class Status { PENDING, DONE }

typealias UserId = String

class Service {
  companion object {
    const val DEFAULT = 1
  }
}
"#;
    let plugin = KotlinPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.kt"), source)
        .expect("extract_all");

    let status = out
        .symbols
        .iter()
        .find(|s| s.name == "Status" && s.symbol_type == SymbolType::Enum)
        .expect("Status");
    assert!(
        status.fields.iter().any(|f| f.name == "PENDING"),
        "PENDING missing: {:?}",
        status.fields
    );
    assert!(status.fields.iter().any(|f| f.name == "DONE"));

    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "UserId" && s.symbol_type == SymbolType::TypeAlias),
        "typealias UserId missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols.iter().any(|s| s.name == "Companion"),
        "companion object missing"
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../kotlin-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_kotlin_ng::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("kotlin", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Corpus must exercise every non-passive kind in kotlin-ast-coverage.json.
    let sources: &[(&str, &[u8])] = &[(
        "ast_coverage_corpus.kt",
        br#"
@file:JvmName("Corpus")
package demo.corp

import kotlin.io.println
import kotlin.collections.List

typealias Id = Int

annotation class Tag(val value: String = "x")

enum class Color { RED, GREEN }

open class Base(open val seed: Int)

interface Named {
  fun label(): String
}

@Tag("svc")
class Widget<T : CharSequence, U>(
  val x: Int,
  private var y: Int = 0,
) : Base(x), Named by object : Named {
  override fun label(): String = "anon"
} where T : Comparable<T> {
  companion object {
    fun create(): Widget<String, Int> = Widget(0)
  }

  constructor() : this(0)

  init {
    println("init")
  }

  var z: Int = 0
    get() = field
    set(v) {
      field = v
    }

  val delegated by lazy { 1 }

  fun run(
    items: List<Int>,
    crossinline block: (Int) -> Unit,
  ): Int {
    val (a, b) = Pair(1, 2)
    val arr = intArrayOf(1, 2)
    val spread = intArrayOf(*arr)
    val lit = listOf(1, 2, 3)
    var i = 0
    i = a + b
    i += -1
    i = (i)
    this.z = i
    super.hashCode()
    val nav = this.z
    val idx = arr[0]
    val range = 0..10
    val inCheck = 1 in range
    val isCheck = items is List<*>
    val asCheck = items as List<Int>
    val infix = 1 to 2
    val ref = Widget<String, Int>::create
    val lambda = { n: Int -> n + 1 }
    val annotatedLambda: (@Tag("p") Int) -> Unit = { _: Int -> }
    val annotatedExpr = @Tag("e") i
    val anon = fun(n: Int): Int { return n }
    labeled@ for (item in items) {
      if (item > 0 && item < 10 || item == 0) {
        block(item)
        create()
        break@labeled
      } else {
        continue@labeled
      }
    }
    while (i < 5) {
      i++
    }
    do {
      i++
    } while (i < 8)
    when (x) {
      0 -> {}
      in 1..2 -> {}
      is Int -> {}
      else -> {}
    }
    try {
      throw Exception("boom")
    } catch (e: Exception) {
      println(e)
    } finally {
      println("done")
    }
    return lambda(idx) + nav + spread.size + lit.size + ref.hashCode() + anon(1) + infix.first + annotatedExpr
  }
}

object Singleton
"#,
    )];

    let plugin = KotlinPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Kotlin AST kind in kotlin-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("kotlin unhandled hotspots: {hotspots:?}");
}
