//! Inheritance edge correctness (EXTENDS / IMPLEMENTS) on ecommerce-java.

mod rgctl_harness;

use rgctl_harness::{assert_ok, cli_json, rgctl, run_in_repo};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn rgctl_bin() -> std::path::PathBuf {
    std::env::var("RGCTL_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| rgctl())
}

fn discover(repo: &Path) {
    static DISCOVERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    DISCOVERED.get_or_init(|| {
        let out = Command::new(rgctl_bin())
            .args(["discover", ".", "--languages", "java"])
            .current_dir(repo)
            .output()
            .expect("discover");
        assert!(
            out.status.success(),
            "discover failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
}

/// Resolve a class by exact name, then list outgoing edges of the given type.
fn outgoing_edge_targets(repo: &Path, class_name: &str, edge: &str) -> Vec<String> {
    let find = cli_json(
        repo,
        &[
            "find", class_name, "--exact", "--type", "class", "--limit", "5",
        ],
    );
    let entities = find
        .get("entities")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !entities.is_empty(),
        "find --exact --type class {class_name}: no entities in {find}"
    );
    let line = entities[0]
        .get("line")
        .and_then(|l| l.as_u64())
        .expect("class entity has line");
    let line_s = line.to_string();

    let out = run_in_repo(
        repo,
        &[
            "-f",
            "json",
            "relations",
            class_name,
            "--edge",
            edge,
            "--direction",
            "out",
            "--line",
            &line_s,
            "--limit",
            "50",
        ],
    );
    assert_ok(&out, &format!("relations {class_name} --edge {edge}"));
    let v: Value = serde_json::from_slice(&out.stdout).expect("relations json");
    v.get("edges")
        .and_then(|e| e.as_array())
        .into_iter()
        .flatten()
        .filter_map(|row| {
            row.get("target")
                .and_then(|t| t.get("name"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}

#[test]
fn java_class_extends_external_filter() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    if !repo.is_dir() {
        return;
    }
    discover(&repo);
    let targets = outgoing_edge_targets(&repo, "JwtAuthenticationFilter", "extends");
    assert!(
        targets.iter().any(|t| t == "OncePerRequestFilter"),
        "expected JwtAuthenticationFilter EXTENDS OncePerRequestFilter, got {targets:?}"
    );
}

#[test]
fn java_class_extends_lang_exception_without_import() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    if !repo.is_dir() {
        return;
    }
    discover(&repo);
    let targets = outgoing_edge_targets(&repo, "ResourceNotFoundException", "extends");
    assert!(
        targets.iter().any(|t| t == "RuntimeException"),
        "expected ResourceNotFoundException EXTENDS RuntimeException, got {targets:?}"
    );
}

#[test]
fn java_class_implements_external_interface() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    if !repo.is_dir() {
        return;
    }
    discover(&repo);
    let targets = outgoing_edge_targets(&repo, "CustomUserDetailsService", "implements");
    assert!(
        targets.iter().any(|t| t.contains("UserDetailsService")),
        "expected CustomUserDetailsService IMPLEMENTS UserDetailsService, got {targets:?}"
    );
}
