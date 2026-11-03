//! Golden correctness + unconditional AST coverage exercise for Go.

use rgctl_lang_go::GoPlugin;
use rgctl_plugin_api::{LanguagePlugin, SymbolType};
use std::path::Path;

#[test]
fn const_var_and_type_alias() {
    let source = br#"
const Max = 10

var Count int

type ID = string
"#;
    let plugin = GoPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.go"), source)
        .expect("extract_all");

    assert!(
        out.symbols.iter().any(|s| {
            s.name == "Max" && s.metadata.get("is_const").and_then(|v| v.as_bool()) == Some(true)
        }),
        "const Max missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols.iter().any(|s| {
            s.name == "Count" && s.metadata.get("kind").and_then(|v| v.as_str()) == Some("var_spec")
        }),
        "var Count missing"
    );
    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "ID" && s.symbol_type == SymbolType::TypeAlias),
        "type alias ID missing"
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../go-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
    let tracker = rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("go", MANIFEST, &lang)
        .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Corpus must exercise every non-Literal active kind in go-ast-coverage.json.
    let sources: &[(&str, &[u8])] = &[(
        "ast_coverage_corpus.go",
        br#"
package main

import (
	"fmt"
	"io"
)

const (
	Max = 10
	IotaVal = iota
)

var (
	Count int
	Ptr   *int
	Arr   [3]int
	Slice []int
	Ch    chan int
	MapV  map[string]int
	FnTy  func(int) int
)

type ID = string

type Pair[T ~int | string] struct {
	X T `json:"x"`
}

type Reader interface {
	Read([]byte) (int, error)
	io.Closer
}

type Neg = ~int

func (p *Pair[T]) Method(a int, rest ...int) T {
	return p.X
}

func Demo[T any](v T, w *Pair[int]) (out int) {
	;
	x := 1
	x = 2
	x++
	x--
	_ = -x + 1
	_ = Arr[0]
	_ = Slice[0:1:2]
	_ = fmt.Println
	_ = w.X
	_ = interface{}(x).(int)
	_ = int(x)
	_ = Pair[int]{}
	go Demo(1, w)
	defer fmt.Println(Max)
	if true {
		return
	}
	for i := 0; i < 1; i++ {
		break
	}
	for range Slice {
		continue
	}
Loop:
	for {
		goto Loop
	}
	switch x {
	case 0:
		fallthrough
	default:
		break
	}
	switch any(x).(type) {
	case int:
		break
	default:
		break
	}
	select {
	case Ch <- 1:
	case v := <-Ch:
		_ = v
	default:
	}
	return Count
}

func Empty() {}
"#,
    )];

    let plugin = GoPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Go AST kind in go-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("go unhandled hotspots: {hotspots:?}");
}
