//! Golden correctness + unconditional AST coverage exercise for Markdown.

use rgctl_lang_markdown::Plugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn headings_code_blocks_and_link_relations() {
    let source = br#"# Overview

```rust
fn main() {}
```

[ADR](./adr.md)

[pay]: ./payments.md#checkout
"#;
    let plugin = Plugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("docs/guide.md"), source)
        .expect("extract_all");

    assert!(
        out.symbols.iter().any(|s| {
            s.symbol_type == SymbolType::Module
                && s.metadata.get("kind") == Some(&serde_json::json!("heading"))
                && s.name == "Overview"
        }),
        "Overview heading missing: {:?}",
        out.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        out.symbols.iter().any(|s| {
            s.metadata.get("kind") == Some(&serde_json::json!("code_block"))
                && s.metadata.get("language").and_then(|v| v.as_str()) == Some("rust")
        }),
        "fenced rust code block missing"
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::References && r.to.ends_with("adr.md")),
        "References to adr.md missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../markdown-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("markdown", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    // Corpus must exercise every non-Literal active kind in markdown-ast-coverage.json.
    let sources: &[(&str, &[u8])] = &[(
        "ast_coverage_corpus.md",
        br#"---
title: Coverage
---

# ATX Heading One

## Nested Section

Setext Heading
==============

Setext Level Two
----------------

> block quote with continuation

- list item minus
* list item star
+ list item plus
1. list item dot
1) list item parenthesis

- [x] checked task
- [ ] unchecked task

```java
class Coverage {}
```

    indented_code_block_line()

| Left | Right |
| :--- | ----: |
| a    | b     |

***

[Inline link](./target.md#frag "title")

[ref link][lab]

[lab]: ./dest.md#anchor "Link Title"

<https://example.com/autolink>

&amp; &#123; \*

+++
title = "toml"
+++
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
        .expect("every active Markdown AST kind in markdown-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("markdown unhandled hotspots: {hotspots:?}");
}

#[test]
fn plugin_manifest_handlers_in_sync() {
    const MANIFEST: &str = include_str!("../markdown-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    let table = rgctl_lang_markdown::ast_handlers();
    let issues =
        rgctl_ast_coverage::check_plugin_manifest_sync("markdown", MANIFEST, table, &lang);
    assert!(
        issues.is_empty(),
        "HANDLED_KINDS / markdown-ast-coverage.json drift: {issues:?}"
    );
}
