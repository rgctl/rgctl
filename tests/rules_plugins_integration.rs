//! Phase 3 integration tests: rule engine and plugins

use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::{Node, NodeType};
use rbuilder::graph::CodeGraph;
use rbuilder::languages::plugin_loader::{PluginLoader, PluginRegistry, PLUGIN_REGISTRY_FILE};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::rules::{RuleEngine, Ruleset};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_rule_engine_integration() {
    let mut graph = CodeGraph::new();
    let backend = graph.backend_mut();

    let mut auth = Node::new(NodeType::Function, "authenticate_user".to_string());
    auth.properties
        .insert("cyclomatic".to_string(), "20".to_string());
    let mut legacy = Node::new(NodeType::Function, "legacy_handler".to_string());
    legacy
        .properties
        .insert("cyclomatic".to_string(), "3".to_string());
    backend.insert_node(auth).unwrap();
    backend.insert_node(legacy).unwrap();

    let rules_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/security-rules.json");
    let ruleset = Ruleset::from_file(&rules_path).unwrap();
    let report = RuleEngine::apply_ruleset(backend, &ruleset, false).unwrap();

    assert!(report.rule_matches["critical_security_function"] >= 1);
    assert!(report.rule_matches["high_complexity"] >= 1);
    assert!(report.rule_matches["deprecated_api"] >= 1);
}

#[test]
fn test_java_plugin_extraction() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("App.java"),
        "public class App { public void run() {} }",
    )
    .unwrap();

    let graph = rbuilder::code_graph_from_repository(temp.path()).unwrap();
    let classes = graph.find_by_type(NodeType::Class).unwrap();
    assert!(classes.iter().any(|n| n.name == "App"));
}

#[test]
fn test_tier1_plugin_registry() {
    let registry = LanguageRegistry::new();
    for id in ["rust", "python", "javascript", "typescript", "go", "java", "csharp", "c", "cpp"] {
        assert!(registry.has_plugin(id), "missing plugin {id}");
    }
}

#[test]
fn test_plugin_registry_install() {
    let temp = TempDir::new().unwrap();
    let plugin_path = temp.path().join("libcustom.so");
    fs::write(&plugin_path, b"fake").unwrap();

    let metadata = PluginLoader::install(temp.path(), &plugin_path).unwrap();
    assert_eq!(metadata.language_id, "libcustom");

    let registry = PluginRegistry::load(temp.path()).unwrap();
    assert_eq!(registry.plugins.len(), 1);
    assert!(temp
        .path()
        .join(".rbuilder")
        .join(PLUGIN_REGISTRY_FILE)
        .exists());
}
