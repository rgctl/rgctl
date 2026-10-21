//! Staged full pipeline: basic discover → CFG/dashboard/harmonic → semantic index.

use super::args::OutputFormat;
use super::context::CliContext;
use super::discover::DiscoverArgs;
use super::discover_impl::{AnalysisOptions, run_full_analysis};
use super::discover_output::DiscoverJsonResponse;
use super::pipeline_status::{
    self, HARMONIC_DIGEST_FILE, MATERIALIZED_FIELDS_DIGEST_FILE, STAGE_BASIC, STAGE_DEEP,
    STAGE_SEMANTIC, StageStatus, try_acquire_lock, write_status,
};
use super::semantic::{CliEmbedderKind, CliSemanticScope, SemanticIndexArgs, run_index_with_emit};
use crate::analysis::{DEFAULT_EMBEDDING_DIMENSIONS, SemanticIndex};
use crate::discovery::{DiscoveryConfig, FileDiscoverer};
use crate::incremental::FileTracker;
use crate::languages::registry::LanguageRegistry;
use anyhow::{Context, Result};
use rgctl_graph::snapshot::MmappedGraphSnapshot;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

/// Extra discover flags forwarded into staged passes.
pub struct FullPipelineArgs {
    pub languages: Option<String>,
    pub exclude: Option<String>,
    pub with_security: bool,
    pub with_taint: bool,
    pub taint_rules: Option<String>,
    pub with_dfg_loops: bool,
    pub with_ast_skeleton: bool,
    pub write_json_graph: bool,
    pub export_migration_hints: bool,
    pub migration_preset: String,
    pub migration_order: String,
    pub artifact_root: Option<PathBuf>,
    pub limits: Option<super::discover_limits::DiscoverLimits>,
}

impl FullPipelineArgs {
    pub fn from_discover(
        args: &DiscoverArgs,
        limits: Option<super::discover_limits::DiscoverLimits>,
    ) -> Self {
        Self {
            languages: args.languages.clone(),
            exclude: args.exclude.clone(),
            with_security: args.with_security,
            with_taint: args.with_taint,
            taint_rules: args.taint_rules.clone(),
            with_dfg_loops: args.with_dfg_loops,
            with_ast_skeleton: args.with_ast_skeleton,
            write_json_graph: args.write_json_graph,
            export_migration_hints: args.export_migration_hints,
            migration_preset: args.migration_preset.clone(),
            migration_order: args.migration_order.clone(),
            artifact_root: args.artifact_root.clone(),
            limits,
        }
    }

    pub fn default_serve() -> Self {
        Self {
            languages: None,
            exclude: None,
            with_security: false,
            with_taint: false,
            taint_rules: None,
            with_dfg_loops: false,
            with_ast_skeleton: false,
            write_json_graph: false,
            export_migration_hints: false,
            migration_preset: "hybrid_default".into(),
            migration_order: "scheduled".into(),
            artifact_root: None,
            limits: None,
        }
    }
}

/// Print the three-stage plan (stderr via tracing).
pub fn print_plan() {
    info!("Full pipeline plan:");
    info!("  1. {STAGE_BASIC}  — index + analysis (queryable snapshot)");
    info!("  2. {STAGE_DEEP}      — --with-cfg --with-dashboard --with-harmonic");
    info!("  3. {STAGE_SEMANTIC} — semantic index (vocab embedder)");
}

/// Artifact root: daemon cache dir or the source tree when unset.
fn artifact_store<'a>(source: &'a Path, extras: &'a FullPipelineArgs) -> &'a Path {
    extras.artifact_root.as_deref().unwrap_or(source)
}

/// Run the staged full pipeline on `root`, holding the repo lock.
pub fn run_full_pipeline(
    ctx: &CliContext,
    root: &str,
    extras: FullPipelineArgs,
) -> Result<DiscoverJsonResponse> {
    let source_path = Path::new(root);
    let store_path = artifact_store(source_path, &extras);
    let _lock = try_acquire_lock(store_path)?;

    let mut status = pipeline_status::default_status(store_path);
    status.mode = Some("full".into());
    status.message = Some("Starting full pipeline".into());
    write_status(store_path, &status)?;

    let human = ctx.format != OutputFormat::Json;
    if human {
        print_plan();
    }

    let work_ctx = CliContext::new(
        Some(store_path.to_path_buf()),
        None,
        ctx.format.clone(),
        None,
        ctx.verbose,
    );

    let mut last_response: Option<DiscoverJsonResponse> = None;

    // --- stage 1 ---
    let skip_basic = should_skip_basic(
        source_path,
        store_path,
        extras.languages.as_deref(),
        extras.exclude.as_deref(),
    );
    if skip_basic {
        pipeline_status::set_stage(store_path, STAGE_BASIC, StageStatus::Skipped)?;
        if human {
            info!("[✓] Initial discover process is complete (snapshot reused)");
        }
    } else {
        pipeline_status::set_stage(store_path, STAGE_BASIC, StageStatus::Running)?;
        let force_reindex =
            pipeline_status::read_digest_marker(store_path, MATERIALIZED_FIELDS_DIGEST_FILE)
                .is_none();
        match run_full_analysis(
            &work_ctx,
            root,
            analysis_opts(&work_ctx, &extras, BasicOrDeep::Basic { force_reindex }),
        ) {
            Ok(outcome) => {
                last_response = Some(outcome.response);
                let mut st =
                    pipeline_status::set_stage(store_path, STAGE_BASIC, StageStatus::Complete)?;
                st.graph_digest = Some(outcome.graph_digest);
                write_status(store_path, &st)?;
                if human {
                    info!("[✓] Initial discover process is complete");
                }
            }
            Err(err) => {
                pipeline_status::set_stage(store_path, STAGE_BASIC, StageStatus::Failed)?;
                return Err(err).context("full pipeline: basic_discover");
            }
        }
    }

    // --- stage 2 ---
    if should_skip_deep(store_path) {
        pipeline_status::set_stage(store_path, STAGE_DEEP, StageStatus::Skipped)?;
    } else {
        pipeline_status::set_stage(store_path, STAGE_DEEP, StageStatus::Running)?;
        match run_full_analysis(
            &work_ctx,
            root,
            analysis_opts(&work_ctx, &extras, BasicOrDeep::Deep),
        ) {
            Ok(outcome) => {
                last_response = Some(outcome.response);
                let mut st =
                    pipeline_status::set_stage(store_path, STAGE_DEEP, StageStatus::Complete)?;
                st.graph_digest = Some(outcome.graph_digest);
                write_status(store_path, &st)?;
            }
            Err(err) => {
                pipeline_status::set_stage(store_path, STAGE_DEEP, StageStatus::Failed)?;
                return Err(err).context("full pipeline: deep_pass");
            }
        }
    }

    // --- stage 3 ---
    if should_skip_semantic(store_path) {
        pipeline_status::set_stage(store_path, STAGE_SEMANTIC, StageStatus::Skipped)?;
    } else {
        pipeline_status::set_stage(store_path, STAGE_SEMANTIC, StageStatus::Running)?;
        match run_index_with_emit(
            &work_ctx,
            SemanticIndexArgs {
                dimensions: DEFAULT_EMBEDDING_DIMENSIONS,
                incremental: true,
                embedder: CliEmbedderKind::Vocab,
                model: None,
                tokenizer: None,
                diffuse: false,
                diffuse_alpha: 0.25,
                diffuse_iters: 2,
                diffuse_bidirectional: false,
                scope: CliSemanticScope::Function,
                embed_bodies: false,
            },
            false,
        ) {
            Ok(()) => {
                pipeline_status::set_stage(store_path, STAGE_SEMANTIC, StageStatus::Complete)?;
                if human {
                    info!("[✓] Semantic index complete");
                }
            }
            Err(err) => {
                pipeline_status::set_stage(store_path, STAGE_SEMANTIC, StageStatus::Failed)?;
                return Err(err).context("full pipeline: semantic_index");
            }
        }
    }

    let mut final_status = pipeline_status::read_status(store_path);
    pipeline_status::refresh_ready_flags(&mut final_status, store_path);
    final_status.phase = Some("complete".into());
    if final_status
        .plan
        .iter()
        .any(|s| s.status == StageStatus::Failed)
    {
        final_status.message = Some("Pipeline failed".into());
    } else {
        final_status.message = Some("Full pipeline complete".into());
    }
    write_status(store_path, &final_status)?;

    let mut response = last_response.unwrap_or_else(|| DiscoverJsonResponse {
        schema_version: super::discover_output::DISCOVER_SCHEMA_VERSION,
        command: "discover".into(),
        full: Some(true),
        plan: Some(final_status.plan.clone()),
        artifacts: None,
        metrics: super::discover_output::DiscoverMetrics {
            files_discovered: 0,
            files_indexed: 0,
            files_skipped: 0,
            nodes_generated: 0,
            edges_generated: 0,
            duration_ms: 0,
        },
    });
    response.full = Some(true);
    response.plan = Some(final_status.plan);

    let mut artifacts = super::discover_output::DiscoverArtifacts::default();
    let dashboard = rgctl_graph::paths::artifact_path(store_path, "dashboard");
    if dashboard.join("manifest.json").is_file() {
        artifacts.dashboard_dir = Some(dashboard.to_string_lossy().into_owned());
    }
    if extras.export_migration_hints {
        let plan = rgctl_graph::paths::artifact_path(store_path, "migration_plan.json");
        if plan.is_file() {
            artifacts.migration_plan = Some(plan.to_string_lossy().into_owned());
        }
    }
    let kantra = rgctl_graph::paths::artifact_path(store_path, "kantra_findings.json");
    if kantra.is_file() {
        artifacts.kantra_findings = Some(kantra.to_string_lossy().into_owned());
    }
    if !artifacts.is_empty() {
        response.artifacts = Some(artifacts);
    }

    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(&response)?)?;
    }

    Ok(response)
}

enum BasicOrDeep {
    Basic { force_reindex: bool },
    Deep,
}

fn analysis_opts<'a>(
    ctx: &'a CliContext,
    extras: &'a FullPipelineArgs,
    which: BasicOrDeep,
) -> AnalysisOptions<'a> {
    let (with_cfg, with_dashboard, with_harmonic, force_reindex, deep) = match which {
        BasicOrDeep::Basic { force_reindex } => (false, false, false, force_reindex, false),
        BasicOrDeep::Deep => (true, true, true, false, true),
    };
    AnalysisOptions {
        languages: extras.languages.clone(),
        exclude: extras.exclude.clone(),
        with_security: extras.with_security,
        with_cfg,
        with_taint: deep && extras.with_taint,
        taint_rules: extras.taint_rules.clone(),
        with_dfg_loops: deep && extras.with_dfg_loops,
        with_ast_skeleton: deep && extras.with_ast_skeleton,
        write_json_graph: extras.write_json_graph,
        with_dashboard,
        export_migration_hints: deep && extras.export_migration_hints,
        with_harmonic,
        with_kantra: false,
        kantra_rules: None,
        kantra_catalog: None,
        kantra_target: None,
        kantra_index_only: false,
        migration_preset: extras.migration_preset.as_str(),
        migration_order: extras.migration_order.as_str(),
        db_path: &ctx.db,
        force_materialize_fields: true,
        force_reindex,
        emit_cli_summary: false,
        artifact_root: extras.artifact_root.as_deref(),
        limits: extras.limits.clone(),
        reuse_basic_analytics: deep,
    }
}

fn snapshot_digest(store: &Path) -> Option<String> {
    let path = MmappedGraphSnapshot::default_path(store);
    if !path.is_file() {
        return None;
    }
    let store = rgctl_graph::SnapshotNodeStore::open(&path).ok()?;
    store.content_digest().ok().map(str::to_string)
}

fn sources_unchanged(source: &Path, store: &Path, languages: Option<&str>, exclude: Option<&str>) -> bool {
    let mut discovery = DiscoveryConfig::default();
    if let Some(langs) = languages {
        discovery.languages = Some(
            langs
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        );
    }
    if let Some(excludes) = exclude {
        discovery.exclude_patterns = excludes
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    let registry = LanguageRegistry::new().into();
    let discoverer = FileDiscoverer::with_config(Arc::clone(&registry), discovery);
    let Ok(files) = discoverer.discover(source) else {
        return false;
    };
    let tracker = FileTracker::load(store).unwrap_or_else(|_| FileTracker::new(store));
    tracker
        .detect_changes(&files)
        .map(|c| c.is_empty())
        .unwrap_or(false)
}

fn should_skip_basic(
    source: &Path,
    store: &Path,
    languages: Option<&str>,
    exclude: Option<&str>,
) -> bool {
    let Some(digest) = snapshot_digest(store) else {
        return false;
    };
    let marker = pipeline_status::read_digest_marker(store, MATERIALIZED_FIELDS_DIGEST_FILE);
    marker.as_deref() == Some(digest.as_str()) && sources_unchanged(source, store, languages, exclude)
}

fn should_skip_deep(store: &Path) -> bool {
    let Some(digest) = snapshot_digest(store) else {
        return false;
    };
    let dash = rgctl_graph::paths::artifact_path(store, "dashboard/index.html");
    if !dash.is_file() {
        return false;
    }
    if !crate::analysis::CfgPdgArchive::default_path(store).is_file() {
        return false;
    }
    pipeline_status::read_digest_marker(store, HARMONIC_DIGEST_FILE).as_deref()
        == Some(digest.as_str())
}

fn should_skip_semantic(store: &Path) -> bool {
    let Some(digest) = snapshot_digest(store) else {
        return false;
    };
    let Ok(Some(index)) = SemanticIndex::open_if_exists(store) else {
        return false;
    };
    index.graph_digest.as_deref() == Some(digest.as_str())
}

/// Spawn the full pipeline on a dedicated OS thread (HTTP/MCP).
pub fn spawn_full_pipeline(root: PathBuf, verbose: bool) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("rgctl-pipeline".into())
        .spawn(move || {
            let ctx = CliContext::new(Some(root.clone()), None, OutputFormat::Json, None, verbose);
            let path = root.to_string_lossy().into_owned();
            if let Err(err) = run_full_pipeline(&ctx, &path, FullPipelineArgs::default_serve()) {
                eprintln!("[!] full pipeline: {err:#}");
            }
        })
        .expect("spawn pipeline thread")
}
