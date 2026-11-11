//! Compile `frameworks/*.toml` → `OUT_DIR/frameworks.bin` and export `RGCTL_FRAMEWORKS_BIN`.

#[path = "src/framework_schema.rs"]
mod framework_schema;

use framework_schema::{CompiledFrameworkCatalog, LanguageFrameworkSpec};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let frameworks_dir = manifest_dir.join("frameworks");
    println!("cargo:rerun-if-changed=frameworks");

    let mut languages = Vec::new();
    let mut seen_ids: HashSet<(String, String)> = HashSet::new();

    if frameworks_dir.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(&frameworks_dir)
            .expect("read frameworks/")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
            .collect();
        entries.sort();

        for path in entries {
            println!("cargo:rerun-if-changed={}", path.display());
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let spec: LanguageFrameworkSpec =
                toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
            validate_language(&spec, &path, &mut seen_ids);
            languages.push(spec);
        }
    }

    let catalog = CompiledFrameworkCatalog {
        version: 1,
        languages,
    };

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_path = out_dir.join("frameworks.bin");
    let bytes = bincode::serialize(&catalog).expect("serialize frameworks catalog");
    fs::write(&out_path, bytes).expect("write frameworks.bin");
    println!(
        "cargo:rustc-env=RGCTL_FRAMEWORKS_BIN={}",
        out_path.display()
    );
}

fn validate_language(
    spec: &LanguageFrameworkSpec,
    path: &Path,
    seen_ids: &mut HashSet<(String, String)>,
) {
    if spec.language.trim().is_empty() {
        panic!("{}: language must be non-empty", path.display());
    }
    for rule in &spec.rules {
        if rule.id.trim().is_empty() {
            panic!("{}: rule id must be non-empty", path.display());
        }
        if rule.framework.trim().is_empty() {
            panic!(
                "{}: rule {} framework must be non-empty",
                path.display(),
                rule.id
            );
        }
        if rule.tags.is_empty() {
            panic!(
                "{}: rule {} tags must be non-empty",
                path.display(),
                rule.id
            );
        }
        if rule.vocab.trim().is_empty() {
            panic!(
                "{}: rule {} vocab must be non-empty",
                path.display(),
                rule.id
            );
        }
        let key = (spec.language.clone(), rule.id.clone());
        if !seen_ids.insert(key) {
            panic!(
                "{}: duplicate rule id '{}' for language '{}'",
                path.display(),
                rule.id,
                spec.language
            );
        }
    }
}
