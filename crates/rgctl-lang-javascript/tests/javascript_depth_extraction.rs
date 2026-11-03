//! Golden correctness + unconditional AST coverage exercise for JavaScript.

use rgctl_lang_javascript::JavaScriptPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn generators_exports_new_and_decorators() {
    let source = br#"
function* gen() {
  yield 1;
}

const g = function* () {
  yield 2;
};

export { foo as bar } from './mod';

@controller
class Service {
  @get
  list() {
    return new Order();
  }
}

class Order {}
"#;
    let plugin = JavaScriptPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.js"), source)
        .expect("extract_all");

    assert!(
        out.symbols.iter().any(|s| s.name == "gen"),
        "generator gen missing"
    );
    assert!(
        out.symbols.iter().any(|s| s.name == "g"),
        "bound generator g missing: {:?}",
        out.symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .map(|s| &s.name)
            .collect::<Vec<_>>()
    );
    assert!(
        out.relations.iter().any(|r| {
            r.relation_type == RelationType::Uses
                && r.to == "bar"
                && r.metadata.get("kind").and_then(|v| v.as_str()) == Some("export_specifier")
        }),
        "export specifier missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Instantiates && r.to == "Order"),
        "Instantiates Order missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| { r.relation_type == RelationType::AnnotatedWith && r.to == "controller" }),
        "class decorator missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../javascript-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("javascript", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Corpus must exercise every non-Literal active kind in javascript-ast-coverage.json.
    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.js",
            br#"
#!/usr/bin/env node
<!-- html comment -->
import def, { named as n } from "./m" with { type: "json" };
import * as ns from "./ns";
import "./side";
export { foo as bar } from "./m";
export * as ns2 from "./m2";
export default class Order {}
export function* gen() { yield 1; }

const { a = 1, b: c, ...rest } = { a: 1, b: 2, d: 3 };
const [x, y = 2, ...zs] = [1, 2, 3];
var legacy = 0;
let mutable = 1;
const arrow = (p, ...more) => p;
const fexpr = function (q) { return q; };
const gexpr = function* () { yield 3; };
function* g2() { yield 2; }

@dec
class Base {
  static { this.ready = true; }
  #priv = 1;
  field = 2;
  @get
  method() {
    return this.#priv;
  }
}
class Child extends Base {
  constructor() {
    super();
    this.x = 1;
  }
}
const Anon = class {
  run() {
    return 1;
  }
};

function demo(obj, arr) {
  debugger;
  ;
  label: while (false) {
    break label;
  }
  with (obj) {
    mutable++;
  }
  do {
    continue;
  } while (false);
  for (let i = 0; i < 1; i++) {
    if (i) {
    } else {
      empty();
    }
  }
  for (const k in obj) {
    void k;
  }
  switch (mutable) {
    case 0:
      break;
    default:
      break;
  }
  try {
    throw new Error("x");
  } catch (e) {
    return e;
  } finally {
    mutable += 1;
  }
  const z = a + b;
  const u = -a;
  const v = ++mutable;
  const seq = (a, b);
  const tern = a ? b : c;
  const mem = obj.prop;
  const sub = arr[0];
  const opt = obj?.prop;
  const spread = { ...obj, k: 1 };
  const tmpl = `hi ${mutable}`;
  const re = /foo/gi;
  using resource = getResource();
  await Promise.resolve(1);
  import("./dyn");
  new.target;
  import.meta.url;
  return;
}

async function go() {
  await demo({}, []);
}

function empty() {}
function getResource() { return { [Symbol.dispose]() {} }; }
"#,
        ),
        (
            "ast_coverage_jsx.jsx",
            br#"
function View(props) {
  return (
    <div className="wrap" ns:attr="v">
      text &amp; more
      <Child value={props.x} />
      <>
        <span>{props.y}</span>
      </>
    </div>
  );
}
"#,
        ),
    ];

    let plugin = JavaScriptPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker.assert_all_exercised().expect(
        "every active JavaScript AST kind in javascript-ast-coverage.json must be exercised",
    );
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("javascript unhandled hotspots: {hotspots:?}");
}
