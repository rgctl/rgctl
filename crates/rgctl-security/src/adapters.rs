//! Bundled / vendored dependency scanners (opt-in; not on default discover).
//!
//! Sync CPU work — callers should use `spawn_blocking` from async contexts.

use rgctl_extraction::extract_declarations;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::error::SecurityError;

/// A dependency hit discovered from a manifest or bundled artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepHit {
    pub ecosystem: String,
    pub name: String,
    pub version: Option<String>,
    pub provenance: String,
}

const MANIFEST_NAMES: &[&str] = &[
    "pom.xml",
    "Cargo.toml",
    "package.json",
    "go.mod",
    "build.gradle",
    "build.gradle.kts",
];

fn should_descend(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 || !entry.file_type().is_dir() {
        return true;
    }
    let Some(name) = entry.file_name().to_str() else {
        return true;
    };
    !matches!(
        name,
        ".git"
            | ".rgctl"
            | ".rgctl-base"
            | "target"
            | "node_modules"
            | "vendor"
            | ".venv"
            | "venv"
            | "dist"
            | "build"
            | ".gradle"
            | ".next"
    )
}

fn collect_manifest_file(path: &Path, hits: &mut Vec<DepHit>) -> Result<(), SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let prov = path.display().to_string();
    for d in extract_declarations(path, &bytes) {
        hits.push(DepHit {
            ecosystem: d.ecosystem,
            name: d.name,
            version: d.version_requirement,
            provenance: prov.clone(),
        });
    }
    Ok(())
}

fn collect_lockfile(path: &Path, name: &str, hits: &mut Vec<DepHit>) {
    let provenance = path.display().to_string();
    let res = match name {
        "Cargo.lock" => parse_cargo_lock(path, &provenance),
        "go.sum" => parse_go_sum(path, &provenance),
        "gradle.lockfile" => parse_gradle_lockfile(path, &provenance),
        "Package.resolved" => parse_package_resolved(path, &provenance),
        "vcpkg.json" => parse_vcpkg_json(path, &provenance),
        "conan.lock" => parse_conan_lock(path, &provenance),
        "package-lock.json" => parse_package_lock_json(path, &provenance),
        "pnpm-lock.yaml" => parse_pnpm_lock_yaml(path, &provenance),
        "yarn.lock" => parse_yarn_lock(path, &provenance),
        "Pipfile.lock" => parse_pipfile_lock(path, &provenance),
        "uv.lock" | "poetry.lock" => parse_toml_pypi_lock(path, &provenance),
        "requirements.txt" => parse_requirements_txt(path, &provenance),
        "packages.lock.json" => parse_nuget_packages_lock(path, &provenance),
        "composer.lock" => parse_composer_lock(path, &provenance),
        "Gemfile.lock" => parse_gemfile_lock(path, &provenance),
        "pubspec.lock" => parse_pubspec_lock(path, &provenance),
        "mix.lock" => parse_mix_lock(path, &provenance),
        _ if name.eq_ignore_ascii_case("puppetfile.lock") => {
            parse_puppetfile_lock(path, &provenance)
        }
        _ if name.ends_with(".rockspec") => parse_rockspec(path, &provenance),
        _ => return,
    };
    if let Ok(parsed) = res {
        hits.extend(parsed);
    }
}

/// Single-pass repository scan collecting both declared manifests and pinned lockfiles,
/// pruning ignored build/vendor directories at `WalkDir` entry time.
pub fn scan_repo_dependencies(repo: &Path) -> Result<Vec<DepHit>, SecurityError> {
    let mut hits = Vec::new();
    for ent in WalkDir::new(repo)
        .into_iter()
        .filter_entry(should_descend)
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = ent.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if MANIFEST_NAMES.iter().any(|n| name.eq_ignore_ascii_case(n)) {
            collect_manifest_file(path, &mut hits)?;
        } else {
            collect_lockfile(path, name, &mut hits);
        }
    }
    Ok(hits)
}

/// Options for bundled scans (all opt-in).
#[derive(Debug, Clone, Default)]
pub struct BundledScanOpts {
    /// Glob roots for JARs/WARs (e.g. `lib`).
    pub jar_roots: Vec<PathBuf>,
    /// Scan `node_modules/*/package.json` under these roots.
    pub node_modules_roots: Vec<PathBuf>,
    pub max_jars: usize,
    pub max_node_packages: usize,
}

impl BundledScanOpts {
    pub fn with_defaults() -> Self {
        Self {
            jar_roots: Vec::new(),
            node_modules_roots: Vec::new(),
            max_jars: 500,
            max_node_packages: 2000,
        }
    }
}

/// Scan declared manifests under `repo` (pom.xml, Cargo.toml, package.json, go.mod, gradle).
pub fn scan_manifests(repo: &Path) -> Result<Vec<DepHit>, SecurityError> {
    let mut hits = Vec::new();
    for ent in WalkDir::new(repo)
        .into_iter()
        .filter_entry(should_descend)
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = ent.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if MANIFEST_NAMES.iter().any(|n| name.eq_ignore_ascii_case(n)) {
            collect_manifest_file(path, &mut hits)?;
        }
    }
    Ok(hits)
}

/// Scan JARs for embedded `META-INF/maven/**/pom.xml` dependencies.
pub fn scan_jars(repo: &Path, opts: &BundledScanOpts) -> Result<Vec<DepHit>, SecurityError> {
    let mut hits = Vec::new();
    let mut jar_count = 0usize;
    for root in &opts.jar_roots {
        let abs = if root.is_absolute() {
            root.clone()
        } else {
            repo.join(root)
        };
        if !abs.exists() {
            continue;
        }
        for ent in WalkDir::new(&abs).into_iter().filter_map(|e| e.ok()) {
            if jar_count >= opts.max_jars {
                break;
            }
            let path = ent.path();
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !ext.eq_ignore_ascii_case("jar") && !ext.eq_ignore_ascii_case("war") {
                continue;
            }
            jar_count += 1;
            hits.extend(scan_one_jar(path)?);
        }
    }
    Ok(hits)
}

fn scan_one_jar(path: &Path) -> Result<Vec<DepHit>, SecurityError> {
    let file = fs::File::open(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| SecurityError::Msg(format!("zip open {}: {e}", path.display())))?;
    let mut hits = Vec::new();
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();
    for name in names {
        if !(name.starts_with("META-INF/maven/") && name.ends_with("/pom.xml")) {
            continue;
        }
        let mut file = archive
            .by_name(&name)
            .map_err(|e| SecurityError::Msg(format!("zip entry {name}: {e}")))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| SecurityError::Io {
            path: format!("{}!{}", path.display(), name),
            source: e,
        })?;
        let virt_path = PathBuf::from(format!("{}!{}", path.display(), name));
        for d in extract_declarations(&virt_path, &buf) {
            hits.push(DepHit {
                ecosystem: d.ecosystem,
                name: d.name,
                version: d.version_requirement,
                provenance: format!("{}!{}", path.display(), name),
            });
        }
    }
    Ok(hits)
}

/// Scan `node_modules/<pkg>/package.json` for name/version (honesty: direct children only).
pub fn scan_node_modules(
    repo: &Path,
    opts: &BundledScanOpts,
) -> Result<Vec<DepHit>, SecurityError> {
    let mut hits = Vec::new();
    let mut count = 0usize;
    for root in &opts.node_modules_roots {
        let nm = if root.is_absolute() {
            root.join("node_modules")
        } else {
            repo.join(root).join("node_modules")
        };
        if !nm.is_dir() {
            continue;
        }
        for ent in fs::read_dir(&nm).map_err(|e| SecurityError::Io {
            path: nm.display().to_string(),
            source: e,
        })? {
            if count >= opts.max_node_packages {
                break;
            }
            let ent = ent.map_err(|e| SecurityError::Io {
                path: nm.display().to_string(),
                source: e,
            })?;
            let pkg_json = ent.path().join("package.json");
            if !pkg_json.is_file() {
                // scoped packages @scope/name
                if ent.path().is_dir() && ent.file_name().to_string_lossy().starts_with('@') {
                    for sub in fs::read_dir(ent.path()).into_iter().flatten().flatten() {
                        if count >= opts.max_node_packages {
                            break;
                        }
                        let pj = sub.path().join("package.json");
                        if pj.is_file() {
                            count += 1;
                            if let Some(h) = read_npm_package_json(&pj)? {
                                hits.push(h);
                            }
                        }
                    }
                }
                continue;
            }
            count += 1;
            if let Some(h) = read_npm_package_json(&pkg_json)? {
                hits.push(h);
            }
        }
    }
    Ok(hits)
}

fn read_npm_package_json(path: &Path) -> Result<Option<DepHit>, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| SecurityError::Msg(format!("package.json {}: {e}", path.display())))?;
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("");
    if name.is_empty() {
        return Ok(None);
    }
    let version = v
        .get("version")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    Ok(Some(DepHit {
        ecosystem: "npm".into(),
        name: name.to_string(),
        version,
        provenance: path.display().to_string(),
    }))
}

/// Scan lockfiles under `repo` for pinned dependency versions (Wave A + Wave B).
///
/// Prunes build/vendor directories at entry time and isolates per-file parse errors so a
/// single malformed lockfile does not discard valid lockfiles elsewhere in the repository.
pub fn scan_lockfiles(repo: &Path) -> Result<Vec<DepHit>, SecurityError> {
    let mut hits = Vec::new();
    for ent in WalkDir::new(repo)
        .into_iter()
        .filter_entry(should_descend)
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = ent.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        collect_lockfile(path, name, &mut hits);
    }
    Ok(hits)
}

/// Parse `Cargo.lock` (TOML v3 format with `[[package]]` entries).
fn parse_cargo_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    // Simple line-by-line parser (avoids a toml dependency in the security crate).
    // Cargo.lock format: `[[package]]\nname = "foo"\nversion = "1.0.0"\n`
    let mut cur_name: Option<String> = None;
    let mut cur_version: Option<String> = None;
    for line in content.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            // Flush previous package.
            if let Some(name) = cur_name.take() {
                hits.push(DepHit {
                    ecosystem: "crates.io".into(),
                    name,
                    version: cur_version.take(),
                    provenance: provenance.to_string(),
                });
            }
            cur_version = None;
        } else if let Some(rest) = line.strip_prefix("name = ") {
            cur_name = Some(rest.trim_matches('"').to_string());
        } else if let Some(rest) = line.strip_prefix("version = ") {
            cur_version = Some(rest.trim_matches('"').to_string());
        }
    }
    // Flush last package.
    if let Some(name) = cur_name {
        hits.push(DepHit {
            ecosystem: "crates.io".into(),
            name,
            version: cur_version,
            provenance: provenance.to_string(),
        });
    }
    Ok(hits)
}

/// Parse `go.sum` lines into `(module, version)` pinned deps.
///
/// `go.sum` format: `<module> <version>[/go.mod] <hash>`
/// We deduplicate by `(module, version)` (strip `/go.mod` suffix).
fn parse_go_sum(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut seen = std::collections::HashSet::new();
    let mut hits = Vec::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let module = parts[0];
        let ver_raw = parts[1];
        // Strip `/go.mod` suffix for the actual module version.
        let version = ver_raw.split('/').next().unwrap_or(ver_raw);
        // Deduplicate: go.sum has two entries per dep (source + go.mod hash).
        let key = format!("{module}@{version}");
        if seen.insert(key) {
            hits.push(DepHit {
                ecosystem: "Go".into(),
                name: module.to_string(),
                version: Some(version.to_string()),
                provenance: provenance.to_string(),
            });
        }
    }
    Ok(hits)
}

/// Parse `gradle.lockfile` (Gradle dependency lock format).
///
/// Format: `<group>:<artifact>:<version>=<config1>,<config2>`
fn parse_gradle_lockfile(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `group:artifact:version=configs`
        let entry = line.split('=').next().unwrap_or(line);
        let parts: Vec<&str> = entry.splitn(3, ':').collect();
        if parts.len() == 3 {
            let gav = format!("{}:{}", parts[0], parts[1]);
            hits.push(DepHit {
                ecosystem: "Maven".into(),
                name: gav,
                version: Some(parts[2].to_string()),
                provenance: provenance.to_string(),
            });
        }
    }
    Ok(hits)
}

/// Parse `Package.resolved` (SwiftPM JSON lockfile).
///
/// v1: `object.pins[].repositoryURL` + `object.pins[].state.version`
/// v2: `pins[].location` + `pins[].state.version`
fn parse_package_resolved(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| SecurityError::Msg(e.to_string()))?;
    let mut hits = Vec::new();
    // Try v2 format: `{"pins": [...]}`
    let pins = v
        .get("pins")
        .and_then(|p| p.as_array())
        // Fallback to v1 format: `{"object": {"pins": [...]}}`
        .or_else(|| v.pointer("/object/pins").and_then(|p| p.as_array()));
    if let Some(pins) = pins {
        for pin in pins {
            let name = pin
                .get("identity")
                .or_else(|| pin.get("package"))
                .and_then(|x| x.as_str())
                .or_else(|| {
                    // Derive from URL: last path component without .git
                    pin.get("location")
                        .or_else(|| pin.get("repositoryURL"))
                        .and_then(|x| x.as_str())
                        .and_then(|url| url.rsplit('/').next())
                        .map(|s| s.trim_end_matches(".git"))
                })
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let version = pin
                .pointer("/state/version")
                .and_then(|x| x.as_str())
                .map(str::to_string);
            hits.push(DepHit {
                ecosystem: "Swift".into(),
                name: name.clone(),
                version: version.clone(),
                provenance: provenance.to_string(),
            });
            // Also emit canonical host/owner/repo coordinate (`github.com/org/Repo`) for SwiftURL OSV matching.
            if let Some(loc) = pin
                .get("location")
                .or_else(|| pin.get("repositoryURL"))
                .and_then(|x| x.as_str())
            {
                let norm_url = loc
                    .trim_end_matches('/')
                    .trim_end_matches(".git")
                    .strip_prefix("https://")
                    .or_else(|| loc.strip_prefix("http://"))
                    .unwrap_or(loc)
                    .trim_end_matches(".git")
                    .to_string();
                if !norm_url.is_empty() && norm_url != name {
                    hits.push(DepHit {
                        ecosystem: "SwiftURL".into(),
                        name: norm_url,
                        version,
                        provenance: provenance.to_string(),
                    });
                }
            }
        }
    }
    Ok(hits)
}

/// Parse `vcpkg.json` for declared port dependencies and pinned `overrides[]` versions.
fn parse_vcpkg_json(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| SecurityError::Msg(e.to_string()))?;
    let mut overrides: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for ov in v
        .get("overrides")
        .and_then(|o| o.as_array())
        .into_iter()
        .flatten()
    {
        if let (Some(n), Some(ver)) = (
            ov.get("name").and_then(|x| x.as_str()),
            ov.get("version")
                .or_else(|| ov.get("version-semver"))
                .or_else(|| ov.get("version-string"))
                .and_then(|x| x.as_str()),
        ) {
            let clean_ver = ver.split('#').next().unwrap_or(ver).to_string();
            overrides.insert(n.to_string(), clean_ver);
        }
    }

    let mut hits = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(deps) = v.get("dependencies").and_then(|d| d.as_array()) {
        for dep in deps {
            let (name, inline_ver) = match dep {
                serde_json::Value::String(s) => (s.clone(), None),
                serde_json::Value::Object(o) => {
                    let n = o
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let ver = o
                        .get("version")
                        .or_else(|| o.get("version>="))
                        .and_then(|x| x.as_str())
                        .map(|s| s.split('#').next().unwrap_or(s).to_string());
                    (n, ver)
                }
                _ => continue,
            };
            if name.is_empty() {
                continue;
            }
            let version = overrides.get(&name).cloned().or(inline_ver);
            seen.insert(name.clone());
            hits.push(DepHit {
                ecosystem: "vcpkg".into(),
                name,
                version,
                provenance: provenance.to_string(),
            });
        }
    }
    for (ov_name, ov_ver) in overrides {
        if seen.insert(ov_name.clone()) {
            hits.push(DepHit {
                ecosystem: "vcpkg".into(),
                name: ov_name,
                version: Some(ov_ver),
                provenance: provenance.to_string(),
            });
        }
    }
    Ok(hits)
}

// --- Wave B lockfile parsers (task 4.1) ---

/// Parse `package-lock.json` v1/v2/v3 (npm), including nested `node_modules/a/node_modules/b` entries.
fn parse_package_lock_json(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    fn collect_v1_deps(
        deps: &serde_json::Map<String, serde_json::Value>,
        provenance: &str,
        hits: &mut Vec<DepHit>,
    ) {
        for (name, dep_val) in deps {
            let version = dep_val
                .get("version")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            hits.push(DepHit {
                ecosystem: "npm".into(),
                name: name.clone(),
                version,
                provenance: provenance.to_string(),
            });
            if let Some(nested) = dep_val.get("dependencies").and_then(|d| d.as_object()) {
                collect_v1_deps(nested, provenance, hits);
            }
        }
    }

    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| SecurityError::Msg(e.to_string()))?;
    let mut hits = Vec::new();
    // v2/v3: `packages` object (`"node_modules/foo"` or `"node_modules/a/node_modules/@scope/bar"`).
    if let Some(pkgs) = v.get("packages").and_then(|p| p.as_object()) {
        for (key, pkg_val) in pkgs {
            if key.is_empty() {
                continue; // Skip root ""
            }
            let name = pkg_val
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    key.rsplit("node_modules/")
                        .next()
                        .unwrap_or(key.as_str())
                        .trim_matches('/')
                        .to_string()
                });
            if name.is_empty() {
                continue;
            }
            let version = pkg_val
                .get("version")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            hits.push(DepHit {
                ecosystem: "npm".into(),
                name,
                version,
                provenance: provenance.to_string(),
            });
        }
    } else if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
        collect_v1_deps(deps, provenance, &mut hits);
    }
    Ok(hits)
}

/// Parse `pnpm-lock.yaml` (v5/v6/v9) for pinned package versions, handling scoped packages
/// and peer-dependency suffixes such as `/foo@1.2.3(react@18.2.0):` or `'@scope/foo@1.2.3':`.
fn parse_pnpm_lock_yaml(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    let mut in_packages = false;
    for line in content.lines() {
        let trimmed_end = line.trim_end();
        if trimmed_end == "packages:" || trimmed_end == "snapshots:" {
            in_packages = true;
            continue;
        }
        if in_packages {
            if !trimmed_end.starts_with(' ')
                && !trimmed_end.starts_with('\t')
                && !trimmed_end.is_empty()
            {
                in_packages = false;
                continue;
            }
            // Package keys under `packages:` / `snapshots:` sit at 2-space indent and end with `:`.
            if !(trimmed_end.starts_with("  ")
                && !trimmed_end.starts_with("   ")
                && trimmed_end.ends_with(':'))
            {
                continue;
            }
            let raw_key = trimmed_end[2..]
                .trim_end_matches(':')
                .trim()
                .trim_matches(['\'', '"'])
                .trim_start_matches('/');
            // Strip peer-dep suffixes `(peer@ver)` or `_peer@ver` before splitting `name@version`.
            let without_peers = raw_key.split('(').next().unwrap_or(raw_key);
            let split_at = if let Some(rest) = without_peers.strip_prefix('@') {
                rest.find('@').map(|idx| idx + 1)
            } else {
                without_peers.find('@')
            };
            if let Some(at_pos) = split_at {
                let name = &without_peers[..at_pos];
                let ver_rest = &without_peers[at_pos + 1..];
                let version = ver_rest.split(['/', '_']).next().unwrap_or(ver_rest);
                if !name.is_empty() && !version.is_empty() {
                    hits.push(DepHit {
                        ecosystem: "npm".into(),
                        name: name.to_string(),
                        version: Some(version.to_string()),
                        provenance: provenance.to_string(),
                    });
                }
            }
        }
    }
    Ok(hits)
}

/// Parse `yarn.lock` v1/Berry for pinned package versions.
///
/// v1: blocks starting with `"name@range":` followed by `  version "X.Y.Z"`.
/// Berry: JSON-like in YAML format, also with `version:` lines.
fn parse_yarn_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    let mut cur_names: Vec<String> = Vec::new();
    for line in content.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        // Block header: `"name@range", "name@range2":` or `name@range:`
        if !line.starts_with(' ') && !line.starts_with('\t') && line.ends_with(':') {
            cur_names.clear();
            let header = line.trim_end_matches(':');
            for part in header.split(", ") {
                let part = part.trim_matches('"');
                // Extract name before the first `@` (but keep scoped `@scope/name`).
                let name = if let Some(rest) = part.strip_prefix('@') {
                    // scoped: @scope/name@version — strip leading @, find next @
                    let at = rest.find('@').unwrap_or(rest.len());
                    format!("@{}", &rest[..at])
                } else {
                    let at = part.find('@').unwrap_or(part.len());
                    part[..at].to_string()
                };
                if !name.is_empty() && !cur_names.contains(&name) {
                    cur_names.push(name);
                }
            }
        } else if line.starts_with("  version") || line.starts_with("\tversion") {
            // `  version "X.Y.Z"` or `  version: X.Y.Z`
            let ver = line
                .trim()
                .strip_prefix("version")
                .map(|s| s.trim().trim_matches(':').trim().trim_matches('"'))
                .unwrap_or("")
                .to_string();
            if !ver.is_empty() {
                for name in &cur_names {
                    hits.push(DepHit {
                        ecosystem: "npm".into(),
                        name: name.clone(),
                        version: Some(ver.clone()),
                        provenance: provenance.to_string(),
                    });
                }
                cur_names.clear();
            }
        }
    }
    Ok(hits)
}

/// Parse `Pipfile.lock` (Python/PyPI).
///
/// Format: `{"default": {"package": {"version": "==1.2.3"}}, "develop": {...}}`
fn parse_pipfile_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| SecurityError::Msg(e.to_string()))?;
    let mut hits = Vec::new();
    for section in ["default", "develop"] {
        if let Some(pkgs) = v.get(section).and_then(|p| p.as_object()) {
            for (name, pkg_val) in pkgs {
                // Version is `"==X.Y.Z"` — strip the `==` prefix.
                let version = pkg_val
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim_start_matches("==").to_string());
                hits.push(DepHit {
                    ecosystem: "PyPI".into(),
                    name: name.to_ascii_lowercase().replace('_', "-"),
                    version,
                    provenance: provenance.to_string(),
                });
            }
        }
    }
    Ok(hits)
}

/// Parse TOML-based Python lockfiles: `uv.lock` and `poetry.lock`.
///
/// Both use `[[package]]` with `name` and `version` fields.
fn parse_toml_pypi_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    let mut cur_name: Option<String> = None;
    let mut cur_version: Option<String> = None;
    for line in content.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            if let Some(name) = cur_name.take() {
                hits.push(DepHit {
                    ecosystem: "PyPI".into(),
                    name,
                    version: cur_version.take(),
                    provenance: provenance.to_string(),
                });
            }
            cur_version = None;
        } else if let Some(rest) = line.strip_prefix("name = ") {
            cur_name = Some(
                rest.trim_matches('"')
                    .to_ascii_lowercase()
                    .replace('_', "-"),
            );
        } else if let Some(rest) = line.strip_prefix("version = ") {
            cur_version = Some(rest.trim_matches('"').to_string());
        }
    }
    if let Some(name) = cur_name {
        hits.push(DepHit {
            ecosystem: "PyPI".into(),
            name,
            version: cur_version,
            provenance: provenance.to_string(),
        });
    }
    Ok(hits)
}

/// Parse pinned `requirements.txt` lines (lines with `==` pin).
fn parse_requirements_txt(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        // Only pinned lines: `name==X.Y.Z` or `name == X.Y.Z`
        if let Some(idx) = line.find("==") {
            let name = line[..idx].trim().to_ascii_lowercase().replace('_', "-");
            let version = line[idx + 2..].trim().to_string();
            if !name.is_empty() && !version.is_empty() {
                hits.push(DepHit {
                    ecosystem: "PyPI".into(),
                    name,
                    version: Some(version),
                    provenance: provenance.to_string(),
                });
            }
        }
    }
    Ok(hits)
}

/// Parse `packages.lock.json` (NuGet / C#).
///
/// Format: `{"dependencies": {"TargetFramework": {"PackageName": {"resolved": "X.Y.Z"}}}}`
fn parse_nuget_packages_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| SecurityError::Msg(e.to_string()))?;
    let mut hits = Vec::new();
    if let Some(frameworks) = v.get("dependencies").and_then(|d| d.as_object()) {
        for (_framework, pkgs) in frameworks {
            if let Some(pkgs) = pkgs.as_object() {
                for (name, pkg_val) in pkgs {
                    let version = pkg_val
                        .get("resolved")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    hits.push(DepHit {
                        ecosystem: "NuGet".into(),
                        name: name.clone(),
                        version,
                        provenance: provenance.to_string(),
                    });
                }
            }
        }
    }
    Ok(hits)
}

/// Parse `composer.lock` (PHP/Packagist).
fn parse_composer_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| SecurityError::Msg(e.to_string()))?;
    let mut hits = Vec::new();
    for section in ["packages", "packages-dev"] {
        if let Some(pkgs) = v.get(section).and_then(|p| p.as_array()) {
            for pkg in pkgs {
                let name = pkg
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let version = pkg
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(|v| v.trim_start_matches('v').to_string());
                if !name.is_empty() {
                    hits.push(DepHit {
                        ecosystem: "Packagist".into(),
                        name,
                        version,
                        provenance: provenance.to_string(),
                    });
                }
            }
        }
    }
    Ok(hits)
}

/// Parse `Gemfile.lock` (Ruby/RubyGems).
///
/// Format: `GEM\n  remote: ...\n  specs:\n    name (version)\n`
fn parse_gemfile_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    let mut in_specs = false;
    for line in content.lines() {
        if line.trim() == "specs:" {
            in_specs = true;
            continue;
        }
        if !line.starts_with(' ') && !line.is_empty() {
            in_specs = false;
        }
        if in_specs {
            // `    name (version)`
            let trimmed = line.trim();
            if let Some(paren) = trimmed.find('(') {
                let name = trimmed[..paren].trim().to_string();
                let ver_part = &trimmed[paren + 1..];
                let version = ver_part.trim_end_matches(')').to_string();
                if !name.is_empty() && !version.is_empty() {
                    hits.push(DepHit {
                        ecosystem: "RubyGems".into(),
                        name,
                        version: Some(version),
                        provenance: provenance.to_string(),
                    });
                }
            }
        }
    }
    Ok(hits)
}

/// Parse `pubspec.lock` (Dart/Flutter).
///
/// Supports both standard `packages:\n  pkg_name:\n    version: "X.Y.Z"` (2-space indent)
/// and top-level `pkg_name:\n  version: "X.Y.Z"`.
fn parse_pubspec_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    let mut cur_name: Option<String> = None;
    let mut in_packages = false;
    for line in content.lines() {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() || trimmed_end.trim_start().starts_with('#') {
            continue;
        }
        let indent = trimmed_end.len() - trimmed_end.trim_start().len();
        let trimmed = trimmed_end.trim();
        if indent == 0 && trimmed.ends_with(':') {
            let key = trimmed.trim_end_matches(':').trim();
            if key == "packages" {
                in_packages = true;
                cur_name = None;
            } else if key == "sdks" {
                in_packages = false;
                cur_name = None;
            } else {
                in_packages = false;
                cur_name = Some(key.to_string());
            }
            continue;
        }
        if in_packages && indent == 2 && trimmed.ends_with(':') {
            cur_name = Some(trimmed.trim_end_matches(':').trim().to_string());
            continue;
        }
        if let Some(name) = &cur_name
            && let Some(rest) = trimmed.strip_prefix("version:")
        {
            let version = rest.trim().trim_matches(['"', '\'']).to_string();
            hits.push(DepHit {
                ecosystem: "Pub".into(),
                name: name.clone(),
                version: Some(version),
                provenance: provenance.to_string(),
            });
            cur_name = None;
        }
    }
    Ok(hits)
}

/// Parse `mix.lock` (Elixir/Hex).
///
/// Format: `%{"name": {:hex, :name, "version", ...}}`
fn parse_mix_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('"') {
            // `"name": {:hex, :name, "version", ...}`
            if let Some(close) = line.find('"').and_then(|_| {
                let rest = &line[1..];
                rest.find('"').map(|i| i + 1)
            }) {
                let name = line[1..close].to_string();
                // Find version in the tuple: typically 3rd element `"X.Y.Z"`
                if let Some(hex_pos) = line.find(":hex") {
                    let after_hex = &line[hex_pos..];
                    // Count commas to find the version string.
                    let parts: Vec<&str> = after_hex.splitn(4, ',').collect();
                    if parts.len() >= 3 {
                        let ver_part = parts[2].trim().trim_matches('"');
                        if !ver_part.is_empty() {
                            hits.push(DepHit {
                                ecosystem: "Hex".into(),
                                name,
                                version: Some(ver_part.to_string()),
                                provenance: provenance.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(hits)
}

/// Parse `Puppetfile.lock` (Puppet Forge).
///
/// Format: `FORGE\n  remote: ...\n  specs:\n    name (version)\n`
/// (Similar to Gemfile.lock structure.)
fn parse_puppetfile_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    let mut in_specs = false;
    for line in content.lines() {
        if line.trim() == "DEPENDENCIES" || line.trim() == "GRAPH" {
            in_specs = false;
        }
        if line.trim() == "specs:" {
            in_specs = true;
            continue;
        }
        if in_specs {
            let trimmed = line.trim();
            if let Some(paren) = trimmed.find('(') {
                let name = trimmed[..paren].trim().to_string();
                let ver_part = &trimmed[paren + 1..];
                let version = ver_part.trim_end_matches(')').to_string();
                if !name.is_empty() {
                    hits.push(DepHit {
                        ecosystem: "Puppet Forge".into(),
                        name,
                        version: Some(version),
                        provenance: provenance.to_string(),
                    });
                }
            }
        }
    }
    Ok(hits)
}

/// Parse `*.rockspec` (Lua/LuaRocks) for root `package` and `dependencies = { ... }` entries.
fn parse_rockspec(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let content = fs::read_to_string(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut hits = Vec::new();
    let mut name = String::new();
    let mut version = String::new();
    let mut in_deps = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("package =") {
            name = rest.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
        } else if let Some(rest) = line.strip_prefix("version =") {
            let raw = rest.trim().trim_matches(['"', '\'']);
            version = raw.split('-').next().unwrap_or(raw).to_string();
        } else if line.starts_with("dependencies") && line.contains('{') {
            in_deps = true;
            if line.contains('}') {
                in_deps = false;
            }
        } else if in_deps {
            if line.starts_with('}') {
                in_deps = false;
                continue;
            }
            let spec = line.trim_end_matches(',').trim().trim_matches(['"', '\'']);
            if spec.is_empty() || spec.starts_with("--") {
                continue;
            }
            if let Some((pkg_part, ver_part)) = spec.split_once("==") {
                let dep_name = pkg_part.trim().to_ascii_lowercase();
                let dep_ver = ver_part
                    .trim()
                    .split('-')
                    .next()
                    .unwrap_or(ver_part.trim())
                    .to_string();
                if !dep_name.is_empty() && dep_name != "lua" {
                    hits.push(DepHit {
                        ecosystem: "LuaRocks".into(),
                        name: dep_name,
                        version: (!dep_ver.is_empty()).then_some(dep_ver),
                        provenance: provenance.to_string(),
                    });
                }
            }
        }
    }
    if !name.is_empty() {
        hits.push(DepHit {
            ecosystem: "LuaRocks".into(),
            name,
            version: (!version.is_empty()).then_some(version),
            provenance: provenance.to_string(),
        });
    }
    Ok(hits)
}

/// Parse `conan.lock` (Conan C/C++).
///
/// JSON format: `{"graph_lock": {"nodes": {"0": {"ref": "name/version@...", ...}}}}`
fn parse_conan_lock(path: &Path, provenance: &str) -> Result<Vec<DepHit>, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| SecurityError::Msg(e.to_string()))?;
    let mut hits = Vec::new();
    if let Some(nodes) = v.pointer("/graph_lock/nodes").and_then(|n| n.as_object()) {
        for (_idx, node) in nodes {
            if let Some(ref_str) = node.get("ref").and_then(|r| r.as_str()) {
                let coord = ref_str.split('@').next().unwrap_or(ref_str);
                let parts: Vec<&str> = coord.splitn(2, '/').collect();
                if parts.len() == 2 {
                    hits.push(DepHit {
                        ecosystem: "Conan".into(),
                        name: parts[0].to_string(),
                        version: Some(parts[1].to_string()),
                        provenance: provenance.to_string(),
                    });
                }
            }
        }
    } else if let Some(requires) = v.get("requires").and_then(|r| r.as_array()) {
        for req in requires {
            if let Some(ref_str) = req.as_str() {
                let coord = ref_str.split('@').next().unwrap_or(ref_str);
                let parts: Vec<&str> = coord.splitn(2, '/').collect();
                if parts.len() == 2 {
                    hits.push(DepHit {
                        ecosystem: "Conan".into(),
                        name: parts[0].to_string(),
                        version: Some(parts[1].to_string()),
                        provenance: provenance.to_string(),
                    });
                }
            }
        }
    }

    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    #[test]
    fn jar_embedded_pom_jackson() {
        let dir = tempdir().unwrap();
        let jar = dir.path().join("audit.jar");
        {
            let f = fs::File::create(&jar).unwrap();
            let mut zip = ZipWriter::new(f);
            let opts = SimpleFileOptions::default();
            zip.start_file("META-INF/maven/com.enterprise/audit/pom.xml", opts)
                .unwrap();
            zip.write_all(
                br#"<?xml version="1.0"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.enterprise</groupId>
  <artifactId>audit</artifactId>
  <version>1.0.0</version>
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
        let mut opts = BundledScanOpts::with_defaults();
        opts.jar_roots = vec![dir.path().to_path_buf()];
        let hits = scan_jars(dir.path(), &opts).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name.contains("jackson-databind")
                    && h.version.as_deref() == Some("2.13.5")),
            "{hits:?}"
        );
    }

    #[test]
    fn node_modules_package() {
        let dir = tempdir().unwrap();
        let pkg = dir.path().join("node_modules").join("lodash");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(
            pkg.join("package.json"),
            r#"{"name":"lodash","version":"4.17.21"}"#,
        )
        .unwrap();
        let mut opts = BundledScanOpts::with_defaults();
        opts.node_modules_roots = vec![PathBuf::from(".")];
        let hits = scan_node_modules(dir.path(), &opts).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "lodash");
        assert_eq!(hits[0].version.as_deref(), Some("4.17.21"));
    }

    // Task 3.1: Lockfile parser tests.

    #[test]
    fn cargo_lock_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.lock"),
            r#"# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "serde"
version = "1.0.189"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "tokio"
version = "1.32.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#,
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "serde" && h.version.as_deref() == Some("1.0.189")),
            "{hits:?}"
        );
        assert!(
            hits.iter()
                .any(|h| h.name == "tokio" && h.version.as_deref() == Some("1.32.0")),
            "{hits:?}"
        );
        assert!(hits.iter().all(|h| h.ecosystem == "crates.io"));
    }

    #[test]
    fn go_sum_parsed_and_deduped() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("go.sum"),
            "golang.org/x/net v0.1.0 h1:abc=\ngolang.org/x/net v0.1.0/go.mod h1:def=\ngithub.com/pkg/errors v0.9.1 h1:xyz=\n",
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        // go.sum has two entries per dep; both should be deduplicated to one hit each.
        let net_count = hits.iter().filter(|h| h.name == "golang.org/x/net").count();
        assert_eq!(net_count, 1, "dedup failed: {hits:?}");
        let err_count = hits
            .iter()
            .filter(|h| h.name == "github.com/pkg/errors")
            .count();
        assert_eq!(err_count, 1);
        assert!(hits.iter().all(|h| h.ecosystem == "Go"));
    }

    #[test]
    fn gradle_lockfile_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("gradle.lockfile"),
            "# This is a Gradle lock file\ncom.fasterxml.jackson.core:jackson-databind:2.13.5=compileClasspath\norg.springframework:spring-core:5.3.29=runtimeClasspath\n",
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "com.fasterxml.jackson.core:jackson-databind"
                    && h.version.as_deref() == Some("2.13.5")),
            "{hits:?}"
        );
        assert!(
            hits.iter()
                .any(|h| h.name == "org.springframework:spring-core"
                    && h.version.as_deref() == Some("5.3.29")),
            "{hits:?}"
        );
    }

    #[test]
    fn package_resolved_v2_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Package.resolved"),
            r#"{"version": 2, "pins": [{"identity": "swift-argument-parser", "location": "https://github.com/apple/swift-argument-parser.git", "state": {"version": "1.2.3"}}]}"#,
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter().any(|h| h.name == "swift-argument-parser"
                && h.version.as_deref() == Some("1.2.3")),
            "{hits:?}"
        );
    }

    #[test]
    fn vcpkg_json_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("vcpkg.json"),
            r#"{"name": "myapp", "version": "1.0", "dependencies": ["openssl", "zlib", {"name": "fmt"}]}"#,
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(hits.iter().any(|h| h.name == "openssl"), "{hits:?}");
        assert!(hits.iter().any(|h| h.name == "zlib"), "{hits:?}");
        assert!(hits.iter().any(|h| h.name == "fmt"), "{hits:?}");
    }

    // Wave B tests (task 4.1).

    #[test]
    fn package_lock_json_v2_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("package-lock.json"),
            r#"{"name": "app", "lockfileVersion": 2, "packages": {"": {"name": "app"}, "node_modules/express": {"version": "4.18.2"}, "node_modules/@types/node": {"version": "18.0.0"}}}"#,
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "express" && h.version.as_deref() == Some("4.18.2")),
            "{hits:?}"
        );
        assert!(
            hits.iter()
                .any(|h| h.name == "@types/node" && h.version.as_deref() == Some("18.0.0")),
            "{hits:?}"
        );
    }

    #[test]
    fn yarn_lock_v1_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("yarn.lock"),
            "# yarn lockfile v1\n\nexpress@^4.18.0:\n  version \"4.18.2\"\n  resolved \"https://registry.yarnpkg.com/express/-/express-4.18.2.tgz\"\n",
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "express" && h.version.as_deref() == Some("4.18.2")),
            "{hits:?}"
        );
    }

    #[test]
    fn pipfile_lock_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Pipfile.lock"),
            r#"{"default": {"requests": {"version": "==2.28.2"}, "urllib3": {"version": "==1.26.15"}}, "develop": {}}"#,
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "requests" && h.version.as_deref() == Some("2.28.2")),
            "{hits:?}"
        );
        assert!(
            hits.iter()
                .any(|h| h.name == "urllib3" && h.version.as_deref() == Some("1.26.15")),
            "{hits:?}"
        );
    }

    #[test]
    fn requirements_txt_pinned_lines_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("requirements.txt"),
            "# comment\nrequests==2.28.2\nflask>=1.0  # unpinned, skip\nurllib3==1.26.15\n",
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "requests" && h.version.as_deref() == Some("2.28.2")),
            "{hits:?}"
        );
        assert!(
            !hits.iter().any(|h| h.name == "flask"),
            "unpinned flask should be skipped"
        );
    }

    #[test]
    fn composer_lock_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("composer.lock"),
            r#"{"packages": [{"name": "symfony/console", "version": "v6.3.0"}], "packages-dev": []}"#,
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "symfony/console" && h.version.as_deref() == Some("6.3.0")),
            "{hits:?}"
        );
    }

    #[test]
    fn gemfile_lock_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Gemfile.lock"),
            "GEM\n  remote: https://rubygems.org/\n  specs:\n    rails (7.0.6)\n    activesupport (7.0.6)\n\nPLATFORMS\n",
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "rails" && h.version.as_deref() == Some("7.0.6")),
            "{hits:?}"
        );
    }

    #[test]
    fn nuget_packages_lock_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("packages.lock.json"),
            r#"{"version": 1, "dependencies": {"net6.0": {"Newtonsoft.Json": {"resolved": "13.0.3"}, "Microsoft.Extensions.Logging": {"resolved": "7.0.0"}}}}"#,
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "Newtonsoft.Json" && h.version.as_deref() == Some("13.0.3")),
            "{hits:?}"
        );
    }

    #[test]
    fn poetry_lock_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("poetry.lock"),
            "# poetry lock\n\n[[package]]\nname = \"requests\"\nversion = \"2.28.2\"\n\n[[package]]\nname = \"urllib3\"\nversion = \"1.26.15\"\n",
        )
        .unwrap();
        let hits = scan_lockfiles(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "requests" && h.version.as_deref() == Some("2.28.2")),
            "{hits:?}"
        );
    }

    #[test]
    fn nested_package_lock_and_pnpm_v6_v9_peer_suffixes_parsed() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("package-lock.json"),
            r#"{
              "name": "app",
              "lockfileVersion": 3,
              "packages": {
                "": {"name": "app"},
                "node_modules/express/node_modules/qs": {"version": "6.5.2"},
                "node_modules/foo/node_modules/@babel/traverse": {"version": "7.23.0"}
              }
            }"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n\npackages:\n  /vite@4.5.0(@types/node@20.0.0):\n    resolution: {integrity: sha512-x}\n  '@scope/pkg@1.2.3':\n    resolution: {integrity: sha512-y}\n",
        )
        .unwrap();
        // Malformed sibling lockfile must not discard valid lockfiles!
        let bad_dir = dir.path().join("sub");
        fs::create_dir_all(&bad_dir).unwrap();
        fs::write(bad_dir.join("composer.lock"), b"{not valid json").unwrap();

        let hits = scan_repo_dependencies(dir.path()).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.name == "qs" && h.version.as_deref() == Some("6.5.2")),
            "{hits:?}"
        );
        assert!(
            hits.iter()
                .any(|h| h.name == "@babel/traverse" && h.version.as_deref() == Some("7.23.0")),
            "{hits:?}"
        );
        assert!(
            hits.iter()
                .any(|h| h.name == "vite" && h.version.as_deref() == Some("4.5.0")),
            "{hits:?}"
        );
        assert!(
            hits.iter()
                .any(|h| h.name == "@scope/pkg" && h.version.as_deref() == Some("1.2.3")),
            "{hits:?}"
        );
    }
}
