//! End-to-end `rgctl writeback patch` rename / conflict / syntax rejection tests.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

fn materialize_multi_lang() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("java")).unwrap();
    fs::create_dir_all(repo.join("rust")).unwrap();
    fs::create_dir_all(repo.join("typescript")).unwrap();
    fs::create_dir_all(repo.join("python")).unwrap();
    fs::create_dir_all(repo.join("go")).unwrap();
    fs::create_dir_all(repo.join("cpp")).unwrap();

    // Shared identifier `alpha_fn` for --rename across languages.
    fs::write(
        repo.join("java/Mod.java"),
        "public class Mod {\n  // keep this comment untouched\n  public static int alpha_fn() { return beta_fn(); }\n  public static int beta_fn() { return 1; }\n}\n",
    )
    .unwrap();
    fs::write(
        repo.join("rust/lib.rs"),
        "fn alpha_fn() {\n    // keep\n    beta_fn();\n}\nfn beta_fn() {}\n",
    )
    .unwrap();
    fs::write(
        repo.join("typescript/mod.ts"),
        "export function alpha_fn(): number {\n  // keep\n  return beta_fn();\n}\nexport function beta_fn(): number { return 1; }\n",
    )
    .unwrap();
    fs::write(
        repo.join("python/mod.py"),
        "def alpha_fn():\n    # keep\n    return beta_fn()\n\ndef beta_fn():\n    return 1\n",
    )
    .unwrap();
    fs::write(
        repo.join("go/mod.go"),
        "package main\n\nfunc alpha_fn() int {\n\t// keep\n\treturn beta_fn()\n}\n\nfunc beta_fn() int { return 1 }\n",
    )
    .unwrap();
    fs::write(
        repo.join("cpp/mod.cpp"),
        "int beta_fn();\nint alpha_fn() {\n  // keep\n  return beta_fn();\n}\nint beta_fn() { return 1; }\n",
    )
    .unwrap();
    (tmp, repo)
}

#[test]
fn writeback_patch_rename_preserves_comments() {
    let (_tmp, repo) = materialize_multi_lang();
    assert_ok(
        &run_in(&repo, &["discover", ".", "--lossless"]),
        "discover --lossless",
    );

    let before_java = fs::read(repo.join("java/Mod.java")).unwrap();
    let out_dir = repo.join("patched");
    let patch = run_in(
        &repo,
        &[
            "writeback",
            "patch",
            "--rename",
            "alpha_fn=omega_fn",
            "--output",
            out_dir.to_str().unwrap(),
            "-f",
            "json",
        ],
    );
    assert_ok(&patch, "writeback patch --rename");
    let report: Value = serde_json::from_slice(&patch.stdout).expect("json");
    assert!(
        report["edits_applied"].as_u64().unwrap_or(0) >= 6,
        "expected renames across languages: {report}"
    );

    let patched_java = fs::read(out_dir.join("java/Mod.java")).expect("patched java");
    let patched_str = String::from_utf8_lossy(&patched_java);
    assert!(patched_str.contains("omega_fn"));
    assert!(!patched_str.contains("alpha_fn"));
    assert!(
        patched_str.contains("// keep this comment untouched"),
        "comments must stay byte-identical outside spliced spans"
    );
    // Non-renamed regions: comment line bytes equal original.
    let comment = b"// keep this comment untouched";
    assert!(
        patched_java.windows(comment.len()).any(|w| w == comment),
        "comment bytes must be untouched"
    );
    assert_ne!(before_java, patched_java);

    for rel in [
        "rust/lib.rs",
        "typescript/mod.ts",
        "python/mod.py",
        "go/mod.go",
        "cpp/mod.cpp",
    ] {
        let text = fs::read_to_string(out_dir.join(rel)).unwrap_or_default();
        assert!(text.contains("omega_fn"), "{rel} missing rename: {text}");
        assert!(text.contains("// keep") || text.contains("# keep"), "{rel}");
    }
}

#[test]
fn writeback_patch_rejects_overlapping_edits() {
    let (_tmp, repo) = materialize_multi_lang();
    assert_ok(
        &run_in(&repo, &["discover", ".", "--lossless"]),
        "discover --lossless",
    );
    let plan = repo.join("plan_conflict.json");
    fs::write(
        &plan,
        r#"{
  "files": [{
    "file": "rust/lib.rs",
    "edits": [
      {"start_byte": 3, "end_byte": 11, "replacement": "omega_fn", "reason": "rename"},
      {"start_byte": 5, "end_byte": 14, "replacement": "other", "reason": "rename"}
    ]
  }]
}"#,
    )
    .unwrap();
    let patch = run_in(
        &repo,
        &[
            "writeback",
            "patch",
            "--plan",
            plan.to_str().unwrap(),
            "--dry-run",
            "-f",
            "json",
        ],
    );
    assert_ok(&patch, "writeback patch conflict plan");
    let report: Value = serde_json::from_slice(&patch.stdout).expect("json");
    let entries = report["entries"].as_array().cloned().unwrap_or_default();
    assert!(
        entries.iter().any(|e| e["status"] == "conflict_rejected"),
        "expected ConflictRejected: {report}"
    );
}

#[test]
fn writeback_patch_rejects_invalid_syntax() {
    let (_tmp, repo) = materialize_multi_lang();
    assert_ok(
        &run_in(&repo, &["discover", ".", "--lossless"]),
        "discover --lossless",
    );
    let plan = repo.join("plan_syntax.json");
    // Replace `alpha_fn` (bytes 3..11 in `fn alpha_fn()...`) with invalid token.
    fs::write(
        &plan,
        r#"{
  "files": [{
    "file": "rust/lib.rs",
    "edits": [
      {"start_byte": 3, "end_byte": 11, "replacement": "@invalid", "reason": "rename"}
    ]
  }]
}"#,
    )
    .unwrap();
    let patch = run_in(
        &repo,
        &[
            "writeback",
            "patch",
            "--plan",
            plan.to_str().unwrap(),
            "--dry-run",
            "-f",
            "json",
        ],
    );
    assert_ok(&patch, "writeback patch syntax plan");
    let report: Value = serde_json::from_slice(&patch.stdout).expect("json");
    let entries = report["entries"].as_array().cloned().unwrap_or_default();
    assert!(
        entries
            .iter()
            .any(|e| e["status"] == "syntax_error_rejected"),
        "expected SyntaxErrorRejected: {report}"
    );
}

#[test]
fn writeback_export_works_standalone_when_sources_removed_and_rename_respects_cst_tokens() {
    let (_tmp, repo) = materialize_multi_lang();
    // Add a file with substrings (`alpha_fn_extended`), comments (`// alpha_fn`), and string literals (`"alpha_fn"`).
    let tricky_rs = "fn alpha_fn() -> &'static str {\n    // alpha_fn in comment must not change\n    let alpha_fn_extended = 42;\n    let _ = alpha_fn_extended;\n    \"alpha_fn\"\n}\n";
    fs::write(repo.join("rust/lib.rs"), tricky_rs).unwrap();

    assert_ok(
        &run_in(&repo, &["discover", ".", "--lossless"]),
        "discover --lossless",
    );

    // Delete the original source files from disk to prove standalone `.rgctl/` reconstruction!
    fs::remove_dir_all(repo.join("rust")).unwrap();
    fs::remove_dir_all(repo.join("java")).unwrap();

    let export_dir = repo.join("standalone_export");
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
    assert_ok(&export, "standalone writeback export");
    let exported_rs = fs::read_to_string(export_dir.join("rust/lib.rs")).expect("exported rust");
    assert_eq!(exported_rs, tricky_rs);

    // Also run `--rename alpha_fn=omega_fn` and verify comments, strings, and `alpha_fn_extended` are untouched!
    let patched_dir = repo.join("standalone_patched");
    let patch = run_in(
        &repo,
        &[
            "writeback",
            "patch",
            "--rename",
            "alpha_fn=omega_fn",
            "--output",
            patched_dir.to_str().unwrap(),
            "-f",
            "json",
        ],
    );
    assert_ok(&patch, "writeback patch --rename with CST token boundaries");
    let patched_rs = fs::read_to_string(patched_dir.join("rust/lib.rs")).expect("patched rust");
    assert!(patched_rs.contains("fn omega_fn()"));
    assert!(
        patched_rs.contains("// alpha_fn in comment must not change"),
        "comment was corrupted: {patched_rs}"
    );
    assert!(
        patched_rs.contains("let alpha_fn_extended = 42;"),
        "identifier prefix was corrupted: {patched_rs}"
    );
    assert!(
        patched_rs.contains("\"alpha_fn\""),
        "string literal was corrupted: {patched_rs}"
    );
}
