//! AST coverage manifest vs pinned `tree-sitter-elixir` grammar.

use std::collections::{HashMap, HashSet};

const MANIFEST_JSON: &str = include_str!("../elixir-ast-coverage.json");

const ALLOWED: &[&str] = &[
    "Symbol",
    "Relation",
    "CfgStatement",
    "AstSkeleton",
    "SkipWrapper",
    "SkipTypeToken",
    "Unimplemented",
    "Literal",
];

pub fn load_manifest() -> HashMap<String, String> {
    let v: serde_json::Value =
        serde_json::from_str(MANIFEST_JSON).expect("elixir-ast-coverage.json parse");
    let grammar = v["grammar"].as_str().unwrap_or("");
    assert!(
        grammar.starts_with("tree-sitter-elixir@"),
        "unexpected grammar pin: {grammar}"
    );
    v["handlers"]
        .as_object()
        .expect("handlers object")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("Unimplemented").to_string()))
        .collect()
}

/// Named node kinds from the pinned grammar (unique names).
pub fn grammar_named_kinds() -> HashSet<String> {
    let lang: tree_sitter::Language = tree_sitter_elixir::LANGUAGE.into();
    let mut set = HashSet::new();
    for i in 0..lang.node_kind_count() {
        if lang.node_kind_is_named(i as u16)
            && let Some(k) = lang.node_kind_for_id(i as u16)
        {
            set.insert(k.to_string());
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elixir_ast_coverage_manifest_matches_grammar() {
        let manifest = load_manifest();
        for handler in manifest.values() {
            assert!(
                ALLOWED.contains(&handler.as_str()),
                "invalid handler {handler}"
            );
        }

        let kinds = grammar_named_kinds();
        for kind in &kinds {
            assert!(
                manifest.contains_key(kind),
                "grammar kind {kind} missing from elixir-ast-coverage.json"
            );
        }

        for key in manifest.keys() {
            assert!(
                kinds.contains(key),
                "manifest key {key} not in grammar named kinds"
            );
        }

        assert_eq!(
            manifest.get("call").map(String::as_str),
            Some("Symbol"),
            "call must be Symbol (defmodule/def/import dispatch at runtime)"
        );
        assert_eq!(
            manifest.get("access_call").map(String::as_str),
            Some("Relation"),
            "access_call must be Relation"
        );
        assert_eq!(
            manifest.get("alias").map(String::as_str),
            Some("Relation"),
            "alias must be Relation"
        );
        assert_eq!(
            manifest.get("stab_clause").map(String::as_str),
            Some("CfgStatement"),
            "stab_clause must be CfgStatement"
        );
        assert_eq!(
            manifest.get("binary_operator").map(String::as_str),
            Some("CfgStatement"),
            "binary_operator must be CfgStatement"
        );
        assert_eq!(
            manifest.values().filter(|h| h.as_str() == "Skip").count(),
            0,
            "raw Skip must be reclassified to SkipWrapper/SkipTypeToken or an active handler"
        );
    }
}
