//! Compare `{lang}-ast-coverage.json` manifests to the live tree-sitter grammar.
//!
//! Used by `rgctl-languages` `build.rs` so `cargo check` / `cargo build` can
//! **warn** when a grammar bump introduces new named kinds (or removes old ones)
//! before unit tests are run. Set `RGCTL_AST_COVERAGE_STRICT=1` to fail the build.
//!
//! [`check_plugin_manifest_sync`] additionally verifies bi-directional agreement
//! between a plugin's [`AstHandlerTable`] and the coverage JSON (when plugins
//! expose handlers via `LanguagePlugin::declared_ast_handlers`).

mod tracker;

pub use tracker::{
    AstCoverageTracker, clear_hit_callback, install_hit_callback, named_kind_ids_in_source,
};

use rgctl_plugin_api::{AstHandlerTable, HandlerMask};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Allowed handler labels in coverage manifests.
///
/// Raw `"Skip"` is banned — use `SkipWrapper` / `SkipTypeToken` / `Unimplemented`.
pub const ALLOWED_HANDLERS: &[&str] = &[
    "Symbol",
    "Relation",
    "Property",
    "CfgStatement",
    "AstSkeleton",
    "SkipWrapper",
    "SkipTypeToken",
    "Unimplemented",
    "Literal",
];

/// True if `label` is a passive classification (`SkipWrapper`, `SkipTypeToken`, `Unimplemented`).
///
/// Legacy raw `"Skip"` is treated as passive for sync/diagnostics but is not in [`ALLOWED_HANDLERS`].
pub fn is_passive_handler_label(label: &str) -> bool {
    matches!(
        label,
        "Skip" | "SkipWrapper" | "SkipTypeToken" | "Unimplemented"
    )
}

/// True if `label` is an active handler (`Symbol`, `Relation`, `Property`, `CfgStatement`, `AstSkeleton`, `Literal`).
pub fn is_active_handler_label(label: &str) -> bool {
    matches!(
        label,
        "Symbol" | "Relation" | "Property" | "CfgStatement" | "AstSkeleton" | "Literal"
    )
}

/// True if a Tree-sitter node kind name matches the semantic node-kind denylist
/// (`*_declaration`, `*_definition`, `*_statement`, `*_expression`, `*_invocation`,
/// `*_call*`, `*_creation*`, `*_clause`, `*_pattern`, `*_directive`, `lambda*`, `closure*`).
pub fn is_semantic_node_kind(kind: &str) -> bool {
    kind.ends_with("_declaration")
        || kind.ends_with("_definition")
        || kind.ends_with("_statement")
        || kind.ends_with("_expression")
        || kind.ends_with("_invocation")
        || kind.contains("_call")
        || kind.contains("_creation")
        || kind.ends_with("_clause")
        || kind.ends_with("_pattern")
        || kind.ends_with("_directive")
        || kind.starts_with("lambda")
        || kind.starts_with("closure")
}

/// Ratified semantic AST coverage grade floor for a Tier-1 language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoverageGrade {
    /// `< 50%` semantic coverage (baseline / migration floor).
    C,
    /// `>= 50%` semantic coverage.
    B,
    /// `>= 75%` semantic coverage.
    A,
    /// `>= 98%` semantic coverage + full Layers A–F deep-analysis parity.
    APlus,
}

impl CoverageGrade {
    /// Minimum semantic coverage percentage (`0..=100`) required for this grade.
    pub const fn min_semantic_coverage_pct(self) -> u8 {
        match self {
            Self::C => 0,
            Self::B => 50,
            Self::A => 75,
            Self::APlus => 98,
        }
    }

    /// Determine achieved grade from a semantic coverage percentage (`0.0..=100.0`).
    pub fn from_percentage(pct: f64) -> Self {
        if pct >= 98.0 {
            Self::APlus
        } else if pct >= 75.0 {
            Self::A
        } else if pct >= 50.0 {
            Self::B
        } else {
            Self::C
        }
    }
}

/// Summary of active vs passive AST coverage for a manifest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SemanticCoverageStats {
    /// Total named node kinds in the manifest.
    pub total_kinds: usize,
    /// Active handlers (`Symbol`, `Relation`, `Property`, `CfgStatement`, `AstSkeleton`, `Literal`).
    pub handled_kinds: usize,
    /// Pure syntax wrappers (`SkipWrapper`) + leaf type tokens (`SkipTypeToken`).
    pub excluded_wrapper_or_type_kinds: usize,
    /// `Unimplemented` kinds included in the semantic denominator.
    pub unimplemented_kinds: usize,
    /// Semantic coverage percentage: `handled_kinds / (total_kinds - excluded_wrapper_or_type_kinds) * 100.0`.
    pub semantic_coverage_pct: f64,
}

/// Compute semantic coverage statistics from a manifest handler map.
pub fn compute_semantic_coverage(handlers: &HashMap<String, String>) -> SemanticCoverageStats {
    let total_kinds = handlers.len();
    let mut handled_kinds = 0usize;
    let mut excluded_wrapper_or_type_kinds = 0usize;
    let mut unimplemented_kinds = 0usize;

    for label in handlers.values() {
        match label.as_str() {
            "Symbol" | "Relation" | "Property" | "CfgStatement" | "AstSkeleton" | "Literal" => {
                handled_kinds += 1;
            }
            "SkipWrapper" | "SkipTypeToken" => {
                excluded_wrapper_or_type_kinds += 1;
            }
            _ => {
                unimplemented_kinds += 1;
            }
        }
    }

    let denom = total_kinds.saturating_sub(excluded_wrapper_or_type_kinds);
    let semantic_coverage_pct = if denom == 0 {
        100.0
    } else {
        (handled_kinds as f64 / denom as f64) * 100.0
    };

    SemanticCoverageStats {
        total_kinds,
        handled_kinds,
        excluded_wrapper_or_type_kinds,
        unimplemented_kinds,
        semantic_coverage_pct,
    }
}

/// Allowed `@capture` names in Tier-2 `tags.scm` packs (`handler = "queries"`).
///
/// Matches the [tree-sitter code-navigation](https://tree-sitter.github.io/tree-sitter/code-navigation)
/// vocabulary plus `@import.path`. Private `@_…` captures are always allowed.
pub const ALLOWED_TAG_CAPTURES: &[&str] = &[
    "name",
    "doc",
    "definition.function",
    "definition.method",
    "definition.class",
    "definition.interface",
    "definition.module",
    "definition.type",
    "definition.namespace",
    "definition.macro",
    "definition.field",
    "definition.enum",
    "definition.enummember",
    "definition.constant",
    "definition.import",
    "reference.call",
    "reference.class",
    "reference.implementation",
    "import.path",
];

/// Collect unique `@capture` names from a `tags.scm` source string.
///
/// Skips `;` line comments and `"…"` string literals so predicate args like
/// `#eq? @_bi "@import"` and documentation comments are not treated as captures.
pub fn extract_tag_capture_names(tags_scm: &str) -> Vec<String> {
    let mut names = Vec::new();
    let bytes = tags_scm.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b';' => {
                // Line comment through newline.
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'@' => {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() {
                    let c = bytes[end];
                    if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'-' {
                        end += 1;
                    } else {
                        break;
                    }
                }
                if end > start {
                    let mut name = String::from_utf8_lossy(&bytes[start..end]).into_owned();
                    // Trailing `.` is punctuation, not part of the capture id.
                    while name.ends_with('.') {
                        name.pop();
                    }
                    if !name.is_empty() && !names.iter().any(|n| n == &name) {
                        names.push(name);
                    }
                }
                i = end.max(start);
            }
            _ => i += 1,
        }
    }
    names
}

/// Validate Tier-2 `tags.scm` capture vocabulary (build / unit-test gate).
///
/// Unknown captures become [`CoverageIssue`]s. `@local.*` and `@_…` private
/// captures are accepted without being listed in [`ALLOWED_TAG_CAPTURES`].
pub fn check_tags_scm(language_id: &str, tags_scm: &str) -> Vec<CoverageIssue> {
    let mut issues = Vec::new();
    for name in extract_tag_capture_names(tags_scm) {
        if name.starts_with('_') || name.starts_with("local.") {
            continue;
        }
        if !ALLOWED_TAG_CAPTURES.contains(&name.as_str()) {
            issues.push(CoverageIssue {
                language: language_id.into(),
                message: format!(
                    "unknown tags.scm capture '@{name}' (allowed: {})",
                    ALLOWED_TAG_CAPTURES.join(", ")
                ),
            });
        }
    }
    issues
}

/// One bundled language to validate.
pub struct CoverageSpec {
    /// Language id (`java`, `kotlin`, …).
    pub id: &'static str,
    /// Crate directory name under `crates/` (`rgctl-lang-java`).
    pub crate_dir: &'static str,
    /// Manifest filename inside that crate.
    pub manifest_file: &'static str,
    /// Expected `grammar` field prefix (`tree-sitter-java@`).
    pub grammar_prefix: &'static str,
    /// Live grammar.
    pub language: fn() -> tree_sitter::Language,
    /// Non-decreasing ratchet floor of active (`Symbol`/`Relation`/`Property`/`CfgStatement`/`AstSkeleton`/`Literal`) kinds.
    pub min_handled_kinds: usize,
    /// Minimum semantic coverage grade floor.
    pub target_grade: CoverageGrade,
}

/// All Tier 1 (+ markdown) coverage specs shipped in-tree.
pub fn bundled_specs() -> &'static [CoverageSpec] {
    &[
        CoverageSpec {
            id: "jinja",
            crate_dir: "rgctl-lang-jinja",
            manifest_file: "jinja-ast-coverage.json",
            grammar_prefix: "tree-sitter-jinja@",
            language: || tree_sitter_jinja::language(),
            min_handled_kinds: 22,
            target_grade: CoverageGrade::C,
        },
        CoverageSpec {
            id: "ansible",
            crate_dir: "rgctl-lang-ansible",
            manifest_file: "ansible-ast-coverage.json",
            grammar_prefix: "tree-sitter-yaml@",
            language: || tree_sitter_yaml::LANGUAGE.into(),
            min_handled_kinds: 14,
            target_grade: CoverageGrade::C,
        },
        CoverageSpec {
            id: "c",
            crate_dir: "rgctl-lang-c",
            manifest_file: "c-ast-coverage.json",
            grammar_prefix: "tree-sitter-c@",
            language: || tree_sitter_c::LANGUAGE.into(),
            min_handled_kinds: 74,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "cpp",
            crate_dir: "rgctl-lang-cpp",
            manifest_file: "cpp-ast-coverage.json",
            grammar_prefix: "tree-sitter-cpp@",
            language: || tree_sitter_cpp::LANGUAGE.into(),
            min_handled_kinds: 108,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "csharp",
            crate_dir: "rgctl-lang-csharp",
            manifest_file: "csharp-ast-coverage.json",
            grammar_prefix: "tree-sitter-c-sharp@",
            language: || tree_sitter_c_sharp::LANGUAGE.into(),
            min_handled_kinds: 166,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "go",
            crate_dir: "rgctl-lang-go",
            manifest_file: "go-ast-coverage.json",
            grammar_prefix: "tree-sitter-go@",
            language: || tree_sitter_go::LANGUAGE.into(),
            min_handled_kinds: 76,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "groovy",
            crate_dir: "rgctl-lang-groovy",
            manifest_file: "groovy-ast-coverage.json",
            grammar_prefix: "tree-sitter-groovy@",
            language: || tree_sitter_groovy::LANGUAGE.into(),
            min_handled_kinds: 106,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "java",
            crate_dir: "rgctl-lang-java",
            manifest_file: "java-ast-coverage.json",
            grammar_prefix: "tree-sitter-java@",
            language: || tree_sitter_java::LANGUAGE.into(),
            min_handled_kinds: 100,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "javascript",
            crate_dir: "rgctl-lang-javascript",
            manifest_file: "javascript-ast-coverage.json",
            grammar_prefix: "tree-sitter-javascript@",
            language: || tree_sitter_javascript::LANGUAGE.into(),
            min_handled_kinds: 99,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "kotlin",
            crate_dir: "rgctl-lang-kotlin",
            manifest_file: "kotlin-ast-coverage.json",
            grammar_prefix: "tree-sitter-kotlin-ng@",
            language: || tree_sitter_kotlin_ng::LANGUAGE.into(),
            min_handled_kinds: 87,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "markdown",
            crate_dir: "rgctl-lang-markdown",
            manifest_file: "markdown-ast-coverage.json",
            grammar_prefix: "tree-sitter-md@",
            language: || tree_sitter_md::LANGUAGE.into(),
            min_handled_kinds: 7,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "php",
            crate_dir: "rgctl-lang-php",
            manifest_file: "php-ast-coverage.json",
            grammar_prefix: "tree-sitter-php@",
            language: || tree_sitter_php::LANGUAGE_PHP.into(),
            min_handled_kinds: 100,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "puppet",
            crate_dir: "rgctl-lang-puppet",
            manifest_file: "puppet-ast-coverage.json",
            grammar_prefix: "tree-sitter-puppet@",
            language: || tree_sitter_puppet::LANGUAGE.into(),
            min_handled_kinds: 48,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "erb",
            crate_dir: "rgctl-lang-erb",
            manifest_file: "erb-ast-coverage.json",
            grammar_prefix: "tree-sitter-embedded-template@",
            language: || tree_sitter_embedded_template::LANGUAGE.into(),
            min_handled_kinds: 6,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "python",
            crate_dir: "rgctl-lang-python",
            manifest_file: "python-ast-coverage.json",
            grammar_prefix: "tree-sitter-python@",
            language: || tree_sitter_python::LANGUAGE.into(),
            min_handled_kinds: 95,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "ruby",
            crate_dir: "rgctl-lang-ruby",
            manifest_file: "ruby-ast-coverage.json",
            grammar_prefix: "tree-sitter-ruby@",
            language: || tree_sitter_ruby::LANGUAGE.into(),
            min_handled_kinds: 115,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "rust",
            crate_dir: "rgctl-lang-rust",
            manifest_file: "rust-ast-coverage.json",
            grammar_prefix: "tree-sitter-rust@",
            language: || tree_sitter_rust::LANGUAGE.into(),
            min_handled_kinds: 90,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "typescript",
            crate_dir: "rgctl-lang-typescript",
            manifest_file: "typescript-ast-coverage.json",
            grammar_prefix: "tree-sitter-typescript@",
            language: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            min_handled_kinds: 106,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "swift",
            crate_dir: "rgctl-lang-swift",
            manifest_file: "swift-ast-coverage.json",
            grammar_prefix: "tree-sitter-swift@",
            language: || tree_sitter_swift::LANGUAGE.into(),
            min_handled_kinds: 88,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "scala",
            crate_dir: "rgctl-lang-scala",
            manifest_file: "scala-ast-coverage.json",
            grammar_prefix: "tree-sitter-scala@",
            language: || tree_sitter_scala::LANGUAGE.into(),
            min_handled_kinds: 94,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "dart",
            crate_dir: "rgctl-lang-dart",
            manifest_file: "dart-ast-coverage.json",
            grammar_prefix: "tree-sitter-dart@",
            language: || tree_sitter_dart::LANGUAGE.into(),
            min_handled_kinds: 139,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "elixir",
            crate_dir: "rgctl-lang-elixir",
            manifest_file: "elixir-ast-coverage.json",
            grammar_prefix: "tree-sitter-elixir@",
            language: || tree_sitter_elixir::LANGUAGE.into(),
            min_handled_kinds: 22,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "lua",
            crate_dir: "rgctl-lang-lua",
            manifest_file: "lua-ast-coverage.json",
            grammar_prefix: "tree-sitter-lua@",
            language: || tree_sitter_lua::LANGUAGE.into(),
            min_handled_kinds: 38,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "bash",
            crate_dir: "rgctl-lang-bash",
            manifest_file: "bash-ast-coverage.json",
            grammar_prefix: "tree-sitter-bash@",
            language: || tree_sitter_bash::LANGUAGE.into(),
            min_handled_kinds: 46,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "sql",
            crate_dir: "rgctl-lang-sql",
            manifest_file: "sql-ast-coverage.json",
            grammar_prefix: "tree-sitter-sql@",
            language: || tree_sitter_sql::LANGUAGE.into(),
            min_handled_kinds: 63,
            target_grade: CoverageGrade::APlus,
        },
        CoverageSpec {
            id: "hcl",
            crate_dir: "rgctl-lang-hcl",
            manifest_file: "hcl-ast-coverage.json",
            grammar_prefix: "tree-sitter-hcl@",
            language: || tree_sitter_hcl::LANGUAGE.into(),
            min_handled_kinds: 14,
            target_grade: CoverageGrade::APlus,
        },
    ]
}

/// Named kinds from a live grammar.
pub fn grammar_named_kinds(lang: &tree_sitter::Language) -> HashSet<String> {
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

/// Drift / validity issues for one manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageIssue {
    /// Language id.
    pub language: String,
    /// Human-readable problem.
    pub message: String,
}

/// Validate one JSON manifest against a live grammar.
pub fn check_manifest(
    language_id: &str,
    json: &str,
    grammar_prefix: &str,
    lang: &tree_sitter::Language,
) -> Vec<CoverageIssue> {
    let mut issues = Vec::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        issues.push(CoverageIssue {
            language: language_id.into(),
            message: "manifest JSON failed to parse".into(),
        });
        return issues;
    };

    let grammar = v["grammar"].as_str().unwrap_or("");
    if !grammar.starts_with(grammar_prefix) {
        issues.push(CoverageIssue {
            language: language_id.into(),
            message: format!(
                "grammar pin `{grammar}` does not start with `{grammar_prefix}` — bump or fix the manifest"
            ),
        });
    }

    let Some(handlers_obj) = v["handlers"].as_object() else {
        issues.push(CoverageIssue {
            language: language_id.into(),
            message: "manifest missing `handlers` object".into(),
        });
        return issues;
    };

    let handlers: HashMap<String, String> = handlers_obj
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("Unimplemented").to_string()))
        .collect();

    let skip_justifications: HashMap<String, String> = v["skip_justifications"]
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    for (kind, handler) in &handlers {
        if !ALLOWED_HANDLERS.contains(&handler.as_str()) {
            issues.push(CoverageIssue {
                language: language_id.into(),
                message: format!("kind `{kind}` has invalid handler `{handler}`"),
            });
        }
        if matches!(handler.as_str(), "SkipWrapper" | "SkipTypeToken")
            && is_semantic_node_kind(kind)
            && !skip_justifications
                .get(kind)
                .is_some_and(|reason| !reason.trim().is_empty())
        {
            issues.push(CoverageIssue {
                language: language_id.into(),
                message: format!(
                    "semantic AST kind `{kind}` cannot be marked `{handler}` without an audited entry in `skip_justifications` (use an active handler, `Unimplemented`, or document why `{kind}` is a pure syntax wrapper)"
                ),
            });
        }
    }

    let kinds = grammar_named_kinds(lang);
    for kind in &kinds {
        if !handlers.contains_key(kind) {
            issues.push(CoverageIssue {
                language: language_id.into(),
                message: format!(
                    "grammar kind `{kind}` missing from ast-coverage.json — classify as an active handler (`Symbol`, `Relation`, `Property`, `CfgStatement`, `AstSkeleton`, `Literal`) or explicit passive category (`SkipWrapper`, `SkipTypeToken`, `Unimplemented`)"
                ),
            });
        }
    }
    for key in handlers.keys() {
        if !kinds.contains(key) {
            issues.push(CoverageIssue {
                language: language_id.into(),
                message: format!(
                    "manifest key `{key}` not in grammar named kinds — remove stale entry after grammar bump"
                ),
            });
        }
    }

    if let Some(spec) = bundled_specs().iter().find(|s| s.id == language_id) {
        let stats = compute_semantic_coverage(&handlers);
        if stats.handled_kinds < spec.min_handled_kinds {
            issues.push(CoverageIssue {
                language: language_id.into(),
                message: format!(
                    "coverage ratchet regression: active handled kinds ({}) dropped below min_handled_kinds floor ({})",
                    stats.handled_kinds, spec.min_handled_kinds
                ),
            });
        }
        let achieved_grade = CoverageGrade::from_percentage(stats.semantic_coverage_pct);
        if achieved_grade < spec.target_grade {
            issues.push(CoverageIssue {
                language: language_id.into(),
                message: format!(
                    "semantic coverage grade regression: {:.1}% ({achieved_grade:?}) is below target_grade {:?} (>= {}%)",
                    stats.semantic_coverage_pct,
                    spec.target_grade,
                    spec.target_grade.min_semantic_coverage_pct()
                ),
            });
        }
    }

    issues
}

/// Bi-directional sync: plugin dispatch table ↔ `{lang}-ast-coverage.json` handlers.
///
/// - Every kind the plugin handles with a semantic mask (`Symbol` / `Relation` /
///   `CfgStatement` / `AstSkeleton`) MUST be marked non-`Skip` in the manifest.
/// - Every non-`Skip` / non-`Literal` manifest kind MUST appear in the plugin table
///   with a compatible semantic mask.
///
/// Call this from language crate tests once `declared_ast_handlers()` is wired.
pub fn check_plugin_manifest_sync(
    language_id: &str,
    manifest_json: &str,
    table: &AstHandlerTable,
    lang: &tree_sitter::Language,
) -> Vec<CoverageIssue> {
    check_plugin_and_analysis_manifest_sync(language_id, manifest_json, table, &[], &[], lang)
}

/// Three-way sync across `{lang}-ast-coverage.json`, the plugin's [`AstHandlerTable`],
/// `rgctl-analysis` CFG statement handlers (`cfg_kinds`), and def-use handlers (`def_use_kinds`).
pub fn check_plugin_and_analysis_manifest_sync(
    language_id: &str,
    manifest_json: &str,
    table: &AstHandlerTable,
    cfg_kinds: &[&str],
    def_use_kinds: &[&str],
    lang: &tree_sitter::Language,
) -> Vec<CoverageIssue> {
    let mut issues = Vec::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(manifest_json) else {
        issues.push(CoverageIssue {
            language: language_id.into(),
            message: "manifest JSON failed to parse".into(),
        });
        return issues;
    };
    let Some(handlers_obj) = v["handlers"].as_object() else {
        issues.push(CoverageIssue {
            language: language_id.into(),
            message: "manifest missing `handlers` object".into(),
        });
        return issues;
    };

    let handlers: HashMap<String, String> = handlers_obj
        .iter()
        .map(|(k, val)| {
            (
                k.clone(),
                val.as_str().unwrap_or("Unimplemented").to_string(),
            )
        })
        .collect();

    // Plugin → manifest: semantic handlers must not be passive in JSON.
    for (kind_id, kind_name, mask) in table.iter() {
        if mask.is_passive() {
            continue;
        }
        match handlers.get(kind_name).map(|s| s.as_str()) {
            None => issues.push(CoverageIssue {
                language: language_id.into(),
                message: format!(
                    "plugin handles `{kind_name}` (kind_id={kind_id}, {}) but it is missing from ast-coverage.json",
                    mask.to_coverage_label()
                ),
            }),
            Some(label) if is_passive_handler_label(label) => issues.push(CoverageIssue {
                language: language_id.into(),
                message: format!(
                    "plugin handles `{kind_name}` as {} but ast-coverage.json marks it `{label}` (passive)",
                    mask.to_coverage_label()
                ),
            }),
            Some(label) => {
                let Some(expected) = HandlerMask::from_coverage_label(label) else {
                    continue;
                };
                if !mask.intersects(expected) && !expected.is_passive() {
                    issues.push(CoverageIssue {
                        language: language_id.into(),
                        message: format!(
                            "plugin `{kind_name}` mask {} does not include manifest handler `{label}`",
                            mask.to_coverage_label()
                        ),
                    });
                }
            }
        }
    }

    // Manifest → plugin / cfg_builder / def_use: active JSON kinds must be backed by a real handler.
    for (kind_name, label) in &handlers {
        let Some(expected) = HandlerMask::from_coverage_label(label) else {
            continue;
        };
        if expected.is_passive() {
            continue;
        }
        let in_analysis = (label == "CfgStatement" && cfg_kinds.contains(&kind_name.as_str()))
            || (label == "AstSkeleton"
                && (cfg_kinds.contains(&kind_name.as_str())
                    || def_use_kinds.contains(&kind_name.as_str())));
        match table.get_by_name(kind_name) {
            None if !in_analysis => {
                let kind_id = lang.id_for_node_kind(kind_name, true);
                issues.push(CoverageIssue {
                    language: language_id.into(),
                    message: format!(
                        "ast-coverage.json marks `{kind_name}` as `{label}` (kind_id={kind_id}) but plugin AstHandlerTable (and cfg_builder/def_use) has no entry"
                    ),
                });
            }
            None => {}
            Some((_id, mask))
                if (mask.is_passive() || !mask.intersects(expected)) && !in_analysis =>
            {
                issues.push(CoverageIssue {
                    language: language_id.into(),
                    message: format!(
                        "ast-coverage.json marks `{kind_name}` as `{label}` but plugin table has {}",
                        mask.to_coverage_label()
                    ),
                });
            }
            Some(_) => {}
        }
    }

    issues
}

/// Validate every bundled spec and Tier-2 `queries/tags.scm` under `crates_dir` (parent of `rgctl-lang-*`).
pub fn check_crates_dir(crates_dir: &Path) -> Vec<CoverageIssue> {
    let mut all = Vec::new();
    for spec in bundled_specs() {
        let path = crates_dir.join(spec.crate_dir).join(spec.manifest_file);
        match std::fs::read_to_string(&path) {
            Ok(json) => {
                let lang = (spec.language)();
                all.extend(check_manifest(spec.id, &json, spec.grammar_prefix, &lang));
            }
            Err(e) => all.push(CoverageIssue {
                language: spec.id.into(),
                message: format!("cannot read {}: {e}", path.display()),
            }),
        }
    }
    for (lang_id, scm_path) in discover_tags_scm_paths(crates_dir) {
        match std::fs::read_to_string(&scm_path) {
            Ok(scm) => all.extend(check_tags_scm(&lang_id, &scm)),
            Err(e) => all.push(CoverageIssue {
                language: lang_id,
                message: format!("cannot read {}: {e}", scm_path.display()),
            }),
        }
    }
    all
}

/// Discover `rgctl-lang-*/queries/tags.scm` files under `crates_dir`.
pub fn discover_tags_scm_paths(crates_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(crates_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(lang_id) = dir_name.strip_prefix("rgctl-lang-") else {
            continue;
        };
        let scm_path = path.join("queries").join("tags.scm");
        if scm_path.is_file() {
            out.push((lang_id.to_string(), scm_path));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Paths that should trigger a rebuild of consumers (`cargo:rerun-if-changed=`).
pub fn rerun_if_changed_paths(crates_dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = bundled_specs()
        .iter()
        .map(|s| crates_dir.join(s.crate_dir).join(s.manifest_file))
        .collect();
    paths.extend(
        discover_tags_scm_paths(crates_dir)
            .into_iter()
            .map(|(_, p)| p),
    );
    paths
}

/// Format issues as `cargo:warning=` lines (and optional hard failure).
///
/// # Errors
/// Returns `Err` when `strict` is true and `issues` is non-empty.
pub fn emit_cargo_warnings(issues: &[CoverageIssue], strict: bool) -> Result<(), String> {
    if issues.is_empty() {
        return Ok(());
    }
    for issue in issues {
        println!(
            "cargo:warning=AST coverage drift [{}]: {}",
            issue.language, issue.message
        );
    }
    println!(
        "cargo:warning=AST coverage: {} issue(s) — update `*-ast-coverage.json` after grammar bumps (RGCTL_AST_COVERAGE_STRICT=1 fails the build)",
        issues.len()
    );
    if strict {
        return Err(format!(
            "RGCTL_AST_COVERAGE_STRICT=1: {} AST coverage issue(s)",
            issues.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_manifest_in_workspace_matches_grammar() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let path = crates.join("rgctl-lang-java/java-ast-coverage.json");
        let json = std::fs::read_to_string(&path).expect("java manifest");
        let lang = tree_sitter_java::LANGUAGE.into();
        let issues = check_manifest("java", &json, "tree-sitter-java@", &lang);
        assert!(issues.is_empty(), "java coverage drift: {issues:?}");
    }

    #[test]
    fn all_bundled_specs_pass_manifest_ratchet_and_grade() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let issues = check_crates_dir(&crates);
        assert!(issues.is_empty(), "workspace coverage issues: {issues:?}");
    }

    #[test]
    fn semantic_denylist_rejects_unjustified_skip_wrapper() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let path = crates.join("rgctl-lang-java/java-ast-coverage.json");
        let json = std::fs::read_to_string(&path).expect("java manifest");
        let mut v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        v["handlers"]["try_with_resources_statement"] =
            serde_json::Value::String("SkipWrapper".into());
        let lang = tree_sitter_java::LANGUAGE.into();
        let issues = check_manifest("java", &v.to_string(), "tree-sitter-java@", &lang);
        assert!(
            issues
                .iter()
                .any(|i| i.message.contains("try_with_resources_statement")
                    && i.message.contains("skip_justifications")),
            "expected semantic denylist error, got {issues:?}"
        );
    }

    #[test]
    fn coverage_ratchet_detects_regression_below_min_handled_kinds() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let path = crates.join("rgctl-lang-java/java-ast-coverage.json");
        let json = std::fs::read_to_string(&path).expect("java manifest");
        let mut v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        v["handlers"]["try_with_resources_statement"] =
            serde_json::Value::String("Unimplemented".into());
        let lang = tree_sitter_java::LANGUAGE.into();
        let issues = check_manifest("java", &v.to_string(), "tree-sitter-java@", &lang);
        assert!(
            issues
                .iter()
                .any(|i| i.message.contains("coverage ratchet regression")),
            "expected ratchet regression error, got {issues:?}"
        );
    }

    #[test]
    fn plugin_manifest_sync_detects_skip_drift() {
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let kind = "function_item";
        let kind_id = lang.id_for_node_kind(kind, true);
        assert_ne!(kind_id, 0, "rust grammar must know function_item");

        let mut table = AstHandlerTable::new();
        table.insert(kind_id, kind, HandlerMask::SYMBOL);

        let manifest = r#"{
            "grammar": "tree-sitter-rust@0",
            "handlers": { "function_item": "Unimplemented" }
        }"#;
        let issues = check_plugin_manifest_sync("rust", manifest, &table, &lang);
        assert!(
            issues.iter().any(|i| i.message.contains("Unimplemented")),
            "expected Unimplemented drift, got {issues:?}"
        );
    }

    #[test]
    fn plugin_manifest_sync_detects_missing_plugin_entry() {
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let table = AstHandlerTable::new();
        let manifest = r#"{
            "grammar": "tree-sitter-rust@0",
            "handlers": { "function_item": "Symbol" }
        }"#;
        let issues = check_plugin_manifest_sync("rust", manifest, &table, &lang);
        assert!(
            issues.iter().any(|i| i.message.contains("no entry")),
            "expected missing plugin entry, got {issues:?}"
        );
    }

    #[test]
    fn bundled_specs_cover_expected_languages() {
        let ids: HashSet<_> = bundled_specs().iter().map(|s| s.id).collect();
        // Tier-1 custom plugins + markdown/erb/puppet/groovy (24 bundled specs).
        // OpenSpec "23 custom" excludes markdown; all of these must be present.
        const EXPECTED: &[&str] = &[
            "c",
            "cpp",
            "csharp",
            "go",
            "groovy",
            "java",
            "javascript",
            "kotlin",
            "markdown",
            "php",
            "puppet",
            "erb",
            "python",
            "ruby",
            "rust",
            "typescript",
            "swift",
            "scala",
            "dart",
            "elixir",
            "lua",
            "bash",
            "sql",
            "hcl",
        ];
        assert_eq!(
            EXPECTED.len(),
            24,
            "update EXPECTED if Tier-1 / bundled set changes"
        );
        for need in EXPECTED {
            assert!(ids.contains(need), "bundled_specs missing {need}");
        }
        assert!(
            ids.len() >= EXPECTED.len(),
            "bundled_specs has {} ids, expected at least {}",
            ids.len(),
            EXPECTED.len()
        );
    }

    #[test]
    fn tags_scm_rejects_unknown_captures() {
        let issues = check_tags_scm("zig", "(fn) @definition.widget\n(x) @name");
        assert!(
            issues
                .iter()
                .any(|i| i.message.contains("definition.widget")),
            "{issues:?}"
        );
    }

    #[test]
    fn tags_scm_accepts_standard_vocabulary() {
        let scm = r#"
(function_definition name: (identifier) @name) @definition.function
(call function: (identifier) @name) @reference.call
(import path: (string) @import.path)
"#;
        assert!(check_tags_scm("demo", scm).is_empty());
    }

    #[test]
    fn tags_scm_ignores_comments_and_string_literals() {
        let scm = r#"
; docs mention @import.path. and @definition.widget in comments only
(builtin_function
  (builtin_identifier) @_bi
  (#eq? @_bi "@import")
  (arguments (string (string_content) @import.path)))
"#;
        let names = extract_tag_capture_names(scm);
        assert!(names.contains(&"_bi".to_string()), "{names:?}");
        assert!(names.contains(&"import.path".to_string()), "{names:?}");
        assert!(
            !names
                .iter()
                .any(|n| n == "import" || n == "definition.widget")
        );
        assert!(
            check_tags_scm("demo", scm).is_empty(),
            "{:?}",
            check_tags_scm("demo", scm)
        );
    }
}

#[cfg(test)]
mod automation_coverage_tests {
    #[test]
    fn automation_manifests_detect_missing_named_kind() {
        for id in ["ansible", "jinja"] {
            let spec = super::bundled_specs()
                .iter()
                .find(|spec| spec.id == id)
                .unwrap();
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join(spec.crate_dir)
                .join(spec.manifest_file);
            let raw = std::fs::read_to_string(path).unwrap();
            let mut manifest: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let removed = if id == "ansible" { "stream" } else { "source" };
            manifest["handlers"]
                .as_object_mut()
                .unwrap()
                .remove(removed);
            let issues = super::check_manifest(
                id,
                &manifest.to_string(),
                spec.grammar_prefix,
                &(spec.language)(),
            );
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.message.contains(removed)
                        && issue.message.contains("missing")),
                "{issues:?}"
            );
        }
    }
}
