//! CLI coverage for discover `--with-cfg` / `--with-taint` / `--with-security` (#34).

mod dashboard_harness;

use dashboard_harness::{copy_dir_all, rgctl_bin};
use std::path::Path;
use std::process::Command;

fn materialize() -> (tempfile::TempDir, std::path::PathBuf) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir_all(&fixture, &repo).expect("copy fixture");
    let _ = std::fs::remove_dir_all(repo.join(".rgctl"));
    (tmp, repo)
}

fn run_discover(repo: &Path, extra: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(rgctl_bin());
    cmd.current_dir(repo).args([
        "-r",
        repo.to_str().unwrap(),
        "discover",
        ".",
        "--languages",
        "java,rust",
    ]);
    cmd.args(extra);
    cmd.output().expect("spawn rgctl discover")
}

fn assert_ok(output: &std::process::Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn discover_all_flag_is_ignored() {
    let (_baseline_tmp, baseline_repo) = materialize();
    let (_all_tmp, all_repo) = materialize();

    fn nodes_generated(repo: &Path, extra: &[&str]) -> u64 {
        let mut cmd = Command::new(rgctl_bin());
        cmd.current_dir(repo).args([
            "-r",
            repo.to_str().unwrap(),
            "-f",
            "json",
            "discover",
            ".",
            "--languages",
            "java,rust",
        ]);
        cmd.args(extra);
        let out = cmd.output().expect("discover json");
        assert!(
            out.status.success(),
            "discover json failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&out.stdout).expect("discover json")["metrics"]
            ["nodes_generated"]
            .as_u64()
            .expect("nodes_generated")
    }

    let baseline = nodes_generated(&baseline_repo, &[]);
    let with_all = nodes_generated(&all_repo, &["--all"]);
    assert_eq!(
        baseline, with_all,
        "--all must not change graph size after #34"
    );
}

#[test]
fn discover_with_cfg_writes_archive_without_requiring_taint() {
    let (_tmp, repo) = materialize();
    let output = run_discover(&repo, &["--with-cfg"]);
    assert_ok(&output, "discover --with-cfg");
    assert!(
        repo.join(".rgctl/analysis/cfg_pdg.archive.bin").is_file()
            || repo.join(".rgctl/analysis").is_dir(),
        "CFG pass should create analysis artifacts"
    );
}

#[test]
fn discover_cfg_alias_still_works() {
    let (_tmp, repo) = materialize();
    let output = run_discover(&repo, &["--cfg"]);
    assert_ok(&output, "discover --cfg alias");
}

#[test]
fn discover_with_dfg_loops_requires_cfg_pass() {
    let (_tmp, repo) = materialize();
    let output = run_discover(&repo, &["--with-dfg-loops", "--with-cfg"]);
    assert_ok(&output, "discover --with-dfg-loops --with-cfg");
}

#[test]
fn discover_with_ast_skeleton_requires_cfg_pass() {
    let (_tmp, repo) = materialize();
    let output = run_discover(&repo, &["--with-ast-skeleton", "--with-cfg"]);
    assert_ok(&output, "discover --with-ast-skeleton --with-cfg");
}

#[test]
fn discover_help_lists_with_flags_not_all() {
    let output = Command::new(rgctl_bin())
        .args(["discover", "--help"])
        .output()
        .expect("help");
    assert_ok(&output, "discover --help");
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("--with-cfg"));
    assert!(help.contains("--with-taint"));
    assert!(help.contains("--with-security"));
    assert!(
        !help.lines().any(|l| l.trim_start().starts_with("--all")),
        "help must not advertise --all:\n{help}"
    );
}

#[test]
fn cfg_cache_distinguishes_owners_and_rebuilds_identity_changes() {
    use rgctl_analysis::{
        AnalysisStorage,
        cfg_pdg_archive::{CfgPdgArchive, CfgPdgArchiveMmap},
    };
    use rgctl_graph::{SnapshotNodeStore, schema::NodeType};

    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let source = |owner: &str, value: u8| {
        format!(
            "class {owner} {{ int score(int v) {{ return v; }} }}\n\
         class Second {{ int score(int v) {{ return v; }} }}\n\
         class Trigger {{ int touch() {{ return {value}; }} }}\n"
        )
    };
    let write =
        |owner, value| std::fs::write(repo.join("Library.java"), source(owner, value)).unwrap();
    let discover = |flags: &[&str]| {
        let mut args = flags.to_vec();
        args.push("-v");
        let output = run_discover(repo, &args);
        assert_ok(&output, "owner cache discover");
        String::from_utf8(output.stdout).unwrap()
    };
    let check_coverage = || {
        let store = SnapshotNodeStore::open(&repo.join(".rgctl/graph.snapshot.bin")).unwrap();
        let col = store.columnar().unwrap();
        let archive = CfgPdgArchiveMmap::open(&CfgPdgArchive::default_path(repo)).unwrap();
        let storage = AnalysisStorage::new(repo.join(".rgctl/analysis"));
        let index = storage.load_analysis_index().unwrap();
        assert_eq!(index.len(), 3);
        for row in 0..col.node_count() {
            let node = col.materialize_node_at(row).unwrap();
            if node.node_type != NodeType::Function {
                continue;
            }
            let record = archive.load_record(node.id).unwrap().unwrap();
            assert_eq!(record.function_id, node.id);
            let analysis = storage.load_function(node.id).unwrap().unwrap();
            assert_eq!(analysis.function_id, node.id);
            assert_eq!(analysis.code_hash.as_deref(), node.code_hash.as_deref());
            assert_eq!(Some(record.code_hash.as_str()), node.code_hash.as_deref());
            assert!(index.values().any(|entry| entry.function_id == node.id));
        }
    };
    write("First", 0);
    discover(&["--with-cfg"]);
    check_coverage();
    let warm = discover(&["--with-cfg"]);
    assert!(
        warm.contains("CFG cache: 3 reused (3 unchanged), 0 recomputed"),
        "{warm}"
    );
    write("First", 1);
    discover(&["--with-fields"]);
    let edited = discover(&["--with-cfg"]);
    assert!(
        edited.contains("CFG cache: 2 reused (2 unchanged), 1 recomputed"),
        "{edited}"
    );
    check_coverage();
    write("Renamed", 1);
    discover(&["--with-fields"]);
    let renamed = discover(&["--with-cfg"]);
    assert!(
        renamed.contains("CFG cache: 2 reused (2 unchanged), 1 recomputed"),
        "{renamed}"
    );
    check_coverage();
}
