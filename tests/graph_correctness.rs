//! Graph data correctness — expected-facts + cross-feature invariants.
//!
//! Fixtures live under `rgctl-tests/ecommerce-*/correctness/expected-facts.json`.
//!
//! ```bash
//! cargo test --test graph_correctness
//! cargo test --test graph_correctness java   # filter by project id in test name
//! ```
//!
//! See `rgctl-tests/correctness/SCHEMA.md` and https://github.com/sshaaf/rgctl/issues/26

#![allow(clippy::too_many_arguments)]

mod graph_correctness_lib;

use graph_correctness_lib::{ProjectSpec, run_project};
use std::path::PathBuf;

fn rgctl_tests_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests")
}

fn rgctl_bin() -> PathBuf {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_rgctl") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl")
}

const PROJECTS: &[ProjectSpec] = &[
    ProjectSpec {
        id: "java",
        dir_name: "ecommerce-java",
        exclude: "target,data",
    },
    ProjectSpec {
        id: "rust",
        dir_name: "ecommerce-rust",
        exclude: "target",
    },
    ProjectSpec {
        id: "python",
        dir_name: "ecommerce-python",
        exclude: ".venv,__pycache__",
    },
    ProjectSpec {
        id: "go",
        dir_name: "ecommerce-go",
        exclude: "vendor",
    },
    ProjectSpec {
        id: "csharp",
        dir_name: "ecommerce-csharp",
        exclude: "bin,obj,data",
    },
    ProjectSpec {
        id: "typescript",
        dir_name: "ecommerce-typescript",
        exclude: "node_modules,dist",
    },
    ProjectSpec {
        id: "javascript",
        dir_name: "ecommerce-javascript",
        exclude: "node_modules",
    },
    ProjectSpec {
        id: "c",
        dir_name: "ecommerce-c",
        exclude: "build",
    },
    ProjectSpec {
        id: "cpp",
        dir_name: "ecommerce-cpp",
        exclude: "build",
    },
    ProjectSpec {
        id: "php",
        dir_name: "ecommerce-php",
        exclude: "vendor",
    },
    ProjectSpec {
        id: "ruby",
        dir_name: "ecommerce-ruby",
        exclude: "vendor",
    },
    ProjectSpec {
        id: "kotlin",
        dir_name: "ecommerce-kotlin",
        exclude: "build,target",
    },
    ProjectSpec {
        id: "groovy",
        dir_name: "ecommerce-groovy",
        exclude: "build,target",
    },
    ProjectSpec {
        id: "puppet",
        dir_name: "ecommerce-puppet",
        exclude: "spec,vendor",
    },
];

fn assert_project(spec: &ProjectSpec) {
    let root = rgctl_tests_root();
    let project_dir = root.join(spec.dir_name);
    let facts = project_dir.join("correctness").join("expected-facts.json");
    if !facts.is_file() {
        eprintln!(
            "skip {}: no {}",
            spec.id,
            facts.strip_prefix(&root).unwrap_or(&facts).display()
        );
        return;
    }
    assert!(
        project_dir.is_dir(),
        "missing project dir {}",
        project_dir.display()
    );
    let bin = rgctl_bin();
    assert!(
        bin.is_file(),
        "rgctl binary not found at {} (build the test binary first)",
        bin.display()
    );

    let report = run_project(&bin, &project_dir, &facts, spec.exclude, true);
    for c in &report.checks {
        let mark = if c.ok {
            "PASS"
        } else if c.severity == "required" {
            "FAIL"
        } else {
            "WARN"
        };
        eprintln!("  [{mark}] {}: {}", c.id, c.message);
    }
    assert_eq!(
        report.required_failures, 0,
        "{}: {} required correctness failure(s)",
        spec.id, report.required_failures
    );
}

#[test]
fn correctness_java() {
    assert_project(&PROJECTS[0]);
}
#[test]
fn correctness_rust() {
    assert_project(&PROJECTS[1]);
}
#[test]
fn correctness_python() {
    assert_project(&PROJECTS[2]);
}
#[test]
fn correctness_go() {
    assert_project(&PROJECTS[3]);
}
#[test]
fn correctness_csharp() {
    assert_project(&PROJECTS[4]);
}
#[test]
fn correctness_typescript() {
    assert_project(&PROJECTS[5]);
}
#[test]
fn correctness_javascript() {
    assert_project(&PROJECTS[6]);
}
#[test]
fn correctness_c() {
    assert_project(&PROJECTS[7]);
}
#[test]
fn correctness_cpp() {
    assert_project(&PROJECTS[8]);
}
#[test]
fn correctness_php() {
    assert_project(&PROJECTS[9]);
}
#[test]
fn correctness_ruby() {
    assert_project(&PROJECTS[10]);
}
#[test]
fn correctness_kotlin() {
    assert_project(&PROJECTS[11]);
}
#[test]
fn correctness_groovy() {
    assert_project(&PROJECTS[12]);
}
#[test]
fn correctness_puppet() {
    assert_project(&PROJECTS[13]);
}
