//! Integration tests for `discover --with-kantra`.

mod rgctl_harness;

use rgctl_harness::rgctl;
use std::process::Command;
use std::sync::Mutex;

/// Both ecommerce-java tests write `.rgctl/` in the same fixture tree.
static ECOMMERCE_LOCK: Mutex<()> = Mutex::new(());
/// tiny_polyglot_repo tests share one `.rgctl/` artifact directory.
static TINY_REPO_LOCK: Mutex<()> = Mutex::new(());

fn rgctl_bin() -> std::path::PathBuf {
    std::env::var("RGCTL_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| rgctl())
}

fn rules_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kantra-rules")
}

#[test]
fn kantra_rules_indexed_in_graph_gql() {
    let _guard = TINY_REPO_LOCK.lock().unwrap();
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    if !repo.is_dir() {
        return;
    }
    let gql = Command::new(rgctl_bin())
        .args([
            "discover",
            ".",
            "-l",
            "go",
            "--with-kantra",
            "--kantra-index-only",
        ])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        gql.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&gql.stderr)
    );
    let gql = Command::new(rgctl_bin())
        .args(["-f", "json", "gql", "MATCH (r:KantraRule) RETURN r"])
        .current_dir(&repo)
        .output()
        .expect("gql");
    assert!(
        gql.status.success(),
        "gql failed: {}",
        String::from_utf8_lossy(&gql.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&gql.stdout).unwrap();
    let rows = doc.get("rows").and_then(|v| v.as_array()).unwrap();
    assert!(
        !rows.is_empty(),
        "expected KantraRule nodes in graph after kantra index"
    );
}

#[test]
fn kantra_embedded_discover_writes_findings_json() {
    let _guard = ECOMMERCE_LOCK.lock().unwrap();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    if !repo.is_dir() {
        return;
    }
    let out = Command::new(rgctl_bin())
        .args(["discover", ".", "--languages", "java", "--with-kantra"])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let findings_path = repo.join(".rgctl/kantra_findings.json");
    assert!(findings_path.is_file(), "kantra_findings.json missing");
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&findings_path).unwrap()).unwrap();
    assert_eq!(
        doc.get("command").and_then(|v| v.as_str()),
        Some("kantra_findings")
    );
    assert!(
        doc.get("catalog_id")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty()),
        "expected catalog_id from embedded catalog"
    );
}

#[test]
fn kantra_rules_and_catalog_mutually_exclusive() {
    let _guard = TINY_REPO_LOCK.lock().unwrap();
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    if !repo.is_dir() {
        return;
    }
    let rules = rules_dir();
    let out = Command::new(rgctl_bin())
        .args([
            "discover",
            ".",
            "--with-kantra",
            "--kantra-rules",
            rules.to_str().unwrap(),
            "--kantra-catalog",
            rules.to_str().unwrap(),
        ])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--kantra-rules") || stderr.contains("--kantra-catalog"));
}

#[test]
fn kantra_discover_writes_findings_json() {
    let _guard = ECOMMERCE_LOCK.lock().unwrap();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    if !repo.is_dir() {
        return;
    }
    let rules = rules_dir();
    let out = Command::new(rgctl_bin())
        .args([
            "discover",
            ".",
            "--languages",
            "java",
            "--with-kantra",
            "--kantra-rules",
            rules.to_str().unwrap(),
        ])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let findings_path = repo.join(".rgctl/kantra_findings.json");
    assert!(findings_path.is_file(), "kantra_findings.json missing");
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&findings_path).unwrap()).unwrap();
    assert_eq!(
        doc.get("command").and_then(|v| v.as_str()),
        Some("kantra_findings")
    );
    assert_eq!(doc.get("schema_version").and_then(|v| v.as_u64()), Some(2));
    let violations = doc.get("violations").and_then(|v| v.as_array()).unwrap();
    assert!(!violations.is_empty(), "expected at least one violation");
    assert!(
        violations[0].get("rule_id").is_some()
            && violations[0].get("file").is_some()
            && violations[0].get("line").is_some()
    );
    let skipped = doc.get("skipped_rules").and_then(|v| v.as_array()).unwrap();
    assert!(
        skipped.iter().any(|s| {
            s.get("rule_id").and_then(|v| v.as_str()) == Some("unsupported-dependency")
        }),
        "expected unsupported rule in skipped_rules"
    );
}

#[test]
fn kantra_markdown_imports_not_matched_by_go_referenced() {
    let _guard = TINY_REPO_LOCK.lock().unwrap();
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    if !repo.is_dir() {
        return;
    }
    let rules = rules_dir();
    let out = Command::new(rgctl_bin())
        .args([
            "discover",
            ".",
            "-l",
            "markdown,go",
            "--with-kantra",
            "--kantra-rules",
            rules.to_str().unwrap(),
        ])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let findings_path = repo.join(".rgctl/kantra_findings.json");
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(findings_path).unwrap()).unwrap();
    let violations = doc.get("violations").and_then(|v| v.as_array()).unwrap();
    assert!(
        !violations.iter().any(|v| {
            v.get("matched_by").and_then(|m| m.as_str()) == Some("go.referenced")
                && v.get("file")
                    .and_then(|f| f.as_str())
                    .is_some_and(|f| f.ends_with(".md"))
        }),
        "markdown imports must not match go.referenced"
    );
}

#[test]
fn kantra_rule_target_property_gql() {
    let _guard = TINY_REPO_LOCK.lock().unwrap();
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    if !repo.is_dir() {
        return;
    }
    let out = Command::new(rgctl_bin())
        .args([
            "discover",
            ".",
            "-l",
            "go",
            "--with-kantra",
            "--kantra-index-only",
        ])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let gql = Command::new(rgctl_bin())
        .args([
            "-f",
            "json",
            "gql",
            "MATCH (r:KantraRule) WHERE r.`konveyor.io/target` = 'quarkus' RETURN r",
        ])
        .current_dir(&repo)
        .output()
        .expect("gql");
    assert!(
        gql.status.success(),
        "gql failed: {}",
        String::from_utf8_lossy(&gql.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&gql.stdout).unwrap();
    let rows = doc.get("rows").and_then(|v| v.as_array()).unwrap();
    assert!(
        !rows.is_empty(),
        "expected KantraRule nodes with konveyor.io/target=quarkus"
    );
}

#[test]
fn kantra_violates_edges_materialized_in_graph() {
    let _guard = ECOMMERCE_LOCK.lock().unwrap();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    if !repo.is_dir() {
        return;
    }
    let rules = rules_dir();
    let out = Command::new(rgctl_bin())
        .args([
            "discover",
            ".",
            "--languages",
            "java",
            "--with-kantra",
            "--kantra-rules",
            rules.to_str().unwrap(),
        ])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let gql = Command::new(rgctl_bin())
        .args([
            "-f",
            "json",
            "gql",
            "MATCH (r:KantraRule)-[:VIOLATES]->(n) RETURN n LIMIT 20",
        ])
        .current_dir(&repo)
        .output()
        .expect("gql");
    assert!(
        gql.status.success(),
        "gql failed: {}",
        String::from_utf8_lossy(&gql.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&gql.stdout).unwrap();
    let rows = doc.get("rows").and_then(|v| v.as_array()).unwrap();
    assert!(!rows.is_empty(), "expected VIOLATES edges in graph");
}

#[test]
fn kantra_findings_include_enrichment() {
    let _guard = ECOMMERCE_LOCK.lock().unwrap();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    if !repo.is_dir() {
        return;
    }
    let rules = rules_dir();
    let out = Command::new(rgctl_bin())
        .args([
            "discover",
            ".",
            "--languages",
            "java",
            "--with-kantra",
            "--kantra-rules",
            rules.to_str().unwrap(),
        ])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(repo.join(".rgctl/kantra_findings.json")).unwrap())
            .unwrap();
    let violations = doc.get("violations").and_then(|v| v.as_array()).unwrap();
    assert!(
        violations.iter().any(|v| {
            v.get("enrichment")
                .and_then(|e| e.get("community_id"))
                .is_some()
        }),
        "expected at least one violation with enrichment.community_id"
    );
}

#[test]
fn kantra_file_cache_records_hits_on_second_discover() {
    let _guard = TINY_REPO_LOCK.lock().unwrap();
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    if !repo.is_dir() {
        return;
    }
    let rules = rules_dir();
    let args = [
        "discover",
        ".",
        "-l",
        "java,rust",
        "--with-kantra",
        "--kantra-rules",
        rules.to_str().unwrap(),
    ];
    let first = Command::new(rgctl_bin())
        .args(args)
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = Command::new(rgctl_bin())
        .args(args)
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(
        repo.join(".rgctl/kantra_cache/manifest.json").is_file(),
        "expected kantra_cache manifest after discover"
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(repo.join(".rgctl/kantra_findings.json")).unwrap())
            .unwrap();
    let hits = doc.get("cache_hits").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(
        hits > 0,
        "expected cache_hits > 0 on second discover, got {hits}"
    );
}

#[test]
fn kantra_dashboard_export_writes_index() {
    let _guard = TINY_REPO_LOCK.lock().unwrap();
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    if !repo.is_dir() {
        return;
    }
    let rules = rules_dir();
    let out = Command::new(rgctl_bin())
        .args([
            "discover",
            ".",
            "-l",
            "java,rust",
            "--with-kantra",
            "--kantra-rules",
            rules.to_str().unwrap(),
            "--with-universe",
        ])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dash = repo.join(".rgctl/dashboard");
    assert!(
        dash.join("kantra_index.json").is_file(),
        "kantra_index.json missing from dashboard bundle"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dash.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["kantra"]["available"], true);
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dash.join("kantra_index.json")).unwrap()).unwrap();
    assert_eq!(index["available"], true);
    assert!(index.get("files").and_then(|v| v.as_array()).is_some());
}

#[test]
fn kantra_all_target_findings_include_targets_metadata() {
    let _guard = ECOMMERCE_LOCK.lock().unwrap();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    if !repo.is_dir() {
        return;
    }
    let out = Command::new(rgctl_bin())
        .args(["discover", ".", "--languages", "java", "--with-kantra"])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let findings_path = repo.join(".rgctl/kantra_findings.json");
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&findings_path).unwrap()).unwrap();
    let violations = doc.get("violations").and_then(|v| v.as_array()).unwrap();
    assert!(!violations.is_empty());
    let with_targets = violations
        .iter()
        .filter(|v| {
            v.get("targets")
                .and_then(|t| t.as_array())
                .is_some_and(|a| !a.is_empty())
        })
        .count();
    assert!(
        with_targets > 0,
        "expected violations to carry targets[] from rule labels"
    );
}

#[test]
fn rules_run_target_findings_and_targets_cli() {
    let _guard = ECOMMERCE_LOCK.lock().unwrap();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java");
    if !repo.is_dir() {
        return;
    }
    // Ensure a snapshot exists (discover without kantra is enough for rules run).
    let discover = Command::new(rgctl_bin())
        .args(["discover", ".", "--languages", "java"])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        discover.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&discover.stderr)
    );

    let run = Command::new(rgctl_bin())
        .args(["-f", "json", "rules", "run", "--target", "quarkus"])
        .current_dir(&repo)
        .output()
        .expect("rules run");
    assert!(
        run.status.success(),
        "rules run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_doc: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(
        run_doc.get("command").and_then(|v| v.as_str()),
        Some("rules_run")
    );
    assert!(run_doc.get("summary").is_some());
    assert!(run_doc.get("skipped_rules_count").is_some());
    assert!(
        run_doc.get("skipped_rules").is_none(),
        "rules_run JSON stdout should omit raw skipped_rules[] for token efficiency"
    );

    let findings = Command::new(rgctl_bin())
        .args([
            "-f", "json", "rules", "findings", "--target", "quarkus", "--limit", "5",
        ])
        .current_dir(&repo)
        .output()
        .expect("rules findings");
    assert!(
        findings.status.success(),
        "rules findings failed: {}",
        String::from_utf8_lossy(&findings.stderr)
    );
    let findings_doc: serde_json::Value = serde_json::from_slice(&findings.stdout).unwrap();
    assert_eq!(
        findings_doc.get("command").and_then(|v| v.as_str()),
        Some("rules_findings")
    );
    let per_rule = findings_doc
        .get("per_rule")
        .and_then(|v| v.as_array())
        .expect("per_rule array");
    let total_matched = findings_doc
        .get("total_matched")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let summary_violations = findings_doc
        .pointer("/summary/violations")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        summary_violations, total_matched,
        "summary.violations must reflect total_matched before --limit truncation"
    );
    let per_rule_sum: u64 = per_rule
        .iter()
        .filter_map(|r| r.get("count").and_then(|c| c.as_u64()))
        .sum();
    assert_eq!(
        per_rule_sum, total_matched,
        "per_rule counts must sum to total_matched before --limit truncation"
    );
    let returned = findings_doc
        .get("returned")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(returned <= 5);
    if let Some(first_v) = findings_doc
        .get("violations")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
    {
        assert!(
            first_v.get("message").is_none(),
            "individual violations should omit duplicate message (carried once in per_rule[])"
        );
    }

    let targets = Command::new(rgctl_bin())
        .args(["-f", "json", "rules", "targets"])
        .current_dir(&repo)
        .output()
        .expect("rules targets");
    assert!(
        targets.status.success(),
        "rules targets failed: {}",
        String::from_utf8_lossy(&targets.stderr)
    );
    let targets_doc: serde_json::Value = serde_json::from_slice(&targets.stdout).unwrap();
    assert_eq!(
        targets_doc.get("command").and_then(|v| v.as_str()),
        Some("rules_targets")
    );
    let target_list = targets_doc
        .get("targets")
        .and_then(|v| v.as_array())
        .unwrap();
    assert!(
        target_list
            .iter()
            .any(|t| t.get("name").and_then(|n| n.as_str()) == Some("quarkus")),
        "expected quarkus in rules targets"
    );
    assert_eq!(
        targets_doc
            .get("findings_present")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn kantra_jr2_to_jr7_and_deps_xml_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let rules = dir.path().join("rules");
    std::fs::create_dir_all(repo.join("src/main/java/com/acme")).unwrap();
    std::fs::create_dir_all(repo.join("src/main/webapp/WEB-INF")).unwrap();
    std::fs::create_dir_all(&rules).unwrap();

    // Maven pom.xml for java.dependency (colon vs dot + version bounds)
    std::fs::write(
        repo.join("pom.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId>
  <artifactId>demo</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>org.jboss.bom</groupId>
      <artifactId>eap-runtime-artifacts</artifactId>
      <version>7.4.0.Final</version>
    </dependency>
  </dependencies>
</project>
"#,
    )
    .unwrap();

    // XML descriptor for builtin.xml (rooted path + union)
    std::fs::write(
        repo.join("src/main/webapp/WEB-INF/jboss-web.xml"),
        r#"<?xml version="1.0"?>
<jboss-web>
  <context-root>/demo</context-root>
</jboss-web>
"#,
    )
    .unwrap();

    // Java file exercising JR2, JR3, JR4, JR5, JR6, JR7, FIELD_DECLARATION, RETURN_TYPE, THROWS, VARIABLE_DECLARATION
    std::fs::write(
        repo.join("src/main/java/com/acme/LegacyService.java"),
        r#"package com.acme;

import java.util.*;
import java.util.List;
import java.util.Scanner;
import java.net.URL;
import java.net.URLClassLoader;
import javax.servlet.http.*;
import javax.persistence.EntityManager;
import javax.persistence.Query;
import javax.persistence.PersistenceException;
import org.jboss.ejb3.annotation.SecurityDomain;

// Comment mentioning java.net.URL and org.jboss.ejb3 should never match
public class LegacyService {
    private EntityManager em;

    public Query handle(HttpSession session, EntityManager paramEm) throws PersistenceException {
        session.setAttribute("user", "alice");
        try {
            URL endpoint = new URL("https://example.com");
        } catch (Exception ignored) {
        }
        return null;
    }
}
"#,
    )
    .unwrap();

    std::fs::write(
        rules.join("ruleset.yaml"),
        "name: koncur-jr-suite\nlabels:\n  - konveyor.io/target=quarkus\n  - konveyor.io/source=java-ee\n",
    )
    .unwrap();
    std::fs::write(
        rules.join("rules.yaml"),
        r#"
- ruleID: jr2-ctor-url
  category: mandatory
  effort: 1
  message: URL constructor call
  when:
    java.referenced:
      location: CONSTRUCTOR_CALL
      pattern: java.net.URL*

- ruleID: jr2-pkg-ejb3
  category: mandatory
  effort: 1
  message: EJB3 package reference
  when:
    java.referenced:
      location: PACKAGE
      pattern: org.jboss.ejb3*

- ruleID: jr3-session-setattr
  category: mandatory
  effort: 3
  message: HttpSession.setAttribute call
  when:
    java.referenced:
      location: METHOD_CALL
      pattern: javax.servlet.http.HttpSession.setAttribute*

- ruleID: jr5-bare-classloader
  category: potential
  effort: 1
  message: Bare ClassLoader should not match URLClassLoader
  when:
    java.referenced:
      location: IMPORT
      pattern: ClassLoader

- ruleID: jr6-formatter-scanner
  category: potential
  effort: 1
  message: Scanner or Formatter import
  when:
    java.referenced:
      location: IMPORT
      pattern: java.util.(Formatter|Scanner)*

- ruleID: jr6-leading-star-em-field
  category: mandatory
  effort: 2
  message: Leading wildcard EntityManager field
  when:
    java.referenced:
      location: FIELD_DECLARATION
      pattern: "*EntityManager"

- ruleID: jr-return-type-query
  category: mandatory
  effort: 1
  message: Query return type
  when:
    java.referenced:
      location: RETURN_TYPE
      pattern: javax.persistence.Query*

- ruleID: jr-throws-persistence
  category: mandatory
  effort: 1
  message: Throws PersistenceException
  when:
    java.referenced:
      location: THROWS_METHOD_DECLARATION
      pattern: javax.persistence.PersistenceException*

- ruleID: jr-param-var-decl
  category: mandatory
  effort: 1
  message: Parameter EntityManager
  when:
    java.referenced:
      location: VARIABLE_DECLARATION
      pattern: javax.persistence.EntityManager*

- ruleID: jr7-tag-only-seam
  description: Tag-only rule must not emit violations
  tag:
    - Seam
  when:
    java.referenced:
      location: IMPORT
      pattern: java.util.*

- ruleID: dep-dot-coord
  category: mandatory
  effort: 1
  message: BOM dependency matched with dot coordinate and bounds
  when:
    java.dependency:
      name: org.jboss.bom.eap-runtime-artifacts
      lowerbound: "7.0"
      upperbound: "8.0"

- ruleID: xml-rooted-union
  category: mandatory
  effort: 1
  message: jboss-web context-root
  when:
    builtin.xml:
      xpath: /jboss-web/context-root | //class-loading

- ruleID: xml-unsupported-position
  category: mandatory
  effort: 1
  message: Unsupported XPath
  when:
    builtin.xml:
      xpath: //foo[position()=1]
"#,
    )
    .unwrap();

    let out = Command::new(rgctl_bin())
        .args([
            "discover",
            ".",
            "--languages",
            "java",
            "--with-kantra",
            "--kantra-rules",
            rules.to_str().unwrap(),
        ])
        .current_dir(&repo)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let findings_path = repo.join(".rgctl/kantra_findings.json");
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&findings_path).unwrap()).unwrap();
    let violations = doc.get("violations").and_then(|v| v.as_array()).unwrap();
    let skipped = doc.get("skipped_rules").and_then(|v| v.as_array()).unwrap();

    let by_rule = |id: &str| -> Vec<&serde_json::Value> {
        violations
            .iter()
            .filter(|v| v.get("rule_id").and_then(|r| r.as_str()) == Some(id))
            .collect()
    };

    // JR2 + JR4: CONSTRUCTOR_CALL hits exact `new URL(...)` line (21), single hit, high confidence.
    let ctor_hits = by_rule("jr2-ctor-url");
    assert_eq!(
        ctor_hits.len(),
        1,
        "expected 1 CONSTRUCTOR_CALL hit, got {ctor_hits:?}"
    );
    assert_eq!(ctor_hits[0].get("line").and_then(|l| l.as_u64()), Some(21));
    assert_eq!(
        ctor_hits[0].get("confidence").and_then(|c| c.as_str()),
        Some("high")
    );

    // JR2: PACKAGE matches import line (12), not comment (14).
    let pkg_hits = by_rule("jr2-pkg-ejb3");
    assert_eq!(
        pkg_hits.len(),
        1,
        "expected 1 PACKAGE hit, got {pkg_hits:?}"
    );
    assert_eq!(pkg_hits[0].get("line").and_then(|l| l.as_u64()), Some(12));

    // JR3 + JR4: METHOD_CALL on wildcard-imported HttpSession parameter hits exact line (19).
    let call_hits = by_rule("jr3-session-setattr");
    assert_eq!(
        call_hits.len(),
        1,
        "expected 1 METHOD_CALL hit, got {call_hits:?}"
    );
    assert_eq!(call_hits[0].get("line").and_then(|l| l.as_u64()), Some(19));

    // JR5: Bare `ClassLoader` must NOT match `import java.net.URLClassLoader;`
    assert!(
        by_rule("jr5-bare-classloader").is_empty(),
        "ClassLoader must not substring-match URLClassLoader"
    );

    // JR6: `java.util.(Formatter|Scanner)*` matches only `import java.util.Scanner;` (line 5),
    // NOT `import java.util.*;` (line 3) or `import java.util.List;` (line 4).
    let jr6_hits = by_rule("jr6-formatter-scanner");
    assert_eq!(
        jr6_hits.len(),
        1,
        "expected 1 Scanner hit, got {jr6_hits:?}"
    );
    assert_eq!(jr6_hits[0].get("line").and_then(|l| l.as_u64()), Some(5));

    // JR6 leading `*EntityManager` + FIELD_DECLARATION:
    assert_eq!(by_rule("jr6-leading-star-em-field").len(), 1);
    // RETURN_TYPE, THROWS_METHOD_DECLARATION, VARIABLE_DECLARATION:
    assert_eq!(by_rule("jr-return-type-query").len(), 1);
    assert_eq!(by_rule("jr-throws-persistence").len(), 1);
    assert_eq!(by_rule("jr-param-var-decl").len(), 1);

    // JR7: Tag-only rule must not appear in violations, and must be in skipped_rules.
    assert!(by_rule("jr7-tag-only-seam").is_empty());
    assert!(
        skipped.iter().any(|s| {
            s.get("rule_id").and_then(|r| r.as_str()) == Some("jr7-tag-only-seam")
                && s.get("reason")
                    .and_then(|r| r.as_str())
                    .is_some_and(|r| r.contains("tag-only"))
        }),
        "expected jr7-tag-only-seam in skipped_rules"
    );

    // #178: java.dependency dot coordinate + version bounds
    assert_eq!(by_rule("dep-dot-coord").len(), 1);

    // #178: builtin.xml rooted union + unsupported XPath skip
    assert_eq!(by_rule("xml-rooted-union").len(), 1);
    assert!(
        skipped.iter().any(|s| {
            s.get("rule_id").and_then(|r| r.as_str()) == Some("xml-unsupported-position")
        }),
        "expected xml-unsupported-position in skipped_rules"
    );
}
