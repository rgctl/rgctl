//! Expected-facts checker (Rust port of the former Python correctness runner).

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

pub struct ProjectSpec {
    pub id: &'static str,
    pub dir_name: &'static str,
    pub exclude: &'static str,
}

#[derive(Debug)]
pub struct CheckResult {
    pub id: String,
    pub severity: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug)]
pub struct ProjectReport {
    pub checks: Vec<CheckResult>,
    pub required_failures: usize,
}

#[derive(Debug, Deserialize)]
struct ExpectedFacts {
    #[serde(default)]
    graph: GraphSpec,
    #[serde(default)]
    symbols: std::collections::BTreeMap<String, SymbolEntry>,
    #[serde(default)]
    invariants: Invariants,
    #[serde(default)]
    analytics: Option<AnalyticsSpec>,
}

#[derive(Debug, Default, Deserialize)]
struct GraphSpec {
    #[serde(default = "required_sev")]
    severity: String,
    min_functions: Option<usize>,
    min_calls_edges: Option<usize>,
    exact_functions: Option<usize>,
    exact_calls_edges: Option<usize>,
    #[serde(default)]
    forbidden_edges: Vec<ForbiddenEdge>,
}

#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct ForbiddenEdge {
    from: String,
    to: String,
    #[serde(default)]
    edge_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Invariants {
    #[serde(default)]
    #[serde(rename = "B1_blast_vs_calls")]
    b1: InvariantSpec,
    #[serde(default)]
    #[serde(rename = "B2_gql_calls_nonzero", alias = "B2_calls_nonzero")]
    b2: InvariantSpec,
    #[serde(default)]
    #[serde(rename = "B5_inspect_cfg_present")]
    b5: InvariantSpec,
    #[serde(default)]
    #[serde(rename = "B6_cfg_calls_subset_of_calls_edges")]
    b6: InvariantSpec,
    #[serde(default)]
    #[serde(rename = "B7_pdg_lines_subset_cfg")]
    b7: InvariantSpec,
    #[serde(default)]
    #[serde(rename = "B8_dom_blocks_match_cfg")]
    b8: InvariantSpec,
    #[serde(default)]
    #[serde(rename = "B9_blast_target_name")]
    b9: InvariantSpec,
}

#[derive(Debug, Default, Deserialize)]
struct InvariantSpec {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default = "required_sev")]
    severity: String,
    #[serde(default)]
    symbols: Vec<String>,
}

fn required_sev() -> String {
    "required".into()
}

#[derive(Debug, Default, Deserialize)]
struct SymbolEntry {
    #[serde(default = "required_sev")]
    severity: String,
    #[serde(default = "true_bool")]
    unique: bool,
    #[serde(default)]
    identity: Option<IdentitySpec>,
    #[serde(rename = "match", default)]
    match_field: Option<MatchSpec>,
    #[serde(default)]
    exact_callees: Option<Vec<String>>,
    #[serde(default)]
    exact_callers: Option<Vec<String>>,
    #[serde(default)]
    direct_callees: Option<Vec<PeerSpec>>,
    #[serde(default)]
    direct_callers: Option<Vec<PeerSpec>>,
    #[serde(default)]
    blast: Option<BlastSpec>,
    #[serde(default)]
    ast: Option<AstSpec>,
    #[serde(default)]
    cfg: Option<CfgSpec>,
    #[serde(default)]
    pdg: Option<PdgSpec>,
    #[serde(default)]
    dataflow: Option<PdgSpec>,
    #[serde(default)]
    dom: Option<DomSpec>,
    #[serde(default)]
    dominance: Option<DomSpec>,
    #[serde(default)]
    mro: Option<MroSpec>,
    #[serde(default)]
    slice: Option<SliceSpec>,
    #[serde(default)]
    flows: Option<FlowsSpec>,
    #[serde(default)]
    mutations: Option<MutationsSpec>,
    #[serde(default)]
    taint: Option<TaintSpec>,
    #[serde(default)]
    clones: Option<ClonesSpec>,
    #[serde(default)]
    semantic: Option<SemanticSpec>,
    #[serde(default)]
    review: Option<ReviewSpec>,
}

fn true_bool() -> bool {
    true
}

impl SymbolEntry {
    fn match_spec(&self) -> MatchSpec {
        self.match_field.clone().unwrap_or_default()
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
struct MatchSpec {
    name: String,
    #[serde(default)]
    class: Option<String>,
    /// Optional path / file suffix filter (`path/to/file.ts::name` for inspect/blast).
    #[serde(default)]
    file: Option<String>,
    /// Overload / signature fragment that must appear on the matched node (`(String)`, `int`).
    #[serde(default)]
    discriminator: Option<String>,
    /// 1-based definition line. Selects one overload when several share a short name.
    #[serde(default)]
    line: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct IdentitySpec {
    #[serde(default = "required_sev")]
    severity: String,
    exact_name: Option<String>,
    canonical_fqn: Option<String>,
    class_context: Option<String>,
    file_suffix: Option<String>,
    language: Option<String>,
    /// Signature or arity fragment that must appear on the resolved identity (`(int)`, `(String)`).
    #[serde(default)]
    discriminator: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct PeerSpec {
    name: String,
    #[serde(default)]
    class: Option<String>,
    #[serde(default = "required_sev")]
    severity: String,
}

#[derive(Debug, Default, Deserialize)]
struct BlastSpec {
    #[serde(default = "required_sev")]
    severity: String,
    exact_direct_callers: Option<Vec<String>>,
    exact_impact_zone: Option<Vec<String>>,
    min_direct_callers: Option<usize>,
    max_direct_callers: Option<usize>,
    caller_names_any: Option<Vec<String>>,
    min_score: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct AstSpec {
    #[serde(default = "required_sev")]
    severity: String,
    must_call: Option<Vec<String>>,
    statements_contain: Option<Vec<String>>,
    exact_statement_kinds: Option<Vec<String>>,
    statement_kinds_any: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct CfgSpec {
    #[serde(default = "required_sev")]
    severity: String,
    exact_block_count: Option<usize>,
    min_blocks: Option<usize>,
    exact_edge_count: Option<usize>,
    min_edges: Option<usize>,
    exact_edge_kinds: Option<Vec<String>>,
    statements_contain: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct PdgSpec {
    #[serde(default = "required_sev")]
    severity: String,
    exact_data_deps: Option<usize>,
    exact_control_deps: Option<usize>,
    exact_node_count: Option<usize>,
    exact_node_kinds: Option<Vec<String>>,
    node_labels_contain: Option<Vec<String>>,
    data_edges: Option<Vec<DataEdgeSpec>>,
}

#[derive(Debug, Deserialize)]
struct DataEdgeSpec {
    variable: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct DomSpec {
    #[serde(default = "required_sev")]
    severity: String,
    exact_block_count: Option<usize>,
    idom_contains: Option<Vec<IdomPair>>,
    exact_idom: Option<Vec<IdomPair>>,
}

#[derive(Debug, Deserialize)]
struct IdomPair {
    block: i64,
    immediate_dominator: i64,
}

#[derive(Debug, Default, Deserialize)]
struct MroSpec {
    #[serde(default = "required_sev")]
    severity: String,
    #[serde(default)]
    extends: Vec<String>,
    #[serde(default)]
    implements: Vec<String>,
    #[serde(default)]
    overrides: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct TextExpect {
    #[serde(default)]
    must_contain: Vec<String>,
    #[serde(default)]
    must_exclude: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SliceSpec {
    #[serde(default = "required_sev")]
    severity: String,
    file: String,
    line: usize,
    variable: String,
    #[serde(default)]
    function: Option<String>,
    #[serde(default = "backward_dir")]
    direction: String,
    #[serde(flatten)]
    text: TextExpect,
}

fn backward_dir() -> String {
    "backward".into()
}

#[derive(Debug, Deserialize)]
struct FlowsSpec {
    #[serde(default = "required_sev")]
    severity: String,
    file: String,
    line: usize,
    variable: String,
    #[serde(default)]
    function: Option<String>,
    #[serde(default = "forward_dir")]
    direction: String,
    #[serde(default = "true_bool")]
    with_alias: bool,
    #[serde(flatten)]
    text: TextExpect,
}

fn forward_dir() -> String {
    "forward".into()
}

#[derive(Debug, Deserialize)]
struct MutationsSpec {
    #[serde(default = "required_sev")]
    severity: String,
    #[serde(rename = "type")]
    type_name: String,
    #[serde(default)]
    member: Option<String>,
    #[serde(default = "true_bool")]
    exclude_ctors: bool,
    #[serde(flatten)]
    text: TextExpect,
}

#[derive(Debug, Deserialize)]
struct TaintSpec {
    #[serde(default = "required_sev")]
    severity: String,
    sink: String,
    #[serde(default = "external_source")]
    source: String,
    #[serde(default)]
    min_flows: Option<usize>,
    #[serde(flatten)]
    text: TextExpect,
}

fn external_source() -> String {
    "external".into()
}

#[derive(Debug, Default, Deserialize)]
struct ClonesSpec {
    #[serde(default = "required_sev")]
    severity: String,
    #[serde(default = "exact_mode")]
    mode: String,
    #[serde(default)]
    seed: Option<String>,
    #[serde(default)]
    min_loc: Option<usize>,
    /// Names that must share one clone group with this symbol.
    #[serde(default)]
    group_with: Vec<String>,
    /// Names that must not share a clone group with this symbol.
    #[serde(default)]
    not_grouped_with: Vec<String>,
}

fn exact_mode() -> String {
    "exact".into()
}

#[derive(Debug, Deserialize)]
struct SemanticSpec {
    #[serde(default = "required_sev")]
    severity: String,
    query: String,
    /// Symbol name that must appear in the top-3 vocab hits.
    top3_contains: String,
    #[serde(default = "true_bool")]
    expand_neighbors: bool,
    /// When set, every name must appear in `--expand neighbors` and no other requirement is implied.
    #[serde(default)]
    neighbor_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ReviewSpec {
    #[serde(default = "required_sev")]
    severity: String,
    /// Repo-relative file edited in the sandbox worktree.
    file: String,
    find: String,
    replace: String,
    /// Caller names that must appear in `review paths --synthetic-head worktree`.
    #[serde(default)]
    must_contain: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AnalyticsSpec {
    #[serde(default = "required_sev")]
    severity: String,
    root: String,
    mid: String,
    leaf: String,
    /// Two symbol names that must sit in different Fast-Leiden communities.
    #[serde(default)]
    separate: Vec<String>,
}

struct GraphIndex {
    functions: Vec<Value>,
    calls: Vec<(Value, Value)>,
}

impl GraphIndex {
    fn function_count(&self) -> usize {
        self.functions.len()
    }
    fn calls_count(&self) -> usize {
        self.calls.len()
    }

    fn has_calls_edge(&self, from: &str, to: &str) -> bool {
        self.calls.iter().any(|(caller, callee)| {
            caller.get("node").and_then(|n| n.as_str()) == Some(from)
                && callee.get("node").and_then(|n| n.as_str()) == Some(to)
        })
    }

    fn find_functions(&self, name: &str, class_hint: Option<&str>) -> Vec<&Value> {
        let hits: Vec<&Value> = self
            .functions
            .iter()
            .filter(|f| f.get("node").and_then(|n| n.as_str()) == Some(name))
            .collect();
        if let Some(hint) = class_hint {
            let narrowed: Vec<&Value> = hits
                .iter()
                .copied()
                .filter(|f| {
                    f.get("file")
                        .and_then(|x| x.as_str())
                        .is_some_and(|p| p.contains(hint))
                })
                .collect();
            if !narrowed.is_empty() {
                return narrowed;
            }
        }
        hits
    }

    fn find_matched(&self, m: &MatchSpec) -> Vec<&Value> {
        let mut hits = self.find_functions(&m.name, m.class.as_deref());
        if let Some(disc) = m.discriminator.as_deref().filter(|s| !s.is_empty()) {
            hits.retain(|f| entity_blob(f).contains(disc));
        }
        if let Some(line) = m.line {
            let any_line = hits
                .iter()
                .any(|f| f.get("line").and_then(|v| v.as_u64()).is_some());
            if any_line {
                hits.retain(|f| f.get("line").and_then(|v| v.as_u64()) == Some(line as u64));
            }
        }
        hits
    }

    fn peers_of(&self, name: &str, hits: &[&Value], outgoing: bool) -> Vec<String> {
        let ids: BTreeSet<&str> = hits
            .iter()
            .filter_map(|t| t.get("id").and_then(|v| v.as_str()))
            .filter(|s| !s.is_empty())
            .collect();
        let files: BTreeSet<&str> = hits
            .iter()
            .filter_map(|t| t.get("file").and_then(|f| f.as_str()))
            .collect();
        let lines: BTreeSet<u64> = hits
            .iter()
            .filter_map(|t| t.get("line").and_then(|v| v.as_u64()))
            .collect();
        let mut out = BTreeSet::new();
        for (caller, callee) in &self.calls {
            let (endpoint, other) = if outgoing {
                (caller, callee)
            } else {
                (callee, caller)
            };
            if !ids.is_empty()
                && let Some(eid) = endpoint
                    .get("id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            {
                if !ids.contains(eid) {
                    continue;
                }
            } else {
                if endpoint.get("node").and_then(|n| n.as_str()) != Some(name) {
                    continue;
                }
                if !files.is_empty() {
                    let cf = endpoint.get("file").and_then(|f| f.as_str()).unwrap_or("");
                    if !files.contains(cf) {
                        continue;
                    }
                }
                if !lines.is_empty()
                    && let Some(line) = endpoint.get("line").and_then(|v| v.as_u64())
                    && !lines.contains(&line)
                {
                    continue;
                }
            }
            if let Some(n) = other.get("node").and_then(|n| n.as_str()) {
                out.insert(n.to_string());
            }
        }
        out.into_iter().collect()
    }

    fn callees_of(&self, name: &str, class_hint: Option<&str>) -> Vec<String> {
        let targets = self.find_functions(name, class_hint);
        self.peers_of(name, &targets, true)
    }

    fn callers_of(&self, name: &str, class_hint: Option<&str>) -> Vec<String> {
        let targets = self.find_functions(name, class_hint);
        self.peers_of(name, &targets, false)
    }
}

fn short_name(fqn: &str) -> String {
    if let Some(i) = fqn.rfind("::") {
        return fqn[i + 2..].to_string();
    }
    if let Some(i) = fqn.rfind('.') {
        return fqn[i + 1..].to_string();
    }
    fqn.to_string()
}

fn normalize_names(names: &[String]) -> Vec<String> {
    let mut s: BTreeSet<String> = names.iter().map(|n| short_name(n)).collect();
    s.retain(|n| !n.is_empty());
    s.into_iter().collect()
}

fn name_matches(actual: &str, expected: &str) -> bool {
    let a = actual.trim();
    let e = expected.trim();
    if a.is_empty() || e.is_empty() {
        return false;
    }
    if a == e {
        return true;
    }
    short_name(a) == short_name(e)
        || a.ends_with(&format!(".{e}"))
        || a.ends_with(&format!("::{e}"))
}

fn check(
    id: impl Into<String>,
    severity: &str,
    ok: bool,
    message: impl Into<String>,
) -> CheckResult {
    CheckResult {
        id: id.into(),
        severity: severity.to_string(),
        ok,
        message: message.into(),
    }
}

fn run_json(bin: &Path, cwd: &Path, args: &[&str]) -> Option<Value> {
    let out = Command::new(bin)
        .current_dir(cwd)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines().rev() {
        let line = line.trim();
        if line.starts_with('{') || line.starts_with('[') {
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                return Some(v);
            }
        }
    }
    serde_json::from_str(stdout.trim()).ok()
}

fn symbol_candidates(m: &MatchSpec) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(file) = &m.file {
        out.push(format!("{file}::{}", m.name));
    }
    if let Some(cls) = &m.class {
        // File-like class hints (Go) are handled via --file in run_blast; skip Class:: here.
        if !(cls.ends_with(".go") || cls.contains('/')) {
            out.push(format!("{cls}::{}", m.name));
        }
    }
    out.push(m.name.clone());
    out
}

fn run_blast(bin: &Path, cwd: &Path, m: &MatchSpec) -> Option<Value> {
    let line = m.line.map(|n| n.to_string());
    let attempt = |args: &[&str]| -> Option<Value> {
        if let Some(line) = line.as_deref() {
            let mut owned: Vec<&str> = args.to_vec();
            owned.push("--line");
            owned.push(line);
            return run_json(bin, cwd, &owned);
        }
        run_json(bin, cwd, args)
    };
    // Go fixtures disambiguate by source file (`class: "methods.go"`).
    if let Some(cls) = &m.class {
        if cls.ends_with(".go") || cls.contains('/') {
            if let Some(v) = attempt(&["-f", "json", "blast-radius", &m.name, "--file", cls]) {
                return Some(v);
            }
        }
    }
    if let Some(file) = &m.file {
        if let Some(v) = attempt(&["-f", "json", "blast-radius", &m.name, "--file", file]) {
            return Some(v);
        }
        let fqn = format!("{file}::{}", m.name);
        if let Some(v) = attempt(&["-f", "json", "blast-radius", &fqn]) {
            return Some(v);
        }
    }
    for sym in symbol_candidates(m) {
        if let Some(v) = attempt(&["-f", "json", "blast-radius", &sym]) {
            return Some(v);
        }
    }
    None
}

fn run_inspect(bin: &Path, cwd: &Path, m: &MatchSpec, layer: &str) -> Option<Value> {
    let line = m.line.map(|n| n.to_string());
    let attempt = |sym: &str| -> Option<Value> {
        let mut args = vec!["-f", "json", "inspect", sym];
        if let Some(line) = line.as_deref() {
            args.extend(["--line", line]);
        }
        args.push(layer);
        run_json(bin, cwd, &args)
    };
    for sym in symbol_candidates(m) {
        if let Some(v) = attempt(&sym) {
            return Some(v);
        }
    }
    // Prefer FQN when class looks like a Go receiver/type name (not a filename).
    if let Some(cls) = &m.class {
        if !cls.ends_with(".go") && !cls.contains('/') {
            let fqn = format!("{cls}.{}", m.name);
            if let Some(v) = attempt(&fqn) {
                return Some(v);
            }
        }
    }
    None
}

fn load_graph_index(bin: &Path, cwd: &Path) -> GraphIndex {
    let mut functions = Vec::new();
    if let Some(data) = run_json(
        bin,
        cwd,
        &[
            "-f", "json", "find", "--type", "function", "--limit", "100000",
        ],
    ) {
        if let Some(entities) = data.get("entities").and_then(|rows| rows.as_array()) {
            for entity in entities {
                functions.push(entity_as_legacy(entity));
            }
        }
    }
    let mut calls = Vec::new();
    if let Some(data) = run_json(
        bin,
        cwd,
        &[
            "-f",
            "json",
            "relations",
            "--edge",
            "calls",
            "--from-type",
            "function",
            "--to-type",
            "function",
            "--limit",
            "100000",
        ],
    ) {
        if let Some(edges) = data.get("edges").and_then(|rows| rows.as_array()) {
            for edge in edges {
                let Some(source) = edge.get("source") else {
                    continue;
                };
                let Some(target) = edge.get("target") else {
                    continue;
                };
                calls.push((entity_as_legacy(source), entity_as_legacy(target)));
            }
        }
    }
    GraphIndex { functions, calls }
}

fn caller_ids_from_doc(doc: &Value) -> BTreeSet<String> {
    doc.get("callers")
        .and_then(|c| c.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    row.get("id")
                        .and_then(|v| v.as_str())
                        .or_else(|| row.get("qualified_name").and_then(|v| v.as_str()))
                        .or_else(|| row.get("name").and_then(|v| v.as_str()))
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn structured_query_checks(bin: &Path, cwd: &Path, index: &GraphIndex) -> Vec<CheckResult> {
    // NOTE: `rgctl gql` is deprecated; all oracle & query-surface checks target canonical
    // structured JSON verbs (`find`, `callers`, `callees`, `relations`, `inventory`, `blast-radius`).
    let mut checks = Vec::new();
    let inventory = run_json(
        bin,
        cwd,
        &["-f", "json", "inventory", "--by", "import-prefix"],
    );
    let inv_ok = inventory.as_ref().is_some_and(|doc| {
        schema_v2(doc)
            && doc.get("by").and_then(|b| b.as_str()) == Some("import-prefix")
            && doc.get("counts").and_then(|c| c.as_array()).is_some()
    });
    checks.push(check(
        "query.inventory_import_prefix",
        "required",
        inv_ok,
        "inventory --by import-prefix returns schema v2 counts",
    ));

    let ann_edges = run_json(
        bin,
        cwd,
        &[
            "-f",
            "json",
            "relations",
            "--edge",
            "annotated-with",
            "--limit",
            "5",
        ],
    );
    let sample_ann = ann_edges
        .as_ref()
        .and_then(|d| d.get("edges"))
        .and_then(|e| e.as_array())
        .and_then(|rows| {
            rows.iter().find_map(|r| {
                r.get("target")
                    .and_then(|t| t.get("name"))
                    .and_then(|n| n.as_str())
                    .filter(|s| !s.is_empty())
            })
        });
    let ann_query = sample_ann.unwrap_or("Override");
    let annotated = run_json(
        bin,
        cwd,
        &[
            "-f",
            "json",
            "find",
            "--annotation",
            ann_query,
            "--show-attributes",
        ],
    );
    let ann_ok = annotated.as_ref().is_some_and(|doc| {
        if !schema_v2(doc) {
            return false;
        }
        let empty = [];
        let entities = doc
            .get("entities")
            .and_then(|e| e.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&empty);
        if sample_ann.is_some() {
            !entities.is_empty()
                && entities.iter().all(|row| {
                    row.get("attributes")
                        .and_then(|a| a.as_str())
                        .is_some_and(|s| s.starts_with('@'))
                })
        } else {
            doc.get("total").and_then(|t| t.as_u64()).is_some()
        }
    });
    checks.push(check(
        "query.find_annotation_attributes",
        "required",
        ann_ok,
        format!("find --annotation {ann_query} --show-attributes"),
    ));

    fn non_self_callee((caller, callee): &(Value, Value)) -> Option<(&str, &str, &str)> {
        if caller == callee {
            return None;
        }
        let name = callee.get("node").and_then(|n| n.as_str())?;
        let file = callee.get("file").and_then(|n| n.as_str())?;
        if file == "<external>" {
            return None;
        }
        let qn = callee
            .get("qualified_name")
            .and_then(|q| q.as_str())
            .unwrap_or(file);
        Some((name, file, qn))
    }
    let scoped = index
        .calls
        .iter()
        .find_map(|pair| {
            let (name, file, qn) = non_self_callee(pair)?;
            name.to_ascii_lowercase()
                .starts_with("correctness")
                .then_some((name, file, qn))
        })
        .or_else(|| index.calls.iter().find_map(non_self_callee));
    if let Some((name, file, qn)) = scoped {
        let scope_prefix = qn
            .rsplit_once("::")
            .or_else(|| qn.rsplit_once('.'))
            .or_else(|| qn.rsplit_once('#'))
            .map(|(owner, _)| owner)
            .filter(|s| !s.is_empty())
            .unwrap_or(file);
        let all_doc = run_json(bin, cwd, &["-f", "json", "callers", name, "--file", file]);
        let inside_doc = run_json(
            bin,
            cwd,
            &[
                "-f",
                "json",
                "callers",
                name,
                "--file",
                file,
                "--scope",
                scope_prefix,
                "--scope-mode",
                "inside",
            ],
        );
        let outside_doc = run_json(
            bin,
            cwd,
            &[
                "-f",
                "json",
                "callers",
                name,
                "--file",
                file,
                "--scope",
                scope_prefix,
                "--scope-mode",
                "outside",
            ],
        );
        let scope_ok = match (&all_doc, &inside_doc, &outside_doc) {
            (Some(all), Some(ins), Some(out))
                if schema_v2(all) && schema_v2(ins) && schema_v2(out) =>
            {
                let all_ids = caller_ids_from_doc(all);
                let in_ids = caller_ids_from_doc(ins);
                let out_ids = caller_ids_from_doc(out);
                let union: BTreeSet<String> = in_ids.union(&out_ids).cloned().collect();
                in_ids.is_disjoint(&out_ids) && union == all_ids && !all_ids.is_empty()
            }
            _ => false,
        };
        checks.push(check(
            "query.callers_scope_outside",
            "required",
            scope_ok,
            format!("callers {name} --file {file} --scope {scope_prefix} --scope-mode inside|outside partition"),
        ));
    }

    let called_files_with_external: BTreeSet<&str> = index
        .calls
        .iter()
        .filter_map(|(from, to)| {
            let f1 = from.get("file").and_then(|f| f.as_str())?;
            let f2 = to.get("file").and_then(|f| f.as_str())?;
            (f1 != f2).then_some(f2)
        })
        .collect();

    let mut container: Option<(String, String, bool)> = None;
    let mut fallback_container: Option<(String, String)> = None;
    for type_name in ["class", "struct", "interface", "module"] {
        let Some(found) = run_json(
            bin,
            cwd,
            &[
                "-f", "json", "find", "--type", type_name, "--limit", "100000",
            ],
        ) else {
            continue;
        };
        if let Some(rows) = found.get("entities").and_then(|r| r.as_array()) {
            for row in rows {
                let Some(name) = row.get("name").and_then(|n| n.as_str()) else {
                    continue;
                };
                let Some(file) = row.get("file").and_then(|n| n.as_str()) else {
                    continue;
                };
                if file == "<external>" {
                    continue;
                }
                if fallback_container.is_none() && (file.contains("src/") || !file.ends_with(".md"))
                {
                    fallback_container = Some((name.to_string(), file.to_string()));
                }
                if called_files_with_external.contains(file)
                    && matches!(type_name, "class" | "struct")
                {
                    container = Some((name.to_string(), file.to_string(), true));
                    break;
                }
            }
        }
        if container.is_some() {
            break;
        }
    }
    if container.is_none()
        && let Some((n, f)) = fallback_container
    {
        container = Some((n, f, false));
    }
    if let Some((name, file, _)) = container {
        let callers = run_json(bin, cwd, &["-f", "json", "callers", &name, "--file", &file]);
        let blast = run_json(
            bin,
            cwd,
            &["-f", "json", "blast-radius", &name, "--file", &file],
        );
        checks.push(check(
            "query.container_callers",
            "required",
            callers.as_ref().is_some_and(|doc| {
                schema_v2(doc) && doc.get("callers").is_some_and(Value::is_array)
            }),
            format!("callers {name} --file {file}"),
        ));
        checks.push(check(
            "query.container_blast_radius",
            "required",
            blast.as_ref().is_some_and(|v| {
                v.get("target").is_some_and(|t| !t.is_null())
                    || v.get("nodes").is_some_and(Value::is_array)
            }),
            format!("blast-radius {name} --file {file}"),
        ));
    }

    checks
}

fn cfg_texts(cfg: &Value) -> Vec<String> {
    let mut texts = Vec::new();
    if let Some(nodes) = cfg.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            if let Some(stmts) = node.get("statements").and_then(|s| s.as_array()) {
                for s in stmts {
                    if let Some(t) = s.get("text").and_then(|t| t.as_str()) {
                        texts.push(t.to_string());
                    }
                }
            }
        }
    }
    texts
}

fn cfg_kinds(cfg: &Value) -> Vec<String> {
    let mut kinds = Vec::new();
    if let Some(nodes) = cfg.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            if let Some(stmts) = node.get("statements").and_then(|s| s.as_array()) {
                for s in stmts {
                    if let Some(k) = s.get("kind").and_then(|k| k.as_str()) {
                        kinds.push(k.to_string());
                    }
                }
            }
        }
    }
    kinds
}

fn cfg_edge_kinds(cfg: &Value) -> Vec<String> {
    let mut kinds = Vec::new();
    if let Some(edges) = cfg.get("edges").and_then(|e| e.as_array()) {
        for e in edges {
            kinds.push(
                e.get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or("")
                    .to_string(),
            );
        }
    }
    kinds.sort();
    kinds
}

fn copy_fixture(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == ".rgctl" || name == "target" || name == "node_modules" {
            continue;
        }
        let target = dst.join(&name);
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn entity_as_legacy(entity: &Value) -> Value {
    let line = entity
        .get("line")
        .cloned()
        .or_else(|| entity.get("start_line").cloned())
        .unwrap_or(Value::Null);
    serde_json::json!({
        "id": entity.get("id").cloned().unwrap_or(Value::Null),
        "node": entity.get("name").cloned().unwrap_or(Value::Null),
        "file": entity.get("file").cloned().unwrap_or(Value::Null),
        "qualified_name": entity.get("qualified_name").cloned().unwrap_or(Value::Null),
        "line": line,
        "signature": entity.get("signature").cloned().unwrap_or(Value::Null),
    })
}

fn entity_blob(entity: &Value) -> String {
    format!(
        "{} {} {} {}",
        entity.get("node").and_then(|v| v.as_str()).unwrap_or(""),
        entity
            .get("qualified_name")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        entity
            .get("signature")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        entity.get("file").and_then(|v| v.as_str()).unwrap_or("")
    )
}

fn schema_v2(value: &Value) -> bool {
    value.get("schema_version").and_then(|v| v.as_i64()) == Some(2)
}

fn ensure_discover(bin: &Path, cwd: &Path, exclude: &str, clean: bool) -> CheckResult {
    let cache = cwd.join(".rgctl");
    if clean && cache.exists() {
        let _ = fs::remove_dir_all(&cache);
    }
    let out = Command::new(bin)
        .current_dir(cwd)
        .args(["-f", "json", "discover", ".", "--cfg", "-e", exclude])
        .output()
        .expect("spawn discover");
    let ok = out.status.success();
    check(
        "discover",
        "required",
        ok,
        format!("exit={}", out.status.code().unwrap_or(-1)),
    )
}

fn ensure_analytics_discover(bin: &Path, cwd: &Path, exclude: &str) -> CheckResult {
    let cache = cwd.join(".rgctl");
    if cache.exists() {
        let _ = fs::remove_dir_all(&cache);
    }
    let out = Command::new(bin)
        .current_dir(cwd)
        .args([
            "-f",
            "json",
            "discover",
            ".",
            "--with-cfg",
            "--with-taint",
            "--with-security",
            "--with-harmonic",
            "--export-migration-hints",
            "-e",
            exclude,
        ])
        .output()
        .expect("spawn analytics discover");
    check(
        "analytics.discover",
        "required",
        out.status.success(),
        format!("exit={}", out.status.code().unwrap_or(-1)),
    )
}

fn check_exact_set(id: &str, sev: &str, actual: &[String], expected: &[String]) -> CheckResult {
    let a = normalize_names(actual);
    let e = normalize_names(expected);
    check(
        id,
        sev,
        a == e,
        format!("exact names actual={a:?} expected={e:?}"),
    )
}

pub fn run_project(
    bin: &Path,
    project_dir: &Path,
    facts_path: &Path,
    exclude: &str,
    clean: bool,
) -> ProjectReport {
    let facts: ExpectedFacts =
        serde_json::from_str(&fs::read_to_string(facts_path).expect("read facts"))
            .expect("parse expected-facts.json");

    let sandbox = tempfile::TempDir::new().expect("correctness sandbox");
    let work = sandbox.path().join("repo");
    copy_fixture(project_dir, &work);

    let needs_full_analytics = facts.analytics.is_some()
        || facts
            .symbols
            .values()
            .any(|s| s.taint.is_some() && s.severity != "unsupported");
    let mut checks = Vec::new();
    let disc = if needs_full_analytics {
        let mut d = ensure_analytics_discover(bin, &work, exclude);
        d.id = "discover".to_string();
        d
    } else {
        ensure_discover(bin, &work, exclude, clean)
    };
    let disc_failed = !disc.ok;
    checks.push(disc);
    if disc_failed {
        return ProjectReport {
            required_failures: 1,
            checks,
        };
    }

    let index = load_graph_index(bin, &work);
    checks.extend(structured_query_checks(bin, &work, &index));

    // Universal columnar structural invariants (INV-1…INV-7).
    {
        use rgctl_graph::{SNAPSHOT_FILE, SnapshotNodeStore, assert_graph_invariants};
        let snap = work.join(".rgctl").join(SNAPSHOT_FILE);
        let inv_check = match SnapshotNodeStore::open(&snap) {
            Ok(store) => match store.mmap().columnar() {
                Some(col) => match assert_graph_invariants(col) {
                    Ok(()) => check(
                        "graph.assert_graph_invariants",
                        "required",
                        true,
                        "INV-1..INV-9 ok",
                    ),
                    Err(e) => check(
                        "graph.assert_graph_invariants",
                        "required",
                        false,
                        e.to_string(),
                    ),
                },
                None => check(
                    "graph.assert_graph_invariants",
                    "required",
                    false,
                    "snapshot is not columnar v2",
                ),
            },
            Err(e) => check(
                "graph.assert_graph_invariants",
                "required",
                false,
                format!("open snapshot: {e}"),
            ),
        };
        checks.push(inv_check);
    }

    let gsev = facts.graph.severity.as_str();
    if let Some(min_fn) = facts.graph.min_functions {
        checks.push(check(
            "graph.min_functions",
            gsev,
            index.function_count() >= min_fn,
            format!("functions={} min={min_fn}", index.function_count()),
        ));
    }
    if let Some(exact_fn) = facts.graph.exact_functions {
        checks.push(check(
            "graph.exact_functions",
            gsev,
            index.function_count() == exact_fn,
            format!("functions={} exact={exact_fn}", index.function_count()),
        ));
    }
    if let Some(min_calls) = facts.graph.min_calls_edges {
        checks.push(check(
            "graph.min_calls_edges",
            gsev,
            index.calls_count() >= min_calls,
            format!("calls={} min={min_calls}", index.calls_count()),
        ));
    }
    if let Some(exact_calls) = facts.graph.exact_calls_edges {
        checks.push(check(
            "graph.exact_calls_edges",
            gsev,
            index.calls_count() == exact_calls,
            format!("calls={} exact={exact_calls}", index.calls_count()),
        ));
    }
    for (i, fe) in facts.graph.forbidden_edges.iter().enumerate() {
        let hit = index.has_calls_edge(&fe.from, &fe.to);
        let ok = !hit;
        checks.push(check(
            &format!("graph.forbidden_edges[{i}]"),
            gsev,
            ok,
            format!("forbidden {} → {} present={hit}", fe.from, fe.to),
        ));
    }

    for (sid, entry) in &facts.symbols {
        checks.extend(check_symbol(sid, entry, bin, &work, &index));
    }
    checks.extend(check_invariants(&facts, bin, &work, &index));
    if let Some(analytics) = &facts.analytics {
        if !needs_full_analytics {
            checks.push(ensure_analytics_discover(bin, &work, exclude));
        }
        checks.extend(check_analytics(analytics, &work));
    }

    let required_failures = checks
        .iter()
        .filter(|c| c.severity == "required" && !c.ok)
        .count();
    ProjectReport {
        checks,
        required_failures,
    }
}

fn check_symbol(
    sid: &str,
    entry: &SymbolEntry,
    bin: &Path,
    cwd: &Path,
    index: &GraphIndex,
) -> Vec<CheckResult> {
    let sev = entry.severity.as_str();
    if sev == "unsupported" {
        return vec![];
    }
    let mut m = entry.match_spec();
    if m.file.is_none() {
        if let Some(ident) = &entry.identity {
            m.file = ident.file_suffix.clone();
        }
    }
    if m.discriminator.is_none() {
        if let Some(ident) = &entry.identity {
            m.discriminator = ident.discriminator.clone();
        }
    }
    if m.name.is_empty() {
        return vec![check(
            format!("symbol.{sid}"),
            sev,
            false,
            "missing match.name",
        )];
    }
    let class = m.class.as_deref();
    let mut out = Vec::new();

    let hits = index.find_matched(&m);
    let exists_ok = if entry.unique {
        hits.len() == 1
    } else {
        !hits.is_empty()
    };
    out.push(check(
        format!("symbol.{sid}.exists"),
        sev,
        exists_ok,
        format!("function '{}' class={class:?} hits={}", m.name, hits.len()),
    ));
    if hits.is_empty() {
        return out;
    }
    if hits.len() == 1 && m.line.is_none() && m.discriminator.is_some() {
        m.line = hits[0]
            .get("line")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
    }

    if let Some(exact) = &entry.exact_callees {
        out.push(check_exact_set(
            &format!("symbol.{sid}.exact_callees"),
            sev,
            &index.peers_of(&m.name, &hits, true),
            exact,
        ));
    } else if let Some(peers) = &entry.direct_callees {
        let actual = index.peers_of(&m.name, &hits, true);
        for peer in peers {
            let psev = peer.severity.as_str();
            out.push(check(
                format!("symbol.{sid}.direct_callees"),
                psev,
                actual.iter().any(|a| name_matches(a, &peer.name)),
                format!("expected peer '{}' in {actual:?}", peer.name),
            ));
        }
    }
    if let Some(exact) = &entry.exact_callers {
        out.push(check_exact_set(
            &format!("symbol.{sid}.exact_callers"),
            sev,
            &index.peers_of(&m.name, &hits, false),
            exact,
        ));
    } else if let Some(peers) = &entry.direct_callers {
        let actual = index.peers_of(&m.name, &hits, false);
        for peer in peers {
            let psev = peer.severity.as_str();
            out.push(check(
                format!("symbol.{sid}.direct_callers"),
                psev,
                actual.iter().any(|a| name_matches(a, &peer.name)),
                format!("expected peer '{}' in {actual:?}", peer.name),
            ));
        }
    }

    let need_blast = entry.blast.is_some() || entry.identity.is_some();
    let blast = if need_blast {
        run_blast(bin, cwd, &m)
    } else {
        None
    };

    if let Some(bs) = &entry.blast {
        let bsev = bs.severity.as_str();
        match &blast {
            None => out.push(check(
                format!("symbol.{sid}.blast"),
                bsev,
                false,
                format!("blast-radius failed for {:?}", symbol_candidates(&m)),
            )),
            Some(b) => out.extend(check_blast(sid, bs, b)),
        }
    }

    let need_cfg = entry.cfg.is_some()
        || entry.ast.is_some()
        || entry.identity.is_some()
        || entry.pdg.is_some()
        || entry.dataflow.is_some()
        || entry.dom.is_some()
        || entry.dominance.is_some();
    let cfg = if need_cfg {
        run_inspect(bin, cwd, &m, "cfg")
    } else {
        None
    };

    if let Some(cs) = &entry.cfg {
        let csev = cs.severity.as_str();
        if csev != "unsupported" {
            match &cfg {
                None => out.push(check(
                    format!("symbol.{sid}.cfg"),
                    csev,
                    false,
                    "inspect cfg failed",
                )),
                Some(c) => out.extend(check_cfg(sid, cs, c)),
            }
        }
    }

    if let Some(ast) = &entry.ast {
        let asev = ast.severity.as_str();
        match &cfg {
            None => out.push(check(
                format!("symbol.{sid}.ast"),
                asev,
                false,
                "ast checks require inspect cfg",
            )),
            Some(c) => out.extend(check_ast(sid, ast, c)),
        }
    }

    if let Some(ident) = &entry.identity {
        out.extend(check_identity(sid, ident, blast.as_ref(), cfg.as_ref()));
    }

    let pdg_spec = entry.pdg.as_ref().or(entry.dataflow.as_ref());
    if let Some(ps) = pdg_spec {
        let psev = ps.severity.as_str();
        if psev != "unsupported" {
            match run_inspect(bin, cwd, &m, "pdg") {
                None => out.push(check(
                    format!("symbol.{sid}.pdg"),
                    psev,
                    false,
                    "inspect pdg failed",
                )),
                Some(p) => out.extend(check_pdg(sid, ps, &p)),
            }
        }
    }

    let dom_spec = entry.dom.as_ref().or(entry.dominance.as_ref());
    if let Some(ds) = dom_spec {
        let dsev = ds.severity.as_str();
        if dsev != "unsupported" {
            match run_inspect(bin, cwd, &m, "dom") {
                None => out.push(check(
                    format!("symbol.{sid}.dom"),
                    dsev,
                    false,
                    "inspect dom failed",
                )),
                Some(d) => out.extend(check_dom(sid, ds, &d)),
            }
        }
    }

    out.extend(check_v2_dimensions(sid, entry, &m, bin, cwd));
    out
}

fn check_blast(sid: &str, bs: &BlastSpec, b: &Value) -> Vec<CheckResult> {
    let sev = bs.severity.as_str();
    let mut out = Vec::new();
    let metrics = b.get("metrics").cloned().unwrap_or(Value::Null);
    let topo = b.get("topology").cloned().unwrap_or(Value::Null);
    let callers: Vec<String> = topo
        .get("direct_callers")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("fqn").and_then(|f| f.as_str()).map(short_name))
                .collect()
        })
        .unwrap_or_default();
    let impact: Vec<String> = topo
        .get("impact_zone")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("fqn").and_then(|f| f.as_str()).map(short_name))
                .collect()
        })
        .unwrap_or_default();
    let dc = metrics
        .get("direct_callers_count")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    if let Some(exact) = &bs.exact_direct_callers {
        out.push(check_exact_set(
            &format!("symbol.{sid}.blast.exact_direct_callers"),
            sev,
            &callers,
            exact,
        ));
        let want_n = normalize_names(exact).len();
        out.push(check(
            format!("symbol.{sid}.blast.direct_callers_count"),
            sev,
            dc == Some(want_n),
            format!("direct_callers_count={dc:?} expected={want_n}"),
        ));
    }
    if let Some(exact) = &bs.exact_impact_zone {
        out.push(check_exact_set(
            &format!("symbol.{sid}.blast.exact_impact_zone"),
            sev,
            &impact,
            exact,
        ));
        let want_n = normalize_names(exact).len();
        let iz = metrics
            .get("impact_zone_size")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        out.push(check(
            format!("symbol.{sid}.blast.impact_zone_size"),
            sev,
            iz == Some(want_n),
            format!("impact_zone_size={iz:?} expected={want_n}"),
        ));
    }
    if bs.exact_direct_callers.is_none() {
        if let Some(mn) = bs.min_direct_callers {
            out.push(check(
                format!("symbol.{sid}.blast.min_direct_callers"),
                sev,
                dc.is_some_and(|d| d >= mn),
                format!("direct_callers_count={dc:?} min={mn}"),
            ));
        }
        if let Some(mx) = bs.max_direct_callers {
            out.push(check(
                format!("symbol.{sid}.blast.max_direct_callers"),
                sev,
                dc.is_some_and(|d| d <= mx),
                format!("direct_callers_count={dc:?} max={mx}"),
            ));
        }
    }
    if let Some(any) = &bs.caller_names_any {
        for want in any {
            out.push(check(
                format!("symbol.{sid}.blast.caller_names_any"),
                sev,
                callers.iter().any(|c| name_matches(c, want)),
                format!("want caller '{want}' in {callers:?}"),
            ));
        }
    }
    if let Some(mn) = bs.min_score {
        let score = metrics.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        out.push(check(
            format!("symbol.{sid}.blast.min_score"),
            sev,
            score >= mn,
            format!("score={score} min={mn}"),
        ));
    }
    out
}

fn check_cfg(sid: &str, cs: &CfgSpec, cfg: &Value) -> Vec<CheckResult> {
    let sev = cs.severity.as_str();
    let mut out = Vec::new();
    let nblocks = cfg
        .get("nodes")
        .and_then(|n| n.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let nedges = cfg
        .get("edges")
        .and_then(|n| n.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    if let Some(want) = cs.exact_block_count {
        out.push(check(
            format!("symbol.{sid}.cfg.exact_block_count"),
            sev,
            nblocks == want,
            format!("blocks={nblocks} expected={want}"),
        ));
    } else if let Some(mn) = cs.min_blocks {
        out.push(check(
            format!("symbol.{sid}.cfg.min_blocks"),
            sev,
            nblocks >= mn,
            format!("blocks={nblocks} min={mn}"),
        ));
    }
    if let Some(want) = cs.exact_edge_count {
        out.push(check(
            format!("symbol.{sid}.cfg.exact_edge_count"),
            sev,
            nedges == want,
            format!("edges={nedges} expected={want}"),
        ));
    } else if let Some(mn) = cs.min_edges {
        out.push(check(
            format!("symbol.{sid}.cfg.min_edges"),
            sev,
            nedges >= mn,
            format!("edges={nedges} min={mn}"),
        ));
    }
    if let Some(want) = &cs.exact_edge_kinds {
        let mut w = want.clone();
        w.sort();
        let got = cfg_edge_kinds(cfg);
        out.push(check(
            format!("symbol.{sid}.cfg.exact_edge_kinds"),
            sev,
            got == w,
            format!("edge_kinds actual={got:?} expected={w:?}"),
        ));
    }
    if let Some(frags) = &cs.statements_contain {
        let texts = cfg_texts(cfg);
        for frag in frags {
            out.push(check(
                format!("symbol.{sid}.cfg.statements_contain"),
                sev,
                texts.iter().any(|t| t.contains(frag)),
                format!("want {frag:?} in {texts:?}"),
            ));
        }
    }
    out
}

fn check_ast(sid: &str, ast: &AstSpec, cfg: &Value) -> Vec<CheckResult> {
    let sev = ast.severity.as_str();
    let mut out = Vec::new();
    let texts = cfg_texts(cfg);
    let kinds = cfg_kinds(cfg);
    let joined = texts.join("\n");
    if let Some(frags) = &ast.statements_contain {
        for frag in frags {
            out.push(check(
                format!("symbol.{sid}.ast.statements_contain"),
                sev,
                joined.contains(frag),
                format!("want text containing {frag:?} in {texts:?}"),
            ));
        }
    }
    if let Some(calls) = &ast.must_call {
        for call in calls {
            let ok = texts
                .iter()
                .any(|t| t.contains(&format!("{call}(")) || t.contains(&format!("{call} (")));
            out.push(check(
                format!("symbol.{sid}.ast.must_call"),
                sev,
                ok,
                format!("want call site '{call}(' in statements {texts:?}"),
            ));
        }
    }
    if let Some(want) = &ast.exact_statement_kinds {
        let mut w = want.clone();
        w.sort();
        let mut g = kinds.clone();
        g.sort();
        out.push(check(
            format!("symbol.{sid}.ast.exact_statement_kinds"),
            sev,
            g == w,
            format!("kinds actual={g:?} expected={w:?}"),
        ));
    }
    if let Some(any) = &ast.statement_kinds_any {
        for kind in any {
            out.push(check(
                format!("symbol.{sid}.ast.statement_kinds_any"),
                sev,
                kinds.iter().any(|k| k == kind),
                format!("want kind {kind:?} in {kinds:?}"),
            ));
        }
    }
    out
}

fn check_identity(
    sid: &str,
    ident: &IdentitySpec,
    blast: Option<&Value>,
    cfg: Option<&Value>,
) -> Vec<CheckResult> {
    let sev = ident.severity.as_str();
    let mut out = Vec::new();
    let target = blast.and_then(|b| b.get("target"));
    if let Some(want) = &ident.exact_name {
        let got_blast = target
            .and_then(|t| {
                t.get("canonical_fqn")
                    .or_else(|| t.get("symbol"))
                    .and_then(|v| v.as_str())
            })
            .map(short_name)
            .unwrap_or_default();
        let got_cfg = cfg
            .and_then(|c| c.get("symbol"))
            .and_then(|s| s.as_str())
            .unwrap_or("");
        let ok = if !got_cfg.is_empty() {
            got_cfg == want
                && (got_blast == *want
                    || name_matches(
                        target
                            .and_then(|t| t.get("canonical_fqn"))
                            .and_then(|v| v.as_str())
                            .unwrap_or(""),
                        want,
                    ))
        } else {
            got_blast == *want
        };
        out.push(check(
            format!("symbol.{sid}.identity.exact_name"),
            sev,
            ok,
            format!("want={want:?} blast_short={got_blast:?} inspect_symbol={got_cfg:?}"),
        ));
    }
    if let (Some(want), Some(t)) = (&ident.canonical_fqn, target) {
        let got = t
            .get("canonical_fqn")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        out.push(check(
            format!("symbol.{sid}.identity.canonical_fqn"),
            sev,
            got == want,
            format!("want={want:?} got={got:?}"),
        ));
    }
    if let (Some(want), Some(t)) = (&ident.class_context, target) {
        let got = t
            .get("class_context")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        out.push(check(
            format!("symbol.{sid}.identity.class_context"),
            sev,
            got == want,
            format!("want={want:?} got={got:?}"),
        ));
    }
    if let (Some(want), Some(t)) = (&ident.file_suffix, target) {
        let path = t.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        out.push(check(
            format!("symbol.{sid}.identity.file_suffix"),
            sev,
            path.ends_with(want) || path.replace('\\', "/").contains(want),
            format!("want suffix {want:?} in {path:?}"),
        ));
    }
    if let (Some(want), Some(t)) = (&ident.language, target) {
        let got = t.get("language").and_then(|v| v.as_str()).unwrap_or("");
        out.push(check(
            format!("symbol.{sid}.identity.language"),
            sev,
            got == want,
            format!("want={want:?} got={got:?}"),
        ));
    }
    if let Some(want) = &ident.discriminator {
        let blob = target.map(|t| t.to_string()).unwrap_or_default();
        let cfg_blob = cfg.map(|c| c.to_string()).unwrap_or_default();
        let ok = blob.contains(want.as_str()) || cfg_blob.contains(want.as_str());
        out.push(check(
            format!("symbol.{sid}.identity.discriminator"),
            sev,
            ok,
            format!("want discriminator {want:?} in blast/inspect identity"),
        ));
    }
    out
}

fn check_v2_dimensions(
    sid: &str,
    entry: &SymbolEntry,
    m: &MatchSpec,
    bin: &Path,
    cwd: &Path,
) -> Vec<CheckResult> {
    let mut out = Vec::new();
    if let Some(spec) = &entry.mro {
        out.extend(check_mro(sid, spec, m, bin, cwd));
    }
    if let Some(spec) = &entry.slice {
        out.extend(check_text_command(
            sid,
            "slice",
            &spec.severity,
            &spec.text,
            bin,
            cwd,
            &slice_args(spec),
        ));
    }
    if let Some(spec) = &entry.flows {
        out.extend(check_text_command(
            sid,
            "flows",
            &spec.severity,
            &spec.text,
            bin,
            cwd,
            &flows_args(spec),
        ));
    }
    if let Some(spec) = &entry.mutations {
        out.extend(check_text_command(
            sid,
            "mutations",
            &spec.severity,
            &spec.text,
            bin,
            cwd,
            &mutation_args(spec),
        ));
    }
    if let Some(spec) = &entry.taint {
        out.extend(check_taint(sid, spec, bin, cwd));
    }
    if let Some(spec) = &entry.clones {
        out.extend(check_clones(sid, &m.name, spec, bin, cwd));
    }
    if let Some(spec) = &entry.semantic {
        out.extend(check_semantic(sid, spec, bin, cwd));
    }
    if let Some(spec) = &entry.review {
        out.extend(check_review(sid, spec, bin, cwd));
    }
    out
}

fn slice_args(spec: &SliceSpec) -> Vec<String> {
    let mut args = vec![
        "-f".into(),
        "json".into(),
        "slice".into(),
        spec.file.clone(),
        "--line".into(),
        spec.line.to_string(),
        "--variable".into(),
        spec.variable.clone(),
        "--direction".into(),
        spec.direction.clone(),
    ];
    if let Some(function) = &spec.function {
        args.push("--function".into());
        args.push(function.clone());
    }
    args
}

fn flows_args(spec: &FlowsSpec) -> Vec<String> {
    let mut args = vec![
        "-f".into(),
        "json".into(),
        "cpg".into(),
        "flows".into(),
        spec.file.clone(),
        "--line".into(),
        spec.line.to_string(),
        "--variable".into(),
        spec.variable.clone(),
        "--direction".into(),
        spec.direction.clone(),
    ];
    if spec.with_alias {
        args.push("--with-alias".into());
    }
    if let Some(function) = &spec.function {
        args.push("--function".into());
        args.push(function.clone());
    }
    args
}

fn mutation_args(spec: &MutationsSpec) -> Vec<String> {
    let mut args = vec![
        "-f".into(),
        "json".into(),
        "cpg".into(),
        "mutations".into(),
        "--type".into(),
        spec.type_name.clone(),
    ];
    if spec.exclude_ctors {
        args.push("--exclude-ctors".into());
    }
    if let Some(member) = &spec.member {
        args.push("--member".into());
        args.push(member.clone());
    }
    args
}

fn check_text_command(
    sid: &str,
    kind: &str,
    severity: &str,
    text: &TextExpect,
    bin: &Path,
    cwd: &Path,
    args: &[String],
) -> Vec<CheckResult> {
    if severity == "unsupported" {
        return vec![];
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let Some(doc) = run_json(bin, cwd, &refs) else {
        return vec![check(
            format!("symbol.{sid}.{kind}"),
            severity,
            false,
            format!("{kind} command failed"),
        )];
    };
    text_checks(sid, kind, severity, text, &doc.to_string())
}

fn text_checks(
    sid: &str,
    kind: &str,
    severity: &str,
    text: &TextExpect,
    blob: &str,
) -> Vec<CheckResult> {
    let mut out = Vec::new();
    for needle in &text.must_contain {
        out.push(check(
            format!("symbol.{sid}.{kind}.contains"),
            severity,
            blob.contains(needle),
            format!("want {needle:?} in {kind} output"),
        ));
    }
    for needle in &text.must_exclude {
        out.push(check(
            format!("symbol.{sid}.{kind}.excludes"),
            severity,
            !blob.contains(needle),
            format!("want {needle:?} absent from {kind} output"),
        ));
    }
    if text.must_contain.is_empty() && text.must_exclude.is_empty() {
        out.push(check(
            format!("symbol.{sid}.{kind}"),
            severity,
            true,
            format!("{kind} command ok"),
        ));
    }
    out
}

fn check_mro(sid: &str, spec: &MroSpec, m: &MatchSpec, bin: &Path, cwd: &Path) -> Vec<CheckResult> {
    if spec.severity == "unsupported" {
        return vec![];
    }
    let mut out = Vec::new();
    let owner = m.class.as_deref().unwrap_or(m.name.as_str());
    for (edge, from, names) in [
        ("extends", owner, &spec.extends),
        ("implements", owner, &spec.implements),
        ("overrides", m.name.as_str(), &spec.overrides),
    ] {
        if names.is_empty() {
            continue;
        }
        let doc = run_json(
            bin,
            cwd,
            &[
                "-f",
                "json",
                "relations",
                "--edge",
                edge,
                "--limit",
                "100000",
            ],
        );
        let Some(doc) = doc else {
            out.push(check(
                format!("symbol.{sid}.mro.{edge}"),
                &spec.severity,
                false,
                format!("relations --edge {edge} failed"),
            ));
            continue;
        };
        let edges = doc.get("edges").and_then(|v| v.as_array());
        for name in names {
            let ok = edges.is_some_and(|rows| {
                rows.iter().any(|row| {
                    let src = row
                        .get("source")
                        .and_then(|n| n.get("name").or_else(|| n.get("qualified_name")))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let dst = row
                        .get("target")
                        .and_then(|n| n.get("name").or_else(|| n.get("qualified_name")))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    name_matches(src, from) && name_matches(dst, name)
                })
            });
            out.push(check(
                format!("symbol.{sid}.mro.{edge}"),
                &spec.severity,
                ok,
                format!("{from} -[{edge}]-> {name}"),
            ));
        }
    }
    out
}

fn check_taint(sid: &str, spec: &TaintSpec, bin: &Path, cwd: &Path) -> Vec<CheckResult> {
    if spec.severity == "unsupported" {
        return vec![];
    }
    let Some(doc) = run_json(
        bin,
        cwd,
        &[
            "-f",
            "json",
            "security",
            "taint",
            "--sink",
            &spec.sink,
            "--source",
            &spec.source,
        ],
    ) else {
        return vec![check(
            format!("symbol.{sid}.taint"),
            &spec.severity,
            false,
            "security taint failed",
        )];
    };
    let mut out = text_checks(sid, "taint", &spec.severity, &spec.text, &doc.to_string());
    if let Some(min) = spec.min_flows {
        let got = doc
            .get("flows")
            .and_then(|v| v.as_array())
            .map(|rows| rows.len())
            .unwrap_or(0);
        out.push(check(
            format!("symbol.{sid}.taint.min_flows"),
            &spec.severity,
            got >= min,
            format!("flows={got} min={min}"),
        ));
    }
    out
}

fn check_clones(
    sid: &str,
    symbol: &str,
    spec: &ClonesSpec,
    bin: &Path,
    cwd: &Path,
) -> Vec<CheckResult> {
    if spec.severity == "unsupported" {
        return vec![];
    }
    let min_loc = spec.min_loc.unwrap_or(1).to_string();
    let mut args = vec![
        "-f",
        "json",
        "clones",
        "--mode",
        spec.mode.as_str(),
        "--no-write",
        "--min-loc",
        min_loc.as_str(),
    ];
    if let Some(seed) = &spec.seed {
        args.push("--seed");
        args.push(seed);
    }
    let Some(doc) = run_json(bin, cwd, &args) else {
        return vec![check(
            format!("symbol.{sid}.clones"),
            &spec.severity,
            false,
            "clones command failed",
        )];
    };
    let groups = doc
        .get("groups")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let names_of = |group: &Value| -> Vec<String> {
        group
            .get("members")
            .and_then(|v| v.as_array())
            .map(|members| {
                members
                    .iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mine: Vec<Vec<String>> = groups
        .iter()
        .map(names_of)
        .filter(|names| names.iter().any(|n| name_matches(n, symbol)))
        .collect();
    let mut out = Vec::new();
    if spec.group_with.is_empty() && spec.not_grouped_with.is_empty() {
        out.push(check(
            format!("symbol.{sid}.clones"),
            &spec.severity,
            true,
            format!("clones {} groups={}", spec.mode, groups.len()),
        ));
        return out;
    }
    for peer in &spec.group_with {
        let ok = mine
            .iter()
            .any(|names| names.iter().any(|n| name_matches(n, peer)));
        out.push(check(
            format!("symbol.{sid}.clones.group_with"),
            &spec.severity,
            ok,
            format!("{symbol} grouped with {peer} in mode {}", spec.mode),
        ));
    }
    for peer in &spec.not_grouped_with {
        let ok = !mine
            .iter()
            .any(|names| names.iter().any(|n| name_matches(n, peer)));
        out.push(check(
            format!("symbol.{sid}.clones.not_grouped_with"),
            &spec.severity,
            ok,
            format!("{symbol} not grouped with {peer} in mode {}", spec.mode),
        ));
    }
    out
}

fn check_semantic(sid: &str, spec: &SemanticSpec, bin: &Path, cwd: &Path) -> Vec<CheckResult> {
    if spec.severity == "unsupported" {
        return vec![];
    }
    if run_json(
        bin,
        cwd,
        &["-f", "json", "semantic", "index", "--embedder", "vocab"],
    )
    .is_none()
    {
        return vec![check(
            format!("symbol.{sid}.semantic"),
            &spec.severity,
            false,
            "semantic index --embedder vocab failed",
        )];
    }
    let mut args = vec![
        "-f",
        "json",
        "semantic",
        "query",
        spec.query.as_str(),
        "--limit",
        "3",
    ];
    if spec.expand_neighbors {
        args.extend(["--expand", "neighbors"]);
    }
    let Some(doc) = run_json(bin, cwd, &args) else {
        return vec![check(
            format!("symbol.{sid}.semantic"),
            &spec.severity,
            false,
            "semantic query failed",
        )];
    };
    let top: Vec<String> = doc
        .get("hits")
        .and_then(|v| v.as_array())
        .map(|hits| {
            hits.iter()
                .take(3)
                .filter_map(|h| h.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut out = vec![check(
        format!("symbol.{sid}.semantic.top3"),
        &spec.severity,
        top.iter().any(|n| name_matches(n, &spec.top3_contains)),
        format!("top3={top:?} want {}", spec.top3_contains),
    )];
    if spec.expand_neighbors && !spec.neighbor_names.is_empty() {
        let neighbors: Vec<String> = doc
            .pointer("/expansion/neighbors")
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|n| n.get("name").and_then(|v| v.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        for name in &spec.neighbor_names {
            out.push(check(
                format!("symbol.{sid}.semantic.neighbor"),
                &spec.severity,
                neighbors.iter().any(|n| name_matches(n, name)),
                format!("neighbors={neighbors:?} want {name}"),
            ));
        }
    }
    out
}

fn check_review(sid: &str, spec: &ReviewSpec, bin: &Path, cwd: &Path) -> Vec<CheckResult> {
    if spec.severity == "unsupported" {
        return vec![];
    }
    let path = cwd.join(&spec.file);
    let Ok(original) = fs::read_to_string(&path) else {
        return vec![check(
            format!("symbol.{sid}.review"),
            &spec.severity,
            false,
            format!("missing {}", spec.file),
        )];
    };
    if !original.contains(&spec.find) {
        return vec![check(
            format!("symbol.{sid}.review"),
            &spec.severity,
            false,
            format!("worktree file {} lacks {:?}", spec.file, spec.find),
        )];
    }
    if !cwd.join(".git").exists() {
        let _ = fs::write(cwd.join(".gitignore"), ".rgctl\n");
        let init = git_sandbox(cwd, &["init"]);
        let add = git_sandbox(cwd, &["add", "-A"]);
        let commit = git_sandbox(cwd, &["commit", "-m", "facts-base"]);
        if !init.status.success() || !add.status.success() || !commit.status.success() {
            return vec![check(
                format!("symbol.{sid}.review"),
                &spec.severity,
                false,
                "sandbox git init/commit failed",
            )];
        }
    }
    let edited = original.replacen(&spec.find, &spec.replace, 1);
    if fs::write(&path, &edited).is_err() {
        return vec![check(
            format!("symbol.{sid}.review"),
            &spec.severity,
            false,
            format!("failed to edit {}", spec.file),
        )];
    }
    let doc = run_json(
        bin,
        cwd,
        &[
            "-f",
            "json",
            "review",
            "paths",
            "--synthetic-head",
            "worktree",
        ],
    );
    let _ = fs::write(&path, &original);
    let Some(doc) = doc else {
        return vec![check(
            format!("symbol.{sid}.review"),
            &spec.severity,
            false,
            "review paths --synthetic-head worktree failed",
        )];
    };
    let blob = doc.to_string();
    let mut out = Vec::new();
    for name in &spec.must_contain {
        out.push(check(
            format!("symbol.{sid}.review.paths"),
            &spec.severity,
            blob.contains(name),
            format!("want {name} in review paths"),
        ));
    }
    if spec.must_contain.is_empty() {
        out.push(check(
            format!("symbol.{sid}.review"),
            &spec.severity,
            true,
            "review paths ok",
        ));
    }
    out
}

fn git_sandbox(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "rgctl-test")
        .env("GIT_AUTHOR_EMAIL", "rgctl-test@example.com")
        .env("GIT_COMMITTER_NAME", "rgctl-test")
        .env("GIT_COMMITTER_EMAIL", "rgctl-test@example.com")
        .output()
        .expect("spawn git")
}

fn check_pdg(sid: &str, ps: &PdgSpec, pdg: &Value) -> Vec<CheckResult> {
    let sev = ps.severity.as_str();
    let mut out = Vec::new();
    let nodes = pdg.get("nodes").and_then(|n| n.as_array());
    let edges = pdg.get("edges").and_then(|e| e.as_array());
    let ncount = nodes.map(|a| a.len()).unwrap_or(0);
    let labels: Vec<String> = nodes
        .map(|arr| {
            arr.iter()
                .filter_map(|n| n.get("label").and_then(|l| l.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let kinds: Vec<String> = nodes
        .map(|arr| {
            arr.iter()
                .filter_map(|n| n.get("kind").and_then(|k| k.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if let Some(want) = ps.exact_data_deps {
        let got = pdg.get("data_deps").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        out.push(check(
            format!("symbol.{sid}.pdg.exact_data_deps"),
            sev,
            got == want,
            format!("data_deps={got} expected={want}"),
        ));
    }
    if let Some(want) = ps.exact_control_deps {
        let got = pdg
            .get("control_deps")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        out.push(check(
            format!("symbol.{sid}.pdg.exact_control_deps"),
            sev,
            got == want,
            format!("control_deps={got} expected={want}"),
        ));
    }
    if let Some(want) = ps.exact_node_count {
        out.push(check(
            format!("symbol.{sid}.pdg.exact_node_count"),
            sev,
            ncount == want,
            format!("nodes={ncount} expected={want}"),
        ));
    }
    if let Some(frags) = &ps.node_labels_contain {
        for frag in frags {
            out.push(check(
                format!("symbol.{sid}.pdg.node_labels_contain"),
                sev,
                labels.iter().any(|l| l.contains(frag)),
                format!("want {frag:?} in labels {labels:?}"),
            ));
        }
    }
    if let Some(want_edges) = &ps.data_edges {
        let edges = edges.cloned().unwrap_or_default();
        for es in want_edges {
            let ok = edges.iter().any(|e| {
                e.get("kind").and_then(|k| k.as_str()) == Some("data")
                    && (es.variable.is_none()
                        || e.get("variable").and_then(|v| v.as_str()) == es.variable.as_deref())
            });
            out.push(check(
                format!("symbol.{sid}.pdg.data_edge"),
                sev,
                ok,
                format!("want data edge variable={:?} in {edges:?}", es.variable),
            ));
        }
    }
    if let Some(want) = &ps.exact_node_kinds {
        let mut w = want.clone();
        w.sort();
        let mut g = kinds.clone();
        g.sort();
        out.push(check(
            format!("symbol.{sid}.pdg.exact_node_kinds"),
            sev,
            g == w,
            format!("kinds actual={g:?} expected={w:?}"),
        ));
    }
    out
}

fn check_dom(sid: &str, ds: &DomSpec, dom: &Value) -> Vec<CheckResult> {
    let sev = ds.severity.as_str();
    let mut out = Vec::new();
    let nblocks = dom
        .get("nodes")
        .and_then(|n| n.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let idom = dom
        .get("idom")
        .and_then(|i| i.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(want) = ds.exact_block_count {
        out.push(check(
            format!("symbol.{sid}.dom.exact_block_count"),
            sev,
            nblocks == want,
            format!("blocks={nblocks} expected={want}"),
        ));
    }
    if let Some(pairs) = &ds.idom_contains {
        for pair in pairs {
            let ok = idom.iter().any(|x| {
                x.get("block").and_then(|v| v.as_i64()) == Some(pair.block)
                    && x.get("immediate_dominator").and_then(|v| v.as_i64())
                        == Some(pair.immediate_dominator)
            });
            out.push(check(
                format!("symbol.{sid}.dom.idom_contains"),
                sev,
                ok,
                format!(
                    "want idom block={}→{} in {idom:?}",
                    pair.block, pair.immediate_dominator
                ),
            ));
        }
    }
    if let Some(exact) = &ds.exact_idom {
        let mut want: Vec<(i64, i64)> = exact
            .iter()
            .map(|p| (p.block, p.immediate_dominator))
            .collect();
        want.sort();
        let mut got: Vec<(i64, i64)> = idom
            .iter()
            .filter_map(|x| {
                Some((
                    x.get("block")?.as_i64()?,
                    x.get("immediate_dominator")?.as_i64()?,
                ))
            })
            .collect();
        got.sort();
        out.push(check(
            format!("symbol.{sid}.dom.exact_idom"),
            sev,
            got == want,
            format!("idom actual={got:?} expected={want:?}"),
        ));
    }
    out
}

fn check_invariants(
    facts: &ExpectedFacts,
    bin: &Path,
    cwd: &Path,
    index: &GraphIndex,
) -> Vec<CheckResult> {
    let mut out = Vec::new();
    let inv = &facts.invariants;

    if inv.b2.enabled.unwrap_or(true) {
        out.push(check(
            "invariant.B2_gql_calls_nonzero",
            &inv.b2.severity,
            index.calls_count() > 0,
            format!("calls_edges={}", index.calls_count()),
        ));
    }

    if inv.b1.enabled.unwrap_or(true) {
        for sid in &inv.b1.symbols {
            let Some(entry) = facts.symbols.get(sid) else {
                continue;
            };
            let m = entry.match_spec();
            let structured_callers: BTreeSet<String> = index
                .callers_of(&m.name, m.class.as_deref())
                .into_iter()
                .map(|n| short_name(&n))
                .collect();
            match run_blast(bin, cwd, &m) {
                None => out.push(check(
                    format!("invariant.B1.{sid}"),
                    &inv.b1.severity,
                    false,
                    "blast-radius failed",
                )),
                Some(b) => {
                    let blast: BTreeSet<String> = b
                        .pointer("/topology/direct_callers")
                        .and_then(|c| c.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|c| {
                                    c.get("fqn").and_then(|f| f.as_str()).map(short_name)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    out.push(check(
                        format!("invariant.B1.{sid}"),
                        &inv.b1.severity,
                        structured_callers == blast,
                        format!(
                            "structured_callers={:?} blast_callers={:?}",
                            structured_callers.iter().collect::<Vec<_>>(),
                            blast.iter().collect::<Vec<_>>()
                        ),
                    ));
                }
            }
        }
    }

    if inv.b5.enabled.unwrap_or(true) {
        for sid in &inv.b5.symbols {
            let Some(entry) = facts.symbols.get(sid) else {
                continue;
            };
            let mut m = entry.match_spec();
            if m.file.is_none() {
                if let Some(ident) = &entry.identity {
                    m.file = ident.file_suffix.clone();
                }
            }
            let cfg = run_inspect(bin, cwd, &m, "cfg");
            let ok = cfg
                .as_ref()
                .is_some_and(|c| c.get("nodes").is_some() || c.get("edges").is_some());
            out.push(check(
                format!("invariant.B5.{sid}"),
                &inv.b5.severity,
                ok,
                format!("inspect cfg for {:?}", symbol_candidates(&m)),
            ));
        }
    }

    if inv.b6.enabled.unwrap_or(false) {
        for sid in &inv.b6.symbols {
            let Some(entry) = facts.symbols.get(sid) else {
                continue;
            };
            let m = entry.match_spec();
            let Some(cfg) = run_inspect(bin, cwd, &m, "cfg") else {
                out.push(check(
                    format!("invariant.B6.{sid}"),
                    &inv.b6.severity,
                    false,
                    "inspect cfg failed",
                ));
                continue;
            };
            let texts = cfg_texts(&cfg);
            let callees: BTreeSet<String> = index
                .callees_of(&m.name, m.class.as_deref())
                .into_iter()
                .collect();
            let mut expect = BTreeSet::new();
            if let Some(exact) = &entry.exact_callees {
                expect.extend(exact.iter().cloned());
            }
            if let Some(peers) = &entry.direct_callees {
                expect.extend(peers.iter().map(|p| p.name.clone()));
            }
            if !expect.is_empty() {
                let missing: Vec<_> = expect
                    .iter()
                    .filter(|c| !callees.contains(*c))
                    .cloned()
                    .collect();
                let ast_missing: Vec<_> = expect
                    .iter()
                    .filter(|c| !texts.iter().any(|t| t.contains(&format!("{c}("))))
                    .cloned()
                    .collect();
                out.push(check(
                    format!("invariant.B6.{sid}.callees_in_graph"),
                    &inv.b6.severity,
                    missing.is_empty(),
                    format!(
                        "expected callees missing from CALLS: {missing:?}; cfg_texts={texts:?}"
                    ),
                ));
                out.push(check(
                    format!("invariant.B6.{sid}.callees_in_ast"),
                    &inv.b6.severity,
                    ast_missing.is_empty(),
                    format!("expected callees missing from CFG/AST text: {ast_missing:?}"),
                ));
            }
        }
    }

    if inv.b7.enabled.unwrap_or(false) {
        for sid in &inv.b7.symbols {
            let Some(entry) = facts.symbols.get(sid) else {
                continue;
            };
            let m = entry.match_spec();
            let cfg = run_inspect(bin, cwd, &m, "cfg");
            let pdg = run_inspect(bin, cwd, &m, "pdg");
            match (cfg, pdg) {
                (Some(cfg), Some(pdg)) => {
                    let mut cfg_lines = BTreeSet::new();
                    if let Some(nodes) = cfg.get("nodes").and_then(|n| n.as_array()) {
                        for node in nodes {
                            if let Some(stmts) = node.get("statements").and_then(|s| s.as_array()) {
                                for s in stmts {
                                    if let Some(line) = s.get("line").and_then(|l| l.as_i64()) {
                                        cfg_lines.insert(line);
                                    }
                                }
                            }
                        }
                    }
                    let mut pdg_lines = BTreeSet::new();
                    if let Some(nodes) = pdg.get("nodes").and_then(|n| n.as_array()) {
                        for n in nodes {
                            if let Some(line) = n.get("line").and_then(|l| l.as_i64()) {
                                pdg_lines.insert(line);
                            }
                        }
                    }
                    let extra: Vec<_> = pdg_lines.difference(&cfg_lines).copied().collect();
                    out.push(check(
                        format!("invariant.B7.{sid}"),
                        &inv.b7.severity,
                        extra.is_empty(),
                        format!(
                            "pdg_lines={:?} cfg_lines={:?} extra={extra:?}",
                            pdg_lines.iter().collect::<Vec<_>>(),
                            cfg_lines.iter().collect::<Vec<_>>()
                        ),
                    ));
                }
                _ => out.push(check(
                    format!("invariant.B7.{sid}"),
                    &inv.b7.severity,
                    false,
                    "cfg/pdg missing",
                )),
            }
        }
    }

    if inv.b8.enabled.unwrap_or(false) {
        for sid in &inv.b8.symbols {
            let Some(entry) = facts.symbols.get(sid) else {
                continue;
            };
            let m = entry.match_spec();
            let cfg = run_inspect(bin, cwd, &m, "cfg");
            let dom = run_inspect(bin, cwd, &m, "dom");
            match (cfg, dom) {
                (Some(cfg), Some(dom)) => {
                    let cfg_idx: Vec<i64> = cfg
                        .get("nodes")
                        .and_then(|n| n.as_array())
                        .map(|arr| {
                            let mut v: Vec<i64> = arr
                                .iter()
                                .filter_map(|n| n.get("block_index").and_then(|b| b.as_i64()))
                                .collect();
                            v.sort();
                            v
                        })
                        .unwrap_or_default();
                    let dom_idx: Vec<i64> = dom
                        .get("nodes")
                        .and_then(|n| n.as_array())
                        .map(|arr| {
                            let mut v: Vec<i64> = arr
                                .iter()
                                .filter_map(|n| n.get("block_index").and_then(|b| b.as_i64()))
                                .collect();
                            v.sort();
                            v
                        })
                        .unwrap_or_default();
                    out.push(check(
                        format!("invariant.B8.{sid}"),
                        &inv.b8.severity,
                        cfg_idx == dom_idx,
                        format!("cfg_blocks={cfg_idx:?} dom_blocks={dom_idx:?}"),
                    ));
                }
                _ => out.push(check(
                    format!("invariant.B8.{sid}"),
                    &inv.b8.severity,
                    false,
                    "cfg/dom missing",
                )),
            }
        }
    }

    if inv.b9.enabled.unwrap_or(false) {
        for sid in &inv.b9.symbols {
            let Some(entry) = facts.symbols.get(sid) else {
                continue;
            };
            let m = entry.match_spec();
            match run_blast(bin, cwd, &m) {
                None => out.push(check(
                    format!("invariant.B9.{sid}"),
                    &inv.b9.severity,
                    false,
                    "blast failed",
                )),
                Some(b) => {
                    let fqn = b
                        .pointer("/target/canonical_fqn")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let got = short_name(fqn);
                    out.push(check(
                        format!("invariant.B9.{sid}"),
                        &inv.b9.severity,
                        got == m.name,
                        format!(
                            "match.name={:?} blast.canonical_short={got:?} fqn={fqn:?}",
                            m.name
                        ),
                    ));
                }
            }
        }
    }

    out
}

fn check_analytics(spec: &AnalyticsSpec, cwd: &Path) -> Vec<CheckResult> {
    use rgctl::analysis::{
        AnalysisResults, FlatGraphIndex, HarmonicCentrality, HyperBallHarmonic, PetGraphView,
        default_behavioral_edges,
    };
    use rgctl_graph::{SNAPSHOT_FILE, SnapshotNodeStore};

    let sev = spec.severity.as_str();
    if sev == "unsupported" {
        return vec![];
    }
    let mut out = Vec::new();
    let snap = cwd.join(".rgctl").join(SNAPSHOT_FILE);
    let store = match SnapshotNodeStore::open(&snap) {
        Ok(store) => store,
        Err(e) => {
            return vec![check(
                "analytics.snapshot",
                sev,
                false,
                format!("open snapshot: {e}"),
            )];
        }
    };
    let Ok(view) = PetGraphView::from_snapshot_store(&store) else {
        return vec![check("analytics.view", sev, false, "snapshot view failed")];
    };
    let analysis = AnalysisResults::load(&cwd.join(".rgctl/analysis_results.bin"));
    let Ok(analysis) = analysis else {
        return vec![check(
            "analytics.results",
            sev,
            false,
            "missing analysis_results.bin",
        )];
    };

    let resolve = |name: &str| -> Option<uuid::Uuid> {
        let nodes = store.mmap().columnar()?.find_nodes_by_name(name).ok()?;
        let mut funcs: Vec<_> = nodes
            .into_iter()
            .filter(|n| n.node_type == rgctl_graph::schema::NodeType::Function)
            .collect();
        funcs.sort_by_key(|n| {
            let path = n.file_path.as_deref().unwrap_or("");
            usize::from(!path.contains("correctness"))
        });
        funcs.first().map(|n| n.id)
    };

    let (Some(root), Some(mid), Some(leaf)) =
        (resolve(&spec.root), resolve(&spec.mid), resolve(&spec.leaf))
    else {
        return vec![check(
            "analytics.symbols",
            sev,
            false,
            format!("missing chain {} / {} / {}", spec.root, spec.mid, spec.leaf),
        )];
    };

    let score = |id| analysis.get_centrality(id);
    let (Some(root_c), Some(mid_c), Some(leaf_c)) = (score(root), score(mid), score(leaf)) else {
        return vec![check(
            "analytics.centrality",
            sev,
            false,
            "centrality row missing for the chain",
        )];
    };
    let near0 = |v: f32| v.abs() < 1e-4;
    out.push(check(
        "analytics.harmonic.order",
        sev,
        root_c.harmonic > mid_c.harmonic
            && mid_c.harmonic > leaf_c.harmonic
            && near0(leaf_c.harmonic),
        format!(
            "harmonic root={} mid={} leaf={}",
            root_c.harmonic, mid_c.harmonic, leaf_c.harmonic
        ),
    ));
    out.push(check(
        "analytics.pagerank.order",
        sev,
        leaf_c.pagerank > mid_c.pagerank && mid_c.pagerank > root_c.pagerank,
        format!(
            "pagerank leaf={} mid={} root={}",
            leaf_c.pagerank, mid_c.pagerank, root_c.pagerank
        ),
    ));
    out.push(check(
        "analytics.betweenness",
        sev,
        mid_c.betweenness > 0.0 && near0(root_c.betweenness) && near0(leaf_c.betweenness),
        format!(
            "betweenness root={} mid={} leaf={}",
            root_c.betweenness, mid_c.betweenness, leaf_c.betweenness
        ),
    ));

    let allowed = default_behavioral_edges();
    let exact = HarmonicCentrality::compute(&view, allowed, 50_000);
    let index = FlatGraphIndex::from_view(&view, allowed);
    let hyper = HyperBallHarmonic::compute_flat(&index, 16);
    let at = |id: uuid::Uuid| -> Option<usize> {
        view.index_uuid_iter()
            .find(|(_, uuid)| *uuid == id)
            .map(|(idx, _)| idx.index())
    };
    let agreement = match (
        exact.get(&root),
        exact.get(&mid),
        exact.get(&leaf),
        at(root),
        at(mid),
        at(leaf),
    ) {
        (Some(er), Some(em), Some(el), Some(ri), Some(mi), Some(li)) => {
            let exact_ok = *er > *em && *em > *el && el.abs() < 1e-6;
            let hr = hyper.get(ri).copied().unwrap_or(0.0);
            let hm = hyper.get(mi).copied().unwrap_or(0.0);
            let hl = hyper.get(li).copied().unwrap_or(0.0);
            let hyper_ok = hr > hm && hm > hl;
            out.push(check(
                "analytics.harmonic.hyperball",
                sev,
                exact_ok && hyper_ok,
                format!("exact=({er},{em},{el}) hyper=({hr},{hm},{hl})"),
            ));
            true
        }
        _ => false,
    };
    if !agreement {
        out.push(check(
            "analytics.harmonic.hyperball",
            sev,
            false,
            "exact or HyperBall scores missing for the chain",
        ));
    }

    let plan_path = cwd.join(".rgctl/migration_plan.json");
    let plan_ok = fs::read_to_string(&plan_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .is_some_and(|doc| json_has_positive_avg_harmonic(&doc));
    out.push(check(
        "analytics.migration_plan.avg_harmonic",
        sev,
        plan_ok,
        "migration_plan.json avg_harmonic finite and > 0",
    ));

    if let Some(table) = analysis.community.as_ref() {
        if spec.separate.len() == 2 {
            let left = resolve(&spec.separate[0]).and_then(|id| analysis.get_community(id));
            let right = resolve(&spec.separate[1]).and_then(|id| analysis.get_community(id));
            let ok = match (left, right) {
                (Some(a), Some(b)) => a != b,
                _ => false,
            };
            out.push(check(
                "analytics.communities.separate",
                sev,
                ok,
                format!(
                    "{:?} vs {:?} communities {left:?}/{right:?}",
                    spec.separate[0], spec.separate[1]
                ),
            ));
        }
        let infra = table.infrastructure_community_id;
        let (connected, conn_detail) =
            match rgctl_analysis::community::build_community_neighbor_lists(&view, &store) {
                Ok(neighbors) => {
                    let mut groups: std::collections::HashMap<usize, Vec<usize>> =
                        std::collections::HashMap::new();
                    for (idx, id) in view.index_uuid_iter() {
                        let Some(cid) = analysis.get_community(id) else {
                            continue;
                        };
                        if Some(cid) == infra {
                            continue;
                        }
                        groups.entry(cid).or_default().push(idx.index());
                    }
                    let mut disconn = None;
                    for (cid, group) in &groups {
                        if group.len() < 2 {
                            continue;
                        }
                        let group_set: std::collections::HashSet<usize> =
                            group.iter().copied().collect();
                        let mut seen = std::collections::HashSet::from([group[0]]);
                        let mut stack = vec![group[0]];
                        while let Some(u) = stack.pop() {
                            for &v in &neighbors[u] {
                                if group_set.contains(&v) && seen.insert(v) {
                                    stack.push(v);
                                }
                            }
                        }
                        if seen.len() != group.len() {
                            disconn = Some(format!(
                                "community {cid} (label={:?}) reached {}/{} members",
                                table.label(*cid),
                                seen.len(),
                                group.len()
                            ));
                            break;
                        }
                    }
                    match disconn {
                        Some(msg) => (false, msg),
                        None => (
                            true,
                            "each non-infrastructure community induces a connected subgraph on the community projection".to_string(),
                        ),
                    }
                }
                Err(e) => (false, format!("build_community_neighbor_lists failed: {e}")),
            };
        out.push(check(
            "analytics.communities.connected",
            sev,
            connected,
            conn_detail,
        ));
    } else {
        out.push(check(
            "analytics.communities",
            sev,
            false,
            "community table missing",
        ));
    }
    out
}

fn json_has_positive_avg_harmonic(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            if let Some(n) = map.get("avg_harmonic").and_then(|v| v.as_f64()) {
                if n.is_finite() && n > 0.0 {
                    return true;
                }
            }
            map.values().any(json_has_positive_avg_harmonic)
        }
        Value::Array(rows) => rows.iter().any(json_has_positive_avg_harmonic),
        _ => false,
    }
}

#[cfg(test)]
mod v2_schema {
    use super::ExpectedFacts;

    #[test]
    fn v2_fields_deserialize() {
        let raw = r#"{
          "symbols": {
            "over": {
              "match": {"name": "process", "discriminator": "(int)", "line": 12},
              "identity": {"discriminator": "(int)"},
              "mro": {"overrides": ["process"]},
              "slice": {"file": "src/A.java", "line": 4, "variable": "y", "must_exclude": ["99"]},
              "flows": {"file": "src/A.java", "line": 4, "variable": "y", "must_contain": ["alias"]},
              "mutations": {"type": "Order", "must_exclude": ["<init>"]},
              "taint": {"sink": "sink", "min_flows": 1},
              "clones": {"mode": "exact", "group_with": ["cloneA"], "not_grouped_with": ["sibling"]},
              "semantic": {"query": "refund", "top3_contains": "processRefund", "neighbor_names": ["save"]},
              "review": {"file": "src/A.java", "find": "a", "replace": "b", "must_contain": ["root"]}
            }
          },
          "analytics": {
            "root": "correctnessRoot",
            "mid": "correctnessMid",
            "leaf": "correctnessLeaf",
            "separate": ["left", "right"]
          }
        }"#;
        let facts: ExpectedFacts = serde_json::from_str(raw).expect("v2 facts");
        let sym = facts.symbols.get("over").expect("symbol");
        let matched = sym.match_spec();
        assert_eq!(matched.discriminator.as_deref(), Some("(int)"));
        assert_eq!(matched.line, Some(12));
        assert_eq!(
            sym.identity
                .as_ref()
                .and_then(|i| i.discriminator.as_deref()),
            Some("(int)")
        );
        assert_eq!(
            sym.mro.as_ref().map(|m| m.overrides.clone()),
            Some(vec!["process".to_string()])
        );
        assert_eq!(
            sym.slice.as_ref().map(|s| s.direction.as_str()),
            Some("backward")
        );
        assert!(sym.flows.as_ref().is_some_and(|f| f.with_alias));
        assert!(sym.mutations.as_ref().is_some_and(|m| m.exclude_ctors));
        assert_eq!(sym.taint.as_ref().and_then(|t| t.min_flows), Some(1));
        assert_eq!(sym.clones.as_ref().map(|c| c.mode.as_str()), Some("exact"));
        assert!(sym.semantic.as_ref().is_some_and(|s| s.expand_neighbors));
        assert_eq!(sym.review.as_ref().map(|r| r.find.as_str()), Some("a"));
        let analytics = facts.analytics.as_ref().expect("analytics");
        assert_eq!(analytics.root, "correctnessRoot");
        assert_eq!(
            analytics.separate,
            ["left".to_string(), "right".to_string()]
        );
    }
}
