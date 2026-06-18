//! Phase 11.3 — polyglot repository end-to-end integration tests

#[path = "common/polyglot.rs"]
mod polyglot;

use polyglot::write_polyglot_repo;
use rbuilder::discovery::DiscoveryConfig;
use rbuilder::extraction::extractor::Extractor;
use rbuilder::extraction::graph_builder::GraphBuilder;
use rbuilder::languages::registry::LanguageRegistry;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;

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
    assert_eq!(registry.stats().language_plugins, 43);
    assert!(registry.supported_languages().len() >= 35);
}
