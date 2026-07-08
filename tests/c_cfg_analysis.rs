//! C CFG analysis against the ecommerce-c fixture.

use rbuilder::analysis::{build_cfg_for_function, cfg_language_id_from_path};
use std::path::PathBuf;

const C_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-c";

fn c_repo() -> PathBuf {
    std::env::var("RBUILDER_C_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(C_REPO))
}

#[test]
fn c_cfg_language_profile_maps_extension() {
    let path = std::path::Path::new("src/services/cart_service.c");
    assert_eq!(cfg_language_id_from_path(path), Some("c"));
}

#[test]
fn c_cfg_builds_checkout_from_fixture() {
    let repo = c_repo();
    let file = repo.join("src/services/order_service.c");
    if !file.is_file() {
        eprintln!("skip: C fixture not found at {}", file.display());
        return;
    }
    let source = std::fs::read_to_string(&file).unwrap();
    let cfg = build_cfg_for_function("c", &source, "order_checkout").expect("order_checkout CFG");
    assert!(cfg.blocks.len() >= 4, "expected branching CFG for checkout");
}

#[test]
fn c_switch_cfg_has_multiple_branches() {
    let code = r#"
int classify(int x) {
    switch (x) {
        case 1: return 10;
        case 2: return 20;
        default: return 0;
    }
}
"#;
    let cfg = build_cfg_for_function("c", code, "classify").expect("classify CFG");
    assert!(cfg.blocks.len() >= 4);
}

#[test]
fn c_loop_has_cycle() {
    let code = r#"
int sum_n(int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        total += i;
    }
    return total;
}
"#;
    let cfg = build_cfg_for_function("c", code, "sum_n").expect("sum_n CFG");
    assert!(cfg.has_cycle());
}
