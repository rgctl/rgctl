//! CLI coverage for unified symbol targeting (`--file` / `--class` / `--line`),
//! query flag parity, and `slice --function Class::method`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn rgctl_bin() -> PathBuf {
    option_env!("CARGO_BIN_EXE_rgctl")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl")
        })
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo")
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn materialize_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&fixture_root(), dir.path()).expect("copy fixture");
    dir
}

fn run_rgctl(repo: &Path, args: &[&str]) -> std::process::Output {
    Command::new(rgctl_bin())
        .current_dir(repo)
        .arg("-r")
        .arg(repo)
        .args(args)
        .output()
        .expect("spawn rgctl")
}

fn discover_java(repo: &Path) {
    let out = run_rgctl(repo, &["discover", ".", "--languages", "java", "--with-cfg"]);
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn inspect_accepts_file_class_line_filters() {
    let dir = materialize_fixture();
    let repo = dir.path();
    discover_java(repo);

    // Flags must precede the layer subcommand: `inspect SYM --file … cfg`.
    let ok = run_rgctl(
        repo,
        &[
            "-f",
            "json",
            "inspect",
            "process",
            "--class",
            "OrderService",
            "--file",
            "OrderService.java",
            "--line",
            "4",
            "cfg",
        ],
    );
    assert!(
        ok.status.success(),
        "inspect with --file/--class/--line failed: stderr={} stdout={}",
        String::from_utf8_lossy(&ok.stderr),
        String::from_utf8_lossy(&ok.stdout)
    );

    // Wrong line → structured not_found (or ambiguous) JSON on stdout, non-zero exit.
    let bad = run_rgctl(
        repo,
        &[
            "-f",
            "json",
            "inspect",
            "process",
            "--class",
            "OrderService",
            "--line",
            "9999",
            "cfg",
        ],
    );
    assert!(!bad.status.success(), "wrong --line should fail");
    let doc: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&bad.stdout).trim())
            .expect("JSON error envelope on stdout");
    assert_eq!(doc.get("ok"), Some(&serde_json::json!(false)));
    assert!(doc.get("error_code").is_some());
    assert!(
        doc.get("recovery_command").is_some() || doc.get("suggestions").is_some(),
        "expected recovery_command or suggestions: {doc}"
    );
}

#[test]
fn cpg_and_blast_radius_accept_file_class_line() {
    let dir = materialize_fixture();
    let repo = dir.path();
    discover_java(repo);

    let cpg = run_rgctl(
        repo,
        &[
            "-f",
            "json",
            "cpg",
            "function",
            "process",
            "--file",
            "OrderService.java",
            "--class",
            "OrderService",
            "--line",
            "4",
        ],
    );
    assert!(
        cpg.status.success(),
        "cpg function disambiguation failed: {}",
        String::from_utf8_lossy(&cpg.stderr)
    );

    let br = run_rgctl(
        repo,
        &[
            "-f",
            "json",
            "blast-radius",
            "process",
            "--class",
            "OrderService",
            "--file",
            "OrderService.java",
            "--line",
            "4",
        ],
    );
    assert!(
        br.status.success(),
        "blast-radius with --line failed: {}",
        String::from_utf8_lossy(&br.stderr)
    );
}

#[test]
fn query_callers_accepts_package_and_methods() {
    let dir = materialize_fixture();
    let repo = dir.path();
    discover_java(repo);

    // Flag parse + wiring: unknown package coords may fail resolve; use methods alone.
    let out = run_rgctl(
        repo,
        &[
            "-f",
            "json",
            "query",
            "callers",
            "audit",
            "--methods",
            "helper,validate",
            "--limit",
            "20",
        ],
    );
    assert!(
        out.status.success(),
        "query callers --methods failed: stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let doc: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).expect("json");
    assert!(doc.get("methods_filter").is_some() || doc.get("callers").is_some());
}

#[test]
fn slice_accepts_class_method_function_selector() {
    let dir = materialize_fixture();
    let repo = dir.path();
    discover_java(repo);

    let file = "java/com/example/OrderService.java";
    let out = run_rgctl(
        repo,
        &[
            "-f",
            "json",
            "slice",
            file,
            "--line",
            "13",
            "--variable",
            "this",
            "--function",
            "OrderService::helper",
        ],
    );
    // Slice may fail if variable not in PDG; success OR structured error is OK —
    // regression is that Class::method is accepted (not "function 'OrderService::helper' not found").
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        !combined.contains("function 'OrderService::helper' not found"),
        "Class::method should normalize to bare name; got: {combined}"
    );
}
