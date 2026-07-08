//! JavaScript CFG analysis against the ecommerce-javascript fixture.

use rbuilder::analysis::{build_cfg_for_function, cfg_language_id_from_path, ProgramDependenceGraph};
use std::path::Path;

const JS_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-javascript";

fn js_repo() -> std::path::PathBuf {
    std::env::var("RBUILDER_JAVASCRIPT_REPO")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(JS_REPO))
}

#[test]
fn javascript_cfg_language_profile_maps_extension() {
    assert_eq!(
        cfg_language_id_from_path(Path::new("src/services/orderService.js")),
        Some("javascript")
    );
}

#[test]
fn javascript_cfg_builds_checkout_from_fixture() {
    let repo = js_repo();
    let file = repo.join("src/services/orderService.js");
    if !file.is_file() {
        eprintln!("skip: javascript fixture not found at {}", file.display());
        return;
    }

    let source = std::fs::read_to_string(&file).unwrap();
    let cfg = build_cfg_for_function("javascript", &source, "checkout").expect("checkout CFG");
    assert!(!cfg.blocks.is_empty());

    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).expect("checkout PDG");
    assert!(!pdg.nodes.is_empty());
}

#[test]
fn javascript_cfg_builds_require_auth_from_fixture() {
    let repo = js_repo();
    let file = repo.join("src/middleware/auth.js");
    if !file.is_file() {
        eprintln!("skip: javascript fixture not found at {}", file.display());
        return;
    }

    let source = std::fs::read_to_string(&file).unwrap();
    let cfg = build_cfg_for_function("javascript", &source, "requireAuth").expect("requireAuth CFG");
    assert!(cfg.blocks.len() >= 2, "expected branching CFG for requireAuth");
}
