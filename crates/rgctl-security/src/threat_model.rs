//! Deterministic threat-model evidence extraction, STRIDE candidate synthesis,
//! and Markdown / Mermaid DFD rendering (#229).
//!
//! Design invariants:
//! - **Zero `discover` bloat:** operates 100% read-only on `.rgctl/graph.snapshot.bin`
//!   hot columnar arrays + sidecars (`parked_relations.bin`, `analysis_results.bin`,
//!   `callable_resolution.json`, optional offline OSV results, and lazy per-sink PDG).
//! - **Strict epistemic separation:** directly observed graph/CPG/OSV facts are
//!   never conflated with rule-derived STRIDE hypotheses (`provenance: "graph_rule"`,
//!   `review_status: "needs_review"`). Every threat candidate links to `>= 1`
//!   concrete `evidence_refs` in `evidence_index` and declares explicit `assumptions`.
//! - **Deterministic output:** all collections are canonically sorted so `-f json`,
//!   `-f text`, and `-f mermaid` outputs are reproducible across runs for CI diffing.

use crate::vuln_analyze::{ExploitabilityVerdict, VulnAnalyzeResult};
use rayon::prelude::*;
use rgctl_analysis::{AnalysisResults, CommunityQueryContext, TaintRuleSet, TaintSink};
use rgctl_extraction::ParkedRelationRecord;
use rgctl_graph::columnar_snapshot::ColumnarGraphMmap;
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_plugin_api::RelationType;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::path::Path;
use uuid::Uuid;

/// Schema version for `rgctl -f json vuln threat-model`.
pub const THREAT_MODEL_SCHEMA_VERSION: u32 = 1;

/// Provenance tier distinguishing observed facts from rule/agent inferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceProvenance {
    /// Directly observed symbol or typed edge in `.rgctl/graph.snapshot.bin` / `parked_relations.bin`.
    GraphFact,
    /// Intra-procedural CFG/PDG data-flow or sanitizer result (`TaintAnalyzer`).
    CpgTaint,
    /// Dependency inventory + call-graph/PDG OSV reachability verdict (`vuln analyze/scan`).
    OsvReachability,
    /// Deterministic rule-derived STRIDE candidate over observed graph facts.
    GraphRule,
    /// Downstream LLM or human analyst inference (reserved for `/rgctl:vuln` enrichment).
    AgentInference,
}

/// Analyst review lifecycle status for threat-model records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    /// Observed structural/taint/OSV fact (not an inferred hypothesis).
    Observed,
    /// Rule-derived candidate requiring human or context-aware review.
    NeedsReview,
    /// Confirmed applicable threat.
    Confirmed,
    /// Mitigated by verified in-code or infrastructure control.
    Mitigated,
    /// Risk accepted.
    Accepted,
    /// Dismissed as false positive.
    FalsePositive,
}

/// STRIDE threat categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrideCategory {
    Spoofing,
    Tampering,
    Repudiation,
    InformationDisclosure,
    DenialOfService,
    ElevationOfPrivilege,
}

impl StrideCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Spoofing => "Spoofing",
            Self::Tampering => "Tampering",
            Self::Repudiation => "Repudiation",
            Self::InformationDisclosure => "Information Disclosure",
            Self::DenialOfService => "Denial of Service",
            Self::ElevationOfPrivilege => "Elevation of Privilege",
        }
    }

    pub fn short_code(self) -> &'static str {
        match self {
            Self::Spoofing => "S",
            Self::Tampering => "T",
            Self::Repudiation => "R",
            Self::InformationDisclosure => "I",
            Self::DenialOfService => "D",
            Self::ElevationOfPrivilege => "E",
        }
    }
}

/// Epistemic confidence level for a rule-derived threat candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

impl ConfidenceLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

/// Entry-point surface kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPointKind {
    HttpRoute,
    RpcOrMessageHandler,
    CliEntrypoint,
    /// Timer / cron-triggered job (`@Scheduled`): internally triggered, not network ingress.
    ScheduledJob,
}

/// Externally reachable entry point observed in the graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntryPointEvidence {
    pub id: String,
    pub kind: EntryPointKind,
    pub name: String,
    pub handler_symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_path: Option<String>,
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
    pub observed_auth_controls: Vec<String>,
    pub observed_validation_controls: Vec<String>,
    pub provenance: EvidenceProvenance,
}

/// Classification of sensitive assets observed in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    SecretOrCredentialConfig,
    ConnectionOrEndpointConfig,
    RuntimeConfig,
    DatabaseTable,
    SensitiveDomainModel,
}

/// Sensitive configuration key, table, or domain entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetEvidence {
    pub id: String,
    pub kind: AssetKind,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub consumers: Vec<String>,
    pub provenance: EvidenceProvenance,
}

/// Intra-procedural PDG taint status for a discovered sink call site.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PdgSinkStatus {
    /// Untrusted source reaches the sink within the enclosing function without sanitizer.
    ProvenUnsanitizedFlow { source_line: u32 },
    /// Taint flow to the sink passes through at least one sanitizer.
    SanitizedFlow { sanitizers: Vec<String> },
    /// CFG/PDG evaluated, no intra-procedural untrusted source flowed to this sink.
    NoLocalSourceFlow,
    /// Lazy CFG/PDG was skipped or unavailable for this language/site.
    NotEvaluated,
}

/// Security-sensitive operation / taint sink call site.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SinkEvidence {
    pub id: String,
    pub sink_category: String,
    pub callee: String,
    pub caller: String,
    pub file: String,
    /// Exact statement line for PDG-verified and parked (external) sinks; the caller's start
    /// line for graph-only sinks (`pdg_taint: not_evaluated` / `no_local_source_flow` from a
    /// `Calls` edge), since hot edge columns carry no call-site line.
    pub line: u32,
    pub is_external_call: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
    pub pdg_taint: PdgSinkStatus,
    pub provenance: EvidenceProvenance,
}

/// Trust-boundary candidate kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustBoundaryKind {
    ExternalIngress,
    SubsystemBridge,
    ExternalEgress,
}

/// Candidate trust boundary inferred from ingress, Fast-Leiden communities, or external egress.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustBoundaryCandidate {
    pub id: String,
    pub kind: TrustBoundaryKind,
    pub from_zone: String,
    pub to_zone: String,
    pub crossing_count: usize,
    pub sample_crossings: Vec<String>,
    pub provenance: EvidenceProvenance,
}

/// Observed entry-point-to-sink call and data-flow path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreatDataFlow {
    pub id: String,
    pub entry_point_id: String,
    pub sink_id: String,
    pub sink_category: String,
    pub call_path: Vec<String>,
    pub hops: usize,
    pub crossed_communities: Vec<String>,
    pub has_observed_auth: bool,
    pub has_observed_sanitizer: bool,
    pub has_observed_audit_log: bool,
    pub pdg_taint: PdgSinkStatus,
    pub provenance: EvidenceProvenance,
}

/// Reachable known vulnerability joined from OSV analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReachableVulnEvidence {
    pub id: String,
    pub osv_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
    pub exploitability: ExploitabilityVerdict,
    pub call_sites: Vec<String>,
    pub reaching_entry_points: Vec<String>,
    pub provenance: EvidenceProvenance,
}

/// Canonical addressable evidence record referenced by `ThreatCandidate.evidence_refs`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub id: String,
    pub provenance: EvidenceProvenance,
    pub kind: String,
    pub symbol_or_target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub summary: String,
}

/// Rule-derived STRIDE threat candidate requiring human or `/rgctl:vuln` contextual review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreatCandidate {
    pub id: String,
    pub stride: StrideCategory,
    pub title: String,
    pub description: String,
    pub affected_component: String,
    pub provenance: EvidenceProvenance,
    pub review_status: ReviewStatus,
    pub confidence: ConfidenceLevel,
    pub evidence_refs: Vec<String>,
    pub assumptions: Vec<String>,
    pub suggested_mitigations: Vec<String>,
}

/// Explicit limitation or unindexed coverage note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LimitationNote {
    pub code: String,
    pub category: String,
    pub detail: String,
}

/// High-level counts for quick CLI / CI inspection.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreatModelSummary {
    pub entry_points: usize,
    pub assets: usize,
    pub sinks: usize,
    pub trust_boundaries: usize,
    pub data_flows: usize,
    pub reachable_vulnerabilities: usize,
    pub threat_candidates: usize,
    pub stride_counts: BTreeMap<String, usize>,
    pub unresolved_or_indirect_calls: usize,
}

/// Top-level versioned output of `rgctl -f json vuln threat-model`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreatModelReport {
    pub schema_version: u32,
    pub command: String,
    pub summary: ThreatModelSummary,
    pub entry_points: Vec<EntryPointEvidence>,
    pub assets: Vec<AssetEvidence>,
    pub sinks: Vec<SinkEvidence>,
    pub trust_boundaries: Vec<TrustBoundaryCandidate>,
    pub data_flows: Vec<ThreatDataFlow>,
    pub reachable_vulnerabilities: Vec<ReachableVulnEvidence>,
    pub threat_candidates: Vec<ThreatCandidate>,
    pub evidence_index: BTreeMap<String, EvidenceRecord>,
    pub limitations_and_gaps: Vec<LimitationNote>,
}

/// Request parameters for [`build_threat_model`].
pub struct ThreatModelBuildOpts<'a> {
    pub repo_root: &'a Path,
    pub columnar: &'a ColumnarGraphMmap,
    pub parked_relations: &'a [ParkedRelationRecord],
    pub analysis: Option<&'a AnalysisResults>,
    pub unresolved_calls_count: usize,
    pub vuln_results: &'a [VulnAnalyzeResult],
    pub osv_evaluated: bool,
    pub max_depth: usize,
    /// Budget of entry-reachable functions inspected with lazy CFG/PDG (closest-first).
    /// Functions beyond the budget are reported in `limitations_and_gaps`, never dropped silently.
    pub max_pdg_functions: usize,
}

/// Default for [`ThreatModelBuildOpts::max_pdg_functions`].
pub const DEFAULT_MAX_LAZY_PDG_FUNCTIONS: usize = 64;

/// Request passed to the optional lazy per-function CFG/PDG evaluator callback.
pub struct FunctionPdgRequest<'a> {
    pub function_id: Uuid,
    pub function_fqn: &'a str,
    pub file: &'a str,
    pub start_line: u32,
}

/// Sink discovered or verified inside a function's intra-procedural PDG.
#[derive(Debug, Clone)]
pub struct DiscoveredPdgSink {
    pub sink_kind: TaintSink,
    pub callee: String,
    pub line: u32,
    pub status: PdgSinkStatus,
}

/// Intra-procedural PDG summary returned for one reachable function.
#[derive(Debug, Clone, Default)]
pub struct FunctionPdgReport {
    pub evaluated: bool,
    pub has_sanitizer: bool,
    pub has_audit_log: bool,
    pub sinks: Vec<DiscoveredPdgSink>,
}

#[derive(Clone)]
struct CompactNodeMeta {
    id: Uuid,
    node_type: NodeType,
    name: String,
    qualified_name: Option<String>,
    file: Option<String>,
    start_line: u32,
    is_declaration: bool,
}

impl CompactNodeMeta {
    fn display_fqn(&self) -> &str {
        if self.node_type == NodeType::File {
            return self.file.as_deref().unwrap_or(&self.name);
        }
        self.qualified_name.as_deref().unwrap_or(&self.name)
    }
}

fn clean_symbol_token(raw: &str) -> &str {
    raw.split('\u{1f}').next().unwrap_or(raw).trim()
}

fn parked_callee_candidates(rec: &ParkedRelationRecord) -> Vec<String> {
    let base = clean_symbol_token(&rec.to_name);
    let mut out = Vec::with_capacity(4);
    if let Some(qh) = rec.to_qualified_hint.as_deref().map(clean_symbol_token)
        && !qh.is_empty()
    {
        out.push(qh.to_string());
    }
    for prefix_opt in [&rec.import_module_hint, &rec.to_type_hint] {
        if let Some(raw_prefix) = prefix_opt.as_deref().map(clean_symbol_token)
            && !raw_prefix.is_empty()
        {
            let prefix = raw_prefix
                .rsplit_once(':')
                .map(|(_, r)| r)
                .unwrap_or(raw_prefix);
            if !base.contains('.') && !base.contains("::") {
                let joined = format!("{prefix}.{base}");
                if !out.contains(&joined) {
                    out.push(joined);
                }
            }
        }
    }
    if !base.is_empty() && !out.iter().any(|s| s == base) {
        out.push(base.to_string());
    }
    out
}

fn relativize_path(repo_root: &Path, canon_repo: Option<&Path>, raw: &str) -> String {
    let p = Path::new(raw);
    if let Ok(rel) = p.strip_prefix(repo_root) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    if let Some(canon) = canon_repo {
        if let Ok(rel) = p.strip_prefix(canon) {
            return rel.to_string_lossy().replace('\\', "/");
        }
        if let Ok(stripped_private) = canon.strip_prefix("/private") {
            let alt = Path::new("/").join(stripped_private);
            if let Ok(rel) = p.strip_prefix(&alt) {
                return rel.to_string_lossy().replace('\\', "/");
            }
        }
    }
    raw.replace('\\', "/")
}

fn language_for_file(file: &str) -> &'static str {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" => "rust",
        "py" | "pyw" => "python",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" | "tsx" => "typescript",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "go" => "go",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        "scala" | "sc" => "scala",
        "groovy" | "gradle" => "groovy",
        "swift" => "swift",
        "dart" => "dart",
        "ex" | "exs" => "elixir",
        "lua" => "lua",
        "sh" | "bash" => "bash",
        "pp" => "puppet",
        // Unknown extensions get no language rules (generic fallbacks only) rather than Python's.
        _ => "",
    }
}

fn sink_category_str(sink: TaintSink) -> &'static str {
    match sink {
        TaintSink::SqlQuery => "sql_query",
        TaintSink::ShellCommand => "shell_command",
        TaintSink::FileWrite => "file_write",
        TaintSink::NetworkOutput => "network_output",
        TaintSink::LogOutput => "log_output",
        TaintSink::HtmlRender => "html_render",
        TaintSink::CodeEval => "code_eval",
    }
}

/// Classify a callee/statement expression as a security-sensitive [`TaintSink`].
pub fn classify_callee_sink(rules: &TaintRuleSet, lang: &str, callee: &str) -> Option<TaintSink> {
    let probe = format!("{callee}(");
    if let Some(compiled) = rules.for_language(lang) {
        let (_, sink, _) = compiled.classify(&probe);
        if sink.is_some() {
            return sink;
        }
        let (_, sink2, _) = compiled.classify(callee);
        if sink2.is_some() {
            return sink2;
        }
    }
    let lower = callee.to_ascii_lowercase();
    let tail = lower
        .rsplit(['.', ':', '\\', '/'])
        .next()
        .unwrap_or(lower.as_str());
    let tokens = identifier_tokens(callee);
    // Receiver/module tokens (everything but the method name) qualify generic method names.
    let qualifier_tokens = &tokens[..tokens.len().saturating_sub(1)];
    let sqlish_receiver = qualifier_tokens.iter().any(|t| {
        matches!(
            t.as_str(),
            "cursor"
                | "conn"
                | "connection"
                | "db"
                | "database"
                | "session"
                | "statement"
                | "stmt"
                | "sql"
                | "jdbc"
                | "engine"
                | "pool"
                | "tx"
                | "transaction"
                | "sqlx"
                | "diesel"
                | "query"
                | "queryset"
                | "objects"
        )
    });
    if matches!(
        tail,
        "executequery" | "executeupdate" | "rawquery" | "executesql" | "query_raw"
    ) || (matches!(tail, "execute" | "raw") && sqlish_receiver)
    {
        Some(TaintSink::SqlQuery)
    } else if matches!(tail, "popen" | "execve" | "execvp" | "runtimeshell")
        || lower.contains("command::new")
        || lower.contains("processbuilder")
        || lower.contains("runtime.getruntime().exec")
        || lower.contains("subprocess.")
        || lower.contains("child_process.")
    {
        Some(TaintSink::ShellCommand)
    } else if matches!(tail, "eval" | "unserialize" | "readobject")
        // Bare / builtin `exec(...)` only: `RegExp.exec`, `stmt.exec` are not code evaluation.
        || (tail == "exec" && (qualifier_tokens.is_empty() || qualifier_tokens == ["builtins"]))
        || lower.contains("pickle.loads")
        || lower.contains("yaml.unsafe_load")
    {
        Some(TaintSink::CodeEval)
    } else if lower.contains("fs::write")
        || lower.contains("writefile")
        || lower.contains("files.write")
        || matches!(tail, "write_all" | "write_text" | "write_bytes")
    {
        Some(TaintSink::FileWrite)
    } else if matches!(
        tail,
        "info" | "warn" | "error" | "audit" | "log" | "println"
    ) && tokens.iter().any(|t| {
        matches!(
            t.as_str(),
            "log"
                | "logs"
                | "logger"
                | "logging"
                | "audit"
                | "tracing"
                | "slf4j"
                | "log4j"
                | "logback"
                | "println"
        )
    }) {
        Some(TaintSink::LogOutput)
    } else {
        None
    }
}

/// Classify whether a callee/statement expression acts as an input sanitizer.
pub fn classify_callee_sanitizer(rules: &TaintRuleSet, lang: &str, callee: &str) -> bool {
    let probe = format!("{callee}(");
    if let Some(compiled) = rules.for_language(lang) {
        let (_, _, san) = compiled.classify(&probe);
        if san.is_some() {
            return true;
        }
    }
    let tail = callee
        .rsplit(['.', ':', '\\', '/'])
        .next()
        .unwrap_or(callee)
        .to_ascii_lowercase();
    tail.starts_with("sanitize")
        || tail.starts_with("escape")
        || tail.starts_with("validate")
        || tail == "quote"
        || tail == "shlex_quote"
}

/// `[profile]` lap for `build_threat_model` stages (`RUST_LOG=profile=info`).
fn profile_lap(clock: &mut std::time::Instant, stage: &str) {
    tracing::info!(target: "profile", "[profile] threat-model build {stage}: {:.3}s", clock.elapsed().as_secs_f64());
    *clock = std::time::Instant::now();
}

/// Owner (enclosing type) of a qualified symbol name: `a::B::f` -> `a::B`, `pkg.Cls.m` -> `pkg.Cls`.
fn qualified_owner(qn: &str) -> Option<&str> {
    if let Some((owner, _)) = qn.rsplit_once("::") {
        return Some(owner);
    }
    qn.rsplit_once(['.', '#', '\\', ':'])
        .map(|(owner, _)| owner)
}

fn qualified_owner_short_name(owner: &str) -> &str {
    owner
        .rsplit(['.', ':', '#', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or(owner)
}

/// Lowercased identifier tokens split on separators and camelCase boundaries
/// (`HTTPBearer` -> `http`,`bearer`; `get_current_user` -> `get`,`current`,`user`).
fn identifier_tokens(name: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if c.is_ascii_uppercase() && !cur.is_empty() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            if prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next_lower)
            {
                tokens.push(std::mem::take(&mut cur));
            }
        }
        cur.push(c.to_ascii_lowercase());
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn tokens_match(tokens: &[String], words: &[&str], pairs: &[(&str, &str)]) -> bool {
    tokens.iter().any(|t| words.contains(&t.as_str()))
        || tokens.windows(2).any(|w| {
            pairs.contains(&(w[0].as_str(), w[1].as_str()))
                // Joined pair catches camel splits like `OAuth2` -> `o`,`auth2`.
                || words.contains(&format!("{}{}", w[0], w[1]).as_str())
        })
}

/// Whole-token auth match: `@PreAuthorize`, `Depends(get_current_user)`, `jwt.verify`, `AuthGuard`
/// match; `get_author`, `safeguard`, `authority_level` do not.
fn is_auth_control_name(name: &str) -> bool {
    const WORDS: &[&str] = &[
        "auth",
        "authn",
        "authz",
        "authenticate",
        "authenticated",
        "authentication",
        "authenticator",
        "authorize",
        "authorized",
        "authorization",
        "preauthorize",
        "rolesallowed",
        "secured",
        "permission",
        "permissions",
        "oauth",
        "oauth2",
        "jwt",
        "bearer",
        "csrf",
        "xsrf",
        "apikey",
        "guard",
        "guards",
    ];
    const PAIRS: &[(&str, &str)] = &[
        ("login", "required"),
        ("current", "user"),
        ("roles", "allowed"),
        ("api", "key"),
        ("has", "role"),
        ("has", "authority"),
        ("requires", "role"),
    ];
    tokens_match(&identifier_tokens(name), WORDS, PAIRS)
}

/// Whole-token validation / rate-limit match: `validate_input`, `@Valid`, `RateLimiter` match;
/// `invalidate_cache`, `get_schema_version` do not.
fn is_validation_or_ratelimit_name(name: &str) -> bool {
    const WORDS: &[&str] = &[
        "valid",
        "validate",
        "validated",
        "validates",
        "validation",
        "validator",
        "sanitize",
        "sanitized",
        "sanitizer",
        "throttle",
        "throttled",
        "throttler",
        "ratelimit",
        "ratelimiter",
        "limiter",
    ];
    const PAIRS: &[(&str, &str)] = &[("rate", "limit"), ("rate", "limiter"), ("rate", "limited")];
    tokens_match(&identifier_tokens(name), WORDS, PAIRS)
}

fn is_handler_annotation(name: &str) -> bool {
    let lower = name
        .trim_start_matches('@')
        .rsplit(['.', ':'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "getmapping"
            | "postmapping"
            | "putmapping"
            | "deletemapping"
            | "patchmapping"
            | "requestmapping"
            | "get"
            | "post"
            | "put"
            | "delete"
            | "patch"
            | "messagedriven"
            | "kafkalistener"
            | "rabbitlistener"
            | "sqslistener"
            | "grpcservice"
            | "scheduled"
    )
}

fn classify_config_asset(name: &str) -> AssetKind {
    let upper = name.to_ascii_uppercase();
    if upper.contains("SECRET")
        || upper.contains("TOKEN")
        || upper.contains("PASSWORD")
        || upper.contains("PASSWD")
        || upper.contains("PRIVATE_KEY")
        || upper.contains("API_KEY")
        || upper.contains("APIKEY")
        || upper.contains("CREDENTIAL")
        || upper.contains("JWT")
        || upper.contains("SIGNING_KEY")
    {
        AssetKind::SecretOrCredentialConfig
    } else if upper.contains("URL")
        || upper.contains("URI")
        || upper.contains("HOST")
        || upper.contains("DSN")
        || upper.contains("ENDPOINT")
        || upper.contains("PORT")
        || upper.contains("DB_")
        || upper.contains("REDIS")
        || upper.contains("KAFKA")
    {
        AssetKind::ConnectionOrEndpointConfig
    } else {
        AssetKind::RuntimeConfig
    }
}

fn is_sensitive_domain_model_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "user"
            | "account"
            | "credential"
            | "credentials"
            | "payment"
            | "card"
            | "token"
            | "apikey"
            | "session"
            | "order"
            | "invoice"
            | "wallet"
            | "secret"
    ) || lower.ends_with("credential")
        || lower.ends_with("payment")
        || lower.ends_with("token")
}

fn is_test_or_fixture_noise_file(rel_file: &str) -> bool {
    rel_file.starts_with("tests/")
        || rel_file.contains("/tests/")
        || rel_file.contains("/__tests__/")
        || rel_file.starts_with("src/test/")
        || rel_file.contains("/src/test/")
        || rel_file.ends_with("_test.go")
        || rel_file.ends_with("_test.rs")
        || rel_file.ends_with(".spec.ts")
        || rel_file.ends_with(".test.ts")
        || rel_file.ends_with(".spec.js")
        || rel_file.ends_with(".test.js")
}

/// Build a deterministic [`ThreatModelReport`] directly from hot columnar snapshot arrays.
pub fn build_threat_model<F>(
    opts: ThreatModelBuildOpts<'_>,
    mut inspect_function_pdg: F,
) -> rgctl_error::Result<ThreatModelReport>
where
    F: FnMut(FunctionPdgRequest<'_>) -> FunctionPdgReport,
{
    let col = opts.columnar;
    let canon_repo = opts.repo_root.canonicalize().ok();
    let n_count = col.node_count();
    let mut stage_clock = std::time::Instant::now();

    let mut nodes: Vec<CompactNodeMeta> = Vec::with_capacity(n_count);
    let mut id_to_idx: HashMap<Uuid, usize> = HashMap::with_capacity(n_count);
    let mut class_by_qn: HashMap<String, usize> = HashMap::new();
    let mut class_by_short_name: HashMap<String, Option<usize>> = HashMap::new();

    for idx in 0..n_count {
        let id = col.node_id_at(idx)?;
        let node_type = col.node_type_at(idx)?;
        let name = col.node_name_str(idx)?.to_string();
        let qualified_name = col.node_qualified_name_str(idx)?.map(str::to_string);
        let file = col
            .node_file_path_str(idx)?
            .map(|f| relativize_path(opts.repo_root, canon_repo.as_deref(), f));
        let (start_line, _) = col.node_lines_at(idx)?;
        let is_declaration = col.node_is_declaration_at(idx).unwrap_or(false);

        if matches!(node_type, NodeType::Class | NodeType::Struct) {
            if let Some(qn) = &qualified_name {
                class_by_qn.insert(qn.clone(), idx);
            }
            // Short-name fallback only when unambiguous across the repo.
            class_by_short_name
                .entry(name.clone())
                .and_modify(|slot| *slot = None)
                .or_insert(Some(idx));
        }

        id_to_idx.insert(id, idx);
        nodes.push(CompactNodeMeta {
            id,
            node_type,
            name,
            qualified_name,
            file,
            start_line: start_line.unwrap_or(0) as u32,
            is_declaration,
        });
    }

    profile_lap(&mut stage_clock, "load_nodes");
    // Build typed adjacency lists in a single streaming edge pass.
    let mut calls_out: Vec<Vec<usize>> = vec![Vec::new(); n_count];
    let mut calls_in_count: Vec<usize> = vec![0; n_count];
    let mut exposes_routes: Vec<(usize, usize)> = Vec::new();
    let mut annotations_on: HashMap<usize, Vec<String>> = HashMap::new();
    let mut injects_on: HashMap<usize, Vec<String>> = HashMap::new();
    let mut uses_config_edges: Vec<(usize, usize)> = Vec::new();
    let mut references_out: Vec<Vec<usize>> = vec![Vec::new(); n_count];

    col.for_each_edge(|from, to, edge_type| {
        let (Some(&u), Some(&v)) = (id_to_idx.get(&from), id_to_idx.get(&to)) else {
            return Ok(());
        };
        match edge_type {
            EdgeType::Calls => {
                calls_out[u].push(v);
                calls_in_count[v] += 1;
            }
            EdgeType::ExposesRoute => {
                exposes_routes.push((u, v));
            }
            EdgeType::AnnotatedWith => {
                annotations_on
                    .entry(u)
                    .or_default()
                    .push(nodes[v].name.clone());
            }
            EdgeType::Injects => {
                injects_on
                    .entry(u)
                    .or_default()
                    .push(nodes[v].display_fqn().to_string());
            }
            EdgeType::UsesConfig => {
                uses_config_edges.push((u, v));
            }
            EdgeType::References | EdgeType::Instantiates | EdgeType::Uses => {
                references_out[u].push(v);
            }
            _ => {}
        }
        Ok(())
    })?;

    for adj in &mut calls_out {
        adj.sort_unstable();
        adj.dedup();
    }

    profile_lap(&mut stage_clock, "edge_pass");
    // Community resolution from `.rgctl/analysis_results.bin` when present.
    let community_ctx = opts.analysis.map(|a| {
        CommunityQueryContext::from_analysis(a, |uuid| {
            id_to_idx
                .get(&uuid)
                .map(|&idx| (nodes[idx].name.clone(), nodes[idx].file.as_ref().cloned()))
        })
    });
    let mut community_labels_by_id: HashMap<usize, (String, usize)> = HashMap::new();
    if let Some(cctx) = &community_ctx {
        for c in &cctx.communities {
            community_labels_by_id.insert(c.id, (c.label.clone(), c.member_count));
        }
    }
    // Resolve each node's (non-singleton) community once; hot loops compare ids, not labels.
    let node_community: Vec<Option<usize>> = match &community_ctx {
        Some(cctx) => nodes
            .iter()
            .map(|n| {
                let &cid = cctx.uuid_to_community.get(&n.id)?;
                let (_, count) = community_labels_by_id.get(&cid)?;
                (*count > 1).then_some(cid)
            })
            .collect(),
        None => vec![None; n_count],
    };
    let community_for_idx = |idx: usize| -> Option<String> {
        let cid = node_community[idx]?;
        community_labels_by_id
            .get(&cid)
            .map(|(label, _)| label.clone())
    };

    let mut parked_callees_by_caller: HashMap<usize, Vec<String>> = HashMap::new();
    for rec in opts.parked_relations {
        if rec.relation_type == RelationType::Calls
            && let Some(&u) = id_to_idx.get(&rec.from_node_id)
        {
            if let Some(best) = parked_callee_candidates(rec).into_iter().next() {
                parked_callees_by_caller.entry(u).or_default().push(best);
            }
        }
    }

    // Helper: collect observed auth & validation controls on a handler + its enclosing class + direct callees.
    let controls_for_handler = |h_idx: usize| -> (Vec<String>, Vec<String>) {
        let mut auth = BTreeSet::new();
        let mut validation = BTreeSet::new();
        let mut candidates: Vec<String> = Vec::new();
        if let Some(anns) = annotations_on.get(&h_idx) {
            candidates.extend(anns.iter().cloned());
        }
        if let Some(injs) = injects_on.get(&h_idx) {
            candidates.extend(injs.iter().cloned());
        }
        if let Some(qn) = &nodes[h_idx].qualified_name
            && let Some(owner) = qualified_owner(qn)
            && let Some(cls_idx) = class_by_qn.get(owner).copied().or_else(|| {
                let short = qualified_owner_short_name(owner);
                class_by_short_name.get(short).copied().flatten()
            })
        {
            if let Some(anns) = annotations_on.get(&cls_idx) {
                candidates.extend(anns.iter().cloned());
            }
            if let Some(injs) = injects_on.get(&cls_idx) {
                candidates.extend(injs.iter().cloned());
            }
        }
        for &callee_idx in &calls_out[h_idx] {
            candidates.push(nodes[callee_idx].display_fqn().to_string());
        }
        if let Some(ext_callees) = parked_callees_by_caller.get(&h_idx) {
            candidates.extend(ext_callees.iter().cloned());
        }
        for c in candidates {
            if is_auth_control_name(&c) {
                auth.insert(c.clone());
            }
            if is_validation_or_ratelimit_name(&c) {
                validation.insert(c);
            }
        }
        (auth.into_iter().collect(), validation.into_iter().collect())
    };

    profile_lap(&mut stage_clock, "communities");
    // 1. Discover Entry Points (ExposesRoute -> handler annotations -> CLI `main` entrypoints).
    #[derive(Clone)]
    struct RawEntry {
        handler_idx: usize,
        kind: EntryPointKind,
        name: String,
        http_method: Option<String>,
        route_path: Option<String>,
    }
    let mut raw_entries: Vec<RawEntry> = Vec::new();
    let mut seen_entry_handlers: HashMap<(usize, String), ()> = HashMap::new();
    // Handler indices already registered via `ExposesRoute` (O(1) membership, not a key scan).
    let mut route_handlers: HashSet<usize> = HashSet::new();

    for &(h_idx, route_idx) in &exposes_routes {
        let h = &nodes[h_idx];
        let Some(file) = h.file.as_deref() else {
            continue;
        };
        if file == "<external>" || is_test_or_fixture_noise_file(file) {
            continue;
        }
        let route_label = nodes[route_idx].name.clone();
        let (http_method, route_path) = match route_label.split_once(' ') {
            Some((m, p)) => (Some(m.trim().to_string()), Some(p.trim().to_string())),
            None => (None, Some(route_label.clone())),
        };
        route_handlers.insert(h_idx);
        if seen_entry_handlers
            .insert((h_idx, route_label.clone()), ())
            .is_none()
        {
            raw_entries.push(RawEntry {
                handler_idx: h_idx,
                kind: EntryPointKind::HttpRoute,
                name: route_label,
                http_method,
                route_path,
            });
        }
    }

    let mut annotated_handlers: Vec<usize> = annotations_on.keys().copied().collect();
    annotated_handlers.sort_unstable();
    for h_idx in annotated_handlers {
        let anns = &annotations_on[&h_idx];
        let h = &nodes[h_idx];
        if h.node_type != NodeType::Function || h.is_declaration {
            continue;
        }
        let Some(file) = h.file.as_deref() else {
            continue;
        };
        if file == "<external>" || is_test_or_fixture_noise_file(file) {
            continue;
        }
        if route_handlers.contains(&h_idx) {
            continue;
        }
        if let Some(ann) = anns.iter().find(|a| is_handler_annotation(a)) {
            let label = format!("@{ann} {}", h.display_fqn());
            if seen_entry_handlers
                .insert((h_idx, label.clone()), ())
                .is_none()
            {
                let ann_lower = ann.to_ascii_lowercase();
                let kind = if ann_lower.contains("scheduled") {
                    EntryPointKind::ScheduledJob
                } else if ann_lower.contains("listener")
                    || ann_lower.contains("message")
                    || ann_lower.contains("grpc")
                {
                    EntryPointKind::RpcOrMessageHandler
                } else {
                    EntryPointKind::HttpRoute
                };
                raw_entries.push(RawEntry {
                    handler_idx: h_idx,
                    kind,
                    name: label,
                    http_method: None,
                    route_path: None,
                });
            }
        }
    }

    // CLI / batch entrypoints (`main`, `__main__`, CLI command dispatchers).
    for (idx, n) in nodes.iter().enumerate() {
        if n.node_type != NodeType::Function || n.is_declaration {
            continue;
        }
        let Some(file) = n.file.as_deref() else {
            continue;
        };
        if file == "<external>" || is_test_or_fixture_noise_file(file) {
            continue;
        }
        let is_cli_main = n.name == "main"
            || (raw_entries.is_empty()
                && calls_in_count[idx] == 0
                && matches!(
                    n.name.as_str(),
                    "run" | "execute" | "cli" | "dispatch" | "entrypoint"
                ));
        if is_cli_main {
            let label = format!("CLI {}", n.display_fqn());
            if seen_entry_handlers
                .insert((idx, label.clone()), ())
                .is_none()
            {
                raw_entries.push(RawEntry {
                    handler_idx: idx,
                    kind: EntryPointKind::CliEntrypoint,
                    name: label,
                    http_method: None,
                    route_path: None,
                });
            }
        }
    }

    raw_entries.sort_by(|a, b| {
        let na = &nodes[a.handler_idx];
        let nb = &nodes[b.handler_idx];
        (na.file.as_deref(), na.start_line, &a.name).cmp(&(
            nb.file.as_deref(),
            nb.start_line,
            &b.name,
        ))
    });

    let mut evidence_index: BTreeMap<String, EvidenceRecord> = BTreeMap::new();
    let mut entry_points: Vec<EntryPointEvidence> = Vec::with_capacity(raw_entries.len());
    let mut entry_handler_indices: Vec<(usize, usize)> = Vec::with_capacity(raw_entries.len()); // (ep_vec_pos, node_idx)

    for (pos, raw) in raw_entries.into_iter().enumerate() {
        let ep_id = format!("EP-{:03}", pos + 1);
        let h = &nodes[raw.handler_idx];
        let file = h.file.clone().unwrap_or_default();
        let (auth, validation) = controls_for_handler(raw.handler_idx);
        let community = community_for_idx(raw.handler_idx);
        evidence_index.insert(
            ep_id.clone(),
            EvidenceRecord {
                id: ep_id.clone(),
                provenance: EvidenceProvenance::GraphFact,
                kind: "entry_point".into(),
                symbol_or_target: h.display_fqn().to_string(),
                file: Some(file.clone()),
                line: Some(h.start_line),
                summary: format!(
                    "Entry point `{}` handled by `{}`",
                    raw.name,
                    h.display_fqn()
                ),
            },
        );
        entry_points.push(EntryPointEvidence {
            id: ep_id,
            kind: raw.kind,
            name: raw.name,
            handler_symbol: h.display_fqn().to_string(),
            http_method: raw.http_method,
            route_path: raw.route_path,
            file,
            line: h.start_line,
            community,
            observed_auth_controls: auth,
            observed_validation_controls: validation,
            provenance: EvidenceProvenance::GraphFact,
        });
        entry_handler_indices.push((pos, raw.handler_idx));
    }

    profile_lap(&mut stage_clock, "entry_points");
    // 2. Bounded BFS from each entry point to find reachable functions & shortest call paths.
    let max_depth = opts.max_depth.max(1);
    struct ReachableTree {
        ep_pos: usize,
        parents: HashMap<usize, Option<usize>>,
        depths: HashMap<usize, usize>,
        has_audit_log: bool,
    }
    let mut reachable_trees: Vec<ReachableTree> = Vec::with_capacity(entry_handler_indices.len());
    let mut min_depth_by_node: HashMap<usize, usize> = HashMap::new();
    // Callees left unexplored because a BFS frontier hit `max_depth` (surfaced as a limitation).
    let mut depth_truncated_nodes: HashSet<usize> = HashSet::new();

    for &(ep_pos, start_idx) in &entry_handler_indices {
        let mut parents: HashMap<usize, Option<usize>> = HashMap::new();
        let mut depths: HashMap<usize, usize> = HashMap::new();
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
        parents.insert(start_idx, None);
        depths.insert(start_idx, 0);
        queue.push_back((start_idx, 0));

        while let Some((curr, d)) = queue.pop_front() {
            min_depth_by_node
                .entry(curr)
                .and_modify(|prev| *prev = (*prev).min(d))
                .or_insert(d);
            if d >= max_depth {
                if calls_out[curr]
                    .iter()
                    .any(|next| !parents.contains_key(next))
                {
                    depth_truncated_nodes.insert(curr);
                }
                continue;
            }
            for &next in &calls_out[curr] {
                if let std::collections::hash_map::Entry::Vacant(e) = parents.entry(next) {
                    e.insert(Some(curr));
                    depths.insert(next, d + 1);
                    queue.push_back((next, d + 1));
                }
            }
        }
        reachable_trees.push(ReachableTree {
            ep_pos,
            parents,
            depths,
            has_audit_log: false,
        });
    }

    profile_lap(&mut stage_clock, "bfs");
    // 3. Discover Sinks (graph `Calls` edges + external `parked_relations.bin` + lazy per-function
    //    PDG inspection on the `max_pdg_functions` closest entry-reachable functions).
    let project_overlay_rules = opts
        .repo_root
        .join(".rgctl")
        .join("taint-rules.d")
        .is_dir()
        .then(|| TaintRuleSet::load_with_overlays(Some(opts.repo_root), None, None).ok())
        .flatten();
    let empty_rules = TaintRuleSet::empty();
    let taint_rules: &TaintRuleSet = project_overlay_rules
        .as_ref()
        .or_else(|| TaintRuleSet::bundled_shared().as_ref().ok())
        .unwrap_or(&empty_rules);

    #[derive(Clone)]
    struct RawSink {
        caller_idx: usize,
        sink_kind: TaintSink,
        callee: String,
        file: String,
        line: u32,
        is_external: bool,
        pdg_taint: PdgSinkStatus,
    }
    let mut raw_sinks: Vec<RawSink> = Vec::new();
    // Map `(caller_idx, sink_category, line)` -> index in `raw_sinks` so PDG-verified sinks
    // enrich or add to graph/parked sinks without duplicating the same statement line.
    let mut sink_slot_by_loc: HashMap<(usize, String, u32), usize> = HashMap::new();
    let mut node_has_audit_log: Vec<bool> = vec![false; n_count];
    let mut node_has_sanitizer: Vec<bool> = vec![false; n_count];

    let max_pdg_functions = opts.max_pdg_functions;
    let mut pdg_budget_skipped: usize = 0;
    let mut ordered_reachable: Vec<(usize, usize)> = min_depth_by_node.into_iter().collect();
    ordered_reachable.sort_by(|&(idx_a, d_a), &(idx_b, d_b)| {
        let na = &nodes[idx_a];
        let nb = &nodes[idx_b];
        (d_a, na.file.as_deref(), na.start_line, na.display_fqn()).cmp(&(
            d_b,
            nb.file.as_deref(),
            nb.start_line,
            nb.display_fqn(),
        ))
    });

    let mut pdg_evaluated_nodes: BTreeSet<usize> = BTreeSet::new();
    for (fn_idx, _) in ordered_reachable {
        let n = &nodes[fn_idx];
        if n.node_type != NodeType::Function || n.is_declaration {
            continue;
        }
        let Some(file) = n.file.as_deref() else {
            continue;
        };
        if file == "<external>" || is_test_or_fixture_noise_file(file) {
            continue;
        }
        if pdg_evaluated_nodes.len() >= max_pdg_functions {
            pdg_budget_skipped += 1;
            continue;
        }
        pdg_evaluated_nodes.insert(fn_idx);
        let rep = inspect_function_pdg(FunctionPdgRequest {
            function_id: n.id,
            function_fqn: n.display_fqn(),
            file,
            start_line: n.start_line,
        });
        if rep.has_sanitizer {
            node_has_sanitizer[fn_idx] = true;
        }
        if rep.has_audit_log {
            node_has_audit_log[fn_idx] = true;
        }
        for ds in rep.sinks {
            let cat = sink_category_str(ds.sink_kind).to_string();
            let loc_key = (fn_idx, cat, ds.line);
            if let Some(&slot) = sink_slot_by_loc.get(&loc_key) {
                raw_sinks[slot].pdg_taint = ds.status;
                raw_sinks[slot].callee = ds.callee;
            } else {
                let slot = raw_sinks.len();
                sink_slot_by_loc.insert(loc_key, slot);
                raw_sinks.push(RawSink {
                    caller_idx: fn_idx,
                    sink_kind: ds.sink_kind,
                    callee: ds.callee,
                    file: file.to_string(),
                    line: ds.line,
                    is_external: true,
                    pdg_taint: ds.status,
                });
            }
        }
    }

    // (function, category) pairs already covered by a PDG-verified sink (exact statement line);
    // graph-only edges add sinks only where the PDG found none. O(1) lookups, not a per-edge scan.
    let pdg_fn_cats: HashSet<(usize, &'static str)> = raw_sinks
        .iter()
        .map(|s| (s.caller_idx, sink_category_str(s.sink_kind)))
        .collect();
    let mut graph_sink_seen: HashSet<(usize, usize)> = HashSet::new();
    // Classification depends only on (callee, language): memoize it. Millions of `Calls` edges
    // share a few hundred thousand distinct callees, and each classify call allocates.
    let mut graph_classify_memo: HashMap<(usize, &'static str), (Option<TaintSink>, bool)> =
        HashMap::new();
    for (u, callees) in calls_out.iter().enumerate() {
        let caller = &nodes[u];
        let Some(file) = caller.file.as_deref() else {
            continue;
        };
        if file == "<external>" || is_test_or_fixture_noise_file(file) {
            continue;
        }
        let lang = language_for_file(file);
        for &v in callees {
            let callee_node = &nodes[v];
            let callee_label = callee_node.display_fqn();
            let (sink_opt, is_sanitizer) =
                *graph_classify_memo.entry((v, lang)).or_insert_with(|| {
                    (
                        classify_callee_sink(taint_rules, lang, callee_label),
                        classify_callee_sanitizer(taint_rules, lang, callee_label),
                    )
                });
            if is_sanitizer {
                node_has_sanitizer[u] = true;
            }
            if let Some(sink_kind) = sink_opt {
                let cat_str = sink_category_str(sink_kind);
                // One graph sink per distinct (caller, callee); hot edge columns carry no
                // call-site line, so `line` is the caller's start line for these sinks.
                if !pdg_fn_cats.contains(&(u, cat_str)) && graph_sink_seen.insert((u, v)) {
                    let loc_key = (u, cat_str.to_string(), caller.start_line);
                    let slot = raw_sinks.len();
                    sink_slot_by_loc.insert(loc_key, slot);
                    raw_sinks.push(RawSink {
                        caller_idx: u,
                        sink_kind,
                        callee: callee_label.to_string(),
                        file: file.to_string(),
                        line: caller.start_line,
                        is_external: callee_node.file.as_deref() == Some("<external>"),
                        pdg_taint: if pdg_evaluated_nodes.contains(&u) {
                            PdgSinkStatus::NoLocalSourceFlow
                        } else {
                            PdgSinkStatus::NotEvaluated
                        },
                    });
                }
            }
        }
    }

    // Per-file relativization and per-(candidate, language) classification are memoized:
    // parked call records repeat the same files and callee names many times over.
    // Parked (external) calls: classify in parallel (records are independent; per-worker memos
    // for file relativization and per-(candidate, language) classification), then apply hits
    // sequentially in record order so sink slots and output stay deterministic.
    struct ParkedHit {
        caller_idx: usize,
        rel_file: String,
        call_site_line: u32,
        any_sanitizer: bool,
        matched: Option<(TaintSink, String)>,
    }
    type RelMemo<'a> = HashMap<&'a str, Option<(String, &'static str)>>;
    type ClassifyMemo = HashMap<(&'static str, String), (Option<TaintSink>, bool)>;
    let parked_hits: Vec<ParkedHit> = opts
        .parked_relations
        .par_iter()
        .map_init(
            || (RelMemo::new(), ClassifyMemo::new()),
            |(rel_memo, cls_memo), rec| -> Option<ParkedHit> {
                if rec.relation_type != RelationType::Calls {
                    return None;
                }
                let &caller_idx = id_to_idx.get(&rec.from_node_id)?;
                let (rel_file, lang) = rel_memo
                    .entry(rec.from_file.as_str())
                    .or_insert_with(|| {
                        let rel =
                            relativize_path(opts.repo_root, canon_repo.as_deref(), &rec.from_file);
                        (!is_test_or_fixture_noise_file(&rel)).then(|| {
                            let lang = language_for_file(&rel);
                            (rel, lang)
                        })
                    })
                    .clone()?;
                let mut any_sanitizer = false;
                let mut matched: Option<(TaintSink, String)> = None;
                for cand in parked_callee_candidates(rec) {
                    let key = (lang, cand);
                    let (sink_opt, is_sanitizer) = match cls_memo.get(&key) {
                        Some(&hit) => hit,
                        None => {
                            let res = (
                                classify_callee_sink(taint_rules, lang, &key.1),
                                classify_callee_sanitizer(taint_rules, lang, &key.1),
                            );
                            cls_memo.insert(key.clone(), res);
                            res
                        }
                    };
                    any_sanitizer |= is_sanitizer;
                    if matched.is_none()
                        && let Some(sk) = sink_opt
                    {
                        matched = Some((sk, key.1));
                    }
                }
                (any_sanitizer || matched.is_some()).then_some(ParkedHit {
                    caller_idx,
                    rel_file,
                    call_site_line: rec.call_site_line,
                    any_sanitizer,
                    matched,
                })
            },
        )
        .flatten()
        .collect();
    for hit in parked_hits {
        let ParkedHit {
            caller_idx,
            rel_file,
            call_site_line,
            any_sanitizer,
            matched,
        } = hit;
        if any_sanitizer {
            node_has_sanitizer[caller_idx] = true;
        }
        if let Some((sink_kind, callee_candidate)) = matched {
            let cat = sink_category_str(sink_kind).to_string();
            let loc_key = (caller_idx, cat, call_site_line);
            if let std::collections::hash_map::Entry::Vacant(e) = sink_slot_by_loc.entry(loc_key) {
                let slot = raw_sinks.len();
                e.insert(slot);
                raw_sinks.push(RawSink {
                    caller_idx,
                    sink_kind,
                    callee: callee_candidate,
                    file: rel_file,
                    line: call_site_line,
                    is_external: true,
                    pdg_taint: if pdg_evaluated_nodes.contains(&caller_idx) {
                        PdgSinkStatus::NoLocalSourceFlow
                    } else {
                        PdgSinkStatus::NotEvaluated
                    },
                });
            }
        }
    }

    raw_sinks.sort_by(|a, b| (&a.file, a.line, &a.callee).cmp(&(&b.file, b.line, &b.callee)));

    let mut sinks_by_caller: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (s_pos, rs) in raw_sinks.iter().enumerate() {
        sinks_by_caller
            .entry(rs.caller_idx)
            .or_default()
            .push(s_pos);
        if rs.sink_kind == TaintSink::LogOutput {
            node_has_audit_log[rs.caller_idx] = true;
        }
    }
    for tree in &mut reachable_trees {
        tree.has_audit_log = tree.parents.keys().any(|&idx| node_has_audit_log[idx]);
    }

    let mut sinks: Vec<SinkEvidence> = Vec::with_capacity(raw_sinks.len());
    for (pos, rs) in raw_sinks.into_iter().enumerate() {
        let sink_id = format!("SNK-{:03}", pos + 1);
        let caller = &nodes[rs.caller_idx];
        let pdg_taint = rs.pdg_taint;
        let provenance = match &pdg_taint {
            PdgSinkStatus::ProvenUnsanitizedFlow { .. } | PdgSinkStatus::SanitizedFlow { .. } => {
                EvidenceProvenance::CpgTaint
            }
            _ => EvidenceProvenance::GraphFact,
        };
        let cat = sink_category_str(rs.sink_kind).to_string();
        evidence_index.insert(
            sink_id.clone(),
            EvidenceRecord {
                id: sink_id.clone(),
                provenance,
                kind: format!("sink:{cat}"),
                symbol_or_target: rs.callee.clone(),
                file: Some(rs.file.clone()),
                line: Some(rs.line),
                summary: format!(
                    "Sink `{}` (`{cat}`) invoked in `{}` ({}:{})",
                    rs.callee,
                    caller.display_fqn(),
                    rs.file,
                    rs.line
                ),
            },
        );
        sinks.push(SinkEvidence {
            id: sink_id,
            sink_category: cat,
            callee: rs.callee,
            caller: caller.display_fqn().to_string(),
            file: rs.file,
            line: rs.line,
            is_external_call: rs.is_external,
            community: community_for_idx(rs.caller_idx),
            pdg_taint,
            provenance,
        });
    }

    profile_lap(&mut stage_clock, "sinks");
    // 4. Assets (`UsesConfig` / `ConfigKey`, `Table`, and sensitive domain models touched by reachable code).
    let mut config_consumers: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for &(u, cfg_idx) in &uses_config_edges {
        if let Some(f) = nodes[u].file.as_deref()
            && is_test_or_fixture_noise_file(f)
        {
            continue;
        }
        config_consumers
            .entry(cfg_idx)
            .or_default()
            .insert(nodes[u].display_fqn().to_string());
    }
    for (idx, n) in nodes.iter().enumerate() {
        if n.node_type == NodeType::ConfigKey {
            if let Some(f) = n.file.as_deref()
                && (is_test_or_fixture_noise_file(f)
                    || f.ends_with("correctness-manifest.json")
                    || n.name.starts_with("files."))
            {
                continue;
            }
            if matches!(
                classify_config_asset(&n.name),
                AssetKind::SecretOrCredentialConfig | AssetKind::ConnectionOrEndpointConfig
            ) {
                config_consumers.entry(idx).or_default();
            }
        }
    }

    let mut assets: Vec<AssetEvidence> = Vec::new();
    for (cfg_idx, consumers) in config_consumers {
        let n = &nodes[cfg_idx];
        let kind = classify_config_asset(&n.name);
        let id = format!("AST-{:03}", assets.len() + 1);
        let consumer_list: Vec<String> = consumers.into_iter().collect();
        evidence_index.insert(
            id.clone(),
            EvidenceRecord {
                id: id.clone(),
                provenance: EvidenceProvenance::GraphFact,
                kind: "asset".into(),
                symbol_or_target: n.name.clone(),
                file: n.file.clone(),
                line: (n.start_line > 0).then_some(n.start_line),
                summary: format!("Configuration asset `{}` ({kind:?})", n.name),
            },
        );
        assets.push(AssetEvidence {
            id,
            kind,
            name: n.name.clone(),
            file: n.file.clone(),
            line: (n.start_line > 0).then_some(n.start_line),
            consumers: consumer_list,
            provenance: EvidenceProvenance::GraphFact,
        });
    }

    // Tables & sensitive domain models referenced from entry-reachable functions.
    let mut all_reachable_nodes: BTreeSet<usize> = BTreeSet::new();
    for tree in &reachable_trees {
        all_reachable_nodes.extend(tree.parents.keys().copied());
    }
    let mut domain_consumers: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for &u in &all_reachable_nodes {
        for &target_idx in &references_out[u] {
            let tn = &nodes[target_idx];
            if tn.node_type == NodeType::Table
                || (matches!(tn.node_type, NodeType::Class | NodeType::Struct)
                    && is_sensitive_domain_model_name(&tn.name))
            {
                domain_consumers
                    .entry(target_idx)
                    .or_default()
                    .insert(nodes[u].display_fqn().to_string());
            }
        }
    }
    for (idx, consumers) in domain_consumers {
        let n = &nodes[idx];
        let kind = if n.node_type == NodeType::Table {
            AssetKind::DatabaseTable
        } else {
            AssetKind::SensitiveDomainModel
        };
        let id = format!("AST-{:03}", assets.len() + 1);
        evidence_index.insert(
            id.clone(),
            EvidenceRecord {
                id: id.clone(),
                provenance: EvidenceProvenance::GraphFact,
                kind: "asset".into(),
                symbol_or_target: n.display_fqn().to_string(),
                file: n.file.clone(),
                line: (n.start_line > 0).then_some(n.start_line),
                summary: format!("Data asset `{}` ({kind:?})", n.display_fqn()),
            },
        );
        assets.push(AssetEvidence {
            id,
            kind,
            name: n.display_fqn().to_string(),
            file: n.file.clone(),
            line: (n.start_line > 0).then_some(n.start_line),
            consumers: consumers.into_iter().collect(),
            provenance: EvidenceProvenance::GraphFact,
        });
    }

    profile_lap(&mut stage_clock, "assets");
    // 5. Data Flows (Entry Point -> Sink paths) & Trust Boundaries.
    let mut data_flows: Vec<ThreatDataFlow> = Vec::new();
    let mut pending_flows: Vec<(ThreatDataFlow, EvidenceRecord)> = Vec::new();
    for tree in &reachable_trees {
        let ep = &entry_points[tree.ep_pos];
        let has_auth = !ep.observed_auth_controls.is_empty();

        for (&caller_idx, sink_positions) in &sinks_by_caller {
            let Some(&depth) = tree.depths.get(&caller_idx) else {
                continue;
            };
            // Reconstruct shortest call path from entry handler to `caller_idx`.
            let mut chain_indices = Vec::with_capacity(depth + 1);
            let mut cursor = Some(caller_idx);
            while let Some(c) = cursor {
                chain_indices.push(c);
                cursor = tree.parents.get(&c).copied().flatten();
            }
            chain_indices.reverse();

            let call_path: Vec<String> = chain_indices
                .iter()
                .map(|&idx| nodes[idx].display_fqn().to_string())
                .collect();
            let mut crossed_communities: Vec<String> = Vec::new();
            for &idx in &chain_indices {
                if let Some(c) = community_for_idx(idx)
                    && crossed_communities.last() != Some(&c)
                {
                    crossed_communities.push(c);
                }
            }
            let path_has_sanitizer = chain_indices.iter().any(|&idx| node_has_sanitizer[idx])
                || !ep.observed_validation_controls.is_empty();

            for &sp in sink_positions {
                let sink = &sinks[sp];
                if sink.sink_category == "log_output" {
                    continue;
                }
                let has_san = path_has_sanitizer
                    || matches!(sink.pdg_taint, PdgSinkStatus::SanitizedFlow { .. });
                let provenance = match &sink.pdg_taint {
                    PdgSinkStatus::ProvenUnsanitizedFlow { .. }
                    | PdgSinkStatus::SanitizedFlow { .. } => EvidenceProvenance::CpgTaint,
                    _ => EvidenceProvenance::GraphFact,
                };
                // IDs are assigned after the canonical sort below; the evidence record
                // travels with its flow so renumbering can never cross-wire summaries.
                let record = EvidenceRecord {
                    id: String::new(),
                    provenance,
                    kind: "data_flow".into(),
                    symbol_or_target: format!("{} -> {}", ep.name, sink.callee),
                    file: Some(sink.file.clone()),
                    line: Some(sink.line),
                    summary: format!(
                        "Flow `{}` -> `{}` ({} hops, sink `{}`)",
                        ep.name, sink.callee, depth, sink.sink_category
                    ),
                };
                pending_flows.push((
                    ThreatDataFlow {
                        id: String::new(),
                        entry_point_id: ep.id.clone(),
                        sink_id: sink.id.clone(),
                        sink_category: sink.sink_category.clone(),
                        call_path: call_path.clone(),
                        hops: depth,
                        crossed_communities: crossed_communities.clone(),
                        has_observed_auth: has_auth,
                        has_observed_sanitizer: has_san,
                        has_observed_audit_log: tree.has_audit_log,
                        pdg_taint: sink.pdg_taint.clone(),
                        provenance,
                    },
                    record,
                ));
            }
        }
    }
    pending_flows.sort_by(|(a, _), (b, _)| {
        (&a.entry_point_id, &a.sink_id, a.hops, &a.call_path).cmp(&(
            &b.entry_point_id,
            &b.sink_id,
            b.hops,
            &b.call_path,
        ))
    });
    // Assign DF IDs exactly once, in canonical order, together with their evidence records.
    data_flows.reserve(pending_flows.len());
    for (i, (mut df, mut record)) in pending_flows.into_iter().enumerate() {
        let id = format!("DF-{:03}", i + 1);
        record.id = id.clone();
        df.id = id.clone();
        evidence_index.insert(id, record);
        data_flows.push(df);
    }

    profile_lap(&mut stage_clock, "data_flows");
    // Trust Boundaries: (a) External Ingress, (b) Cross-Community Bridges, (c) External Egress.
    let mut trust_boundaries: Vec<TrustBoundaryCandidate> = Vec::new();
    if !entry_points.is_empty() {
        let mut by_zone: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for ep in &entry_points {
            let zone = ep
                .community
                .clone()
                .unwrap_or_else(|| "Application".to_string());
            by_zone
                .entry(zone)
                .or_default()
                .push(format!("{} -> {}", ep.name, ep.handler_symbol));
        }
        for (zone, samples) in by_zone {
            let tb_id = format!("TB-{:03}", trust_boundaries.len() + 1);
            let count = samples.len();
            let sample_crossings: Vec<String> = samples.into_iter().take(5).collect();
            evidence_index.insert(
                tb_id.clone(),
                EvidenceRecord {
                    id: tb_id.clone(),
                    provenance: EvidenceProvenance::GraphFact,
                    kind: "trust_boundary".into(),
                    symbol_or_target: format!("External -> {zone}"),
                    file: None,
                    line: None,
                    summary: format!("External ingress into `{zone}` ({count} entry point(s))"),
                },
            );
            trust_boundaries.push(TrustBoundaryCandidate {
                id: tb_id,
                kind: TrustBoundaryKind::ExternalIngress,
                from_zone: "External Caller".into(),
                to_zone: zone,
                crossing_count: count,
                sample_crossings,
                provenance: EvidenceProvenance::GraphFact,
            });
        }
    }

    // Per community pair: total crossing count + first 5 sample edges (edge order is the
    // deterministic snapshot order). Only the kept samples are formatted.
    /// (crossing count, first sample `(caller, callee)` node indices).
    type BridgeTally = (usize, Vec<(usize, usize)>);
    let mut bridge_counts: HashMap<(usize, usize), BridgeTally> = HashMap::new();
    for (u, callees) in calls_out.iter().enumerate() {
        let Some(c_from) = node_community[u] else {
            continue;
        };
        for &v in callees {
            let Some(c_to) = node_community[v] else {
                continue;
            };
            if c_from != c_to {
                let (count, samples) = bridge_counts.entry((c_from, c_to)).or_default();
                *count += 1;
                if samples.len() < 5 {
                    samples.push((u, v));
                }
            }
        }
    }
    let label_of = |cid: usize| -> String {
        community_labels_by_id
            .get(&cid)
            .map(|(label, _)| label.clone())
            .unwrap_or_default()
    };
    let mut community_bridges: BTreeMap<(String, String), (usize, Vec<String>)> = BTreeMap::new();
    for ((c_from, c_to), (count, samples)) in bridge_counts {
        let entry = community_bridges
            .entry((label_of(c_from), label_of(c_to)))
            .or_default();
        entry.0 += count;
        entry.1.extend(
            samples
                .into_iter()
                .map(|(u, v)| format!("{} -> {}", nodes[u].display_fqn(), nodes[v].display_fqn())),
        );
    }
    for ((from_zone, to_zone), (count, mut samples)) in community_bridges {
        let tb_id = format!("TB-{:03}", trust_boundaries.len() + 1);
        samples.sort();
        samples.dedup();
        let sample_crossings: Vec<String> = samples.into_iter().take(5).collect();
        evidence_index.insert(
            tb_id.clone(),
            EvidenceRecord {
                id: tb_id.clone(),
                provenance: EvidenceProvenance::GraphFact,
                kind: "trust_boundary".into(),
                symbol_or_target: format!("{from_zone} -> {to_zone}"),
                file: None,
                line: None,
                summary: format!("Cross-community call boundary `{from_zone}` -> `{to_zone}`"),
            },
        );
        trust_boundaries.push(TrustBoundaryCandidate {
            id: tb_id,
            kind: TrustBoundaryKind::SubsystemBridge,
            from_zone,
            to_zone,
            crossing_count: count,
            sample_crossings,
            provenance: EvidenceProvenance::GraphFact,
        });
    }

    let mut egress_by_cat: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for s in &sinks {
        if s.is_external_call && s.sink_category != "log_output" {
            egress_by_cat
                .entry(s.sink_category.clone())
                .or_default()
                .push(format!("{} -> {}", s.caller, s.callee));
        }
    }
    for (cat, samples) in egress_by_cat {
        let tb_id = format!("TB-{:03}", trust_boundaries.len() + 1);
        let count = samples.len();
        let sample_crossings: Vec<String> = samples.into_iter().take(5).collect();
        evidence_index.insert(
            tb_id.clone(),
            EvidenceRecord {
                id: tb_id.clone(),
                provenance: EvidenceProvenance::GraphFact,
                kind: "trust_boundary".into(),
                symbol_or_target: format!("Application -> external:{cat}"),
                file: None,
                line: None,
                summary: format!("Outbound external boundary to `{cat}` ({count} call site(s))"),
            },
        );
        trust_boundaries.push(TrustBoundaryCandidate {
            id: tb_id,
            kind: TrustBoundaryKind::ExternalEgress,
            from_zone: "Application".into(),
            to_zone: format!("External ({cat})"),
            crossing_count: count,
            sample_crossings,
            provenance: EvidenceProvenance::GraphFact,
        });
    }

    profile_lap(&mut stage_clock, "trust_boundaries");
    // 6. Reachable OSV Vulnerabilities join.
    let mut reachable_vulnerabilities: Vec<ReachableVulnEvidence> = Vec::new();
    for vr in opts.vuln_results {
        if !matches!(
            vr.exploitability,
            ExploitabilityVerdict::Exploitable | ExploitabilityVerdict::UnderInvestigation
        ) {
            continue;
        }
        let osv_ref_id = format!("OSV-{:03}", reachable_vulnerabilities.len() + 1);
        let purl = vr
            .openvex
            .statements
            .first()
            .and_then(|st| st.products.first())
            .cloned();
        let mut sites = Vec::new();
        let mut reaching_eps = BTreeSet::new();
        if let Some(st) = &vr.sink_taint {
            for cs in &st.call_sites {
                sites.push(format!(
                    "{} -> {} ({}:{})",
                    cs.caller, cs.target_method, cs.file, cs.line
                ));
                for tree in &reachable_trees {
                    let ep = &entry_points[tree.ep_pos];
                    if tree.parents.keys().any(|&idx| {
                        nodes[idx].display_fqn() == cs.caller || nodes[idx].name == cs.caller
                    }) {
                        reaching_eps.insert(ep.id.clone());
                    }
                }
            }
        }
        evidence_index.insert(
            osv_ref_id.clone(),
            EvidenceRecord {
                id: osv_ref_id.clone(),
                provenance: EvidenceProvenance::OsvReachability,
                kind: "osv_advisory".into(),
                symbol_or_target: vr.osv_id.clone(),
                file: None,
                line: None,
                summary: format!(
                    "Advisory `{}` ({:?}) on {}",
                    vr.osv_id,
                    vr.exploitability,
                    purl.as_deref().unwrap_or("dependency")
                ),
            },
        );
        reachable_vulnerabilities.push(ReachableVulnEvidence {
            id: osv_ref_id,
            osv_id: vr.osv_id.clone(),
            purl,
            exploitability: vr.exploitability,
            call_sites: sites,
            reaching_entry_points: reaching_eps.into_iter().collect(),
            provenance: EvidenceProvenance::OsvReachability,
        });
    }

    profile_lap(&mut stage_clock, "osv_join");
    // 7. Deterministic STRIDE Threat Candidates (`provenance: GraphRule`, `review_status: NeedsReview`).
    let mut threat_candidates: Vec<ThreatCandidate> = Vec::new();
    let ep_by_id: HashMap<&str, &EntryPointEvidence> =
        entry_points.iter().map(|ep| (ep.id.as_str(), ep)).collect();
    let sink_by_id: HashMap<&str, &SinkEvidence> =
        sinks.iter().map(|s| (s.id.as_str(), s)).collect();

    // S — Spoofing: externally exposed HTTP/RPC entry points without observed in-code auth controls.
    for ep in &entry_points {
        if matches!(
            ep.kind,
            EntryPointKind::HttpRoute | EntryPointKind::RpcOrMessageHandler
        ) && ep.observed_auth_controls.is_empty()
        {
            threat_candidates.push(ThreatCandidate {
                id: format!("THR-{:03}", threat_candidates.len() + 1),
                stride: StrideCategory::Spoofing,
                title: format!(
                    "Unauthenticated ingress candidate on `{}`",
                    ep.name
                ),
                description: format!(
                    "Entry point `{}` (`{}` at `{}:{}`) exposes an external handler with no observed authentication decorator, guard injection, or auth helper call in the local call graph.",
                    ep.name, ep.handler_symbol, ep.file, ep.line
                ),
                affected_component: ep.handler_symbol.clone(),
                provenance: EvidenceProvenance::GraphRule,
                review_status: ReviewStatus::NeedsReview,
                confidence: ConfidenceLevel::Medium,
                evidence_refs: vec![ep.id.clone()],
                assumptions: vec![
                    "Out-of-repo infrastructure (API Gateway, Envoy/Ingress JWT filter, or global runtime middleware) is not visible in static source analysis.".into(),
                    "Route may be intentionally public; verify business intent before treating as a defect.".into(),
                ],
                suggested_mitigations: vec![
                    "Verify authentication enforcement at the controller, router middleware, or ingress gateway.".into(),
                ],
            });
        }
    }

    // T — Tampering & E — Elevation of Privilege & R — Repudiation & D — Denial of Service from entry-to-sink flows.
    let mut repudiation_emitted_for_ep: BTreeSet<String> = BTreeSet::new();
    let mut dos_emitted_for_ep: BTreeSet<String> = BTreeSet::new();

    for df in &data_flows {
        let Some(&ep) = ep_by_id.get(df.entry_point_id.as_str()) else {
            continue;
        };
        let Some(&sink) = sink_by_id.get(df.sink_id.as_str()) else {
            continue;
        };

        // T — Tampering: unsanitized flow to SQL / Shell / FileWrite / CodeEval.
        if matches!(
            df.sink_category.as_str(),
            "sql_query" | "shell_command" | "file_write" | "code_eval"
        ) && !df.has_observed_sanitizer
        {
            let confidence = match df.pdg_taint {
                PdgSinkStatus::ProvenUnsanitizedFlow { .. } => ConfidenceLevel::High,
                _ => ConfidenceLevel::Medium,
            };
            threat_candidates.push(ThreatCandidate {
                id: format!("THR-{:03}", threat_candidates.len() + 1),
                stride: StrideCategory::Tampering,
                title: format!(
                    "Potential integrity / injection path from `{}` to `{}` (`{}`)",
                    ep.name, sink.callee, df.sink_category
                ),
                description: format!(
                    "Entry point `{}` reaches `{}` (`{}:{}`) across {} hop(s) without an observed sanitizer along the call/PDG path (PDG status: `{:?}`).",
                    ep.name, sink.callee, sink.file, sink.line, df.hops, df.pdg_taint
                ),
                affected_component: sink.caller.clone(),
                provenance: EvidenceProvenance::GraphRule,
                review_status: ReviewStatus::NeedsReview,
                confidence,
                evidence_refs: vec![ep.id.clone(), sink.id.clone(), df.id.clone()],
                assumptions: vec![
                    "Caller-supplied inputs reach sink arguments without custom wrapper parameterization not listed in built-in taint rules.".into(),
                ],
                suggested_mitigations: vec![
                    "Use parameterized queries / prepared statements, strict allowlists, or typed schema validation before invoking the sink.".into(),
                ],
            });
        }

        // E — Elevation of Privilege: command execution / eval reachable from external entry point without observed authz guard.
        if matches!(df.sink_category.as_str(), "shell_command" | "code_eval")
            && !df.has_observed_auth
        {
            let confidence = if matches!(df.pdg_taint, PdgSinkStatus::ProvenUnsanitizedFlow { .. })
            {
                ConfidenceLevel::High
            } else {
                ConfidenceLevel::Medium
            };
            threat_candidates.push(ThreatCandidate {
                id: format!("THR-{:03}", threat_candidates.len() + 1),
                stride: StrideCategory::ElevationOfPrivilege,
                title: format!(
                    "Privileged `{}` execution reachable from `{}` without observed authorization guard",
                    df.sink_category, ep.name
                ),
                description: format!(
                    "Handler `{}` can invoke `{}` (`{}:{}`) with the host process privileges and has no observed authorization annotation or guard.",
                    ep.handler_symbol, sink.callee, sink.file, sink.line
                ),
                affected_component: sink.caller.clone(),
                provenance: EvidenceProvenance::GraphRule,
                review_status: ReviewStatus::NeedsReview,
                confidence,
                evidence_refs: vec![ep.id.clone(), sink.id.clone(), df.id.clone()],
                assumptions: vec![
                    "Process runs with privileges exceeding an unauthenticated or low-privilege caller.".into(),
                ],
                suggested_mitigations: vec![
                    "Enforce role/permission checks before reaching command/code execution and Drop/sandbox subprocess privileges.".into(),
                ],
            });
        }

        // R — Repudiation: state-mutating route/sink without observed audit log in handler call cone.
        let is_mutating_http = ep.http_method.as_deref().is_some_and(|m| {
            matches!(
                m.to_ascii_uppercase().as_str(),
                "POST" | "PUT" | "DELETE" | "PATCH"
            )
        });
        let is_mutating_sink = matches!(
            df.sink_category.as_str(),
            "sql_query" | "shell_command" | "file_write"
        );
        if (is_mutating_http || is_mutating_sink)
            && !df.has_observed_audit_log
            && repudiation_emitted_for_ep.insert(ep.id.clone())
        {
            threat_candidates.push(ThreatCandidate {
                id: format!("THR-{:03}", threat_candidates.len() + 1),
                stride: StrideCategory::Repudiation,
                title: format!(
                    "No explicit audit log observed on state-sensitive flow from `{}`",
                    ep.name
                ),
                description: format!(
                    "Reachable call cone from `{}` reaches `{}` (`{}`) with zero observed audit/structured logging calls.",
                    ep.name, sink.callee, df.sink_category
                ),
                affected_component: ep.handler_symbol.clone(),
                provenance: EvidenceProvenance::GraphRule,
                review_status: ReviewStatus::NeedsReview,
                confidence: ConfidenceLevel::Low,
                evidence_refs: vec![ep.id.clone(), sink.id.clone(), df.id.clone()],
                assumptions: vec![
                    "HTTP access logs, AOP interceptors, or database audit triggers outside the handler's direct call cone are not observed.".into(),
                ],
                suggested_mitigations: vec![
                    "Record actor identity, action, target resource, and outcome in an immutable audit log.".into(),
                ],
            });
        }

        // I — Information Disclosure: unescaped HTML render or outbound network sink reachable from entry point.
        if matches!(df.sink_category.as_str(), "html_render" | "network_output")
            && !df.has_observed_sanitizer
        {
            threat_candidates.push(ThreatCandidate {
                id: format!("THR-{:03}", threat_candidates.len() + 1),
                stride: StrideCategory::InformationDisclosure,
                title: format!(
                    "Outbound `{}` flow from `{}` via `{}`",
                    df.sink_category, ep.name, sink.callee
                ),
                description: format!(
                    "Entry point `{}` reaches outbound/rendering sink `{}` (`{}:{}`), which may expose sensitive data or allow SSRF/XSS if untrusted inputs control destination or output.",
                    ep.name, sink.callee, sink.file, sink.line
                ),
                affected_component: sink.caller.clone(),
                provenance: EvidenceProvenance::GraphRule,
                review_status: ReviewStatus::NeedsReview,
                confidence: ConfidenceLevel::Medium,
                evidence_refs: vec![ep.id.clone(), sink.id.clone(), df.id.clone()],
                assumptions: vec![
                    "Outbound destination URL or rendered payload incorporates caller-controlled or sensitive fields.".into(),
                ],
                suggested_mitigations: vec![
                    "Enforce destination allowlists / output encoding and redact sensitive fields.".into(),
                ],
            });
        }

        // D — Denial of Service: external I/O sink reachable from HTTP/RPC entry without observed rate-limit/validation guard.
        if matches!(
            ep.kind,
            EntryPointKind::HttpRoute | EntryPointKind::RpcOrMessageHandler
        ) && matches!(
            df.sink_category.as_str(),
            "shell_command" | "network_output" | "file_write" | "sql_query"
        ) && ep.observed_validation_controls.is_empty()
            && dos_emitted_for_ep.insert(ep.id.clone())
        {
            threat_candidates.push(ThreatCandidate {
                id: format!("THR-{:03}", threat_candidates.len() + 1),
                stride: StrideCategory::DenialOfService,
                title: format!(
                    "Resource-intensive `{}` sink reachable from `{}` without observed rate/input bound",
                    df.sink_category, ep.name
                ),
                description: format!(
                    "Externally reachable handler `{}` invokes I/O or subprocess sink `{}` (`{}:{}`) with no in-code rate-limit, throttle, or input-bound decorator observed.",
                    ep.handler_symbol, sink.callee, sink.file, sink.line
                ),
                affected_component: ep.handler_symbol.clone(),
                provenance: EvidenceProvenance::GraphRule,
                review_status: ReviewStatus::NeedsReview,
                confidence: ConfidenceLevel::Low,
                evidence_refs: vec![ep.id.clone(), sink.id.clone(), df.id.clone()],
                assumptions: vec![
                    "Edge WAF / reverse-proxy rate limits, connection pools, and request timeouts cannot be verified from source graph alone.".into(),
                ],
                suggested_mitigations: vec![
                    "Enforce request rate limits, payload size bounds, and explicit timeouts on downstream I/O and subprocesses.".into(),
                ],
            });
        }
    }

    // I — Information Disclosure on Secret/Credential config assets consumed in application code.
    for asset in &assets {
        if asset.kind == AssetKind::SecretOrCredentialConfig && !asset.consumers.is_empty() {
            threat_candidates.push(ThreatCandidate {
                id: format!("THR-{:03}", threat_candidates.len() + 1),
                stride: StrideCategory::InformationDisclosure,
                title: format!(
                    "Secret/credential configuration asset `{}` accessed by {} symbol(s)",
                    asset.name,
                    asset.consumers.len()
                ),
                description: format!(
                    "Credential/secret key `{}` is consumed by `{}`; accidental logging, error serialization, or insecure storage could disclose secret material.",
                    asset.name,
                    asset.consumers.join(", ")
                ),
                affected_component: asset.consumers[0].clone(),
                provenance: EvidenceProvenance::GraphRule,
                review_status: ReviewStatus::NeedsReview,
                confidence: ConfidenceLevel::Medium,
                evidence_refs: vec![asset.id.clone()],
                assumptions: vec![
                    "Secret values are injected via environment or secret manager at runtime and must never be written to logs or error responses.".into(),
                ],
                suggested_mitigations: vec![
                    "Ensure secret redaction in logs/exceptions, least-privilege secret scoping, and rotation support.".into(),
                ],
            });
        }
    }

    // OSV Reachable Vulnerabilities -> STRIDE ElevationOfPrivilege / Tampering candidates.
    for rv in &reachable_vulnerabilities {
        let mut refs = vec![rv.id.clone()];
        refs.extend(rv.reaching_entry_points.iter().cloned());
        let confidence = if rv.exploitability == ExploitabilityVerdict::Exploitable {
            ConfidenceLevel::High
        } else {
            ConfidenceLevel::Medium
        };
        threat_candidates.push(ThreatCandidate {
            id: format!("THR-{:03}", threat_candidates.len() + 1),
            stride: StrideCategory::ElevationOfPrivilege,
            title: format!(
                "Reachable dependency advisory `{}` ({:?})",
                rv.osv_id, rv.exploitability
            ),
            description: format!(
                "Known vulnerability `{}` in `{}` has `{:?}` reachability in the repository call graph ({} call site(s)).",
                rv.osv_id,
                rv.purl.as_deref().unwrap_or("dependency"),
                rv.exploitability,
                rv.call_sites.len()
            ),
            affected_component: rv
                .purl
                .clone()
                .unwrap_or_else(|| rv.osv_id.clone()),
            provenance: EvidenceProvenance::GraphRule,
            review_status: ReviewStatus::NeedsReview,
            confidence,
            evidence_refs: refs,
            assumptions: vec![
                "Runtime configuration activates the affected code path and version matches deployed artifact.".into(),
            ],
            suggested_mitigations: vec![
                format!("Upgrade affected package for `{}` or apply documented inline mitigation / VEX justification.", rv.osv_id),
            ],
        });
    }

    profile_lap(&mut stage_clock, "stride");
    // 8. Explicit Limitations & Coverage Gaps.
    let mut limitations_and_gaps = vec![LimitationNote {
        code: "OUT_OF_REPO_CONTROLS".into(),
        category: "infrastructure_visibility".into(),
        detail: "Static source/CPG analysis cannot observe out-of-repo API gateways, WAF rules, TLS termination, IAM policies, or database ACLs.".into(),
    }];
    if opts.unresolved_calls_count > 0 {
        limitations_and_gaps.push(LimitationNote {
            code: "DYNAMIC_OR_INDIRECT_CALLS".into(),
            category: "call_graph_completeness".into(),
            detail: format!(
                "{} call site(s) target callback parameters, local function values, or unresolved overloads (`.rgctl/callable_resolution.json`); dynamic/reflective targets are not guessed.",
                opts.unresolved_calls_count
            ),
        });
    }
    if !opts.osv_evaluated {
        limitations_and_gaps.push(LimitationNote {
            code: "OSV_OFFLINE_EMPTY".into(),
            category: "dependency_vulnerabilities".into(),
            detail: "No local OSV advisories were evaluated (`.rgctl/osv/` empty and no `--osv` / `--from-report` supplied); run `rgctl vuln fetch` first to include CVE reachability.".into(),
        });
    }
    if !depth_truncated_nodes.is_empty() {
        limitations_and_gaps.push(LimitationNote {
            code: "CALL_DEPTH_LIMITED".into(),
            category: "call_graph_completeness".into(),
            detail: format!(
                "{} function(s) at the reachability depth limit ({max_depth} hops) have unexplored callees; sinks deeper than {max_depth} hops from an entry point are not reported. Re-run with a larger `--max-depth` for full coverage.",
                depth_truncated_nodes.len()
            ),
        });
    }
    if pdg_budget_skipped > 0 {
        limitations_and_gaps.push(LimitationNote {
            code: "PDG_BUDGET_EXHAUSTED".into(),
            category: "data_flow_completeness".into(),
            detail: format!(
                "Lazy CFG/PDG taint inspection covered the {max_pdg_functions} closest entry-reachable function(s); {pdg_budget_skipped} further reachable function(s) were not inspected, so their sinks carry graph-only evidence. Re-run with a larger `--max-pdg-functions` for full coverage.",
            ),
        });
    }
    if community_ctx.is_none() {
        limitations_and_gaps.push(LimitationNote {
            code: "COMMUNITIES_UNAVAILABLE".into(),
            category: "trust_boundaries".into(),
            detail: "Fast-Leiden community assignments (`.rgctl/analysis_results.bin`) were not present; subsystem bridge boundaries were omitted.".into(),
        });
    }

    let mut stride_counts: BTreeMap<String, usize> = BTreeMap::new();
    for tc in &threat_candidates {
        let key = match tc.stride {
            StrideCategory::Spoofing => "spoofing",
            StrideCategory::Tampering => "tampering",
            StrideCategory::Repudiation => "repudiation",
            StrideCategory::InformationDisclosure => "information_disclosure",
            StrideCategory::DenialOfService => "denial_of_service",
            StrideCategory::ElevationOfPrivilege => "elevation_of_privilege",
        };
        *stride_counts.entry(key.to_string()).or_insert(0) += 1;
    }

    let summary = ThreatModelSummary {
        entry_points: entry_points.len(),
        assets: assets.len(),
        sinks: sinks.len(),
        trust_boundaries: trust_boundaries.len(),
        data_flows: data_flows.len(),
        reachable_vulnerabilities: reachable_vulnerabilities.len(),
        threat_candidates: threat_candidates.len(),
        stride_counts,
        unresolved_or_indirect_calls: opts.unresolved_calls_count,
    };

    profile_lap(&mut stage_clock, "limitations_summary");
    Ok(ThreatModelReport {
        schema_version: THREAT_MODEL_SCHEMA_VERSION,
        command: "vuln threat-model".into(),
        summary,
        entry_points,
        assets,
        sinks,
        trust_boundaries,
        data_flows,
        reachable_vulnerabilities,
        threat_candidates,
        evidence_index,
        limitations_and_gaps,
    })
}

fn sanitize_mermaid_label(raw: &str) -> String {
    raw.replace('"', "'")
        .replace(['\n', '\r'], " ")
        .replace('<', "(")
        .replace('>', ")")
}

/// Render a standalone Mermaid `flowchart LR` Data-Flow Diagram (DFD) from [`ThreatModelReport`].
pub fn render_threat_model_mermaid(report: &ThreatModelReport) -> String {
    // Diagram size caps keep the DFD legible; anything beyond them is counted in an explicit
    // "omitted" node (the JSON output is always complete).
    const MAX_EPS: usize = 20;
    const MAX_SINKS: usize = 20;
    const MAX_ASSETS: usize = 15;
    const MAX_FLOWS: usize = 30;

    let mut out = String::from("flowchart LR\n");
    let _ = writeln!(out, "    EXT[\"External Caller / Actor\"]");

    for ep in report.entry_points.iter().take(MAX_EPS) {
        let id = ep.id.replace('-', "_");
        let label = sanitize_mermaid_label(&format!("{}: {}", ep.id, ep.name));
        let _ = writeln!(out, "    {id}[\"{label}\"]");
        let _ = writeln!(out, "    EXT --> {id}");
    }

    for sink in report
        .sinks
        .iter()
        .filter(|s| s.sink_category != "log_output")
        .take(MAX_SINKS)
    {
        let id = sink.id.replace('-', "_");
        let label = sanitize_mermaid_label(&format!(
            "{}: {} ({})",
            sink.id, sink.callee, sink.sink_category
        ));
        let _ = writeln!(out, "    {id}[\"{label}\"]");
    }

    for asset in report.assets.iter().take(MAX_ASSETS) {
        let id = asset.id.replace('-', "_");
        let label = sanitize_mermaid_label(&format!("{}: {}", asset.id, asset.name));
        let _ = writeln!(out, "    {id}[(\"{label}\")]");
    }

    let shown_eps: BTreeSet<&str> = report
        .entry_points
        .iter()
        .take(MAX_EPS)
        .map(|e| e.id.as_str())
        .collect();
    let shown_sinks: BTreeSet<&str> = report
        .sinks
        .iter()
        .filter(|s| s.sink_category != "log_output")
        .take(MAX_SINKS)
        .map(|s| s.id.as_str())
        .collect();

    // Filter to drawable flows first, then cap, so edges between shown nodes are never lost.
    let drawable: Vec<&ThreatDataFlow> = report
        .data_flows
        .iter()
        .filter(|df| {
            shown_eps.contains(df.entry_point_id.as_str())
                && shown_sinks.contains(df.sink_id.as_str())
        })
        .collect();
    for df in drawable.iter().take(MAX_FLOWS) {
        let from = df.entry_point_id.replace('-', "_");
        let to = df.sink_id.replace('-', "_");
        let _ = writeln!(out, "    {from} -->|\"{} hop(s)\"| {to}", df.hops);
    }

    let total_sinks = report
        .sinks
        .iter()
        .filter(|s| s.sink_category != "log_output")
        .count();
    let omitted = [
        (
            report.entry_points.len().saturating_sub(MAX_EPS),
            "entry point(s)",
        ),
        (total_sinks.saturating_sub(MAX_SINKS), "sink(s)"),
        (report.assets.len().saturating_sub(MAX_ASSETS), "asset(s)"),
        (
            report
                .data_flows
                .len()
                .saturating_sub(drawable.len().min(MAX_FLOWS)),
            "data flow(s)",
        ),
    ];
    let parts: Vec<String> = omitted
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
    if !parts.is_empty() {
        let label = sanitize_mermaid_label(&format!(
            "Omitted for legibility: {} (full list in JSON output)",
            parts.join(", ")
        ));
        let _ = writeln!(out, "    OMITTED[\"{label}\"]");
    }

    out
}

/// Render a human-readable GitHub-Flavored Markdown threat model report.
pub fn render_threat_model_markdown(report: &ThreatModelReport) -> String {
    let mut md = String::with_capacity(4096);
    let _ = writeln!(md, "# Threat Model Evidence & STRIDE Candidates\n");
    let _ = writeln!(
        md,
        "> [!IMPORTANT]\n> **Epistemic Boundary:** `graph_fact`, `cpg_taint`, and `osv_reachability` records are directly observed from the repository snapshot. All `threat_candidates` (`provenance: graph_rule`, `review_status: needs_review`) are **deterministic hypotheses**, not confirmed vulnerabilities.\n"
    );

    let s = &report.summary;
    let _ = writeln!(md, "## 1. Summary\n");
    let _ = writeln!(md, "| Metric | Count |");
    let _ = writeln!(md, "|--------|------:|");
    let _ = writeln!(md, "| Entry Points | {} |", s.entry_points);
    let _ = writeln!(md, "| Sensitive Assets | {} |", s.assets);
    let _ = writeln!(md, "| Security-Sensitive Sinks | {} |", s.sinks);
    let _ = writeln!(md, "| Trust-Boundary Candidates | {} |", s.trust_boundaries);
    let _ = writeln!(md, "| Entry-to-Sink Data Flows | {} |", s.data_flows);
    let _ = writeln!(
        md,
        "| Reachable Known Vulnerabilities (OSV) | {} |",
        s.reachable_vulnerabilities
    );
    let _ = writeln!(
        md,
        "| STRIDE Threat Candidates (`needs_review`) | {} |",
        s.threat_candidates
    );
    let _ = writeln!(
        md,
        "| Unresolved / Indirect Calls | {} |\n",
        s.unresolved_or_indirect_calls
    );

    let _ = writeln!(md, "## 2. Entry Points & Observed Controls\n");
    if report.entry_points.is_empty() {
        let _ = writeln!(
            md,
            "_No external routes, message handlers, or CLI entrypoints observed._\n"
        );
    } else {
        let _ = writeln!(
            md,
            "| ID | Kind | Entry Point | Handler | Location | Observed Auth | Observed Validation |"
        );
        let _ = writeln!(
            md,
            "|----|------|-------------|---------|----------|---------------|---------------------|"
        );
        for ep in &report.entry_points {
            let auth = if ep.observed_auth_controls.is_empty() {
                "none".to_string()
            } else {
                ep.observed_auth_controls.join(", ")
            };
            let val = if ep.observed_validation_controls.is_empty() {
                "none".to_string()
            } else {
                ep.observed_validation_controls.join(", ")
            };
            let _ = writeln!(
                md,
                "| `{}` | `{:?}` | `{}` | `{}` | `{}:{}` | {} | {} |",
                ep.id, ep.kind, ep.name, ep.handler_symbol, ep.file, ep.line, auth, val
            );
        }
        let _ = writeln!(md);
    }

    let _ = writeln!(md, "## 3. Assets & Trust Boundaries\n");
    if !report.assets.is_empty() {
        let _ = writeln!(md, "| ID | Kind | Asset | Consumers |");
        let _ = writeln!(md, "|----|------|-------|-----------|");
        for a in &report.assets {
            let consumers = if a.consumers.is_empty() {
                "—".to_string()
            } else {
                {
                    let mut shown = a.consumers.iter().take(4).cloned().collect::<Vec<_>>();
                    if a.consumers.len() > 4 {
                        shown.push(format!("+{} more", a.consumers.len() - 4));
                    }
                    shown.join(", ")
                }
            };
            let _ = writeln!(
                md,
                "| `{}` | `{:?}` | `{}` | {} |",
                a.id, a.kind, a.name, consumers
            );
        }
        let _ = writeln!(md);
    }
    if !report.trust_boundaries.is_empty() {
        let _ = writeln!(md, "| ID | Kind | From Zone | To Zone | Crossings |");
        let _ = writeln!(md, "|----|------|-----------|---------|----------:|");
        for tb in &report.trust_boundaries {
            let _ = writeln!(
                md,
                "| `{}` | `{:?}` | `{}` | `{}` | {} |",
                tb.id, tb.kind, tb.from_zone, tb.to_zone, tb.crossing_count
            );
        }
        let _ = writeln!(md);
    }

    let _ = writeln!(md, "## 4. Data-Flow Diagram\n");
    let _ = writeln!(md, "```mermaid");
    md.push_str(&render_threat_model_mermaid(report));
    let _ = writeln!(md, "```\n");

    let _ = writeln!(md, "## 5. STRIDE Threat Candidates (`needs_review`)\n");
    if report.threat_candidates.is_empty() {
        let _ = writeln!(md, "_No STRIDE threat candidates triggered._\n");
    } else {
        for tc in &report.threat_candidates {
            let _ = writeln!(
                md,
                "### `{}` [{}] {} _(confidence: {}, status: `needs_review`)_\n",
                tc.id,
                tc.stride.short_code(),
                tc.title,
                tc.confidence.as_str()
            );
            let _ = writeln!(md, "- **Category:** {} (`graph_rule`)", tc.stride.label());
            let _ = writeln!(md, "- **Component:** `{}`", tc.affected_component);
            let _ = writeln!(md, "- **Description:** {}", tc.description);
            let _ = writeln!(
                md,
                "- **Evidence Refs:** {}",
                tc.evidence_refs
                    .iter()
                    .map(|r| format!("`{r}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let _ = writeln!(md, "- **Assumptions:** {}", tc.assumptions.join(" "));
            let _ = writeln!(
                md,
                "- **Suggested Mitigation:** {}\n",
                tc.suggested_mitigations.join(" ")
            );
        }
    }

    let _ = writeln!(md, "## 6. Limitations & Coverage Gaps\n");
    for lim in &report.limitations_and_gaps {
        let _ = writeln!(
            md,
            "- **`{}`** ({}): {}",
            lim.code, lim.category, lim.detail
        );
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_and_validation_controls_match_whole_tokens_only() {
        for yes in [
            "PreAuthorize",
            "org.springframework.security.access.prepost.PreAuthorize",
            "RolesAllowed",
            "Secured",
            "get_current_user",
            "login_required",
            "jwt.verify",
            "require_auth",
            "AuthGuard",
            "HTTPBearer",
            "hasRole",
            "OAuth2Client",
        ] {
            assert!(is_auth_control_name(yes), "{yes} should be an auth control");
        }
        for no in [
            "get_author",
            "safeguard_value",
            "authority_level",
            "AuthorService.find",
        ] {
            assert!(
                !is_auth_control_name(no),
                "{no} must not be an auth control"
            );
        }
        for yes in [
            "validate_input",
            "Valid",
            "RateLimiter",
            "rate_limit",
            "throttle",
            "sanitize",
        ] {
            assert!(
                is_validation_or_ratelimit_name(yes),
                "{yes} should be validation"
            );
        }
        for no in [
            "invalidate_cache",
            "get_schema_version",
            "InvalidRequestError",
        ] {
            assert!(
                !is_validation_or_ratelimit_name(no),
                "{no} must not be a validation control"
            );
        }
    }

    #[test]
    fn identifier_tokens_split_camel_acronyms_and_separators() {
        assert_eq!(identifier_tokens("HTTPBearer"), ["http", "bearer"]);
        assert_eq!(
            identifier_tokens("get_current_user"),
            ["get", "current", "user"]
        );
        assert_eq!(
            identifier_tokens("a::B::preAuthorize"),
            ["a", "b", "pre", "authorize"]
        );
        assert_eq!(identifier_tokens("OAuth2Client"), ["o", "auth2", "client"]);
    }

    #[test]
    fn fallback_sink_heuristics_require_qualified_receivers() {
        let rules = TaintRuleSet::default();
        // Unqualified generic method names are not sinks.
        assert_eq!(classify_callee_sink(&rules, "", "std::thread::spawn"), None);
        assert_eq!(classify_callee_sink(&rules, "", "Executor::execute"), None);
        assert_eq!(classify_callee_sink(&rules, "", "pattern.exec"), None);
        assert_eq!(classify_callee_sink(&rules, "", "catalog.info"), None);
        // Qualified ones still are.
        assert_eq!(
            classify_callee_sink(&rules, "", "cursor.execute"),
            Some(TaintSink::SqlQuery)
        );
        assert_eq!(
            classify_callee_sink(&rules, "", "std::process::Command::new"),
            Some(TaintSink::ShellCommand)
        );
        assert_eq!(
            classify_callee_sink(&rules, "", "exec"),
            Some(TaintSink::CodeEval)
        );
        assert_eq!(
            classify_callee_sink(&rules, "", "logger.info"),
            Some(TaintSink::LogOutput)
        );
    }

    #[test]
    fn qualified_owner_handles_rust_paths_and_dotted_names() {
        assert_eq!(qualified_owner("a::B::f"), Some("a::B"));
        assert_eq!(
            qualified_owner("com.acme.OrderController.create"),
            Some("com.acme.OrderController")
        );
        assert_eq!(qualified_owner("main"), None);
        assert_eq!(qualified_owner_short_name("a::B"), "B");
        assert_eq!(
            qualified_owner_short_name("com.acme.OrderController"),
            "OrderController"
        );
    }

    #[test]
    fn config_asset_classification_distinguishes_secrets_endpoints_and_runtime_flags() {
        assert_eq!(
            classify_config_asset("STRIPE_SECRET_KEY"),
            AssetKind::SecretOrCredentialConfig
        );
        assert_eq!(
            classify_config_asset("JWT_SIGNING_TOKEN"),
            AssetKind::SecretOrCredentialConfig
        );
        assert_eq!(
            classify_config_asset("DATABASE_URL"),
            AssetKind::ConnectionOrEndpointConfig
        );
        assert_eq!(
            classify_config_asset("REDIS_HOST"),
            AssetKind::ConnectionOrEndpointConfig
        );
        assert_eq!(
            classify_config_asset("FEATURE_CHECKOUT_V2"),
            AssetKind::RuntimeConfig
        );
    }

    #[test]
    fn mermaid_and_markdown_renderers_escape_special_labels_cleanly() {
        let mut evidence_index = BTreeMap::new();
        evidence_index.insert(
            "EP-001".to_string(),
            EvidenceRecord {
                id: "EP-001".into(),
                provenance: EvidenceProvenance::GraphFact,
                kind: "entry_point".into(),
                symbol_or_target: "api.exec_cmd".into(),
                file: Some("app.py".into()),
                line: Some(12),
                summary: "POST /exec".into(),
            },
        );
        evidence_index.insert(
            "SNK-001".to_string(),
            EvidenceRecord {
                id: "SNK-001".into(),
                provenance: EvidenceProvenance::CpgTaint,
                kind: "sink:shell_command".into(),
                symbol_or_target: "subprocess.run".into(),
                file: Some("app.py".into()),
                line: Some(15),
                summary: "subprocess.run".into(),
            },
        );
        let report = ThreatModelReport {
            schema_version: THREAT_MODEL_SCHEMA_VERSION,
            command: "vuln threat-model".into(),
            summary: ThreatModelSummary {
                entry_points: 1,
                sinks: 1,
                data_flows: 1,
                threat_candidates: 1,
                ..Default::default()
            },
            entry_points: vec![EntryPointEvidence {
                id: "EP-001".into(),
                kind: EntryPointKind::HttpRoute,
                name: "POST /exec/<id> \"raw\"".into(),
                handler_symbol: "api.exec_cmd".into(),
                http_method: Some("POST".into()),
                route_path: Some("/exec/<id>".into()),
                file: "app.py".into(),
                line: 12,
                community: None,
                observed_auth_controls: vec![],
                observed_validation_controls: vec![],
                provenance: EvidenceProvenance::GraphFact,
            }],
            assets: vec![],
            sinks: vec![SinkEvidence {
                id: "SNK-001".into(),
                sink_category: "shell_command".into(),
                callee: "subprocess.run".into(),
                caller: "api.exec_cmd".into(),
                file: "app.py".into(),
                line: 15,
                is_external_call: true,
                community: None,
                pdg_taint: PdgSinkStatus::ProvenUnsanitizedFlow { source_line: 13 },
                provenance: EvidenceProvenance::CpgTaint,
            }],
            trust_boundaries: vec![],
            data_flows: vec![ThreatDataFlow {
                id: "DF-001".into(),
                entry_point_id: "EP-001".into(),
                sink_id: "SNK-001".into(),
                sink_category: "shell_command".into(),
                call_path: vec!["api.exec_cmd".into()],
                hops: 0,
                crossed_communities: vec![],
                has_observed_auth: false,
                has_observed_sanitizer: false,
                has_observed_audit_log: false,
                pdg_taint: PdgSinkStatus::ProvenUnsanitizedFlow { source_line: 13 },
                provenance: EvidenceProvenance::CpgTaint,
            }],
            reachable_vulnerabilities: vec![],
            threat_candidates: vec![ThreatCandidate {
                id: "THR-001".into(),
                stride: StrideCategory::Tampering,
                title: "Command injection candidate".into(),
                description: "Unsanitized route to subprocess.run".into(),
                affected_component: "api.exec_cmd".into(),
                provenance: EvidenceProvenance::GraphRule,
                review_status: ReviewStatus::NeedsReview,
                confidence: ConfidenceLevel::High,
                evidence_refs: vec!["EP-001".into(), "SNK-001".into()],
                assumptions: vec!["Input reaches subprocess command string.".into()],
                suggested_mitigations: vec!["Pass fixed argument list.".into()],
            }],
            evidence_index,
            limitations_and_gaps: vec![],
        };

        let mermaid = render_threat_model_mermaid(&report);
        assert!(mermaid.starts_with("flowchart LR\n"));
        assert!(mermaid.contains("EP_001[\"EP-001: POST /exec/(id) 'raw'\"]"));
        assert!(mermaid.contains("EP_001 -->|\"0 hop(s)\"| SNK_001"));

        let md = render_threat_model_markdown(&report);
        assert!(md.contains("Epistemic Boundary"));
        assert!(md.contains("`THR-001` [T]"));
        assert!(md.contains("`EP-001`, `SNK-001`"));
    }
}
