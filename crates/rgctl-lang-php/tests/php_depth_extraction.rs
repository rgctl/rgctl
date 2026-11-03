//! Golden correctness + unconditional AST coverage exercise for PHP.

use rgctl_lang_php::PhpPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn top_level_and_class_constants_plus_use() {
    let source = br#"<?php
namespace App;

use Vendor\Lib\Helper;

const MAX_RETRIES = 3;

class Order {
    public const STATUS_OPEN = 'open';
    private const FLAG = 1;

    public function run(): void {}
}
"#;
    let plugin = PhpPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("Order.php"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| { s.name == "MAX_RETRIES" && s.symbol_type == SymbolType::Variable }),
        "top-level const missing: {:?}",
        out.symbols
            .iter()
            .map(|s| (&s.name, &s.symbol_type))
            .collect::<Vec<_>>()
    );

    let order = out
        .symbols
        .iter()
        .find(|s| s.name == "Order" && s.symbol_type == SymbolType::Class)
        .expect("Order class");
    assert!(
        order.fields.iter().any(|f| f.name == "STATUS_OPEN"),
        "class const STATUS_OPEN missing: {:?}",
        order.fields
    );
    assert!(
        order.fields.iter().any(|f| f.name == "FLAG"),
        "class const FLAG missing: {:?}",
        order.fields
    );

    assert!(
        out.symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::Import && s.name.contains("Helper")),
        "namespace use Helper missing: {:?}",
        out.symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Import)
            .map(|s| &s.name)
            .collect::<Vec<_>>()
    );
}

#[test]
fn include_and_object_creation_relations() {
    let source = br#"<?php
namespace App;
class Factory {
    public function make() {
        include 'a.php';
        require_once "b.php";
        return new Order();
    }
}
class Order {}
"#;
    let plugin = PhpPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("Factory.php"), source)
        .expect("extract_all");
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::DependsOn && r.to.contains("a.php")),
        "include DependsOn missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Instantiates && r.to.contains("Order")),
        "object_creation Instantiates missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../php-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_php::LANGUAGE_PHP.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("php", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.php",
            br#"
<?php
declare(strict_types=1);
namespace App\Demo;
use App\Models\Order as Ord;
use Vendor\{Helper, Util as U};
const MAX = 10;
interface Repo { public function save(): void; }
trait Timestampable {
  use SoftDeletes { SoftDeletes::boot as private bootSoft; SoftDeletes::restore insteadof Other; }
}
enum Status: int { case Pending = 1; }
#[Route('/api')]
abstract final class Order extends Base implements Repo {
  public const LIMIT = 5;
  public static $shared;
  public string $name { get => $this->name; }
  public function __construct(public int $id, string $label = "", ...$extra) {}
  public function run(string $x, ...$rest): void {
    global $g;
    static $st = 1;
    $y = &$x;
    $x = 1;
    $x += 1;
    $x++;
    -$x;
    @$x;
    (int)$x;
    clone $this;
    $arr = [1, 2];
    $arr[0];
    $this->name;
    $this?->name;
    self::$shared;
    Order::LIMIT;
    $fn = function ($a) use ($x) { return $a; };
    $arrow = fn($a) => $a;
    echo $x;
    print $x;
    `ls`;
    include 'a.php';
    include_once 'b.php';
    require 'c.php';
    require_once 'd.php';
    if ($x) {} elseif ($y) {} else {}
    label:
    goto label;
    for ($i = 0, $j = 0; $i < 1; $i++) { break; }
    foreach ([] as $k => $v) { continue; }
    while (false) {}
    do { break; } while (false);
    switch ($x) { case 0: break; default: break; }
    match ($x) { 1 => 2, default => 0 };
    try {} catch (\Exception $e) {} finally {}
    yield 1;
    throw new \Exception();
    unset($x);
    exit(0);
    ;;;
    $cond = $x ? 1 : 2;
    foo();
    $this->run();
    Ord::staticCall();
    $obj?->method();
    return;
  }
}
function foo(int|string $v, ?Repo $r = null): void {}
"#,
        ),
        (
            "ast_coverage_heredoc.php",
            br#"
<?php
$s = <<<EOT
hello {$world}
EOT;
$n = <<<'NOW'
plain
NOW;
"#,
        ),
    ];

    let plugin = PhpPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active PHP AST kind in php-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("php unhandled hotspots: {hotspots:?}");
}
