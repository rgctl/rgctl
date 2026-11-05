//! Compile Konveyor rulesets into `OUT_DIR/kantra_catalog.bin` (`RBKC`).

use serde::Deserialize;
use serde_yaml::Value;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAGIC: &[u8; 4] = b"RBKC";
const VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct RulesetMeta {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct BuildRule {
    #[serde(rename = "ruleID")]
    rule_id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    effort: Option<u32>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    when: Value,
}

#[derive(Debug, serde::Serialize)]
struct StoredRule {
    rule_id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    effort: Option<u32>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    when_yaml: String,
}

impl StoredRule {
    fn from_build(rule: &BuildRule) -> Result<Self, String> {
        let when_yaml = serde_yaml::to_string(&rule.when)
            .map_err(|e| format!("rule {} when yaml: {e}", rule.rule_id))?;
        Ok(Self {
            rule_id: rule.rule_id.clone(),
            description: rule.description.clone(),
            category: rule.category.clone(),
            effort: rule.effort,
            message: rule.message.clone(),
            labels: rule.labels.clone(),
            when_yaml,
        })
    }
}

#[derive(Debug, serde::Serialize)]
struct StoredCatalog {
    version: u32,
    catalog_id: String,
    name: String,
    description: Option<String>,
    rules: Vec<StoredRule>,
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_path = out_dir.join("kantra_catalog.bin");

    let embedded_root = manifest_dir.join("assets/rulesets/stable/java");
    let rulesets_root = manifest_dir.join("assets/rulesets");
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crates/rgctl-kantra must live under crates/");
    let fixture_root = repo_root
        .join("tests/fixtures/kantra-rules")
        .canonicalize()
        .unwrap_or_else(|_| repo_root.join("tests/fixtures/kantra-rules"));

    let (source_label, source_roots, rulesets_rev) = if embedded_root.is_dir() {
        println!("cargo:rerun-if-changed={}", embedded_root.display());
        let dirs = collect_ruleset_dirs(&embedded_root);
        for dir in &dirs {
            println!("cargo:rerun-if-changed={}", dir.display());
        }
        let rev = rulesets_git_sha(&rulesets_root);
        if let Some(ref sha) = rev {
            let dot_git = rulesets_root.join(".git");
            if dot_git.exists() {
                println!("cargo:rerun-if-changed={}", dot_git.display());
            }
            write_rulesets_source(&out_dir, sha);
        }
        ("stable-java", dirs, rev)
    } else {
        println!(
            "cargo:warning=konveyor rulesets absent at {}; using fixture catalog",
            embedded_root.display()
        );
        println!("cargo:rerun-if-changed={}", fixture_root.display());
        ("fixture", vec![fixture_root], None)
    };

    let mut rules = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut duplicates_skipped = 0usize;
    let mut names = Vec::new();
    let mut descriptions = Vec::new();

    for root in &source_roots {
        match load_ruleset_dir(root, &mut seen_ids, &mut duplicates_skipped) {
            Ok((meta, mut batch)) => {
                names.push(meta.name);
                if let Some(desc) = meta.description {
                    descriptions.push(desc);
                }
                rules.append(&mut batch);
            }
            Err(err) => {
                panic!("kantra catalog build failed for {}: {err}", root.display());
            }
        }
    }

    let catalog_id = match rulesets_rev {
        Some(sha) => format!("{source_label}@{sha}"),
        None => format!("{}@{:x}", source_label, catalog_hash(&rules)),
    };
    let name = if names.len() == 1 {
        names[0].clone()
    } else {
        format!("embedded-{source_label}")
    };
    let description = if descriptions.len() == 1 {
        descriptions.pop()
    } else {
        None
    };

    let stored_rules: Vec<StoredRule> = rules
        .iter()
        .map(StoredRule::from_build)
        .collect::<Result<Vec<_>, _>>()
        .expect("serialize rule when clauses");

    let stored = StoredCatalog {
        version: VERSION,
        catalog_id,
        name,
        description,
        rules: stored_rules,
    };

    let mut out = fs::File::create(&out_path).expect("create kantra_catalog.bin");
    out.write_all(MAGIC).unwrap();
    out.write_all(&VERSION.to_le_bytes()).unwrap();
    bincode::serialize_into(&mut out, &stored).expect("serialize kantra catalog");

    println!(
        "cargo:rustc-env=RGCTL_KANTRA_CATALOG={}",
        out_path.display()
    );
    if duplicates_skipped > 0 {
        println!(
            "kantra catalog: skipped {duplicates_skipped} duplicate ruleIDs from upstream rulesets"
        );
    }
    println!(
        "kantra embedded catalog: {} rules ({})",
        stored.rules.len(),
        stored.catalog_id
    );
}

fn write_rulesets_source(out_dir: &Path, sha: &str) {
    let path = out_dir.join("kantra_rulesets_source.txt");
    if let Ok(mut file) = fs::File::create(path) {
        let _ = writeln!(file, "konveyor/rulesets@{sha}");
    }
}

fn rulesets_git_sha(rulesets_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(rulesets_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    let sha = sha.trim();
    if sha.is_empty() {
        return None;
    }
    Some(sha.to_string())
}

fn catalog_hash(rules: &[BuildRule]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    rules.len().hash(&mut hasher);
    for rule in rules {
        rule.rule_id.hash(&mut hasher);
    }
    hasher.finish()
}

fn collect_ruleset_dirs(root: &Path) -> Vec<PathBuf> {
    if root.join("ruleset.yaml").is_file() {
        return vec![root.to_path_buf()];
    }
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_ruleset_dirs(&path));
        }
    }
    out
}

fn load_ruleset_dir(
    dir: &Path,
    seen_ids: &mut HashSet<String>,
    duplicates_skipped: &mut usize,
) -> Result<(RulesetMeta, Vec<BuildRule>), String> {
    let meta_path = dir.join("ruleset.yaml");
    let meta_text =
        fs::read_to_string(&meta_path).map_err(|e| format!("{}: {e}", meta_path.display()))?;
    let meta: RulesetMeta =
        serde_yaml::from_str(&meta_text).map_err(|e| format!("{}: {e}", meta_path.display()))?;

    let mut rules = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        if path.file_name() == Some(std::ffi::OsStr::new("ruleset.yaml")) {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut batch: Vec<BuildRule> =
            serde_yaml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        for rule in &mut batch {
            merge_ruleset_labels(&meta.labels, rule);
            if !seen_ids.insert(rule.rule_id.clone()) {
                *duplicates_skipped += 1;
                continue;
            }
            rules.push(rule.clone());
        }
    }
    Ok((meta, rules))
}

fn merge_ruleset_labels(ruleset_labels: &[String], rule: &mut BuildRule) {
    for label in ruleset_labels {
        if !rule.labels.iter().any(|l| l == label) {
            rule.labels.push(label.clone());
        }
    }
}
