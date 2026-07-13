//! Taint flow export for dashboard (reads `.rbuilder/analysis/` index + selective loads).

use crate::export_util::write_json_compact;
use rbuilder_analysis::pdg::{PdgNodeId, ProgramDependenceGraph};
use rbuilder_analysis::storage::AnalysisStorage;
use rbuilder_analysis::taint::{Sanitizer, TaintFlow, TaintSink, TaintSource};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const TAINT_INDEX_FILE: &str = "taint_index.json";
pub const TAINT_DETAIL_DIR: &str = "taint";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintIndexPayload {
    pub schema_version: u32,
    pub available: bool,
    pub detail_dir: String,
    pub function_count: usize,
    pub total_flows: usize,
    pub vulnerable_flows: usize,
    pub functions: Vec<TaintFunctionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintFunctionEntry {
    pub function_id: String,
    pub name: String,
    pub file_path: Option<String>,
    pub flow_count: usize,
    pub vulnerable_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintBundlePayload {
    pub schema_version: u32,
    pub function_id: String,
    pub name: String,
    pub file_path: Option<String>,
    pub flows: Vec<TaintFlowView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintFlowView {
    pub id: usize,
    pub variable: String,
    pub source_type: String,
    pub sink_type: String,
    pub severity: u8,
    pub vulnerable: bool,
    pub sanitizers: Vec<String>,
    pub source_line: usize,
    pub sink_line: usize,
    pub source_text: String,
    pub sink_text: String,
    pub path_lines: Vec<usize>,
    pub path_statements: Vec<String>,
}

#[derive(Debug, Default)]
pub struct TaintExportSummary {
    pub available: bool,
    pub function_count: usize,
    pub total_flows: usize,
    pub vulnerable_flows: usize,
}

pub fn export_taint_bundle(repo_root: &Path, out_dir: &Path) -> Result<TaintExportSummary, String> {
    let analysis_dir = repo_root.join(".rbuilder/analysis");
    let storage = AnalysisStorage::new(&analysis_dir);
    let index = storage.load_analysis_index().map_err(|e| e.to_string())?;

    let taint_dir = out_dir.join(TAINT_DETAIL_DIR);
    if taint_dir.exists() {
        fs::remove_dir_all(&taint_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&taint_dir).map_err(|e| e.to_string())?;

    let mut functions = Vec::new();
    let mut total_flows = 0usize;
    let mut vulnerable_flows = 0usize;

    let mut entries: Vec<_> = index.values().collect();
    entries.sort_by(|a, b| a.stable_key.cmp(&b.stable_key));

    for entry in entries {
        if entry.flow_count == 0 && entry.vulnerable_count == 0 {
            continue;
        }
        let Some(analysis) = storage
            .load_function(entry.function_id)
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let Some(flows) = analysis.taint.as_ref().filter(|f| !f.is_empty()) else {
            continue;
        };
        let pdg = analysis.pdg.as_ref();
        let flow_views: Vec<TaintFlowView> = flows
            .iter()
            .enumerate()
            .map(|(id, flow)| export_flow(id, flow, pdg))
            .collect();
        let vuln = flow_views.iter().filter(|f| f.vulnerable).count();
        total_flows += flow_views.len();
        vulnerable_flows += vuln;

        let bundle = TaintBundlePayload {
            schema_version: 1,
            function_id: analysis.function_id.to_string(),
            name: analysis.function_name.clone(),
            file_path: Some(analysis.file_path.clone()),
            flows: flow_views,
        };
        write_json_compact(
            &taint_dir.join(format!("{}.json", analysis.function_id)),
            &bundle,
        )?;

        functions.push(TaintFunctionEntry {
            function_id: analysis.function_id.to_string(),
            name: analysis.function_name,
            file_path: Some(analysis.file_path),
            flow_count: bundle.flows.len(),
            vulnerable_count: vuln,
        });
    }

    functions.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.function_id.cmp(&b.function_id))
    });

    let available = !functions.is_empty();
    let index_payload = TaintIndexPayload {
        schema_version: 1,
        available,
        detail_dir: TAINT_DETAIL_DIR.into(),
        function_count: functions.len(),
        total_flows,
        vulnerable_flows,
        functions,
    };
    write_json_compact(&out_dir.join(TAINT_INDEX_FILE), &index_payload)?;

    Ok(TaintExportSummary {
        available,
        function_count: index_payload.function_count,
        total_flows,
        vulnerable_flows,
    })
}

fn export_flow(id: usize, flow: &TaintFlow, pdg: Option<&ProgramDependenceGraph>) -> TaintFlowView {
    let (source_line, source_text) = node_line_text(pdg, &flow.source);
    let (sink_line, sink_text) = node_line_text(pdg, &flow.sink);
    let (path_lines, path_statements) = path_details(pdg, &flow.path);

    TaintFlowView {
        id,
        variable: flow.variable.clone(),
        source_type: format_taint_source(flow.source_type),
        sink_type: format_taint_sink(flow.sink_type),
        severity: flow.severity,
        vulnerable: flow.is_vulnerable(),
        sanitizers: flow.sanitizers.iter().map(format_sanitizer).collect(),
        source_line,
        sink_line,
        source_text,
        sink_text,
        path_lines,
        path_statements,
    }
}

fn node_line_text(pdg: Option<&ProgramDependenceGraph>, id: &PdgNodeId) -> (usize, String) {
    pdg.and_then(|g| g.nodes.get(id))
        .map(|n| (n.statement.line, n.statement.text.clone()))
        .unwrap_or((0, String::new()))
}

fn path_details(
    pdg: Option<&ProgramDependenceGraph>,
    path: &[PdgNodeId],
) -> (Vec<usize>, Vec<String>) {
    let Some(pdg) = pdg else {
        return (vec![], vec![]);
    };
    let mut lines = Vec::with_capacity(path.len());
    let mut statements = Vec::with_capacity(path.len());
    for id in path {
        if let Some(node) = pdg.nodes.get(id) {
            lines.push(node.statement.line);
            statements.push(node.statement.text.clone());
        }
    }
    (lines, statements)
}

fn format_taint_source(source: TaintSource) -> String {
    match source {
        TaintSource::HttpParameter => "HttpParameter",
        TaintSource::FileInput => "FileInput",
        TaintSource::NetworkInput => "NetworkInput",
        TaintSource::CommandLineArg => "CommandLineArg",
        TaintSource::EnvironmentVar => "EnvironmentVar",
        TaintSource::DatabaseResult => "DatabaseResult",
    }
    .into()
}

fn format_taint_sink(sink: TaintSink) -> String {
    match sink {
        TaintSink::SqlQuery => "SqlQuery",
        TaintSink::ShellCommand => "ShellCommand",
        TaintSink::FileWrite => "FileWrite",
        TaintSink::NetworkOutput => "NetworkOutput",
        TaintSink::LogOutput => "LogOutput",
        TaintSink::HtmlRender => "HtmlRender",
        TaintSink::CodeEval => "CodeEval",
    }
    .into()
}

fn format_sanitizer(s: &Sanitizer) -> String {
    match s {
        Sanitizer::SqlParameterize => "SqlParameterize".into(),
        Sanitizer::HtmlEscape => "HtmlEscape".into(),
        Sanitizer::ShellEscape => "ShellEscape".into(),
        Sanitizer::Validation(v) => format!("Validation({v})"),
        Sanitizer::TypeCast(v) => format!("TypeCast({v})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_analysis::storage::FunctionAnalysis;
    use rbuilder_analysis::taint::{TaintFlow, TaintSink, TaintSource};
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn empty_analysis_exports_unavailable_index() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let out = tmp.path().join("dashboard");
        fs::create_dir_all(&out).unwrap();

        let summary = export_taint_bundle(&repo, &out).unwrap();
        assert!(!summary.available);

        let index: TaintIndexPayload =
            serde_json::from_slice(&fs::read(out.join(TAINT_INDEX_FILE)).unwrap()).unwrap();
        assert_eq!(index.schema_version, 1);
        assert!(!index.available);
        assert_eq!(index.function_count, 0);
    }

    #[test]
    fn exports_taint_from_analysis_index() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let analysis_dir = repo.join(".rbuilder/analysis");
        fs::create_dir_all(&analysis_dir).unwrap();
        let out = tmp.path().join("dashboard");
        fs::create_dir_all(&out).unwrap();

        let fid = Uuid::new_v4();
        let source = Uuid::new_v4();
        let sink = Uuid::new_v4();
        let flow = TaintFlow {
            source,
            source_type: TaintSource::HttpParameter,
            sink,
            sink_type: TaintSink::SqlQuery,
            variable: "user".into(),
            path: vec![source, sink],
            sanitizers: vec![],
            severity: 10,
        };
        let analysis = FunctionAnalysis {
            function_id: fid,
            function_name: "handle".into(),
            file_path: "App.java".into(),
            code_hash: Some("abc".into()),
            cfg: None,
            pdg: None,
            dominance: None,
            taint: Some(vec![flow]),
        };
        AnalysisStorage::new(&analysis_dir)
            .save_function(&analysis)
            .unwrap();

        let summary = export_taint_bundle(&repo, &out).unwrap();
        assert!(summary.available);
        assert_eq!(summary.function_count, 1);
        assert_eq!(summary.total_flows, 1);
        assert_eq!(summary.vulnerable_flows, 1);

        let bundle: TaintBundlePayload = serde_json::from_slice(
            &fs::read(out.join(TAINT_DETAIL_DIR).join(format!("{fid}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(bundle.flows.len(), 1);
        assert!(bundle.flows[0].vulnerable);
        assert_eq!(bundle.flows[0].source_type, "HttpParameter");
    }
}
