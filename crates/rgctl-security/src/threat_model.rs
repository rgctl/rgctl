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
use rgctl_analysis::{AnalysisResults, CommunityQueryContext, TaintRuleSet, TaintSink};
use rgctl_extraction::ParkedRelationRecord;
use rgctl_graph::columnar_snapshot::ColumnarGraphMmap;
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_plugin_api::RelationType;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
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
}

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
        _ => "python",
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
    if matches!(
        tail,
        "execute"
            | "executequery"
            | "executeupdate"
            | "raw"
            | "rawquery"
            | "executesql"
            | "query_raw"
    ) {
        Some(TaintSink::SqlQuery)
    } else if matches!(
        tail,
        "popen" | "execve" | "execvp" | "spawn" | "runtimeshell"
    ) || lower.contains("command::new")
        || lower.contains("processbuilder")
        || lower.contains("runtime.getruntime().exec")
        || lower.contains("subprocess.")
        || lower.contains("child_process.")
    {
        Some(TaintSink::ShellCommand)
    } else if matches!(tail, "eval" | "exec" | "unserialize" | "readobject")
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
    ) && (lower.contains("log")
        || lower.contains("audit")
        || lower.contains("tracing")
        || lower.contains("logger"))
    {
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

fn is_auth_control_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("auth")
        || lower.contains("rolesallowed")
        || lower.contains("preauthorize")
        || lower.contains("secured")
        || lower.contains("permission")
        || lower.contains("login_required")
        || lower.contains("current_user")
        || lower.contains("jwt")
        || lower.contains("oauth")
        || lower.contains("bearer")
        || lower.contains("csrf")
        || lower.contains("apikey")
        || lower.contains("guard")
}

fn is_validation_or_ratelimit_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("valid")
        || lower.contains("ratelimit")
        || lower.contains("rate_limit")
        || lower.contains("throttle")
        || lower.contains("sanitize")
        || lower.contains("schema")
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

    let mut nodes: Vec<CompactNodeMeta> = Vec::with_capacity(n_count);
    let mut id_to_idx: HashMap<Uuid, usize> = HashMap::with_capacity(n_count);
    let mut class_by_qn: HashMap<String, usize> = HashMap::new();

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
            class_by_qn.entry(name.clone()).or_insert(idx);
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
    let community_for_idx = |idx: usize| -> Option<String> {
        let cctx = community_ctx.as_ref()?;
        let &cid = cctx.uuid_to_community.get(&nodes[idx].id)?;
        let (label, count) = community_labels_by_id.get(&cid)?;
        (*count > 1).then(|| label.clone())
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
            && let Some((owner, _)) = qn.rsplit_once(['.', ':'])
            && let Some(&cls_idx) = class_by_qn.get(owner)
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

    for (&h_idx, anns) in &annotations_on {
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
        if seen_entry_handlers.keys().any(|(idx, _)| *idx == h_idx) {
            continue;
        }
        if let Some(ann) = anns.iter().find(|a| is_handler_annotation(a)) {
            let label = format!("@{ann} {}", h.display_fqn());
            if seen_entry_handlers
                .insert((h_idx, label.clone()), ())
                .is_none()
            {
                let kind = if ann.to_ascii_lowercase().contains("listener")
                    || ann.to_ascii_lowercase().contains("message")
                    || ann.to_ascii_lowercase().contains("grpc")
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

    // 3. Discover Sinks (graph `Calls` edges + external `parked_relations.bin` + lazy per-function
    //    PDG inspection on up to 64 closest entry-reachable functions).
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

    const MAX_LAZY_PDG_FUNCTIONS: usize = 64;
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
        if pdg_evaluated_nodes.len() >= MAX_LAZY_PDG_FUNCTIONS {
            break;
        }
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
            if classify_callee_sanitizer(taint_rules, lang, callee_label) {
                node_has_sanitizer[u] = true;
            }
            if let Some(sink_kind) = classify_callee_sink(taint_rules, lang, callee_label) {
                let cat = sink_category_str(sink_kind).to_string();
                let has_same_fn_cat = raw_sinks
                    .iter()
                    .any(|s| s.caller_idx == u && sink_category_str(s.sink_kind) == cat);
                if !has_same_fn_cat {
                    let loc_key = (u, cat, caller.start_line);
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

    for rec in opts.parked_relations {
        if rec.relation_type != RelationType::Calls {
            continue;
        }
        let Some(&caller_idx) = id_to_idx.get(&rec.from_node_id) else {
            continue;
        };
        let rel_file = relativize_path(opts.repo_root, canon_repo.as_deref(), &rec.from_file);
        if is_test_or_fixture_noise_file(&rel_file) {
            continue;
        }
        let lang = language_for_file(&rel_file);
        let candidates = parked_callee_candidates(rec);
        if candidates
            .iter()
            .any(|c| classify_callee_sanitizer(taint_rules, lang, c))
        {
            node_has_sanitizer[caller_idx] = true;
        }
        let mut matched: Option<(TaintSink, String)> = None;
        for cand in &candidates {
            if let Some(sk) = classify_callee_sink(taint_rules, lang, cand) {
                matched = Some((sk, cand.clone()));
                break;
            }
        }
        if let Some((sink_kind, callee_candidate)) = matched {
            let cat = sink_category_str(sink_kind).to_string();
            let loc_key = (caller_idx, cat, rec.call_site_line);
            if let std::collections::hash_map::Entry::Vacant(e) = sink_slot_by_loc.entry(loc_key) {
                let slot = raw_sinks.len();
                e.insert(slot);
                raw_sinks.push(RawSink {
                    caller_idx,
                    sink_kind,
                    callee: callee_candidate,
                    file: rel_file,
                    line: rec.call_site_line,
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

    let mut sinks_by_caller: HashMap<usize, Vec<usize>> = HashMap::new();
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

    // 5. Data Flows (Entry Point -> Sink paths) & Trust Boundaries.
    let mut data_flows: Vec<ThreatDataFlow> = Vec::new();
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
                let df_id = format!("DF-{:03}", data_flows.len() + 1);
                let provenance = match &sink.pdg_taint {
                    PdgSinkStatus::ProvenUnsanitizedFlow { .. }
                    | PdgSinkStatus::SanitizedFlow { .. } => EvidenceProvenance::CpgTaint,
                    _ => EvidenceProvenance::GraphFact,
                };
                evidence_index.insert(
                    df_id.clone(),
                    EvidenceRecord {
                        id: df_id.clone(),
                        provenance,
                        kind: "data_flow".into(),
                        symbol_or_target: format!("{} -> {}", ep.name, sink.callee),
                        file: Some(sink.file.clone()),
                        line: Some(sink.line),
                        summary: format!(
                            "Flow `{}` -> `{}` ({} hops, sink `{}`)",
                            ep.name, sink.callee, depth, sink.sink_category
                        ),
                    },
                );
                data_flows.push(ThreatDataFlow {
                    id: df_id,
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
                });
            }
        }
    }
    data_flows.sort_by(|a, b| {
        (&a.entry_point_id, &a.sink_id, a.hops).cmp(&(&b.entry_point_id, &b.sink_id, b.hops))
    });
    // Re-number DF IDs deterministically after sort.
    for (i, df) in data_flows.iter_mut().enumerate() {
        let new_id = format!("DF-{:03}", i + 1);
        if df.id != new_id {
            if let Some(mut rec) = evidence_index.remove(&df.id) {
                rec.id = new_id.clone();
                evidence_index.insert(new_id.clone(), rec);
            }
            df.id = new_id;
        }
    }

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

    let mut community_bridges: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (u, callees) in calls_out.iter().enumerate() {
        let Some(c_from) = community_for_idx(u) else {
            continue;
        };
        for &v in callees {
            let Some(c_to) = community_for_idx(v) else {
                continue;
            };
            if c_from != c_to {
                community_bridges
                    .entry((c_from.clone(), c_to))
                    .or_default()
                    .push(format!(
                        "{} -> {}",
                        nodes[u].display_fqn(),
                        nodes[v].display_fqn()
                    ));
            }
        }
    }
    for ((from_zone, to_zone), samples) in community_bridges {
        let tb_id = format!("TB-{:03}", trust_boundaries.len() + 1);
        let count = samples.len();
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
    let mut out = String::from("flowchart LR\n");
    let _ = writeln!(out, "    EXT[\"External Caller / Actor\"]");

    for ep in report.entry_points.iter().take(20) {
        let id = ep.id.replace('-', "_");
        let label = sanitize_mermaid_label(&format!("{}: {}", ep.id, ep.name));
        let _ = writeln!(out, "    {id}[\"{label}\"]");
        let _ = writeln!(out, "    EXT --> {id}");
    }

    for sink in report
        .sinks
        .iter()
        .filter(|s| s.sink_category != "log_output")
        .take(20)
    {
        let id = sink.id.replace('-', "_");
        let label = sanitize_mermaid_label(&format!(
            "{}: {} ({})",
            sink.id, sink.callee, sink.sink_category
        ));
        let _ = writeln!(out, "    {id}[\"{label}\"]");
    }

    for asset in report.assets.iter().take(15) {
        let id = asset.id.replace('-', "_");
        let label = sanitize_mermaid_label(&format!("{}: {}", asset.id, asset.name));
        let _ = writeln!(out, "    {id}[(\"{label}\")]");
    }

    let shown_eps: BTreeSet<&str> = report
        .entry_points
        .iter()
        .take(20)
        .map(|e| e.id.as_str())
        .collect();
    let shown_sinks: BTreeSet<&str> = report
        .sinks
        .iter()
        .filter(|s| s.sink_category != "log_output")
        .take(20)
        .map(|s| s.id.as_str())
        .collect();

    for df in report.data_flows.iter().take(30) {
        if shown_eps.contains(df.entry_point_id.as_str())
            && shown_sinks.contains(df.sink_id.as_str())
        {
            let from = df.entry_point_id.replace('-', "_");
            let to = df.sink_id.replace('-', "_");
            let _ = writeln!(out, "    {from} -->|\"{} hop(s)\"| {to}", df.hops);
        }
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
                a.consumers
                    .iter()
                    .take(4)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
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
