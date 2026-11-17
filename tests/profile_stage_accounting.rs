//! Verify `[profile]` stage wall-clock accounting and `--full` file-list reuse.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

fn rgctl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rgctl")
}

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("mkdir");
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let ty = entry.file_type().expect("file_type");
        let name = entry.file_name();
        if name == ".rgctl" || name == "target" {
            continue;
        }
        let target = dst.join(name);
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else if ty.is_file() {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

fn extract_field(line: &str, key: &str) -> Option<f64> {
    let needle = format!(" {key}=");
    let rest = line.split(&needle).nth(1)?;
    let token = rest.split_whitespace().next()?;
    token.parse::<f64>().ok()
}

fn extract_stage_name(line: &str) -> Option<String> {
    let rest = line.split("stage=\"").nth(1)?;
    let name = rest.split('"').next()?;
    Some(name.to_string())
}

#[test]
fn cold_discover_profile_accounts_for_wall_secs_within_half_second() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    copy_dir_all(&fixture, tmp.path());

    let out = Command::new(rgctl_bin())
        .current_dir(tmp.path())
        .env("RUST_LOG", "info,profile=info")
        .env("NO_COLOR", "1")
        .args(["-f", "json", "discover", ".", "-v"])
        .output()
        .expect("run rgctl discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stderr = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let summary_line = stderr
        .lines()
        .find(|l| l.contains("[profile] discover summary"))
        .unwrap_or_else(|| panic!("missing discover summary in:\n{stderr}"));

    let wall = extract_field(summary_line, "wall_secs").expect("wall_secs");
    let pre = extract_field(summary_line, "pre_index_secs").expect("pre_index_secs");
    let index = extract_field(summary_line, "index_secs").expect("index_secs");
    let post = extract_field(summary_line, "post_index_secs").expect("post_index_secs");
    let accounted = extract_field(summary_line, "accounted_secs").expect("accounted_secs");

    assert!(
        (accounted - (pre + index + post)).abs() < 1e-6,
        "accounted_secs mismatch: {summary_line}"
    );
    let unattributed = (wall - accounted).abs();
    assert!(
        unattributed < 0.5,
        "unattributed wall gap {unattributed:.4}s >= 0.5s in:\n{stderr}"
    );

    let mut stages: HashMap<String, f64> = HashMap::new();
    for line in stderr.lines().filter(|l| l.contains("[profile] stage")) {
        if let (Some(name), Some(secs)) = (extract_stage_name(line), extract_field(line, "secs")) {
            stages.insert(name, secs);
        }
    }

    for expected in [
        "file_discover",
        "detect_changes",
        "index_extract",
        "index_graph_build",
        "cold_open",
        "topology",
        "community",
        "save_analysis",
        "save_tracker",
    ] {
        assert!(
            stages.contains_key(expected),
            "expected [profile] stage `{expected}` in:\n{stderr}"
        );
    }
}

#[test]
fn full_discover_reuses_discovered_files_and_profiles_semantic_index() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    copy_dir_all(&fixture, tmp.path());

    let out = Command::new(rgctl_bin())
        .current_dir(tmp.path())
        .env("RUST_LOG", "info,profile=info")
        .env("NO_COLOR", "1")
        .args(["-f", "json", "discover", ".", "--full", "-v"])
        .output()
        .expect("run rgctl discover --full");
    assert!(
        out.status.success(),
        "discover --full failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stderr = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let file_discover_count = stderr
        .lines()
        .filter(|l| l.contains("[profile] stage") && l.contains("stage=\"file_discover\""))
        .count();
    assert_eq!(
        file_discover_count, 1,
        "--full must walk directory tree once (expected 1 file_discover stage, got {file_discover_count}):\n{stderr}"
    );

    let has_semantic = stderr
        .lines()
        .any(|l| l.contains("[profile] stage") && l.contains("stage=\"semantic_index\""));
    assert!(
        has_semantic,
        "expected [profile] stage `semantic_index` in --full output:\n{stderr}"
    );
}
