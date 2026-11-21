//! Exercise actual native producers; never manufacture indirect flags in fixtures.
use rgctl_extraction::{Extractor, GraphBuilder, facts_cache::FactsCache};
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_plugin_api::{RelationType, SymbolType};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(serde::Deserialize)]
struct Case {
    language: String,
    file: Option<String>,
    project: Option<String>,
    fixture: Option<String>,
    indirect_applicable: bool,
    reason: String,
    #[serde(default)]
    scope_after: bool,
}
fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("fixtures/tier1_call_targets/cases.json")).unwrap()
}
#[test]
fn native_tier1_producers_preserve_lexical_call_targets() {
    let registry = Arc::new(rgctl_languages::default_registry());
    let cache = Arc::new(FactsCache::new());
    let extractor = Extractor::new(registry.clone())
        .with_repo_root("/repo")
        .with_facts_cache(cache.clone());
    for case in cases() {
        assert!(!case.reason.is_empty());
        let Some(name) = case.file else {
            assert!(!case.indirect_applicable);
            continue;
        };
        let source = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(case.project.as_ref().unwrap())
                .join(case.fixture.as_ref().unwrap()),
        )
        .unwrap();
        let path = Path::new("/repo/scopes").join(&name);
        let plugin = registry.get_language_plugin(&case.language).unwrap();
        let all = plugin.extract_all(&path, &source).unwrap();
        let check = |relations: &[rgctl_plugin_api::Relation]| {
            if case.indirect_applicable {
                for owner in ["binding_parameter", "binding_local"] {
                    assert!(
                        relations
                            .iter()
                            .any(|r| r.relation_type == RelationType::Calls
                                && r.from.contains(owner)
                                && r.indirect_call),
                        "{} {owner}: {relations:?}",
                        case.language
                    );
                }
            } else {
                assert!(
                    relations.iter().all(|r| !r.indirect_call),
                    "{}",
                    case.language
                );
            }
            if case.scope_after {
                let values: Vec<_> = relations
                    .iter()
                    .filter(|r| {
                        r.relation_type == RelationType::Calls
                            && r.from.contains("binding_after")
                            && r.to.contains("binding_helper")
                    })
                    .map(|r| r.indirect_call)
                    .collect();
                assert_eq!(
                    values,
                    if case.language == "python" {
                        vec![true, true]
                    } else {
                        vec![true, false]
                    },
                    "{} scope boundaries",
                    case.language
                );
            }
            if case.language == "rust" {
                assert!(
                    relations.iter().any(|r| r.from.contains("binding_inner")
                        && r.to.contains("binding_helper")
                        && !r.indirect_call),
                    "Rust fn item must not capture outer parameter"
                );
            }
            assert!(
                relations
                    .iter()
                    .any(|r| r.relation_type == RelationType::Calls
                        && r.from.contains("binding_direct")
                        && !r.indirect_call),
                "{}: missing native direct call",
                case.language
            );
        };
        check(&all.relations);
        if !matches!(
            case.language.as_str(),
            "swift" | "scala" | "dart" | "elixir" | "lua" | "bash"
        ) {
            let symbols = plugin.extract_symbols(&path, &source).unwrap();
            let relations = plugin.extract_relations(&path, &source, &symbols).unwrap();
            check(&relations);
        }
        let file = extractor
            .extract_file_with_source(&path, source.clone())
            .unwrap();
        check(&file.relations);
        let cached = extractor
            .extract_file_with_source(&path, source.clone())
            .unwrap();
        assert_eq!(file.symbols, cached.symbols, "{}", case.language);
        assert_eq!(file.relations, cached.relations, "{}", case.language);
        let saved = tempfile::tempdir().unwrap();
        cache.save(saved.path()).unwrap();
        let restored = Arc::new(FactsCache::load(saved.path()));
        let moved_path = Path::new("/relocated/other").join(&name);
        let moved = Extractor::new(registry.clone())
            .with_repo_root("/relocated")
            .with_facts_cache(restored.clone())
            .extract_file_with_source(&moved_path, source.clone())
            .unwrap();
        let fresh = Extractor::new(registry.clone())
            .with_repo_root("/relocated")
            .extract_file_with_source(&moved_path, source.clone())
            .unwrap();
        assert_eq!(restored.hit_count(), 1, "{}", case.language);
        assert_eq!(moved.symbols, fresh.symbols, "{}", case.language);
        assert_eq!(moved.relations, fresh.relations, "{}", case.language);
        check(&moved.relations);
        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&[file], &mut builder).unwrap();
        let (nodes, edges) = builder.into_graph();
        let id = |name: &str| {
            nodes
                .iter()
                .find(|n| n.node_type == NodeType::Function && n.name.as_str() == name)
                .unwrap_or_else(|| panic!("{}: missing {name}", case.language))
                .id
        };
        let target = id("binding_helper");
        assert!(
            edges.iter().any(|e| e.edge_type == EdgeType::Calls
                && e.from == id("binding_direct")
                && e.to == target),
            "{}: missing direct edge",
            case.language
        );
        for owner in ["binding_parameter", "binding_local"] {
            assert!(
                !edges.iter().any(|e| e.edge_type == EdgeType::Calls
                    && e.from == id(owner)
                    && e.to == target),
                "{} {owner}: false global edge",
                case.language
            );
        }
        if case.scope_after {
            assert_eq!(
                edges.iter().any(|e| e.edge_type == EdgeType::Calls
                    && e.from == id("binding_after")
                    && e.to == target),
                case.language != "python",
                "{}: materialized scope boundary",
                case.language
            );
        }
        if case.language == "rust" {
            assert!(edges.iter().any(|e| e.edge_type == EdgeType::Calls
                && e.from == id("binding_inner")
                && e.to == target));
        }
        assert!(
            all.symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Function && s.name == "binding_helper")
        );
    }
}
#[test]
fn lexical_target_fixture_inventory_matches_all_tier1_languages() {
    let cases = cases();
    assert_eq!(cases.len(), 24);
    let actual: std::collections::BTreeSet<_> = cases.iter().map(|c| c.language.as_str()).collect();
    let expected: std::collections::BTreeSet<_> = rgctl_ast_coverage::bundled_specs()
        .iter()
        .map(|s| s.id.to_owned())
        .collect();
    assert_eq!(
        actual
            .into_iter()
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
}
