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
        (
            "python",
            "src/app.py",
            "def helper():\n    return 1\ndef exercise():\n    return helper()\n",
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
        if matches!(language, "javascript" | "typescript" | "python") {
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
        .filter(|(p, _, _, _)| p.language_id() != "python")
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
fn native_member_names_do_not_merge_free_functions_or_depend_on_cache_order() {
    for (plugin, path, _, _) in plugins()
        .into_iter()
        .filter(|(p, _, _, _)| p.language_id() != "python")
    {
        let suffix = Path::new(path).extension().unwrap().to_str().unwrap();
        let source = b"class Foo { static create(){return 1;} } function create(){function helper(){return 2;} return helper();} function run(){return create();}";
        let cache = Arc::new(FactsCache::new());
        let extractor = Extractor::new(Arc::new(rgctl_languages::default_registry()))
            .with_repo_root("/repo")
            .with_facts_cache(cache.clone());
        // 1. Nested `src/{Other,Foo}.{js,ts}`: candidate `src.Foo.create` does NOT
        //    collide with `Native` `Foo.create`, so free `create` and `helper`
        //    qualify under `src.{name}` while staying distinct from `Foo.create`.
        for rel_path in [
            format!("src/Other.{suffix}"),
            format!("src/Foo.{suffix}"),
            format!("Foo.{suffix}"),
        ] {
            let full_path = Path::new("/repo").join(&rel_path);
            let file = extractor
                .extract_file_with_source(&full_path, source.to_vec())
                .unwrap();
            let fresh = Extractor::new(Arc::new(rgctl_languages::default_registry()))
                .with_repo_root("/repo")
                .extract_file_with_source(&full_path, source.to_vec())
                .unwrap();
            assert_eq!(
                file.symbols, fresh.symbols,
                "cache-dependent declaration names for {rel_path}"
            );
            assert_eq!(
                file.relations, fresh.relations,
                "cache-dependent call endpoints for {rel_path}"
            );

            let is_root_collision = rel_path == format!("Foo.{suffix}");
            let helper_qn = file
                .symbols
                .iter()
                .find(|s| s.name == "helper")
                .unwrap()
                .qualified_name
                .as_deref();
            if is_root_collision {
                // Root `Foo.{js,ts}` has module `Foo`, so `Foo.create` collides
                // with `Native` `Foo.create` — free `create` and `helper` defer.
                assert!(helper_qn.is_none());
            } else {
                let mod_prefix =
                    rgctl_plugin_helpers::qualified_names::file_module(Path::new(&rel_path));
                assert_eq!(
                    helper_qn,
                    Some(format!("{mod_prefix}.create.helper").as_str())
                );
            }

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
                "{}: declaration bodies merged in {rel_path}",
                plugin.language_id()
            );
            let member = creates
                .iter()
                .find(|n| n.qualified_name.as_deref() == Some("Foo.create"))
                .unwrap();
            let free = creates.iter().find(|n| n.id != member.id).unwrap();
            if is_root_collision {
                assert!(free.qualified_name.is_none());
            } else {
                assert!(free.qualified_name.is_some());
            }
            let caller = nodes.iter().find(|n| n.name.as_str() == "run").unwrap();
            assert!(
                edges.iter().any(|e| e.edge_type == EdgeType::Calls
                    && e.from == caller.id
                    && e.to == free.id)
            );
        }

        // 2. Non-colliding class method `Helper.create` in two same-stem files
        //    (`first/OrderService`, `second/OrderService`) must NOT suppress
        //    module qualification of top-level `create()`.
        let multi_src = b"class Helper { create(){return 0;} } function create(){return 1;} function run(){return create();}";
        let files: Vec<_> = ["first", "second"]
            .iter()
            .map(|dir| {
                extractor
                    .extract_file_with_source(
                        &Path::new("/repo")
                            .join(dir)
                            .join(format!("OrderService.{suffix}")),
                        multi_src.to_vec(),
                    )
                    .unwrap()
            })
            .collect();
        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&files, &mut builder).unwrap();
        let (nodes, edges) = builder.into_graph();
        let run_calls: Vec<_> = edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Calls)
            .filter_map(|e| {
                let from = nodes.iter().find(|n| n.id == e.from)?;
                let to = nodes.iter().find(|n| n.id == e.to)?;
                (from.name.as_str() == "run" && to.name.as_str() == "create").then_some((from, to))
            })
            .collect();
        assert_eq!(run_calls.len(), 2);
        for (from, to) in run_calls {
            assert_eq!(from.file_path, to.file_path);
            assert!(
                to.qualified_name
                    .as_deref()
                    .unwrap()
                    .ends_with(".OrderService.create")
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
            for rel in ["src/Other.ts", "src/app.ts", "app.ts"] {
                let path = Path::new("/repo").join(rel);
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
        assert_eq!(
            all.relations, relations,
            "{path}: combined vs split relations"
        );
        assert!(
            relations.iter().any(|r| r.from == format!("{module}.outer")
                && r.to == "inner"
                && r.to_qualified_hint.as_deref()
                    == Some(format!("{module}.outer.inner").as_str())),
            "{path}: {relations:?}"
        );
        assert!(
            relations.iter().any(|r| r.from == "Owner.method"
                && r.to == "outer"
                && r.to_qualified_hint.as_deref() == Some(format!("{module}.outer").as_str())),
            "{path}: {relations:?}"
        );
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

    // Method-nested closures in the same class and nested classes with identical inner names
    // must keep distinct qualified names and preserve all Calls edges.
    let nested_src = br#"class Box:
    def method_a(self):
        def inner():
            return 1
        return inner()

    def method_b(self):
        def inner():
            return 2
        return inner()

class OuterA:
    class Meta:
        def check(self):
            return 1

class OuterB:
    class Meta:
        def check(self):
            return 2

type Point = tuple[int, int]
helper = lambda x: x + 1
callbacks = [lambda a: a, lambda b: b]
"#;
    let extractor =
        Extractor::new(Arc::new(rgctl_languages::default_registry())).with_repo_root("/repo");
    let file = extractor
        .extract_file_with_source(Path::new("/repo/pkg/nested.py"), nested_src.to_vec())
        .unwrap();
    let qn_of = |name: &str| -> Vec<Option<&str>> {
        file.symbols
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.qualified_name.as_deref())
            .collect()
    };
    assert_eq!(
        qn_of("inner"),
        vec![Some("Box.method_a.inner"), Some("Box.method_b.inner")]
    );
    assert_eq!(
        qn_of("check"),
        vec![Some("OuterA.Meta.check"), Some("OuterB.Meta.check")]
    );
    assert_eq!(qn_of("Point"), vec![Some("pkg.nested.Point")]);
    assert_eq!(qn_of("helper"), vec![Some("pkg.nested.helper")]);
    let anon_lambdas: Vec<_> = file
        .symbols
        .iter()
        .filter(|s| s.name.starts_with("<lambda@"))
        .collect();
    assert_eq!(anon_lambdas.len(), 2);
    assert_ne!(anon_lambdas[0].name, anon_lambdas[1].name);
    assert!(anon_lambdas.iter().all(|s| s.qualified_name.is_none()));

    let mut builder = GraphBuilder::new();
    extractor.populate_graph(&[file], &mut builder).unwrap();
    let (nodes, edges) = builder.into_graph();
    let node_id = |qn: &str| {
        nodes
            .iter()
            .find(|n| n.qualified_name.as_deref() == Some(qn))
            .unwrap_or_else(|| panic!("missing node {qn}: {nodes:?}"))
            .id
    };
    assert!(edges.iter().any(|e| {
        e.edge_type == EdgeType::Calls
            && e.from == node_id("Box.method_a")
            && e.to == node_id("Box.method_a.inner")
    }));
    assert!(edges.iter().any(|e| {
        e.edge_type == EdgeType::Calls
            && e.from == node_id("Box.method_b")
            && e.to == node_id("Box.method_b.inner")
    }));
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
    for (rel_path, expected_free_qn, expected_nested_qn) in [
        (
            "src/Other.py",
            Some("src.Other.create"),
            Some("src.Other.create.nested"),
        ),
        (
            "src/Native.py",
            Some("src.Native.create"),
            Some("src.Native.create.nested"),
        ),
        ("Native.py", None, None),
    ] {
        let path = Path::new("/repo").join(rel_path);
        let file = extractor
            .extract_file_with_source(&path, source.to_vec())
            .unwrap();
        let fresh = Extractor::new(registry.clone())
            .with_repo_root("/repo")
            .extract_file_with_source(&path, source.to_vec())
            .unwrap();
        assert_eq!(file.symbols, fresh.symbols, "{rel_path}");
        assert_eq!(file.relations, fresh.relations, "{rel_path}");
        assert_eq!(
            file.symbols
                .iter()
                .find(|s| s.name == "nested")
                .unwrap()
                .qualified_name
                .as_deref(),
            expected_nested_qn,
            "{rel_path}"
        );
        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&[file], &mut builder).unwrap();
        let (nodes, edges) = builder.into_graph();
        let creates: Vec<_> = nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function && n.name.as_str() == "create")
            .collect();
        assert_eq!(creates.len(), 2);
        let free = creates
            .iter()
            .find(|n| n.qualified_name.as_deref() == expected_free_qn)
            .unwrap();
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

    // Two integration modules (`hue/sensor.py`, `mqtt/sensor.py`) each defining
    // `class Sensor: def setup(self)` plus module-level `def setup(hass)` must keep
    // distinct module-qualified `setup` functions without colliding with `Sensor.setup`.
    let sensor_src = b"class Sensor:\n    def setup(self):\n        return 1\ndef setup(hass):\n    return 2\ndef init(hass):\n    return setup(hass)\n";
    let hue = extractor
        .extract_file_with_source(Path::new("/repo/hue/sensor.py"), sensor_src.to_vec())
        .unwrap();
    let mqtt = extractor
        .extract_file_with_source(Path::new("/repo/mqtt/sensor.py"), sensor_src.to_vec())
        .unwrap();
    let mut builder = GraphBuilder::new();
    extractor
        .populate_graph(&[hue, mqtt], &mut builder)
        .unwrap();
    let (nodes, edges) = builder.into_graph();
    for pkg in ["hue", "mqtt"] {
        let init_node = nodes
            .iter()
            .find(|n| n.qualified_name.as_deref() == Some(&format!("{pkg}.sensor.init")))
            .unwrap();
        let setup_node = nodes
            .iter()
            .find(|n| n.qualified_name.as_deref() == Some(&format!("{pkg}.sensor.setup")))
            .unwrap();
        assert!(edges.iter().any(|e| {
            e.edge_type == EdgeType::Calls && e.from == init_node.id && e.to == setup_node.id
        }));
    }
    assert!(cache.hit_count() > 0);
}
