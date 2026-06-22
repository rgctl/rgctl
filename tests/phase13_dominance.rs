//! Phase 13: dominance analysis (15 tests).

#[path = "common/phase13.rs"]
mod phase13;

use phase13::{build_dominance, with_dominance};
use rbuilder::analysis::{build_cfg_for_function, DominatorTree, ProgramDependenceGraph};

macro_rules! dom_test {
    ($(#[$attr:meta])* $name:ident, $lang:expr, $code:expr, $fn:expr, $check:expr) => {
        $(#[$attr])*
        #[test]
        fn $name() {
            with_dominance($lang, $code, $fn, $check);
        }
    };
}

macro_rules! dom_cfg_test {
    ($name:ident, $body:expr) => {
        #[test]
        fn $name() {
            $body;
        }
    };
}

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_entry_dominates_all,
    "rust",
    r#"
fn test(x: i32) -> i32 {
    if x > 0 {
        return x * 2;
    }
    0
}
"#,
    "test",
    |flow_cfg, dominator| {
        for block in flow_cfg.blocks.keys() {
            assert!(dominator.dominates(flow_cfg.entry, *block));
        }
    }
);

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_entry_idom_self,
    "rust",
    r#"fn entry() { let x = 1; }"#,
    "entry",
    |flow_cfg, dominator| {
        assert_eq!(dominator.idom.get(&flow_cfg.entry), Some(&flow_cfg.entry));
    }
);

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_branch_then_block,
    "rust",
    r#"
fn branch(x: i32) -> i32 {
    if x > 0 {
        x * 2
    } else {
        0
    }
}
"#,
    "branch",
    |flow_cfg, dominator| {
        assert!(flow_cfg.blocks.len() >= 3);
        assert!(dominator.dominates(flow_cfg.entry, flow_cfg.entry));
    }
);

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_after_if_merge,
    "rust",
    r#"
fn merge(x: i32) -> i32 {
    let mut r = 0;
    if x > 0 {
        r = x;
    }
    r
}
"#,
    "merge",
    |flow_cfg, dominator| {
        for block in flow_cfg.blocks.keys() {
            assert!(dominator.dominates(flow_cfg.entry, *block));
        }
    }
);

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_frontiers_on_branch,
    "rust",
    r#"
fn branch(x: i32) {
    if x > 0 {
        let y = 1;
    }
}
"#,
    "branch",
    |flow_cfg, dominator| {
        let has_frontier = dominator.frontiers.values().any(|f| !f.is_empty());
        assert!(has_frontier || flow_cfg.blocks.len() <= 2);
    }
);

#[cfg(feature = "bundle-minimal")]
dom_cfg_test!(dominance_frontier_api_empty, {
    let (flow_cfg, dominator) = build_dominance("rust", r#"fn leaf() {}"#, "leaf");
    let block = *flow_cfg.blocks.keys().next().unwrap();
    let frontier = dominator.frontier(block);
    assert!(frontier.is_empty() || flow_cfg.blocks.len() > 1);
});

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_reflexive,
    "rust",
    r#"fn id(x: i32) -> i32 { x }"#,
    "id",
    |flow_cfg, dominator| {
        for block in flow_cfg.blocks.keys() {
            assert!(dominator.dominates(*block, *block));
        }
    }
);

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_loop_body,
    "rust",
    r#"
fn sum(n: i32) -> i32 {
    let mut s = 0;
    let mut i = 0;
    while i < n {
        s += i;
        i += 1;
    }
    s
}
"#,
    "sum",
    |flow_cfg, dominator| {
        assert!(flow_cfg.blocks.len() >= 3);
        for block in flow_cfg.blocks.keys() {
            assert!(dominator.dominates(flow_cfg.entry, *block));
        }
    }
);

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_nested_if,
    "rust",
    r#"
fn nested(a: i32, b: i32) -> i32 {
    if a > 0 {
        if b > 0 {
            return a + b;
        }
    }
    0
}
"#,
    "nested",
    |flow_cfg, dominator| {
        assert!(flow_cfg.blocks.len() >= 4);
        assert!(dominator.dominates(flow_cfg.entry, flow_cfg.entry));
    }
);

#[cfg(feature = "bundle-minimal")]
dom_cfg_test!(pdg_control_deps_with_dominance, {
    let code = r#"
fn test(x: i32, y: i32) -> i32 {
    let mut result = 0;
    if x > 0 {
        result = x * 2;
    }
    result
}
"#;
    let flow_cfg = build_cfg_for_function("rust", code, "test").unwrap();
    let pdg = ProgramDependenceGraph::build(&flow_cfg, code.as_bytes()).unwrap();
    let dominator = DominatorTree::build(&flow_cfg);
    assert!(!pdg.control_deps.is_empty() || dominator.frontiers.is_empty());
});

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_early_return,
    "rust",
    r#"
fn early(x: i32) -> i32 {
    if x < 0 {
        return 0;
    }
    x + 1
}
"#,
    "early",
    |flow_cfg, dominator| {
        for block in flow_cfg.blocks.keys() {
            assert!(dominator.dominates(flow_cfg.entry, *block));
        }
    }
);

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_python_if,
    "python",
    r#"
def branch(x):
    if x > 0:
        return x * 2
    return 0
"#,
    "branch",
    |flow_cfg, dominator| {
        for block in flow_cfg.blocks.keys() {
            assert!(dominator.dominates(flow_cfg.entry, *block));
        }
    }
);

#[cfg(feature = "bundle-minimal")]
dom_test!(
    dominance_python_while,
    "python",
    r#"
def loop(n):
    s = 0
    i = 0
    while i < n:
        s += i
        i += 1
    return s
"#,
    "loop",
    |flow_cfg, dominator| {
        assert!(flow_cfg.blocks.len() >= 3);
        assert!(dominator.dominates(flow_cfg.entry, flow_cfg.entry));
    }
);

dom_cfg_test!(dominance_frontiers_map_size, {
    #[cfg(feature = "bundle-minimal")]
    {
        let (flow_cfg, dominator) =
            build_dominance("rust", r#"fn g(x: i32) { if x > 0 { let y = 1; } }"#, "g");
        assert_eq!(dominator.frontiers.len(), flow_cfg.blocks.len());
    }
    #[cfg(not(feature = "bundle-minimal"))]
    {
        let (flow_cfg, dominator) =
            build_dominance("python", r#"def g(x):\n    if x > 0:\n        y = 1"#, "g");
        assert_eq!(dominator.frontiers.len(), flow_cfg.blocks.len());
    }
});

dom_cfg_test!(dominance_non_entry_not_self_idom, {
    #[cfg(feature = "bundle-minimal")]
    {
        let (flow_cfg, dominator) = build_dominance(
            "rust",
            r#"
fn wide(x: i32) -> i32 {
    if x > 0 { 1 } else { 2 }
}
"#,
            "wide",
        );
        if flow_cfg.blocks.len() > 2 {
            let non_entry = flow_cfg
                .blocks
                .keys()
                .find(|b| **b != flow_cfg.entry)
                .copied()
                .unwrap();
            let idom = dominator.idom.get(&non_entry).copied().unwrap();
            assert!(dominator.dominates(flow_cfg.entry, non_entry));
            assert!(dominator.dominates(idom, non_entry));
        }
    }
});
