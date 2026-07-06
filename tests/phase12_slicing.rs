//! Phase 12.1 integration: CFG, PDG, and backward slicing.
#![allow(dead_code, unused_imports, unused_macros)]

use rbuilder::analysis::{
    build_cfg_for_function, BackwardSlicer, ProgramDependenceGraph, SliceCriterion,
};

#[cfg(feature = "bundle-minimal")]
#[test]
fn test_rust_backward_slice_excludes_dead_assignments() {
    let code = r#"
fn process(input: String) -> String {
    let a = 10;
    let b = 20;
    let x = input.len();
    let y = x * 2;
    format!("{}", y)
}
"#;
    let cfg = build_cfg_for_function("rust", code, "process").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    let slicer = BackwardSlicer::new(&pdg, &cfg);

    let y_line = pdg
        .nodes
        .values()
        .find(|n| n.defined_vars.contains("y"))
        .unwrap()
        .statement
        .line;

    let slice = slicer
        .slice(SliceCriterion {
            variable: "y".to_string(),
            line: y_line,
        })
        .unwrap();

    let a_line = pdg
        .nodes
        .values()
        .find(|n| n.defined_vars.contains("a"))
        .unwrap()
        .statement
        .line;

    assert!(!slice.lines.contains(&a_line));
    assert!(slice.lines.contains(&y_line));
    assert!(slice.reduction_percent > 0.0);
}

#[cfg(feature = "bundle-minimal")]
#[test]
fn test_cfg_has_entry_and_exit() {
    let code = r#"
fn simple() {
    let x = 1;
}
"#;
    let cfg = build_cfg_for_function("rust", code, "simple").unwrap();
    assert!(cfg.entry != uuid::Uuid::nil());
    assert!(!cfg.exits.is_empty());
    assert!(!cfg.blocks.is_empty());
}

#[cfg(feature = "bundle-minimal")]
#[test]
fn test_pdg_builds_data_dependencies() {
    let code = r#"
fn flow() {
    let x = 1;
    let y = x + 1;
    let _ = y;
}
"#;
    let cfg = build_cfg_for_function("rust", code, "flow").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
    assert!(!pdg.data_deps.is_empty());
    assert!(!pdg.nodes.is_empty());
}

#[cfg(feature = "bundle-minimal")]
#[test]
fn test_python_cfg_builds() {
    let code = r#"
def process(input_val):
    a = 10
    x = len(input_val)
    y = x * 2
    return y
"#;
    let cfg = build_cfg_for_function("python", code, "process").unwrap();
    assert!(!cfg.blocks.is_empty());
}
