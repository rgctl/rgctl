//! Multi-ecosystem package coordinate → import/module prefix resolver.
//!
//! Tables + heuristics for Maven, npm, Cargo, Go, PyPI, NuGet, RubyGems, Composer.
//! Non-trivial Maven groups (e.g. `xalan`) MUST use the mapping table — never guess.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Resolved identity for structured queries (`find --package`, …).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageResolution {
    /// Original coordinate string.
    pub input: String,
    /// Normalized ecosystem (`Maven`, `npm`, `crates.io`, …).
    pub ecosystem: String,
    /// Canonical package name (GAV / crate / module).
    pub name: String,
    /// Import / module prefixes usable with `find --type import` / scope filters.
    pub import_prefixes: Vec<String>,
    /// Typically shipped with the language runtime / JDK / stdlib.
    pub runtime_bundled: bool,
    /// Honesty note when mapping is table-driven or heuristic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub honesty: Option<String>,
}

/// Resolver error.
#[derive(Debug, thiserror::Error)]
pub enum PackageResolveError {
    /// Coordinate could not be parsed.
    #[error("unrecognized package coordinate: {0}")]
    Unrecognized(String),
    /// Maven short group with no table entry (refuse silent wrong guess).
    #[error(
        "Maven group `{group}` has no import mapping (refusing heuristic guess); \
         add an entry or pass an explicit import prefix"
    )]
    UnmappedMavenGroup {
        /// Group id.
        group: String,
    },
}

/// Explicit honesty notes for Tier-1 languages that do not have a canonical application
/// lockfile / package ecosystem (Task 4.3).
pub fn structural_only_language_honesty(language: &str) -> Option<&'static str> {
    match language.trim().to_ascii_lowercase().as_str() {
        "bash" | "sh" | "shell" => Some(
            "Bash has no canonical package lockfile; CPG taint and parked command calls are \
             evaluated structurally only (pair with `--sbom` or `--from-report` for OS packages).",
        ),
        "sql" => Some(
            "SQL plugins emit structural/CFG artifacts only and have no third-party package lockfile.",
        ),
        "hcl" | "terraform" => Some(
            "HCL/Terraform is declarative infrastructure configuration without imperative CFG/PDG taint.",
        ),
        "markdown" | "md" => Some(
            "Markdown is documentation-only and never participates in executable vulnerability reachability.",
        ),
        _ => None,
    }
}

/// Resolve coordinates with an optional known ecosystem hint (from lockfiles / OSV `affected[].package.ecosystem`).
pub fn resolve_package_with_ecosystem(
    ecosystem_hint: Option<&str>,
    coords: &str,
) -> Result<PackageResolution, PackageResolveError> {
    let trimmed = coords.trim();
    if trimmed.is_empty() {
        return Err(PackageResolveError::Unrecognized(coords.into()));
    }
    if trimmed.starts_with("pkg:") {
        return resolve_package(trimmed);
    }
    if let Some(eco) = ecosystem_hint {
        let e = eco.trim().to_ascii_lowercase();
        let clean = trimmed.split('@').next().unwrap_or(trimmed);
        match e.as_str() {
            "maven" | "gradle" if clean.contains(':') => return resolve_maven_gav(clean),
            "npm" | "javascript" | "typescript" => {
                return Ok(PackageResolution {
                    input: trimmed.into(),
                    ecosystem: "npm".into(),
                    name: clean.into(),
                    import_prefixes: vec![clean.to_string()],
                    runtime_bundled: false,
                    honesty: Some("npm package name used as JS/TS import prefix".into()),
                });
            }
            "crates.io" | "cargo" | "rust" => {
                let mut prefixes = vec![clean.replace('-', "_")];
                if !prefixes.iter().any(|p| p == clean) {
                    prefixes.push(clean.to_string());
                }
                return Ok(PackageResolution {
                    input: trimmed.into(),
                    ecosystem: "crates.io".into(),
                    name: clean.into(),
                    import_prefixes: prefixes,
                    runtime_bundled: false,
                    honesty: Some(
                        "Cargo crate name → Rust `use` path (hyphens → underscores)".into(),
                    ),
                });
            }
            "nuget" | "dotnet" | "csharp" => {
                let root_ns = clean.split('.').next().unwrap_or(clean).to_string();
                let mut prefixes = vec![clean.to_string()];
                if root_ns != clean && !root_ns.is_empty() {
                    prefixes.push(root_ns);
                }
                return Ok(PackageResolution {
                    input: trimmed.into(),
                    ecosystem: "NuGet".into(),
                    name: clean.into(),
                    import_prefixes: prefixes,
                    runtime_bundled: false,
                    honesty: Some("NuGet package ID → C# `using` namespace heuristic".into()),
                });
            }
            "packagist" | "composer" | "php" => {
                let ns = composer_to_psr4_hint(clean);
                let mut prefixes = vec![ns];
                prefixes.push(clean.to_string());
                return Ok(PackageResolution {
                    input: trimmed.into(),
                    ecosystem: "Packagist".into(),
                    name: clean.into(),
                    import_prefixes: prefixes,
                    runtime_bundled: false,
                    honesty: Some(
                        "Packagist vendor/package → PSR-4 PHP namespace heuristic".into(),
                    ),
                });
            }
            "rubygems" | "gem" | "ruby" => {
                let slash = clean.replace('-', "/");
                let under = clean.replace('-', "_");
                let mut prefixes = vec![clean.to_string()];
                if !prefixes.contains(&slash) {
                    prefixes.push(slash);
                }
                if !prefixes.contains(&under) {
                    prefixes.push(under);
                }
                return Ok(PackageResolution {
                    input: trimmed.into(),
                    ecosystem: "RubyGems".into(),
                    name: clean.into(),
                    import_prefixes: prefixes,
                    runtime_bundled: false,
                    honesty: Some("RubyGems → Ruby `require` path heuristic".into()),
                });
            }
            "pub" | "dart" => {
                return Ok(resolve_dart_pub(trimmed, clean));
            }
            "hex" | "elixir" => {
                return Ok(resolve_elixir_hex(trimmed, clean));
            }
            "swift" | "swifturl" | "swiftpm" => {
                return Ok(resolve_swift_pm(trimmed, clean));
            }
            "conan" | "conancenter" | "vcpkg" | "c" | "cpp" => {
                return Ok(resolve_cpp_package(
                    trimmed,
                    clean,
                    if e == "vcpkg" { "vcpkg" } else { "Conan" },
                ));
            }
            "puppet" => {
                return Ok(resolve_puppet_module(trimmed, clean));
            }
            "luarocks" | "lua" => {
                return Ok(resolve_luarocks(trimmed, clean));
            }
            "pypi" | "python" => {
                return Ok(PackageResolution {
                    input: trimmed.into(),
                    ecosystem: "PyPI".into(),
                    name: clean.into(),
                    import_prefixes: vec![clean.replace('-', "_"), clean.to_string()],
                    runtime_bundled: is_python_stdlib(clean),
                    honesty: Some("PyPI name → Python import module heuristic".into()),
                });
            }
            "go" | "golang" => {
                let mut prefixes = vec![clean.to_string()];
                let pkg_seg = go_package_tail(clean);
                if !pkg_seg.is_empty() && !prefixes.iter().any(|p| p == pkg_seg) {
                    prefixes.push(pkg_seg.to_string());
                }
                return Ok(PackageResolution {
                    input: trimmed.into(),
                    ecosystem: "Go".into(),
                    name: clean.into(),
                    import_prefixes: prefixes,
                    runtime_bundled: false,
                    honesty: None,
                });
            }
            _ => {}
        }
    }
    resolve_package(trimmed)
}

/// Resolve PURL or ecosystem-native coordinates to import prefixes.
pub fn resolve_package(coords: &str) -> Result<PackageResolution, PackageResolveError> {
    let trimmed = coords.trim();
    if trimmed.is_empty() {
        return Err(PackageResolveError::Unrecognized(coords.into()));
    }
    if let Some(rest) = trimmed.strip_prefix("pkg:") {
        return resolve_purl(rest, trimmed);
    }
    if let Some(pub_pkg) = trimmed.strip_prefix("package:") {
        let name = pub_pkg.split('/').next().unwrap_or(pub_pkg);
        return Ok(resolve_dart_pub(trimmed, name));
    }
    if trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.ends_with(".git")
    {
        return Ok(resolve_swift_pm(trimmed, trimmed));
    }
    // npm scoped (@scope/name) before Maven GAV (group:artifact)
    if trimmed.starts_with('@') {
        let name = trimmed
            .split('@')
            .nth(1)
            .map(|s| format!("@{s}"))
            .unwrap_or_else(|| trimmed.to_string());
        // Actually "@types/node".split('@') => ["", "types/node"] → "@types/node"
        let name = if trimmed.starts_with('@') {
            trimmed.to_string()
        } else {
            name
        };
        return Ok(PackageResolution {
            input: trimmed.into(),
            ecosystem: "npm".into(),
            name: name.clone(),
            import_prefixes: vec![name],
            runtime_bundled: false,
            honesty: Some("npm scoped package name used as JS/TS import prefix".into()),
        });
    }
    // Maven GAV: group:artifact or group:artifact:version
    if trimmed.contains(':') && !trimmed.contains('/') {
        return resolve_maven_gav(trimmed);
    }
    // crates.io / Cargo
    if trimmed.starts_with("crates.io/") || looks_like_crate(trimmed) {
        let name = trimmed
            .strip_prefix("crates.io/")
            .unwrap_or(trimmed)
            .split('@')
            .next()
            .unwrap_or(trimmed)
            .to_string();
        return Ok(PackageResolution {
            input: trimmed.into(),
            ecosystem: "crates.io".into(),
            name: name.clone(),
            import_prefixes: vec![name.replace('-', "_")],
            runtime_bundled: false,
            honesty: Some("Cargo crate name → Rust use path (hyphens → underscores)".into()),
        });
    }
    // npm scoped or plain
    if trimmed.starts_with('@') || trimmed.contains('/') && !trimmed.contains('.') {
        let name = trimmed.split('@').next().unwrap_or(trimmed).to_string();
        return Ok(PackageResolution {
            input: trimmed.into(),
            ecosystem: "npm".into(),
            name: name.clone(),
            import_prefixes: vec![name],
            runtime_bundled: false,
            honesty: Some("npm package name used as JS/TS import prefix".into()),
        });
    }
    // Go module path
    if trimmed.contains('.') && trimmed.contains('/') {
        let mut prefixes = vec![trimmed.to_string()];
        let pkg_seg = go_package_tail(trimmed);
        if !pkg_seg.is_empty() && !prefixes.iter().any(|p| p == pkg_seg) {
            prefixes.push(pkg_seg.to_string());
        }
        return Ok(PackageResolution {
            input: trimmed.into(),
            ecosystem: "Go".into(),
            name: trimmed.to_string(),
            import_prefixes: prefixes,
            runtime_bundled: false,
            honesty: Some("treated as Go module import path".into()),
        });
    }
    // PyPI bare name
    Ok(PackageResolution {
        input: trimmed.into(),
        ecosystem: "PyPI".into(),
        name: trimmed.to_string(),
        import_prefixes: vec![trimmed.replace('-', "_")],
        runtime_bundled: is_python_stdlib(trimmed),
        honesty: Some(
            "PyPI name → import heuristic (hyphens → underscores); verify for namespace packages"
                .into(),
        ),
    })
}

fn resolve_purl(rest: &str, original: &str) -> Result<PackageResolution, PackageResolveError> {
    // pkg:maven/group/artifact@version
    let (eco, rem) = rest
        .split_once('/')
        .ok_or_else(|| PackageResolveError::Unrecognized(original.into()))?;
    let eco_l = eco.to_ascii_lowercase();
    match eco_l.as_str() {
        "maven" => {
            let (path, _ver) = rem.split_once('@').unwrap_or((rem, ""));
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() < 2 {
                return Err(PackageResolveError::Unrecognized(original.into()));
            }
            let group = parts[0].replace('%', "."); // rarely encoded
            let group = group.replace("%2E", ".");
            let artifact = parts[1];
            resolve_maven_parts(&format!("{group}:{artifact}"), &group, artifact)
        }
        "npm" => {
            let (path, _) = rem.split_once('@').unwrap_or((rem, ""));
            // scoped: @scope/name encoded as %40scope/name or @scope/name
            let name = path.replace("%40", "@");
            Ok(PackageResolution {
                input: original.into(),
                ecosystem: "npm".into(),
                name: name.clone(),
                import_prefixes: vec![name],
                runtime_bundled: false,
                honesty: None,
            })
        }
        "cargo" | "crate" => {
            let (name, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(PackageResolution {
                input: original.into(),
                ecosystem: "crates.io".into(),
                name: name.into(),
                import_prefixes: vec![name.replace('-', "_")],
                runtime_bundled: false,
                honesty: None,
            })
        }
        "golang" | "go" => {
            let (path, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(PackageResolution {
                input: original.into(),
                ecosystem: "Go".into(),
                name: path.into(),
                import_prefixes: vec![path.into()],
                runtime_bundled: false,
                honesty: None,
            })
        }
        "pypi" => {
            let (name, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(PackageResolution {
                input: original.into(),
                ecosystem: "PyPI".into(),
                name: name.into(),
                import_prefixes: vec![name.replace('-', "_")],
                runtime_bundled: is_python_stdlib(name),
                honesty: None,
            })
        }
        "nuget" => {
            let (name, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(PackageResolution {
                input: original.into(),
                ecosystem: "NuGet".into(),
                name: name.into(),
                import_prefixes: vec![name.replace('.', "/"), name.into()],
                runtime_bundled: false,
                honesty: Some("NuGet id → C# namespace heuristic".into()),
            })
        }
        "gem" | "rubygems" => {
            let (name, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(PackageResolution {
                input: original.into(),
                ecosystem: "RubyGems".into(),
                name: name.into(),
                import_prefixes: vec![name.replace('-', "/"), name.replace('-', "_")],
                runtime_bundled: false,
                honesty: Some("RubyGems → require path heuristic".into()),
            })
        }
        "composer" | "packagist" => {
            let (path, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(PackageResolution {
                input: original.into(),
                ecosystem: "Packagist".into(),
                name: path.into(),
                import_prefixes: vec![composer_to_psr4_hint(path), path.replace('/', "\\")],
                runtime_bundled: false,
                honesty: Some("Composer vendor/package → PHP namespace stub".into()),
            })
        }
        "pub" => {
            let (name, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(resolve_dart_pub(original, name))
        }
        "hex" => {
            let (name, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(resolve_elixir_hex(original, name))
        }
        "swift" | "swifturl" | "swiftpm" => {
            let (path, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(resolve_swift_pm(original, path))
        }
        "conan" | "conancenter" => {
            let (path, _) = rem.split_once('@').unwrap_or((rem, ""));
            let name = path.rsplit('/').next().unwrap_or(path);
            Ok(resolve_cpp_package(original, name, "Conan"))
        }
        "vcpkg" => {
            let (path, _) = rem.split_once('@').unwrap_or((rem, ""));
            let name = path.rsplit('/').next().unwrap_or(path);
            Ok(resolve_cpp_package(original, name, "vcpkg"))
        }
        "puppet" => {
            let (path, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(resolve_puppet_module(original, path))
        }
        "luarocks" => {
            let (name, _) = rem.split_once('@').unwrap_or((rem, ""));
            Ok(resolve_luarocks(original, name))
        }
        _ => Err(PackageResolveError::Unrecognized(original.into())),
    }
}

fn resolve_maven_gav(gav: &str) -> Result<PackageResolution, PackageResolveError> {
    let parts: Vec<&str> = gav.split(':').collect();
    if parts.len() < 2 {
        return Err(PackageResolveError::Unrecognized(gav.into()));
    }
    resolve_maven_parts(gav, parts[0], parts[1])
}

fn resolve_maven_parts(
    input: &str,
    group: &str,
    artifact: &str,
) -> Result<PackageResolution, PackageResolveError> {
    let name = format!("{group}:{artifact}");
    let runtime = is_jdk_bundled(group, artifact);
    let resolved = if let Some(prefix) = maven_table().get(&format!("{group}:{artifact}")) {
        PackageResolution {
            input: input.into(),
            ecosystem: "Maven".into(),
            name,
            import_prefixes: vec![prefix.clone()],
            runtime_bundled: runtime,
            honesty: Some("table-mapped Maven coordinate".into()),
        }
    } else if let Some(prefix) = maven_table().get(group) {
        PackageResolution {
            input: input.into(),
            ecosystem: "Maven".into(),
            name,
            import_prefixes: vec![prefix.clone()],
            runtime_bundled: runtime,
            honesty: Some("table-mapped Maven group".into()),
        }
    } else if !group.contains('.') {
        // Short / non-dotted groups without a table entry: refuse silent guess.
        return Err(PackageResolveError::UnmappedMavenGroup {
            group: group.into(),
        });
    } else {
        // Dotted group → Java package prefix (drop trailing segments that look like artifact org).
        let prefix = heuristic_maven_prefix(group, artifact);
        PackageResolution {
            input: input.into(),
            ecosystem: "Maven".into(),
            name,
            import_prefixes: vec![prefix],
            runtime_bundled: runtime,
            honesty: Some("heuristic: dotted Maven group → Java package prefix".into()),
        }
    };
    Ok(enrich_xalan_jdk_dual_path(group, resolved))
}

/// Xalan exists both as Maven (`org.apache.xalan` / `org.apache.xml.serializer`) and as the
/// JDK-bundled JAXP XSLTC engine (`com.sun.org.apache.xalan.internal.*`) reached via
/// `javax.xml.transform`. Surface both so `find --package` / callers do not miss JAXP imports.
fn enrich_xalan_jdk_dual_path(group: &str, mut resolved: PackageResolution) -> PackageResolution {
    if group != "xalan" {
        return resolved;
    }
    resolved.runtime_bundled = true;
    for alias in ["javax.xml.transform", "com.sun.org.apache.xalan.internal"] {
        if !resolved.import_prefixes.iter().any(|p| p == alias) {
            resolved.import_prefixes.push(alias.into());
        }
    }
    let note = "Xalan also ships as JDK-bundled JAXP (XSLTC); \
                prefixes include javax.xml.transform + com.sun.org.apache.xalan.internal";
    resolved.honesty = Some(match resolved.honesty.take() {
        Some(prev) => format!("{prev}; {note}"),
        None => note.into(),
    });
    resolved
}

fn heuristic_maven_prefix(group: &str, artifact: &str) -> String {
    // Prefer longest useful prefix: group itself for jackson.core style.
    if group.starts_with("com.fasterxml.jackson") {
        return "com.fasterxml.jackson".into();
    }
    if artifact.contains('.') {
        return format!("{group}.{artifact}");
    }
    group.to_string()
}

fn maven_table() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut m = HashMap::new();
        // Non-trivial short groups
        m.insert("xalan".into(), "org.apache.xalan".into());
        m.insert("xalan:xalan".into(), "org.apache.xalan".into());
        m.insert(
            "xalan:serializer".into(),
            "org.apache.xml.serializer".into(),
        );
        m.insert(
            "xalan:serializer-bcel-6.6.1".into(),
            "org.apache.xml.serializer".into(),
        );
        m.insert("xerces".into(), "org.apache.xerces".into());
        m.insert("xerces:xercesImpl".into(), "org.apache.xerces".into());
        m.insert("dom4j".into(), "org.dom4j".into());
        m.insert("dom4j:dom4j".into(), "org.dom4j".into());
        m.insert("antlr".into(), "org.antlr".into());
        m.insert("asm".into(), "org.objectweb.asm".into());
        // Common aliases
        m.insert(
            "com.fasterxml.jackson.core:jackson-databind".into(),
            "com.fasterxml.jackson".into(),
        );
        m.insert(
            "com.fasterxml.jackson.core:jackson-core".into(),
            "com.fasterxml.jackson".into(),
        );
        m.insert(
            "com.fasterxml.jackson.core:jackson-annotations".into(),
            "com.fasterxml.jackson".into(),
        );
        m.insert("org.slf4j:slf4j-api".into(), "org.slf4j".into());
        m.insert(
            "ch.qos.logback:logback-classic".into(),
            "ch.qos.logback".into(),
        );
        m
    })
}

fn is_jdk_bundled(group: &str, artifact: &str) -> bool {
    matches!(
        (group, artifact),
        ("javax.xml" | "jakarta.xml.bind" | "org.w3c" | "org.xml", _)
            | ("com.sun.xml" | "com.sun.org.apache", _)
    ) || group.starts_with("java.")
        || group == "jdk"
        || group == "xalan" // also JDK JAXP XSLTC; enrich_xalan_jdk_dual_path adds aliases
        || (group == "xml-apis" && artifact == "xml-apis")
}

fn is_python_stdlib(name: &str) -> bool {
    matches!(
        name,
        "os" | "sys" | "json" | "re" | "typing" | "pathlib" | "http" | "urllib" | "asyncio"
    )
}

fn pascal_case_identifier(raw: &str) -> String {
    raw.split(['-', '_', '.'])
        .filter(|seg| !seg.is_empty())
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn composer_to_psr4_hint(coord: &str) -> String {
    coord
        .split('/')
        .filter(|s| !s.is_empty())
        .map(pascal_case_identifier)
        .collect::<Vec<_>>()
        .join("\\")
}

fn resolve_dart_pub(input: &str, name: &str) -> PackageResolution {
    let clean = name.trim_end_matches(".dart");
    PackageResolution {
        input: input.into(),
        ecosystem: "Pub".into(),
        name: clean.into(),
        import_prefixes: vec![format!("package:{clean}"), clean.into()],
        runtime_bundled: false,
        honesty: Some("Pub package → Dart `package:<name>/...` import prefix".into()),
    }
}

fn resolve_elixir_hex(input: &str, name: &str) -> PackageResolution {
    let pascal = pascal_case_identifier(name);
    let mut prefixes = vec![pascal];
    if !prefixes.iter().any(|p| p == name) {
        prefixes.push(name.into());
    }
    PackageResolution {
        input: input.into(),
        ecosystem: "Hex".into(),
        name: name.into(),
        import_prefixes: prefixes,
        runtime_bundled: false,
        honesty: Some("Hex package → Elixir CamelCase module alias heuristic".into()),
    }
}

fn resolve_swift_pm(input: &str, url_or_identity: &str) -> PackageResolution {
    let stripped = url_or_identity
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .strip_prefix("https://")
        .or_else(|| url_or_identity.strip_prefix("http://"))
        .unwrap_or(url_or_identity)
        .trim_end_matches(".git");
    let repo_tail = stripped.rsplit('/').next().unwrap_or(stripped);
    let no_swift_prefix = repo_tail
        .strip_prefix("swift-")
        .or_else(|| repo_tail.strip_prefix("Swift-"))
        .unwrap_or(repo_tail);
    let mut prefixes = Vec::new();
    for candidate in [
        pascal_case_identifier(repo_tail),
        pascal_case_identifier(no_swift_prefix),
        no_swift_prefix.to_ascii_uppercase(),
        repo_tail.replace('-', "_"),
    ] {
        if !candidate.is_empty() && !prefixes.contains(&candidate) {
            prefixes.push(candidate);
        }
    }
    PackageResolution {
        input: input.into(),
        ecosystem: "SwiftURL".into(),
        name: stripped.into(),
        import_prefixes: prefixes,
        runtime_bundled: false,
        honesty: Some("SwiftPM repository identity → Swift module `import` heuristic".into()),
    }
}

fn resolve_cpp_package(input: &str, name: &str, eco: &str) -> PackageResolution {
    let clean = name.split('/').next().unwrap_or(name);
    let stem = clean
        .strip_prefix("lib")
        .filter(|s| s.len() >= 2)
        .unwrap_or(clean);
    let mut prefixes = vec![clean.replace('-', "_")];
    if clean != prefixes[0] {
        prefixes.push(clean.to_string());
    }
    if stem != clean && !prefixes.iter().any(|p| p == stem) {
        prefixes.push(stem.to_string());
    }
    PackageResolution {
        input: input.into(),
        ecosystem: eco.into(),
        name: clean.into(),
        import_prefixes: prefixes,
        runtime_bundled: false,
        honesty: Some(format!(
            "{eco} package → C/C++ `#include` header directory/stem heuristic"
        )),
    }
}

fn resolve_puppet_module(input: &str, raw: &str) -> PackageResolution {
    let normalized = raw.replace('-', "/");
    let mod_name = normalized.rsplit('/').next().unwrap_or(raw);
    let mut prefixes = vec![mod_name.to_string()];
    let ns_colons = normalized.replace('/', "::");
    if !prefixes.contains(&ns_colons) {
        prefixes.push(ns_colons);
    }
    PackageResolution {
        input: input.into(),
        ecosystem: "Puppet".into(),
        name: raw.into(),
        import_prefixes: prefixes,
        runtime_bundled: false,
        honesty: Some("Puppet Forge module → Puppet class/include namespace".into()),
    }
}

fn resolve_luarocks(input: &str, name: &str) -> PackageResolution {
    let mut prefixes = vec![name.to_string()];
    if let Some(stripped) = name.strip_prefix("lua").filter(|s| !s.is_empty()) {
        let trimmed = stripped.trim_start_matches(['-', '_']);
        if !trimmed.is_empty() && !prefixes.iter().any(|p| p == trimmed) {
            prefixes.push(trimmed.to_string());
        }
    }
    PackageResolution {
        input: input.into(),
        ecosystem: "LuaRocks".into(),
        name: name.into(),
        import_prefixes: prefixes,
        runtime_bundled: false,
        honesty: Some("LuaRocks rock → Lua `require(...)` module heuristic".into()),
    }
}

fn go_package_tail(module_path: &str) -> &str {
    let mut parts = module_path.rsplit('/');
    let last = parts.next().unwrap_or(module_path);
    // Go semantic import versioning: `github.com/foo/bar/v2` → package identifier is `bar`.
    if let Some(ver_digits) = last.strip_prefix('v')
        && !ver_digits.is_empty()
        && ver_digits.chars().all(|c| c.is_ascii_digit())
        && let Some(prev) = parts.next()
    {
        return prev;
    }
    last
}

fn looks_like_crate(s: &str) -> bool {
    !s.contains('.')
        && !s.contains(':')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jackson_gav_to_prefix() {
        let r = resolve_package("com.fasterxml.jackson.core:jackson-databind").unwrap();
        assert_eq!(r.ecosystem, "Maven");
        assert!(
            r.import_prefixes
                .iter()
                .any(|p| p == "com.fasterxml.jackson")
        );
    }

    #[test]
    fn xalan_uses_table_not_guess() {
        let r = resolve_package("xalan:serializer").unwrap();
        assert!(r.import_prefixes.iter().any(|p| p.contains("apache")));
        assert!(!r.import_prefixes.iter().any(|p| p == "xalan"));
    }

    #[test]
    fn xalan_includes_jdk_jaxp_aliases() {
        for coords in [
            "xalan:xalan",
            "xalan:serializer",
            "pkg:maven/xalan/xalan@2.7.2",
        ] {
            let r = resolve_package(coords).unwrap();
            assert!(
                r.runtime_bundled,
                "{coords}: expected runtime_bundled for JDK dual path"
            );
            assert!(
                r.import_prefixes.iter().any(|p| p == "javax.xml.transform"),
                "{coords}: missing javax.xml.transform; got {:?}",
                r.import_prefixes
            );
            assert!(
                r.import_prefixes
                    .iter()
                    .any(|p| p == "com.sun.org.apache.xalan.internal"),
                "{coords}: missing com.sun.org.apache.xalan.internal; got {:?}",
                r.import_prefixes
            );
            assert!(
                r.honesty.as_deref().is_some_and(|h| h.contains("JAXP")),
                "{coords}: honesty should mention JAXP dual path"
            );
        }
        let serializer = resolve_package("xalan:serializer").unwrap();
        assert!(
            serializer
                .import_prefixes
                .iter()
                .any(|p| p == "org.apache.xml.serializer")
        );
        let core = resolve_package("xalan:xalan").unwrap();
        assert!(core.import_prefixes.iter().any(|p| p == "org.apache.xalan"));
    }

    #[test]
    fn unmapped_short_maven_group_errors() {
        let err = resolve_package("unknownshort:thing").unwrap_err();
        assert!(matches!(
            err,
            PackageResolveError::UnmappedMavenGroup { .. }
        ));
    }

    #[test]
    fn cargo_crate_prefix() {
        let r = resolve_package("serde_json").unwrap();
        assert_eq!(r.ecosystem, "crates.io");
        assert_eq!(r.import_prefixes, vec!["serde_json".to_string()]);
    }

    #[test]
    fn npm_scoped() {
        let r = resolve_package("@types/node").unwrap();
        assert_eq!(r.ecosystem, "npm");
        assert!(r.import_prefixes.iter().any(|p| p == "@types/node"));
    }

    #[test]
    fn jdk_runtime_flag() {
        let r = resolve_package("javax.xml:bind-api").unwrap();
        assert!(r.runtime_bundled);
    }

    #[test]
    fn tier1_swift_conan_pub_hex_puppet_luarocks_and_honesty_resolve() {
        let swift = resolve_package("https://github.com/apple/swift-nio.git").unwrap();
        assert_eq!(swift.ecosystem, "SwiftURL");
        assert!(
            swift
                .import_prefixes
                .iter()
                .any(|p| p == "NIO" || p == "SwiftNio")
        );

        let conan = resolve_package("pkg:conan/openssl@3.1.0").unwrap();
        assert_eq!(conan.ecosystem, "Conan");
        assert!(conan.import_prefixes.iter().any(|p| p == "openssl"));

        let vcpkg = resolve_package_with_ecosystem(Some("vcpkg"), "nlohmann-json").unwrap();
        assert_eq!(vcpkg.ecosystem, "vcpkg");
        assert!(vcpkg.import_prefixes.iter().any(|p| p == "nlohmann_json"));

        let pub_pkg = resolve_package("pkg:pub/dio@5.0.0").unwrap();
        assert_eq!(pub_pkg.ecosystem, "Pub");
        assert!(pub_pkg.import_prefixes.contains(&"package:dio".to_string()));

        let hex = resolve_package("pkg:hex/jason@1.4.0").unwrap();
        assert_eq!(hex.ecosystem, "Hex");
        assert!(hex.import_prefixes.contains(&"Jason".to_string()));

        let puppet = resolve_package("pkg:puppet/puppetlabs/stdlib@9.0.0").unwrap();
        assert_eq!(puppet.ecosystem, "Puppet");
        assert!(puppet.import_prefixes.contains(&"stdlib".to_string()));

        let lua = resolve_package("pkg:luarocks/luasocket@3.1.0").unwrap();
        assert_eq!(lua.ecosystem, "LuaRocks");
        assert!(lua.import_prefixes.contains(&"socket".to_string()));

        for lang in ["bash", "sql", "hcl", "markdown"] {
            assert!(structural_only_language_honesty(lang).is_some(), "{lang}");
        }
        assert!(structural_only_language_honesty("rust").is_none());
    }
}
