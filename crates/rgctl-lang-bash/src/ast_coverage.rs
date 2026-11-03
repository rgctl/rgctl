//! AST coverage manifest vs pinned `tree-sitter-bash` grammar.

use std::collections::{HashMap, HashSet};

const MANIFEST_JSON: &str = include_str!("../bash-ast-coverage.json");

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
        serde_json::from_str(MANIFEST_JSON).expect("bash-ast-coverage.json parse");
    let grammar = v["grammar"].as_str().unwrap_or("");
    assert!(
        grammar.starts_with("tree-sitter-bash@"),
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
    let lang: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
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
    fn bash_ast_coverage_manifest_matches_grammar() {
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
                "grammar kind {kind} missing from bash-ast-coverage.json"
            );
        }

        for key in manifest.keys() {
            assert!(
                kinds.contains(key),
                "manifest key {key} not in grammar named kinds"
            );
        }

        assert_eq!(
            manifest.get("function_definition").map(String::as_str),
            Some("Symbol"),
            "function_definition must be Symbol"
        );
        assert_eq!(
            manifest.get("command").map(String::as_str),
            Some("Relation"),
            "command must be Relation"
        );
        assert_eq!(
            manifest.get("declaration_command").map(String::as_str),
            Some("Symbol"),
            "declaration_command must be Symbol"
        );
        assert_eq!(
            manifest.get("command_substitution").map(String::as_str),
            Some("Relation"),
            "command_substitution must be Relation"
        );
        assert!(
            !manifest.values().any(|h| h == "Skip"),
            "raw Skip must be zero for Tier-1 A+"
        );
    }
}
