//! Build-manifest extractors → `SymbolType::Dependency` + `DependsOn`.

mod cargo;
mod go_mod;
mod gradle;
mod maven;
mod npm;

use rgctl_plugin_api::{Relation, RelationType, SourceLocation, Symbol, SymbolType};
use std::path::Path;

/// Declared dependency from a build manifest (v1: no lockfile/transitive closure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyDeclaration {
    /// Coordinate name (e.g. `io.quarkus:quarkus-hibernate-orm`, `serde`).
    pub name: String,
    /// Version requirement string when present.
    pub version_requirement: Option<String>,
    /// Scope/configuration (`compile`, `test`, `dev`, …).
    pub scope: Option<String>,
    /// Ecosystem id: `maven` | `cargo` | `npm` | `golang` | `gradle`.
    pub ecosystem: String,
    pub location: SourceLocation,
    pub optional: bool,
    /// Extra honesty flags (e.g. unresolved workspace inheritance).
    pub unresolved: bool,
}

/// Parse a known build manifest into dependency declarations (no graph emit).
pub fn extract_declarations(path: &Path, source: &[u8]) -> Vec<DependencyDeclaration> {
    let basename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match basename.as_str() {
        "pom.xml" => maven::extract(path, source),
        "cargo.toml" => cargo::extract(path, source),
        "package.json" => npm::extract(path, source),
        "go.mod" => go_mod::extract(path, source),
        "build.gradle" | "build.gradle.kts" => gradle::extract(path, source),
        _ => Vec::new(),
    }
}

/// Extract Dependency symbols and File→Dependency `DependsOn` relations.
pub fn extract_manifest(path: &Path, source: &[u8]) -> (Vec<Symbol>, Vec<Relation>) {
    declarations_to_graph(path, extract_declarations(path, source))
}

fn declarations_to_graph(
    path: &Path,
    decls: Vec<DependencyDeclaration>,
) -> (Vec<Symbol>, Vec<Relation>) {
    let file = path.to_string_lossy().to_string();
    let mut symbols = Vec::with_capacity(decls.len());
    let mut relations = Vec::with_capacity(decls.len());
    for d in decls {
        let qn = format!("{}:{}", d.ecosystem, d.name);
        let mut meta = serde_json::json!({
            "ecosystem": d.ecosystem,
            "optional": d.optional,
        });
        if let Some(v) = &d.version_requirement {
            meta["version"] = serde_json::Value::String(v.clone());
        }
        if let Some(s) = &d.scope {
            meta["scope"] = serde_json::Value::String(s.clone());
        }
        if d.unresolved {
            meta["unresolved"] = serde_json::Value::Bool(true);
        }
        symbols.push(Symbol {
            name: d.name.clone(),
            symbol_type: SymbolType::Dependency,
            qualified_name: Some(qn),
            location: d.location.clone(),
            signature: d.version_requirement.clone(),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: meta,
            ..Default::default()
        });
        relations.push(Relation {
            from: file.clone(),
            to: d.name,
            relation_type: RelationType::DependsOn,
            location: d.location,
            metadata: serde_json::json!({ "ecosystem": d.ecosystem }),
            to_qualified_hint: None,
            to_type_hint: Some("dependency".to_string()),
            ..Default::default()
        });
    }
    (symbols, relations)
}

pub(crate) fn loc(path: &Path, start_line: usize, end_line: usize) -> SourceLocation {
    SourceLocation {
        file: path.to_string_lossy().to_string(),
        start_line: start_line.max(1),
        end_line: end_line.max(start_line.max(1)),
        start_column: 1,
        end_column: 1,
    }
}
