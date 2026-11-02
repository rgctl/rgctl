//! AST coverage manifest vs pinned `tree-sitter-hcl` grammar.

use std::collections::{HashMap, HashSet};

const MANIFEST_JSON: &str = include_str!("../hcl-ast-coverage.json");

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
        serde_json::from_str(MANIFEST_JSON).expect("hcl-ast-coverage.json parse");
    let grammar = v["grammar"].as_str().unwrap_or("");
    assert!(
        grammar.starts_with("tree-sitter-hcl@"),
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
    let lang: tree_sitter::Language = tree_sitter_hcl::LANGUAGE.into();
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
    fn hcl_ast_coverage_manifest_matches_grammar() {
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
                "grammar kind {kind} missing from hcl-ast-coverage.json"
            );
        }

        for key in manifest.keys() {
            assert!(
                kinds.contains(key),
                "manifest key {key} not in grammar named kinds"
            );
        }

        assert_eq!(
            manifest.get("block").map(String::as_str),
            Some("Symbol"),
            "block must be Symbol"
        );
        assert_eq!(
            manifest.get("function_call").map(String::as_str),
            Some("Relation"),
            "function_call must be Relation"
        );
        assert_eq!(
            manifest.get("attribute").map(String::as_str),
            Some("CfgStatement"),
            "attribute must be CfgStatement"
        );
        assert_eq!(
            manifest.get("conditional").map(String::as_str),
            Some("CfgStatement"),
            "conditional must be CfgStatement"
        );
        assert_eq!(
            manifest.get("for_expr").map(String::as_str),
            Some("CfgStatement"),
            "for_expr must be CfgStatement"
        );
        assert_eq!(
            manifest.get("variable_expr").map(String::as_str),
            Some("AstSkeleton"),
            "variable_expr must be AstSkeleton"
        );
        assert_eq!(
            manifest.get("get_attr").map(String::as_str),
            Some("AstSkeleton"),
            "get_attr must be AstSkeleton"
        );
        assert_eq!(
            manifest.get("template_directive").map(String::as_str),
            Some("AstSkeleton"),
            "template_directive must be AstSkeleton"
        );
        assert!(
            !manifest.values().any(|h| h == "Skip"),
            "hcl-ast-coverage.json must contain zero raw Skip entries"
        );
    }
}
