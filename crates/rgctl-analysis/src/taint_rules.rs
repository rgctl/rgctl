//! Declarative taint source/sink/sanitizer rules (#16).
//!
//! Built-in packs live under `crates/rgctl-analysis/rules/taint/`. Matchers are
//! compiled once per language (substring + optional regex). Discover taint
//! remains opt-in (`--with-taint`); language packs are scoped to the active set.
//!
//! Merge precedence: built-in < `.rgctl/taint-rules.d/` < `--taint-rules` < overlays.
//! See issue <https://github.com/rgctl/rgctl/issues/16>.

use crate::language_profile::canonical_language_id;
use crate::taint::{Sanitizer, TaintSink, TaintSource};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Supported on-disk schema version.
pub const TAINT_RULE_SCHEMA_VERSION: u32 = 1;

/// Role of a taint rule within a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleRole {
    /// Taint source.
    Source,
    /// Taint sink.
    Sink,
    /// Sanitizer that may break taint flow.
    Sanitizer,
}

/// Match descriptor (v1: substring / regex / compounds).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MatchSpec {
    /// Single substring.
    Substring {
        /// Needle.
        value: String,
        /// Reject if any of these substrings are also present.
        #[serde(default)]
        unless_any: Vec<String>,
    },
    /// Any of the listed substrings.
    AnySubstring {
        /// Needles (OR).
        values: Vec<String>,
    },
    /// All listed substrings must appear.
    AllSubstrings {
        /// Needles (AND).
        values: Vec<String>,
    },
    /// Regex (compiled into a `RegexSet` per language when possible).
    Regex {
        /// Pattern.
        value: String,
    },
    /// Conjunction of nested matchers.
    AllOf {
        /// Child matchers.
        of: Vec<MatchSpec>,
    },
}

/// One declarative rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintRule {
    /// Stable rule id.
    pub id: String,
    /// Source / sink / sanitizer.
    pub role: RuleRole,
    /// Kind name matching [`TaintSource`] / [`TaintSink`] / [`Sanitizer`] variants.
    pub kind: String,
    /// Optional CWE id (e.g. `CWE-89`).
    #[serde(default)]
    pub cwe: Option<String>,
    /// Detail for `TypeCast` / `Validation` sanitizers.
    #[serde(default)]
    pub detail: Option<String>,
    /// Match descriptor.
    #[serde(rename = "match")]
    pub match_spec: MatchSpec,
}

/// On-disk / embedded rule document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintRuleDocument {
    /// Schema version (must be [`TAINT_RULE_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Pack id.
    pub id: String,
    /// Languages this pack applies to.
    pub languages: Vec<String>,
    /// Ordered rules (first match wins per role).
    pub rules: Vec<TaintRule>,
}

/// CWE catalog entry (bridges former `CwePattern` defaults).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CweCatalogEntry {
    /// CWE id.
    pub cwe_id: String,
    /// Short name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Severity 1–10.
    pub severity: u8,
    /// Source regex patterns.
    #[serde(default)]
    pub source_patterns: Vec<String>,
    /// Sink regex patterns.
    #[serde(default)]
    pub sink_patterns: Vec<String>,
    /// Sanitizer regex patterns.
    #[serde(default)]
    pub sanitizer_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CweCatalogDocument {
    schema_version: u32,
    #[allow(dead_code)]
    id: String,
    patterns: Vec<CweCatalogEntry>,
}

/// Rule load / compile errors.
#[derive(Debug, thiserror::Error)]
pub enum TaintRuleError {
    /// Unsupported schema version.
    #[error("unsupported taint rule schema_version {found} (expected {expected})")]
    UnsupportedVersion {
        /// Found version.
        found: u32,
        /// Expected version.
        expected: u32,
    },
    /// YAML parse failure.
    #[error("taint rule YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    /// IO failure.
    #[error("taint rule IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Invalid regex.
    #[error("taint rule regex error in {rule_id}: {source}")]
    Regex {
        /// Rule id.
        rule_id: String,
        /// Regex error.
        #[source]
        source: regex::Error,
    },
    /// Unknown kind string.
    #[error("unknown taint {role:?} kind `{kind}` in rule `{rule_id}`")]
    UnknownKind {
        /// Role.
        role: RuleRole,
        /// Kind string.
        kind: String,
        /// Rule id.
        rule_id: String,
    },
}

/// Compiled matcher for one rule.
#[derive(Debug)]
enum CompiledMatcher {
    Substring {
        value: String,
        unless_any: Vec<String>,
    },
    AnySubstring(Vec<String>),
    AllSubstrings(Vec<String>),
    Regex(Regex),
    AllOf(Vec<CompiledMatcher>),
}

impl CompiledMatcher {
    fn matches(&self, text: &str) -> bool {
        match self {
            Self::Substring { value, unless_any } => {
                text.contains(value.as_str()) && !unless_any.iter().any(|u| text.contains(u))
            }
            Self::AnySubstring(values) => values.iter().any(|v| text.contains(v)),
            Self::AllSubstrings(values) => values.iter().all(|v| text.contains(v)),
            Self::Regex(re) => re.is_match(text),
            Self::AllOf(parts) => parts.iter().all(|p| p.matches(text)),
        }
    }

    fn compile(spec: &MatchSpec, rule_id: &str) -> Result<Self, TaintRuleError> {
        Ok(match spec {
            MatchSpec::Substring { value, unless_any } => Self::Substring {
                value: value.clone(),
                unless_any: unless_any.clone(),
            },
            MatchSpec::AnySubstring { values } => Self::AnySubstring(values.clone()),
            MatchSpec::AllSubstrings { values } => Self::AllSubstrings(values.clone()),
            MatchSpec::Regex { value } => {
                Self::Regex(Regex::new(value).map_err(|source| TaintRuleError::Regex {
                    rule_id: rule_id.to_string(),
                    source,
                })?)
            }
            MatchSpec::AllOf { of } => {
                let mut parts = Vec::with_capacity(of.len());
                for child in of {
                    parts.push(Self::compile(child, rule_id)?);
                }
                Self::AllOf(parts)
            }
        })
    }
}

#[derive(Debug)]
struct CompiledRule {
    matcher: CompiledMatcher,
    source: Option<TaintSource>,
    sink: Option<TaintSink>,
    sanitizer: Option<Sanitizer>,
}

/// Compiled rules for one language (first match wins per role).
#[derive(Debug, Default)]
pub struct CompiledLanguageRules {
    sources: Vec<CompiledRule>,
    sinks: Vec<CompiledRule>,
    sanitizers: Vec<CompiledRule>,
}

impl CompiledLanguageRules {
    /// Number of compiled taint source matchers for this language.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Number of compiled taint sink matchers for this language.
    pub fn sink_count(&self) -> usize {
        self.sinks.len()
    }

    /// Number of compiled taint sanitizer matchers for this language.
    pub fn sanitizer_count(&self) -> usize {
        self.sanitizers.len()
    }

    /// Apply rules to statement text; returns first match per role.
    pub fn classify(
        &self,
        text: &str,
    ) -> (Option<TaintSource>, Option<TaintSink>, Option<Sanitizer>) {
        let source = self
            .sources
            .iter()
            .find(|r| r.matcher.matches(text))
            .and_then(|r| r.source);
        let sink = self
            .sinks
            .iter()
            .find(|r| r.matcher.matches(text))
            .and_then(|r| r.sink);
        let sanitizer = self
            .sanitizers
            .iter()
            .find(|r| r.matcher.matches(text))
            .and_then(|r| r.sanitizer.clone());
        (source, sink, sanitizer)
    }

    fn push_rule(&mut self, rule: &TaintRule) -> Result<(), TaintRuleError> {
        let matcher = CompiledMatcher::compile(&rule.match_spec, &rule.id)?;
        match rule.role {
            RuleRole::Source => {
                let source =
                    parse_source_kind(&rule.kind).ok_or_else(|| TaintRuleError::UnknownKind {
                        role: rule.role,
                        kind: rule.kind.clone(),
                        rule_id: rule.id.clone(),
                    })?;
                self.sources.push(CompiledRule {
                    matcher,
                    source: Some(source),
                    sink: None,
                    sanitizer: None,
                });
            }
            RuleRole::Sink => {
                let sink =
                    parse_sink_kind(&rule.kind).ok_or_else(|| TaintRuleError::UnknownKind {
                        role: rule.role,
                        kind: rule.kind.clone(),
                        rule_id: rule.id.clone(),
                    })?;
                self.sinks.push(CompiledRule {
                    matcher,
                    source: None,
                    sink: Some(sink),
                    sanitizer: None,
                });
            }
            RuleRole::Sanitizer => {
                let sanitizer = parse_sanitizer_kind(&rule.kind, rule.detail.as_deref())
                    .ok_or_else(|| TaintRuleError::UnknownKind {
                        role: rule.role,
                        kind: rule.kind.clone(),
                        rule_id: rule.id.clone(),
                    })?;
                self.sanitizers.push(CompiledRule {
                    matcher,
                    source: None,
                    sink: None,
                    sanitizer: Some(sanitizer),
                });
            }
        }
        Ok(())
    }
}

/// Loaded + compiled taint rule set (language-scoped).
#[derive(Debug, Default)]
pub struct TaintRuleSet {
    by_language: HashMap<String, CompiledLanguageRules>,
}

impl TaintRuleSet {
    /// Empty set.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Built-in packs for all Tier‑1 languages (compiled once).
    pub fn bundled() -> Result<Self, TaintRuleError> {
        let mut set = Self::empty();
        for yaml in BUNDLED_PACKS {
            set.merge_document(&parse_document(yaml)?)?;
        }
        // C++ inherits C patterns (parity with former `detect_cpp_patterns`).
        set.extend_language_from("cpp", "c")?;
        Ok(set)
    }

    /// Shared process-wide built-in set (lazy).
    pub fn bundled_shared() -> &'static Result<Self, String> {
        static CELL: OnceLock<Result<TaintRuleSet, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::bundled().map_err(|e| e.to_string()))
    }

    /// Load YAML documents from a directory (`*.yaml` / `*.yml`).
    pub fn load_dir(dir: &Path) -> Result<Self, TaintRuleError> {
        let mut set = Self::empty();
        if !dir.is_dir() {
            return Ok(set);
        }
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "yaml" || e == "yml")
            })
            .collect();
        paths.sort();
        for path in paths {
            let text = std::fs::read_to_string(&path)?;
            set.merge_document(&parse_document(&text)?)?;
        }
        Ok(set)
    }

    /// Merge built-in < project dir < CLI path (file or directory).
    pub fn load_with_overlays(
        project_root: Option<&Path>,
        cli_path: Option<&Path>,
        languages: Option<&[String]>,
    ) -> Result<Self, TaintRuleError> {
        let mut set = Self::bundled()?;
        if let Some(root) = project_root {
            let project = root.join(".rgctl").join("taint-rules.d");
            set.merge_set(Self::load_dir(&project)?)?;
        }
        if let Some(path) = cli_path {
            if path.is_dir() {
                set.merge_set(Self::load_dir(path)?)?;
            } else {
                let text = std::fs::read_to_string(path)?;
                set.merge_document(&parse_document(&text)?)?;
            }
        }
        if let Some(langs) = languages {
            set = set.scoped(langs);
        }
        Ok(set)
    }

    /// Keep only listed languages (plus canonical aliases).
    pub fn scoped(mut self, languages: &[String]) -> Self {
        let mut keep = std::collections::HashSet::new();
        for lang in languages {
            let key = canonical_language_id(lang).unwrap_or(lang.as_str());
            keep.insert(key.to_string());
            if key == "typescript" {
                keep.insert("javascript".into());
            }
            if key == "javascript" {
                keep.insert("typescript".into());
            }
            if key == "cpp" {
                keep.insert("c".into());
            }
        }
        self.by_language.retain(|k, _| keep.contains(k));
        self
    }

    /// Append OSV / query sink overlays (substring sinks, default [`TaintSink::CodeEval`]).
    pub fn with_overlays(mut self, sink_names: &[impl AsRef<str>]) -> Self {
        for name in sink_names {
            let needle = name.as_ref().to_string();
            if needle.is_empty() {
                continue;
            }
            let rule = TaintRule {
                id: format!("overlay-sink-{}", needle),
                role: RuleRole::Sink,
                kind: "CodeEval".into(),
                cwe: None,
                detail: None,
                match_spec: MatchSpec::Substring {
                    value: needle,
                    unless_any: Vec::new(),
                },
            };
            // Apply overlay to every loaded language (query-time composition).
            let langs: Vec<String> = self.by_language.keys().cloned().collect();
            for lang in langs {
                if let Some(compiled) = self.by_language.get_mut(&lang) {
                    // Prepend so overlay wins over built-ins when both match.
                    let matcher = CompiledMatcher::compile(&rule.match_spec, &rule.id)
                        .expect("overlay substring compile");
                    compiled.sinks.insert(
                        0,
                        CompiledRule {
                            matcher,
                            source: None,
                            sink: Some(TaintSink::CodeEval),
                            sanitizer: None,
                        },
                    );
                }
            }
        }
        self
    }

    /// Rules for a language id (canonicalized).
    pub fn for_language(&self, language: &str) -> Option<&CompiledLanguageRules> {
        let key = canonical_language_id(language).unwrap_or(language);
        self.by_language.get(key).or_else(|| {
            // typescript shares javascript pack.
            if key == "typescript" {
                self.by_language.get("javascript")
            } else {
                None
            }
        })
    }

    /// Number of languages with compiled rules.
    pub fn language_count(&self) -> usize {
        self.by_language.len()
    }

    fn merge_document(&mut self, doc: &TaintRuleDocument) -> Result<(), TaintRuleError> {
        if doc.schema_version != TAINT_RULE_SCHEMA_VERSION {
            return Err(TaintRuleError::UnsupportedVersion {
                found: doc.schema_version,
                expected: TAINT_RULE_SCHEMA_VERSION,
            });
        }
        for lang in &doc.languages {
            let key = canonical_language_id(lang).unwrap_or(lang).to_string();
            let entry = self.by_language.entry(key).or_default();
            for rule in &doc.rules {
                entry.push_rule(rule)?;
            }
        }
        Ok(())
    }

    fn merge_set(&mut self, other: Self) -> Result<(), TaintRuleError> {
        for (lang, rules) in other.by_language {
            let entry = self.by_language.entry(lang).or_default();
            // Later packs append — first-match-wins means earlier rules keep priority.
            // Project/CLI overlays that should win must be prepended; we append project
            // after built-in so built-in still wins. Spec says built-in < project < CLI
            // meaning later layers override. Prepend other rules.
            let mut merged = CompiledLanguageRules::default();
            merged.sources.extend(rules.sources);
            merged.sources.extend(std::mem::take(&mut entry.sources));
            merged.sinks.extend(rules.sinks);
            merged.sinks.extend(std::mem::take(&mut entry.sinks));
            merged.sanitizers.extend(rules.sanitizers);
            merged
                .sanitizers
                .extend(std::mem::take(&mut entry.sanitizers));
            *entry = merged;
        }
        Ok(())
    }

    fn extend_language_from(&mut self, target: &str, source: &str) -> Result<(), TaintRuleError> {
        let Some(src) = self.by_language.get(source) else {
            return Ok(());
        };
        // Clone source rules then prepend under target (target-specific rules already present
        // should win — they were merged first from cpp.yaml).
        let cloned_sources = clone_compiled_rules(&src.sources);
        let cloned_sinks = clone_compiled_rules(&src.sinks);
        let cloned_sans = clone_compiled_rules(&src.sanitizers);
        let entry = self.by_language.entry(target.to_string()).or_default();
        // Append C rules after C++-specific so C++-specific wins on conflict.
        entry.sources.extend(cloned_sources);
        entry.sinks.extend(cloned_sinks);
        entry.sanitizers.extend(cloned_sans);
        Ok(())
    }
}

fn clone_compiled_rules(rules: &[CompiledRule]) -> Vec<CompiledRule> {
    rules
        .iter()
        .map(|r| CompiledRule {
            matcher: clone_matcher(&r.matcher),
            source: r.source,
            sink: r.sink,
            sanitizer: r.sanitizer.clone(),
        })
        .collect()
}

fn clone_matcher(m: &CompiledMatcher) -> CompiledMatcher {
    match m {
        CompiledMatcher::Substring { value, unless_any } => CompiledMatcher::Substring {
            value: value.clone(),
            unless_any: unless_any.clone(),
        },
        CompiledMatcher::AnySubstring(v) => CompiledMatcher::AnySubstring(v.clone()),
        CompiledMatcher::AllSubstrings(v) => CompiledMatcher::AllSubstrings(v.clone()),
        CompiledMatcher::Regex(re) => {
            CompiledMatcher::Regex(Regex::new(re.as_str()).expect("recompile regex"))
        }
        CompiledMatcher::AllOf(parts) => {
            CompiledMatcher::AllOf(parts.iter().map(clone_matcher).collect())
        }
    }
}

fn parse_document(yaml: &str) -> Result<TaintRuleDocument, TaintRuleError> {
    Ok(serde_yaml::from_str(yaml)?)
}

fn parse_source_kind(kind: &str) -> Option<TaintSource> {
    Some(match kind {
        "HttpParameter" => TaintSource::HttpParameter,
        "FileInput" => TaintSource::FileInput,
        "NetworkInput" => TaintSource::NetworkInput,
        "CommandLineArg" => TaintSource::CommandLineArg,
        "EnvironmentVar" => TaintSource::EnvironmentVar,
        "DatabaseResult" => TaintSource::DatabaseResult,
        _ => return None,
    })
}

fn parse_sink_kind(kind: &str) -> Option<TaintSink> {
    Some(match kind {
        "SqlQuery" => TaintSink::SqlQuery,
        "ShellCommand" => TaintSink::ShellCommand,
        "FileWrite" => TaintSink::FileWrite,
        "NetworkOutput" => TaintSink::NetworkOutput,
        "LogOutput" => TaintSink::LogOutput,
        "HtmlRender" => TaintSink::HtmlRender,
        "CodeEval" => TaintSink::CodeEval,
        _ => return None,
    })
}

fn parse_sanitizer_kind(kind: &str, detail: Option<&str>) -> Option<Sanitizer> {
    Some(match kind {
        "SqlParameterize" => Sanitizer::SqlParameterize,
        "HtmlEscape" => Sanitizer::HtmlEscape,
        "ShellEscape" => Sanitizer::ShellEscape,
        "Validation" => Sanitizer::Validation(detail.unwrap_or("pattern").into()),
        "TypeCast" => Sanitizer::TypeCast(detail.unwrap_or("numeric").into()),
        _ => return None,
    })
}

/// Load bundled CWE catalog (replaces hardcoded `default_cwe_patterns` body).
pub fn bundled_cwe_catalog() -> Result<Vec<CweCatalogEntry>, TaintRuleError> {
    let doc: CweCatalogDocument = serde_yaml::from_str(CWE_CATALOG_YAML)?;
    if doc.schema_version != TAINT_RULE_SCHEMA_VERSION {
        return Err(TaintRuleError::UnsupportedVersion {
            found: doc.schema_version,
            expected: TAINT_RULE_SCHEMA_VERSION,
        });
    }
    Ok(doc.patterns)
}

const BUNDLED_PACKS: &[&str] = &[
    include_str!("../rules/taint/python.yaml"),
    include_str!("../rules/taint/javascript.yaml"),
    include_str!("../rules/taint/rust.yaml"),
    include_str!("../rules/taint/go.yaml"),
    include_str!("../rules/taint/java.yaml"),
    include_str!("../rules/taint/csharp.yaml"),
    include_str!("../rules/taint/c.yaml"),
    include_str!("../rules/taint/cpp.yaml"),
    include_str!("../rules/taint/php.yaml"),
    include_str!("../rules/taint/ruby.yaml"),
    include_str!("../rules/taint/puppet.yaml"),
    include_str!("../rules/taint/kotlin.yaml"),
    include_str!("../rules/taint/groovy.yaml"),
    include_str!("../rules/taint/tier1-extended.yaml"),
];

const CWE_CATALOG_YAML: &str = include_str!("../rules/taint/cwe-catalog.yaml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_loads_all_tier1() {
        let set = TaintRuleSet::bundled().expect("bundled");
        for lang in [
            "python",
            "javascript",
            "typescript",
            "rust",
            "go",
            "java",
            "csharp",
            "c",
            "cpp",
            "php",
            "ruby",
            "puppet",
            "kotlin",
            "groovy",
            "swift",
            "scala",
            "dart",
            "elixir",
            "lua",
            "bash",
        ] {
            assert!(set.for_language(lang).is_some(), "missing rules for {lang}");
        }
    }

    #[test]
    fn python_http_to_sql_classify() {
        let set = TaintRuleSet::bundled().unwrap();
        let rules = set.for_language("python").unwrap();
        let (src, _, _) = rules.classify("q = request.GET['id']");
        assert_eq!(src, Some(TaintSource::HttpParameter));
        let (_, sink2, _) = rules.classify("cursor.execute(q)");
        assert_eq!(sink2, Some(TaintSink::SqlQuery));
    }

    #[test]
    fn reject_bad_schema_version() {
        let yaml = r#"
schema_version: 99
id: bad
languages: [python]
rules: []
"#;
        let err = parse_document(yaml)
            .and_then(|d| {
                let mut s = TaintRuleSet::empty();
                s.merge_document(&d)
            })
            .unwrap_err();
        assert!(matches!(
            err,
            TaintRuleError::UnsupportedVersion { found: 99, .. }
        ));
    }

    #[test]
    fn overlay_prepends_sink() {
        let set = TaintRuleSet::bundled()
            .unwrap()
            .scoped(&["java".into()])
            .with_overlays(&["ObjectMapper.readValue"]);
        let rules = set.for_language("java").unwrap();
        let (_, sink, _) = rules.classify("mapper.ObjectMapper.readValue(json)");
        assert_eq!(sink, Some(TaintSink::CodeEval));
    }

    #[test]
    fn cwe_catalog_has_owasp_core() {
        let patterns = bundled_cwe_catalog().unwrap();
        assert!(patterns.len() >= 10);
        assert!(patterns.iter().any(|p| p.cwe_id == "CWE-89"));
        assert!(patterns.iter().any(|p| p.cwe_id == "CWE-79"));
    }

    #[test]
    fn c_printf_unless_sprintf() {
        let set = TaintRuleSet::bundled().unwrap();
        let rules = set.for_language("c").unwrap();
        let (_, sink, _) = rules.classify("printf(user)");
        assert_eq!(sink, Some(TaintSink::HtmlRender));
        let (_, sink2, _) = rules.classify("snprintf(buf, n, user)");
        assert_ne!(sink2, Some(TaintSink::HtmlRender));
    }

    #[test]
    fn classify_microbench_python_under_budget() {
        let set = TaintRuleSet::bundled().unwrap();
        let rules = set.for_language("python").unwrap();
        let samples = [
            "q = request.GET['id']",
            "cursor.execute(q)",
            "os.system(cmd)",
            "x = int(q)",
            "safe = html.escape(q)",
        ];
        let start = std::time::Instant::now();
        let mut hits = 0u64;
        for _ in 0..50_000 {
            for s in &samples {
                let (a, b, c) = rules.classify(s);
                if a.is_some() || b.is_some() || c.is_some() {
                    hits += 1;
                }
            }
        }
        let elapsed = start.elapsed();
        assert!(hits > 0);
        // Loose budget: 250k classify calls should finish well under 2s on CI/dev.
        assert!(
            elapsed.as_secs_f64() < 2.0,
            "classify microbench too slow: {elapsed:?}"
        );
    }
}
