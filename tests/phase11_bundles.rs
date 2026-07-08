//! Tier 1 language plugin integration tests

use rbuilder::languages::registry::LanguageRegistry;
use std::path::Path;

#[test]
fn test_all_tier1_plugins_registered() {
    let registry = LanguageRegistry::new();
    assert_eq!(registry.stats().language_plugins, 9);
    for id in [
        "rust",
        "python",
        "javascript",
        "typescript",
        "go",
        "java",
        "csharp",
        "c",
        "cpp",
    ] {
        assert!(registry.has_plugin(id), "missing plugin {id}");
    }
}

#[test]
fn test_extension_routing() {
    let registry = LanguageRegistry::new();
    for (file, id) in [
        ("main.rs", "rust"),
        ("app.py", "python"),
        ("index.js", "javascript"),
        ("index.ts", "typescript"),
        ("main.go", "go"),
        ("App.java", "java"),
        ("Program.cs", "csharp"),
        ("main.c", "c"),
        ("types.h", "c"),
        ("main.cpp", "cpp"),
        ("types.hpp", "cpp"),
    ] {
        let plugin = registry
            .get_plugin_for_file(Path::new(file))
            .unwrap_or_else(|_| panic!("no plugin for {file}"));
        assert_eq!(plugin.language_id(), id);
    }
}
