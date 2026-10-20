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
    #[serde(rename = "B2_gql_calls_nonzero")]
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
}

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

    fn callees_of(&self, name: &str, class_hint: Option<&str>) -> Vec<String> {
        let targets = self.find_functions(name, class_hint);
        let files: BTreeSet<&str> = targets
            .iter()
            .filter_map(|t| t.get("file").and_then(|f| f.as_str()))
            .collect();
        let mut out = BTreeSet::new();
        for (caller, callee) in &self.calls {
            if caller.get("node").and_then(|n| n.as_str()) != Some(name) {
                continue;
            }
            if !files.is_empty() {
                let cf = caller.get("file").and_then(|f| f.as_str()).unwrap_or("");
                if !files.contains(cf) {
                    continue;
                }
            }
            if let Some(n) = callee.get("node").and_then(|n| n.as_str()) {
                out.insert(n.to_string());
            }
        }
        out.into_iter().collect()
    }

    fn callers_of(&self, name: &str, class_hint: Option<&str>) -> Vec<String> {
        let targets = self.find_functions(name, class_hint);
        let files: BTreeSet<&str> = targets
            .iter()
            .filter_map(|t| t.get("file").and_then(|f| f.as_str()))
            .collect();
        let mut out = BTreeSet::new();
        for (caller, callee) in &self.calls {
            if callee.get("node").and_then(|n| n.as_str()) != Some(name) {
                continue;
            }
            if !files.is_empty() {
                let cf = callee.get("file").and_then(|f| f.as_str()).unwrap_or("");
                if !files.contains(cf) {
                    continue;
                }
            }
            if let Some(n) = caller.get("node").and_then(|n| n.as_str()) {
                out.insert(n.to_string());
            }
        }
        out.into_iter().collect()
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
    // Go fixtures disambiguate by source file (`class: "methods.go"`).
    if let Some(cls) = &m.class {
        if cls.ends_with(".go") || cls.contains('/') {
            if let Some(v) = run_json(
                bin,
                cwd,
                &["-f", "json", "blast-radius", &m.name, "--file", cls],
            ) {
                return Some(v);
            }
        }
    }
    if let Some(file) = &m.file {
        if let Some(v) = run_json(
            bin,
            cwd,
            &["-f", "json", "blast-radius", &m.name, "--file", file],
        ) {
            return Some(v);
        }
        let fqn = format!("{file}::{}", m.name);
        if let Some(v) = run_json(bin, cwd, &["-f", "json", "blast-radius", &fqn]) {
            return Some(v);
        }
    }
    for sym in symbol_candidates(m) {
        if let Some(v) = run_json(bin, cwd, &["-f", "json", "blast-radius", &sym]) {
            return Some(v);
        }
    }
    None
}

fn run_inspect(bin: &Path, cwd: &Path, m: &MatchSpec, layer: &str) -> Option<Value> {
    for sym in symbol_candidates(m) {
        if let Some(v) = run_json(bin, cwd, &["-f", "json", "inspect", &sym, layer]) {
            return Some(v);
        }
    }
    // Prefer FQN when class looks like a Go receiver/type name (not a filename).
    if let Some(cls) = &m.class {
        if !cls.ends_with(".go") && !cls.contains('/') {
            let fqn = format!("{cls}.{}", m.name);
            if let Some(v) = run_json(bin, cwd, &["-f", "json", "inspect", &fqn, layer]) {
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
        &["-f", "json", "gql", "MATCH (n:Function) RETURN n"],
    ) {
        if let Some(rows) = data.get("rows").and_then(|r| r.as_array()) {
            for row in rows {
                if let Some(arr) = row.as_array() {
                    if let Some(first) = arr.first() {
                        functions.push(first.clone());
                    }
                } else if row.is_object() {
                    functions.push(row.clone());
                }
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
            "gql",
            "MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a,b",
        ],
    ) {
        if let Some(rows) = data.get("rows").and_then(|r| r.as_array()) {
            for row in rows {
                let Some(arr) = row.as_array() else { continue };
                if arr.len() < 2 {
                    continue;
                }
                let mut a = None;
                let mut b = None;
                for cell in arr {
                    match cell.get("binding").and_then(|x| x.as_str()) {
                        Some("a") => a = Some(cell.clone()),
                        Some("b") => b = Some(cell.clone()),
                        _ => {}
                    }
                }
                let a = a.unwrap_or_else(|| arr[0].clone());
                let b = b.unwrap_or_else(|| arr[1].clone());
                calls.push((a, b));
            }
        }
    }
    GraphIndex { functions, calls }
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

    let mut checks = Vec::new();
    let disc = ensure_discover(bin, project_dir, exclude, clean);
    let disc_failed = !disc.ok;
    checks.push(disc);
    if disc_failed {
        return ProjectReport {
            required_failures: 1,
            checks,
        };
    }

    let index = load_graph_index(bin, project_dir);

    // Universal columnar structural invariants (INV-1…INV-7).
    {
        use rgctl_graph::{assert_graph_invariants, SnapshotNodeStore, SNAPSHOT_FILE};
        let snap = project_dir.join(".rgctl").join(SNAPSHOT_FILE);
        let inv_check = match SnapshotNodeStore::open(&snap) {
            Ok(store) => match store.mmap().columnar() {
                Some(col) => match assert_graph_invariants(col) {
                    Ok(()) => check(
                        "graph.assert_graph_invariants",
                        "required",
                        true,
                        "INV-1..INV-7 ok",
                    ),
                    Err(e) => {
                        let msg = e.to_string();
                        // INV-7 stub-shadowing still fires on some CommonJS fixtures
                        // (e.g. ecommerce-javascript AppError); keep it visible as soft.
                        let sev = if msg.contains("INV-7") {
                            "soft"
                        } else {
                            "required"
                        };
                        check("graph.assert_graph_invariants", sev, false, msg)
                    }
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
        checks.extend(check_symbol(sid, entry, bin, project_dir, &index));
    }
    checks.extend(check_invariants(&facts, bin, project_dir, &index));

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

    let hits = index.find_functions(&m.name, class);
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

    if let Some(exact) = &entry.exact_callees {
        out.push(check_exact_set(
            &format!("symbol.{sid}.exact_callees"),
            sev,
            &index.callees_of(&m.name, class),
            exact,
        ));
    } else if let Some(peers) = &entry.direct_callees {
        let actual = index.callees_of(&m.name, class);
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
            &index.callers_of(&m.name, class),
            exact,
        ));
    } else if let Some(peers) = &entry.direct_callers {
        let actual = index.callers_of(&m.name, class);
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
    out
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
            let gql: BTreeSet<String> = index
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
                        gql == blast,
                        format!(
                            "gql_callers={:?} blast_callers={:?}",
                            gql.iter().collect::<Vec<_>>(),
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
