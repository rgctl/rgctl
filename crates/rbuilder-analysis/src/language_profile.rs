//! Canonical language profiles for CFG/PDG analysis and path-based language detection.

use rbuilder_error::{Error, Result};
use std::path::Path;
use tree_sitter::{Language, Parser, Tree};

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
        function_kinds: &["method_declaration", "constructor_declaration"],
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
            "method_definition",
            "arrow_function",
        ],
        cfg_enabled: true,
        taint_enabled: true,
    },
    LanguageAnalysisProfile {
        id: "ruby",
        aliases: &["rb"],
        extensions: &["rb"],
        function_kinds: &[],
        cfg_enabled: false,
        taint_enabled: false,
    },
];

/// Return the profile for a canonical id or alias.
pub fn profile_for_language(language: &str) -> Option<&'static LanguageAnalysisProfile> {
    let key = language.to_lowercase();
    PROFILES.iter().find(|p| {
        p.id == key.as_str() || p.aliases.iter().any(|alias| *alias == key.as_str())
    })
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
        .find(|p| p.extensions.iter().any(|e| *e == ext.as_str()))
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
        other => Err(Error::UnsupportedLanguage(other.to_string())),
    }
}

/// Parse source with the tree-sitter grammar for `language`.
pub fn parse_source(language: &str, source: &[u8]) -> Result<Tree> {
    let profile = profile_for_language(language)
        .filter(|p| p.cfg_enabled)
        .ok_or_else(|| Error::UnsupportedLanguage(language.to_string()))?;

    let mut parser = Parser::new();
    parser
        .set_language(&grammar_for(profile)?)
        .map_err(|e| Error::PluginError(format!("{} grammar: {e}", profile.id)))?;

    parser.parse(source, None).ok_or_else(|| Error::ParseError {
        file: "source".into(),
        line: 0,
        message: "Failed to parse source".to_string(),
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
    fn javascript_cfg_enabled() {
        assert_eq!(
            cfg_language_id_from_path(Path::new("app.js")),
            Some("javascript")
        );
        assert_eq!(language_id_from_path(Path::new("app.js")), Some("javascript"));
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
