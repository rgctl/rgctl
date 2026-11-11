//! Cold discover profile gates for linux, metasfresh, kafka, and markdown (k8s-website).
//!
//! ```text
//! cargo build --release --bin rgctl
//! cargo test --release --test cold_profile_gates -- --ignored --nocapture
//! ```
//!
//! Cold profile policy: run a fresh release build immediately before profiling and
//! use that `target/release/rgctl` binary only (no debug/stale binaries).
//!
//! Markdown corpus: `./scripts/fetch-profile-repos.sh` then
//! `k8s_website_markdown_cold_discover_within_baseline` or
//! `k8s_website_obsidian_export_to_vault` (warm index).

mod dashboard_harness;

use dashboard_harness::{metasfresh_repo_path, rgctl_bin};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// Post–single-pass-AST linux cold wall (default discover, no cfg/dashboard/harmonic).
/// Baseline: **75 s** cold wall on reference M3 Pro / 36 GB (2026-10-21; 3-run mean ~71 s).
const LINUX_COLD_WALL_BASELINE_SECS: f64 = 75.0;
const LINUX_COLD_MAX_NODES: u64 = 2_800_000;
/// metasfresh `discover --full` cold wall (gate uses process elapsed, not just final summary).
/// Baseline: **50 s** on same reference machine (2026-10-21; elapsed ~45 s, final summary ~25 s).
const METASFRESH_COLD_WALL_BASELINE_SECS: f64 = 50.0;
/// Override via `RGCTL_KAFKA_COLD_BASELINE_SECS`. Measured ~9.6 s (2026-10-21).
const KAFKA_COLD_WALL_BASELINE_SECS: f64 = 12.0;
/// `rust-lang/rust` full tree with `-l rust`. Baseline: **25 s** (2026-10-21; measured ~23.5 s).
const RUST_COLD_WALL_BASELINE_SECS: f64 = 25.0;
/// llvm/llvm-project `clang/` sparse checkout with `-l cpp`. Baseline: **12 s** (2026-10-21; ~9.7 s).
const LLVM_CPP_COLD_WALL_BASELINE_SECS: f64 = 12.0;
/// dotnet/roslyn `src/` with `-l csharp`. Baseline: **22 s** (2026-11-03 post-98% AST; ~20.2 s process).
const ROSLYN_CSHARP_COLD_WALL_BASELINE_SECS: f64 = 22.0;
/// microsoft/vscode `src/` with `-l typescript`. Baseline: **17 s** (2026-10-21; ~15.0 s).
const VSCODE_TYPESCRIPT_COLD_WALL_BASELINE_SECS: f64 = 17.0;
/// nodejs/node `test/` with `-l javascript`. Baseline: **4 s** (2026-10-21; ~3.2 s).
const NODE_JAVASCRIPT_COLD_WALL_BASELINE_SECS: f64 = 4.0;
/// Same corpus with `--with-cfg`. Baseline: **5 s** (2026-10-21; ~3.8 s).
const NODE_JAVASCRIPT_COLD_WITH_CFG_WALL_BASELINE_SECS: f64 = 5.0;
/// home-assistant/core with `-l python`. Baseline: **15 s** (2026-10-21; ~13.4 s).
const HOME_ASSISTANT_PYTHON_COLD_WALL_BASELINE_SECS: f64 = 15.0;
/// discourse with `-l ruby`. Baseline: **8 s** (2026-11-03 post-98% AST; ~7.0 s process / ~5.4 s wall).
const DISCOURSE_RUBY_COLD_WALL_BASELINE_SECS: f64 = 8.0;
/// JetBrains/kotlin sparse `libraries`+`plugins`+`analysis` (`-l kotlin`).
/// Baseline: **15 s** (2026-11-03 post-98% AST; ~13.7 s process, ~238k nodes).
/// Override via `RGCTL_KOTLIN_COLD_BASELINE_SECS`.
const KOTLIN_COLD_WALL_BASELINE_SECS: f64 = 15.0;
/// gradle/gradle under `example/groovy` (`-l groovy`).
/// Baseline: **8 s** (2026-11-03 post-98% AST; ~6.9 s wall, ~139k nodes).
/// Override via `RGCTL_GROOVY_COLD_BASELINE_SECS`.
const GROOVY_COLD_WALL_BASELINE_SECS: f64 = 8.0;
/// kubernetes/website markdown-only discover. Baseline: **9 s** (2026-10-21; ~7.7 s).
const K8S_WEBSITE_MARKDOWN_COLD_WALL_BASELINE_SECS: f64 = 9.0;
/// ecommerce-java default discover cold wall (inheritance stub gate).
/// Baseline: **0.36 s** (2026-10-21; ~0.33 s).
const ECOMMERCE_JAVA_COLD_WALL_BASELINE_SECS: f64 = 0.36;
/// `index_graph_build` stage for the same corpus (relation commit + stubs).
const ECOMMERCE_JAVA_COLD_INDEX_GRAPH_BUILD_BASELINE_SECS: f64 = 0.013;
const K8S_WEBSITE_MIN_HEADING_MODULES: u64 = 500;
/// Obsidian vault export on warm k8s index (~15–25s on maintainer machine).
const K8S_WEBSITE_OBSIDIAN_EXPORT_BASELINE_SECS: f64 = 30.0;
/// Delta `pr-check` on `crates/rgctl-graph` self-slice (`HEAD~5..HEAD`, base cache only).
/// Baseline: **1.0 s** wall on reference M3 Pro (2026-09-10, release binary).
const PR_CHECK_RGCTL_GRAPH_SLICE_BASELINE_SECS: f64 = 1.0;
/// Linux cold *diff* (v7.1 vs HEAD snapshots under `.rgctl-diff/`). Separate from cold discover.
/// Provisional baseline until recorded on reference machine; override via
/// `RGCTL_LINUX_COLD_DIFF_BASELINE_SECS`.
const LINUX_COLD_DIFF_WALL_BASELINE_SECS: f64 = 30.0;
const TOLERANCE: f64 = 1.10;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProfileSummary {
    pub wall_secs: f64,
    pub peak_rss_mb: f64,
    pub ingest_peak_rss_mb: f64,
    pub nodes: u64,
    pub functions: u64,
    pub index_graph_build_secs: Option<f64>,
}

pub fn linux_repo_path() -> PathBuf {
    std::env::var("RGCTL_LINUX_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/linux"))
}

pub fn kafka_repo_path() -> PathBuf {
    std::env::var("RGCTL_KAFKA_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/kafka"))
}

pub fn rgctl_graph_repo_path() -> PathBuf {
    std::env::var("RGCTL_GRAPH_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("crates/rgctl-graph"))
}

pub fn k8s_website_repo_path() -> PathBuf {
    std::env::var("RGCTL_K8S_WEBSITE_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/k8s-website"))
}

pub fn ecommerce_java_repo_path() -> PathBuf {
    std::env::var("RGCTL_ECOMMERCE_JAVA_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java")
        })
}

pub fn rust_repo_path() -> PathBuf {
    std::env::var("RGCTL_RUST_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/rust"))
}

pub fn llvm_cpp_repo_path() -> PathBuf {
    std::env::var("RGCTL_LLVM_CPP_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/llvm-project/clang")
        })
}

pub fn roslyn_csharp_repo_path() -> PathBuf {
    std::env::var("RGCTL_ROSLYN_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/roslyn/src"))
}

pub fn vscode_typescript_repo_path() -> PathBuf {
    std::env::var("RGCTL_VSCODE_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/vscode/src"))
}

pub fn home_assistant_python_repo_path() -> PathBuf {
    std::env::var("RGCTL_HOME_ASSISTANT_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/home-assistant")
        })
}

pub fn discourse_ruby_repo_path() -> PathBuf {
    std::env::var("RGCTL_DISCOURSE_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/discourse"))
}

pub fn kotlin_repo_path() -> PathBuf {
    std::env::var("RGCTL_KOTLIN_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/kotlin"))
}

pub fn groovy_repo_path() -> PathBuf {
    std::env::var("RGCTL_GROOVY_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/groovy"))
}

pub fn node_javascript_repo_path() -> PathBuf {
    std::env::var("RGCTL_NODE_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/node/test"))
}

pub fn parse_profile_summary(log: &str) -> Option<ProfileSummary> {
    let mut summary = ProfileSummary::default();
    for line in log.lines() {
        if line.contains("[profile] discover summary") {
            if let Some(v) = parse_field_f64(line, "wall_secs=") {
                summary.wall_secs = v;
            }
            if let Some(v) = parse_field_f64(line, "peak_rss_mb=") {
                summary.peak_rss_mb = v;
            }
            if let Some(v) = parse_field_f64(line, "ingest_peak_rss_mb=") {
                summary.ingest_peak_rss_mb = v;
            }
            if let Some(v) = parse_field_u64(line, "nodes=") {
                summary.nodes = v;
            }
            if let Some(v) = parse_field_u64(line, "functions=") {
                summary.functions = v;
            }
        } else if line.contains("[profile] stage") && line.contains("index_graph_build") {
            if let Some(secs) = parse_field_f64(line, "secs=") {
                summary.index_graph_build_secs = Some(secs);
            }
        }
    }
    if summary.wall_secs > 0.0 {
        Some(summary)
    } else {
        None
    }
}

fn find_json_object_end(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_discover_json(stdout: &str) -> Option<Value> {
    for anchor in ["\"command\": \"discover\"", "\"command\":\"discover\""] {
        if let Some(anchor_idx) = stdout.rfind(anchor) {
            let start = stdout[..anchor_idx].rfind('{')?;
            let slice = &stdout[start..];
            let end = find_json_object_end(slice)?;
            return serde_json::from_str(slice[..=end].trim()).ok();
        }
    }
    None
}

fn resolve_profile_summary(stdout: &str, stderr: &str, elapsed: Duration) -> ProfileSummary {
    let combined = format!("{stdout}\n{stderr}");
    if let Some(summary) = parse_profile_summary(&combined) {
        return summary;
    }
    if let Some(doc) = parse_discover_json(stdout) {
        let metrics = doc.get("metrics").and_then(|m| m.as_object());
        let nodes = metrics
            .and_then(|m| m.get("nodes_generated"))
            .and_then(|n| n.as_u64())
            .unwrap_or(0);
        let wall_secs = metrics
            .and_then(|m| m.get("duration_ms"))
            .and_then(|n| n.as_u64())
            .map(|ms| ms as f64 / 1000.0)
            .unwrap_or_else(|| elapsed.as_secs_f64());
        return ProfileSummary {
            wall_secs,
            nodes,
            ..ProfileSummary::default()
        };
    }
    let stdout_tail: String = stdout
        .chars()
        .rev()
        .take(1200)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    panic!(
        "profile summary missing (expected [profile] discover summary or JSON metrics)\n\
         rgctl: {}\n\
         stdout_bytes={} stderr_bytes={}\n\
         stdout_tail:\n{stdout_tail}\n\
         stderr:\n{stderr}",
        rgctl_bin().display(),
        stdout.len(),
        stderr.len(),
    );
}

fn parse_field_f64(line: &str, key: &str) -> Option<f64> {
    let rest = line.split(key).nth(1)?;
    let token = rest.split_whitespace().next()?;
    token.parse().ok()
}

fn parse_field_u64(line: &str, key: &str) -> Option<u64> {
    let rest = line.split(key).nth(1)?;
    let token = rest.split_whitespace().next()?;
    token.parse().ok()
}

pub fn run_cold_discover_timed(repo: &Path, extra_args: &[&str]) -> (Output, Duration) {
    static DISCOVER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = DISCOVER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let rb = repo.join(".rgctl");
    if rb.exists() {
        std::fs::remove_dir_all(&rb).expect("remove stale .rgctl for cold discover");
    }
    let bin = rgctl_bin();
    assert!(
        bin.is_file(),
        "rgctl binary not found at {} — run cargo build --release --bin rgctl",
        bin.display()
    );
    let start = Instant::now();
    let output = Command::new(&bin)
        .current_dir(repo)
        .env("RUST_LOG", "info,profile=info")
        .env("NO_COLOR", "1")
        .args(["-f", "json", "discover", ".", "-v"])
        .args(extra_args)
        .output()
        .expect("spawn rgctl discover");
    (output, start.elapsed())
}

fn assert_within_baseline(label: &str, elapsed: Duration, baseline_secs: f64) {
    let limit = baseline_secs * TOLERANCE;
    assert!(
        elapsed.as_secs_f64() <= limit,
        "{label}: {:.1}s exceeds baseline {:.1}s (+10% = {:.1}s)",
        elapsed.as_secs_f64(),
        baseline_secs,
        limit
    );
}

fn count_files_recursive(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files_recursive(&path);
            } else if path.is_file() {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn profile_parser_reads_linux_log_snippet() {
    let snippet = r#"2026-08-13T10:50:09.578546Z  INFO profile: [profile] discover summary wall_secs=157.810180667 index_secs=103.66666675 post_index_secs=24.973709293 peak_rss_mb=15256.0 ingest_peak_rss_mb=12929.0 analysis_peak_rss_mb=15256.0 functions=1862845 nodes=3312280 cfg=false security=false
2026-08-13T10:50:09.578564Z  INFO profile: [profile] stage stage="index_graph_build" secs=15.808327416000001 pct_wall=10.017305188540167"#;
    let parsed = parse_profile_summary(snippet).expect("parse");
    assert!((parsed.wall_secs - 157.810).abs() < 0.01);
    assert_eq!(parsed.nodes, 3_312_280);
    assert_eq!(parsed.functions, 1_862_845);
    assert!(parsed.index_graph_build_secs.unwrap() > 15.0);
}

#[test]
fn profile_parser_tolerates_missing_optional_fields() {
    let snippet = "INFO profile: [profile] discover summary wall_secs=1.1 peak_rss_mb=100.0 functions=0 nodes=60994 cfg=false security=false";
    let parsed = parse_profile_summary(snippet).expect("parse");
    assert!((parsed.wall_secs - 1.1).abs() < 0.01);
    assert_eq!(parsed.nodes, 60994);
    assert_eq!(parsed.ingest_peak_rss_mb, 0.0);
}

#[test]
fn profile_resolver_falls_back_to_discover_json() {
    let stdout = r#"{
  "command": "discover",
  "metrics": {
    "duration_ms": 825,
    "nodes_generated": 60994
  },
  "schema_version": 2
}"#;
    let summary = resolve_profile_summary(stdout, "", Duration::from_millis(900));
    assert!((summary.wall_secs - 0.825).abs() < 0.01);
    assert_eq!(summary.nodes, 60994);
}

#[test]
#[ignore = "manual: cold discover profile on example/linux"]
fn linux_cold_discover_within_baseline() {
    let repo = linux_repo_path();
    if !repo.is_dir() {
        eprintln!("skip: linux not at {}", repo.display());
        return;
    }

    let (output, elapsed) = run_cold_discover_timed(&repo, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "linux cold: wall={:.1}s nodes={} index_graph_build={:?}",
        profile.wall_secs, profile.nodes, profile.index_graph_build_secs
    );
    assert!(
        profile.nodes <= LINUX_COLD_MAX_NODES,
        "nodes {} exceed cap {}",
        profile.nodes,
        LINUX_COLD_MAX_NODES
    );
    assert_within_baseline(
        "linux cold discover",
        elapsed,
        LINUX_COLD_WALL_BASELINE_SECS,
    );
}

#[test]
#[ignore = "manual: cold discover profile on metasfresh"]
fn metasfresh_cold_discover_within_baseline() {
    let repo = metasfresh_repo_path();
    if !repo.is_dir() {
        eprintln!("skip: metasfresh not at {}", repo.display());
        return;
    }

    let (output, elapsed) = run_cold_discover_timed(&repo, &["--full"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "metasfresh cold: wall={:.1}s nodes={} functions={}",
        profile.wall_secs, profile.nodes, profile.functions
    );
    assert_within_baseline(
        "metasfresh cold discover",
        elapsed,
        METASFRESH_COLD_WALL_BASELINE_SECS,
    );
}

#[test]
#[ignore = "manual: cold discover profile on example/kafka"]
fn kafka_cold_discover_within_baseline() {
    let repo = kafka_repo_path();
    if !repo.is_dir() {
        eprintln!("skip: kafka not at {}", repo.display());
        return;
    }

    let baseline = std::env::var("RGCTL_KAFKA_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(KAFKA_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "kafka cold: wall={:.1}s nodes={} functions={} (baseline {:.0}s)",
        profile.wall_secs, profile.nodes, profile.functions, baseline
    );
    assert_within_baseline("kafka cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover profile on example/rust (rust-lang/rust, -l rust)"]
fn rust_cold_discover_within_baseline() {
    let repo = rust_repo_path();
    if !repo.is_dir() {
        eprintln!("skip: rust not at {}", repo.display());
        return;
    }

    let baseline = std::env::var("RGCTL_RUST_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(RUST_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "rust"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "rust cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("rust cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover profile on example/llvm-project/clang (-l cpp)"]
fn llvm_cpp_cold_discover_within_baseline() {
    let repo = llvm_cpp_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: llvm clang not at {} (run ./scripts/fetch-profile-repos.sh)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_LLVM_CPP_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(LLVM_CPP_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "cpp"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "llvm cpp cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("llvm cpp cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover profile on example/roslyn/src (-l csharp)"]
fn roslyn_csharp_cold_discover_within_baseline() {
    let repo = roslyn_csharp_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: roslyn src not at {} (run ./scripts/fetch-profile-repos.sh)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_ROSLYN_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(ROSLYN_CSHARP_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "csharp"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "roslyn csharp cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("roslyn csharp cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover profile on example/vscode/src (-l typescript)"]
fn vscode_typescript_cold_discover_within_baseline() {
    let repo = vscode_typescript_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: vscode src not at {} (run ./scripts/fetch-profile-repos.sh)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_VSCODE_TYPESCRIPT_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(VSCODE_TYPESCRIPT_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "typescript"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "vscode typescript cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("vscode typescript cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover profile on example/node/test (-l javascript)"]
fn node_javascript_cold_discover_within_baseline() {
    let repo = node_javascript_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: node test corpus not at {} (run ./scripts/fetch-profile-repos.sh)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_NODE_JAVASCRIPT_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(NODE_JAVASCRIPT_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "javascript"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "node javascript cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("node javascript cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover profile on example/node/test (-l javascript --with-cfg)"]
fn node_javascript_cold_discover_with_cfg_within_baseline() {
    let repo = node_javascript_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: node test corpus not at {} (run ./scripts/fetch-profile-repos.sh)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_NODE_JAVASCRIPT_COLD_WITH_CFG_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(NODE_JAVASCRIPT_COLD_WITH_CFG_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "javascript", "--with-cfg"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "node javascript cfg cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("node javascript cfg cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover profile on example/home-assistant (-l python)"]
fn home_assistant_python_cold_discover_within_baseline() {
    let repo = home_assistant_python_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: home-assistant not at {} (run ./scripts/fetch-profile-repos.sh)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_HOME_ASSISTANT_PYTHON_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(HOME_ASSISTANT_PYTHON_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "python"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "home-assistant python cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("home-assistant python cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover profile on rgctl-tests/ecommerce-java (inheritance stubs)"]
fn ecommerce_java_inheritance_cold_discover_within_baseline() {
    let repo = ecommerce_java_repo_path();
    if !repo.is_dir() {
        eprintln!("skip: ecommerce-java not at {}", repo.display());
        return;
    }

    let wall_baseline = std::env::var("RGCTL_ECOMMERCE_JAVA_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(ECOMMERCE_JAVA_COLD_WALL_BASELINE_SECS);
    let index_baseline = std::env::var("RGCTL_ECOMMERCE_JAVA_INDEX_GRAPH_BUILD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(ECOMMERCE_JAVA_COLD_INDEX_GRAPH_BUILD_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "ecommerce-java cold: wall={:.3}s nodes={} index_graph_build={:?} peak_rss_mb={:.1} (wall baseline {:.3}s, index_graph_build baseline {:.4}s)",
        profile.wall_secs,
        profile.nodes,
        profile.index_graph_build_secs,
        profile.peak_rss_mb,
        wall_baseline,
        index_baseline
    );
    assert_within_baseline(
        "ecommerce-java cold discover",
        Duration::from_secs_f64(profile.wall_secs),
        wall_baseline,
    );
    if let Some(index_secs) = profile.index_graph_build_secs {
        let limit = index_baseline * TOLERANCE;
        assert!(
            index_secs <= limit,
            "index_graph_build {:.4}s exceeds baseline {:.4}s (+10% = {:.4}s)",
            index_secs,
            index_baseline,
            limit
        );
    } else {
        panic!("index_graph_build stage missing from profile output");
    }
}

fn parse_gql_json(stdout: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(stdout);
    for anchor in ["\"rows\"", "\"count\""] {
        if let Some(anchor_idx) = text.rfind(anchor) {
            let start = text[..anchor_idx].rfind('{')?;
            let slice = &text[start..];
            let end = find_json_object_end(slice)?;
            if let Ok(value) = serde_json::from_str(slice[..=end].trim()) {
                return Some(value);
            }
        }
    }
    None
}

fn run_json_gql(repo: &Path, query: &str) -> Value {
    let bin = rgctl_bin();
    let output = Command::new(&bin)
        .args(["-r", repo.to_str().unwrap(), "-f", "json", "gql", query])
        .output()
        .expect("spawn gql");
    assert!(
        output.status.success(),
        "gql failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    parse_gql_json(&output.stdout).expect("gql json")
}

#[test]
#[ignore = "manual: cold markdown discover on example/k8s-website (kubernetes/website content/en)"]
fn k8s_website_markdown_cold_discover_within_baseline() {
    let repo = k8s_website_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: k8s-website not at {} (run ./scripts/fetch-profile-repos.sh)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_K8S_WEBSITE_DISCOVER_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(K8S_WEBSITE_MARKDOWN_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "markdown"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "k8s-website markdown cold: wall={:.1}s elapsed={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.1}s)",
        profile.wall_secs,
        elapsed.as_secs_f64(),
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_eq!(
        profile.functions, 0,
        "markdown-only discover should index no functions"
    );
    assert_within_baseline(
        "k8s-website markdown cold discover",
        Duration::from_secs_f64(profile.wall_secs),
        baseline,
    );

    let headings = run_json_gql(&repo, "MATCH (n:Module) WHERE n.kind = 'heading' RETURN n");
    let heading_count = headings.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    eprintln!("k8s-website heading modules: {heading_count}");
    assert!(
        heading_count >= K8S_WEBSITE_MIN_HEADING_MODULES,
        "expected at least {K8S_WEBSITE_MIN_HEADING_MODULES} heading modules, got {heading_count}"
    );

    let functions = run_json_gql(&repo, "MATCH (n:Function) RETURN n");
    let function_count = functions.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    assert_eq!(
        function_count, 0,
        "markdown-only discover should index no functions"
    );
}

#[test]
#[ignore = "manual: obsidian export on example/k8s-website (requires discover index)"]
fn k8s_website_obsidian_export_to_vault() {
    let repo = k8s_website_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: k8s-website not at {} (run ./scripts/fetch-profile-repos.sh)",
            repo.display()
        );
        return;
    }
    if !repo.join(".rgctl/graph.snapshot.bin").is_file() {
        eprintln!(
            "skip: no graph at {} (run rgctl -r \"{}\" discover . -l markdown)",
            repo.join(".rgctl").display(),
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_K8S_WEBSITE_OBSIDIAN_EXPORT_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(K8S_WEBSITE_OBSIDIAN_EXPORT_BASELINE_SECS);

    let vault = repo.join("vault");
    if vault.exists() {
        std::fs::remove_dir_all(&vault).expect("remove stale vault");
    }

    let bin = rgctl_bin();
    let start = Instant::now();
    let output = Command::new(&bin)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "export",
            "--export-format",
            "obsidian",
            "--export-output",
            vault.to_str().unwrap(),
            "--query",
            "all",
        ])
        .output()
        .expect("spawn obsidian export");
    let elapsed = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "obsidian export failed:\nstdout={stdout}\nstderr={stderr}"
    );

    let note_count = count_files_recursive(&vault);

    let headings = run_json_gql(&repo, "MATCH (n:Module) WHERE n.kind = 'heading' RETURN n");
    let heading_count = headings.get("count").and_then(|c| c.as_u64()).unwrap_or(0);

    eprintln!(
        "k8s-website obsidian export: wall={:.1}s notes={} headings={} (baseline {:.1}s)",
        elapsed.as_secs_f64(),
        note_count,
        heading_count,
        baseline
    );
    assert_eq!(
        note_count as u64, heading_count,
        "vault note count should match heading modules"
    );
    assert!(
        heading_count >= K8S_WEBSITE_MIN_HEADING_MODULES,
        "expected at least {K8S_WEBSITE_MIN_HEADING_MODULES} heading modules, got {heading_count}"
    );
    assert_within_baseline("k8s-website obsidian export", elapsed, baseline);
}

#[test]
#[ignore = "manual: delta pr-check on crates/rgctl-graph (requires warm .rgctl-base only)"]
fn pr_check_rgctl_graph_slice_within_baseline() {
    let repo = rgctl_graph_repo_path();
    let base_snapshot = repo.join(".rgctl-base/.rgctl/graph.snapshot.bin");
    if !base_snapshot.is_file() {
        eprintln!(
            "skip: no base snapshot at {} (copy head artifact to .rgctl-base/.rgctl/)",
            base_snapshot.display()
        );
        return;
    }

    let policy = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/rgctl-pr-policy.json");
    if !policy.is_file() {
        eprintln!("skip: policy fixture missing at {}", policy.display());
        return;
    }

    let baseline = std::env::var("RGCTL_PR_CHECK_RGCTL_GRAPH_SLICE_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(PR_CHECK_RGCTL_GRAPH_SLICE_BASELINE_SECS);

    let bin = rgctl_bin();
    let start = Instant::now();
    let output = Command::new(&bin)
        .current_dir(&repo)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "-f",
            "json",
            "pr-check",
            "--policy-file",
            policy.to_str().unwrap(),
            "--base-ref",
            "HEAD~5",
            "--head-ref",
            "HEAD",
        ])
        .output()
        .expect("spawn pr-check");
    let elapsed = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "pr-check failed:\nstdout={stdout}\nstderr={stderr}"
    );
    eprintln!(
        "pr-check rgctl-graph slice: wall={:.3}s (baseline {:.3}s)",
        elapsed.as_secs_f64(),
        baseline
    );
    assert_within_baseline("pr-check rgctl-graph slice", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover on example/discourse (-l ruby app lib plugins); set RGCTL_DISCOURSE_RUBY_COLD_BASELINE_SECS after reference run"]
fn discourse_cold_discover_within_baseline() {
    let repo = discourse_ruby_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: discourse corpus not at {} (clone example/discourse or set RGCTL_DISCOURSE_REPO)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_DISCOURSE_RUBY_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DISCOURSE_RUBY_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "ruby"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "discourse ruby cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("discourse ruby cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover Gate B on example/kotlin (-l kotlin); ./scripts/fetch-profile-repos.sh"]
fn kotlin_cold_discover_within_baseline() {
    let repo = kotlin_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: kotlin corpus not at {} (run ./scripts/fetch-profile-repos.sh or set RGCTL_KOTLIN_REPO)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_KOTLIN_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(KOTLIN_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "kotlin"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "kotlin cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("kotlin cold discover", elapsed, baseline);
}

#[test]
#[ignore = "manual: cold discover Gate B on example/groovy (gradle/gradle, -l groovy); ./scripts/fetch-profile-repos.sh"]
fn groovy_cold_discover_within_baseline() {
    let repo = groovy_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: groovy corpus not at {} (run ./scripts/fetch-profile-repos.sh or set RGCTL_GROOVY_REPO)",
            repo.display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_GROOVY_COLD_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(GROOVY_COLD_WALL_BASELINE_SECS);

    let (output, elapsed) = run_cold_discover_timed(&repo, &["-l", "groovy"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "discover failed:\nstdout={stdout}\nstderr={stderr}"
    );
    let profile = resolve_profile_summary(&stdout, &stderr, elapsed);
    eprintln!(
        "groovy cold: wall={:.1}s nodes={} functions={} index_graph_build={:?} (baseline {:.0}s)",
        profile.wall_secs,
        profile.nodes,
        profile.functions,
        profile.index_graph_build_secs,
        baseline
    );
    assert_within_baseline("groovy cold discover", elapsed, baseline);
}

#[derive(Debug, Clone, Default, PartialEq)]
struct DiffProfileSummary {
    wall_secs: f64,
    open_secs: f64,
    digest_secs: f64,
    diff_secs: f64,
    nodes_added: u64,
    nodes_removed: u64,
    nodes_changed: u64,
}

fn parse_diff_profile_summary(log: &str) -> Option<DiffProfileSummary> {
    let mut summary = DiffProfileSummary::default();
    for line in log.lines() {
        if !line.contains("[profile] diff summary") {
            continue;
        }
        if let Some(v) = parse_field_f64(line, "wall_secs=") {
            summary.wall_secs = v;
        }
        if let Some(v) = parse_field_f64(line, "open_secs=") {
            summary.open_secs = v;
        }
        if let Some(v) = parse_field_f64(line, "digest_secs=") {
            summary.digest_secs = v;
        }
        if let Some(v) = parse_field_f64(line, "diff_secs=") {
            summary.diff_secs = v;
        }
        if let Some(v) = parse_field_u64(line, "nodes_added=") {
            summary.nodes_added = v;
        }
        if let Some(v) = parse_field_u64(line, "nodes_removed=") {
            summary.nodes_removed = v;
        }
        if let Some(v) = parse_field_u64(line, "nodes_changed=") {
            summary.nodes_changed = v;
        }
    }
    if summary.wall_secs > 0.0 {
        Some(summary)
    } else {
        None
    }
}

fn parse_diff_json(stdout: &str) -> Option<DiffProfileSummary> {
    let anchor = stdout.find("\"schema_version\"")?;
    let start = stdout[..anchor].rfind('{')?;
    let slice = &stdout[start..];
    let end = find_json_object_end(slice)?;
    let value: Value = serde_json::from_str(&slice[..=end]).ok()?;
    let timings = value.get("timings")?;
    Some(DiffProfileSummary {
        wall_secs: timings.get("wall_secs")?.as_f64()?,
        open_secs: timings
            .get("open_secs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        digest_secs: timings
            .get("digest_secs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        diff_secs: timings
            .get("diff_secs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        nodes_added: value.get("nodes_added")?.as_u64()?,
        nodes_removed: value.get("nodes_removed")?.as_u64()?,
        nodes_changed: value.get("nodes_changed")?.as_u64()?,
    })
}

fn linux_diff_pair_paths(repo: &Path) -> (PathBuf, PathBuf) {
    let root = repo.join(".rgctl-diff");
    (root.join("base"), root.join("head"))
}

/// Cold snapshot-pair diff (not discover). Prep with:
/// `./scripts/prepare-linux-diff-snapshots.sh` (default BASE_REF=v7.1, HEAD_REF=HEAD).
#[test]
#[ignore = "manual: cold diff profile on example/linux/.rgctl-diff (run prepare-linux-diff-snapshots.sh first)"]
fn linux_cold_diff_within_baseline() {
    let repo = linux_repo_path();
    let (base, head) = linux_diff_pair_paths(&repo);
    let base_snap = base.join("graph.snapshot.bin");
    let head_snap = head.join("graph.snapshot.bin");
    if !base_snap.is_file() || !head_snap.is_file() {
        eprintln!(
            "skip: linux diff snapshots missing at {} (run ./scripts/prepare-linux-diff-snapshots.sh)",
            repo.join(".rgctl-diff").display()
        );
        return;
    }

    let baseline = std::env::var("RGCTL_LINUX_COLD_DIFF_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(LINUX_COLD_DIFF_WALL_BASELINE_SECS);

    let bin = rgctl_bin();
    assert!(
        bin.is_file(),
        "rgctl binary not found at {} — run cargo build --release --bin rgctl",
        bin.display()
    );

    let start = Instant::now();
    let output = Command::new(&bin)
        .current_dir(&repo)
        .env("RUST_LOG", "info,profile=info")
        .args([
            "-f",
            "json",
            "diff",
            "--base",
            base.to_str().expect("utf8 base path"),
            "--head",
            head.to_str().expect("utf8 head path"),
        ])
        .output()
        .expect("spawn rgctl diff");
    let elapsed = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "diff failed:\nstdout={stdout}\nstderr={stderr}"
    );

    let profile = parse_diff_json(&stdout)
        .or_else(|| parse_diff_profile_summary(&stderr))
        .unwrap_or(DiffProfileSummary {
            wall_secs: elapsed.as_secs_f64(),
            ..DiffProfileSummary::default()
        });
    eprintln!(
        "linux cold diff: wall={:.3}s open={:.3}s digest={:.3}s diff={:.3}s added={} removed={} changed={} (baseline {:.1}s)",
        profile.wall_secs,
        profile.open_secs,
        profile.digest_secs,
        profile.diff_secs,
        profile.nodes_added,
        profile.nodes_removed,
        profile.nodes_changed,
        baseline
    );
    assert_within_baseline("linux cold diff", elapsed, baseline);
}

/// Opt-in lossless discover + writeback verify throughput (does not alter Gate A).
///
/// Default Gate A (`linux_cold_discover_within_baseline`) remains the non-`--lossless`
/// discover path; this gate only measures the opt-in sidecar path on a small fixture.
const TIER1_LOSSLESS_WRITEBACK_BASELINE_SECS: f64 = 8.0;

#[test]
#[ignore = "manual: tier1 lossless writeback roundtrip perf (discover --lossless + writeback verify)"]
fn tier1_lossless_writeback_roundtrip_perf() {
    let repo = ecommerce_java_repo_path();
    if !repo.is_dir() {
        eprintln!("skip: ecommerce-java not at {}", repo.display());
        return;
    }
    let baseline = std::env::var("RGCTL_TIER1_LOSSLESS_WRITEBACK_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(TIER1_LOSSLESS_WRITEBACK_BASELINE_SECS);

    let bin = rgctl_bin();
    assert!(
        bin.is_file(),
        "rgctl binary not found at {} — run cargo build --release --bin rgctl",
        bin.display()
    );

    let _ = std::fs::remove_dir_all(repo.join(".rgctl"));
    let start = Instant::now();
    let discover = Command::new(&bin)
        .current_dir(&repo)
        .env("RUST_LOG", "info,profile=info")
        .args(["discover", ".", "--lossless"])
        .output()
        .expect("spawn discover --lossless");
    let discover_elapsed = start.elapsed();
    assert!(
        discover.status.success(),
        "discover --lossless failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&discover.stdout),
        String::from_utf8_lossy(&discover.stderr)
    );

    let verify_start = Instant::now();
    let verify = Command::new(&bin)
        .current_dir(&repo)
        .args(["writeback", "verify", "-f", "json"])
        .output()
        .expect("spawn writeback verify");
    let verify_elapsed = verify_start.elapsed();
    assert!(
        verify.status.success(),
        "writeback verify failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );

    let total = discover_elapsed + verify_elapsed;
    eprintln!(
        "tier1 lossless writeback: discover={:.3}s verify={:.3}s total={:.3}s (baseline {:.1}s); Gate A path unchanged (opt-in --lossless only)",
        discover_elapsed.as_secs_f64(),
        verify_elapsed.as_secs_f64(),
        total.as_secs_f64(),
        baseline
    );
    assert_within_baseline("tier1 lossless writeback roundtrip", total, baseline);

    // Content store + CST must exist after --lossless; default discover does not require them.
    assert!(
        repo.join(".rgctl/content_store.bin").is_file(),
        "expected RGCS content_store.bin after --lossless"
    );
    assert!(
        repo.join(".rgctl/cst.archive.bin").is_file(),
        "expected RGCT cst.archive.bin after --lossless"
    );
}
