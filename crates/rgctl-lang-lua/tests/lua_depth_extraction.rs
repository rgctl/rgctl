//! Golden correctness + unconditional AST coverage exercise for Lua.

use rgctl_lang_lua::Plugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn function_call_and_method_names() {
    let source = br#"
local M = {}
function M.foo(x)
  bar(x)
  return x
end
function M:baz(y)
  return y
end
"#;
    let plugin = Plugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.lua"), source)
        .expect("extract_all");

    assert!(
        out.symbols.iter().any(|s| {
            s.symbol_type == SymbolType::Function && (s.name == "M.foo" || s.name.ends_with(".foo"))
        }),
        "M.foo missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Calls),
        "Calls relation missing"
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../lua-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_lua::LANGUAGE.into();
    let tracker = rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("lua", MANIFEST, &lang)
        .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Corpus must exercise every non-Literal active kind in lua-ast-coverage.json.
    let sources: &[(&str, &[u8])] = &[(
        "ast_coverage_corpus.lua",
        br#"
#!/usr/bin/env lua
-- Coverage corpus exercising every active Lua AST handler.
local a <close> = 1
local function lf(x, ...)
  return x
end
local M = {}
function M.foo(self)
end
function M:bar()
  local t = { [1] = true, name = "hi\n", nested = { false } }
  local v = t.name
  local w = t[1]
  local m = self:bar()
  if a then
    print(1)
  elseif not a then
    goto lbl
  else
    ;
  end
  while a do
    break
  end
  repeat
    a = a + 1
  until a > 10
  for i = 1, 10, 2 do
  end
  for k, v in pairs(t) do
  end
  do
    local x = (a)
  end
  ::lbl::
  return nil, ...
end
M.baz = function() end
global *
global <close> *
"#,
    )];

    let plugin = Plugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Lua AST kind in lua-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("lua unhandled hotspots: {hotspots:?}");
}
