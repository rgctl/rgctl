//! Sink-first taint helpers for `rgctl taint` / `vuln analyze`.
//!
//! Uses declarative [`rgctl_analysis::TaintRuleSet`] overlays — no new detect_* arms.

use rgctl_analysis::TaintRuleSet;
use serde::{Deserialize, Serialize};

/// How the sink symbol was resolved against the indexed graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SinkResolution {
    /// Sink symbol present in the graph.
    Resolved,
    /// Sink only outside indexed source (e.g. JAR bytecode).
    Unresolved,
}

/// Discovered call site to an OSV affected method with file + line coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SinkCallSite {
    /// Qualified name of the caller in the indexed repository.
    pub caller: String,
    /// Target OSV affected method matched.
    pub target_method: String,
    /// Repo-relative file path of the call site.
    pub file: String,
    /// 1-based line number of the call site.
    pub line: u32,
}

/// Sink-first analysis result (schema for CLI JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SinkFirstTaintResult {
    /// Schema version.
    pub schema_version: String,
    /// Requested sink symbol / method.
    pub sink: String,
    /// Source mode (`external`).
    pub source: String,
    /// Depth limit applied.
    pub depth: usize,
    /// Graph resolution honesty.
    pub sink_resolution: SinkResolution,
    /// Honesty note when unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub honesty: Option<String>,
    /// Call sites / callers of the sink (or affected methods) found in-graph.
    pub sink_callers: Vec<String>,
    /// Discovered call-site coordinates (`caller`, `target_method`, `file`, `line`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub call_sites: Vec<SinkCallSite>,
    /// Exploitable path summaries (empty when none).
    pub paths: Vec<SinkPathSummary>,
    /// True when CFG/PDG archive was available.
    pub cfg_available: bool,
}

/// One sink-first path summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SinkPathSummary {
    /// Source description.
    pub from: String,
    /// Sink description.
    pub to: String,
    /// Hop count if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hops: Option<usize>,
}

/// Build a result when CFG artifacts are missing.
pub fn missing_cfg_error_message() -> String {
    "CFG/PDG archive not found — run `rgctl discover --with-cfg` (and `--with-taint` if \
     discover-time flows are needed) before sink-first taint"
        .into()
}

/// Summarize sink-first reachability from caller names + resolution.
pub fn build_sink_first_result(
    sink: &str,
    depth: usize,
    cfg_available: bool,
    sink_in_graph: bool,
    sink_callers: Vec<String>,
    path_summaries: Vec<SinkPathSummary>,
) -> SinkFirstTaintResult {
    build_sink_first_result_with_sites(
        sink,
        depth,
        cfg_available,
        sink_in_graph,
        sink_callers,
        vec![],
        path_summaries,
    )
}

/// Summarize sink-first reachability including discovered `file:line` call-site coordinates.
pub fn build_sink_first_result_with_sites(
    sink: &str,
    depth: usize,
    cfg_available: bool,
    sink_in_graph: bool,
    sink_callers: Vec<String>,
    call_sites: Vec<SinkCallSite>,
    path_summaries: Vec<SinkPathSummary>,
) -> SinkFirstTaintResult {
    let resolved = sink_in_graph || !call_sites.is_empty();
    let sink_resolution = if resolved {
        SinkResolution::Resolved
    } else {
        SinkResolution::Unresolved
    };
    let honesty = if !resolved {
        Some(
            "sink symbol not present in indexed source graph (may live only in bundled \
             bytecode/vendor binaries); empty paths MUST NOT alone prove not_exploitable"
                .into(),
        )
    } else {
        None
    };
    SinkFirstTaintResult {
        schema_version: "1".into(),
        sink: sink.into(),
        source: "external".into(),
        depth,
        sink_resolution,
        honesty,
        sink_callers,
        call_sites,
        paths: path_summaries,
        cfg_available,
    }
}

/// Compile rule set with OSV affected-method overlays for the active languages.
pub fn rules_with_osv_overlays(
    languages: Option<&[String]>,
    affected_methods: &[String],
) -> Result<TaintRuleSet, String> {
    let mut set = TaintRuleSet::bundled().map_err(|e| e.to_string())?;
    if let Some(langs) = languages {
        set = set.scoped(langs);
    }
    // Overlay short method names and FQNs.
    let mut needles = Vec::new();
    for m in affected_methods {
        needles.push(m.clone());
        if let Some(short) = m.rsplit('.').next()
            && short != m.as_str()
        {
            needles.push(short.to_string());
        }
    }
    Ok(set.with_overlays(&needles))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_honesty_when_not_in_graph() {
        let r = build_sink_first_result("ObjectMapper.readValue", 8, true, false, vec![], vec![]);
        assert_eq!(r.sink_resolution, SinkResolution::Unresolved);
        assert!(r.honesty.is_some());
        assert!(r.paths.is_empty());
    }

    #[test]
    fn overlays_include_short_name() {
        let set = rules_with_osv_overlays(
            Some(&["java".into()]),
            &["com.fasterxml.jackson.databind.ObjectMapper.readValue".into()],
        )
        .unwrap();
        let rules = set.for_language("java").unwrap();
        let (_, sink, _) = rules.classify("mapper.readValue(json)");
        assert!(sink.is_some());
    }
}
