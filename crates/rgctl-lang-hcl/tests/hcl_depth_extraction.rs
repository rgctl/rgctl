//! Golden correctness + unconditional AST coverage exercise for HCL.

use rgctl_lang_hcl::Plugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn resource_block_call_and_conditional() {
    let source = br#"
resource "aws_instance" "web" {
  count = var.enabled ? 1 : 0
  tags = merge(local.a, {})
}
"#;
    let plugin = Plugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.tf"), source)
        .expect("extract_all");

    assert!(
        out.symbols.iter().any(|s| {
            s.name == "resource.aws_instance.web" && s.symbol_type == SymbolType::Struct
        }),
        "resource block missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Calls && r.to == "merge"),
        "Calls to merge missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../hcl-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_hcl::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("hcl", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Corpus must exercise every non-Literal active kind in hcl-ast-coverage.json:
    // block, attribute, conditional, for_expr, function_call, get_attr,
    // template_directive, variable_expr.
    let sources: &[(&str, &[u8])] = &[(
        "ast_coverage_corpus.tf",
        br#"
# Coverage corpus for HCL Tier-1 AST handlers.
resource "aws_instance" "web" {
  count = var.enabled ? 1 : 0
  ami   = var.ami
  tags  = merge(local.base, { Name = var.name })
  ids   = [for x in var.list : upper(x) if x != ""]
  map   = { for k, v in var.map : k => v }
  nested = local.obj.attr
  msg    = "%{ if var.x }yes%{ else }no%{ endif }"
  loop   = "%{ for i in var.xs }${i}%{ endfor }"
  here   = <<-EOT
    %{ if true }hi%{ endif }
  EOT
  n   = null
  b   = true
  num = 42
  idx = var.list[0]
  leg = var.list.0
  splat = var.list[*].id
}
module "vpc" {
  source = "./vpc"
}
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
        .expect("every active HCL AST kind in hcl-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("hcl unhandled hotspots: {hotspots:?}");
}
