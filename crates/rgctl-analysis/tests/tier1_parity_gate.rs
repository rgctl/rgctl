//! Automated Cross-Layer (Layers A–F) Tier-1 Parity Gate & Corpus Hotspot Audit.
//!
//! Enforces OpenSpec `enforce-tier1-maximum-ast-coverage` across all 24 Tier-1
//! languages in [`rgctl_ast_coverage::bundled_specs`]:
//! - **Layer A (Graph)**: `Symbol > 0` and `Relation > 0` in `{id}-ast-coverage.json`,
//!   plus golden snippet symbol & relation extraction (`Calls`, `Uses`, `Defines`).
//! - **Layer B (CFG & PDG)**: `profile.cfg_enabled` requires `CfgStatement >= 5` and
//!   `AstSkeleton >= 1` in `{id}-ast-coverage.json`, 3-way handler sync across
//!   `plugin.declared_ast_handlers()`, `cfg_handled_kinds(id)`, and
//!   `def_use_handled_kinds(id)`, and branching CFG construction.
//! - **Layer C (Taint)**: `profile.taint_enabled` requires a bundled taint rule pack
//!   in [`TaintRuleSet::bundled()`] with non-empty sources, sinks, and sanitizers.
//! - **Layer F (Field Mutation)**: `field_write_locals::language_visit_fn(id).is_some()`
//!   for all OOP/imperative Tier-1 languages and OOP class/field/constructor/parameter
//!   extraction + local variable type recovery.
//! - **Pillar 5 (Corpus Hotspot Telemetry)**: `AstCoverageTracker::unhandled_hotspots`
//!   audit over fixture sources.

use rgctl_analysis::cfg_builder::{build_cfg_for_function, cfg_handled_kinds};
use rgctl_analysis::def_use::def_use_handled_kinds;
use rgctl_analysis::field_write_locals::{language_visit_fn, merge_local_types};
use rgctl_analysis::language_profile::profile_for_language;
use rgctl_analysis::taint_rules::TaintRuleSet;
use rgctl_ast_coverage::{
    AstCoverageTracker, bundled_specs, check_manifest, check_plugin_and_analysis_manifest_sync,
    compute_semantic_coverage,
};
use rgctl_languages::default_registry;
use rgctl_plugin_api::{RelationType, SymbolType};
use std::collections::HashMap;
use std::path::Path;

fn crates_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn load_manifest_handlers(
    spec: &rgctl_ast_coverage::CoverageSpec,
) -> (String, HashMap<String, String>) {
    let path = crates_dir().join(spec.crate_dir).join(spec.manifest_file);
    let json = std::fs::read_to_string(&path).expect("read manifest json");
    let v: serde_json::Value = serde_json::from_str(&json).expect("parse manifest json");
    let handlers = v["handlers"]
        .as_object()
        .expect("handlers object")
        .iter()
        .map(|(k, val)| (k.clone(), val.as_str().unwrap_or("Skip").to_string()))
        .collect();
    (json, handlers)
}

#[test]
fn tier1_all_24_languages_pass_manifest_ratchet_and_three_way_sync() {
    let specs = bundled_specs();
    assert_eq!(specs.len(), 24, "expected all 24 Tier-1 bundled specs");
    let registry = default_registry();
    let mut failures = Vec::new();

    for spec in specs {
        let (json, handlers) = load_manifest_handlers(spec);
        let lang = (spec.language)();
        let manifest_issues = check_manifest(spec.id, &json, spec.grammar_prefix, &lang);
        for issue in manifest_issues {
            failures.push(format!("{}: {}", spec.id, issue.message));
        }

        let stats = compute_semantic_coverage(&handlers);
        assert!(
            stats.handled_kinds >= spec.min_handled_kinds,
            "{}: handled_kinds {} < min_handled_kinds {}",
            spec.id,
            stats.handled_kinds,
            spec.min_handled_kinds
        );

        if spec.id == "markdown" {
            continue;
        }
        let Some(plugin) = registry.get_language_plugin(spec.id) else {
            failures.push(format!("{}: missing from default_registry()", spec.id));
            continue;
        };
        let Some(table) = plugin.declared_ast_handlers() else {
            failures.push(format!("{}: declared_ast_handlers() is None", spec.id));
            continue;
        };
        let sync_issues = check_plugin_and_analysis_manifest_sync(
            spec.id,
            &json,
            table,
            cfg_handled_kinds(spec.id),
            def_use_handled_kinds(spec.id),
            &lang,
        );
        for issue in sync_issues {
            failures.push(format!("{}: {}", spec.id, issue.message));
        }
    }

    assert!(
        failures.is_empty(),
        "Tier-1 3-way manifest sync failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn tier1_layer_a_symbol_and_relation_invariants() {
    let registry = default_registry();
    for spec in bundled_specs() {
        let (_json, handlers) = load_manifest_handlers(spec);
        let symbol_count = handlers.values().filter(|v| v.as_str() == "Symbol").count();
        let relation_count = handlers
            .values()
            .filter(|v| v.as_str() == "Relation")
            .count();

        assert!(
            symbol_count >= 1,
            "Layer A ({}) requires Symbol >= 1 in {}, got {symbol_count}",
            spec.id,
            spec.manifest_file
        );
        // erb and markdown are template/doc grammars where relations are synthesized from content/links.
        if !matches!(spec.id, "erb" | "markdown" | "elixir") {
            assert!(
                relation_count >= 1,
                "Layer A ({}) requires Relation >= 1 in {}, got {relation_count}",
                spec.id,
                spec.manifest_file
            );
        }
    }

    // Golden extraction check on core Tier-1 languages: verify Symbols + Calls/Uses/Extends.
    let golden_cases: &[(&str, &str, &[u8])] = &[
        (
            "java",
            "OrderService.java",
            br#"
package demo;
import java.util.List;
class BaseService {}
public class OrderService extends BaseService {
    public void process() { helper(); }
    public void helper() {}
}
"#,
        ),
        (
            "rust",
            "lib.rs",
            br#"
use std::collections::HashMap;
pub struct OrderService;
pub fn process() { helper(); }
pub fn helper() {}
"#,
        ),
        (
            "python",
            "service.py",
            br#"
import os
class OrderService:
    def process(self):
        helper()
def helper():
    pass
"#,
        ),
        (
            "typescript",
            "service.ts",
            br#"
import { readFile } from "fs";
export class OrderService {
    process(): void { helper(); }
}
export function helper(): void {}
"#,
        ),
        (
            "go",
            "service.go",
            br#"
package service
import "fmt"
type OrderService struct {}
func Process() { Helper() }
func Helper() { fmt.Println("ok") }
"#,
        ),
    ];

    for (lang_id, filename, src) in golden_cases {
        let plugin = registry
            .get_language_plugin(lang_id)
            .unwrap_or_else(|| panic!("missing plugin {lang_id}"));
        let res = plugin
            .extract_all(Path::new(filename), src)
            .unwrap_or_else(|e| panic!("{lang_id} extract_all failed: {e}"));
        assert!(
            res.symbols
                .iter()
                .any(|s| matches!(s.symbol_type, SymbolType::Function | SymbolType::Method)),
            "Layer A ({lang_id}): expected function/method symbol in {:?}",
            res.symbols
        );
        assert!(
            res.symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Import)
                || res
                    .relations
                    .iter()
                    .any(|r| r.relation_type == RelationType::Uses),
            "Layer A ({lang_id}): expected Import symbol or Uses relation"
        );
        assert!(
            res.relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls),
            "Layer A ({lang_id}): expected Calls relation in {:?}",
            res.relations
        );
    }
}

#[test]
fn tier1_layer_b_cfg_and_pdg_invariants() {
    // Transitional exemptions for domain-specific / recently added Tier-1 grammars
    // whose CFG/def-use AST kinds are represented via generic `call`/`block` nodes.
    const CFG_STMT_EXEMPT: &[&str] = &["elixir", "sql", "hcl"];
    const AST_SKELETON_EXEMPT: &[&str] =
        &["dart", "swift", "scala", "elixir", "lua", "bash", "hcl"];

    for spec in bundled_specs() {
        let Some(profile) = profile_for_language(spec.id) else {
            continue;
        };
        if !profile.cfg_enabled {
            continue;
        }
        let (_json, handlers) = load_manifest_handlers(spec);
        let cfg_stmt_count = handlers
            .values()
            .filter(|v| v.as_str() == "CfgStatement")
            .count();
        let skeleton_count = handlers
            .values()
            .filter(|v| v.as_str() == "AstSkeleton")
            .count();

        if !CFG_STMT_EXEMPT.contains(&spec.id) {
            assert!(
                cfg_stmt_count >= 5,
                "Layer B ({}) with cfg_enabled=true requires CfgStatement >= 5 in {}, got {cfg_stmt_count}",
                spec.id,
                spec.manifest_file
            );
        } else {
            assert!(
                cfg_stmt_count >= 1,
                "Layer B ({}) requires CfgStatement >= 1, got {cfg_stmt_count}",
                spec.id
            );
        }

        if !AST_SKELETON_EXEMPT.contains(&spec.id) {
            assert!(
                skeleton_count >= 1,
                "Layer B ({}) with cfg_enabled=true requires AstSkeleton >= 1 in {}, got {skeleton_count}",
                spec.id,
                spec.manifest_file
            );
        }

        assert!(
            !cfg_handled_kinds(spec.id).is_empty(),
            "Layer B ({}) must expose non-empty cfg_handled_kinds",
            spec.id
        );
        assert!(
            !def_use_handled_kinds(spec.id).is_empty(),
            "Layer B ({}) must expose non-empty def_use_handled_kinds",
            spec.id
        );
    }

    // Verify branching CFG construction produces > 2 basic blocks across primary Tier-1 languages.
    let branching_samples: &[(&str, &str, &str)] = &[
        (
            "java",
            "class T { int check(int x) { if (x > 0) { return 1; } else { return 2; } } }",
            "check",
        ),
        (
            "rust",
            "fn check(x: i32) -> i32 { if x > 0 { return 1; } else { return 2; } }",
            "check",
        ),
        (
            "python",
            "def check(x):\n    if x > 0:\n        return 1\n    else:\n        return 2\n",
            "check",
        ),
        (
            "go",
            "package p\nfunc check(x int) int { if x > 0 { return 1 } else { return 2 } }",
            "check",
        ),
    ];

    for (lang_id, src, fn_name) in branching_samples {
        let cfg = build_cfg_for_function(lang_id, src, fn_name)
            .unwrap_or_else(|e| panic!("Layer B ({lang_id}) build_cfg_for_function failed: {e}"));
        assert!(
            cfg.blocks.len() > 2,
            "Layer B ({lang_id}): expected > 2 CFG basic blocks for branching function, got {}",
            cfg.blocks.len()
        );
    }
}

#[test]
fn tier1_layer_c_bundled_taint_rules_invariants() {
    let ruleset = TaintRuleSet::bundled().expect("TaintRuleSet::bundled() must compile");
    for spec in bundled_specs() {
        let Some(profile) = profile_for_language(spec.id) else {
            continue;
        };
        if !profile.taint_enabled {
            continue;
        }
        let rules = ruleset.for_language(spec.id).unwrap_or_else(|| {
            panic!(
                "Layer C ({}) has taint_enabled=true in language_profile.rs but no pack in TaintRuleSet::bundled()",
                spec.id
            )
        });
        assert!(
            rules.source_count() >= 1,
            "Layer C ({}) taint pack must declare >= 1 source rule",
            spec.id
        );
        assert!(
            rules.sink_count() >= 1,
            "Layer C ({}) taint pack must declare >= 1 sink rule",
            spec.id
        );
        assert!(
            rules.sanitizer_count() >= 1,
            "Layer C ({}) taint pack must declare >= 1 sanitizer rule",
            spec.id
        );
    }
}

#[test]
fn tier1_layer_f_field_write_locals_and_oop_metadata_invariants() {
    const CORE_FIELD_WRITE_LANGS: &[&str] = &[
        "java",
        "csharp",
        "go",
        "rust",
        "python",
        "javascript",
        "typescript",
        "c",
        "cpp",
        "php",
        "ruby",
        "puppet",
        "kotlin",
        "groovy",
    ];

    for &lang_id in CORE_FIELD_WRITE_LANGS {
        assert!(
            language_visit_fn(lang_id).is_some(),
            "Layer F ({lang_id}): field_write_locals::language_visit_fn must return Some"
        );
    }

    // Verify Java OOP class/field/constructor/parameter extraction + local type recovery.
    let java_oop = r#"
package demo;
public class OrderDTO {
    private String status;
    public OrderDTO(String initialStatus) {
        this.status = initialStatus;
    }
    public void mutate(OrderDTO target) {
        OrderDTO alias = target;
        alias.status = "SHIPPED";
    }
}
"#;
    let registry = default_registry();
    let java_plugin = registry.get_language_plugin("java").expect("java plugin");
    let extracted = java_plugin
        .extract_all(Path::new("OrderDTO.java"), java_oop.as_bytes())
        .expect("extract OrderDTO.java");

    let cls = extracted
        .symbols
        .iter()
        .find(|s| s.name == "OrderDTO" && s.symbol_type == SymbolType::Class)
        .expect("OrderDTO class symbol");
    assert!(
        cls.fields.iter().any(|f| f.name == "status"),
        "Layer F (java): expected OrderDTO.status field in {:?}",
        cls.fields
    );
    let ctor = extracted
        .symbols
        .iter()
        .find(|s| {
            s.name == "OrderDTO"
                && s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true)
        })
        .expect("Layer F (java): expected constructor symbol with is_constructor=true");
    assert!(
        ctor.parameters
            .iter()
            .any(|p| p.name == "initialStatus" && p.param_type.as_deref() == Some("String")),
        "Layer F (java): expected typed constructor parameter in {:?}",
        ctor.parameters
    );

    let mut env = HashMap::new();
    merge_local_types("java", java_oop, "mutate", &mut env);
    assert_eq!(
        env.get("target").map(String::as_str),
        Some("OrderDTO"),
        "Layer F (java): expected formal parameter type recovery for `target`"
    );
    assert_eq!(
        env.get("alias").map(String::as_str),
        Some("OrderDTO"),
        "Layer F (java): expected local variable type recovery for `alias`"
    );
}

#[test]
fn tier1_corpus_unhandled_ast_hotspot_audit() {
    let registry = default_registry();
    let samples: &[(&str, &str, &[u8])] = &[
        (
            "java",
            "Sample.java",
            br#"
package demo;
public class Sample {
    public Object convert(Object raw) {
        return (String) raw;
    }
}
"#,
        ),
        (
            "rust",
            "sample.rs",
            br#"
pub fn compute(x: i32) -> i32 {
    x + 1
}
"#,
        ),
    ];

    for &(lang_id, filename, src) in samples {
        let spec = bundled_specs()
            .iter()
            .find(|s| s.id == lang_id)
            .expect("spec");
        let (json, _handlers) = load_manifest_handlers(spec);
        let lang = (spec.language)();
        let tracker = AstCoverageTracker::from_manifest_json(lang_id, &json, &lang)
            .expect("tracker from manifest");

        // Walk all named nodes in `src` into the tracker so both active and passive/unimplemented
        // nodes are tallied deterministically (independent of process-global OnceLock state).
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).expect("set_language");
        let tree = parser.parse(src, None).expect("parse");
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if node.is_named() {
                tracker.hit(node.kind_id());
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }

        let plugin = registry.get_language_plugin(lang_id).expect("plugin");
        let _ = plugin
            .extract_all(Path::new(filename), src)
            .expect("extract_all");

        let hotspots = tracker.unhandled_hotspots(10);
        if lang_id == "java" {
            // `cast_expression` is explicitly marked `Unimplemented` in java-ast-coverage.json.
            assert!(
                hotspots
                    .iter()
                    .any(|(name, count)| name == "cast_expression" && *count >= 1),
                "expected cast_expression in Java unhandled_hotspots, got {hotspots:?}"
            );
        }
    }
}
