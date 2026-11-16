//! `deps check` — match OSV package against manifests + optional bundled scans.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::adapters::{
    BundledScanOpts, DepHit, scan_jars, scan_node_modules, scan_repo_dependencies,
};
use crate::error::SecurityError;
use crate::osv::{OsvPackageTriage, OsvTriage, triage_osv_path};
use crate::version::engine_for_ecosystem;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DepsVerdict {
    NotAffected,
    AffectedCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepsMatch {
    pub ecosystem: String,
    pub name: String,
    pub version: Option<String>,
    pub provenance: String,
    pub in_range: bool,
    #[serde(default)]
    pub is_direct: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepsCheckResult {
    pub schema_version: u32,
    pub command: &'static str,
    pub osv_id: String,
    pub verdict: DepsVerdict,
    pub checked: Vec<String>,
    pub matched: Vec<DepsMatch>,
    pub triage: OsvTriage,
}

#[derive(Debug, Clone, Default)]
pub struct DepsCheckOpts {
    pub include_jars: Vec<PathBuf>,
    pub include_node_modules: Vec<PathBuf>,
    pub max_jars: Option<usize>,
    pub max_node_packages: Option<usize>,
}

/// Run dependency check for an OSV file against `repo`.
pub fn deps_check(
    repo: &Path,
    osv_path: &Path,
    opts: &DepsCheckOpts,
) -> Result<DepsCheckResult, SecurityError> {
    let triage = triage_osv_path(osv_path)?;
    deps_check_for_triage(repo, triage, opts)
}

/// Run dependency check using an already-parsed [`OsvTriage`] in a single pruned filesystem walk.
pub fn deps_check_for_triage(
    repo: &Path,
    triage: OsvTriage,
    opts: &DepsCheckOpts,
) -> Result<DepsCheckResult, SecurityError> {
    let mut checked = vec!["manifests".to_string(), "lockfiles".to_string()];
    let mut inventory = scan_repo_dependencies(repo)?;

    let mut bundled = BundledScanOpts::with_defaults();
    if let Some(n) = opts.max_jars {
        bundled.max_jars = n;
    }
    if let Some(n) = opts.max_node_packages {
        bundled.max_node_packages = n;
    }
    if !opts.include_jars.is_empty() {
        bundled.jar_roots = opts.include_jars.clone();
        checked.push("jars".into());
        inventory.extend(scan_jars(repo, &bundled)?);
    }
    if !opts.include_node_modules.is_empty() {
        bundled.node_modules_roots = opts.include_node_modules.clone();
        checked.push("node_modules".into());
        inventory.extend(scan_node_modules(repo, &bundled)?);
    }

    deps_check_against_inventory(triage, &inventory, checked)
}

/// Evaluate an already-parsed [`OsvTriage`] against a pre-scanned [`DepHit`] inventory
/// without walking the filesystem again (used by batch `vuln scan`).
pub fn deps_check_against_inventory(
    triage: OsvTriage,
    inventory: &[DepHit],
    checked: Vec<String>,
) -> Result<DepsCheckResult, SecurityError> {
    let mut matched = Vec::new();
    for pkg in &triage.packages {
        matched.extend(match_package(pkg, inventory)?);
    }

    let verdict = if matched.iter().any(|m| m.in_range) {
        DepsVerdict::AffectedCandidate
    } else {
        DepsVerdict::NotAffected
    };

    Ok(DepsCheckResult {
        schema_version: 1,
        command: "deps check",
        osv_id: triage.id.clone(),
        verdict,
        checked,
        matched,
        triage,
    })
}

fn is_direct_provenance(provenance: &str) -> bool {
    let fname = Path::new(provenance)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(provenance);
    matches!(
        fname,
        "pom.xml"
            | "Cargo.toml"
            | "package.json"
            | "go.mod"
            | "build.gradle"
            | "build.gradle.kts"
            | "vcpkg.json"
            | "requirements.txt"
    ) || fname.ends_with(".rockspec")
}

fn match_package(
    pkg: &OsvPackageTriage,
    inventory: &[DepHit],
) -> Result<Vec<DepsMatch>, SecurityError> {
    let engine = engine_for_ecosystem(&pkg.ecosystem)?;
    let target = normalize_name(&pkg.ecosystem, &pkg.name);
    let mut out = Vec::new();
    for hit in inventory {
        let hit_name = normalize_name(&hit.ecosystem, &hit.name);
        if !names_match(&target, &hit_name) {
            continue;
        }
        let direct = is_direct_provenance(&hit.provenance) && !hit.provenance.contains("jar:");
        let Some(ver) = hit.version.as_deref() else {
            out.push(DepsMatch {
                ecosystem: hit.ecosystem.clone(),
                name: hit.name.clone(),
                version: None,
                provenance: hit.provenance.clone(),
                in_range: false,
                is_direct: direct,
            });
            continue;
        };
        // Skip version ranges like ^1.0 in npm for membership honesty — treat as unknown
        if ver.starts_with('^') || ver.starts_with('~') || ver.contains('*') || ver.contains(',') {
            out.push(DepsMatch {
                ecosystem: hit.ecosystem.clone(),
                name: hit.name.clone(),
                version: Some(ver.to_string()),
                provenance: hit.provenance.clone(),
                in_range: false,
                is_direct: direct,
            });
            continue;
        }
        let in_range = engine.in_any_vulnerable_range(ver, &pkg.ranges, &pkg.listed_versions)?;
        out.push(DepsMatch {
            ecosystem: hit.ecosystem.clone(),
            name: hit.name.clone(),
            version: Some(ver.to_string()),
            provenance: hit.provenance.clone(),
            in_range,
            is_direct: direct,
        });
    }
    Ok(out)
}

fn normalize_name(ecosystem: &str, name: &str) -> String {
    let eco = ecosystem.to_ascii_lowercase();
    let n = name.trim().to_string();
    if eco.starts_with("maven") || eco == "gradle" {
        n.replace('/', ":")
    } else if eco.starts_with("puppet") {
        n.replace('/', "-")
    } else {
        n
    }
}

fn names_match(osv_name: &str, hit_name: &str) -> bool {
    osv_name.eq_ignore_ascii_case(hit_name)
        || osv_name.ends_with(&format!(":{hit_name}"))
        || hit_name.ends_with(&format!(":{osv_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    #[test]
    fn coolstore_shaped_jackson_candidate() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("pom.xml"),
            r#"<?xml version="1.0"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.redhat.coolstore</groupId>
  <artifactId>monolith</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>javax</groupId>
      <artifactId>javaee-api</artifactId>
      <version>7.0</version>
    </dependency>
  </dependencies>
</project>"#,
        )
        .unwrap();
        let lib = dir.path().join("lib");
        fs::create_dir_all(&lib).unwrap();
        let jar = lib.join("audit.jar");
        {
            let f = fs::File::create(&jar).unwrap();
            let mut zip = ZipWriter::new(f);
            zip.start_file(
                "META-INF/maven/com.enterprise/audit/pom.xml",
                SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(
                br#"<?xml version="1.0"?>
<project>
  <dependencies>
    <dependency>
      <groupId>com.fasterxml.jackson.core</groupId>
      <artifactId>jackson-databind</artifactId>
      <version>2.13.5</version>
    </dependency>
  </dependencies>
</project>"#,
            )
            .unwrap();
            zip.finish().unwrap();
        }
        let osv = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/osv/jackson-CVE-2022-42004.json");
        let opts = DepsCheckOpts {
            include_jars: vec![PathBuf::from("lib")],
            ..Default::default()
        };
        let r = deps_check(dir.path(), &osv, &opts).unwrap();
        assert_eq!(r.verdict, DepsVerdict::AffectedCandidate);
        assert!(r.matched.iter().any(|m| m.in_range));
    }

    #[test]
    fn xalan_not_affected() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("pom.xml"),
            r#"<?xml version="1.0"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>app</artifactId>
  <version>1.0.0</version>
</project>"#,
        )
        .unwrap();
        let osv = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/osv/xalan-CVE-2022-34169.json");
        let r = deps_check(dir.path(), &osv, &DepsCheckOpts::default()).unwrap();
        assert_eq!(r.verdict, DepsVerdict::NotAffected);
    }

    use std::fs;
}
