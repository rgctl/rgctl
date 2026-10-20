//! `rgctl review export` — compose the review workflow into one versioned artifact.
//!
//! Design: `openspec/changes/add-review-workflow-export/`.
//!
//! This module deliberately **composes** reports that already exist rather than recomputing
//! anything, so the artifact can never disagree with `review paths` / `review check`.
//!
//! Two invariants matter more than anything else here:
//!
//! 1. **Read-only.** Exporting a review must not mutate the state it reports on, so the
//!    policy section is evaluated against an in-memory ledger (see
//!    [`super::pr_check::evaluate_read_only`]) and `.rgctl/violation_ledger.jsonl` is opened
//!    for reading only.
//! 2. **Absent is not empty.** A section that could not be produced is `null` with a reason
//!    in `provenance.unavailable`, never an empty object that reads as a clean result.

use super::args::OutputFormat;
use super::context::CliContext;
use super::pr_check::{self, PrCheckArgs};
use super::pr_check_output::PrCheckResponse;
use super::review::{ReviewPathsArgs, build_paths_bundle};
use crate::analysis::{VIOLATION_LEDGER_FILE, ViolationLedgerEntry};
use anyhow::{Context, Result};
use rgctl_graph::code_graph::GRAPH_DIR;
use serde::Serialize;
use std::io::BufRead;
use std::path::{Path, PathBuf};

/// Envelope schema version (independent of the composed sections' own versions).
pub const REVIEW_EXPORT_SCHEMA_VERSION: &str = "1";

/// Directory under `.rgctl/` holding composed workflow artifacts.
pub const REVIEW_WORKFLOW_DIR: &str = "workflows";

/// Default artifact name.
pub const REVIEW_WORKFLOW_FILE: &str = "review.json";

pub struct ReviewExportArgs {
    pub paths: ReviewPathsArgs,
    /// When `None`, the policy section is `null` with a recorded reason rather than
    /// silently assumed clean.
    pub policy_file: Option<String>,
    /// Write the envelope to stdout instead of the default artifact path.
    pub stdout: bool,
}

#[derive(Debug, Serialize)]
pub struct TruncationAggregate {
    pub symbols: bool,
    pub fanout: bool,
    pub depth: bool,
}

impl TruncationAggregate {
    fn from_sections(
        report: &crate::analysis::ReviewPathsReport,
    ) -> Self {
        let mut agg = Self {
            symbols: report.truncation.symbols,
            fanout: report.truncation.fanout,
            depth: report.truncation.depth,
        };
        for symbol in &report.symbols {
            agg.symbols |= symbol.truncation.symbols;
            agg.fanout |= symbol.truncation.fanout;
            agg.depth |= symbol.truncation.depth;
        }
        agg
    }
}

#[derive(Debug, Serialize)]
pub struct ProvenanceGraph {
    pub base_snapshot: String,
    pub head_snapshot: String,
    pub base_digest: String,
    pub head_digest: String,
}

/// Why a section could not be produced. Omitted entirely when everything was produced.
#[derive(Debug, Default, Serialize)]
pub struct ProvenanceUnavailable {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Provenance {
    /// Concrete, copy-pasteable command lines behind the populated sections.
    pub commands: Vec<String>,
    pub graph: ProvenanceGraph,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<ProvenanceUnavailable>,
}

#[derive(Debug, Serialize)]
pub struct ReviewExportEnvelope {
    pub schema_version: &'static str,
    pub command: &'static str,
    /// Unix seconds. The only field permitted to differ between two exports over the
    /// same inputs.
    pub generated_at: String,
    pub review_paths: crate::analysis::ReviewPathsReport,
    /// `null` means **not evaluated**, never "no violations". An evaluated run that found
    /// nothing is an object with `passed` and an empty `violations` array.
    pub policy: Option<PrCheckResponse>,
    pub timeline: Vec<ViolationLedgerEntry>,
    pub truncation: TruncationAggregate,
    pub provenance: Provenance,
}

/// Default artifact path: `{repo}/.rgctl/workflows/review.json`.
pub fn default_artifact_path(repo: &Path) -> PathBuf {
    repo.join(GRAPH_DIR)
        .join(REVIEW_WORKFLOW_DIR)
        .join(REVIEW_WORKFLOW_FILE)
}

/// Read the append-only violation ledger. Read-only by construction.
///
/// Returns `Ok(None)` when the file does not exist, so the caller can distinguish
/// "no ledger yet" from "ledger exists but is empty".
fn read_timeline(repo: &Path) -> Result<Option<Vec<ViolationLedgerEntry>>> {
    let path = repo.join(GRAPH_DIR).join(VIOLATION_LEDGER_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let file = std::fs::File::open(&path)
        .with_context(|| format!("open ledger {}", path.display()))?;
    let mut entries = Vec::new();
    for (lineno, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("read ledger line {}", lineno + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ViolationLedgerEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                // A malformed line is not worth failing an export over, but it must not be
                // silently dropped either. Skip it loudly.
                eprintln!("[!] skipping unreadable ledger line {}: {e}", lineno + 1);
            }
        }
    }
    // Deterministic order: the ledger is append-only, but a caller may have rewritten it.
    entries.sort_by(|a, b| {
        a.stable_key
            .cmp(&b.stable_key)
            .then_with(|| a.rule.cmp(&b.rule))
            .then_with(|| a.ts.cmp(&b.ts))
    });
    Ok(Some(entries))
}

fn now_unix_secs() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

pub fn run_export(ctx: &CliContext, args: ReviewExportArgs) -> Result<()> {
    let ReviewExportArgs {
        paths,
        policy_file,
        stdout,
    } = args;

    // `review_paths` is the section that defines the change under review, so a failure
    // here is fatal. Everything else is optional.
    let base_ref = paths.base_ref.clone();
    let head_ref = paths.head_ref.clone();
    let bundle = build_paths_bundle(ctx, paths)?;

    let mut commands = vec![format!(
        "rgctl review paths --base-ref {base_ref} --head-ref {head_ref} -f json"
    )];

    // Policy: evaluated read-only, or explicitly unavailable.
    let (policy, policy_unavailable) = match policy_file {
        Some(path) => {
            // Reuse the already-prepared base/head snapshots from the paths bundle so
            // policy evaluation does not re-synthesize (and does not drop --base-artifact).
            let check_args = PrCheckArgs {
                policy_file: path.clone(),
                base_artifact: Some(bundle.base_snapshot.display().to_string()),
                head_artifact: Some(bundle.head_snapshot.display().to_string()),
                base_ref: base_ref.clone(),
                head_ref: head_ref.clone(),
                strict: false,
                cascade_depth: 1,
                full_snapshots: true,
                bisect: false,
                synthetic_head: None,
                strict_calendar: false,
            };
            commands.push(format!(
                "rgctl review check --policy-file {path} --base-ref {base_ref} --head-ref {head_ref} --full-snapshots -f json"
            ));
            match pr_check::evaluate_read_only(ctx, &check_args) {
                Ok(response) => (Some(response), None),
                Err(e) => (None, Some(format!("policy check failed: {e}"))),
            }
        }
        None => (
            None,
            Some("no --policy-file provided; policy was not evaluated".to_string()),
        ),
    };

    let (timeline, timeline_unavailable) = match read_timeline(&ctx.repo)? {
        Some(entries) => (entries, None),
        None => (
            Vec::new(),
            Some(format!("no {VIOLATION_LEDGER_FILE} under .rgctl/")),
        ),
    };

    let unavailable = ProvenanceUnavailable {
        policy: policy_unavailable,
        timeline: timeline_unavailable,
    };
    let unavailable = if unavailable.policy.is_none() && unavailable.timeline.is_none() {
        None
    } else {
        Some(unavailable)
    };

    let envelope = ReviewExportEnvelope {
        schema_version: REVIEW_EXPORT_SCHEMA_VERSION,
        command: "review export",
        generated_at: now_unix_secs(),
        truncation: TruncationAggregate::from_sections(&bundle.report),
        review_paths: bundle.report,
        policy,
        timeline,
        provenance: Provenance {
            commands,
            graph: ProvenanceGraph {
                base_snapshot: bundle.base_snapshot.display().to_string(),
                head_snapshot: bundle.head_snapshot.display().to_string(),
                base_digest: bundle.base_digest,
                head_digest: bundle.head_digest,
            },
            unavailable,
        },
    };

    let json = serde_json::to_string_pretty(&envelope)?;

    // Precedence: an explicit `-o PATH` wins, then `--stdout`, then the default
    // artifact path. (`ctx.emit` also honours `-o`, so keeping that order avoids the
    // two disagreeing when both flags are supplied.)
    let target = if let Some(out) = &ctx.output {
        Some(out.clone())
    } else if stdout {
        None
    } else {
        Some(default_artifact_path(&ctx.repo))
    };

    match target {
        None => {
            ctx.emit(&json)?;
        }
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            std::fs::write(&path, format!("{json}\n"))
                .with_context(|| format!("write {}", path.display()))?;
            // Confirmation goes to stderr so stdout stays machine-readable.
            if ctx.format != OutputFormat::Json {
                eprintln!("wrote {}", path.display());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{ChangeSummary, ReviewPathsReport, SymbolPathReport, TruncationFlags};
    use std::fs;
    use tempfile::TempDir;

    fn report_with(symbol_truncation: TruncationFlags, top: TruncationFlags) -> ReviewPathsReport {
        ReviewPathsReport {
            schema_version: 1,
            command: "review paths".to_string(),
            change_summary: ChangeSummary::default(),
            truncation: top,
            symbols: vec![SymbolPathReport {
                stable_key: "abc".to_string(),
                name: "f".to_string(),
                kind: "function".to_string(),
                base: None,
                head: None,
                path_before: Vec::new(),
                path_after: Vec::new(),
                path_delta: Vec::new(),
                truncation: symbol_truncation,
            }],
            unscored_files: Vec::new(),
            ambiguous: Vec::new(),
        }
    }

    #[test]
    fn default_artifact_path_is_under_rgctl_workflows() {
        let path = default_artifact_path(Path::new("/repo"));
        assert!(
            path.ends_with(".rgctl/workflows/review.json"),
            "unexpected default path: {}",
            path.display()
        );
    }

    #[test]
    fn truncation_is_aggregated_from_top_level_and_symbols() {
        // Per-symbol truncation must surface at the envelope level: a consumer reading
        // only the aggregate must not conclude the report was complete.
        let report = report_with(
            TruncationFlags {
                symbols: false,
                fanout: true,
                depth: false,
            },
            TruncationFlags::default(),
        );
        let agg = TruncationAggregate::from_sections(&report);
        assert!(agg.fanout, "per-symbol fanout truncation must aggregate");
        assert!(!agg.symbols);
        assert!(!agg.depth);
    }

    #[test]
    fn missing_ledger_is_reported_as_absent_not_empty() {
        let temp = TempDir::new().unwrap();
        assert!(
            read_timeline(temp.path()).unwrap().is_none(),
            "a missing ledger must be distinguishable from an empty one"
        );
    }

    #[test]
    fn timeline_parses_sorts_and_skips_malformed_lines() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join(GRAPH_DIR);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(VIOLATION_LEDGER_FILE),
            concat!(
                r#"{"stable_key":2,"rule":"b","class":"new","commit":"c2","ts":"2"}"#,
                "\n",
                "this line is not json\n",
                "\n",
                r#"{"stable_key":1,"rule":"a","class":"existing","commit":"c1","ts":"1"}"#,
                "\n",
            ),
        )
        .unwrap();

        let entries = read_timeline(temp.path()).unwrap().expect("ledger present");
        assert_eq!(entries.len(), 2, "malformed and blank lines are skipped");
        assert_eq!(entries[0].stable_key, 1, "entries are deterministically sorted");
        assert_eq!(entries[1].stable_key, 2);
    }

    #[test]
    fn unavailable_is_omitted_when_every_section_was_produced() {
        let value = serde_json::to_value(Provenance {
            commands: vec!["rgctl review paths".to_string()],
            graph: ProvenanceGraph {
                base_snapshot: "b".to_string(),
                head_snapshot: "h".to_string(),
                base_digest: "d1".to_string(),
                head_digest: "d2".to_string(),
            },
            unavailable: None,
        })
        .unwrap();
        assert!(
            value.get("unavailable").is_none(),
            "`unavailable` must be omitted entirely when nothing was unavailable"
        );
    }
}
