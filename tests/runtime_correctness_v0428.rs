//! Regression tests for v0.4.28 runtime/CLI correctness fixes (#214):
//! - `rgctl metrics --betweenness` active-pivot sampling & `--iterations` passthrough
//! - `StructuredQuery` (`rgctl callers --file`) repo-relative path-boundary matching
//! - `rgctl vuln scan` un-truncated `Import` indexing past 5,000 imports

use rgctl_graph::columnar_snapshot::write_columnar_from_nodes_edges;
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::snapshot::SNAPSHOT_FILE;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn rgctl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rgctl")
}

#[test]
fn metrics_json_honors_iterations_and_computes_sampled_betweenness() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("mkdir");
    std::fs::write(
        src_dir.join("lib.rs"),
        r#"
pub fn entry_a() { bridge(); }
pub fn entry_b() { bridge(); }
pub fn bridge() { leaf_x(); leaf_y(); }
pub fn leaf_x() {}
pub fn leaf_y() {}
"#,
    )
    .expect("write");

    let disc = Command::new(rgctl_bin())
        .current_dir(tmp.path())
        .args(["discover", "."])
        .output()
        .expect("discover");
    assert!(disc.status.success(), "discover failed");

    let pr_out = Command::new(rgctl_bin())
        .current_dir(tmp.path())
        .args(["-f", "json", "metrics", "--pagerank", "--iterations", "2"])
        .output()
        .expect("metrics pagerank");
    assert!(pr_out.status.success(), "metrics pagerank failed");
    let pr_json: Value = serde_json::from_slice(&pr_out.stdout).expect("json");
    let iters = pr_json["pagerank"]["iterations"]
        .as_u64()
        .expect("iterations field");
    assert!(
        iters <= 2,
        "expected PageRank iterations <= 2 when --iterations 2 passed, got {iters}: {pr_json}"
    );

    let bc_out = Command::new(rgctl_bin())
        .current_dir(tmp.path())
        .args(["-f", "json", "metrics", "--betweenness"])
        .output()
        .expect("metrics betweenness");
    assert!(bc_out.status.success(), "metrics betweenness failed");
    let bc_json: Value = serde_json::from_slice(&bc_out.stdout).expect("json");
    let rows = bc_json["betweenness"]
        .as_array()
        .expect("betweenness array");
    assert!(
        !rows.is_empty(),
        "expected non-empty betweenness rows for bridge call graph: {bc_json}"
    );
    assert_eq!(rows[0]["name"].as_str(), Some("bridge"));
}

#[test]
fn callers_accepts_repo_relative_file_path_and_enforces_segment_boundary() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir_a = tmp.path().join("crates/alpha/src");
    let dir_b = tmp.path().join("crates/beta_alpha/src");
    std::fs::create_dir_all(&dir_a).expect("mkdir a");
    std::fs::create_dir_all(&dir_b).expect("mkdir b");

    std::fs::write(
        dir_a.join("helper.rs"),
        "pub fn shared_target() {}\npub fn caller_alpha() { shared_target(); }\n",
    )
    .expect("write a");
    std::fs::write(
        dir_b.join("helper.rs"),
        "pub fn shared_target() {}\npub fn caller_beta() { shared_target(); }\n",
    )
    .expect("write b");

    let disc = Command::new(rgctl_bin())
        .current_dir(tmp.path())
        .args(["discover", "."])
        .output()
        .expect("discover");
    assert!(disc.status.success(), "discover failed");

    // Repo-relative path with slashes must resolve `crates/alpha/src/helper.rs` without matching `beta_alpha/src/helper.rs`.
    let out = Command::new(rgctl_bin())
        .current_dir(tmp.path())
        .args([
            "-f",
            "json",
            "callers",
            "shared_target",
            "--file",
            "alpha/src/helper.rs",
        ])
        .output()
        .expect("callers");
    assert!(
        out.status.success(),
        "callers --file alpha/src/helper.rs failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let doc: Value = serde_json::from_slice(&out.stdout).expect("json");
    let callers = doc["callers"].as_array().expect("callers array");
    assert_eq!(callers.len(), 1, "expected 1 caller, got {doc}");
    assert_eq!(callers[0]["name"].as_str(), Some("caller_alpha"));
}

#[test]
fn vuln_scan_matches_imports_beyond_5000_node_threshold() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo: &Path = tmp.path();

    // Minimal Maven pom declaring vulnerable jackson-databind 2.13.0.
    std::fs::write(
        repo.join("pom.xml"),
        r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>demo</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>com.fasterxml.jackson.core</groupId>
      <artifactId>jackson-databind</artifactId>
      <version>2.13.0</version>
    </dependency>
  </dependencies>
</project>"#,
    )
    .expect("write pom.xml");

    // Local offline OSV cache entry in `.rgctl/osv/`.
    let osv_dir = repo.join(".rgctl/osv");
    std::fs::create_dir_all(&osv_dir).expect("mkdir osv");
    std::fs::write(
        osv_dir.join("GHSA-test-5500.json"),
        r#"{
  "id": "GHSA-test-5500",
  "modified": "2026-01-01T00:00:00Z",
  "summary": "Test advisory beyond 5000 imports",
  "affected": [
    {
      "package": {
        "ecosystem": "Maven",
        "name": "com.fasterxml.jackson.core:jackson-databind"
      },
      "ranges": [
        {
          "type": "ECOSYSTEM",
          "events": [
            { "introduced": "0" },
            { "fixed": "2.13.4.2" }
          ]
        }
      ],
      "ecosystem_specific": {
        "imports": [
          {
            "path": "com.fasterxml.jackson.databind.ObjectMapper",
            "symbols": ["readValue"]
          }
        ]
      }
    }
  ]
}"#,
    )
    .expect("write osv");

    // Build a columnar snapshot with 5,499 unrelated Import nodes (UUIDs < target) and
    // one matching Import node at index 5,499 (UUID `ffffffff-...` sorts last!).
    let mut nodes = Vec::with_capacity(5_501);
    for i in 0..5_499u32 {
        let mut n = Node::new(NodeType::Import, format!("org.unrelated.pkg{i}.Helper"));
        n.id = uuid::Uuid::from_u128((i as u128) + 1);
        n.file_path = Some(
            repo.join("src/Unrelated.java")
                .to_string_lossy()
                .into_owned()
                .into(),
        );
        nodes.push(n);
    }
    let mut caller_fn = Node::new(NodeType::Function, "run".to_string());
    caller_fn.id = uuid::Uuid::from_u128(u128::MAX - 2);
    caller_fn.qualified_name = Some("com.acme.App.run".into());
    caller_fn.file_path = Some(
        repo.join("src/App.java")
            .to_string_lossy()
            .into_owned()
            .into(),
    );
    nodes.push(caller_fn.clone());

    let mut matching = Node::new(
        NodeType::Import,
        "com.fasterxml.jackson.databind.ObjectMapper".to_string(),
    );
    matching.id = uuid::Uuid::from_u128(u128::MAX - 1);
    matching.file_path = Some(
        repo.join("src/App.java")
            .to_string_lossy()
            .into_owned()
            .into(),
    );
    nodes.push(matching);

    let edges: Vec<Edge> = Vec::new();
    let snap_path = repo.join(".rgctl").join(SNAPSHOT_FILE);
    write_columnar_from_nodes_edges(nodes, edges, &snap_path).expect("write snapshot");
    let _ = EdgeType::Calls;

    // Bare parked call `readValue` in `src/App.java` — requires Import #5,500 (`ObjectMapper`)
    // in `file_imports` to attribute `readValue` to `ObjectMapper.readValue`!
    let parked = vec![rgctl_extraction::ParkedRelationRecord {
        from_node_id: caller_fn.id,
        from_qualified_name: "com.acme.App.run".to_string(),
        from_file: repo.join("src/App.java").to_string_lossy().into_owned(),
        call_site_line: 12,
        relation_type: rgctl_plugin_api::RelationType::Calls,
        to_name: "readValue".to_string(),
        to_qualified_hint: None,
        to_type_hint: None,
        import_module_hint: None,
        stub_node_id: None,
    }];
    rgctl_extraction::save_parked_relations(
        &repo
            .join(".rgctl")
            .join(rgctl_extraction::PARKED_RELATIONS_FILE),
        &parked,
    )
    .expect("save parked");

    let out = Command::new(rgctl_bin())
        .current_dir(repo)
        .args(["-f", "json", "vuln", "scan", "--offline"])
        .output()
        .expect("vuln scan");
    assert!(
        out.status.success(),
        "vuln scan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: Value = serde_json::from_slice(&out.stdout).expect("json");
    let results = doc["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1);
    assert_ne!(
        results[0]["exploitability"].as_str(),
        Some("not_exploitable"),
        "expected Import at index 5,500 to link parked readValue call without 5,000-import truncation: {}",
        results[0]
    );
    let callers = results[0]["sink_taint"]["sink_callers"]
        .as_array()
        .expect("sink_callers");
    assert!(
        callers
            .iter()
            .any(|c| c.as_str() == Some("com.acme.App.run")),
        "expected com.acme.App.run in sink_callers, got {callers:?}"
    );
}
