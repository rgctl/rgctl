//! Golden correctness + unconditional AST coverage exercise for Python.

use rgctl_lang_python::PythonPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn self_cls_annotated_enum_and_lambda_fields() {
    let source = br#"
from enum import Enum

class Status(Enum):
    PENDING = "pending"
    DONE = "done"

class Client:
    host: str = "localhost"
    timeout: int
    transform = lambda x: x

    def __init__(self):
        self.session = None

    @classmethod
    def configure(cls):
        cls.default_timeout = 30

    def refresh(self):
        self.token = "x"
"#;
    let plugin = PythonPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("client.py"), source)
        .expect("extract_all");

    let status = out
        .symbols
        .iter()
        .find(|s| s.name == "Status" && s.symbol_type == SymbolType::Class)
        .expect("Status");
    assert!(status.fields.iter().any(|f| f.name == "PENDING"));
    assert!(status.fields.iter().any(|f| f.name == "DONE"));

    let client = out
        .symbols
        .iter()
        .find(|s| s.name == "Client" && s.symbol_type == SymbolType::Class)
        .expect("Client");
    assert!(
        client
            .fields
            .iter()
            .any(|f| f.name == "host" && f.field_type.as_deref() == Some("str")),
        "annotated host: {fields:?}",
        fields = client.fields
    );
    assert!(client.fields.iter().any(|f| f.name == "timeout"));
    assert!(client.fields.iter().any(|f| f.name == "transform"));
    assert!(client.fields.iter().any(|f| f.name == "session"));
    assert!(client.fields.iter().any(|f| f.name == "default_timeout"));
    assert!(client.fields.iter().any(|f| f.name == "token"));
}

#[test]
fn type_alias_with_as_guards_and_walrus() {
    let source = br#"
from typing import TYPE_CHECKING
import sys

type Point = tuple[int, int]

if TYPE_CHECKING:
    from models import Order
    DEFAULT_LIMIT = 100

try:
    import orjson
except ImportError:
    orjson = None

with open("f") as handle:
    pass

def parse(data):
    if (n := len(data)):
        return n
"#;
    let plugin = PythonPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("models.py"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| { s.symbol_type == SymbolType::TypeAlias && s.name == "Point" }),
        "type Point alias missing: {:?}",
        out.symbols
            .iter()
            .map(|s| (&s.name, &s.symbol_type))
            .collect::<Vec<_>>()
    );
    assert!(
        out.symbols.iter().any(|s| {
            s.name == "DEFAULT_LIMIT"
                && s.symbol_type == SymbolType::Variable
                && s.metadata.get("guard").and_then(|v| v.as_str()) == Some("typing")
        }),
        "guard binding DEFAULT_LIMIT missing: {:?}",
        out.symbols
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::Import && s.name.contains("Order")),
        "TYPE_CHECKING import Order missing"
    );
    assert!(
        out.symbols.iter().any(|s| {
            s.name == "handle"
                && s.metadata.get("kind").and_then(|v| v.as_str()) == Some("with_as_target")
        }),
        "with-as handle missing: {:?}",
        out.symbols
    );
    assert!(
        out.symbols.iter().any(|s| {
            s.name == "n"
                && s.metadata.get("kind").and_then(|v| v.as_str()) == Some("named_expression")
        }),
        "walrus n missing"
    );
}

#[test]
fn subscript_getitem_setitem_when_typed() {
    let source = br#"
def read_map(d: dict):
    x = d["a"]
    d["b"] = 1
"#;
    let plugin = PythonPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("map.py"), source)
        .expect("extract_all");

    let calls: Vec<_> = out
        .relations
        .iter()
        .filter(|r| r.relation_type == RelationType::Calls)
        .collect();
    assert!(
        calls.iter().any(|r| {
            r.to == "__getitem__" && r.to_qualified_hint.as_deref() == Some("dict.__getitem__")
        }),
        "getitem missing: {calls:?}"
    );
    assert!(
        calls.iter().any(|r| {
            r.to == "__setitem__" && r.to_qualified_hint.as_deref() == Some("dict.__setitem__")
        }),
        "setitem missing: {calls:?}"
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../python-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("python", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.py",
            br#"
from __future__ import annotations
import sys as system
from os import path as ospath
from . import sibling
from ..pkg import util
from typing import *

type Point[T] = list[T]

@classmethod
def decorate(cls):
    pass

class Client:
    host: str = "localhost"
    transform = lambda x, y=1: x

    def __init__(self):
        self.session = None

    def refresh(self, a: int, b: int = 1, /, *args, c: int, d: int = 2, **kwargs):
        self.token = "x"
        self.token += "y"
        x = self.host
        dmap = {"a": 1}
        y = dmap["a"] if False else 1
        y = dmap["a"]
        dmap["b"] = 2
        assert True
        if False:
            pass
        elif True:
            pass
        else:
            pass
        for i in range(1):
            if i:
                continue
            break
        while False:
            break
        try:
            raise ValueError("x")
        except ValueError:
            pass
        finally:
            pass
        with open("f") as handle:
            pass
        match i:
            case 0 | 1:
                pass
            case Point(x=1, y=2):
                pass
            case [a, *rest]:
                pass
            case (a, b):
                pass
            case {"k": v, **rest}:
                pass
            case 1+2j:
                pass
            case int() as n:
                pass
            case _:
                pass
        [x for x in range(1) if x]
        {x for x in range(1)}
        {k: v for k, v in {}.items()}
        (x for x in range(1))
        if (n := 1):
            pass
        z = a + b
        z = a and b
        z = a < b
        z = not a
        z = -a
        f"{n:.2f}"
        yield 1
        return

async def go():
    await decorate()

g1 = 0

def scope_demo():
    global g1
    def inner():
        nonlocal g1
        del g1
        pass
"#,
        ),
        (
            "py2_compat_corpus.py",
            // tree-sitter-python still emits print_statement / exec_statement / chevron
            // for legacy statement forms that remain named grammar kinds.
            br#"
print "hi"
print >> sys.stderr, "x"
exec "x = 1"
"#,
        ),
    ];

    let plugin = PythonPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Python AST kind in python-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("python unhandled hotspots: {hotspots:?}");
}
