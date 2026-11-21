use rgctl_extraction::{Extractor, GraphBuilder, facts_cache::FactsCache};
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
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
        (
            "python",
            "src/app.py",
            "def helper():\n    return 1\ndef exercise():\n    return helper()\n",
            "src.app",
        ),
        (
            "go",
            "src/app.go",
            "package demo\nfunc helper() int { return 1 }\nfunc exercise() int { return helper() }\n",
            "demo",
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
        if matches!(language, "javascript" | "typescript" | "python" | "go") {
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
    for (plugin, path, _, module) in plugins()
        .into_iter()
        .filter(|(p, _, _, _)| matches!(p.language_id(), "javascript" | "typescript"))
    {
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
        let scoped = plugin.extract_all(
            Path::new(path),
            b"function first(){function helper(){return 1;}} function second(){function helper(){return 2;}}",
        ).unwrap();
        let helpers: Vec<_> = scoped
            .symbols
            .iter()
            .filter(|s| s.name == "helper")
            .collect();
        assert_eq!(helpers.len(), 2);
        assert!(
            helpers.iter().all(|s| !s.is_superseded_definition()),
            "unrelated lexical declarations are not redefinitions"
        );
        assert_ne!(helpers[0].qualified_name, helpers[1].qualified_name);
        for (source, caller, target, explicit_owner) in [
            (&b"var Foo = (function () { function Foo() { this.methodA(); } Foo.prototype.methodA = function () {}; return Foo; }());"[..], "Foo", "methodA", None),
            (&b"class Box { method(){ function inner(){return 1;} return inner(); } }"[..], "method", "inner", Some("Box.method.inner")),
        ] {
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
                (from.name.as_str() == caller && to.name.as_str() == target).then_some((from, to))
            })
            .collect();
        assert_eq!(
            calls.len(),
            2,
            "{}: {caller}->{target} qualified local calls were lost",
            plugin.language_id()
        );
        for (from, to) in calls {
            assert_eq!(
                from.file_path, to.file_path,
                "local call crossed modules"
            );
            let file = Path::new(to.file_path.as_deref().unwrap())
                .strip_prefix("/repo")
                .unwrap();
            let prefix = rgctl_plugin_helpers::qualified_names::file_module(file);
            let qualified = to.qualified_name.as_deref().unwrap();
            if let Some(owner) = explicit_owner {
                assert_eq!(qualified, owner);
            } else {
                assert!(qualified.starts_with(&format!("{prefix}.")), "{qualified}");
                assert!(qualified.ends_with(".methodA"), "{qualified}");
            }
        }
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
    for (plugin, path, source, _) in plugins()
        .into_iter()
        .filter(|(p, _, _, _)| p.language_id() != "go")
    {
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
fn native_member_names_do_not_merge_free_functions_or_depend_on_cache_order() {
    for (plugin, path, _, _) in plugins()
        .into_iter()
        .filter(|(p, _, _, _)| matches!(p.language_id(), "javascript" | "typescript"))
    {
        let suffix = Path::new(path).extension().unwrap().to_str().unwrap();
        let source = b"class Foo { static create(){return 1;} } function create(){function helper(){return 2;} return helper();} function run(){return create();}";
        let cache = Arc::new(FactsCache::new());
        let extractor = Extractor::new(Arc::new(rgctl_languages::default_registry()))
            .with_repo_root("/repo")
            .with_facts_cache(cache.clone());
        for name in ["Other", "Foo"] {
            let file = extractor
                .extract_file_with_source(
                    &Path::new("/repo").join(format!("src/{name}.{suffix}")),
                    source.to_vec(),
                )
                .unwrap();
            let fresh = Extractor::new(Arc::new(rgctl_languages::default_registry()))
                .with_repo_root("/repo")
                .extract_file_with_source(
                    &Path::new("/repo").join(format!("src/{name}.{suffix}")),
                    source.to_vec(),
                )
                .unwrap();
            assert_eq!(
                file.symbols, fresh.symbols,
                "cache-dependent declaration names"
            );
            assert_eq!(
                file.relations, fresh.relations,
                "cache-dependent call endpoints"
            );
            assert!(
                file.symbols
                    .iter()
                    .find(|s| s.name == "helper")
                    .unwrap()
                    .qualified_name
                    .is_none()
            );
            let mut builder = GraphBuilder::new();
            extractor.populate_graph(&[file], &mut builder).unwrap();
            let (nodes, edges) = builder.into_graph();
            let creates: Vec<_> = nodes
                .iter()
                .filter(|n| n.node_type == NodeType::Function && n.name.as_str() == "create")
                .collect();
            assert_eq!(
                creates.len(),
                2,
                "{}: declaration bodies merged",
                plugin.language_id()
            );
            let free = creates.iter().find(|n| n.qualified_name.is_none()).unwrap();
            let member = creates
                .iter()
                .find(|n| n.qualified_name.as_deref() == Some("Foo.create"))
                .unwrap();
            assert_ne!(free.id, member.id);
            let caller = nodes.iter().find(|n| n.name.as_str() == "run").unwrap();
            assert!(
                edges.iter().any(|e| e.edge_type == EdgeType::Calls
                    && e.from == caller.id
                    && e.to == free.id)
            );
        }
        for source in [
            &b"class Foo { constructor(){} } const obj = { '<init>': () => 2 };"[..],
            &b"const obj = { 'outer.inner': () => 2 }; function outer(){function inner(){return 1;}}"[..],
        ] {
            let result = plugin.extract_all(Path::new(&format!("Foo.{suffix}")), source).unwrap();
            assert!(result.symbols.iter().filter(|s| s.name == "<init>" || s.name == "outer.inner").all(|s| s.qualified_name.is_none()));
            assert!(result.symbols.iter().all(|s| !s.is_superseded_definition()));
        }
        if plugin.language_id() == "typescript" {
            let source = b"namespace app { export function helper(){return 1;} } function helper(){return 2;} function run(){return helper();}";
            let cache = Arc::new(FactsCache::new());
            let extractor = Extractor::new(Arc::new(rgctl_languages::default_registry()))
                .with_repo_root("/repo")
                .with_facts_cache(cache);
            for name in ["Other", "app"] {
                let path = Path::new("/repo/src").join(format!("{name}.ts"));
                let cached = extractor
                    .extract_file_with_source(&path, source.to_vec())
                    .unwrap();
                let fresh = Extractor::new(Arc::new(rgctl_languages::default_registry()))
                    .with_repo_root("/repo")
                    .extract_file_with_source(&path, source.to_vec())
                    .unwrap();
                assert_eq!(cached.symbols, fresh.symbols);
                assert_eq!(cached.relations, fresh.relations);
            }
        }
        assert!(cache.hit_count() > 0);
    }
}

#[test]
fn persisted_cache_rebases_names_and_call_endpoints_for_both_languages() {
    for (plugin, path, source, _) in plugins()
        .into_iter()
        .filter(|(p, _, _, _)| p.language_id() != "go")
    {
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

#[test]
fn python_packages_nested_functions_and_native_owners_agree_across_apis() {
    let plugin = rgctl_languages::default_registry()
        .get_language_plugin("python")
        .unwrap();
    let source = b"def outer():\n    def inner():\n        return 1\n    return inner()\n\nclass Owner:\n    def __init__(self):\n        pass\n    def method(self):\n        return outer()\n";
    for (path, module) in [
        ("pkg/__init__.py", "pkg"),
        ("pkg/deep/app.py", "pkg.deep.app"),
        ("app.pyw", "app"),
    ] {
        let all = plugin.extract_all(Path::new(path), source).unwrap();
        let split = plugin.extract_symbols(Path::new(path), source).unwrap();
        for symbols in [&all.symbols, &split] {
            for (name, expected) in [
                ("outer", format!("{module}.outer")),
                ("inner", format!("{module}.outer.inner")),
                ("Owner", format!("{module}.Owner")),
                ("__init__", "Owner.<init>".into()),
                ("method", "Owner.method".into()),
            ] {
                let symbol = symbols.iter().find(|s| s.name == name).unwrap();
                assert_eq!(
                    symbol.qualified_name.as_deref(),
                    Some(expected.as_str()),
                    "{path}: {name}"
                );
            }
        }
        let relations = plugin
            .extract_relations(Path::new(path), source, &split)
            .unwrap();
        for relations in [&all.relations, &relations] {
            assert!(
                relations.iter().any(|r| r.from == format!("{module}.outer")
                    && r.to_qualified_hint.as_deref()
                        == Some(format!("{module}.outer.inner").as_str())),
                "{path}: {relations:?}"
            );
            assert!(
                relations.iter().any(|r| r.from == "Owner.method"
                    && r.to_qualified_hint.as_deref() == Some(format!("{module}.outer").as_str())),
                "{path}: {relations:?}"
            );
        }
    }
    let repeated = plugin.extract_all(Path::new("pkg/app.py"), b"def first():\n    def helper():\n        return 1\ndef second():\n    def helper():\n        return 2\n").unwrap();
    let helpers: Vec<_> = repeated
        .symbols
        .iter()
        .filter(|s| s.name == "helper")
        .collect();
    assert_eq!(helpers.len(), 2);
    assert_ne!(helpers[0].qualified_name, helpers[1].qualified_name);
    assert!(helpers.iter().all(|s| !s.is_superseded_definition()));
}

#[test]
fn python_package_cache_rebases_file_names_and_preserves_native_members() {
    let source = b"def outer():\n    def inner():\n        return 1\n    return inner()\nclass Owner:\n    def method(self):\n        return outer()\n";
    let cache = Arc::new(FactsCache::new());
    let registry = Arc::new(rgctl_languages::default_registry());
    let before = Extractor::new(registry.clone())
        .with_repo_root("/before")
        .with_facts_cache(cache.clone());
    before
        .extract_file_with_source(Path::new("/before/first/__init__.py"), source.to_vec())
        .unwrap();
    let saved = tempfile::tempdir().unwrap();
    cache.save(saved.path()).unwrap();
    let restored = Arc::new(FactsCache::load(saved.path()));
    let after = Extractor::new(registry.clone())
        .with_repo_root("/after")
        .with_facts_cache(restored.clone());
    let path = Path::new("/after/second/nested/__init__.py");
    let cached = after
        .extract_file_with_source(path, source.to_vec())
        .unwrap();
    let fresh = Extractor::new(registry)
        .with_repo_root("/after")
        .extract_file_with_source(path, source.to_vec())
        .unwrap();
    assert_eq!(restored.hit_count(), 1);
    assert_eq!(cached.symbols, fresh.symbols);
    assert_eq!(cached.relations, fresh.relations);
    for (name, expected) in [
        ("outer", "second.nested.outer"),
        ("inner", "second.nested.outer.inner"),
        ("method", "Owner.method"),
    ] {
        assert_eq!(
            cached
                .symbols
                .iter()
                .find(|s| s.name == name)
                .unwrap()
                .qualified_name
                .as_deref(),
            Some(expected)
        );
    }
    assert!(
        cached
            .relations
            .iter()
            .any(|r| r.from == "second.nested.outer"
                && r.to_qualified_hint.as_deref() == Some("second.nested.outer.inner"))
    );
}

#[test]
fn python_native_member_collision_keeps_distinct_bodies_and_cache_order() {
    let source = b"class Native:\n    @staticmethod\n    def create():\n        return 1\ndef create():\n    def nested():\n        return 2\n    return nested()\ndef exercise():\n    return create()\n";
    let registry = Arc::new(rgctl_languages::default_registry());
    let cache = Arc::new(FactsCache::new());
    let extractor = Extractor::new(registry.clone())
        .with_repo_root("/repo")
        .with_facts_cache(cache.clone());
    for stem in ["Other", "Native"] {
        let path = Path::new("/repo/src").join(format!("{stem}.py"));
        let file = extractor
            .extract_file_with_source(&path, source.to_vec())
            .unwrap();
        let fresh = Extractor::new(registry.clone())
            .with_repo_root("/repo")
            .extract_file_with_source(&path, source.to_vec())
            .unwrap();
        assert_eq!(file.symbols, fresh.symbols);
        assert_eq!(file.relations, fresh.relations);
        assert!(
            file.symbols
                .iter()
                .find(|s| s.name == "nested")
                .unwrap()
                .qualified_name
                .is_none()
        );
        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&[file], &mut builder).unwrap();
        let (nodes, edges) = builder.into_graph();
        let creates: Vec<_> = nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function && n.name.as_str() == "create")
            .collect();
        assert_eq!(creates.len(), 2);
        let free = creates.iter().find(|n| n.qualified_name.is_none()).unwrap();
        let native = creates
            .iter()
            .find(|n| n.qualified_name.as_deref() == Some("Native.create"))
            .unwrap();
        let caller = nodes
            .iter()
            .find(|n| n.name.as_str() == "exercise")
            .unwrap();
        assert_ne!(free.id, native.id);
        assert!(
            edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Calls && e.from == caller.id && e.to == free.id)
        );
        assert!(
            !edges.iter().any(|e| e.edge_type == EdgeType::Calls
                && e.from == caller.id
                && e.to == native.id)
        );
    }
    assert!(cache.hit_count() > 0);
}

#[test]
fn go_package_names_cache_moves_and_native_owners_are_consistent() {
    use rgctl_plugin_api::QualifiedNameOrigin;
    let plugin = rgctl_languages::default_registry()
        .get_language_plugin("go")
        .unwrap();
    let source = b"package demo\nfunc Helper() int { return 1 }\nfunc Run() int { return Helper() }\nfunc ExternalRun() { External() }\ntype Widget struct {}\nfunc NewWidget() *Widget { return &Widget{} }\nfunc (w *Widget) Use() int { return Helper() }\n";
    let path = Path::new("src/app.go");
    let all = plugin.extract_all(path, source).unwrap();
    let split = plugin.extract_symbols(path, source).unwrap();
    for symbols in [&all.symbols, &split] {
        for (name, expected) in [
            ("Helper", "demo.Helper"),
            ("Run", "demo.Run"),
            ("Widget", "demo.Widget"),
            ("NewWidget", "Widget.<init>"),
            ("Use", "Widget.Use"),
        ] {
            assert_eq!(
                symbols
                    .iter()
                    .find(|s| s.name == name)
                    .unwrap()
                    .qualified_name
                    .as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            symbols
                .iter()
                .find(|s| s.name == "Helper")
                .unwrap()
                .qualified_name_origin,
            QualifiedNameOrigin::Package
        );
    }
    let relations = plugin.extract_relations(path, source, &split).unwrap();
    for relations in [&all.relations, &relations] {
        assert!(
            relations
                .iter()
                .any(|r| r.from == "demo.Run"
                    && r.to_qualified_hint.as_deref() == Some("demo.Helper")),
            "{relations:?}"
        );
    }
    let cache = Arc::new(FactsCache::new());
    let registry = Arc::new(rgctl_languages::default_registry());
    Extractor::new(registry.clone())
        .with_repo_root("/before")
        .with_facts_cache(cache.clone())
        .extract_file_with_source(Path::new("/before/first/tools/app.go"), source.to_vec())
        .unwrap();
    let saved = tempfile::tempdir().unwrap();
    cache.save(saved.path()).unwrap();
    let restored = Arc::new(FactsCache::load(saved.path()));
    let current = Path::new("/after/second/demo/app.go");
    let cached = Extractor::new(registry.clone())
        .with_repo_root("/after")
        .with_facts_cache(restored.clone())
        .extract_file_with_source(current, source.to_vec())
        .unwrap();
    let fresh = Extractor::new(registry)
        .with_repo_root("/after")
        .extract_file_with_source(current, source.to_vec())
        .unwrap();
    assert_eq!(restored.hit_count(), 1);
    assert_eq!(cached.symbols, fresh.symbols);
    assert_eq!(cached.relations, fresh.relations);
    assert!(
        cached
            .relations
            .iter()
            .any(|r| r.from == "second.demo.ExternalRun"
                && r.to_qualified_hint.as_deref() == Some("second.demo.External")
                && r.to_qualified_hint_origin == QualifiedNameOrigin::Package)
    );
    for (name, expected) in [
        ("Helper", "second.demo.Helper"),
        ("Widget", "second.demo.Widget"),
        ("Use", "Widget.Use"),
    ] {
        assert_eq!(
            cached
                .symbols
                .iter()
                .find(|s| s.name == name)
                .unwrap()
                .qualified_name
                .as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn go_cross_file_package_calls_stay_in_their_declaring_directory() {
    let extractor =
        Extractor::new(Arc::new(rgctl_languages::default_registry())).with_repo_root("/repo");
    let mut files = Vec::new();
    for dir in ["first", "second"] {
        for (name, source) in [
            (
                "caller.go",
                &b"package demo\nfunc Run() int { return Helper() }\n"[..],
            ),
            (
                "target.go",
                &b"package demo\nfunc Helper() int { return 1 }\n"[..],
            ),
        ] {
            files.push(
                extractor
                    .extract_file_with_source(
                        &Path::new("/repo").join(dir).join(name),
                        source.to_vec(),
                    )
                    .unwrap(),
            );
        }
    }
    files.push(
        extractor
            .extract_file_with_source(
                Path::new("/repo/third/caller.go"),
                b"package demo\nfunc Run() int { return Helper() }\n".to_vec(),
            )
            .unwrap(),
    );
    let mut builder = GraphBuilder::new();
    extractor.populate_graph(&files, &mut builder).unwrap();
    let (nodes, edges) = builder.into_graph();
    for (own, other) in [("first", "second"), ("second", "first")] {
        let id = |qn: &str| {
            nodes
                .iter()
                .find(|n| n.qualified_name.as_deref() == Some(qn))
                .unwrap()
                .id
        };
        let caller = id(&format!("{own}.demo.Run"));
        assert!(edges.iter().any(|e| e.edge_type == EdgeType::Calls
            && e.from == caller
            && e.to == id(&format!("{own}.demo.Helper"))));
        assert!(!edges.iter().any(|e| e.edge_type == EdgeType::Calls
            && e.from == caller
            && e.to == id(&format!("{other}.demo.Helper"))));
    }
    let missing = nodes
        .iter()
        .find(|n| n.qualified_name.as_deref() == Some("third.demo.Run"))
        .unwrap()
        .id;
    assert!(
        !edges
            .iter()
            .any(|e| e.edge_type == EdgeType::Calls && e.from == missing)
    );
}

#[test]
fn go_import_aliases_do_not_guess_between_unlinked_packages() {
    let extractor =
        Extractor::new(Arc::new(rgctl_languages::default_registry())).with_repo_root("/repo");
    let files:Vec<_>=[("first/target.go",&b"package tools\nfunc Helper() int { return 1 }\n"[..]),("second/target.go",&b"package tools\nfunc Helper() int { return 2 }\n"[..]),("caller/app.go",&b"package app\nimport one \"example.org/project/first\"\nfunc Run() int { return one.Helper() }\n"[..])].into_iter().map(|(p,s)|extractor.extract_file_with_source(&Path::new("/repo").join(p),s.to_vec()).unwrap()).collect();
    let mut builder = GraphBuilder::new();
    extractor.populate_graph(&files, &mut builder).unwrap();
    let (nodes, edges) = builder.into_graph();
    let id = |qn: &str| {
        nodes
            .iter()
            .find(|n| n.qualified_name.as_deref() == Some(qn))
            .unwrap()
            .id
    };
    let caller = id("caller.app.Run");
    // Import-to-directory linking belongs to the module/environment slice.
    // Until then an alias must not guess between packages with the same leaf name.
    assert!(
        !edges.iter().any(|e| e.edge_type == EdgeType::Calls
            && e.from == caller
            && e.to == id("first.tools.Helper")),
        "{edges:?}"
    );
    assert!(!edges.iter().any(|e| e.edge_type == EdgeType::Calls
        && e.from == caller
        && e.to == id("second.tools.Helper")));
}

#[test]
fn go_lexical_values_do_not_resolve_to_package_functions() {
    let source = br#"package demo
func Helper() int { return 1 }
type Container[Helper any] struct { Value Helper }
func Parameter(Helper func() int) int { return Helper() }
func Parenthesized(Helper func() int) int { return ((Helper))() }
func ParenthesizedDirect() int { return (Helper)() }
func Local() int { Helper := func() int { return 2 }; return Helper() }
func Typed() int { var Helper func() int = func() int { return 2 }; return Helper() }
func Boundaries() int {
    result := Helper()
    { Helper := func() int { return 2 }; result += Helper() }
    return result + Helper()
}
func NestedType(callback func(Helper int)) int { return Helper() }
func Initializer() int { Helper := func() int { return Helper() }; return Helper() }
func Range(values []func() int) int {
    for _, Helper := range values { _ = Helper() }
    return Helper()
}
func Select(values chan func() int) int {
    select { case Helper := <-values: return Helper(); default: return Helper() }
}
func Condition() int {
    if Helper := func() int { return 2 }; true { return Helper() }
    return Helper()
}
func TypeSwitch(value any) int {
    switch Helper := value.(type) { case func() int: return Helper() }
    return Helper()
}
func Literal() int { return func(Helper func() int) int { return Helper() }(func() int { return 1 }) }
"#;
    let registry = Arc::new(rgctl_languages::default_registry());
    let plugin = registry.get_language_plugin("go").unwrap();
    let path = Path::new("src/caller.go");
    let all = plugin.extract_all(path, source).unwrap();
    let symbols = plugin.extract_symbols(path, source).unwrap();
    let split = plugin.extract_relations(path, source, &symbols).unwrap();
    for relations in [&all.relations, &split] {
        for (owner, expected) in [
            ("Parameter", vec![true]),
            ("Parenthesized", vec![true]),
            ("ParenthesizedDirect", vec![false]),
            ("Local", vec![true]),
            ("Typed", vec![true]),
            ("Boundaries", vec![false, true, false]),
            ("NestedType", vec![false]),
            ("Initializer", vec![false, true]),
            ("Range", vec![true, false]),
            ("Select", vec![true, false]),
            ("Condition", vec![true, false]),
            ("TypeSwitch", vec![true, false]),
            ("Literal", vec![true]),
        ] {
            let calls: Vec<_> = relations
                .iter()
                .filter(|r| {
                    r.relation_type == RelationType::Calls
                        && r.from == format!("demo.{owner}")
                        && r.to.rsplit('.').next() == Some("Helper")
                })
                .collect();
            assert_eq!(
                calls.iter().map(|r| r.indirect_call).collect::<Vec<_>>(),
                expected,
                "{owner}: {calls:?}"
            );
            for call in calls.iter().filter(|r| r.indirect_call) {
                assert_eq!(call.to, "Helper");
                assert!(call.to_qualified_hint.is_none(), "{call:?}");
                assert!(call.to_identity_discriminator.is_none(), "{call:?}");
            }
        }
    }
    let cache = Arc::new(FactsCache::new());
    let extractor = Extractor::new(registry.clone())
        .with_repo_root("/repo")
        .with_facts_cache(cache.clone());
    let first = extractor
        .extract_file_with_source(Path::new("/repo/first/caller.go"), source.to_vec())
        .unwrap();
    let saved = tempfile::tempdir().unwrap();
    cache.save(saved.path()).unwrap();
    let restored = Arc::new(FactsCache::load(saved.path()));
    let cached = Extractor::new(registry.clone())
        .with_repo_root("/moved")
        .with_facts_cache(restored.clone())
        .extract_file_with_source(Path::new("/moved/second/caller.go"), source.to_vec())
        .unwrap();
    let fresh = Extractor::new(registry)
        .with_repo_root("/moved")
        .extract_file_with_source(Path::new("/moved/second/caller.go"), source.to_vec())
        .unwrap();
    assert_eq!(restored.hit_count(), 1);
    assert_eq!(cached.symbols, fresh.symbols);
    assert_eq!(cached.relations, fresh.relations);
    let mut builder = GraphBuilder::new();
    extractor.populate_graph(&[first], &mut builder).unwrap();
    let report = builder.take_callable_resolution_report();
    assert!(report.samples.iter().any(|r| r.indirect_call));
    assert!(!builder.take_parked_relations().iter().any(|r| matches!(
        r.from_qualified_name.as_str(),
        "first.demo.Parameter" | "first.demo.Local" | "first.demo.Typed"
    )));
    let (nodes, edges) = builder.into_graph();
    for owner in ["Parameter", "Parenthesized", "Local", "Typed"] {
        let caller = nodes
            .iter()
            .find(|n| n.qualified_name.as_deref() == Some(format!("first.demo.{owner}").as_str()))
            .unwrap()
            .id;
        assert!(
            !edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Calls && e.from == caller),
            "{owner}"
        );
    }
}
