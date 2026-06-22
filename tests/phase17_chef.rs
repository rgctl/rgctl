//! Phase 17 — Chef cookbook, recipe, security, and graph integration tests.

#![cfg(feature = "iac-langs")]

use rbuilder::analysis::chef_cookbooks::{CookbookDependencyAnalyzer, CookbookDependencyGraph};
use rbuilder::extraction::extractor::Extractor;
use rbuilder::extraction::graph_builder::GraphBuilder;
use rbuilder::graph::query::execute;
use rbuilder::graph::schema::{EdgeType, NodeType};
use rbuilder::languages::plugin_trait::{LanguagePlugin, RelationType, SymbolType};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::security::chef::{ChefSecurityScanner, ChefSeverity};
use rbuilder_lang_chef::parser::ChefParser;
use rbuilder_lang_chef::ChefPlugin;
use std::path::{Path, PathBuf};
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chef")
}

fn build_fixture_graph() -> rbuilder::graph::backend::MemoryBackend {
    let root = fixture_root();
    let graph = rbuilder::code_graph_from_repository(&root).expect("build graph");
    graph.backend().clone()
}

#[test]
fn test_chef_path_detection() {
    assert!(ChefParser::is_chef_path(
        "cookbooks/nginx/recipes/default.rb"
    ));
    assert!(ChefParser::is_chef_path("cookbooks/nginx/metadata.rb"));
    assert!(!ChefParser::is_chef_path("lib/helper.rb"));
}

#[test]
fn test_recipe_resource_extraction() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/recipes/default.rb");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::ChefRecipe));
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::ChefResource));
}

#[test]
fn test_recipe_relations() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/recipes/default.rb");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::DeclaresResource));
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::IncludesRecipe));
}

#[test]
fn test_metadata_dependencies() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/metadata.rb");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::DependsOnCookbook));
    assert_eq!(relations.len(), 2);
}

#[test]
fn test_chef_registry_routing() {
    let registry = LanguageRegistry::new();
    let plugin = registry
        .get_plugin_for_file(Path::new("cookbooks/nginx/recipes/default.rb"))
        .unwrap();
    assert_eq!(plugin.language_id(), "chef");
}

#[test]
fn test_pipeline_indexes_chef_fixture() {
    let backend = build_fixture_graph();
    assert!(!backend
        .find_nodes_by_type(NodeType::ChefCookbook)
        .unwrap()
        .is_empty());
}

#[test]
fn test_query_cookbooks() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "cookbooks").unwrap().is_empty());
}

#[test]
fn test_query_resource_execute() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "resource:execute").unwrap().is_empty());
}

#[test]
fn test_cookbook_dependency_from_disk() {
    let graph = CookbookDependencyAnalyzer::new()
        .analyze_cookbooks_dir(&fixture_root().join("cookbooks"))
        .unwrap();
    assert_eq!(
        graph.get_dependencies("nginx").unwrap(),
        vec!["common", "apt"]
    );
}

#[test]
fn test_security_scan_command_injection() {
    let backend = build_fixture_graph();
    let findings = ChefSecurityScanner::new().scan_graph(&backend);
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
}

#[test]
fn test_security_scan_insecure_permissions() {
    let backend = build_fixture_graph();
    let findings = ChefSecurityScanner::new().scan_graph(&backend);
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-732")));
}

#[test]
fn test_graph_builder_chef_nodes() {
    let path = fixture_root().join("cookbooks/nginx/recipes/default.rb");
    let registry = LanguageRegistry::new().into();
    let extractor = Extractor::new(registry);
    let extraction = extractor.extract_file(&path).unwrap();
    let mut builder = GraphBuilder::new();
    extractor
        .populate_graph(&[extraction], &mut builder)
        .unwrap();
    assert!(builder
        .nodes()
        .iter()
        .any(|n| n.node_type == NodeType::ChefRecipe));
}

#[test]
fn test_edges_in_fixture_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    let types: std::collections::HashSet<_> = edges.iter().map(|e| e.edge_type).collect();
    assert!(types.contains(&EdgeType::DependsOnCookbook));
    assert!(types.contains(&EdgeType::DeclaresResource));
}

#[test]
fn test_attributes_extraction() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/attributes/default.rb");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::ChefAttribute));
}

#[test]
fn test_template_extraction() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/templates/nginx.conf.erb");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::ChefTemplate));
}

#[test]
fn test_topological_sort_cookbooks() {
    let graph = CookbookDependencyAnalyzer::new()
        .analyze_cookbooks_dir(&fixture_root().join("cookbooks"))
        .unwrap();
    let sorted = graph.topological_sort().unwrap();
    let common_pos = sorted.iter().position(|c| c == "common").unwrap();
    let nginx_pos = sorted.iter().position(|c| c == "nginx").unwrap();
    assert!(common_pos < nginx_pos);
}

#[test]
fn test_cookbook_dependency_from_graph() {
    let backend = build_fixture_graph();
    let graph = CookbookDependencyGraph::from_graph(&backend).unwrap();
    assert!(!graph.cookbooks.is_empty());
}

#[test]
fn test_query_chefrecipes() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "chefrecipes").unwrap().is_empty());
}

#[test]
fn test_resource_type_on_graph() {
    let backend = build_fixture_graph();
    let resources = backend.find_nodes_by_type(NodeType::ChefResource).unwrap();
    assert!(resources
        .iter()
        .any(|r| r.get_property("resource_type") == Some(&"execute".to_string())));
}

#[test]
fn test_metadata_cookbook_symbol() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/metadata.rb");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols.iter().any(|s| s.name == "cookbook::nginx"));
}

#[test]
fn test_multiple_resources_in_recipe() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/recipes/default.rb");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    let resources = symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::ChefResource)
        .count();
    assert!(resources >= 4);
}

#[test]
fn test_uses_template_edge_in_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    assert!(edges.iter().any(|e| e.edge_type == EdgeType::UsesTemplate));
}

#[test]
fn test_includes_recipe_in_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    assert!(edges
        .iter()
        .any(|e| e.edge_type == EdgeType::IncludesRecipe));
}

#[test]
fn test_defines_attribute_in_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    assert!(edges
        .iter()
        .any(|e| e.edge_type == EdgeType::DefinesAttribute));
}

#[test]
fn test_no_cycles_in_cookbooks() {
    let graph = CookbookDependencyAnalyzer::new()
        .analyze_cookbooks_dir(&fixture_root().join("cookbooks"))
        .unwrap();
    graph.validate_no_cycles().unwrap();
}

#[test]
fn test_security_severity_filter() {
    let backend = build_fixture_graph();
    let all = ChefSecurityScanner::new().scan_graph(&backend);
    let critical = ChefSecurityScanner::filter_by_severity(all, ChefSeverity::Critical);
    assert!(critical
        .iter()
        .all(|f| f.severity >= ChefSeverity::Critical));
}

#[test]
fn test_chef_plugin_language_id() {
    assert_eq!(ChefPlugin::new().unwrap().language_id(), "chef");
}

#[test]
fn test_chef_not_ruby_plugin() {
    let registry = LanguageRegistry::new();
    let plugin = registry
        .get_plugin_for_file(Path::new("cookbooks/nginx/recipes/default.rb"))
        .unwrap();
    assert_ne!(plugin.language_id(), "ruby");
}

#[test]
fn test_query_type_chefresource() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "type:chefresource").unwrap().is_empty());
}

#[test]
fn test_notifies_resource_relation() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/recipes/default.rb");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::NotifiesResource));
}

#[test]
fn test_common_cookbook_indexed() {
    let backend = build_fixture_graph();
    let cookbooks = backend.find_nodes_by_type(NodeType::ChefCookbook).unwrap();
    assert!(cookbooks.iter().any(|c| c.name.contains("common")));
}

#[test]
fn test_package_resource_metadata() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/recipes/default.rb");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols.iter().any(|s| {
        s.symbol_type == SymbolType::ChefResource
            && s.metadata.get("resource_type").and_then(|v| v.as_str()) == Some("package")
    }));
}

#[test]
fn test_service_resource_in_recipe() {
    let plugin = ChefPlugin::new().unwrap();
    let path = fixture_root().join("cookbooks/nginx/recipes/default.rb");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| { s.metadata.get("resource_type").and_then(|v| v.as_str()) == Some("service") }));
}
