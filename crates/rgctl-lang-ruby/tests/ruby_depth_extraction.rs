//! Golden correctness + unconditional AST coverage exercise for Ruby.

use rgctl_lang_ruby::RubyPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn attrs_ivars_lambda_and_superclass() {
    let source = br#"
class Animal
end

class Dog < Animal
  attr_reader :name
  attr_accessor :age

  def initialize(name)
    @name = name
    @breed = "lab"
  end

  def transform
    ->(x) { x }
  end
end
"#;
    let plugin = RubyPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("dog.rb"), source)
        .expect("extract_all");

    let dog = out
        .symbols
        .iter()
        .find(|s| s.name == "Dog" && s.symbol_type == SymbolType::Class)
        .expect("Dog");
    assert!(
        dog.fields.iter().any(|f| f.name == "name"),
        "attr_reader name missing: {:?}",
        dog.fields
    );
    assert!(
        dog.fields.iter().any(|f| f.name == "age"),
        "attr_accessor age missing: {:?}",
        dog.fields
    );
    assert!(
        dog.fields.iter().any(|f| f.name == "breed"),
        "@breed ivar missing: {:?}",
        dog.fields
    );

    assert!(
        out.relations.iter().any(|r| {
            r.relation_type == RelationType::Extends
                && r.to.contains("Animal")
                && r.from.contains("Dog")
        }),
        "superclass Extends missing: {:?}",
        out.relations
    );

    assert!(
        out.symbols.iter().any(|s| {
            s.symbol_type == SymbolType::Function
                && s.metadata.get("kind").and_then(|v| v.as_str()) == Some("lambda")
        }),
        "lambda symbol missing: {:?}",
        out.symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .map(|s| &s.name)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../ruby-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_ruby::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("ruby", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.rb",
            br#"
# Coverage corpus exercising every active Ruby AST handler.
module M; end
class Animal; end
class Dog < Animal
  attr_reader :name
  attr_writer :age
  attr_accessor :color
  alias bark woof

  def initialize(a, *rest)
    @name = a
    x = 1
    x += 1
    y = a ? 1 : 2
    (p, q) = [1, 2]
    r, s = 1, 2
    *t = 1, 2
    u = [1, 2]
    u[0]
    v = 1..2
    w = !true
    w = -1
    if true
    elsif false
    else
    end
    unless false
    end
    while false
      break
      next
      redo
    end
    until true
    end
    for i in 1..1
      break
    end
    begin
      raise "x"
    rescue StandardError => e
      retry
    ensure
      nil
    end
    case 1
    when 1 then nil
    else nil
    end
    case 1
    in Integer => n if n > 0
      n
    in [a, b]
      a
    in [*, 1, *]
      1
    in {k:, **nil}
      k
    in A | B
      1
    in ^(1 + 1)
      1
    in ^@name
      1
    end
    1 => Integer
    1 in Integer
    lambda { |y| y }
    ->(z) { z }
    ping()
    yield
    return
    `echo hi`
    undef ping
  end

  def woof; end
  def self.ping; end

  class << self
    def class_side; end
  end
end

BEGIN { }
END { }
;
top if true
top unless false
top while false
top until true
top rescue nil
(1; 2)
mapped = [1].map do |n|
  n
end
braced = [1].map { |n| n }
"#,
        ),
        (
            "heredoc_corpus.rb",
            br#"
def say
  <<~MSG
    hello
  MSG
end
"#,
        ),
    ];

    let plugin = RubyPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Ruby AST kind in ruby-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("ruby unhandled hotspots: {hotspots:?}");
}
