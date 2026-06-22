//! Phase 18 — Puppet module, manifest, security, and graph integration tests.

#![cfg(feature = "iac-langs")]

use rbuilder::analysis::puppet_modules::{ModuleDependencyAnalyzer, ModuleDependencyGraph};
use rbuilder::graph::query::execute;
use rbuilder::graph::schema::{EdgeType, NodeType};
use rbuilder::languages::plugin_trait::{LanguagePlugin, RelationType, SymbolType};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::security::puppet::{PuppetSecurityScanner, PuppetSeverity};
use rbuilder_lang_puppet::parser::PuppetParser;
use rbuilder_lang_puppet::PuppetPlugin;
use std::path::{Path, PathBuf};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/puppet")
}

fn build_fixture_graph() -> rbuilder::graph::backend::MemoryBackend {
    let root = fixture_root();
    let graph = rbuilder::code_graph_from_repository(&root).expect("build graph");
    graph.backend().clone()
}

#[test]
fn test_puppet_path_detection() {
    assert!(PuppetParser::is_puppet_path(
        "modules/nginx/manifests/init.pp"
    ));
    assert!(PuppetParser::is_puppet_path("modules/nginx/metadata.json"));
    assert!(!PuppetParser::is_puppet_path("lib/helper.rb"));
}

#[test]
fn test_manifest_class_extraction() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/init.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::PuppetClass));
    assert!(symbols.iter().any(|s| s.name == "class::nginx"));
}

#[test]
fn test_manifest_resource_extraction() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/init.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::PuppetResource));
}

#[test]
fn test_manifest_relations() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/init.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::DeclaresResource));
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::IncludesClass));
}

#[test]
fn test_metadata_dependencies() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/metadata.json");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::DependsOnModule));
}

#[test]
fn test_puppet_registry_routing() {
    let registry = LanguageRegistry::new();
    let plugin = registry
        .get_plugin_for_file(Path::new("modules/nginx/manifests/init.pp"))
        .unwrap();
    assert_eq!(plugin.language_id(), "puppet");
}

#[test]
fn test_pipeline_indexes_puppet_fixture() {
    let backend = build_fixture_graph();
    assert!(!backend
        .find_nodes_by_type(NodeType::PuppetModule)
        .unwrap()
        .is_empty());
}

#[test]
fn test_defined_type_extraction() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/server.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::PuppetDefinedType));
}

#[test]
fn test_includes_class_relation() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    assert!(edges.iter().any(|e| e.edge_type == EdgeType::IncludesClass));
}

#[test]
fn test_inherits_class_relation() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/init.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::InheritsClass));
}

#[test]
fn test_declares_resource_in_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    assert!(edges
        .iter()
        .any(|e| e.edge_type == EdgeType::DeclaresResource));
}

#[test]
fn test_notifies_resource_in_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    assert!(edges
        .iter()
        .any(|e| e.edge_type == EdgeType::NotifiesResource));
}

#[test]
fn test_requires_resource_in_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    assert!(edges
        .iter()
        .any(|e| e.edge_type == EdgeType::RequiresResource));
}

#[test]
fn test_uses_fact_in_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    assert!(edges.iter().any(|e| e.edge_type == EdgeType::UsesFact));
}

#[test]
fn test_module_dependency_graph() {
    let graph = ModuleDependencyGraph::from_graph(&build_fixture_graph()).unwrap();
    assert!(graph.modules.contains_key("nginx"));
    assert!(graph.modules.contains_key("common"));
}

#[test]
fn test_topological_sort() {
    let graph = ModuleDependencyGraph::from_graph(&build_fixture_graph()).unwrap();
    let sorted = graph.topological_sort().unwrap();
    let nginx_pos = sorted.iter().position(|m| m == "nginx").unwrap();
    let common_pos = sorted.iter().position(|m| m == "common").unwrap();
    assert!(common_pos < nginx_pos);
}

#[test]
fn test_no_cycles_in_modules() {
    let graph = ModuleDependencyAnalyzer::new()
        .analyze_modules_dir(&fixture_root().join("modules"))
        .unwrap();
    graph.validate_no_cycles().unwrap();
}

#[test]
fn test_security_scan_finds_issues() {
    let backend = build_fixture_graph();
    let findings = PuppetSecurityScanner::new().scan_graph(&backend);
    assert!(!findings.is_empty());
}

#[test]
fn test_security_severity_filter() {
    let backend = build_fixture_graph();
    let all = PuppetSecurityScanner::new().scan_graph(&backend);
    let critical = PuppetSecurityScanner::filter_by_severity(all, PuppetSeverity::Critical);
    assert!(critical
        .iter()
        .all(|f| f.severity >= PuppetSeverity::Critical));
}

#[test]
fn test_puppet_plugin_language_id() {
    assert_eq!(PuppetPlugin::new().unwrap().language_id(), "puppet");
}

#[test]
fn test_query_type_puppetresource() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "type:puppetresource").unwrap().is_empty());
}

#[test]
fn test_query_type_puppetclass() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "type:puppetclass").unwrap().is_empty());
}

#[test]
fn test_query_puppetmodules() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "puppetmodules").unwrap().is_empty());
}

#[test]
fn test_common_module_indexed() {
    let backend = build_fixture_graph();
    let modules = backend.find_nodes_by_type(NodeType::PuppetModule).unwrap();
    assert!(modules.iter().any(|m| m.name.contains("common")));
}

#[test]
fn test_package_resource_metadata() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/init.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols.iter().any(|s| {
        s.symbol_type == SymbolType::PuppetResource
            && s.metadata.get("resource_type").and_then(|v| v.as_str()) == Some("package")
    }));
}

#[test]
fn test_module_node_from_metadata() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/metadata.json");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols.iter().any(|s| s.name == "module::nginx"));
}

#[test]
fn test_multiple_resources_in_manifest() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/init.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    let resources = symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::PuppetResource)
        .count();
    assert!(resources >= 4);
}

#[test]
fn test_defined_type_in_graph() {
    let backend = build_fixture_graph();
    assert!(!backend
        .find_nodes_by_type(NodeType::PuppetDefinedType)
        .unwrap()
        .is_empty());
}

#[test]
fn test_puppet_variable_symbols() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/init.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::PuppetVariable));
}

#[test]
fn test_module_depends_on_common() {
    let graph = ModuleDependencyGraph::from_graph(&build_fixture_graph()).unwrap();
    let nginx = graph.modules.get("nginx").unwrap();
    assert!(nginx.dependencies.contains(&"common".to_string()));
}

#[test]
fn test_server_class_indexed() {
    let backend = build_fixture_graph();
    let classes = backend.find_nodes_by_type(NodeType::PuppetClass).unwrap();
    assert!(classes.iter().any(|c| c.name.contains("nginx::server")));
}

#[test]
fn test_security_hardcoded_secret() {
    let backend = build_fixture_graph();
    let findings = PuppetSecurityScanner::new().scan_graph(&backend);
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-798")));
}

#[test]
fn test_security_file_permissions() {
    let backend = build_fixture_graph();
    let findings = PuppetSecurityScanner::new().scan_graph(&backend);
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-732")));
}

#[test]
fn test_security_command_injection() {
    let backend = build_fixture_graph();
    let findings = PuppetSecurityScanner::new().scan_graph(&backend);
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
}

#[test]
fn test_depends_on_module_in_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    assert!(edges
        .iter()
        .any(|e| e.edge_type == EdgeType::DependsOnModule));
}

#[test]
fn test_puppet_fact_nodes() {
    let backend = build_fixture_graph();
    assert!(!backend
        .find_nodes_by_type(NodeType::PuppetFact)
        .unwrap()
        .is_empty());
}

#[test]
fn test_notifies_resource_relation() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/init.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::NotifiesResource));
}

#[test]
fn test_requires_resource_relation() {
    let plugin = PuppetPlugin::new().unwrap();
    let path = fixture_root().join("modules/nginx/manifests/init.pp");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::RequiresResource));
}

#[test]
fn test_analyze_modules_dir() {
    let graph = ModuleDependencyAnalyzer::new()
        .analyze_modules_dir(&fixture_root().join("modules"))
        .unwrap();
    assert!(graph.modules.len() >= 2);
}
