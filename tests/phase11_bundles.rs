//! Phase 11.1.3 — feature bundle integration tests

use rbuilder::languages::registry::LanguageRegistry;
use std::path::Path;

macro_rules! bundle_lang_test {
    ($name:ident, $feature:literal, $ext:literal, $lang_id:literal) => {
        #[cfg(feature = $feature)]
        #[test]
        fn $name() {
            let registry = LanguageRegistry::new();
            let plugin = registry
                .get_plugin_for_file(Path::new(concat!("sample.", $ext)))
                .unwrap();
            assert_eq!(plugin.language_id(), $lang_id);
        }
    };
}

bundle_lang_test!(minimal_includes_rust, "bundle-minimal", "rs", "rust");
bundle_lang_test!(minimal_includes_typescript, "bundle-minimal", "ts", "typescript");
bundle_lang_test!(extended_includes_java, "bundle-extended", "java", "java");
bundle_lang_test!(extended_includes_cpp, "bundle-extended", "cpp", "cpp");
bundle_lang_test!(full_includes_swift, "bundle-full", "swift", "swift");
bundle_lang_test!(full_includes_nim, "bundle-full", "nim", "nim");
bundle_lang_test!(extra_includes_scala, "bundle-extra", "scala", "scala");

#[cfg(all(feature = "bundle-minimal", not(feature = "bundle-extended")))]
#[test]
fn test_minimal_excludes_tier2_language() {
    let registry = LanguageRegistry::new();
    assert!(registry.get_plugin_for_file(Path::new("app.swift")).is_err());
}

#[cfg(all(feature = "bundle-extended", not(feature = "bundle-full")))]
#[test]
fn test_extended_excludes_tier2_language() {
    let registry = LanguageRegistry::new();
    assert!(registry.get_plugin_for_file(Path::new("app.swift")).is_err());
}

bundle_lang_test!(extended_includes_sql, "bundle-extended", "sql", "sql");
bundle_lang_test!(extended_includes_bash, "bundle-extended", "sh", "bash");

#[cfg(all(feature = "bundle-full", not(feature = "bundle-extra")))]
#[test]
fn test_full_includes_all_bundle_languages() {
    let registry = LanguageRegistry::new();
    assert_eq!(registry.stats().language_plugins, 28);
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_extra_includes_all_bundle_languages() {
    let registry = LanguageRegistry::new();
    assert_eq!(registry.stats().language_plugins, 41);
}

#[cfg(feature = "bundle-extended")]
#[test]
fn test_dockerfile_path_routing() {
    let registry = LanguageRegistry::new();
    let plugin = registry
        .get_plugin_for_file(Path::new("Dockerfile"))
        .unwrap();
    assert_eq!(plugin.language_id(), "dockerfile");
}

#[cfg(feature = "bundle-extended")]
#[test]
fn test_github_actions_path_routing() {
    let registry = LanguageRegistry::new();
    let plugin = registry
        .get_plugin_for_file(Path::new(".github/workflows/ci.yml"))
        .unwrap();
    assert_eq!(plugin.language_id(), "github_actions");
}
