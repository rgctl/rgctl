//! C++ CFG analysis against the ecommerce-cpp fixture.

use rbuilder::analysis::{build_cfg_for_function, cfg_language_id_from_path};
use std::path::PathBuf;

const CPP_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-cpp";

fn cpp_repo() -> PathBuf {
    std::env::var("RBUILDER_CPP_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(CPP_REPO))
}

#[test]
fn cpp_cfg_language_profile_maps_extension() {
    let path = std::path::Path::new("src/services/order_service.cpp");
    assert_eq!(cfg_language_id_from_path(path), Some("cpp"));
}

#[test]
fn cpp_cfg_builds_checkout_from_fixture() {
    let repo = cpp_repo();
    let file = repo.join("src/services/order_service.cpp");
    if !file.is_file() {
        eprintln!("skip: C++ fixture not found at {}", file.display());
        return;
    }
    let source = std::fs::read_to_string(&file).unwrap();
    let cfg =
        build_cfg_for_function("cpp", &source, "checkout").expect("checkout CFG");
    assert!(cfg.blocks.len() >= 4, "expected branching CFG for checkout");
}

#[test]
fn cpp_range_for_has_cycle() {
    let code = r#"
#include <vector>

int sum_vec(const std::vector<int>& v) {
    int total = 0;
    for (int x : v) {
        total += x;
    }
    return total;
}
"#;
    let cfg = build_cfg_for_function("cpp", code, "sum_vec").expect("sum_vec CFG");
    assert!(cfg.has_cycle());
}

#[test]
fn cpp_switch_cfg_has_multiple_branches() {
    let code = r#"
int classify(int x) {
    switch (x) {
        case 1: return 10;
        case 2: return 20;
        default: return 0;
    }
}
"#;
    let cfg = build_cfg_for_function("cpp", code, "classify").expect("classify CFG");
    assert!(cfg.blocks.len() >= 4);
}
