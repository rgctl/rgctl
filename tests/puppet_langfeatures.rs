//! Puppet extraction GQL gates on `rgctl-tests/ecommerce-puppet`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-puppet")
}

fn bin() -> PathBuf {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_rgctl") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/rgctl")
}

fn ensure_discovered() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let repo = repo();
        assert!(repo.is_dir(), "missing fixture {}", repo.display());
        let _ = std::fs::remove_dir_all(repo.join(".rgctl"));
        let out = Command::new(bin())
            .args(["discover", ".", "-l", "puppet"])
            .current_dir(&repo)
            .output()
            .expect("run discover");
        assert!(
            out.status.success(),
            "discover failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
}

fn gql(repo: &Path, query: &str) -> Value {
    let out = Command::new(bin())
        .args(["-f", "json", "gql", query])
        .current_dir(repo)
        .output()
        .expect("gql");
    assert!(
        out.status.success(),
        "gql failed: {}\n{}",
        query,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json")
}

fn node_count(repo: &Path, label: &str) -> usize {
    let q = format!("MATCH (n:{label}) RETURN n LIMIT 10000");
    gql(repo, &q)
        .get("count")
        .and_then(|c| c.as_u64())
        .unwrap_or(0) as usize
}

fn edge_count(repo: &Path, rel: &str) -> usize {
    let q = format!("MATCH (a)-[:{rel}]->(b) RETURN a,b LIMIT 10000");
    gql(repo, &q)
        .get("count")
        .and_then(|c| c.as_u64())
        .unwrap_or(0) as usize
}

#[test]
fn puppet_ecommerce_classes_nonzero() {
    ensure_discovered();
    let n = node_count(&repo(), "PuppetClass");
    assert!(n > 0, "expected PuppetClass nodes, got {n}");
}

#[test]
fn puppet_ecommerce_includes_nonzero() {
    ensure_discovered();
    let n = edge_count(&repo(), "INCLUDESCLASS");
    assert!(n > 0, "expected IncludesClass edges, got {n}");
}

#[test]
fn puppet_ecommerce_module_present() {
    ensure_discovered();
    let n = node_count(&repo(), "PuppetModule");
    assert!(n > 0, "expected PuppetModule from metadata.json, got {n}");
}

#[test]
fn puppet_config_and_erb_routes_remain_separate_from_ansible_jinja() {
    let registry = std::sync::Arc::new(rgctl_languages::default_registry());
    for (path, language) in [
        ("modules/web/manifests/init.pp", "puppet"),
        ("modules/web/templates/page.erb", "erb"),
        ("roles/web/templates/legacy.erb", "erb"),
    ] {
        assert_eq!(
            registry
                .get_plugin_for_file(Path::new(path))
                .expect("existing route")
                .language_id(),
            language
        );
    }
    let extraction = rgctl_extraction::Extractor::new(registry)
        .extract_file_with_source(
            Path::new("modules/web/data/inventory.yaml"),
            b"all:\n  hosts:\n    puppet_node:\n      profile::web::port: 8080\n".to_vec(),
        )
        .expect("generic YAML");
    assert_eq!(extraction.language.as_deref(), Some("yaml"));
    assert!(extraction.symbols.is_empty());
    assert!(!extraction.config_keys.is_empty());
    assert!(extraction.source_references.is_empty());
}
