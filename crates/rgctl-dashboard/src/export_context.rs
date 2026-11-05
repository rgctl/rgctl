//! In-memory inputs for dashboard export (avoids reloading analysis / kantra artifacts).

use rgctl_analysis::AnalysisResults;
use rgctl_kantra::KantraFindings;

/// Optional in-memory inputs passed from discover (falls back to disk load when absent).
#[derive(Debug, Clone, Copy, Default)]
pub struct DashboardExportContext<'a> {
    pub analysis: Option<&'a AnalysisResults>,
    pub kantra_findings: Option<&'a KantraFindings>,
}

impl<'a> DashboardExportContext<'a> {
    pub fn new(analysis: Option<&'a AnalysisResults>) -> Self {
        Self {
            analysis,
            kantra_findings: None,
        }
    }

    pub fn with_analysis(analysis: &'a AnalysisResults) -> Self {
        Self {
            analysis: Some(analysis),
            kantra_findings: None,
        }
    }

    pub fn with_kantra_findings(mut self, findings: &'a KantraFindings) -> Self {
        self.kantra_findings = Some(findings);
        self
    }
}

pub(crate) fn resolve_analysis<'a>(
    ctx: &DashboardExportContext<'a>,
    repo_root: &std::path::Path,
) -> Result<std::borrow::Cow<'a, AnalysisResults>, String> {
    if let Some(results) = ctx.analysis {
        return Ok(std::borrow::Cow::Borrowed(results));
    }
    let path = repo_root.join(".rgctl/analysis_results.bin");
    if !path.is_file() {
        return Err("analysis_results.bin missing".into());
    }
    AnalysisResults::load(&path)
        .map(std::borrow::Cow::Owned)
        .map_err(|e| e.to_string())
}
