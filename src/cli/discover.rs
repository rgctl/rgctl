//! `rgctl discover` — index and analyze a repository.

use super::args::OutputFormat;
use super::context::CliContext;
use super::discover_impl::{AnalysisOptions, run_full_analysis};
use super::pipeline_session::{FullPipelineArgs, run_full_pipeline};
use anyhow::Result;
use std::path::Path;

#[derive(Clone)]
pub struct DiscoverArgs {
    pub path: Option<String>,
    /// When set, list matching project-root candidates and exit (no index).
    pub find_roots: Option<String>,
    pub languages: Option<String>,
    pub exclude: Option<String>,
    /// Secret scanning. Default off.
    pub with_security: bool,
    /// CFG / dominators / PDG. Default off.
    pub with_cfg: bool,
    /// Discover-time taint (implies CFG pass). Default off.
    pub with_taint: bool,
    /// Extra taint rule pack path (file or directory).
    pub taint_rules: Option<String>,
    /// Classify loop-carried PDG data deps (implies CFG). Default off.
    pub with_dfg_loops: bool,
    /// Write coarse AST skeleton archive (implies CFG). Default off.
    pub with_ast_skeleton: bool,
    /// Also write legacy JSON graph files (`graph.db` / `graph.json`).
    pub write_json_graph: bool,
    /// Export `.rgctl/dashboard/` bundle. Default off.
    pub with_dashboard: bool,
    /// Write a migration roadmap JSON after analysis completes.
    pub export_migration_hints: bool,
    /// Compute harmonic centrality (HyperBall on large graphs). Default off.
    pub with_harmonic: bool,
    /// Native Kantra rule evaluation during discover.
    pub with_kantra: bool,
    /// Override embedded catalog with a single ruleset directory.
    pub kantra_rules: Option<String>,
    /// Override embedded catalog with a ruleset tree (`ruleset.yaml` dirs).
    pub kantra_catalog: Option<String>,
    /// Evaluate only rules with `konveyor.io/target=<name>`.
    pub kantra_target: Option<String>,
    /// Hydrate Kantra rule nodes only; skip violation eval.
    pub kantra_index_only: bool,
    /// Staged full pipeline (`--full`).
    pub full: bool,
    /// Resource limits (`--with-limits SPEC` / `RGCTL_WITH_LIMITS`).
    pub with_limits: Option<String>,
    /// Preset strategy for `--export-migration-hints` (default: hybrid_default).
    pub migration_preset: String,
    /// Roadmap row order: `scheduled` (deps) or `priority` (score rank).
    pub migration_order: String,
    /// When set, persist artifacts here instead of the scanned tree (daemon cache).
    pub artifact_root: Option<std::path::PathBuf>,
    /// Incremental file paths (repo-relative); skips full discover when set.
    pub files: Option<Vec<String>>,
    /// Reverse call-dependency hops for `--files` updates.
    pub cascade_depth: usize,
}

/// Choose the session `--repo` base before joining a relative positional `PATH`.
///
/// Absolute `PATH` wins. Relative `PATH` (including `"."`) keeps `--repo` so
/// `rgctl -r <REPO> discover .` and `rgctl -r <REPO> discover <subdir>` do not
/// replace `--repo` with a cwd-relative path.
pub fn session_repo_from_flags(
    repo_flag: Option<&std::path::Path>,
    positional: Option<&str>,
) -> Option<std::path::PathBuf> {
    match positional {
        Some(p) if std::path::Path::new(p).is_absolute() => Some(std::path::PathBuf::from(p)),
        _ => repo_flag.map(std::path::Path::to_path_buf),
    }
}

/// Resolve discover/update/serve root: absolute PATH, PATH joined to `--repo`, or `--repo`/cwd.
pub fn resolve_session_root(ctx: &CliContext, path: Option<&str>) -> String {
    let raw = path
        .map(|p| {
            if std::path::Path::new(p).is_absolute() {
                p.to_string()
            } else {
                ctx.repo.join(p).to_string_lossy().into_owned()
            }
        })
        .unwrap_or_else(|| ctx.repo.to_string_lossy().into_owned());
    let p = std::path::PathBuf::from(&raw);
    p.canonicalize()
        .or_else(|_| std::env::current_dir().map(|cwd| cwd.join(&p)))
        .unwrap_or(p)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod session_root_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn dash_r_with_dot_keeps_repo_flag() {
        let repo = Path::new("/tmp/my-repo");
        assert_eq!(
            session_repo_from_flags(Some(repo), Some(".")),
            Some(repo.to_path_buf())
        );
    }

    #[test]
    fn dash_r_with_relative_subdir_keeps_repo_flag() {
        let repo = Path::new("/tmp/my-repo");
        assert_eq!(
            session_repo_from_flags(Some(repo), Some("pkg/core")),
            Some(repo.to_path_buf())
        );
    }

    #[test]
    fn absolute_positional_wins_over_dash_r() {
        let repo = Path::new("/tmp/my-repo");
        assert_eq!(
            session_repo_from_flags(Some(repo), Some("/abs/other")),
            Some(Path::new("/abs/other").to_path_buf())
        );
    }

    #[test]
    fn no_dash_r_relative_uses_cwd_via_none_base() {
        assert_eq!(session_repo_from_flags(None, Some("subdir")), None);
    }

    #[test]
    fn resolve_joins_relative_path_onto_repo() {
        let ctx = CliContext::new(
            Some(Path::new("/tmp/my-repo").to_path_buf()),
            None,
            OutputFormat::Text,
            None,
            false,
        );
        let root = resolve_session_root(&ctx, Some("pkg"));
        assert!(
            root.ends_with("my-repo/pkg") || root.ends_with("my-repo\\pkg"),
            "expected join under repo, got {root}"
        );
    }
}

pub fn run(ctx: &CliContext, args: DiscoverArgs) -> Result<()> {
    super::kantra_discover::validate_kantra_flags(
        args.with_kantra,
        &args.kantra_rules,
        &args.kantra_catalog,
    )?;
    let limits = super::discover_limits::DiscoverLimits::from_cli(args.with_limits.as_deref())?;
    let path = resolve_session_root(ctx, args.path.as_deref());

    if let Some(pat) = args.find_roots.as_deref() {
        return run_find_roots(ctx, &path, pat);
    }

    if let Some(files) = &args.files {
        return run_files_update(ctx, &path, files.clone(), &args);
    }

    if args.full {
        run_full_pipeline(
            ctx,
            &path,
            FullPipelineArgs::from_discover(&args, limits.clone()),
        )?;
        return Ok(());
    }

    let _ = run_full_analysis(
        ctx,
        &path,
        AnalysisOptions {
            languages: args.languages,
            exclude: args.exclude,
            with_security: args.with_security,
            with_cfg: args.with_cfg,
            with_taint: args.with_taint,
            taint_rules: args.taint_rules.clone(),
            with_dfg_loops: args.with_dfg_loops,
            with_ast_skeleton: args.with_ast_skeleton,
            write_json_graph: args.write_json_graph,
            with_dashboard: args.with_dashboard,
            export_migration_hints: args.export_migration_hints,
            with_harmonic: args.with_harmonic,
            with_kantra: args.with_kantra,
            kantra_rules: args.kantra_rules.clone(),
            kantra_catalog: args.kantra_catalog.clone(),
            kantra_target: args.kantra_target.clone(),
            kantra_index_only: args.kantra_index_only,
            migration_preset: &args.migration_preset,
            migration_order: &args.migration_order,
            db_path: &ctx.db,
            force_materialize_fields: false,
            force_reindex: false,
            emit_cli_summary: true,
            artifact_root: args.artifact_root.as_deref(),
            limits,
            reuse_basic_analytics: false,
        },
    )?;
    Ok(())
}

fn run_files_update(ctx: &CliContext, path: &str, files: Vec<String>, args: &DiscoverArgs) -> Result<()> {
    // Compatible alias for `rgctl update --files` (incremental structural patch only).
    super::update::run(
        ctx,
        super::update::UpdateArgs {
            path: Some(path.to_string()),
            files: Some(files),
            cascade_depth: args.cascade_depth,
            languages: args.languages.clone(),
            exclude: args.exclude.clone(),
            ..super::update::UpdateArgs::default()
        },
    )
}

/// List directories under `root` whose path/name matches a glob (project-root locator).
fn run_find_roots(ctx: &CliContext, root: &str, pattern: &str) -> Result<()> {
    let root_path = Path::new(root);
    let mut hits: Vec<String> = Vec::new();
    let markers = [
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "Cargo.toml",
        "package.json",
        "go.mod",
        "settings.gradle",
    ];
    for entry in ignore::WalkBuilder::new(root_path)
        .max_depth(Some(6))
        .git_ignore(true)
        .build()
        .flatten()
    {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let rel = path
            .strip_prefix(root_path)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let name_ok = rgctl_graph::glob_match(pattern, name);
        let rel_ok = rgctl_graph::glob_match(pattern, &rel);
        if !(name_ok || rel_ok) {
            continue;
        }
        let looks_like_project = markers.iter().any(|m| path.join(m).is_file())
            || path.join("src").is_dir()
            || path.join("pom.xml").is_file();
        if looks_like_project || name_ok {
            hits.push(if rel.is_empty() {
                ".".into()
            } else {
                rel
            });
        }
    }
    hits.sort();
    hits.dedup();
    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::json!({
            "schema_version": 1,
            "command": "discover_find",
            "pattern": pattern,
            "roots": hits,
            "returned": hits.len(),
        }))?;
    } else if hits.is_empty() {
        ctx.stdout_line("discover --find: (no matching project roots)")?;
    } else {
        for h in hits {
            ctx.stdout_line(&h)?;
        }
    }
    Ok(())
}
