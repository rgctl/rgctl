//! `rgctl pr-check` / `rgctl review check` — temporal PR policy gate.

use super::args::OutputFormat;
use super::context::CliContext;
use super::policy_file::PolicyFile;
use super::pr_check_output::{PrCheckResponse, build_pr_check_response};
use super::temporal_prep::{
    SyntheticHeadMode, TemporalArtifactArgs, prepare_artifacts, resolve_scope_paths,
    resolve_unified_diff,
};
use crate::analysis::{
    BlastRadiusEngine, PolicyDelta, PolicyRegistry, TemporalClass, ViolationLedger,
    apply_ledger_regression, build_pr_check_centrality, collect_upstream_call_closure,
    evaluate_calendar_for_deltas, evaluate_temporal, hydrate_subset, record_deltas_to_ledger,
    scope_entity_ids, system_today_days, system_unix_secs,
};
use anyhow::{Context, Result};
use rgctl_graph::code_graph::GRAPH_DIR;
use rgctl_graph::snapshot_diff::{SnapshotPair, VecDiffSink, diff_snapshots};
use rgctl_graph::stable_key::StableNodeKey;
use rgctl_incremental::{EntityScope, HunkIndex, git_rev_list_reverse};
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

pub struct PrCheckArgs {
    pub policy_file: String,
    pub base_artifact: Option<String>,
    pub head_artifact: Option<String>,
    pub base_ref: String,
    pub head_ref: String,
    pub strict: bool,
    pub cascade_depth: usize,
    /// Require pre-built head snapshot (`--head-artifact` or `{repo}/.rgctl/`).
    pub full_snapshots: bool,
    pub bisect: bool,
    pub synthetic_head: Option<String>,
    pub strict_calendar: bool,
}

impl PrCheckArgs {
    fn temporal(&self) -> TemporalArtifactArgs {
        TemporalArtifactArgs {
            base_artifact: self.base_artifact.clone(),
            head_artifact: self.head_artifact.clone(),
            base_ref: self.base_ref.clone(),
            head_ref: self.head_ref.clone(),
            cascade_depth: self.cascade_depth,
            full_snapshots: self.full_snapshots,
        }
    }
}

struct PrCheckEval {
    deltas: Vec<PolicyDelta>,
    graph_diff: rgctl_graph::snapshot_diff::DiffStats,
    scoped_files: usize,
    scoped_entities: usize,
}

pub fn run(ctx: &CliContext, args: PrCheckArgs) -> Result<()> {
    let policy = PolicyFile::load(Path::new(&args.policy_file))
        .with_context(|| format!("load policy {}", args.policy_file))?;
    let synthetic_head = SyntheticHeadMode::parse(args.synthetic_head.as_deref())?;
    let new_violations_only = policy.scope.new_violations_only;
    let fail_on_regression = policy.scope.fail_on_regression;
    let max_changed_files = policy.size_limits.max_changed_files;
    let max_scoped_entities = policy.size_limits.max_scoped_entities;
    let temporal = policy.temporal.clone();
    let registry = policy.into_registry();

    let eval = evaluate_pr_check(
        ctx,
        &args,
        &registry,
        max_changed_files,
        max_scoped_entities,
        synthetic_head,
    )?;
    let mut deltas = eval.deltas;

    let rgctl_dir = ctx.repo.join(GRAPH_DIR);
    let mut ledger = ViolationLedger::open(&rgctl_dir)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    apply_ledger_regression(&mut deltas, &ledger);

    let introduced_in_commit = if args.bisect {
        bisect_new_violations(
            ctx,
            &args,
            &registry,
            max_changed_files,
            max_scoped_entities,
            synthetic_head,
            &deltas,
        )?
    } else {
        HashMap::new()
    };

    let calendar = evaluate_calendar_for_deltas(
        &deltas,
        &temporal,
        &ledger,
        system_today_days(),
        system_unix_secs(),
    );

    let response = build_pr_check_response(
        &deltas,
        eval.graph_diff,
        eval.scoped_files,
        eval.scoped_entities,
        new_violations_only,
        fail_on_regression,
        &introduced_in_commit,
        &calendar,
        args.strict_calendar,
    );

    let commit_ref = resolve_head_commit(&ctx.repo, &args.head_ref)?;
    record_deltas_to_ledger(&mut ledger, &deltas, &commit_ref)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(&response)?)?;
    } else if response.passed {
        if response.violations_summary.resolved > 0 {
            println!(
                "PR check passed ({} scoped entities, {} file changes); resolved {} violations (debt paid down)",
                response.scope.entities,
                response.scope.files,
                response.violations_summary.resolved
            );
        } else {
            println!(
                "PR check passed ({} scoped entities, {} file changes)",
                response.scope.entities,
                response.scope.files
            );
        }
    } else {
        println!("PR policy violations: {}", response.violations.len());
        for v in &response.violations {
            if let Some(commit) = &v.introduced_in_commit {
                println!(
                    "  {} [{}]: {} (introduced in {})",
                    v.symbol,
                    v.classification,
                    v.violation,
                    commit
                );
            } else {
                println!("  {} [{}]: {}", v.symbol, v.classification, v.violation);
            }
        }
        if response.violations_summary.resolved > 0 {
            println!(
                "Resolved {} violations (debt paid down)",
                response.violations_summary.resolved
            );
        }
    }

    if !response.passed {
        std::process::exit(1);
    }
    Ok(())
}

/// Evaluate the policy gate **without appending to `.rgctl/violation_ledger.jsonl`**.
///
/// [`run`] opens the on-disk ledger, classifies against it, and then records observations
/// into it (`record_deltas_to_ledger`). The classification is a read; the recording is the mutation, and
/// it is the part a reporting command must not do — an append would change the classification inputs for
/// the very next export, so exporting twice would not be idempotent.
///
/// **The read is that same read.** An earlier version of this function used
/// [`ViolationLedger::in_memory()`], which is more than read-only: it discards the on-disk history
/// entirely. That made the two halves of this module's contract disagree —
/// [`record_deltas_to_ledger`] stops the *state* from changing, while the empty ledger stopped the
/// *classification* from matching. Regressions were not reclassified and calendar `first_seen` ages did
/// not line up, so the exported `policy` section could contradict `review check` on a repository whose
/// ledger already had entries. `in_memory()` is documented as being for tests, and this was not one.
///
/// So: open the real ledger, classify against it, and skip only the append.
pub fn evaluate_read_only(ctx: &CliContext, args: &PrCheckArgs) -> Result<PrCheckResponse> {
    let policy = PolicyFile::load(Path::new(&args.policy_file))
        .with_context(|| format!("load policy {}", args.policy_file))?;
    let synthetic_head = SyntheticHeadMode::parse(args.synthetic_head.as_deref())?;
    let new_violations_only = policy.scope.new_violations_only;
    let fail_on_regression = policy.scope.fail_on_regression;
    let max_changed_files = policy.size_limits.max_changed_files;
    let max_scoped_entities = policy.size_limits.max_scoped_entities;
    let temporal = policy.temporal.clone();
    let registry = policy.into_registry();

    let eval = evaluate_pr_check(
        ctx,
        args,
        &registry,
        max_changed_files,
        max_scoped_entities,
        synthetic_head,
    )?;
    let mut deltas = eval.deltas;

    // The on-disk ledger, opened for reading only. `ViolationLedger::open` loads the file if it exists and
    // creates nothing, so the only mutation in the `run` path — `record_deltas_to_ledger` — is the one
    // thing this function leaves out.
    let ledger = ViolationLedger::open(&ctx.repo.join(GRAPH_DIR)).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    apply_ledger_regression(&mut deltas, &ledger);

    let calendar = evaluate_calendar_for_deltas(
        &deltas,
        &temporal,
        &ledger,
        system_today_days(),
        system_unix_secs(),
    );

    Ok(build_pr_check_response(
        &deltas,
        eval.graph_diff,
        eval.scoped_files,
        eval.scoped_entities,
        new_violations_only,
        fail_on_regression,
        // `--bisect` is intentionally not run here: it walks the commit range once per
        // violating symbol, which is far too costly for an export and would make the
        // artifact's cost scale with violation count rather than repo size.
        &HashMap::new(),
        &calendar,
        args.strict_calendar,
    ))
}

fn evaluate_pr_check(
    ctx: &CliContext,
    args: &PrCheckArgs,
    registry: &PolicyRegistry,
    max_changed_files: Option<usize>,
    max_scoped_entities: Option<usize>,
    synthetic_head: SyntheticHeadMode,
) -> Result<PrCheckEval> {
    let temporal = args.temporal();
    let artifacts = prepare_artifacts(ctx, &temporal, synthetic_head)?;
    let pair = SnapshotPair::open(&artifacts.base_snapshot, &artifacts.head_snapshot)
        .with_context(|| "open snapshot pair")?;

    let mut sink = VecDiffSink::default();
    let graph_diff = diff_snapshots(&pair.base, &pair.head, &mut sink)?;

    let paths = resolve_scope_paths(ctx, &temporal, synthetic_head)?;
    if args.strict && paths.is_empty() {
        anyhow::bail!("strict diff scope: no changed files between refs");
    }
    if let Some(max) = max_changed_files {
        if paths.len() > max {
            anyhow::bail!(
                "changed file count {} exceeds policy size_limits.max_changed_files {}",
                paths.len(),
                max
            );
        }
    }

    let unified = resolve_unified_diff(ctx, &temporal, synthetic_head)?;
    let hunk_index = HunkIndex::from_unified_diff(&unified);
    let scope = EntityScope::new(&pair.head, &paths, Some(&hunk_index));
    let scope_keys = scope.changed_entities()?;
    if let Some(max) = max_scoped_entities {
        if scope_keys.len() > max {
            anyhow::bail!(
                "scoped entity count {} exceeds policy size_limits.max_scoped_entities {}",
                scope_keys.len(),
                max
            );
        }
    }

    let head_seed_ids = scope_entity_ids(&pair.head, &scope_keys)?;
    let base_seed_ids = scope_entity_ids(&pair.base, &scope_keys)?;

    let head_engine = BlastRadiusEngine::build_scoped(&pair.head, &head_seed_ids)?;
    let base_engine = BlastRadiusEngine::build_scoped(&pair.base, &base_seed_ids)?;

    let head_seeds: HashSet<_> = head_seed_ids.iter().copied().collect();
    let base_seeds: HashSet<_> = base_seed_ids.iter().copied().collect();
    let head_ids = collect_upstream_call_closure(&pair.head, &head_seeds)?;
    let base_ids = collect_upstream_call_closure(&pair.base, &base_seeds)?;
    let head_backend = hydrate_subset(&pair.head, &head_ids)?;
    let base_backend = hydrate_subset(&pair.base, &base_ids)?;

    let centrality = build_pr_check_centrality(&ctx.repo, &pair.head, &head_seed_ids)?;

    let deltas = evaluate_temporal(
        &pair.base,
        &pair.head,
        &base_backend,
        &head_backend,
        &head_engine,
        &base_engine,
        &scope_keys,
        &registry,
        &centrality,
    )?;

    Ok(PrCheckEval {
        deltas,
        graph_diff,
        scoped_files: paths.len(),
        scoped_entities: scope_keys.len(),
    })
}

fn bisect_new_violations(
    ctx: &CliContext,
    args: &PrCheckArgs,
    registry: &PolicyRegistry,
    max_changed_files: Option<usize>,
    max_scoped_entities: Option<usize>,
    synthetic_head: SyntheticHeadMode,
    deltas: &[PolicyDelta],
) -> Result<HashMap<u64, String>> {
    if synthetic_head == SyntheticHeadMode::Worktree {
        return Ok(HashMap::new());
    }

    let commits = git_rev_list_reverse(&ctx.repo, &args.base_ref, &args.head_ref)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if commits.is_empty() {
        return Ok(HashMap::new());
    }

    let mut introduced = HashMap::new();
    for delta in deltas {
        if delta.classification != TemporalClass::New
            && delta.classification != TemporalClass::Regression
        {
            continue;
        }
        if let Some(commit) = bisect_first_new_commit(
            ctx,
            args,
            registry,
            max_changed_files,
            max_scoped_entities,
            &commits,
            delta.key,
        )? {
            introduced.insert(delta.key.as_u64(), commit);
        }
    }
    Ok(introduced)
}

fn bisect_first_new_commit(
    ctx: &CliContext,
    args: &PrCheckArgs,
    registry: &PolicyRegistry,
    max_changed_files: Option<usize>,
    max_scoped_entities: Option<usize>,
    commits: &[String],
    target_key: StableNodeKey,
) -> Result<Option<String>> {
    let mut lo = 0usize;
    let mut hi = commits.len() - 1;
    let mut first: Option<String> = None;

    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let commit = &commits[mid];
        let bisect_args = PrCheckArgs {
            head_ref: commit.clone(),
            head_artifact: None,
            full_snapshots: false,
            bisect: false,
            synthetic_head: None,
            strict_calendar: false,
            ..args.clone_fields()
        };
        let eval = evaluate_pr_check(
            ctx,
            &bisect_args,
            registry,
            max_changed_files,
            max_scoped_entities,
            SyntheticHeadMode::None,
        )?;
        let is_new = eval
            .deltas
            .iter()
            .any(|d| d.key == target_key && d.classification == TemporalClass::New);
        if is_new {
            first = Some(commit.clone());
            if mid == 0 {
                break;
            }
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
    }
    Ok(first)
}

impl PrCheckArgs {
    fn clone_fields(&self) -> Self {
        Self {
            policy_file: self.policy_file.clone(),
            base_artifact: self.base_artifact.clone(),
            head_artifact: self.head_artifact.clone(),
            base_ref: self.base_ref.clone(),
            head_ref: self.head_ref.clone(),
            strict: self.strict,
            cascade_depth: self.cascade_depth,
            full_snapshots: self.full_snapshots,
            bisect: false,
            synthetic_head: None,
            strict_calendar: self.strict_calendar,
        }
    }
}

fn resolve_head_commit(repo: &Path, head_ref: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", head_ref])
        .current_dir(repo)
        .output()
        .context("run git rev-parse")?;
    if !output.status.success() {
        anyhow::bail!(
            "git rev-parse {} failed: {}",
            head_ref,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
