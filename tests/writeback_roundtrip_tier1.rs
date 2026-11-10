//! Lossless discover → writeback verify/export round-trip across Tier-1 languages.

use rgctl_graph::{
    CST_ARCHIVE_FILE, CstArchiveReader, LOSSLESS_CONTENT_STORE_FILE, LosslessContentReader,
};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct LangFixture {
    id: &'static str,
    filename: &'static str,
    source: &'static str,
}

/// Minimal fixtures for all 16 Tier-1 languages (comments / multibyte / macros where relevant).
const FIXTURES: &[LangFixture] = &[
    LangFixture {
        id: "c",
        filename: "mod.c",
        source: "/* keep */\nint target_fn(void) { return 1; }\n",
    },
    LangFixture {
        id: "cpp",
        filename: "mod.cpp",
        source: "// shift\nint target_fn() { return 1; }\n",
    },
    LangFixture {
        id: "csharp",
        filename: "Mod.cs",
        source: "public class Mod {\n  // note\n  public static int TargetFn() { return 1; }\n}\n",
    },
    LangFixture {
        id: "go",
        filename: "mod.go",
        source: "package main\n\n// comment\nfunc target_fn() int {\n\treturn 1\n}\n",
    },
    LangFixture {
        id: "groovy",
        filename: "Mod.groovy",
        source: "// groovy\ndef target_fn() { return 1 }\n",
    },
    LangFixture {
        id: "java",
        filename: "Mod.java",
        source: "public class Mod {\n  /** docs */\n  public static int target_fn() { return 1; }\n}\n",
    },
    LangFixture {
        id: "javascript",
        filename: "mod.js",
        source: "// js\nexport function target_fn() {\n  return 1;\n}\n",
    },
    LangFixture {
        id: "kotlin",
        filename: "Mod.kt",
        source: "// kt\nfun target_fn(): Int = 1\n",
    },
    LangFixture {
        id: "markdown",
        filename: "doc.md",
        source: "# Heading\n\nParagraph with café ☕ and `code`.\n",
    },
    LangFixture {
        id: "php",
        filename: "mod.php",
        source: "<?php\n// php\nfunction target_fn() { return 1; }\n",
    },
    LangFixture {
        id: "puppet",
        filename: "mod.pp",
        source: "# puppet\nclass target_fn {\n}\n",
    },
    LangFixture {
        id: "erb",
        filename: "mod.html.erb",
        source: "<p><%= @target_fn %></p>\n",
    },
    LangFixture {
        id: "python",
        filename: "mod.py",
        source: "# py — café\ndef target_fn():\n    return 1\n",
    },
    LangFixture {
        id: "ruby",
        filename: "mod.rb",
        source: "# ruby\ndef target_fn\n  1\nend\n",
    },
    LangFixture {
        id: "rust",
        filename: "mod.rs",
        source: "// rust macro-ish\nmacro_rules! m { () => {}; }\npub fn target_fn() {\n    let x = 1;\n}\n",
    },
    LangFixture {
        id: "typescript",
        filename: "mod.ts",
        source: "// ts\nexport function target_fn(): number {\n  return 1;\n}\n",
    },
];

fn rgctl_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_rgctl")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl"))
}

fn run_in(repo: &Path, args: &[&str]) -> Output {
    Command::new(rgctl_bin())
        .args(args)
        .current_dir(repo)
        .output()
        .expect("spawn rgctl")
}

fn assert_ok(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed status={:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn materialize_polyglot() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    for fix in FIXTURES {
        let dir = repo.join(fix.id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(fix.filename), fix.source).unwrap();
    }
    (tmp, repo)
}

fn cst_leaf_coverage_for_file(cst: &CstArchiveReader, rel: &str, source_len: usize) {
    if let Some(cst_idx) = cst.find_file(rel).expect("find_file") {
        let leaves = cst.leaf_spans_at(cst_idx).expect("leaf_spans_at");
        assert!(!leaves.is_empty(), "{rel}: expected non-empty CST leaves");
        let mut prev_end = 0u32;
        for (_kind, start, end) in &leaves {
            assert!(
                *start >= prev_end && *start <= *end && (*end as usize) <= source_len,
                "{rel}: invalid leaf span [{start}, {end}) (prev_end={prev_end}, len={source_len})"
            );
            prev_end = *end;
        }
    }
}

#[test]
fn writeback_roundtrip_all_tier1_languages() {
    let (_tmp, repo) = materialize_polyglot();
    let out = run_in(&repo, &["discover", ".", "--lossless"]);
    assert_ok(&out, "discover --lossless");

    let content_path = repo.join(".rgctl").join(LOSSLESS_CONTENT_STORE_FILE);
    let cst_path = repo.join(".rgctl").join(CST_ARCHIVE_FILE);
    assert!(content_path.is_file(), "missing content_store.bin");
    assert!(cst_path.is_file(), "missing cst.archive.bin");

    let verify = run_in(&repo, &["writeback", "verify", "-f", "json"]);
    assert_ok(&verify, "writeback verify");
    let report: Value = serde_json::from_slice(&verify.stdout).expect("verify json");
    assert!(
        report["files_verified_identical"].as_u64().unwrap_or(0) >= FIXTURES.len() as u64,
        "expected ≥{} identical files, got {report}",
        FIXTURES.len()
    );

    let export_dir = repo.join("export_out");
    let export = run_in(
        &repo,
        &[
            "writeback",
            "export",
            "--output",
            export_dir.to_str().unwrap(),
            "-f",
            "json",
        ],
    );
    assert_ok(&export, "writeback export");

    let content = LosslessContentReader::open(&content_path).expect("open content");
    let cst = CstArchiveReader::open(&cst_path).expect("open cst");
    assert!(cst.file_count() > 0);

    for fix in FIXTURES {
        let rel = format!("{}/{}", fix.id, fix.filename);
        let on_disk = fs::read(repo.join(&rel)).unwrap();
        let idx = content
            .find_path(&rel)
            .unwrap()
            .unwrap_or_else(|| panic!("missing frame for {rel}"));
        let reconstructed = content.decompress_at(idx).unwrap();
        assert_eq!(reconstructed, on_disk, "{rel}: BLAKE3/byte identity failed");
        assert_eq!(
            blake3::hash(&reconstructed),
            blake3::hash(&on_disk),
            "{rel}: blake3 mismatch"
        );
        let exported = fs::read(export_dir.join(&rel)).expect("exported file");
        assert_eq!(exported, on_disk, "{rel}: export mismatch");
        cst_leaf_coverage_for_file(&cst, &rel, on_disk.len());
    }

    // Name-span identity via structured find JSON (skip markdown headings without idents).
    let find = run_in(
        &repo,
        &["-f", "json", "find", "--type", "function", "--limit", "200"],
    );
    assert_ok(&find, "find functions");
    let find_json: Value = serde_json::from_slice(&find.stdout).expect("find json");
    let entities = find_json
        .get("entities")
        .or_else(|| find_json.get("results"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut checked = 0usize;
    for ent in &entities {
        let Some(file) = ent.get("file").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(name) = ent.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(ns) = ent.get("name_start_byte").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(ne) = ent.get("name_end_byte").and_then(|v| v.as_u64()) else {
            continue;
        };
        let sb = ent.get("start_byte").and_then(|v| v.as_u64());
        let eb = ent.get("end_byte").and_then(|v| v.as_u64());
        assert!(
            sb.is_some() && eb.is_some() && sb <= eb,
            "expected start_byte and end_byte on hot-scan find entity {ent}"
        );
        if name == "<anonymous>" || name.is_empty() {
            continue;
        }
        let src_path = if Path::new(file).is_absolute() {
            PathBuf::from(file)
        } else {
            repo.join(file)
        };
        let src = fs::read(&src_path).unwrap_or_default();
        if (ne as usize) > src.len() || ns > ne {
            continue;
        }
        let slice = &src[ns as usize..ne as usize];
        assert_eq!(
            slice,
            name.as_bytes(),
            "name span mismatch for {name} in {file}"
        );
        checked += 1;
    }
    assert!(
        checked >= 8,
        "expected several name-span checks, got {checked}"
    );
}
