//! Go CFG analysis against the ecommerce-go fixture (no embedded dashboard required).

use rbuilder::analysis::{build_cfg_for_function, cfg_language_id_from_path, ProgramDependenceGraph};
use std::path::Path;

const GO_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-go";

fn go_repo() -> std::path::PathBuf {
    std::env::var("RBUILDER_GO_REPO")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(GO_REPO))
}

#[test]
fn go_cfg_language_profile_maps_extension() {
    let path = Path::new("internal/handler/auth.go");
    assert_eq!(cfg_language_id_from_path(path), Some("go"));
}

#[test]
fn go_cfg_builds_auth_login_from_fixture() {
    let repo = go_repo();
    let file = repo.join("internal/handler/auth.go");
    if !file.is_file() {
        eprintln!("skip: go fixture not found at {}", file.display());
        return;
    }

    let source = std::fs::read_to_string(&file).unwrap();
    let cfg = build_cfg_for_function("go", &source, "Login").expect("Login CFG");
    assert!(cfg.blocks.len() >= 4, "expected branching CFG for Login");

    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).expect("Login PDG");
    assert!(!pdg.nodes.is_empty());
}

#[test]
fn go_cfg_builds_package_level_function() {
    let repo = go_repo();
    let file = repo.join("internal/handler/auth.go");
    if !file.is_file() {
        eprintln!("skip: go fixture not found at {}", file.display());
        return;
    }

    let source = std::fs::read_to_string(&file).unwrap();
    let cfg = build_cfg_for_function("go", &source, "NewAuthHandler").expect("NewAuthHandler CFG");
    assert!(!cfg.blocks.is_empty());
}
