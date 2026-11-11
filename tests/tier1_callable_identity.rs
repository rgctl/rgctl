//! Real examples for the complete Tier 1 inventory. Declarative/template
//! languages are explicit non-applicable cases, not invented overload fixtures.
use rgctl_analysis::mro::{InheritanceGraph, MroIndex, MroStrategy};
use rgctl_extraction::{Extractor, GraphBuilder};
use rgctl_graph::{
    backend::MemoryBackend,
    schema::{Edge, Node, NodeType},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tier1_callable_identity")
}
fn cases() -> Vec<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(root().join("inventory.json")).unwrap()).unwrap()
}
fn graph(file: &str, source: &str) -> (Vec<Node>, Vec<Edge>) {
    static EXTRACTOR: std::sync::OnceLock<Extractor> = std::sync::OnceLock::new();
    let extractor =
        EXTRACTOR.get_or_init(|| Extractor::new(Arc::new(rgctl_languages::default_registry())));
    let extraction = extractor
        .extract_file_with_source(Path::new(file), source.as_bytes().to_vec())
        .unwrap();
    let mut builder = GraphBuilder::new();
    extractor
        .populate_graph(&[extraction], &mut builder)
        .unwrap();
    builder.into_graph()
}
fn function_keys(nodes: &[Node]) -> BTreeMap<String, (uuid::Uuid, Option<String>)> {
    nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Function)
        .map(|n| {
            (
                format!(
                    "{}\u{1f}{}",
                    n.qualified_name.as_deref().unwrap_or(&n.name),
                    n.identity_discriminator().unwrap_or("")
                ),
                (n.id, n.code_hash.as_ref().map(ToString::to_string)),
            )
        })
        .collect()
}
#[test]
fn inventory_covers_every_registered_tier1_language() {
    let actual: BTreeSet<_> = cases()
        .iter()
        .map(|c| c["language"].as_str().unwrap().to_owned())
        .collect();
    let expected: BTreeSet<_> = rgctl_ast_coverage::bundled_specs()
        .iter()
        .map(|spec| spec.id.to_owned())
        .collect();
    assert_eq!(actual, expected);
}
#[test]
fn all_tier1_callables_survive_body_edits_line_shifts_parameter_renames_and_snapshots() {
    let expected = BTreeMap::from([
        ("c", 3),
        ("cpp", 3),
        ("csharp", 3),
        ("go", 2),
        ("groovy", 3),
        ("java", 3),
        ("javascript", 2),
        ("kotlin", 3),
        ("markdown", 0),
        ("php", 2),
        ("puppet", 2),
        ("erb", 0),
        ("python", 2),
        ("ruby", 2),
        ("rust", 4),
        ("typescript", 4),
        ("swift", 3),
        ("scala", 3),
        ("dart", 2),
        ("elixir", 3),
        ("lua", 2),
        ("bash", 2),
        ("sql", 2),
        ("hcl", 0),
    ]);
    for case in cases() {
        let language = case["language"].as_str().unwrap();
        let file = case["file"].as_str().unwrap();
        let source = std::fs::read_to_string(root().join(file)).unwrap();
        let (nodes, edges) = graph(file, &source);
        let before = function_keys(&nodes);
        assert_eq!(
            before.len(),
            expected[language],
            "{language}: callable declarations collapsed or missing: {before:?}"
        );
        if before.is_empty() {
            assert!(!case["semantics"].as_str().unwrap().is_empty());
            continue;
        }
        let marker = if source.contains("21") { "21" } else { "11" };
        let candidate = source.replacen(marker, "22", 1);
        let (changed, _) = graph(file, &candidate);
        let after = function_keys(&changed);
        assert_eq!(
            before.keys().collect::<Vec<_>>(),
            after.keys().collect::<Vec<_>>(),
            "{language}: body changed identity"
        );
        let mut count = 0;
        for (key, (id, hash)) in &before {
            assert_eq!(*id, after[key].0, "{language}: UUID changed");
            assert!(hash.is_some(), "{language}: missing code hash for {key}");
            if *hash != after[key].1 {
                count += 1;
            }
        }
        assert_eq!(
            count, 1,
            "{language}: body change leaked across callable boundaries"
        );
        let shifted = graph(file, &format!("\n\n{source}")).0;
        let renamed = graph(file, &source.replace("value", "renamed")).0;
        for variant in [shifted, renamed] {
            let keys = function_keys(&variant);
            assert_eq!(
                before.keys().collect::<Vec<_>>(),
                keys.keys().collect::<Vec<_>>(),
                "{language}: layout/parameter spelling changed identity"
            );
            for (key, (id, _)) in &before {
                assert_eq!(*id, keys[key].0, "{language}: layout/rename changed UUID");
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("graph.bin");
        rgctl_graph::write_columnar_from_nodes_edges(nodes, edges, &path).unwrap();
        let loaded = rgctl_graph::CodeGraph::open_snapshot(&path)
            .unwrap()
            .snapshot()
            .unwrap();
        assert_eq!(
            before,
            function_keys(&loaded.nodes),
            "{language}: snapshot lost callable identity"
        );
    }
}
#[test]
fn inherited_overrides_never_bind_a_different_overload() {
    for language in [
        "cpp",
        "csharp",
        "groovy",
        "java",
        "kotlin",
        "swift",
        "scala",
        "javascript",
        "typescript",
        "python",
        "ruby",
        "php",
        "dart",
    ] {
        let case = cases()
            .into_iter()
            .find(|c| c["language"] == language)
            .unwrap();
        let file = case["file"].as_str().unwrap();
        let source = std::fs::read_to_string(root().join(file)).unwrap();
        let (nodes, edges) = graph(file, &source);
        let child = nodes
            .iter()
            .find(|n| {
                n.node_type == NodeType::Function
                    && n.qualified_name.as_deref().is_some_and(|q| {
                        q.starts_with("Child.")
                            || q.starts_with("Child::")
                            || q.starts_with("Child#")
                    })
            })
            .unwrap_or_else(|| panic!("{language}: child missing: {nodes:?}"));
        let expected = nodes
            .iter()
            .find(|n| {
                n.node_type == NodeType::Function
                    && n.qualified_name.as_deref().is_some_and(|q| {
                        q.starts_with("Base.") || q.starts_with("Base::") || q.starts_with("Base#")
                    })
                    && n.identity_discriminator() == child.identity_discriminator()
            })
            .unwrap_or_else(|| panic!("{language}: base missing: {nodes:?}"));
        let extracted_overrides: Vec<_> = edges
            .iter()
            .filter(|edge| {
                edge.edge_type == rgctl_graph::schema::EdgeType::Overrides && edge.from == child.id
            })
            .collect();
        assert_eq!(
            extracted_overrides.len(),
            1,
            "{language}: extraction-time override missing/ambiguous"
        );
        assert_eq!(
            extracted_overrides[0].to, expected.id,
            "{language}: extraction-time override chose another overload"
        );
        let mut backend = MemoryBackend::new();
        backend.insert_nodes_batch(nodes.clone()).unwrap();
        backend.insert_edges_batch(edges).unwrap();
        let mut index = MroIndex::build(
            InheritanceGraph::from_backend(&backend).unwrap(),
            MroStrategy::from_language(language),
        )
        .unwrap();
        index.index_methods_from_backend(&backend).unwrap();
        let overrides = index.overrides_edges();
        assert!(
            overrides
                .iter()
                .any(|edge| edge.from == child.id && edge.to == expected.id),
            "{language}: matching override missing"
        );
        assert!(
            !overrides
                .iter()
                .any(|edge| edge.from == child.id && edge.to != expected.id),
            "{language}: override selected another overload"
        );
    }
}

#[test]
fn literal_calls_bind_their_own_overloads_and_callers() {
    let examples = [
        (
            "cpp",
            "calls.cpp",
            "class Example { public: int quote(int value){return 11;} int quote(const char* value){return 21;} int intCaller(){return quote(7);} int stringCaller(){return quote(\"hello\");} };",
            "int",
            "constchar*",
        ),
        (
            "csharp",
            "calls.cs",
            "class Example { int Quote(int value){return 11;} int Quote(string value){return 21;} int intCaller(){return Quote(7);} int stringCaller(){return Quote(\"hello\");} }",
            "int",
            "string",
        ),
        (
            "groovy",
            "calls.groovy",
            "class Example {\nint quote(int value){return 11}\nint quote(String value){return 21}\nint intCaller(){return quote(7)}\nint stringCaller(){return quote(\"hello\")}\n}",
            "int",
            "String",
        ),
        (
            "java",
            "Calls.java",
            "class Example { int quote(int value){return 11;} int quote(String value){return 21;} int intCaller(){return quote(7);} int stringCaller(){return quote(\"hello\");} }",
            "int",
            "String",
        ),
        (
            "kotlin",
            "calls.kt",
            "class Example {\nfun quote(value: Int): Int { return 11 }\nfun quote(value: String): Int { return 21 }\nfun intCaller(): Int { return quote(7) }\nfun stringCaller(): Int { return quote(\"hello\") }\n}",
            "Int",
            "String",
        ),
        (
            "swift",
            "calls.swift",
            "class Example {\nfunc quote(_ value: Int) -> Int { return 11 }\nfunc quote(_ value: String) -> Int { return 21 }\nfunc intCaller() -> Int { return quote(7) }\nfunc stringCaller() -> Int { return quote(\"hello\") }\n}",
            "_:Int",
            "_:String",
        ),
        (
            "scala",
            "calls.scala",
            "class Example {\ndef quote(value: Int): Int = { return 11 }\ndef quote(value: String): Int = { return 21 }\ndef intCaller(): Int = quote(7)\ndef stringCaller(): Int = quote(\"hello\")\n}",
            "Int",
            "String",
        ),
    ];
    for (language, file, source, int, string) in examples {
        let (nodes, edges) = graph(file, source);
        for (caller, parameter) in [("intCaller", int), ("stringCaller", string)] {
            let from = nodes
                .iter()
                .find(|n| n.name == caller)
                .unwrap_or_else(|| panic!("{language}: missing caller {caller}"));
            let expected = nodes
                .iter()
                .find(|n| {
                    n.node_type == NodeType::Function
                        && n.identity_discriminator() == Some(&format!("({parameter})"))
                })
                .unwrap_or_else(|| panic!("{language}: missing overload {parameter}"));
            let calls: Vec<_> = edges
                .iter()
                .filter(|edge| {
                    edge.from == from.id && edge.edge_type == rgctl_graph::schema::EdgeType::Calls
                })
                .collect();
            assert_eq!(
                calls.len(),
                1,
                "{language}: missing or extra calls from {caller}: {calls:?}"
            );
            assert_eq!(
                calls[0].to, expected.id,
                "{language}: caller bound the wrong overload"
            );
        }
    }
}
#[test]
fn rust_trait_implementations_override_only_their_own_trait() {
    let source = std::fs::read_to_string(root().join("rust/example.rs")).unwrap();
    let (nodes, edges) = graph("example.rs", &source);
    let mut backend = MemoryBackend::new();
    backend.insert_nodes_batch(nodes.clone()).unwrap();
    backend.insert_edges_batch(edges).unwrap();
    let mut index = MroIndex::build(
        InheritanceGraph::from_backend(&backend).unwrap(),
        MroStrategy::RustInherentOverTrait,
    )
    .unwrap();
    index.index_methods_from_backend(&backend).unwrap();
    let overrides = index.overrides_edges();
    for name in ["One", "Two"] {
        let implementation = nodes
            .iter()
            .find(|n| n.identity_discriminator() == Some(&format!("trait:{name}")))
            .unwrap();
        let declared = nodes
            .iter()
            .find(|n| n.qualified_name.as_deref() == Some(&format!("{name}::quote")))
            .unwrap();
        assert!(
            overrides
                .iter()
                .any(|edge| edge.from == implementation.id && edge.to == declared.id)
        );
        assert!(
            !overrides
                .iter()
                .any(|edge| edge.from == implementation.id && edge.to != declared.id)
        );
    }
}

#[test]
fn generic_arity_is_identity_and_type_parameter_spelling_is_not() {
    for (file, source) in [
        (
            "generic.cs",
            "class Example { T quote<T>() {return default(T);} T quote<T,U>() {return default(T);} }",
        ),
        (
            "generic.cpp",
            "template<class T> int quote(){return 11;} template<class T,class U> int quote(){return 21;}",
        ),
    ] {
        let before = function_keys(&graph(file, source).0);
        assert_eq!(
            before.len(),
            2,
            "{file}: generic arities collapsed: {before:?}"
        );
        let after = function_keys(&graph(file, &source.replace("T", "Renamed")).0);
        assert_eq!(
            before.keys().collect::<Vec<_>>(),
            after.keys().collect::<Vec<_>>(),
            "{file}: type parameter spelling changed identity"
        );
    }
}

#[test]
fn cpp_function_pointer_parameter_names_are_not_part_of_identity() {
    let source = "int quote(void (*callback)(int item)){return 11;} int quote(void (*callback)(double item)){return 21;}";
    let before = function_keys(&graph("pointers.cpp", source).0);
    assert_eq!(before.len(), 2);
    let changed = function_keys(
        &graph(
            "pointers.cpp",
            &source
                .replace("callback", "renamed")
                .replace("item", "another"),
        )
        .0,
    );
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        changed.keys().collect::<Vec<_>>()
    );
}

#[test]
fn receiver_qualifiers_labels_modifiers_and_sql_schemas_distinguish_callables() {
    for (file, source) in [
        (
            "qualifiers.cpp",
            "class Example { int quote(int value){return 11;} int quote(int value) const {return 21;} };",
        ),
        (
            "modifiers.cs",
            "class Example { int quote(int value){return 11;} int quote(ref int value){return 21;} }",
        ),
        (
            "labels.swift",
            "func quote(left value: Int) -> Int {return 11}\nfunc quote(right value: Int) -> Int {return 21}\n",
        ),
        (
            "receivers.kt",
            "fun Int.quote(): Int { return 11 }\nfun String.quote(): Int { return 21 }\n",
        ),
        (
            "schemas.sql",
            "CREATE FUNCTION first.quote(value INTEGER) RETURNS INTEGER AS $$ BEGIN RETURN 11; END; $$ LANGUAGE plpgsql;\nCREATE FUNCTION second.quote(value INTEGER) RETURNS INTEGER AS $$ BEGIN RETURN 21; END; $$ LANGUAGE plpgsql;\n",
        ),
    ] {
        let functions = function_keys(&graph(file, source).0);
        assert_eq!(
            functions.len(),
            2,
            "{file}: dispatch identities collapsed: {functions:?}"
        );
    }
}
#[test]
fn dynamic_redefinitions_keep_both_bodies_and_bind_calls_to_the_active_definition() {
    for (file, source) in [
        (
            "redefined.py",
            "def quote(value):\n    return 11\ndef quote(value):\n    return 21\ndef caller(value):\n    return quote(value)\n",
        ),
        (
            "redefined.js",
            "function quote(value){return 11;}\nfunction quote(value){return 21;}\nfunction caller(value){return quote(value);}\n",
        ),
        (
            "redefined.rb",
            "def quote(value)\nreturn 11\nend\ndef quote(value)\nreturn 21\nend\ndef caller(value)\nquote(value)\nend\n",
        ),
    ] {
        let (nodes, edges) = graph(file, source);
        let active = nodes
            .iter()
            .find(|n| n.identity_discriminator() == Some("definition:1"))
            .unwrap();
        let caller = nodes.iter().find(|n| n.name == "caller").unwrap();
        let calls: Vec<_> = edges
            .iter()
            .filter(|e| e.from == caller.id && e.edge_type == rgctl_graph::schema::EdgeType::Calls)
            .collect();
        assert_eq!(
            function_keys(&nodes).len(),
            3,
            "{file}: definition discarded"
        );
        assert_eq!(calls.len(), 1, "{file}: active binding not unique");
        assert_eq!(
            calls[0].to, active.id,
            "{file}: call selected the superseded definition"
        );
    }
}

#[test]
fn discover_rebuilds_stale_semantics_instead_of_reusing_collapsed_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path();
    std::fs::write(
        repo.join("Example.java"),
        "class Example { int quote(int value){return 11;} int quote(String value){return 21;} }",
    )
    .unwrap();
    let discover = |extra: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rgctl"))
            .current_dir(repo)
            .args(["discover", "."])
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "discover failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    let snapshot = rgctl_graph::snapshot::MmappedGraphSnapshot::default_path(repo);
    let stamp = repo.join(".rgctl/extraction_semantics.json");
    discover(&[]);
    assert!(rgctl_extraction::callable_resolution::semantics_current(
        repo
    ));
    for incremental in [false, true] {
        // Simulate an older extractor's collapsed snapshot, while source hashes stay unchanged.
        let mut loaded = rgctl_graph::CodeGraph::open_snapshot(&snapshot)
            .unwrap()
            .snapshot()
            .unwrap();
        let victim = loaded
            .nodes
            .iter()
            .find(|n| n.node_type == NodeType::Function)
            .unwrap()
            .id;
        loaded.nodes.retain(|n| n.id != victim);
        loaded.edges.retain(|e| e.from != victim && e.to != victim);
        rgctl_graph::write_columnar_from_nodes_edges(loaded.nodes, loaded.edges, &snapshot)
            .unwrap();
        if incremental {
            std::fs::write(&stamp, "{\"version\":0}").unwrap();
            discover(&["--files", "Example.java"]);
        } else {
            std::fs::remove_file(&stamp).unwrap();
            discover(&[]);
        }
        assert!(rgctl_extraction::callable_resolution::semantics_current(
            repo
        ));
        let loaded = rgctl_graph::CodeGraph::open_snapshot(&snapshot)
            .unwrap()
            .snapshot()
            .unwrap();
        assert_eq!(
            function_keys(&loaded.nodes).len(),
            2,
            "stale snapshot reused"
        );
        assert!(repo.join(".rgctl/callable_resolution.json").is_file());
    }
}

#[test]
fn unresolved_call_evidence_is_bounded_and_replaced_after_recovery() {
    let extractor = Extractor::new(Arc::new(rgctl_languages::default_registry()));
    let directory = tempfile::tempdir().unwrap();
    let calls = "quote(unknown());".repeat(1030);
    for body in [calls.as_str(), "quote(7);"] {
        let source = format!(
            "class Example {{ int quote(int value){{return 11;}} int quote(string value){{return 21;}} void caller(){{{body}}} }}"
        );
        let extraction = extractor
            .extract_file_with_source(Path::new("Example.cs"), source.into_bytes())
            .unwrap();
        let mut builder = GraphBuilder::new();
        extractor
            .populate_graph(&[extraction], &mut builder)
            .unwrap();
        rgctl_extraction::callable_resolution::save(directory.path(), &mut builder).unwrap();
        let report: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.path().join(".rgctl/callable_resolution.json")).unwrap(),
        )
        .unwrap();
        if body == calls {
            assert!(report["unresolved_calls"].as_u64().unwrap() > 1024);
            assert_eq!(report["samples"].as_array().unwrap().len(), 1024);
            assert_eq!(report["samples_truncated"], true);
        } else {
            assert_eq!(report["unresolved_calls"], 0);
            assert!(report["samples"].as_array().unwrap().is_empty());
        }
        // Diagnostic persistence alone must not bless a snapshot that wasn't written.
        assert!(!rgctl_extraction::callable_resolution::semantics_current(
            directory.path()
        ));
    }
}

#[test]
fn extraction_never_overrides_a_unique_parent_with_a_different_signature() {
    for (file, source) in [
        (
            "Mismatch.java",
            "class Base { int quote(int value){return 11;} } class Child extends Base { int quote(String value){return 21;} }",
        ),
        (
            "mismatch.cs",
            "class Base { int quote(int value){return 11;} } class Child : Base { int quote(string value){return 21;} }",
        ),
        (
            "mismatch.cpp",
            "class Base { public: int quote(int value){return 11;} }; class Child : public Base { public: int quote(const char* value){return 21;} };",
        ),
    ] {
        let (nodes, edges) = graph(file, source);
        let child = nodes
            .iter()
            .find(|n| {
                n.node_type == NodeType::Function
                    && n.qualified_name
                        .as_deref()
                        .is_some_and(|qn| qn.starts_with("Child.") || qn.starts_with("Child::"))
            })
            .unwrap();
        assert!(
            !edges
                .iter()
                .any(|e| e.from == child.id
                    && e.edge_type == rgctl_graph::schema::EdgeType::Overrides),
            "{file}: wrong-signature override emitted"
        );
    }
}
