//! Phase 11.3 — polyglot repository end-to-end integration tests

use rbuilder::discovery::DiscoveryConfig;
use rbuilder::extraction::extractor::Extractor;
use rbuilder::extraction::graph_builder::GraphBuilder;
use rbuilder::languages::registry::LanguageRegistry;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;

fn write_polyglot_repo(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join(".github/workflows")).unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();

    fs::write(root.join("src/main.rs"), "fn main() { helper(); }\nfn helper() {}\n").unwrap();
    fs::write(root.join("src/util.py"), "def helper():\n    return 1\n").unwrap();
    fs::write(root.join("src/app.ts"), "export function helper(): number { return 1; }\n").unwrap();
    fs::write(root.join("src/app.js"), "function helper() { return 1; }\n").unwrap();
    fs::write(root.join("src/main.go"), "package main\nfunc helper() int { return 1 }\n").unwrap();
    fs::write(
        root.join("schema.sql"),
        "CREATE TABLE users (id INTEGER PRIMARY KEY);\n",
    )
    .unwrap();
    fs::write(
        root.join("Dockerfile"),
        "FROM alpine:3.19\nCOPY schema.sql .\nRUN echo ok\n",
    )
    .unwrap();
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n",
    )
    .unwrap();
    fs::write(
        root.join("scripts/deploy.sh"),
        "deploy() { echo deploy; }\nsource ./common.sh\n",
    )
    .unwrap();
    fs::write(root.join("config.yaml"), "app:\n  name: demo\n").unwrap();
}

#[cfg(feature = "bundle-extended")]
#[test]
fn test_polyglot_repo_extraction() {
    let tmp = TempDir::new().unwrap();
    write_polyglot_repo(tmp.path());

    let registry = Arc::new(LanguageRegistry::new());
    let extractor = Extractor::new(Arc::clone(&registry));
    let start = Instant::now();
    let extractions = extractor
        .extract_repository(tmp.path(), &DiscoveryConfig::default())
        .unwrap();
    let elapsed = start.elapsed();

    assert!(
        extractions.len() >= 8,
        "expected multiple file extractions, got {}",
        extractions.len()
    );

    let mut languages = HashSet::new();
    for extraction in &extractions {
        if let Ok(plugin) = registry.get_plugin_for_file(&extraction.path) {
            languages.insert(plugin.language_id().to_string());
        }
    }
    for expected in [
        "rust",
        "python",
        "typescript",
        "javascript",
        "go",
        "sql",
        "dockerfile",
        "github_actions",
        "bash",
    ] {
        assert!(
            languages.contains(expected),
            "missing language {expected} in {:?}",
            languages
        );
    }

    let mut builder = GraphBuilder::new();
    extractor
        .populate_graph(&extractions, &mut builder)
        .unwrap();
    assert!(builder.node_count() > 0);
    assert!(
        elapsed.as_secs() < 120,
        "polyglot extraction took {:?}",
        elapsed
    );
}

#[cfg(feature = "bundle-extra")]
#[test]
fn test_extra_bundle_language_count() {
    let registry = LanguageRegistry::new();
    assert_eq!(registry.stats().language_plugins, 41);
    assert!(registry.supported_languages().len() >= 35);
}
