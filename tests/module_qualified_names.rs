use rgctl_extraction::{Extractor, GraphBuilder, facts_cache::FactsCache};
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_plugin_api::{LanguagePlugin, SymbolType};
use std::{path::Path, sync::Arc};

type Case = (
    Arc<dyn LanguagePlugin>,
    &'static str,
    &'static str,
    &'static str,
);
fn plugins() -> Vec<Case> {
    let registry = rgctl_languages::default_registry();
    vec![
        (
            "javascript",
            "src/app.js",
            "function helper(){return 1;} function exercise(){return helper();}",
            "src.app",
        ),
        (
            "typescript",
            "src/app.ts",
            "function helper():number{return 1;} function exercise():number{return helper();}",
            "src.app",
        ),
    ]
    .into_iter()
    .map(|(language, path, source, module)| {
        (
            registry.get_language_plugin(language).unwrap(),
            path,
            source,
            module,
        )
    })
    .collect()
}

#[test]
fn public_extraction_entry_points_agree_on_names_and_call_endpoints() {
    for (plugin, path, source, module) in plugins() {
        let all = plugin
            .extract_all(Path::new(path), source.as_bytes())
            .unwrap();
        let symbols = plugin
            .extract_symbols(Path::new(path), source.as_bytes())
            .unwrap();
        for name in ["helper", "exercise"] {
            for list in [&all.symbols, &symbols] {
                let symbol = list
                    .iter()
                    .find(|s| s.name == name && s.symbol_type == SymbolType::Function)
                    .unwrap();
                assert_eq!(
                    symbol.qualified_name.as_deref(),
                    Some(format!("{module}.{name}").as_str()),
                    "{}",
                    plugin.language_id()
                );
            }
        }
        let relations = plugin
            .extract_relations(Path::new(path), source.as_bytes(), &symbols)
            .unwrap();
        for list in [&all.relations, &relations] {
            assert!(
                list.iter().any(|r| r.from == format!("{module}.exercise")
                    && (r.to_qualified_hint.as_deref()
                        == Some(format!("{module}.helper").as_str())
                        || r.to == format!("{module}.helper"))),
                "{}: {list:?}",
                plugin.language_id()
            );
        }
    }
}

#[test]
fn logical_modules_survive_checkout_moves_and_cache_reuse() {
    let cache = Arc::new(FactsCache::new());
    let source = b"function helper(){return 1;} function exercise(){return helper();}";
    let registry = Arc::new(rgctl_languages::default_registry());
    for root in ["/checkout/baseline", "/different/candidate"] {
        let extractor = Extractor::new(registry.clone())
            .with_repo_root(root)
            .with_facts_cache(cache.clone());
        for directory in ["first", "second"] {
            let path = Path::new(root).join(directory).join("app.js");
            let file = extractor
                .extract_file_with_source(&path, source.to_vec())
                .unwrap();
            let exercise = file.symbols.iter().find(|s| s.name == "exercise").unwrap();
            assert_eq!(
                exercise.qualified_name.as_deref(),
                Some(format!("{directory}.app.exercise").as_str())
            );
            let mut builder = GraphBuilder::new();
            extractor.populate_graph(&[file], &mut builder).unwrap();
            let (nodes, edges) = builder.into_graph();
            let find = |name: &str| {
                nodes
                    .iter()
                    .find(|n| n.node_type == NodeType::Function && &*n.name == name)
                    .unwrap()
                    .id
            };
            assert!(edges.iter().any(|e| e.from == find("exercise")
                && e.to == find("helper")
                && e.edge_type == EdgeType::Calls));
        }
    }
}

const TIER1: &[(&str, &str, &str)] = &[
    (
        "rust",
        "rust/lib.rs",
        "// café\npub fn target_fn() -> i32 {\n    sibling_fn()\n}\npub fn sibling_fn() -> i32 {\n    /* RGCTL_BODY */\n    2\n}\n",
    ),
    (
        "python",
        "python/mod.py",
        "# café\ndef target_fn():\n    return sibling_fn()\n\ndef sibling_fn():\n    # RGCTL_BODY\n    return 2\n",
    ),
    (
        "typescript",
        "typescript/mod.ts",
        "// café\nexport class Box {\n  status: number = 0;\n  status(): number { return target_fn(); }\n}\nexport function target_fn(): number {\n  return sibling_fn();\n}\nexport function sibling_fn(): number {\n  /* RGCTL_BODY */\n  return 2;\n}\n",
    ),
    (
        "javascript",
        "javascript/mod.js",
        "// café\nexport function target_fn() {\n  return sibling_fn();\n}\nexport function sibling_fn() {\n  /* RGCTL_BODY */\n  return 2;\n}\n",
    ),
    (
        "go",
        "go/mod.go",
        "package main\n\n// café\nfunc target_fn() int {\n\treturn sibling_fn()\n}\n\nfunc sibling_fn() int {\n\t// RGCTL_BODY\n\treturn 2\n}\n",
    ),
    (
        "java",
        "java/Mod.java",
        "public class Mod {\n  // café\n  public int status;\n  public static int target_fn() { return sibling_fn(); }\n  public int status() { return target_fn(); }\n  public static int sibling_fn() { /* RGCTL_BODY */ return 2; }\n}\n",
    ),
    (
        "csharp",
        "csharp/Mod.cs",
        "public class Mod {\n  // café\n  public int Status;\n  public static int TargetFn() { return SiblingFn(); }\n  public int Status() { return TargetFn(); }\n  public static int SiblingFn() { /* RGCTL_BODY */ return 2; }\n}\n",
    ),
    (
        "cpp",
        "cpp/mod.cpp",
        "// café\nclass Holder {\npublic:\n  int status;\n  int status_fn() { return status; }\n};\nint target_fn() { return sibling_fn(); }\nint sibling_fn() { /* RGCTL_BODY */ return 2; }\n",
    ),
    (
        "c",
        "c/mod.c",
        "/* café */\nint target_fn(void) { return sibling_fn(); }\nint sibling_fn(void) { /* RGCTL_BODY */ return 2; }\n",
    ),
    (
        "php",
        "php/mod.php",
        "<?php\n// café\nfunction target_fn() { return sibling_fn(); }\nfunction sibling_fn() { /* RGCTL_BODY */ return 2; }\n",
    ),
    (
        "ruby",
        "ruby/mod.rb",
        "# café\ndef target_fn\n  sibling_fn\nend\ndef sibling_fn\n  # RGCTL_BODY\n  2\nend\n",
    ),
    (
        "puppet",
        "puppet/mod.pp",
        "# café\nfunction target_fn() {\n  1\n}\nfunction sibling_fn() {\n  # RGCTL_BODY\n  2\n}\n",
    ),
    (
        "kotlin",
        "kotlin/Mod.kt",
        "// café\nclass Mod {\n  var status: Int = 0\n  fun status(): Int = target_fn()\n}\nfun target_fn(): Int = sibling_fn()\nfun sibling_fn(): Int {\n  // RGCTL_BODY\n  return 2\n}\n",
    ),
    (
        "groovy",
        "groovy/Mod.groovy",
        "// café\ndef target_fn() { return sibling_fn() }\ndef sibling_fn() { /* RGCTL_BODY */ return 2 }\n",
    ),
    (
        "swift",
        "swift/Mod.swift",
        "// café\nfunc target_fn() -> Int { return sibling_fn() }\nfunc sibling_fn() -> Int { /* RGCTL_BODY */ return 2 }\n",
    ),
    (
        "scala",
        "scala/Mod.scala",
        "// café\nclass Mod {\n  var status: Int = 0\n  def status(): Int = target_fn()\n  def target_fn(): Int = sibling_fn()\n  def sibling_fn(): Int = {\n    // RGCTL_BODY\n    2\n  }\n}\n",
    ),
    (
        "dart",
        "dart/mod.dart",
        "// café\nint target_fn() { return sibling_fn(); }\nint sibling_fn() { /* RGCTL_BODY */ return 2; }\n",
    ),
    (
        "elixir",
        "elixir/mod.ex",
        "# café\ndefmodule Mod do\n  def target_fn do\n    sibling_fn()\n  end\n  def sibling_fn do\n    # RGCTL_BODY\n    2\n  end\nend\n",
    ),
    (
        "lua",
        "lua/mod.lua",
        "-- café\nfunction target_fn()\n  return sibling_fn()\nend\nfunction sibling_fn()\n  -- RGCTL_BODY\n  return 2\nend\n",
    ),
    (
        "bash",
        "bash/mod.sh",
        "# café\ntarget_fn() { sibling_fn; }\nsibling_fn() { echo RGCTL_BODY; }\n",
    ),
    (
        "sql",
        "sql/mod.sql",
        "-- café\nCREATE FUNCTION target_fn() RETURNS INT LANGUAGE sql AS $$ SELECT 1; $$;\nCREATE FUNCTION sibling_fn() RETURNS INT LANGUAGE sql AS $$ SELECT 2; /* RGCTL_BODY */ $$;\n",
    ),
    (
        "hcl",
        "hcl/mod.tf",
        "# café\nresource \"null_resource\" \"target_fn\" {\n}\nresource \"null_resource\" \"sibling_fn\" {\n  # RGCTL_BODY\n}\n",
    ),
    (
        "erb",
        "erb/mod.html.erb",
        "<p><%= target_fn %></p>\n<% # RGCTL_BODY %>\n<%= sibling_fn %>\n<!-- café -->\n",
    ),
    (
        "markdown",
        "markdown/doc.md",
        "# Target Fn\n\nCafé paragraph.\n\n# Sibling Fn\n\nRGCTL_BODY\n",
    ),
];

#[test]
fn every_tier1_language_round_trips_and_preserves_native_names() {
    use rgctl_plugin_helpers::qualified_names::qualify_in_repo;
    use std::collections::HashSet;
    let registry = rgctl_languages::default_registry();
    let covered: HashSet<_> = TIER1.iter().map(|c| c.0).collect();
    let bundled: HashSet<_> = rgctl_ast_coverage::bundled_specs()
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(covered, bundled);
    for &(language, path, source) in TIER1 {
        let plugin = registry.get_language_plugin(language).unwrap();
        let mut result = plugin
            .extract_all(Path::new(path), source.as_bytes())
            .unwrap();
        let bytes = serde_json::to_vec(&result).unwrap();
        let restored =
            serde_json::from_slice::<rgctl_plugin_api::ExtractAllResult>(&bytes).unwrap();
        assert_eq!(result, restored, "{language}: cached facts changed");
        assert!(
            result
                .symbols
                .iter()
                .all(|s| s.metadata.get("module_namespace").is_none()),
            "{language}: per-symbol namespace metadata"
        );
        if matches!(language, "javascript" | "typescript") {
            assert!(
                result.file_namespace.is_some(),
                "{language}: qualification has no provenance"
            );
        } else {
            assert!(
                result.file_namespace.is_none(),
                "{language}: unexpected module policy"
            );
            let original = result.clone();
            qualify_in_repo(language, Path::new("moved/file"), &mut result);
            assert_eq!(
                result, original,
                "{language}: native qualification was rewritten"
            );
        }
    }
}

#[test]
fn nested_declarations_and_explicit_method_owners_are_preserved() {
    for (plugin, path, _, module) in plugins() {
        let source = b"function outer(){ function inner(){ return 1; } return inner(); } class Box { method(){ return outer(); } }";
        let result = plugin.extract_all(Path::new(path), source).unwrap();
        let name = |bare: &str| {
            result
                .symbols
                .iter()
                .find(|s| s.name == bare)
                .unwrap()
                .qualified_name
                .as_deref()
                .unwrap()
        };
        assert_eq!(name("outer"), format!("{module}.outer"));
        assert_eq!(name("inner"), format!("{module}.outer.inner"));
        assert_eq!(name("Box"), format!("{module}.Box"));
        assert_eq!(name("method"), "Box.method");
        assert!(
            result
                .relations
                .iter()
                .any(|r| r.from == format!("{module}.outer")
                    && r.to_qualified_hint.as_deref()
                        == Some(format!("{module}.outer.inner").as_str()))
        );
        let source = b"var Foo = (function () { function Foo() { this.methodA(); } Foo.prototype.methodA = function () {}; return Foo; }());";
        let extractor =
            Extractor::new(Arc::new(rgctl_languages::default_registry())).with_repo_root("/repo");
        let files: Vec<_> = ["first", "second"]
            .into_iter()
            .map(|dir| {
                extractor
                    .extract_file_with_source(
                        &Path::new("/repo").join(dir).join(path),
                        source.to_vec(),
                    )
                    .unwrap()
            })
            .collect();
        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&files, &mut builder).unwrap();
        let (nodes, edges) = builder.into_graph();
        let calls: Vec<_> = edges
            .iter()
            .filter_map(|edge| {
                if edge.edge_type != EdgeType::Calls {
                    return None;
                }
                let from = nodes.iter().find(|node| node.id == edge.from).unwrap();
                let to = nodes.iter().find(|node| node.id == edge.to).unwrap();
                (from.name.as_str() == "Foo" && to.name.as_str() == "methodA").then_some((from, to))
            })
            .collect();
        assert_eq!(
            calls.len(),
            2,
            "{}: prototype receiver calls were lost",
            plugin.language_id()
        );
        for (from, to) in calls {
            assert_eq!(
                from.file_path, to.file_path,
                "prototype call crossed modules"
            );
            let file = Path::new(to.file_path.as_deref().unwrap())
                .strip_prefix("/repo")
                .unwrap();
            let prefix = rgctl_plugin_helpers::qualified_names::file_module(file);
            let qualified = to.qualified_name.as_deref().unwrap();
            assert!(qualified.starts_with(&format!("{prefix}.")), "{qualified}");
            assert!(qualified.ends_with(".methodA"), "{qualified}");
        }
    }
    let plugin = rgctl_lang_typescript::TypeScriptPlugin::default();
    let result = plugin
        .extract_all(
            Path::new("src/app.ts"),
            b"namespace Api { export function helper(){ return 1; } }",
        )
        .unwrap();
    let helper = result.symbols.iter().find(|s| s.name == "helper").unwrap();
    assert_eq!(helper.qualified_name.as_deref(), Some("Api.helper"));

    let source = b"namespace app { export function helper(){ function nested(){return 1;} return nested(); } } function exercise(){return 2;}";
    let mut result = plugin.extract_all(Path::new("app.ts"), source).unwrap();
    result = serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
    rgctl_plugin_helpers::qualified_names::qualify_in_repo(
        "typescript",
        Path::new("src/app.ts"),
        &mut result,
    );
    for (name, qualified) in [
        ("helper", "app.helper"),
        ("nested", "app.helper.nested"),
        ("exercise", "src.app.exercise"),
    ] {
        let symbol = result.symbols.iter().find(|s| s.name == name).unwrap();
        assert_eq!(symbol.qualified_name.as_deref(), Some(qualified));
    }
    assert!(
        result.relations.iter().any(|r| r.from == "app.helper"
            && r.to_qualified_hint.as_deref() == Some("app.helper.nested"))
    );
}

#[test]
fn same_stem_modules_do_not_cross_wire_calls() {
    for (plugin, path, source, _) in plugins() {
        let root = Path::new("/repo");
        let cache = Arc::new(FactsCache::new());
        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry)
            .with_repo_root(root)
            .with_facts_cache(cache.clone());
        let basename = Path::new(path).file_name().unwrap();
        let files: Vec<_> = ["first", "second"]
            .iter()
            .map(|dir| {
                extractor
                    .extract_file_with_source(
                        &root.join(dir).join(basename),
                        source.as_bytes().to_vec(),
                    )
                    .unwrap()
            })
            .collect();
        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&files, &mut builder).unwrap();
        let (nodes, edges) = builder.into_graph();
        let calls: Vec<_> = edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .collect();
        assert_eq!(
            calls.len(),
            2,
            "{}: missing local calls",
            plugin.language_id()
        );
        for edge in calls {
            let from = nodes.iter().find(|n| n.id == edge.from).unwrap();
            let to = nodes.iter().find(|n| n.id == edge.to).unwrap();
            assert_eq!(from.name.as_str(), "exercise");
            assert_eq!(to.name.as_str(), "helper");
            assert_eq!(
                from.file_path,
                to.file_path,
                "{}: crossed modules",
                plugin.language_id()
            );
        }
        assert!(
            cache.hit_count() > 0,
            "second module should reuse raw facts"
        );
    }
}

#[test]
fn persisted_cache_rebases_names_and_call_endpoints_for_both_languages() {
    for (plugin, path, source, _) in plugins() {
        let registry = Arc::new(rgctl_languages::default_registry());
        let cache = Arc::new(FactsCache::new());
        let basename = Path::new(path).file_name().unwrap();
        let before = Extractor::new(registry.clone())
            .with_repo_root("/before")
            .with_facts_cache(cache.clone());
        before
            .extract_file_with_source(
                &Path::new("/before/first").join(basename),
                source.as_bytes().to_vec(),
            )
            .unwrap();
        let saved = tempfile::tempdir().unwrap();
        cache.save(saved.path()).unwrap();
        let restored = Arc::new(FactsCache::load(saved.path()));
        let after = Extractor::new(registry)
            .with_repo_root("/after")
            .with_facts_cache(restored.clone());
        let file = after
            .extract_file_with_source(
                &Path::new("/after/second").join(basename),
                source.as_bytes().to_vec(),
            )
            .unwrap();
        assert_eq!(
            restored.hit_count(),
            1,
            "{}: expected disk-cache hit",
            plugin.language_id()
        );
        assert!(
            file.symbols
                .iter()
                .filter(|s| matches!(s.name.as_str(), "helper" | "exercise"))
                .all(|s| s.qualified_name.as_deref()
                    == Some(format!("second.app.{}", s.name).as_str()))
        );
        assert!(
            file.relations
                .iter()
                .any(|r| r.from == "second.app.exercise"
                    && r.to_qualified_hint.as_deref() == Some("second.app.helper"))
        );
        assert!(file.relations.iter().all(|r| {
            !r.from.starts_with("first.")
                && !r
                    .to_qualified_hint
                    .as_deref()
                    .is_some_and(|name| name.starts_with("first."))
        }));
    }
}
