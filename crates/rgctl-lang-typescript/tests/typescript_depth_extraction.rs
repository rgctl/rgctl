//! Golden correctness for OpenSpec 3.3 TypeScript AST depth.

use rgctl_lang_typescript::TypeScriptPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn abstract_class_fields_methods_and_namespace() {
    let source = br#"
abstract class Base {
  abstract run(): void;
  host: string = "localhost";
}

namespace Utils {
  export function helper() {}
}

function* gen() {
  yield 1;
}

const g = function* () {
  yield 2;
};
"#;
    let plugin = TypeScriptPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.ts"), source)
        .expect("extract_all");

    let base = out
        .symbols
        .iter()
        .find(|s| s.name == "Base" && s.symbol_type == SymbolType::Class)
        .expect("Base");
    assert!(base.fields.iter().any(|f| f.name == "host"));

    assert!(
        out.symbols.iter().any(|s| {
            s.name == "run"
                && s.qualified_name.as_deref() == Some("Base.run")
                && s.metadata.get("is_abstract").and_then(|v| v.as_bool()) == Some(true)
        }),
        "abstract run missing: {:?}",
        out.symbols
            .iter()
            .map(|s| (&s.name, &s.qualified_name))
            .collect::<Vec<_>>()
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "Utils" && s.symbol_type == SymbolType::Module),
        "namespace Utils missing"
    );
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
}

#[test]
fn export_specifier_and_instantiates() {
    let source = br#"
export { foo as bar } from './mod';

class OrderDto {}
class Service {
  make(): OrderDto {
    return new OrderDto();
  }
}
"#;
    let plugin = TypeScriptPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("svc.ts"), source)
        .expect("extract_all");

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
            .any(|r| r.relation_type == RelationType::Instantiates && r.to == "OrderDto"),
        "Instantiates OrderDto missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../typescript-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("typescript", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Comprehensive corpus covering every active (non-passive) TypeScript AST kind.
    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.ts",
            br#"
#! /usr/bin/env node
import type { Kind } from "./kinds";
import foo, { bar as baz, type T } from "./mod" with { type: "json" };
import * as ns from "./ns";
import req = require("./cjs");
import Alias = ns.Inner;

export { baz as exported };
export * as nsExport from "./ns";
export default class Def {};
export type { Kind };

declare const ambient: number;
declare function ambientFn(x: string): void;
declare module "ambient-mod" {
  export const z: number;
}

type Mapped<T> = { -readonly [K in keyof T]+?: T[K] };
type Sat = { a: 1 };
const sat = { a: 1 as const } satisfies Sat;
const cast = (0 as unknown) as number;
const bang = (null as any)!;
function id<T>(x: T): T {
  return x;
}
const instantiated = id<number>;

enum Color {
  Red = 1,
  Green,
}

function Marker(_t: unknown) {}

const Dyn = class {
  ping() {
    return import("./lazy");
  }
};
const fe = function namedFe() {
  return 0;
};

@Marker
abstract class Base extends Object implements Kind {
  abstract run(): void;
  public host: string = "localhost";
  private #secret = 1;
  ["computed"]: number = 2;

  static {
    void 0;
  }

  constructor(public readonly flag?: boolean, ...rest: number[]) {
    super();
  }

  override toString(): string {
    return this.host;
  }
}

namespace Utils {
  export function helper() {}
}

module Legacy {
  export const v = 1;
}

function* gen() {
  yield 1;
}
const g = function* () {
  yield 2;
};
const arrow = async (a, b = 1) => {
  await Promise.resolve(a + b);
  return a;
};

interface Repo<T extends string> {
  save(item: T): void;
  readonly id?: number;
}
interface Sub extends Repo<string> {}

function demo(
  required: number,
  optional?: string,
  ...rest: number[]
): asserts required is number {
  "use strict";
  ;
  debugger;
  label: {
    break label;
  }
  with ({}) {
    void 0;
  }
  let lex = 1;
  var classic = 2;
  classic++;
  --classic;
  lex = lex + classic;
  lex += 1;
  const seq = (lex, classic);
  const member = ns.helper;
  const sub = ([1, 2])[0];
  const unary = -lex;
  const arrPat: [number, ...number[]] = [1, 2];
  const [a, ...arest] = arrPat;
  const objPat = { k: 1, n: 2 };
  const { k: renamed = 0, n, m = 3, ...orest } = objPat;
  const re = /foo+/gim;
  const tpl = `x=${lex}`;
  const tern = lex ? 1 : 2;
  const call = helper(renamed, n);
  const neo = new Base(true, 1, 2);
  try {
    if (lex > 0) {
      for (let i = 0; i < 2; i++) {
        if (i === 1) continue;
        break;
      }
      for (const x of [1]) {
        switch (x) {
          case 1:
            break;
          default:
            break;
        }
      }
      while (false) {
        break;
      }
      do {
        break;
      } while (false);
    } else {
      throw new Error("x");
    }
  } catch (e) {
    void e;
  } finally {
    return call ?? neo;
  }
}

function helper(..._args: unknown[]) {
  return 0;
}
"#,
        ),
    ];

    let plugin = TypeScriptPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active TypeScript AST kind in typescript-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("typescript unhandled hotspots: {hotspots:?}");
}
