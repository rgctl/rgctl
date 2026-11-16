//! Networked OSV feed fetching (`api.osv.dev`), local advisory overrides,
//! upstream scanner report parsers (Trivy, Grype, OSV-Scanner), and CycloneDX/SPDX SBOM ingestion.
//!
//! Policy: never invoked on `rgctl discover`. Any local override (`--osv`, `--from-report`,
//! `--offline`) strictly bypasses outbound HTTP traffic.

use crate::adapters::{
    BundledScanOpts, DepHit, scan_jars, scan_node_modules, scan_repo_dependencies,
};
use crate::error::SecurityError;
use crate::osv::{OsvTriage, triage_osv_bytes, triage_osv_path};
use crate::package_resolve::resolve_package;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

/// Default upstream OSV API base URL.
pub const DEFAULT_OSV_API_URL: &str = "https://api.osv.dev";
/// Environment variable overriding [`DEFAULT_OSV_API_URL`].
pub const OSV_API_URL_ENV: &str = "RGCTL_OSV_API_URL";
/// Maximum package queries per `/v1/querybatch` request per OSV API limits.
pub const MAX_QUERYBATCH_SIZE: usize = 1000;
/// Default relative cache directory under `{repo}/.rgctl/`.
pub const OSV_CACHE_REL: &str = "osv";

/// Options for [`fetch_osv_for_repo`] and [`fetch_osv_for_inventory`].
#[derive(Debug, Clone)]
pub struct OsvFetchOpts {
    /// OSV API base URL (defaults to `RGCTL_OSV_API_URL` or [`DEFAULT_OSV_API_URL`]).
    pub api_url: String,
    /// Directory where hydrated `{id}.json` files are written.
    pub output_dir: PathBuf,
    /// Per-request HTTP timeout in seconds.
    pub timeout_secs: u64,
    /// Opt-in JAR roots when scanning repo dependencies.
    pub include_jars: Vec<PathBuf>,
    /// Opt-in `node_modules` roots when scanning repo dependencies.
    pub include_node_modules: Vec<PathBuf>,
    /// Optional CycloneDX / SPDX SBOM path to include in queried dependencies.
    pub sbom: Option<PathBuf>,
}

impl OsvFetchOpts {
    /// Construct defaults for `repo`.
    pub fn for_repo(repo: &Path, api_url: Option<String>, output_dir: Option<PathBuf>) -> Self {
        let resolved_url = api_url
            .or_else(|| std::env::var(OSV_API_URL_ENV).ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OSV_API_URL.to_string());
        let out =
            output_dir.unwrap_or_else(|| rgctl_graph::paths::artifact_path(repo, OSV_CACHE_REL));
        Self {
            api_url: resolved_url,
            output_dir: out,
            timeout_secs: 15,
            include_jars: Vec::new(),
            include_node_modules: Vec::new(),
            sbom: None,
        }
    }
}

/// Structured JSON summary returned by `rgctl vuln fetch`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OsvFetchSummary {
    pub schema_version: u32,
    pub command: &'static str,
    pub api_url: String,
    pub output_dir: String,
    pub packages_queried: usize,
    pub advisories_matched: usize,
    pub advisory_ids: Vec<String>,
    pub cached_files: Vec<String>,
}

/// One loaded OSV advisory (either from disk, hydrated cache, or synthesized from a scanner report).
#[derive(Debug, Clone)]
pub struct LoadedAdvisory {
    /// Advisory identifier (e.g. `GHSA-...` or `CVE-...`).
    pub id: String,
    /// Pre-parsed [`OsvTriage`] envelope.
    pub triage: OsvTriage,
    /// Provenance path or report label.
    pub source_label: String,
}

/// Unified advisory & inventory resolution options for `vuln scan` / batch `vuln analyze`.
#[derive(Debug, Clone, Default)]
pub struct AdvisorySourceOpts {
    /// Explicit local OSV file or directory override (`--osv`).
    pub osv: Option<PathBuf>,
    /// Explicit upstream scanner report override (`--from-report`: Trivy, Grype, OSV-Scanner JSON).
    pub from_report: Option<PathBuf>,
    /// Optional CycloneDX or SPDX JSON SBOM (`--sbom`).
    pub sbom: Option<PathBuf>,
    /// Never perform network calls; use local overrides or existing `.rgctl/osv/` cache (`--offline`).
    pub offline: bool,
    /// Optional OSV API URL override (`--osv-api-url`).
    pub api_url: Option<String>,
    /// Opt-in JAR roots.
    pub include_jars: Vec<PathBuf>,
    /// Opt-in `node_modules` roots.
    pub include_node_modules: Vec<PathBuf>,
}

/// Build the full dependency inventory for `repo` (manifests + lockfiles + optional JARs,
/// `node_modules`, and CycloneDX/SPDX SBOM).
pub fn collect_full_inventory(
    repo: &Path,
    include_jars: &[PathBuf],
    include_node_modules: &[PathBuf],
    sbom: Option<&Path>,
) -> Result<Vec<DepHit>, SecurityError> {
    let mut inventory = scan_repo_dependencies(repo)?;
    let mut bundled = BundledScanOpts::with_defaults();
    if !include_jars.is_empty() {
        bundled.jar_roots = include_jars.to_vec();
        inventory.extend(scan_jars(repo, &bundled)?);
    }
    if !include_node_modules.is_empty() {
        bundled.node_modules_roots = include_node_modules.to_vec();
        inventory.extend(scan_node_modules(repo, &bundled)?);
    }
    if let Some(sbom_path) = sbom {
        inventory.extend(parse_sbom_file(sbom_path)?);
    }
    Ok(inventory)
}

/// Map internal ecosystem strings to canonical OSV API ecosystem identifiers.
pub fn to_osv_api_ecosystem(label: &str) -> Option<&'static str> {
    let l = label.trim().to_ascii_lowercase();
    if l.starts_with("maven") || l == "gradle" {
        Some("Maven")
    } else if l == "crates.io" || l == "cargo" || l == "rust" {
        Some("crates.io")
    } else if l == "npm" || l == "javascript" {
        Some("npm")
    } else if l == "go" || l == "golang" {
        Some("Go")
    } else if l == "pypi" || l == "python" {
        Some("PyPI")
    } else if l.starts_with("nuget") || l == "dotnet" || l == "csharp" {
        Some("NuGet")
    } else if l == "packagist" || l == "composer" || l == "php" {
        Some("Packagist")
    } else if l == "rubygems" || l == "gem" || l == "ruby" {
        Some("RubyGems")
    } else if l == "pub" || l == "dart" {
        Some("Pub")
    } else if l == "hex" || l == "elixir" {
        Some("Hex")
    } else if l == "swift" || l == "swifturl" || l == "swiftpm" {
        Some("SwiftURL")
    } else if l == "conan" || l == "conancenter" {
        Some("ConanCenter")
    } else {
        None
    }
}

fn is_exact_pinned_version(ver: &str) -> bool {
    let v = ver.trim();
    !v.is_empty()
        && !v.starts_with('^')
        && !v.starts_with('~')
        && !v.contains('*')
        && !v.contains(',')
        && !v.contains("${")
        && !v.contains('>')
        && !v.contains('<')
}

fn sanitize_advisory_filename(id: &str) -> String {
    let clean: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{clean}.json")
}

/// Query `POST {api_url}/v1/querybatch` for `repo` dependencies and hydrate full OSV records.
pub fn fetch_osv_for_repo(
    repo: &Path,
    opts: &OsvFetchOpts,
) -> Result<OsvFetchSummary, SecurityError> {
    let inventory = collect_full_inventory(
        repo,
        &opts.include_jars,
        &opts.include_node_modules,
        opts.sbom.as_deref(),
    )?;
    fetch_osv_for_inventory(&inventory, opts)
}

/// Query `POST {api_url}/v1/querybatch` (chunked at `<= 1000`) and hydrate `GET /v1/vulns/{id}`
/// into `opts.output_dir`.
pub fn fetch_osv_for_inventory(
    inventory: &[DepHit],
    opts: &OsvFetchOpts,
) -> Result<OsvFetchSummary, SecurityError> {
    fs::create_dir_all(&opts.output_dir).map_err(|e| SecurityError::Io {
        path: opts.output_dir.display().to_string(),
        source: e,
    })?;

    // Deduplicate `(osv_eco, name, version)` queries deterministically.
    let mut unique_queries: BTreeSet<(String, String, String)> = BTreeSet::new();
    for hit in inventory {
        let Some(osv_eco) = to_osv_api_ecosystem(&hit.ecosystem) else {
            continue;
        };
        let Some(ver) = hit
            .version
            .as_deref()
            .filter(|v| is_exact_pinned_version(v))
        else {
            continue;
        };
        let norm_name = if osv_eco == "Maven" {
            hit.name.replace('/', ":")
        } else {
            hit.name.clone()
        };
        unique_queries.insert((osv_eco.to_string(), norm_name, ver.trim().to_string()));
    }

    let queries_vec: Vec<(String, String, String)> = unique_queries.into_iter().collect();
    let packages_queried = queries_vec.len();
    let base = opts.api_url.trim_end_matches('/');

    if packages_queried == 0 {
        return Ok(OsvFetchSummary {
            schema_version: 1,
            command: "vuln fetch",
            api_url: opts.api_url.clone(),
            output_dir: opts.output_dir.display().to_string(),
            packages_queried: 0,
            advisories_matched: 0,
            advisory_ids: Vec::new(),
            cached_files: Vec::new(),
        });
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(opts.timeout_secs.max(1)))
        .user_agent(concat!("rgctl/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| SecurityError::Msg(format!("HTTP client build failed: {e}")))?;

    let mut matched_ids: BTreeSet<String> = BTreeSet::new();
    for chunk in queries_vec.chunks(MAX_QUERYBATCH_SIZE) {
        let payload = json!({
            "queries": chunk.iter().map(|(eco, name, ver)| {
                json!({
                    "package": {
                        "name": name,
                        "ecosystem": eco,
                    },
                    "version": ver,
                })
            }).collect::<Vec<_>>()
        });

        let batch_url = format!("{base}/v1/querybatch");
        let resp = client
            .post(&batch_url)
            .json(&payload)
            .send()
            .map_err(|e| SecurityError::Msg(format!("POST {batch_url} failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(SecurityError::Msg(format!(
                "POST {batch_url} returned HTTP {}",
                resp.status()
            )));
        }
        let body: Value = resp
            .json()
            .map_err(|e| SecurityError::Msg(format!("decode {batch_url} JSON: {e}")))?;

        for res in body
            .get("results")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
        {
            for vuln in res
                .get("vulns")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(id) = vuln.get("id").and_then(|i| i.as_str())
                    && !id.is_empty()
                {
                    matched_ids.insert(id.to_string());
                }
            }
        }
    }

    let mut cached_files = Vec::with_capacity(matched_ids.len());
    for id in &matched_ids {
        let dest = opts.output_dir.join(sanitize_advisory_filename(id));
        let vuln_url = format!("{base}/v1/vulns/{id}");
        let resp = client
            .get(&vuln_url)
            .send()
            .map_err(|e| SecurityError::Msg(format!("GET {vuln_url} failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(SecurityError::Msg(format!(
                "GET {vuln_url} returned HTTP {}",
                resp.status()
            )));
        }
        let bytes = resp
            .bytes()
            .map_err(|e| SecurityError::Msg(format!("read {vuln_url} body: {e}")))?;
        // Validate OSV schema before persisting.
        let _ = triage_osv_bytes(&bytes)?;
        fs::write(&dest, &bytes).map_err(|e| SecurityError::Io {
            path: dest.display().to_string(),
            source: e,
        })?;
        cached_files.push(dest.display().to_string());
    }

    let advisory_ids: Vec<String> = matched_ids.into_iter().collect();
    Ok(OsvFetchSummary {
        schema_version: 1,
        command: "vuln fetch",
        api_url: opts.api_url.clone(),
        output_dir: opts.output_dir.display().to_string(),
        packages_queried,
        advisories_matched: advisory_ids.len(),
        advisory_ids,
        cached_files,
    })
}

/// Resolve candidate advisories + dependency inventory honoring strict local override precedence:
///
/// 1. `--from-report <path>` → parse Trivy / Grype / OSV-Scanner JSON locally (0 HTTP calls).
/// 2. `--osv <file|dir>` → parse local OSV file or directory of `*.json` files (0 HTTP calls).
/// 3. `--offline` → load cached `*.json` files from `{repo}/.rgctl/osv/` (0 HTTP calls).
/// 4. Default → fetch matching OSVs via [`fetch_osv_for_inventory`] into `{repo}/.rgctl/osv/` and load them.
pub fn resolve_advisories_and_inventory(
    repo: &Path,
    opts: &AdvisorySourceOpts,
) -> Result<(Vec<LoadedAdvisory>, Vec<DepHit>), SecurityError> {
    let mut inventory = collect_full_inventory(
        repo,
        &opts.include_jars,
        &opts.include_node_modules,
        opts.sbom.as_deref(),
    )?;

    // Override 1: Upstream scanner report (`--from-report`).
    if let Some(report_path) = &opts.from_report {
        let (advisories, report_deps) = parse_scanner_report(report_path)?;
        for dep in report_deps {
            if !inventory
                .iter()
                .any(|h| h.name == dep.name && h.version == dep.version)
            {
                inventory.push(dep);
            }
        }
        return Ok((advisories, inventory));
    }

    // Override 2: Local `--osv <file|dir>`.
    if let Some(osv_path) = &opts.osv {
        let advisories = load_local_osv_path(osv_path)?;
        return Ok((advisories, inventory));
    }

    // Override 3: `--offline` reads existing `.rgctl/osv/` cache with zero network traffic.
    let cache_dir = rgctl_graph::paths::artifact_path(repo, OSV_CACHE_REL);
    if opts.offline {
        if !cache_dir.is_dir() {
            return Ok((Vec::new(), inventory));
        }
        let advisories = load_local_osv_path(&cache_dir)?;
        return Ok((advisories, inventory));
    }

    // Default network path: fetch matching advisories for `inventory`, then load only those matched files.
    let fetch_opts = OsvFetchOpts::for_repo(repo, opts.api_url.clone(), Some(cache_dir));
    let summary = fetch_osv_for_inventory(&inventory, &fetch_opts)?;
    let mut advisories = Vec::with_capacity(summary.cached_files.len());
    for file in &summary.cached_files {
        let path = Path::new(file);
        let triage = triage_osv_path(path)?;
        advisories.push(LoadedAdvisory {
            id: triage.id.clone(),
            triage,
            source_label: file.clone(),
        });
    }
    advisories.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((advisories, inventory))
}

/// Load a single OSV JSON file or all `*.json` OSV files inside a directory (sorted by ID).
pub fn load_local_osv_path(path: &Path) -> Result<Vec<LoadedAdvisory>, SecurityError> {
    if path.is_file() {
        let triage = triage_osv_path(path)?;
        return Ok(vec![LoadedAdvisory {
            id: triage.id.clone(),
            triage,
            source_label: path.display().to_string(),
        }]);
    }
    if !path.is_dir() {
        return Err(SecurityError::Msg(format!(
            "OSV path does not exist: {}",
            path.display()
        )));
    }
    let mut files: Vec<PathBuf> = WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    files.sort();

    let mut by_id: BTreeMap<String, LoadedAdvisory> = BTreeMap::new();
    for file in files {
        if let Ok(triage) = triage_osv_path(&file) {
            by_id.insert(
                triage.id.clone(),
                LoadedAdvisory {
                    id: triage.id.clone(),
                    triage,
                    source_label: file.display().to_string(),
                },
            );
        }
    }
    Ok(by_id.into_values().collect())
}

/// Parse CycloneDX (`components[]`) or SPDX (`packages[]`) JSON SBOMs into [`DepHit`] records.
pub fn parse_sbom_file(path: &Path) -> Result<Vec<DepHit>, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: Value = serde_json::from_slice(&bytes)
        .map_err(|e| SecurityError::Msg(format!("SBOM JSON {}: {e}", path.display())))?;
    let prov = path.display().to_string();
    let mut hits = Vec::new();

    // CycloneDX: `components[]`
    if let Some(components) = v.get("components").and_then(|c| c.as_array()) {
        for comp in components {
            if let Some(purl) = comp.get("purl").and_then(|p| p.as_str())
                && let Some(hit) = dep_hit_from_purl(purl, &prov)
            {
                hits.push(hit);
                continue;
            }
            let group = comp.get("group").and_then(|g| g.as_str()).unwrap_or("");
            let name = comp.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let version = comp
                .get("version")
                .and_then(|ver| ver.as_str())
                .map(str::to_string);
            if !name.is_empty() {
                let (eco, coord) = if !group.is_empty() {
                    ("Maven".to_string(), format!("{group}:{name}"))
                } else {
                    ("npm".to_string(), name.to_string())
                };
                hits.push(DepHit {
                    ecosystem: eco,
                    name: coord,
                    version,
                    provenance: prov.clone(),
                });
            }
        }
    }

    // SPDX 2.x JSON: `packages[]`
    if let Some(packages) = v.get("packages").and_then(|p| p.as_array()) {
        for pkg in packages {
            let mut purl_hit = None;
            for ext in pkg
                .get("externalRefs")
                .and_then(|r| r.as_array())
                .into_iter()
                .flatten()
            {
                if ext.get("referenceType").and_then(|t| t.as_str()) == Some("purl")
                    && let Some(loc) = ext.get("referenceLocator").and_then(|l| l.as_str())
                {
                    purl_hit = dep_hit_from_purl(loc, &prov);
                    if purl_hit.is_some() {
                        break;
                    }
                }
            }
            if let Some(hit) = purl_hit {
                hits.push(hit);
                continue;
            }
            let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let version = pkg
                .get("versionInfo")
                .and_then(|ver| ver.as_str())
                .map(str::to_string);
            if !name.is_empty() && version.is_some() {
                if let Ok(res) = resolve_package(name) {
                    hits.push(DepHit {
                        ecosystem: res.ecosystem,
                        name: res.name,
                        version,
                        provenance: prov.clone(),
                    });
                }
            }
        }
    }

    Ok(hits)
}

fn dep_hit_from_purl(purl: &str, provenance: &str) -> Option<DepHit> {
    let no_qual = purl.split(['?', '#']).next().unwrap_or(purl);
    let version = no_qual
        .rsplit_once('@')
        .and_then(|(lhs, ver)| (!lhs.is_empty() && !ver.is_empty()).then(|| ver.to_string()));
    let res = resolve_package(no_qual).ok()?;
    Some(DepHit {
        ecosystem: res.ecosystem,
        name: res.name,
        version,
        provenance: provenance.to_string(),
    })
}

/// Parse Trivy (`Results[].Vulnerabilities[]`), Grype (`matches[]`), or OSV-Scanner
/// (`results[].packages[].vulnerabilities[]`) JSON reports into normalized [`LoadedAdvisory`]
/// and [`DepHit`] records without network access.
pub fn parse_scanner_report(
    path: &Path,
) -> Result<(Vec<LoadedAdvisory>, Vec<DepHit>), SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let v: Value = serde_json::from_slice(&bytes)
        .map_err(|e| SecurityError::Msg(format!("scanner report {}: {e}", path.display())))?;
    let prov = path.display().to_string();
    let mut advisories: BTreeMap<String, LoadedAdvisory> = BTreeMap::new();
    let mut deps = Vec::new();

    // 1. Trivy JSON: `Results[].Vulnerabilities[]`
    if let Some(results) = v.get("Results").and_then(|r| r.as_array()) {
        for res in results {
            let target_type = res.get("Type").and_then(|t| t.as_str()).unwrap_or("");
            let eco = trivy_type_to_ecosystem(target_type);
            for vuln in res
                .get("Vulnerabilities")
                .and_then(|x| x.as_array())
                .into_iter()
                .flatten()
            {
                let id = vuln
                    .get("VulnerabilityID")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let pkg_name = vuln.get("PkgName").and_then(|s| s.as_str()).unwrap_or("");
                if id.is_empty() || pkg_name.is_empty() {
                    continue;
                }
                let installed = vuln
                    .get("InstalledVersion")
                    .and_then(|s| s.as_str())
                    .map(str::to_string);
                let fixed = vuln.get("FixedVersion").and_then(|s| s.as_str());
                let resolved_eco = vuln
                    .pointer("/PkgIdentifier/PURL")
                    .and_then(|p| p.as_str())
                    .and_then(|p| dep_hit_from_purl(p, &prov))
                    .map(|h| h.ecosystem)
                    .unwrap_or_else(|| eco.to_string());

                deps.push(DepHit {
                    ecosystem: resolved_eco.clone(),
                    name: pkg_name.to_string(),
                    version: installed.clone(),
                    provenance: prov.clone(),
                });
                if let Some(loaded) = synthesize_report_advisory(
                    id,
                    &resolved_eco,
                    pkg_name,
                    installed.as_deref(),
                    fixed,
                    vuln.get("Title").and_then(|s| s.as_str()),
                    &prov,
                ) {
                    advisories.entry(id.to_string()).or_insert(loaded);
                }
            }
        }
    }

    // 2. Grype JSON: `matches[]`
    if let Some(matches) = v.get("matches").and_then(|m| m.as_array()) {
        for m in matches {
            let id = m
                .pointer("/vulnerability/id")
                .and_then(|s| s.as_str())
                .unwrap_or("");
            let pkg_name = m
                .pointer("/artifact/name")
                .and_then(|s| s.as_str())
                .unwrap_or("");
            if id.is_empty() || pkg_name.is_empty() {
                continue;
            }
            let installed = m
                .pointer("/artifact/version")
                .and_then(|s| s.as_str())
                .map(str::to_string);
            let art_type = m
                .pointer("/artifact/type")
                .and_then(|s| s.as_str())
                .unwrap_or("");
            let purl_hit = m
                .pointer("/artifact/purl")
                .and_then(|p| p.as_str())
                .and_then(|p| dep_hit_from_purl(p, &prov));
            let (eco, coord_name) = match purl_hit {
                Some(h) => (h.ecosystem, h.name),
                None => (
                    trivy_type_to_ecosystem(art_type).to_string(),
                    pkg_name.to_string(),
                ),
            };
            let fixed = m
                .pointer("/vulnerability/fix/versions/0")
                .and_then(|s| s.as_str());
            deps.push(DepHit {
                ecosystem: eco.clone(),
                name: coord_name.clone(),
                version: installed.clone(),
                provenance: prov.clone(),
            });
            if let Some(loaded) = synthesize_report_advisory(
                id,
                &eco,
                &coord_name,
                installed.as_deref(),
                fixed,
                m.pointer("/vulnerability/description")
                    .and_then(|s| s.as_str()),
                &prov,
            ) {
                advisories.entry(id.to_string()).or_insert(loaded);
            }
        }
    }

    // 3. OSV-Scanner JSON: `results[].packages[].vulnerabilities[]` (full OSV records embedded!)
    if let Some(results) = v.get("results").and_then(|r| r.as_array()) {
        for r in results {
            for pkg_entry in r
                .get("packages")
                .and_then(|p| p.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(pkg_obj) = pkg_entry.get("package") {
                    let name = pkg_obj.get("name").and_then(|s| s.as_str()).unwrap_or("");
                    let eco = pkg_obj
                        .get("ecosystem")
                        .and_then(|s| s.as_str())
                        .unwrap_or("");
                    let ver = pkg_obj
                        .get("version")
                        .and_then(|s| s.as_str())
                        .map(str::to_string);
                    if !name.is_empty() && !eco.is_empty() {
                        deps.push(DepHit {
                            ecosystem: eco.to_string(),
                            name: name.to_string(),
                            version: ver,
                            provenance: prov.clone(),
                        });
                    }
                }
                for vuln_val in pkg_entry
                    .get("vulnerabilities")
                    .and_then(|vs| vs.as_array())
                    .into_iter()
                    .flatten()
                {
                    if let Ok(raw) = serde_json::to_vec(vuln_val)
                        && let Ok(triage) = triage_osv_bytes(&raw)
                    {
                        advisories
                            .entry(triage.id.clone())
                            .or_insert(LoadedAdvisory {
                                id: triage.id.clone(),
                                triage,
                                source_label: prov.clone(),
                            });
                    }
                }
            }
        }
    }

    Ok((advisories.into_values().collect(), deps))
}

fn trivy_type_to_ecosystem(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pom" | "jar" | "gradle" | "java-archive" | "maven" => "Maven",
        "cargo" | "rust-crate" | "crates.io" => "crates.io",
        "npm" | "yarn" | "pnpm" | "node-pkg" => "npm",
        "gomod" | "gobinary" | "go-module" | "go" => "Go",
        "pip" | "pipenv" | "poetry" | "uv" | "python-pkg" | "pypi" => "PyPI",
        "nuget" | "dotnet-core" | "packages.lock.json" => "NuGet",
        "composer" | "php-composer" | "packagist" => "Packagist",
        "bundler" | "gemspec" | "gem" | "rubygems" => "RubyGems",
        "pub" | "dart-pub" => "Pub",
        "hex" | "mix" => "Hex",
        "swift" | "cocoapods" => "Swift",
        "conan" => "Conan",
        _ => "Maven",
    }
}

fn synthesize_report_advisory(
    id: &str,
    ecosystem: &str,
    pkg_name: &str,
    installed_version: Option<&str>,
    fixed_version: Option<&str>,
    summary: Option<&str>,
    source_label: &str,
) -> Option<LoadedAdvisory> {
    let osv_eco = to_osv_api_ecosystem(ecosystem).unwrap_or("Maven");
    let first_fixed = fixed_version
        .map(|f| f.split([',', ' ']).next().unwrap_or(f).trim())
        .filter(|f| !f.is_empty());
    let events = match first_fixed {
        Some(fix) => json!([{"introduced": "0"}, {"fixed": fix}]),
        None => json!([{"introduced": "0"}]),
    };
    let listed = installed_version
        .map(|v| vec![v.to_string()])
        .unwrap_or_default();
    let doc = json!({
        "id": id,
        "modified": "2025-01-01T00:00:00Z",
        "summary": summary.unwrap_or(""),
        "affected": [{
            "package": {
                "ecosystem": osv_eco,
                "name": pkg_name,
            },
            "ranges": [{
                "type": "ECOSYSTEM",
                "events": events,
            }],
            "versions": listed,
        }]
    });
    let bytes = serde_json::to_vec(&doc).ok()?;
    let triage = triage_osv_bytes(&bytes).ok()?;
    Some(LoadedAdvisory {
        id: triage.id.clone(),
        triage,
        source_label: source_label.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::{Path as AxumPath, State},
        routing::{get, post},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tempfile::tempdir;

    #[derive(Clone)]
    struct MockOsvState {
        hits: Arc<AtomicUsize>,
    }

    async fn handle_querybatch(
        State(st): State<MockOsvState>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        st.hits.fetch_add(1, Ordering::SeqCst);
        let queries = body["queries"].as_array().cloned().unwrap_or_default();
        let results: Vec<Value> = queries
            .iter()
            .map(|q| {
                let name = q["package"]["name"].as_str().unwrap_or("");
                if name.contains("jackson-databind") {
                    json!({ "vulns": [{ "id": "GHSA-mock-jackson-0001" }] })
                } else {
                    json!({ "vulns": [] })
                }
            })
            .collect();
        Json(json!({ "results": results }))
    }

    async fn handle_vuln_by_id(
        State(st): State<MockOsvState>,
        AxumPath(id): AxumPath<String>,
    ) -> Json<Value> {
        st.hits.fetch_add(1, Ordering::SeqCst);
        Json(json!({
            "id": id,
            "modified": "2025-01-01T00:00:00Z",
            "summary": "Mock Jackson deserialization vulnerability",
            "affected": [{
                "package": {
                    "ecosystem": "Maven",
                    "name": "com.fasterxml.jackson.core:jackson-databind"
                },
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [{"introduced": "2.0.0"}, {"fixed": "2.14.0"}]
                }],
                "ecosystem_specific": {
                    "affected_methods": ["com.fasterxml.jackson.databind.ObjectMapper.readValue"]
                }
            }]
        }))
    }

    fn spawn_mock_osv_server() -> (String, Arc<AtomicUsize>, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let state = MockOsvState { hits: hits.clone() };
        let app = Router::new()
            .route("/v1/querybatch", post(handle_querybatch))
            .route("/v1/vulns/{id}", get(handle_vuln_by_id))
            .with_state(state);
        let (addr, server_fut) = rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let fut = async move {
                let _ = axum::serve(listener, app).await;
            };
            (addr, fut)
        });
        rt.spawn(server_fut);
        (format!("http://{addr}"), hits, rt)
    }

    #[test]
    fn fetch_hydrates_osv_cache_and_local_overrides_make_zero_network_requests() {
        let (base_url, hits, _rt) = spawn_mock_osv_server();
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("gradle.lockfile"),
            "com.fasterxml.jackson.core:jackson-databind:2.13.5=compileClasspath\norg.slf4j:slf4j-api:2.0.7=compileClasspath\n",
        )
        .unwrap();

        let opts = OsvFetchOpts::for_repo(dir.path(), Some(base_url.clone()), None);
        let summary = fetch_osv_for_repo(dir.path(), &opts).unwrap();
        assert_eq!(summary.packages_queried, 2);
        assert_eq!(summary.advisories_matched, 1);
        assert_eq!(summary.advisory_ids, vec!["GHSA-mock-jackson-0001"]);
        assert_eq!(hits.load(Ordering::SeqCst), 2); // 1 POST /v1/querybatch + 1 GET /v1/vulns/{id}
        let cached_path = Path::new(&summary.cached_files[0]);
        assert!(cached_path.is_file());

        // Reset counter and verify `--osv`, `--from-report`, and `--offline` all perform ZERO network calls!
        hits.store(0, Ordering::SeqCst);

        let (offline_advs, _) = resolve_advisories_and_inventory(
            dir.path(),
            &AdvisorySourceOpts {
                offline: true,
                api_url: Some(base_url.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(offline_advs.len(), 1);
        assert_eq!(hits.load(Ordering::SeqCst), 0);

        let (dir_override_advs, _) = resolve_advisories_and_inventory(
            dir.path(),
            &AdvisorySourceOpts {
                osv: Some(cached_path.parent().unwrap().to_path_buf()),
                api_url: Some(base_url.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(dir_override_advs.len(), 1);
        assert_eq!(hits.load(Ordering::SeqCst), 0);

        // Trivy report override + CycloneDX SBOM override -> ZERO network calls!
        let trivy_path = dir.path().join("trivy.json");
        fs::write(
            &trivy_path,
            r#"{
              "Results": [{
                "Target": "pom.xml",
                "Type": "pom",
                "Vulnerabilities": [{
                  "VulnerabilityID": "CVE-2022-42004",
                  "PkgName": "com.fasterxml.jackson.core:jackson-databind",
                  "InstalledVersion": "2.13.5",
                  "FixedVersion": "2.14.0",
                  "Title": "Jackson stack overflow"
                }]
              }]
            }"#,
        )
        .unwrap();
        let sbom_path = dir.path().join("bom.cdx.json");
        fs::write(
            &sbom_path,
            r#"{
              "bomFormat": "CycloneDX",
              "specVersion": "1.5",
              "components": [{
                "name": "express",
                "version": "4.18.2",
                "purl": "pkg:npm/express@4.18.2"
              }]
            }"#,
        )
        .unwrap();
        let (report_advs, merged_inv) = resolve_advisories_and_inventory(
            dir.path(),
            &AdvisorySourceOpts {
                from_report: Some(trivy_path),
                sbom: Some(sbom_path),
                api_url: Some(base_url),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report_advs.len(), 1);
        assert_eq!(report_advs[0].id, "CVE-2022-42004");
        assert!(merged_inv.iter().any(|d| d.name == "express"));
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }
}
