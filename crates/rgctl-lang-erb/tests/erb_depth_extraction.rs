//! Golden correctness + unconditional AST coverage exercise for ERB.

use rgctl_lang_erb::ErbPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn output_and_variable_relations() {
    let source = b"<h1><%= @hostname %></h1>\n<% if @enabled %>\nok\n<% end %>";
    let plugin = ErbPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("templates/index.html.erb"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| s.name.contains("erb:output") && s.symbol_type == SymbolType::Variable),
        "output symbol missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name.contains("erb:code") && s.symbol_type == SymbolType::Variable),
        "code/directive symbol missing"
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::UsesVariable && r.to == "$hostname"),
        "UsesVariable $hostname missing: {:?}",
        out.relations
    );
}

#[test]
fn graphql_directive_emits_property_symbol() {
    let source = b"<%graphql\n  query {\n    host { name }\n  }\n%>\n";
    let plugin = ErbPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("templates/gql.erb"), source)
        .expect("extract_all");

    let gql = out
        .symbols
        .iter()
        .find(|s| s.name.contains("erb:graphql"))
        .expect("graphql symbol");
    assert_eq!(
        gql.metadata.get("handler").and_then(|v| v.as_str()),
        Some("Property")
    );
    assert!(
        gql.metadata
            .get("graphql_body")
            .and_then(|v| v.as_str())
            .is_some_and(|b| b.contains("query")),
        "graphql_body missing: {:?}",
        gql.metadata
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../erb-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_embedded_template::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("erb", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let sources: &[(&str, &[u8])] = &[(
        "ast_coverage_corpus.erb",
        br#"
# Coverage corpus exercising every active ERB AST handler.
<html>
  <head><%= @title %></head>
  <body>
    <%# banner comment %>
    <% if @enabled %>
      <p><%= @hostname %></p>
    <% else %>
      <p>disabled</p>
    <% end %>
    <%graphql
      query Host($id: ID!) {
        host(id: $id) { name fqdn }
      }
    %>
  </body>
</html>
"#,
    )];

    let plugin = ErbPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active ERB AST kind in erb-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("erb unhandled hotspots: {hotspots:?}");
}
