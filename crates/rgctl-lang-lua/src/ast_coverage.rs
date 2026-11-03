//! AST coverage manifest vs pinned `tree-sitter-lua` grammar.

use std::collections::{HashMap, HashSet};

const MANIFEST_JSON: &str = include_str!("../lua-ast-coverage.json");

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
        serde_json::from_str(MANIFEST_JSON).expect("lua-ast-coverage.json parse");
    let grammar = v["grammar"].as_str().unwrap_or("");
    assert!(
        grammar.starts_with("tree-sitter-lua@"),
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
    let lang: tree_sitter::Language = tree_sitter_lua::LANGUAGE.into();
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
    fn lua_ast_coverage_manifest_matches_grammar() {
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
                "grammar kind {kind} missing from lua-ast-coverage.json"
            );
        }

        for key in manifest.keys() {
            assert!(
                kinds.contains(key),
                "manifest key {key} not in grammar named kinds"
            );
        }

        for must_symbol in ["function_declaration", "function_definition"] {
            assert_eq!(
                manifest.get(must_symbol).map(String::as_str),
                Some("Symbol"),
                "{must_symbol} must be Symbol"
            );
        }
        assert_eq!(
            manifest.get("function_call").map(String::as_str),
            Some("Relation"),
            "function_call must be Relation"
        );
        assert_eq!(
            manifest.values().filter(|h| h.as_str() == "Skip").count(),
            0,
            "raw Skip must be zero for Tier-1 Gold"
        );
    }
}
