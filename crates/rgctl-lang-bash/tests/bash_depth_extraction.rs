//! Golden correctness + unconditional AST coverage exercise for Bash.

use rgctl_lang_bash::Plugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn function_call_and_declaration_symbols() {
    let source = br#"
demo() {
  declare -i COUNT=0
  helper
}
helper() { :; }
"#;
    let plugin = Plugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("demo.sh"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::Function && s.name == "demo"),
        "demo function missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::Variable && s.name == "COUNT"),
        "COUNT variable missing"
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Calls && r.to == "helper"),
        "Calls to helper missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../bash-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("bash", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Corpus must exercise every non-Literal / non-Skip* active kind in bash-ast-coverage.json.
    let sources: &[(&str, &[u8])] = &[(
        "ast_coverage_corpus.sh",
        br#"
#!/usr/bin/env bash
# Coverage corpus for every active Bash AST handler.

declare -a ARR=(1 2 3)
export PATH="/usr/bin:$PATH"
readonly FLAG=1

cover() {
  local x=1
  unset FLAG
  x=$((x + 1))
  : ${x:-default} $x
  echo {1..3}

  if [[ $x -gt 0 ]]; then
    echo ok
  elif (( x < 10 )); then
    : $(( x > 0 ? 1 : 0 ))
    : $((x++))
  else
    ! false
  fi

  while [[ -n $x ]]; do
    echo "$x"
    x=$((x - 1))
  done

  for i in a b; do
    echo "$i"
  done

  for ((i = 0; i < 3; i++)); do
    echo "$i"
  done

  case $x in
    a*|b) echo arm ;;
    *) echo other ;;
  esac

  echo hello | grep e || true && false
  ( cd /tmp && pwd )
  { echo compound; }
  echo redirected > /tmp/rgctl_bash_out 2>&1
  echo "$(echo sub)"
  [[ -f /tmp ]] && test -z ""
  [[ $1 == @(foo|bar) ]]
}

helper() { :; }
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
        .expect("every active Bash AST kind in bash-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("bash unhandled hotspots: {hotspots:?}");
}
