//! Phase 16 — Ansible playbook, role, security, and graph integration tests.

#![cfg(feature = "iac-langs")]

use rbuilder::analysis::ansible_roles::{RoleDependencyAnalyzer, RoleDependencyGraph};
use rbuilder::extraction::extractor::Extractor;
use rbuilder::extraction::graph_builder::GraphBuilder;
use rbuilder::graph::query::execute;
use rbuilder::graph::schema::{EdgeType, NodeType};
use rbuilder::languages::plugin_trait::{LanguagePlugin, RelationType, SymbolType};
use rbuilder::languages::registry::LanguageRegistry;
use rbuilder::security::ansible::{AnsibleSecurityScanner, AnsibleSeverity};
use rbuilder_lang_ansible::parser::AnsibleParser;
use rbuilder_lang_ansible::AnsiblePlugin;
use std::path::{Path, PathBuf};
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ansible")
}

fn build_fixture_graph() -> rbuilder::graph::backend::MemoryBackend {
    let root = fixture_root();
    let graph = rbuilder::code_graph_from_repository(&root).expect("build graph");
    graph.backend().clone()
}

#[test]
fn test_ansible_path_detection() {
    assert!(AnsibleParser::is_ansible_path("playbooks/site.yml"));
    assert!(AnsibleParser::is_ansible_path("roles/nginx/tasks/main.yml"));
    assert!(AnsibleParser::is_ansible_path(
        "roles/nginx/templates/app.j2"
    ));
    assert!(AnsibleParser::is_ansible_path("group_vars/all.yml"));
    assert!(!AnsibleParser::is_ansible_path(".github/workflows/ci.yml"));
}

#[test]
fn test_jinja_variable_extraction() {
    let parser = AnsibleParser::new();
    let vars = parser.extract_jinja_vars("{{ app_name }} on {{ ansible_hostname }}");
    assert_eq!(vars.len(), 2);
}

#[test]
fn test_playbook_symbol_extraction() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("playbooks/site.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::AnsiblePlaybook));
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::AnsiblePlay));
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::AnsibleTask));
}

#[test]
fn test_playbook_relation_extraction() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("playbooks/site.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &symbols)
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::IncludesRole));
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::ExecutesTask));
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::IncludesPlaybook));
}

#[test]
fn test_role_meta_dependencies() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("roles/nginx/meta/main.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| { r.relation_type == RelationType::DependsOnRole && r.to == "common" }));
}

#[test]
fn test_template_variable_extraction() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("roles/nginx/templates/nginx.conf.j2");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::AnsibleTemplate));
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::AnsibleVariable));
}

#[test]
fn test_ansible_registry_routing() {
    let registry = LanguageRegistry::new();
    let plugin = registry
        .get_plugin_for_file(Path::new("playbooks/site.yml"))
        .unwrap();
    assert_eq!(plugin.language_id(), "ansible");
}

#[test]
fn test_graph_builder_ansible_node_types() {
    let path = fixture_root().join("playbooks/site.yml");
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
        .any(|n| n.node_type == NodeType::AnsiblePlaybook));
}

#[test]
fn test_pipeline_indexes_ansible_fixture() {
    let backend = build_fixture_graph();
    assert!(!backend
        .find_nodes_by_type(NodeType::AnsiblePlaybook)
        .unwrap()
        .is_empty());
}

#[test]
fn test_query_playbooks() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "playbooks").unwrap().is_empty());
}

#[test]
fn test_query_module_shell() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "module:shell").unwrap().is_empty());
}

#[test]
fn test_role_dependency_graph_from_disk() {
    let graph = RoleDependencyAnalyzer::new()
        .analyze_roles_dir(&fixture_root().join("roles"))
        .unwrap();
    assert_eq!(graph.get_dependencies("nginx").unwrap(), vec!["common"]);
}

#[test]
fn test_topological_sort_order() {
    let graph = RoleDependencyAnalyzer::new()
        .analyze_roles_dir(&fixture_root().join("roles"))
        .unwrap();
    let sorted = graph.topological_sort().unwrap();
    let common_pos = sorted.iter().position(|r| r == "common").unwrap();
    let nginx_pos = sorted.iter().position(|r| r == "nginx").unwrap();
    assert!(common_pos < nginx_pos);
}

#[test]
fn test_security_scan_finds_shell_injection() {
    let backend = build_fixture_graph();
    let findings = AnsibleSecurityScanner::new().scan_graph(&backend);
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
}

#[test]
fn test_security_severity_filter() {
    let backend = build_fixture_graph();
    let all = AnsibleSecurityScanner::new().scan_graph(&backend);
    let critical = AnsibleSecurityScanner::filter_by_severity(all, AnsibleSeverity::Critical);
    assert!(critical
        .iter()
        .all(|f| f.severity >= AnsibleSeverity::Critical));
}

#[test]
fn test_edges_in_full_fixture_graph() {
    let backend = build_fixture_graph();
    let edges = backend.all_edges().unwrap();
    let types: std::collections::HashSet<_> = edges.iter().map(|e| e.edge_type).collect();
    assert!(types.contains(&EdgeType::DependsOnRole));
    assert!(types.contains(&EdgeType::ExecutesTask));
}

#[test]
fn test_role_dependency_graph_from_indexed_graph() {
    let backend = build_fixture_graph();
    let graph = RoleDependencyGraph::from_graph(&backend).unwrap();
    assert!(!graph.roles.is_empty());
}

#[test]
fn test_notify_handler_relation() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("playbooks/site.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::NotifiesHandler));
}

#[test]
fn test_uses_variable_relation() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("playbooks/site.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::UsesVariable));
}

#[test]
fn test_renders_template_relation() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("roles/nginx/tasks/main.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations
        .iter()
        .any(|r| r.relation_type == RelationType::RendersTemplate));
}

#[test]
fn test_group_vars_extraction() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("group_vars/all.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::AnsibleVariable));
}

#[test]
fn test_query_type_ansibletask() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "type:ansibletask").unwrap().is_empty());
}

#[test]
fn test_query_ansibleroles() {
    let backend = build_fixture_graph();
    assert!(!execute(&backend, "ansibleroles").unwrap().is_empty());
}

#[test]
fn test_transitive_dependencies() {
    let graph = RoleDependencyAnalyzer::new()
        .analyze_roles_dir(&fixture_root().join("roles"))
        .unwrap();
    let deps = graph.transitive_dependencies("nginx").unwrap();
    assert!(deps.contains(&"common".to_string()));
}

#[test]
fn test_no_cycles_in_fixture_roles() {
    let graph = RoleDependencyAnalyzer::new()
        .analyze_roles_dir(&fixture_root().join("roles"))
        .unwrap();
    graph.validate_no_cycles().unwrap();
}

#[test]
fn test_task_module_property_on_graph() {
    let backend = build_fixture_graph();
    let tasks = backend.find_nodes_by_type(NodeType::AnsibleTask).unwrap();
    assert!(tasks.iter().any(|t| t.get_property("module").is_some()));
}

#[test]
fn test_ansible_plugin_language_id() {
    assert_eq!(AnsiblePlugin::new().unwrap().language_id(), "ansible");
}

#[test]
fn test_ansible_handler_extraction() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("roles/nginx/handlers/main.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::AnsibleHandler));
}

#[test]
fn test_import_playbook_relation() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("playbooks/site.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    assert!(relations.iter().any(|r| r.to == "monitoring.yml"));
}

#[test]
fn test_multiple_roles_in_play() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("playbooks/site.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let relations = plugin
        .extract_relations(&path, source.as_bytes(), &[])
        .unwrap();
    let role_includes = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::IncludesRole)
        .count();
    assert!(role_includes >= 2);
}

#[test]
fn test_play_hosts_metadata() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("playbooks/site.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    let play = symbols
        .iter()
        .find(|s| s.symbol_type == SymbolType::AnsiblePlay)
        .unwrap();
    assert!(play.metadata.get("hosts").is_some());
}

#[test]
fn test_ansible_not_yaml_config() {
    let registry = LanguageRegistry::new();
    assert!(registry
        .get_plugin_for_file(Path::new("roles/nginx/tasks/main.yml"))
        .is_ok());
}

#[test]
fn test_role_tasks_extraction() {
    let plugin = AnsiblePlugin::new().unwrap();
    let path = fixture_root().join("roles/nginx/tasks/main.yml");
    let source = std::fs::read_to_string(&path).unwrap();
    let symbols = plugin.extract_symbols(&path, source.as_bytes()).unwrap();
    assert!(symbols
        .iter()
        .any(|s| s.symbol_type == SymbolType::AnsibleTask));
}

#[test]
fn test_security_scan_node_metadata() {
    let backend = build_fixture_graph();
    let tasks = backend.find_nodes_by_type(NodeType::AnsibleTask).unwrap();
    assert!(tasks
        .iter()
        .any(|t| t.get_property("module") == Some(&"shell".to_string())));
}
