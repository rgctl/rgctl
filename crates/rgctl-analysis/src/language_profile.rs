//! Canonical language profiles for CFG/PDG analysis and path-based language detection.

use rgctl_error::{Error, Result};
use std::path::Path;
use tree_sitter::{Language, Tree};

/// Analysis capabilities for a supported language.
#[derive(Debug, Clone, Copy)]
pub struct LanguageAnalysisProfile {
    /// Canonical language id (e.g. `go`, `java`).
    pub id: &'static str,
    /// Alternate ids accepted on the CLI (`py`, `rs`, …).
    pub aliases: &'static [&'static str],
    /// Source file extensions without dot.
    pub extensions: &'static [&'static str],
    /// Tree-sitter node kinds treated as functions for CFG lookup.
    pub function_kinds: &'static [&'static str],
    /// Whether `discover --cfg` runs CFG/PDG/taint for this language.
    pub cfg_enabled: bool,
    /// Whether taint pattern detection is available.
    pub taint_enabled: bool,
}

const PROFILES: &[LanguageAnalysisProfile] = &[
    LanguageAnalysisProfile {
        id: "rust",
        aliases: &["rs"],
        extensions: &["rs"],
        function_kinds: &["function_item"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "python",
        aliases: &["py"],
        extensions: &["py"],
        function_kinds: &["function_definition"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "java",
        aliases: &[],
        extensions: &["java"],
        function_kinds: &[
            "method_declaration",
            "constructor_declaration",
            "compact_constructor_declaration",
            "static_initializer",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "csharp",
        aliases: &["cs", "c#"],
        extensions: &["cs"],
        function_kinds: &[
            "method_declaration",
            "local_function_statement",
            "constructor_declaration",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "c",
        aliases: &[],
        extensions: &["c", "h"],
        function_kinds: &["function_definition"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "cpp",
        aliases: &["c++", "cxx", "cc"],
        extensions: &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        function_kinds: &["function_definition"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "go",
        aliases: &["golang"],
        extensions: &["go"],
        function_kinds: &["function_declaration", "method_declaration"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "javascript",
        aliases: &["js"],
        extensions: &["js", "jsx", "mjs", "cjs"],
        function_kinds: &[
            "function_declaration",
            "function_expression",
            "generator_function_declaration",
            "generator_function",
            "method_definition",
            "arrow_function",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "typescript",
        aliases: &["ts"],
        extensions: &["ts", "tsx"],
        function_kinds: &[
            "function_declaration",
            "function_expression",
            "generator_function_declaration",
            "generator_function",
            "method_definition",
            "arrow_function",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "php",
        aliases: &[],
        extensions: &["php"],
        function_kinds: &[
            "function_definition",
            "method_declaration",
            "arrow_function",
            "anonymous_function",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "ruby",
        aliases: &["rb"],
        extensions: &["rb"],
        function_kinds: &["method", "singleton_method"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "puppet",
        aliases: &["pp"],
        extensions: &["pp"],
        function_kinds: &[
            "function_declaration",
            "class_definition",
            "defined_resource_type",
            "node_definition",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "kotlin",
        aliases: &["kt"],
        extensions: &["kt", "kts"],
        function_kinds: &[
            "function_declaration",
            "primary_constructor",
            "secondary_constructor",
            "anonymous_function",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "groovy",
        aliases: &[],
        extensions: &["groovy", "gradle"],
        function_kinds: &[
            "method_declaration",
            "function_definition",
            "constructor_declaration",
            "compact_constructor_declaration",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "swift",
        aliases: &[],
        extensions: &["swift"],
        function_kinds: &[
            "function_declaration",
            "init_declaration",
            "deinit_declaration",
            "subscript_declaration",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "scala",
        aliases: &[],
        extensions: &["scala", "sc"],
        function_kinds: &["function_definition", "function_declaration"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "dart",
        aliases: &[],
        extensions: &["dart"],
        function_kinds: &[
            "function_declaration",
            "method_declaration",
            "function_expression",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "elixir",
        aliases: &["ex"],
        extensions: &["ex", "exs"],
        // Elixir defs are `call` nodes; CFG looks up by callable name from the call.
        function_kinds: &["call"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "lua",
        aliases: &[],
        extensions: &["lua"],
        function_kinds: &["function_declaration", "function_definition"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "bash",
        aliases: &["sh", "shell"],
        extensions: &["sh", "bash"],
        function_kinds: &["function_definition"],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "sql",
        aliases: &[],
        extensions: &["sql"],
        function_kinds: &["create_function"],
        cfg_enabled: true,
        taint_enabled: false,
    },
    LanguageAnalysisProfile {
        id: "hcl",
        aliases: &["terraform", "tf"],
        extensions: &["hcl", "tf", "tfvars"],
        function_kinds: &["block"],
        cfg_enabled: true,
        taint_enabled: false,
    },
];

/// Return the profile for a canonical id or alias.
pub fn profile_for_language(language: &str) -> Option<&'static LanguageAnalysisProfile> {
    let key = language.to_lowercase();
    PROFILES
        .iter()
        .find(|p| p.id == key.as_str() || p.aliases.contains(&key.as_str()))
}

/// Map a file path to a canonical language id when the extension is known.
pub fn language_id_from_path(path: &Path) -> Option<&'static str> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    profile_for_extension(ext).map(|p| p.id)
}

/// Map a file path to a canonical language id when CFG analysis is enabled for it.
pub fn cfg_language_id_from_path(path: &Path) -> Option<&'static str> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    profile_for_extension(ext)
        .filter(|p| p.cfg_enabled)
        .map(|p| p.id)
}

/// Canonical ids for languages with CFG analysis enabled.
pub fn cfg_language_ids() -> Vec<&'static str> {
    PROFILES
        .iter()
        .filter(|p| p.cfg_enabled)
        .map(|p| p.id)
        .collect()
}

/// Human-readable list for CLI messages.
pub fn cfg_language_list() -> String {
    cfg_language_ids().join(", ")
}

/// Normalize a CLI or path-derived language id to its canonical profile id.
pub fn canonical_language_id(language: &str) -> Option<&'static str> {
    profile_for_language(language).map(|p| p.id)
}

/// Whether taint pattern detection is enabled for this language id or alias.
pub fn taint_enabled_for(language: &str) -> bool {
    profile_for_language(language)
        .map(|p| p.taint_enabled)
        .unwrap_or(false)
}

fn profile_for_extension(ext: &str) -> Option<&'static LanguageAnalysisProfile> {
    let ext = ext.to_lowercase();
    PROFILES
        .iter()
        .find(|p| p.extensions.contains(&ext.as_str()))
}

fn grammar_for(profile: &LanguageAnalysisProfile) -> Result<Language> {
    match profile.id {
        "rust" => Ok(tree_sitter_rust::LANGUAGE.into()),
        "python" => Ok(tree_sitter_python::LANGUAGE.into()),
        "java" => Ok(tree_sitter_java::LANGUAGE.into()),
        "go" => Ok(tree_sitter_go::LANGUAGE.into()),
        "csharp" => Ok(tree_sitter_c_sharp::LANGUAGE.into()),
        "c" => Ok(tree_sitter_c::LANGUAGE.into()),
        "cpp" => Ok(tree_sitter_cpp::LANGUAGE.into()),
        "javascript" => Ok(tree_sitter_javascript::LANGUAGE.into()),
        "typescript" => Ok(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "php" => Ok(tree_sitter_php::LANGUAGE_PHP.into()),
        "ruby" => Ok(tree_sitter_ruby::LANGUAGE.into()),
        "puppet" => Ok(tree_sitter_puppet::LANGUAGE.into()),
        "kotlin" => Ok(tree_sitter_kotlin_ng::LANGUAGE.into()),
        "groovy" => Ok(tree_sitter_groovy::LANGUAGE.into()),
        "swift" => Ok(tree_sitter_swift::LANGUAGE.into()),
        "scala" => Ok(tree_sitter_scala::LANGUAGE.into()),
        "dart" => Ok(tree_sitter_dart::LANGUAGE.into()),
        "elixir" => Ok(tree_sitter_elixir::LANGUAGE.into()),
        "lua" => Ok(tree_sitter_lua::LANGUAGE.into()),
        "bash" => Ok(tree_sitter_bash::LANGUAGE.into()),
        "sql" => Ok(tree_sitter_sequel::LANGUAGE.into()),
        "hcl" => Ok(tree_sitter_hcl::LANGUAGE.into()),
        other => Err(Error::UnsupportedLanguage(other.to_string())),
    }
}

/// Parse source with the tree-sitter grammar for `language`.
///
/// Uses [`rgctl_plugin_api::parse_with_pool`] so CFG / slice / taint share the
/// same Rayon worker parser pool as language extract plugins.
pub fn parse_source(language: &str, source: &[u8]) -> Result<Tree> {
    let profile = profile_for_language(language)
        .filter(|p| p.cfg_enabled)
        .ok_or_else(|| Error::UnsupportedLanguage(language.to_string()))?;

    let grammar = grammar_for(profile)?;
    rgctl_plugin_api::parse_with_pool(profile.id, grammar, source).map_err(|message| {
        Error::ParseError {
            file: "source".into(),
            line: 0,
            message,
        }
    })
}

/// Function node kinds for CFG name lookup.
pub fn function_kinds_for(language: &str) -> Result<&'static [&'static str]> {
    let profile = profile_for_language(language)
        .filter(|p| p.cfg_enabled)
        .ok_or_else(|| Error::UnsupportedLanguage(language.to_string()))?;
    Ok(profile.function_kinds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn go_extension_maps_to_go() {
        assert_eq!(
            language_id_from_path(Path::new("handler/auth.go")),
            Some("go")
        );
        assert_eq!(
            cfg_language_id_from_path(Path::new("handler/auth.go")),
            Some("go")
        );
    }

    #[test]
    fn normalize_go_aliases() {
        assert_eq!(profile_for_language("golang").map(|p| p.id), Some("go"));
        assert_eq!(profile_for_language("go").map(|p| p.id), Some("go"));
    }

    #[test]
    fn cfg_language_list_includes_go() {
        let list = cfg_language_list();
        assert!(list.contains("go"));
        assert!(list.contains("java"));
    }

    #[test]
    fn puppet_extension_maps_to_puppet() {
        assert_eq!(
            cfg_language_id_from_path(Path::new("modules/nginx/manifests/init.pp")),
            Some("puppet")
        );
    }

    #[test]
    fn javascript_cfg_enabled() {
        assert_eq!(
            cfg_language_id_from_path(Path::new("app.js")),
            Some("javascript")
        );
        assert_eq!(
            language_id_from_path(Path::new("app.js")),
            Some("javascript")
        );
    }

    #[test]
    fn typescript_cfg_enabled() {
        assert_eq!(
            cfg_language_id_from_path(Path::new("app.ts")),
            Some("typescript")
        );
    }

    #[test]
    fn canonical_language_id_normalizes_aliases() {
        assert_eq!(canonical_language_id("py"), Some("python"));
        assert_eq!(canonical_language_id("rs"), Some("rust"));
        assert_eq!(canonical_language_id("golang"), Some("go"));
    }

    #[test]
    fn new_tier1_extensions_map_to_cfg_profiles() {
        for (path, id) in [
            ("App.swift", "swift"),
            ("App.scala", "scala"),
            ("job.sc", "scala"),
            ("main.dart", "dart"),
            ("lib.ex", "elixir"),
            ("test.exs", "elixir"),
            ("util.lua", "lua"),
            ("setup.sh", "bash"),
            ("run.bash", "bash"),
            ("schema.sql", "sql"),
            ("main.tf", "hcl"),
            ("vars.tfvars", "hcl"),
        ] {
            assert_eq!(
                cfg_language_id_from_path(Path::new(path)),
                Some(id),
                "{path}"
            );
        }
        assert!(grammar_for(profile_for_language("swift").unwrap()).is_ok());
        assert!(grammar_for(profile_for_language("lua").unwrap()).is_ok());
        assert!(grammar_for(profile_for_language("hcl").unwrap()).is_ok());
    }

    #[test]
    fn cfg_enabled_languages_have_taint() {
        for id in [
            "rust",
            "python",
            "java",
            "go",
            "csharp",
            "c",
            "cpp",
            "javascript",
            "typescript",
        ] {
            assert!(taint_enabled_for(id), "{id} should have taint");
            assert!(profile_for_language(id).unwrap().cfg_enabled);
        }
    }

    #[test]
    fn javascript_has_taint_and_cfg() {
        assert!(profile_for_language("javascript").unwrap().cfg_enabled);
        assert!(taint_enabled_for("javascript"));
        assert!(taint_enabled_for("js"));
    }
}
