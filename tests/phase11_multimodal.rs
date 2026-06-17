//! Phase 11.2 — multi-modal plugin integration tests

use rbuilder::languages::multimodal::{
    bash::BashPlugin, dockerfile::DockerfilePlugin, gitlab_ci::GitlabCiPlugin, sql::SqlPlugin,
};
use rbuilder::languages::plugin_trait::{LanguagePlugin, RelationType, SymbolType};
use rbuilder::languages::registry::LanguageRegistry;
use std::path::Path;

#[test]
fn test_sql_ddl_extraction() {
    let plugin = SqlPlugin::new().unwrap();
    let source = br#"
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) NOT NULL
);
CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    user_id INTEGER REFERENCES users(id)
);
"#;
    let symbols = plugin
        .extract_symbols(Path::new("schema.sql"), source)
        .unwrap();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "users");
    assert_eq!(symbols[0].symbol_type, SymbolType::Table);
}

#[test]
fn test_sql_view_and_index_in_multimodal() {
    let plugin = SqlPlugin::new().unwrap();
    let source = br#"
CREATE TABLE users (id INTEGER PRIMARY KEY);
CREATE VIEW active_users AS SELECT id FROM users;
CREATE INDEX users_id_idx ON users (id);
"#;
    let symbols = plugin
        .extract_symbols(Path::new("schema.sql"), source)
        .unwrap();
    assert!(symbols.iter().any(|s| s.name == "active_users"));
}

#[test]
fn test_dockerfile_routing_and_extraction() {
    let registry = LanguageRegistry::new();
    let plugin = registry
        .get_plugin_for_file(Path::new("Dockerfile"))
        .unwrap();
    assert_eq!(plugin.language_id(), "dockerfile");

    let docker = DockerfilePlugin::new().unwrap();
    let source = b"FROM rust:1.75\nCOPY Cargo.toml .\nRUN cargo build";
    let symbols = docker
        .extract_symbols(Path::new("Dockerfile"), source)
        .unwrap();
    assert!(
        symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::Dependency && s.name == "rust:1.75")
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::BuildStep)
    );
}

#[test]
fn test_github_actions_routing() {
    let registry = LanguageRegistry::new();
    let path = Path::new(".github/workflows/ci.yml");
    let plugin = registry.get_plugin_for_file(path).unwrap();
    assert_eq!(plugin.language_id(), "github_actions");

    let source = br#"jobs:
  test:
    runs-on: ubuntu-latest
  build:
    needs: test
    runs-on: ubuntu-latest
"#;
    let symbols = plugin.extract_symbols(path, source).unwrap();
    assert_eq!(
        symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Job)
            .count(),
        2
    );
    let relations = plugin.extract_relations(path, source, &symbols).unwrap();
    assert!(relations.iter().any(|r| r.relation_type == RelationType::DependsOn));
}

#[test]
fn test_gitlab_ci_routing() {
    let registry = LanguageRegistry::new();
    let path = Path::new(".gitlab-ci.yml");
    let plugin = registry.get_plugin_for_file(path).unwrap();
    assert_eq!(plugin.language_id(), "gitlab_ci");

    let gitlab = GitlabCiPlugin::new().unwrap();
    let source = br#"stages: [test, build]
test_job:
  stage: test
  script: cargo test
build_job:
  stage: build
  needs: [test_job]
  script: cargo build
"#;
    let symbols = gitlab.extract_symbols(path, source).unwrap();
    assert!(
        symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::Job && s.name == "test_job")
    );
}

#[cfg(feature = "lang-bash")]
#[test]
fn test_bash_shell_extraction() {
    let registry = LanguageRegistry::new();
    let plugin = registry
        .get_plugin_for_file(Path::new("scripts/deploy.sh"))
        .unwrap();
    assert_eq!(plugin.language_id(), "bash");

    let bash = BashPlugin::new().unwrap();
    let source = b"deploy() {\n  echo 'Deploying...'\n}\nsource ./lib/common.sh";
    let symbols = bash
        .extract_symbols(Path::new("deploy.sh"), source)
        .unwrap();
    assert_eq!(symbols[0].name, "deploy");
    let relations = bash
        .extract_relations(Path::new("deploy.sh"), source, &symbols)
        .unwrap();
    assert!(relations.iter().any(|r| r.to == "./lib/common.sh"));
}
