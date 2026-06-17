//! Shared polyglot repository fixtures for Phase 11 integration and benchmark tests.

use std::fs;
use std::path::Path;

/// Write a small multi-language repository fixture to `root`.
pub fn write_polyglot_repo(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join(".github/workflows")).unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();

    fs::write(root.join("src/main.rs"), "fn main() { helper(); }\nfn helper() {}\n").unwrap();
    fs::write(root.join("src/util.py"), "def helper():\n    return 1\n").unwrap();
    fs::write(root.join("src/app.ts"), "export function helper(): number { return 1; }\n").unwrap();
    fs::write(root.join("src/app.js"), "function helper() { return 1; }\n").unwrap();
    fs::write(root.join("src/main.go"), "package main\nfunc helper() int { return 1 }\n").unwrap();
    fs::write(
        root.join("schema.sql"),
        "CREATE TABLE users (id INTEGER PRIMARY KEY);\nCREATE VIEW active_users AS SELECT id FROM users;\n",
    )
    .unwrap();
    fs::write(
        root.join("Dockerfile"),
        "FROM alpine:3.19\nCOPY schema.sql .\nRUN echo ok\n",
    )
    .unwrap();
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n",
    )
    .unwrap();
    fs::write(
        root.join("scripts/deploy.sh"),
        "deploy() { echo deploy; }\nsource ./common.sh\n",
    )
    .unwrap();
    fs::write(root.join("config.yaml"), "app:\n  name: demo\n").unwrap();
}

/// Write a larger polyglot fixture for performance benchmarks (`file_count` code files).
pub fn write_scaled_polyglot_repo(root: &Path, file_count: usize) {
    write_polyglot_repo(root);
    for i in 0..file_count {
        let path = root.join(format!("src/bench_{i}.rs"));
        fs::write(path, format!("fn bench_fn_{i}() -> i32 {{ {i} }}\n")).unwrap();
    }
}
