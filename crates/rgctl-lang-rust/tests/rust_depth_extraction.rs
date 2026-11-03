//! Golden correctness + unconditional AST coverage exercise for Rust.

use rgctl_lang_rust::RustPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn scoped_associated_and_turbofish_call_hints() {
    let source = br#"
pub struct Widget;

impl Widget {
    pub fn new() -> Self { Widget }
    fn helper(&self) {}
    pub fn run(&self) {
        let _a = Widget::new();
        Self::helper(self);
        identity::<Widget>(self);
    }
}

fn identity<T>(x: T) -> T { x }
"#;
    let plugin = RustPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("src/widget.rs"), source)
        .expect("extract_all");

    let calls: Vec<_> = out
        .relations
        .iter()
        .filter(|r| r.relation_type == RelationType::Calls)
        .collect();

    assert!(
        calls.iter().any(|r| {
            r.to_type_hint.as_deref() == Some("Widget")
                && (r.to.contains("new")
                    || r.to.contains("<init>")
                    || r.to_qualified_hint
                        .as_deref()
                        .is_some_and(|h| h.contains("Widget") && h.contains("new")))
        }),
        "Widget::new hint missing: {calls:?}"
    );
    assert!(
        calls.iter().any(|r| {
            r.from.contains("run")
                && r.to.contains("helper")
                && r.to_type_hint.as_deref() == Some("Widget")
        }),
        "Self::helper hint missing: {calls:?}"
    );
    assert!(
        calls.iter().any(|r| r.to.contains("identity")),
        "turbofish identity::<Widget> missing: {calls:?}"
    );
}

#[test]
fn enum_variants_type_item_macro_and_trait_signatures() {
    let source = br#"
enum Color { Red, Green }

type Id = u64;

macro_rules! noop { () => {}; }

trait Painter {
    type Medium;
    fn paint(&self);
}
"#;
    let plugin = RustPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.rs"), source)
        .expect("extract_all");

    let color = out
        .symbols
        .iter()
        .find(|s| s.name == "Color" && s.symbol_type == SymbolType::Enum)
        .expect("Color enum");
    assert_eq!(color.fields.len(), 2);
    assert!(out.symbols.iter().any(|s| {
        s.name == "Red" && s.metadata.get("is_enum_variant").and_then(|v| v.as_bool()) == Some(true)
    }));
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "Id" && s.symbol_type == SymbolType::TypeAlias)
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "noop" && s.symbol_type == SymbolType::Macro)
    );
    assert!(out.symbols.iter().any(|s| {
        s.name == "Medium"
            && s.metadata.get("kind").and_then(|v| v.as_str()) == Some("associated_type")
    }));
    assert!(out.symbols.iter().any(|s| {
        s.name == "paint" && s.metadata.get("trait_method").and_then(|v| v.as_bool()) == Some(true)
    }));
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../rust-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("rust", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let sources: &[(&str, &[u8])] = &[(
        "ast_coverage_corpus.rs",
        br#"
#![allow(dead_code, unused)]
#![crate_type = "lib"]

extern crate core;
use std::fmt::{Debug as FmtDebug, Display};
use std::collections::HashMap as Map;

mod nested {
    pub fn ping() {}
}

const MAX: i32 = 10;
static GLOBAL: i32 = 0;
type Id = u64;
union Word { i: i32, u: u32 }

#[derive(Clone, Debug)]
struct Point {
    x: i32,
    y: i32,
}

enum Color {
    Red,
    Green(i32),
    Blue { a: i32, b: i32 },
}

trait Painter: FmtDebug
where
    Self: Sized,
{
    type Medium;
    fn paint(&self);
    fn signature_only(&self) -> i32;
}

macro_rules! noop {
    () => {};
    ($($t:tt)*) => { $($t)* };
}

impl Point {
    fn helper(&self) {}
    fn run(&self) -> Result<(), ()> {
        let mut n = 0;
        n = 1;
        n += 1;
        let _arr = [1, 2, 3];
        let _tup = (1, 2);
        let _unit = ();
        let _ref = &n;
        let _idx = _arr[0];
        let _cast = n as i64;
        let _neg = -n;
        let _bin = n + 1;
        let _range = 0..1;
        let _c = |x: i32| x + 1;
        let _a = async { 1 };
        let _u = unsafe { 1 };
        let _await = async { 1 }.await;
        let _pt = Point { x: 1, y: 2 };
        let _field = self.x;
        ;
        if let Some(x) = Some(1)
            && let Some(y) = Some(2)
        {
            let _ = x + y;
        }
        if true {
        } else {
        }
        for _i in 0..1 {
            break;
        }
        while false {
            continue;
        }
        loop {
            break;
        }
        match n {
            0 | 1 => {}
            2..=4 => {}
            ref r => {
                let _ = r;
            }
            mut m => {
                m = 0;
            }
            x @ 10 => {
                let _ = x;
            }
            _ => {}
        }
        match Some(1) {
            None::<i32> => {}
            Some(v) => {
                let _ = v;
            }
        }
        match Color::Green(1) {
            Color::Green(v) => {
                let _ = v;
            }
            Color::Blue { a, b: _, .. } => {
                let _ = a;
            }
            Color::Red => {}
        }
        match *_pt {
            Point { x, y } => {
                let _ = (x, y);
            }
        }
        match [1, 2, 3] {
            [a, b, ..] => {
                let _ = (a, b);
            }
        }
        match (1, 2) {
            (a, b) => {
                let _ = (a, b);
            }
        }
        match &n {
            &ref_pat => {
                let _ = ref_pat;
            }
        }
        // `generic_pattern`: turbofish path with no constructor args (`None::<T>`).
        match Option::<i32>::None {
            None::<i32> => {}
            Some::<i32>(_v) => {}
        }
        let _ = self.try_me()?;
        noop!();
        nested::ping();
        identity::<i32>(1);
        Point::helper(self);
        return Ok(());
    }
    fn try_me(&self) -> Result<(), ()> {
        Ok(())
    }
}

fn identity<T: FmtDebug>(x: T) -> T
where
    T: Display,
{
    x
}

fn gen_block() {
    let _g = gen {
        yield 1;
    };
}
"#,
    )];

    let plugin = RustPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Rust AST kind in rust-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("rust unhandled hotspots: {hotspots:?}");
}
