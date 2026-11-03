//! Golden correctness + unconditional AST coverage exercise for Puppet.

use rgctl_lang_puppet::PuppetPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn class_include_and_resource_relations() {
    let source = br#"
class profile::web inherits profile::base {
  include profile::base
  package { 'httpd':
    ensure => installed,
  }
}
"#;
    let plugin = PuppetPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("web.pp"), source)
        .expect("extract_all");

    assert!(
        out.symbols.iter().any(|s| {
            s.name.contains("profile::web")
                && matches!(
                    s.symbol_type,
                    SymbolType::Class | SymbolType::PuppetClass | SymbolType::Module
                )
        }),
        "class symbol missing: {:?}",
        out.symbols
            .iter()
            .map(|s| (&s.name, &s.symbol_type))
            .collect::<Vec<_>>()
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::IncludesClass),
        "include relation missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::InheritsClass),
        "inherits relation missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../puppet-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_puppet::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("puppet", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.pp",
            br#"
# Coverage corpus exercising every active Puppet AST handler.
type Myalias = Variant[String, Integer]

node 'web.example.com' {
  include profile::base
  require stdlib
  tag production, 'edge'
}

class profile::base (
  String $docroot = '/var/www',
) inherits profile::common {
  $pkg = 'httpd'
  $flag = !true
  $sum = 1 + 2
  $family = $facts.os

  if $flag and $sum > 0 {
    package { $pkg:
      ensure => installed,
    }
  } elsif $sum < 0 {
    notify { 'neg': }
  } else {
    notify { 'fallback': }
  }

  unless $flag {
    file { $docroot:
      ensure => directory,
    }
  }

  case $family {
    'RedHat': {
      $svc = 'httpd'
    }
    default: {
      $svc = 'apache2'
    }
  }

  $mode = $family ? {
    'RedHat' => '0755',
    default  => '0750',
  }

  File {
    owner => 'root',
    mode  => $mode,
  }

  File <| title == '/etc/hosts' |>

  Package['httpd'] -> Service['httpd']

  each($pkg) |$mnt| {
    notice($mnt)
  }

  lookup('web.healthcheck_cmd')
  profile::helpers::ok()
}

define profile::helpers::ok ($msg = 'ok') {
  notify { $msg: }
}

function profile::helpers::ping (String $host) >> Boolean {
  true
}
"#,
        ),
        (
            "lambda_corpus.pp",
            br#"
# Lambda attaches to function_call in expression position (not statement-level iterator).
$doubled = with(1) |$x| { notice($x) }
"#,
        ),
    ];

    let plugin = PuppetPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Puppet AST kind in puppet-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("puppet unhandled hotspots: {hotspots:?}");
}
