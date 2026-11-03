//! Golden correctness + unconditional AST coverage exercise for Elixir.

use rgctl_lang_elixir::Plugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn module_function_call_and_alias_relations() {
    let source = br#"
defmodule Foo.Bar do
  alias Enum
  def run(x) do
    helper(x)
  end
  defp helper(x), do: x
end
"#;
    let plugin = Plugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("bar.ex"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::Module && s.name.contains("Foo")),
        "module symbol missing: {:?}",
        out.symbols
            .iter()
            .map(|s| (&s.name, &s.symbol_type))
            .collect::<Vec<_>>()
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::Function && s.name == "run"),
        "function run missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Calls && r.to.contains("helper")),
        "Calls to helper missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::References && r.to.contains("Enum")),
        "alias References to Enum missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../elixir-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_elixir::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("elixir", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.ex",
            br#"
# Coverage corpus exercising every active Elixir AST handler.
defmodule Demo.App do
  @moduledoc "demo"
  alias Demo.Helpers
  import Enum, only: [map: 2]
  require Logger
  use GenServer

  defstruct [:id, :name]

  def run(list, flag) when is_list(list) and flag do
    anon = fn
      0 -> :zero
      n when n > 0 -> :pos
      _ -> :neg
    end

    value = list[0]
    sum = 1 + 2
    ok? = true and false or true

    if ok? do
      Helpers.ping(value)
    else
      Logger.info("skip")
    end

    case anon.(sum) do
      :zero -> :ok
      other -> other
    end

    try do
      raise "boom"
    rescue
      e in RuntimeError -> {:error, e}
    catch
      :throw, val -> {:caught, val}
    else
      result -> result
    after
      :cleanup
    end

    ~w(a b c)
    ~r/foo/i
    'charlist'
    "string #{sum}"
    :atom
    nil
  end

  defp helper(x), do: x
end
"#,
        ),
        (
            "access_and_stab.ex",
            br#"
defmodule AccessDemo do
  def fetch(map, key) do
    map[key]
  end

  def multi do
    fn
      :a -> 1
      :b -> 2
    end
  end
end
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
        .expect("every active Elixir AST kind in elixir-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("elixir unhandled hotspots: {hotspots:?}");
}
