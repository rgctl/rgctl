//! rgctl CLI command definitions and dispatch.

mod args;
mod blast_radius;
pub mod blast_radius_output;
mod check;
mod pr_check;
mod pr_check_output;
mod review;
mod review_export;
mod temporal_prep;
pub mod check_output;
mod clones;
mod communities;
mod context;
mod cpg;
mod diff;
mod discover;
mod discover_cfg;
mod discover_impl;
mod discover_limits;
mod kantra_discover;
pub mod discover_output;
mod export;
mod gql;
pub mod gql_output;
mod http_serve;
mod inspect;
pub mod inspect_output;
mod agent_pack;
mod install;
pub mod install_output;
mod markup;
mod metrics;
pub mod metrics_output;
mod pipeline_session;
pub mod pipeline_status;
mod policy_file;
mod semantic;
mod semantic_api;
pub mod semantic_output;
mod slice;
pub mod slice_output;
mod stage_profile;
mod structured_query;
mod session_status;
pub mod update;
mod file_watch;
mod help_ia;
mod rules;
mod vuln_deps;

pub use args::{OutputFormat, SymbolTargetArgs};
pub use help_ia::root_command;

use crate::BUILD_INFO;
use crate::analysis::{DEFAULT_CANDIDATE_POOL, DEFAULT_EMBEDDING_DIMENSIONS};
use args::{
    ExportFormat, InspectLayer, PdgEdgeLayer, SkillHost, SliceDirection, SliceView,
};
use clap::{ArgAction, Parser, Subcommand};
use context::CliContext;
use std::time::{Duration, Instant};

/// Merge repeated `-e` / `--exclude` values and comma-separated lists into one CSV.
fn join_exclude_patterns(patterns: &[String]) -> Option<String> {
    let parts: Vec<&str> = patterns
        .iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(","))
    }
}

#[derive(Parser)]
#[command(name = "rgctl")]
#[command(about = "A code knowledge graph built for LLM agents", version = BUILD_INFO)]
pub struct Cli {
    /// Path to the graph cache database
    #[arg(short = 'd', long = "db", global = true)]
    pub db: Option<std::path::PathBuf>,

    /// Target repository root
    #[arg(short = 'r', long = "repo", global = true)]
    pub repo: Option<std::path::PathBuf>,

    /// Output format (`text`, `json`, …). Also set via `RGCTL_FORMAT`.
    #[arg(
        short = 'f',
        long = "format",
        value_enum,
        global = true,
        env = "RGCTL_FORMAT"
    )]
    pub format: Option<OutputFormat>,

    /// Write output to file instead of stdout
    #[arg(short = 'o', long = "output", global = true)]
    pub output: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Index and analyze a codebase
    #[command(display_order = 1)]
    Discover {
        /// Repository path (defaults to --repo or cwd)
        #[arg(value_name = "PATH")]
        path: Option<String>,

        /// Locate candidate project roots under PATH matching a glob (e.g. `*coolstore*`); no full index
        #[arg(long = "find", value_name = "GLOB",
            help_heading = "Paths & filters")]
        find_roots: Option<String>,

        #[arg(short = 'l', long = "languages",
            help_heading = "Paths & filters")]
        languages: Option<String>,

        /// Path exclude glob (repeatable; comma-separated values also accepted)
        #[arg(short = 'e', long = "exclude", action = ArgAction::Append,
            help_heading = "Paths & filters")]
        exclude: Vec<String>,

        #[arg(short = 'v', long = "verbose",
            help_heading = "Session / limits")]
        verbose: bool,

        /// Secret scanning (SecretDetector). Off by default.
        #[arg(long = "with-security", visible_alias = "security",
            help_heading = "Pipeline features")]
        with_security: bool,

        /// Per-function CFG, dominators, and PDG → `.rgctl/analysis/` + cfg_pdg archive.
        /// Off by default. Does **not** include discover-time taint (see `--with-taint`).
        /// Large C++ corpora (e.g. llvm `clang/`) run CFG on parallel workers with a 16 MiB
        /// stack; pathological deep ASTs are skipped at depth 2048 (see `docs/internal/profile.md`).
        #[arg(long = "with-cfg", visible_alias = "cfg",
            help_heading = "Pipeline features")]
        with_cfg: bool,

        /// Discover-time taint analysis (requires CFG/PDG; implies CFG pass if needed).
        /// Off by default. On-demand: `slice ... --taint`.
        #[arg(long = "with-taint",
            help_heading = "Pipeline features")]
        with_taint: bool,

        /// Extra taint rule pack (YAML file or directory). Merged after built-ins and
        /// `.rgctl/taint-rules.d/` (see docs). Only used with `--with-taint`.
        #[arg(long = "taint-rules", value_name = "PATH",
            help_heading = "Pipeline features")]
        taint_rules: Option<String>,

        /// Classify loop-carried data dependencies on the PDG (implies CFG).
        #[arg(long = "with-dfg-loops",
            help_heading = "Pipeline features")]
        with_dfg_loops: bool,

        /// Write coarse AST skeleton archive under `.rgctl/analysis/` (implies CFG).
        #[arg(long = "with-ast-skeleton",
            help_heading = "Pipeline features")]
        with_ast_skeleton: bool,

        /// Write legacy JSON graph files (`graph.db` / `graph.json`); default is snapshot-only.
        #[arg(long = "write-json-graph",
            help_heading = "Session / limits")]
        write_json_graph: bool,

        /// Export the static dashboard bundle under `.rgctl/dashboard/`. Off by default.
        #[arg(long = "with-dashboard",
            help_heading = "Session / limits")]
        with_dashboard: bool,

        /// Write a migration roadmap JSON after analysis (default: `.rgctl/migration_plan.json`).
        /// Alias: `--export-migration-plan` (deprecated name).
        #[arg(
            long = "export-migration-hints",
            visible_alias = "export-migration-plan",
            help_heading = "Migration")]
        export_migration_hints: bool,

        /// Compute harmonic centrality (exact or HyperBall). Off by default — needed for
        /// migration ranking; adds ~30s and multi‑GB peak RSS on kernel-scale graphs.
        #[arg(long = "with-harmonic",
            help_heading = "Migration")]
        with_harmonic: bool,

        /// Evaluate Konveyor Kantra rules natively during discover (embedded catalog by default).
        #[arg(long = "with-kantra",
            help_heading = "Pipeline features")]
        with_kantra: bool,

        /// Override embedded catalog with a Kantra rules directory (`ruleset.yaml` + `*.yaml`).
        #[arg(long = "kantra-rules", value_name = "DIR",
            help_heading = "Pipeline features")]
        kantra_rules: Option<String>,

        /// Override embedded catalog with a ruleset tree (walks for `ruleset.yaml` dirs).
        #[arg(long = "kantra-catalog", value_name = "ROOT",
            help_heading = "Pipeline features")]
        kantra_catalog: Option<String>,

        /// Evaluate only rules labeled `konveyor.io/target=<NAME>`.
        #[arg(long = "kantra-target", value_name = "NAME",
            help_heading = "Pipeline features")]
        kantra_target: Option<String>,

        /// Index Kantra rules into the graph without running violation eval.
        #[arg(long = "kantra-index-only",
            help_heading = "Pipeline features")]
        kantra_index_only: bool,

        /// Staged full pipeline: basic discover (queryable snapshot), then CFG + dashboard +
        /// harmonic, then semantic index. Prints a plan first; does not imply taint/security.
        #[arg(long = "full",
            help_heading = "Session / limits")]
        full: bool,

        /// Constrain resource use for containers / small machines.
        /// Spec: `max-mem-mb=4096,threads=1` (comma-separated). Flag alone enables limits
        /// mode with no overrides. Env: `RGCTL_WITH_LIMITS`.
        #[arg(
            long = "with-limits",
            value_name = "SPEC",
            num_args = 0..=1,
            default_missing_value = "",
            help_heading = "Session / limits")]
        with_limits: Option<String>,

        /// Strategy preset for migration plan export.
        #[arg(
            long = "migration-preset",
            default_value = "hybrid_default",
            value_parser = ["hybrid_default", "foundational_first", "dense_cluster", "risk_mitigation"],
            help_heading = "Migration")]
        migration_preset: String,

        /// Roadmap sort order for migration plan export: scheduled (dependency-aware) or priority (score rank).
        #[arg(
            long = "migration-order",
            default_value = "scheduled",
            value_parser = ["scheduled", "priority"],
            help_heading = "Migration")]
        migration_order: String,

        /// Incremental update for repo-relative paths only (requires existing `.rgctl/` snapshot).
        /// Prefer `rgctl update --files` for the same behavior with clearer semantics.
        #[arg(long = "files", value_name = "PATH", value_delimiter = ',',
            help_heading = "Paths & filters")]
        files: Option<Vec<String>>,

        /// Reverse call-dependency hops when re-indexing `--files` (default 1; 0 = disabled).
        #[arg(long = "cascade-depth", default_value = "1",
            help_heading = "Paths & filters")]
        cascade_depth: usize,

        /// Flags after `--` (e.g. `discover . -- --full`)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        extra: Vec<String>,
    },

    /// Experimental graph query language — prefer `find` / `callers` / `relations` / `inventory` for common queries
    #[command(display_order = 16)]
    Gql {
        query: String,

        #[arg(long)]
        explain: bool,

        #[arg(long)]
        macro_name: Option<String>,
    },

    /// Find symbols by name/type over the mmap graph index (no GQL)
    #[command(display_order = 10)]
    Find {
        /// Name or glob (`Foo`, `User*`, `*Service*`). Omit with `--type` to list by type.
        #[arg(value_name = "PATTERN")]
        pattern: Option<String>,

        /// Filter by node type (function, class, import, annotation, …)
        #[arg(short = 't', long = "type", value_name = "TYPE")]
        type_name: Option<String>,

        /// Path glob filter
        #[arg(long = "file", value_name = "GLOB")]
        file: Option<String>,

        /// Language filter (java, rust, …)
        #[arg(short = 'l', long = "lang", value_name = "LANG")]
        lang: Option<String>,

        /// qualified_name prefix scope
        #[arg(long = "scope", value_name = "PREFIX")]
        scope: Option<String>,

        /// Scope mode: inside (default), outside, crossing
        #[arg(long = "scope-mode", value_name = "MODE")]
        scope_mode: Option<String>,

        /// Alias for `--scope-mode outside`
        #[arg(long = "exclude-scope")]
        exclude_scope: bool,

        /// Exact name match (disable glob)
        #[arg(long = "exact")]
        exact: bool,

        /// Max results [default: 50]
        #[arg(long = "limit", value_name = "N")]
        limit: Option<usize>,

        /// Return counts only
        #[arg(long = "count-only")]
        count_only: bool,

        /// Invert AnnotatedWith: entities carrying this annotation (`@Foo` or `Foo[,Bar…]`)
        #[arg(long = "annotation", value_name = "NAME[,NAME…]")]
        annotation: Option<String>,

        /// Include annotation argument text when indexed (requires --annotation)
        #[arg(long = "show-attributes")]
        show_attributes: bool,

        /// Package coordinates (PURL / GAV / crate) → import prefix filter
        #[arg(long = "package", value_name = "COORDS")]
        package: Option<String>,
    },

    /// Incoming CALLS neighbors for a symbol
    #[command(display_order = 11)]
    Callers {
        #[arg(value_name = "SYMBOL")]
        symbol: String,

        #[arg(long = "depth", default_value_t = 1)]
        depth: usize,

        #[arg(long = "file", value_name = "PATH")]
        file: Option<String>,

        #[arg(long = "class", value_name = "NAME")]
        class: Option<String>,

        /// Definition line (disambiguates same-name overloads)
        #[arg(long = "line", value_name = "N")]
        line: Option<usize>,

        #[arg(long = "scope", value_name = "PREFIX")]
        scope: Option<String>,

        #[arg(long = "scope-mode", value_name = "MODE")]
        scope_mode: Option<String>,

        #[arg(long = "exclude-scope")]
        exclude_scope: bool,

        #[arg(long = "limit", value_name = "N")]
        limit: Option<usize>,

        /// Package coordinates for facade / import-scope filtering
        #[arg(long = "package", value_name = "COORDS")]
        package: Option<String>,

        /// Filter / seed by method names (comma-separated; e.g. OSV affected_methods)
        #[arg(long = "methods", value_name = "NAME[,NAME…]", value_delimiter = ',')]
        methods: Vec<String>,
    },

    /// Outgoing CALLS neighbors for a symbol
    #[command(display_order = 12)]
    Callees {
        #[arg(value_name = "SYMBOL")]
        symbol: String,

        #[arg(long = "depth", default_value_t = 1)]
        depth: usize,

        #[arg(long = "file", value_name = "PATH")]
        file: Option<String>,

        #[arg(long = "class", value_name = "NAME")]
        class: Option<String>,

        /// Definition line (disambiguates same-name overloads)
        #[arg(long = "line", value_name = "N")]
        line: Option<usize>,

        #[arg(long = "scope", value_name = "PREFIX")]
        scope: Option<String>,

        #[arg(long = "scope-mode", value_name = "MODE")]
        scope_mode: Option<String>,

        #[arg(long = "exclude-scope")]
        exclude_scope: bool,

        #[arg(long = "limit", value_name = "N")]
        limit: Option<usize>,
    },

    /// Typed edge traversal; omit SYMBOL for seedless set-wide scan
    #[command(display_order = 13)]
    Relations {
        /// Optional seed symbol (omit for seedless typed-edge scan)
        #[arg(value_name = "SYMBOL")]
        symbol: Option<String>,

        /// Edge type (calls, uses, extends, implements, annotatedwith, …)
        #[arg(short = 'e', long = "edge", value_name = "TYPE")]
        edge: String,

        /// in | out | both [default: out]
        #[arg(long = "direction", default_value = "out")]
        direction: String,

        /// Filter edge source node type (seedless / seeded)
        #[arg(long = "from-type", value_name = "TYPE")]
        from_type: Option<String>,

        /// Filter edge target node type
        #[arg(long = "to-type", value_name = "TYPE")]
        to_type: Option<String>,

        #[arg(long = "depth", default_value_t = 1)]
        depth: usize,

        #[arg(long = "file", value_name = "PATH")]
        file: Option<String>,

        #[arg(long = "class", value_name = "NAME")]
        class: Option<String>,

        /// Definition line (disambiguates same-name overloads)
        #[arg(long = "line", value_name = "N")]
        line: Option<usize>,

        #[arg(long = "scope", value_name = "PREFIX")]
        scope: Option<String>,

        #[arg(long = "scope-mode", value_name = "MODE")]
        scope_mode: Option<String>,

        #[arg(long = "exclude-scope")]
        exclude_scope: bool,

        #[arg(long = "limit", value_name = "N")]
        limit: Option<usize>,
    },

    /// Aggregate symbol/edge counts (includes zero-count schema kinds for type/edge)
    #[command(display_order = 14)]
    Inventory {
        /// Aggregation dimension: type | edge | lang | file | community | import-prefix
        #[arg(long = "by", default_value = "type")]
        by: String,

        #[arg(long = "file", value_name = "GLOB")]
        file: Option<String>,

        #[arg(long = "scope", value_name = "PREFIX")]
        scope: Option<String>,

        #[arg(long = "scope-mode", value_name = "MODE")]
        scope_mode: Option<String>,

        #[arg(long = "exclude-scope")]
        exclude_scope: bool,
    },

    /// Session graph status (snapshot presence, digest, node/edge counts, source staleness; no rediscover)
    #[command(display_order = 2)]
    Status,

    /// Incremental structural graph update for changed files (not a full discover / analysis rebuild).
    ///
    /// Patches `graph.snapshot.bin` via `IncrementalUpdater`. Prefer this (or `serve --watch`)
    /// over re-running `discover` when sources change. Analysis sidecars (CFG, semantic, …)
    /// may be invalidated — re-run `discover` with the needed flags for those.
    ///
    /// When `serve --watch` is already running, this command enqueues work for the watcher
    /// (sole writer) and waits for the result by default (`--no-wait` to return after enqueue).
    #[command(display_order = 3)]
    Update {
        /// Repository path (defaults to `--repo` or cwd)
        #[arg(value_name = "PATH")]
        path: Option<String>,

        /// Update only these repo-relative paths (comma-separated)
        #[arg(long = "files", value_name = "PATH", value_delimiter = ',')]
        files: Option<Vec<String>>,

        /// Update files changed since this git ref (`git diff --name-only`)
        #[arg(long = "since", value_name = "REF")]
        since: Option<String>,

        /// Force a full structural rebuild through the updater (prefer `discover` for analysis).
        /// Rejected while `serve --watch` is live (stop the watcher or run full discover).
        #[arg(long)]
        force: bool,

        /// Reverse call-dependency hops when re-indexing changed files (default 1; 0 = disabled)
        #[arg(long = "cascade-depth", default_value = "1")]
        cascade_depth: usize,

        /// Restrict languages (comma-separated)
        #[arg(short = 'l', long = "languages")]
        languages: Option<String>,

        /// Exclude path globs (comma-separated)
        #[arg(short = 'e', long = "exclude", value_delimiter = ',')]
        exclude: Vec<String>,

        /// When a live watcher holds the lock, enqueue and exit without waiting for the result
        #[arg(long = "no-wait")]
        no_wait: bool,

        /// Seconds to wait for a queued update result when `serve --watch` is live (default 60)
        #[arg(
            long = "wait-timeout",
            value_name = "SECS",
            default_value = "60",
            env = "RGCTL_UPDATE_WAIT_TIMEOUT_SECS"
        )]
        wait_timeout: u64,
    },

    /// Evaluate Konveyor-shaped rules against the session (Kantra engine)
    #[command(display_order = 53)]
    Rules {
        #[command(subcommand)]
        action: RulesCommands,
    },

    /// Alias namespace for structured query verbs (`query find`, `query callers`, …)
    #[command(display_order = 15)]
    Query {
        #[command(subcommand)]
        action: QueryCommands,
    },

    /// Line-level program slice or taint trace
    #[command(display_order = 21)]
    Slice {
        file: String,

        #[arg(long)]
        line: usize,

        #[arg(long)]
        variable: String,

        #[arg(long)]
        function: Option<String>,

        #[arg(long)]
        language: Option<String>,

        #[arg(long, value_enum, default_value = "backward")]
        direction: SliceDirection,

        #[arg(long)]
        taint: bool,

        #[arg(long, value_enum, default_value = "text")]
        view: SliceView,
    },

    /// Macro impact / blast radius for a symbol
    #[command(display_order = 20)]
    BlastRadius {
        /// Function symbol name, UUID, or FQN (e.g. `Class::method`)
        #[arg(value_name = "SYMBOL")]
        symbol: String,

        /// Limit upstream impact zone to N incoming call hops (default: full transitive closure)
        #[arg(long, value_name = "N")]
        depth: Option<usize>,

        /// Run statement-level slice hand-off analysis (slow on large graphs)
        #[arg(long)]
        with_slices: bool,

        /// Explicit class or namespace filter
        #[arg(long, value_name = "NAME")]
        class: Option<String>,

        /// Explicit container source file path filter
        #[arg(long, value_name = "PATH")]
        file: Option<String>,

        /// Restrict to function defined on this 1-based source line
        #[arg(long, value_name = "N")]
        line: Option<usize>,

        #[arg(long, value_name = "PATH")]
        policy_file: Option<String>,

        #[arg(long)]
        no_policy: bool,

        /// Label impact nodes using boundary catalogs (REST / messaging / …)
        #[arg(long = "classify-boundary")]
        classify_boundary: bool,

        /// Restrict boundaries to kinds/methods (`REST_ENDPOINT`, `POST`, …); repeatable
        #[arg(long = "boundary", value_name = "KIND", action = clap::ArgAction::Append)]
        boundary: Vec<String>,
    },

    /// Sink-first taint (`--sink` + `--source external`); requires `discover --with-cfg`
    #[command(display_order = 22)]
    Taint {
        /// Sink symbol / method (e.g. `ObjectMapper.readValue`)
        #[arg(long = "sink", value_name = "SYMBOL")]
        sink: String,

        /// Source mode (v1: `external` only)
        #[arg(long = "source", default_value = "external")]
        source: String,

        /// Max call/dataflow depth
        #[arg(long = "depth", default_value_t = 8)]
        depth: usize,
    },

    /// Inspect raw CFG / PDG / dominance for a function symbol
    #[command(display_order = 23)]
    Inspect {
        symbol: String,
        #[command(flatten)]
        target: SymbolTargetArgs,
        #[command(subcommand)]
        layer: InspectLayer,
    },

    /// Network analytics (PageRank, betweenness, communities)
    #[command(display_order = 24)]
    Metrics {
        #[arg(long)]
        pagerank: bool,

        #[arg(long)]
        betweenness: bool,

        #[arg(long)]
        communities: bool,

        #[arg(long)]
        iterations: Option<usize>,
    },

    /// Opt-in semantic search over function symbols (separate index artifact)
    #[command(display_order = 25)]
    Semantic {
        #[command(subcommand)]
        action: SemanticCommands,
    },

    /// List or refresh named communities (analysis overlay)
    #[command(display_order = 26)]
    Communities {
        #[command(subcommand)]
        action: CommunitiesCommands,
    },

    /// Clone groups: exact Type-1 (`code_hash`) or bloom candidates — not semantic search
    #[command(
        display_order = 26,
        long_about = "Find duplicate function implementations (clone groups).\n\
                      Modes: exact (Type-1 code_hash groups) | bloom (token_bloom Jaccard candidates).\n\
                      Bloom results are candidates (high false-positive risk), not Type-1 clones.\n\
                      Distinct from `semantic query` (NL/embedding nearest neighbors)."
    )]
    Clones {
        /// Optional symbol: report only the clone group containing this function
        #[arg(value_name = "SYMBOL")]
        symbol: Option<String>,

        /// Clone mode: exact | bloom | fragment (semantic/structural reserved)
        #[arg(long, default_value = "exact")]
        mode: String,

        /// Seed for fragment clones: symbol or file:lines (e.g. PaymentService::processRefund or src/lib.rs:10-25)
        #[arg(long = "seed", value_name = "SEED")]
        seed: Option<String>,

        /// Line range for seed fragment (e.g. 45-55)
        #[arg(long = "lines", value_name = "START-END")]
        lines: Option<String>,

        /// Minimum statement count for fragment clones (default: 3)
        #[arg(long = "min-statements", value_name = "N")]
        min_statements: Option<usize>,

        /// Maximum statement count for fragment clones (default: 15)
        #[arg(long = "max-statements", value_name = "N")]
        max_statements: Option<usize>,

        /// Minimum function LOC to include (default: 5)
        #[arg(long = "min-loc", value_name = "N")]
        min_loc: Option<usize>,

        /// Exclude path needles/globs (repeatable), e.g. test, generated
        #[arg(long = "exclude", value_name = "GLOB")]
        exclude: Vec<String>,

        /// Language filter (e.g. java, c)
        #[arg(long = "lang", value_name = "ID")]
        language: Option<String>,

        /// Bloom min Jaccard threshold in [0,1] (default: 0.85; bloom mode only)
        #[arg(long = "threshold", value_name = "F")]
        threshold: Option<f64>,

        /// Disambiguate symbol by file path/glob
        #[arg(long = "file", value_name = "PATH")]
        file: Option<String>,

        /// Disambiguate symbol by enclosing class
        #[arg(long = "class", value_name = "NAME")]
        class: Option<String>,

        /// Disambiguate symbol by definition line
        #[arg(long = "line", value_name = "N")]
        line: Option<usize>,

        /// Do not write `.rgctl/clones.json` / `clones.bloom.json` (full-repo writes by default)
        #[arg(long = "no-write", default_value_t = false)]
        no_write: bool,

        /// Do not read/write the clones sidecar cache
        #[arg(long = "no-cache", default_value_t = false)]
        no_cache: bool,
    },

    /// Hybrid CPG façade (topology + CFG/PDG archive)
    #[command(display_order = 27)]
    Cpg {
        #[command(subcommand)]
        action: CpgCommands,
    },

    /// CI policy gateway
    #[command(display_order = 50)]
    Check {
        #[arg(long)]
        policy_file: String,

        #[arg(long)]
        base_ref: Option<String>,

        #[arg(long)]
        head_ref: Option<String>,

        #[arg(long)]
        strict: bool,

        /// Use temporal `pr-check` semantics (base/head snapshots + git scope)
        #[arg(long)]
        temporal: bool,

        /// Treat calendar `warn` violations as failures (with `--temporal`)
        #[arg(long = "strict-calendar")]
        strict_calendar: bool,
    },

    /// Temporal PR analysis (`review paths` / `review check`)
    #[command(display_order = 51)]
    Review {
        #[command(subcommand)]
        action: ReviewCommands,
    },

    /// Compatibility alias for `rgctl review check` (prefer `review check`)
    #[command(display_order = 52)]
    PrCheck {
        #[arg(long)]
        policy_file: String,

        /// Base graph artifact root, snapshot file, `$RGCTL_BASE_ARTIFACT`, or `{repo}/.rgctl-base`
        #[arg(long)]
        base_artifact: Option<String>,

        /// Head graph artifact root or snapshot file [default: `-r` / cwd repo]
        #[arg(long)]
        head_artifact: Option<String>,

        #[arg(long, default_value = "origin/main")]
        base_ref: String,

        #[arg(long, default_value = "HEAD")]
        head_ref: String,

        #[arg(long)]
        strict: bool,

        /// Reverse call-dependency hops when synthesizing a delta head (0 = disabled)
        #[arg(long, default_value = "1")]
        cascade_depth: usize,

        /// Require pre-built head snapshot; skip delta head synthesis from base artifact.
        #[arg(long = "full-snapshots")]
        full_snapshots: bool,

        /// Binary-search introducing commit for each new/regression violation
        #[arg(long)]
        bisect: bool,

        /// Synthetic head source (`worktree` = uncommitted changes over HEAD snapshot)
        #[arg(long = "synthetic-head", value_name = "MODE")]
        synthetic_head: Option<String>,

        /// Treat calendar `warn` violations as failures (grace / sunset warn windows)
        #[arg(long = "strict-calendar")]
        strict_calendar: bool,
    },

    /// Export graph or projections
    #[command(display_order = 60)]
    Export {
        #[arg(long = "export-format", value_enum)]
        export_format: ExportFormat,

        #[arg(long = "export-output", value_name = "FILE")]
        export_output: String,

        #[arg(long, default_value = "all")]
        query: String,
    },

    /// Serve the analysis dashboard and GQL query API over HTTP.
    ///
    /// Default: dashboard at `/` and query API at `/api/query` (alias `/graphql`).
    /// Starts the full discover pipeline unless `--no-pipeline`.
    #[command(display_order = 4)]
    Serve {
        /// Repository path to index (defaults to `--repo` or cwd)
        #[arg(value_name = "PATH")]
        path: Option<String>,

        /// Do not auto-run discover; fail fast if artifacts are missing
        #[arg(long = "no-pipeline")]
        no_pipeline: bool,

        /// Bind host [default: 127.0.0.1]
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// HTTP port [default: 8080]
        #[arg(long, default_value_t = 8080)]
        port: u16,

        /// Dashboard directory [default: `<repo>/.rgctl/dashboard`]
        #[arg(long, value_name = "DIR")]
        dashboard_dir: Option<std::path::PathBuf>,

        /// Open the dashboard in the default browser
        #[arg(long)]
        open: bool,

        /// Serve the query API only (no dashboard static files)
        #[arg(long)]
        query_only: bool,

        /// Serve the dashboard only (no query API)
        #[arg(long)]
        dashboard_only: bool,

        /// Watch source files and apply incremental graph updates (debounce via `rgctl.toml` `[watch]`)
        #[arg(long)]
        watch: bool,
    },

    /// Diff two columnar graph snapshots (cold diff profiling / compare path)
    #[command(display_order = 5)]
    Diff {
        /// Base snapshot file or directory containing `graph.snapshot.bin`
        #[arg(long, value_name = "PATH")]
        base: std::path::PathBuf,

        /// Head snapshot file or directory containing `graph.snapshot.bin`
        #[arg(long, value_name = "PATH")]
        head: std::path::PathBuf,
    },

    /// Install bundled agent pack (skills, optional policy)
    #[command(display_order = 61)]
    Install {
        /// Install the `rgctl` skill (references include workflow playbooks)
        #[arg(long = "skill")]
        skill: bool,

        /// Install structural policy snippet (Cursor rules; best-effort)
        #[arg(long = "with-policy")]
        with_policy: bool,

        /// Print agent registry and exit
        #[arg(long = "list-agents")]
        list_agents: bool,

        /// User-level agent dirs instead of repository-local paths
        #[arg(short = 'g', long = "global")]
        global_install: bool,

        /// Agent ids to install (comma-separated), or `all` for every adapter in the registry
        #[arg(long = "tools", value_delimiter = ',')]
        tools: Option<Vec<String>>,

        /// Deprecated: use `--tools`
        #[arg(long = "host", value_enum)]
        host: Option<SkillHost>,

        /// Overwrite rgctl-managed files that differ from the bundle
        #[arg(long)]
        force: bool,
    },

    /// OSV vulnerability triage / analyze
    #[command(display_order = 40)]
    Vuln {
        #[command(subcommand)]
        action: VulnCommands,
    },

    /// Dependency inventory match against OSV
    #[command(display_order = 41)]
    Deps {
        #[command(subcommand)]
        action: DepsCommands,
    },

    /// Security / OSV entrypoints (aliases of top-level `vuln`, `deps`, `taint`)
    #[command(display_order = 42)]
    Security {
        #[command(subcommand)]
        action: SecurityCommands,
    },

}

#[derive(Subcommand)]
pub enum VulnCommands {
    /// Parse and normalize an OSV JSON document (no repo scan)
    Triage {
        /// Path to OSV JSON file
        #[arg(long = "osv", value_name = "PATH")]
        osv: std::path::PathBuf,
    },
    /// Reachability pipeline → OpenVEX (triage → deps → package/callers → taint)
    Analyze {
        /// Path to OSV JSON file
        #[arg(long = "osv", value_name = "PATH")]
        osv: std::path::PathBuf,
        /// JAR/WAR roots (opt-in)
        #[arg(long = "include-jars", value_name = "DIR", num_args = 1..)]
        include_jars: Vec<std::path::PathBuf>,
        /// node_modules roots (opt-in)
        #[arg(long = "include-node-modules", value_name = "DIR", num_args = 1..)]
        include_node_modules: Vec<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum DepsCommands {
    /// Match OSV package against manifests and optional bundled artifacts
    Check {
        /// Path to OSV JSON file
        #[arg(long = "osv", value_name = "PATH")]
        osv: std::path::PathBuf,
        /// JAR/WAR roots to scan for embedded Maven poms (opt-in; e.g. `lib`)
        #[arg(long = "include-jars", value_name = "DIR", num_args = 1..)]
        include_jars: Vec<std::path::PathBuf>,
        /// Roots containing `node_modules/` to scan (opt-in)
        #[arg(long = "include-node-modules", value_name = "DIR", num_args = 1..)]
        include_node_modules: Vec<std::path::PathBuf>,
    },
}


#[derive(Subcommand)]
pub enum SecurityCommands {
    /// OSV vulnerability triage / analyze (same as `rgctl vuln`)
    Vuln {
        #[command(subcommand)]
        action: VulnCommands,
    },
    /// Dependency inventory match against OSV (same as `rgctl deps`)
    Deps {
        #[command(subcommand)]
        action: DepsCommands,
    },
    /// Sink-first taint (same as `rgctl taint`); requires `discover --with-cfg`
    Taint {
        /// Sink symbol / method (e.g. `ObjectMapper.readValue`)
        #[arg(long = "sink", value_name = "SYMBOL")]
        sink: String,

        /// Source mode (v1: `external` only)
        #[arg(long = "source", default_value = "external")]
        source: String,

        /// Max call/dataflow depth
        #[arg(long = "depth", default_value_t = 8)]
        depth: usize,
    },
}


#[derive(Subcommand)]
pub enum RulesCommands {
    /// Evaluate a ruleset directory (or catalog) against the current session graph
    Run {
        /// Ruleset directory (`ruleset.yaml` + `*.yaml`), like `--kantra-rules`
        #[arg(value_name = "DIR")]
        rules_dir: std::path::PathBuf,

        /// Filter by `konveyor.io/target` label
        #[arg(long = "target", value_name = "NAME")]
        target: Option<String>,

        /// Override with a rulesets tree (mutually exclusive with DIR as single ruleset when set)
        #[arg(long = "catalog", value_name = "ROOT")]
        catalog: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum ReviewCommands {
    /// Before/after call-path report for a PR (base/head snapshots)
    Paths {
        /// Base graph artifact root, snapshot file, `$RGCTL_BASE_ARTIFACT`, or `{repo}/.rgctl-base`
        #[arg(long)]
        base_artifact: Option<String>,

        /// Head graph artifact root or snapshot file [default: `-r` / cwd repo]
        #[arg(long)]
        head_artifact: Option<String>,

        #[arg(long, default_value = "origin/main")]
        base_ref: String,

        #[arg(long, default_value = "HEAD")]
        head_ref: String,

        /// Reverse call-dependency hops when synthesizing a delta head (0 = disabled)
        #[arg(long, default_value = "1")]
        cascade_depth: usize,

        /// Require pre-built head snapshot; skip delta head synthesis from base artifact.
        #[arg(long = "full-snapshots")]
        full_snapshots: bool,

        /// Synthetic head source (`worktree` = uncommitted changes over HEAD snapshot)
        #[arg(long = "synthetic-head", value_name = "MODE")]
        synthetic_head: Option<String>,

        /// Reverse-Calls hops in representative spines [default: 2]
        #[arg(long = "upstream-depth", default_value_t = 2)]
        upstream_depth: usize,

        /// Forward-Calls hops in representative spines [default: 1]
        #[arg(long = "downstream-depth", default_value_t = 1)]
        downstream_depth: usize,

        /// Restrict report to one symbol (name or stable-key hex); ambiguity lists candidates
        #[arg(long = "symbol", value_name = "NAME")]
        symbol: Option<String>,

        /// Max symbols in the report [default: 50]
        #[arg(long = "max-symbols", default_value_t = 50)]
        max_symbols: usize,

        /// Fan-out cap per hop when selecting spine neighbors [default: 10]
        #[arg(long = "fanout", default_value_t = 10)]
        fanout: usize,
    },

    /// Temporal PR policy gate (same as `rgctl pr-check`)
    Check {
        #[arg(long)]
        policy_file: String,

        /// Base graph artifact root, snapshot file, `$RGCTL_BASE_ARTIFACT`, or `{repo}/.rgctl-base`
        #[arg(long)]
        base_artifact: Option<String>,

        /// Head graph artifact root or snapshot file [default: `-r` / cwd repo]
        #[arg(long)]
        head_artifact: Option<String>,

        #[arg(long, default_value = "origin/main")]
        base_ref: String,

        #[arg(long, default_value = "HEAD")]
        head_ref: String,

        #[arg(long)]
        strict: bool,

        /// Reverse call-dependency hops when synthesizing a delta head (0 = disabled)
        #[arg(long, default_value = "1")]
        cascade_depth: usize,

        /// Require pre-built head snapshot; skip delta head synthesis from base artifact.
        #[arg(long = "full-snapshots")]
        full_snapshots: bool,

        /// Binary-search introducing commit for each new/regression violation
        #[arg(long)]
        bisect: bool,

        /// Synthetic head source (`worktree` = uncommitted changes over HEAD snapshot)
        #[arg(long = "synthetic-head", value_name = "MODE")]
        synthetic_head: Option<String>,

        /// Treat calendar `warn` violations as failures (grace / sunset warn windows)
        #[arg(long = "strict-calendar")]
        strict_calendar: bool,
    },

    /// Compose the review workflow into one versioned artifact
    Export {
        /// Base graph artifact root, snapshot file, `$RGCTL_BASE_ARTIFACT`, or `{repo}/.rgctl-base`
        #[arg(long)]
        base_artifact: Option<String>,

        /// Head graph artifact root or snapshot file [default: `-r` / cwd repo]
        #[arg(long)]
        head_artifact: Option<String>,

        #[arg(long, default_value = "origin/main")]
        base_ref: String,

        #[arg(long, default_value = "HEAD")]
        head_ref: String,

        /// Reverse call-dependency hops when synthesizing a delta head (0 = disabled)
        #[arg(long, default_value = "1")]
        cascade_depth: usize,

        /// Require pre-built head snapshot; skip delta head synthesis from base artifact.
        #[arg(long = "full-snapshots")]
        full_snapshots: bool,

        /// Synthetic head source (`worktree` = uncommitted changes over HEAD snapshot)
        #[arg(long = "synthetic-head", value_name = "MODE")]
        synthetic_head: Option<String>,

        /// Reverse-Calls hops in representative spines [default: 2]
        #[arg(long = "upstream-depth", default_value_t = 2)]
        upstream_depth: usize,

        /// Forward-Calls hops in representative spines [default: 1]
        #[arg(long = "downstream-depth", default_value_t = 1)]
        downstream_depth: usize,

        /// Restrict report to one symbol (name or stable-key hex); ambiguity lists candidates
        #[arg(long = "symbol", value_name = "NAME")]
        symbol: Option<String>,

        /// Max symbols in the report [default: 50]
        #[arg(long = "max-symbols", default_value_t = 50)]
        max_symbols: usize,

        /// Fan-out cap per hop when selecting spine neighbors [default: 10]
        #[arg(long = "fanout", default_value_t = 10)]
        fanout: usize,

        /// Policy file for the `policy` section. Omit to record it as not evaluated
        /// (the section is then `null`, never an empty "clean" result).
        #[arg(long = "policy-file", value_name = "PATH")]
        policy_file: Option<String>,

        /// Write the envelope to stdout instead of `{repo}/.rgctl/workflows/review.json`
        #[arg(long = "stdout")]
        stdout: bool,
    },
}

#[derive(Subcommand)]
pub enum QueryCommands {
    /// Alias for `rgctl find`
    Find {
        #[arg(value_name = "PATTERN")]
        pattern: Option<String>,
        #[arg(short = 't', long = "type", value_name = "TYPE")]
        type_name: Option<String>,
        #[arg(long = "file", value_name = "GLOB")]
        file: Option<String>,
        #[arg(short = 'l', long = "lang", value_name = "LANG")]
        lang: Option<String>,
        #[arg(long = "scope", value_name = "PREFIX")]
        scope: Option<String>,
        #[arg(long = "scope-mode", value_name = "MODE")]
        scope_mode: Option<String>,
        #[arg(long = "exclude-scope")]
        exclude_scope: bool,
        #[arg(long = "exact")]
        exact: bool,
        #[arg(long = "limit", value_name = "N")]
        limit: Option<usize>,
        #[arg(long = "count-only")]
        count_only: bool,
        #[arg(long = "annotation", value_name = "NAME[,NAME…]")]
        annotation: Option<String>,
        #[arg(long = "show-attributes")]
        show_attributes: bool,
        /// Package coordinates for facade / import-scope filtering
        #[arg(long = "package", value_name = "COORDS")]
        package: Option<String>,
    },
    /// Alias for `rgctl callers`
    Callers {
        symbol: String,
        #[arg(long = "depth", default_value_t = 1)]
        depth: usize,
        #[arg(long = "file")]
        file: Option<String>,
        #[arg(long = "class")]
        class: Option<String>,
        #[arg(long = "line", value_name = "N")]
        line: Option<usize>,
        #[arg(long = "scope")]
        scope: Option<String>,
        #[arg(long = "scope-mode")]
        scope_mode: Option<String>,
        #[arg(long = "exclude-scope")]
        exclude_scope: bool,
        #[arg(long = "limit")]
        limit: Option<usize>,
        /// Package coordinates for facade / import-scope filtering
        #[arg(long = "package", value_name = "COORDS")]
        package: Option<String>,
        /// Filter / seed by method names (comma-separated)
        #[arg(long = "methods", value_name = "NAME[,NAME…]", value_delimiter = ',')]
        methods: Vec<String>,
    },
    /// Alias for `rgctl callees`
    Callees {
        symbol: String,
        #[arg(long = "depth", default_value_t = 1)]
        depth: usize,
        #[arg(long = "file")]
        file: Option<String>,
        #[arg(long = "class")]
        class: Option<String>,
        #[arg(long = "line", value_name = "N")]
        line: Option<usize>,
        #[arg(long = "scope")]
        scope: Option<String>,
        #[arg(long = "scope-mode")]
        scope_mode: Option<String>,
        #[arg(long = "exclude-scope")]
        exclude_scope: bool,
        #[arg(long = "limit")]
        limit: Option<usize>,
    },
    /// Alias for `rgctl relations`
    Relations {
        symbol: Option<String>,
        #[arg(short = 'e', long = "edge")]
        edge: String,
        #[arg(long = "direction", default_value = "out")]
        direction: String,
        #[arg(long = "from-type")]
        from_type: Option<String>,
        #[arg(long = "to-type")]
        to_type: Option<String>,
        #[arg(long = "depth", default_value_t = 1)]
        depth: usize,
        #[arg(long = "file")]
        file: Option<String>,
        #[arg(long = "class")]
        class: Option<String>,
        #[arg(long = "line", value_name = "N")]
        line: Option<usize>,
        #[arg(long = "scope")]
        scope: Option<String>,
        #[arg(long = "scope-mode")]
        scope_mode: Option<String>,
        #[arg(long = "exclude-scope")]
        exclude_scope: bool,
        #[arg(long = "limit")]
        limit: Option<usize>,
    },
    /// Alias for `rgctl inventory`
    Inventory {
        #[arg(long = "by", default_value = "type")]
        by: String,
        #[arg(long = "file")]
        file: Option<String>,
        #[arg(long = "scope")]
        scope: Option<String>,
        #[arg(long = "scope-mode")]
        scope_mode: Option<String>,
        #[arg(long = "exclude-scope")]
        exclude_scope: bool,
    },
}

#[derive(Subcommand)]
pub enum SemanticCommands {
    /// Build `.rgctl/semantic_index.bin` from function symbols (not run by default discover)
    Index {
        /// Embedding dimensions before sign quantization (multiple of 8) [default: 256]
        #[arg(long, default_value_t = DEFAULT_EMBEDDING_DIMENSIONS)]
        dimensions: usize,

        /// Reuse embeddings for unchanged `code_hash` values [default: true]
        #[arg(long, default_value_t = true)]
        incremental: bool,

        /// Embedder backend: vocab (default, compiled token table), hash, onnx, or code-daemon
        #[arg(long, value_enum, default_value_t = semantic::CliEmbedderKind::Vocab)]
        embedder: semantic::CliEmbedderKind,

        /// Path to ONNX model (required for `--embedder onnx`; optional for code-daemon)
        #[arg(long, value_name = "PATH")]
        model: Option<std::path::PathBuf>,

        /// SentencePiece tokenizer for ONNX embedders (auto-detected beside `--model` when omitted)
        #[arg(long, value_name = "PATH")]
        tokenizer: Option<std::path::PathBuf>,

        /// Re-read function source and append body identifier tokens (off: declaration metadata only)
        #[arg(long, default_value_t = false)]
        embed_bodies: bool,

        /// Diffuse dense embeddings over the call graph before sign quantization
        #[arg(long, default_value_t = false)]
        diffuse: bool,

        /// Disable call-graph diffusion (default; overrides `--diffuse` when both are set)
        #[arg(long, default_value_t = false)]
        no_diffuse: bool,

        /// Jacobi blend weight toward neighbor mean [default: 0.25]
        #[arg(long, default_value_t = 0.25)]
        diffuse_alpha: f64,

        /// Jacobi diffusion iterations [default: 2]
        #[arg(long, default_value_t = 2)]
        diffuse_iters: usize,

        /// Include callers as well as callees in diffusion neighbors
        #[arg(long, default_value_t = false)]
        diffuse_bidirectional: bool,

        /// Index scope: functions (default), docs (headings), or all
        #[arg(long, value_enum, default_value = "function")]
        scope: semantic::CliSemanticScope,
    },

    /// Distill `vocab_tokens.txt` through a teacher embedder into an RBVK matrix
    Distill {
        /// RBVK destination (copy to crates/rgctl-analysis/assets/vocab_matrix.bin)
        #[arg(long = "matrix", value_name = "PATH")]
        matrix: std::path::PathBuf,

        /// Token list (one identifier per line). Defaults to analysis crate assets.
        #[arg(long, value_name = "PATH")]
        tokens: Option<std::path::PathBuf>,

        /// Teacher embedder (code-daemon recommended; hash for tests)
        #[arg(long, value_enum, default_value = "code-daemon")]
        embedder: semantic::CliEmbedderKind,

        /// Output dimensions (multiple of 8) [default: 256]
        #[arg(long, default_value_t = DEFAULT_EMBEDDING_DIMENSIONS)]
        dimensions: usize,

        /// Teacher batch size [default: 32]
        #[arg(long, default_value_t = 32)]
        batch_size: usize,

        /// Path to ONNX model (required for `--embedder onnx`; optional for code-daemon)
        #[arg(long, value_name = "PATH")]
        model: Option<std::path::PathBuf>,

        /// SentencePiece tokenizer for ONNX teachers
        #[arg(long, value_name = "PATH")]
        tokenizer: Option<std::path::PathBuf>,
    },

    /// Hamming nearest-neighbor search over the semantic index
    Query {
        /// Natural-language or keyword query
        #[arg(value_name = "TEXT")]
        query: String,

        /// Maximum hits to return [default: 20]
        #[arg(long, default_value_t = 20)]
        limit: usize,

        /// Expand top hits into graph context: neighbors, blast, gql, or all
        #[arg(long, value_enum, value_name = "MODE")]
        expand: Option<semantic::CliExpandMode>,

        /// CALLS hop depth for neighbor/gql expansion [default: 1]
        #[arg(long, default_value_t = 1)]
        expand_depth: usize,

        /// ONNX model path (when index was built with onnx/code-daemon)
        #[arg(long, value_name = "PATH")]
        model: Option<std::path::PathBuf>,

        /// SentencePiece tokenizer path (ONNX/code-daemon indexes)
        #[arg(long, value_name = "PATH")]
        tokenizer: Option<std::path::PathBuf>,

        /// Disable late fusion re-ranking (pure Hamming top-k)
        #[arg(long)]
        no_fusion: bool,

        /// Hamming candidate pool size before late fusion [default: 256]
        #[arg(long, default_value_t = DEFAULT_CANDIDATE_POOL)]
        candidate_pool: usize,

        /// Require all query keywords to match entry metadata (AND filter)
        #[arg(long)]
        keyword_and: bool,

        /// Search functions (default) or pooled communities
        #[arg(long, value_enum, default_value = "function")]
        scope: semantic::CliSemanticScope,
    },
}

#[derive(Subcommand)]
pub enum CommunitiesCommands {
    /// List communities with heuristic labels
    List,
    /// Refresh heuristic labels and write them into analysis_results.bin
    Label {
        /// Persist updated labels (default: true)
        #[arg(long, default_value_t = true)]
        write: bool,
    },
}

#[derive(Subcommand)]
pub enum CpgCommands {
    /// Show L_proc archive readiness (CFG/PDG)
    Status,
    /// Resolve a function in L_repo and whether L_proc exists
    Function {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        #[command(flatten)]
        target: SymbolTargetArgs,
    },
    /// CALL neighborhood for a function
    Calls {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        #[command(flatten)]
        target: SymbolTargetArgs,
    },
    /// Field mutations for a type (requires discover --with-cfg)
    Mutations {
        /// Type / class name (e.g. OrderDTO)
        #[arg(long = "type", value_name = "NAME")]
        type_name: String,
        /// Exclude constructor / `<init>` writes
        #[arg(long, default_value_t = false)]
        exclude_ctors: bool,
        /// Optional field name filter
        #[arg(long)]
        member: Option<String>,
        /// Include writes whose receiver type could not be resolved
        #[arg(long, default_value_t = false)]
        include_unresolved: bool,
    },
    /// Data/control flows from a variable at a line (wraps slice)
    Flows {
        file: String,
        #[arg(long)]
        line: usize,
        #[arg(long)]
        variable: String,
        /// Enclosing method / function name
        #[arg(long)]
        function: String,
        #[arg(long)]
        language: Option<String>,
        #[arg(long, value_enum, default_value = "forward")]
        direction: SliceDirection,
        /// Expand may-alias names (copies / field bases) — P3 T2 on-demand
        #[arg(long = "with-alias")]
        with_alias: bool,
    },
    /// Show coarse AST skeleton for a function (requires --with-ast-skeleton)
    Ast {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        #[command(flatten)]
        target: SymbolTargetArgs,
    },
    /// Export hybrid CPG view (GraphML / GraphSON)
    Export {
        /// graphml | graphson
        #[arg(long = "format", default_value = "graphson")]
        format: String,
        #[arg(long, value_name = "FILE")]
        output: String,
        /// Keep only nodes whose file_path contains this substring
        #[arg(long = "path-contains")]
        path_contains: Option<String>,
        /// Include PDG DATA_FLOW edges from CFG archive
        #[arg(long = "include-l-proc", default_value_t = true)]
        include_l_proc: bool,
        /// Include field-write sites from the mutation index
        #[arg(long = "include-field-writes", default_value_t = true)]
        include_field_writes: bool,
    },
    /// PDG overlay (wraps `inspect pdg`; prefers live rebuild today)
    Pdg {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        #[command(flatten)]
        target: SymbolTargetArgs,
        #[arg(long, value_enum, default_value = "all")]
        edge_layer: PdgEdgeLayer,
        #[arg(long)]
        def_use: bool,
    },
    /// Line-level slice (wraps `slice`)
    Slice {
        file: String,
        #[arg(long)]
        line: usize,
        #[arg(long)]
        variable: String,
        #[arg(long)]
        function: Option<String>,
        #[arg(long)]
        language: Option<String>,
        #[arg(long, value_enum, default_value = "backward")]
        direction: SliceDirection,
        #[arg(long)]
        taint: bool,
        #[arg(long, value_enum, default_value = "text")]
        view: SliceView,
    },
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        let wall_start = Instant::now();
        let command_label = command_label_for(&self.command);
        let long_running = matches!(self.command, Commands::Serve { .. });

        let verbose = matches!(self.command, Commands::Discover { verbose: true, .. });
        let format = self.format.clone().unwrap_or_default();
        let quiet_json = format == OutputFormat::Json && !verbose;
        let discover_json = matches!(self.command, Commands::Discover { .. })
            && format == OutputFormat::Json;
        init_logging(verbose, discover_json);

        if !long_running && !quiet_json {
            eprintln!("[>] rgctl {command_label}");
        }

        // Session root for discover/update/serve:
        // - absolute PATH → that path
        // - relative PATH (including ".") → keep `--repo` (or cwd); join in resolve_session_root
        let session_repo = match &self.command {
            Commands::Discover { path: Some(p), .. }
            | Commands::Serve { path: Some(p), .. }
            | Commands::Update { path: Some(p), .. } => {
                discover::session_repo_from_flags(self.repo.as_deref(), Some(p))
            }
            _ => self.repo.clone(),
        };

        let ctx = CliContext::new(session_repo, self.db, format, self.output, verbose);

        let result = match self.command {
            Commands::Discover {
                path,
                find_roots,
                languages,
                exclude,
                verbose: _,
                with_security,
                with_cfg,
                with_taint,
                taint_rules,
                with_dfg_loops,
                with_ast_skeleton,
                write_json_graph,
                with_dashboard,
                export_migration_hints,
                with_harmonic,
                with_kantra,
                kantra_rules,
                kantra_catalog,
                kantra_target,
                kantra_index_only,
                mut full,
                with_limits,
                migration_preset,
                migration_order,
                files,
                cascade_depth,
                extra,
            } => {
                if extra.iter().any(|a| a == "--full") {
                    full = true;
                }
                discover::run(
                    &ctx,
                    discover::DiscoverArgs {
                        path,
                        find_roots,
                        languages,
                        exclude: join_exclude_patterns(&exclude),
                        with_security,
                        with_cfg,
                        with_taint,
                        taint_rules,
                        with_dfg_loops,
                        with_ast_skeleton,
                        write_json_graph,
                        with_dashboard,
                        export_migration_hints,
                        with_harmonic,
                        with_kantra,
                        kantra_rules,
                        kantra_catalog,
                        kantra_target,
                        kantra_index_only,
                        full,
                        with_limits,
                        migration_preset,
                        migration_order,
                        artifact_root: None,
                        files,
                        cascade_depth,
                    },
                )
            }
            Commands::Gql {
                query,
                explain,
                macro_name,
            } => gql::run(
                &ctx,
                gql::GqlArgs {
                    query,
                    explain,
                    macro_name,
                },
            ),
            Commands::Slice {
                file,
                line,
                variable,
                function,
                language,
                direction,
                taint,
                view,
            } => slice::run(
                &ctx,
                slice::SliceArgs {
                    file,
                    line,
                    variable,
                    function,
                    language,
                    direction,
                    taint,
                    view,
                },
            ),
            Commands::BlastRadius {
                symbol,
                depth,
                policy_file,
                no_policy,
                with_slices,
                class,
                file,
                line,
                classify_boundary,
                boundary,
            } => blast_radius::run(
                &ctx,
                blast_radius::BlastRadiusArgs {
                    symbol,
                    depth,
                    policy_file,
                    no_policy,
                    with_slices,
                    class,
                    file,
                    line,
                    classify_boundary,
                    boundary,
                },
            ),
            Commands::Taint {
                sink,
                source,
                depth,
            } => vuln_deps::run_sink_taint(&ctx, sink, source, depth),
            Commands::Find {
                pattern,
                type_name,
                file,
                lang,
                scope,
                scope_mode,
                exclude_scope,
                exact,
                limit,
                count_only,
                annotation,
                show_attributes,
                package,
            } => structured_query::run_find(
                &ctx,
                pattern,
                type_name,
                structured_query::SharedQueryArgs {
                    file,
                    class: None,
                    line: None,
                    scope,
                    scope_mode,
                    exclude_scope,
                    lang,
                    limit,
                    package,
                    methods: None,
                },
                exact,
                count_only,
                annotation,
                show_attributes,
            ),
            Commands::Callers {
                symbol,
                depth,
                file,
                class,
                line,
                scope,
                scope_mode,
                exclude_scope,
                limit,
                package,
                methods,
            } => structured_query::run_call_neighbors(
                &ctx,
                symbol,
                true,
                depth,
                structured_query::SharedQueryArgs {
                    file,
                    class,
                    line,
                    scope,
                    scope_mode,
                    exclude_scope,
                    lang: None,
                    limit,
                    package,
                    methods: if methods.is_empty() {
                        None
                    } else {
                        Some(methods)
                    },
                },
            ),
            Commands::Callees {
                symbol,
                depth,
                file,
                class,
                line,
                scope,
                scope_mode,
                exclude_scope,
                limit,
            } => structured_query::run_call_neighbors(
                &ctx,
                symbol,
                false,
                depth,
                structured_query::SharedQueryArgs {
                    file,
                    class,
                    line,
                    scope,
                    scope_mode,
                    exclude_scope,
                    lang: None,
                    limit,
                    package: None,
                    methods: None,
                },
            ),
            Commands::Relations {
                symbol,
                edge,
                direction,
                from_type,
                to_type,
                depth,
                file,
                class,
                line,
                scope,
                scope_mode,
                exclude_scope,
                limit,
            } => structured_query::run_relations(
                &ctx,
                symbol,
                edge,
                direction,
                from_type,
                to_type,
                depth,
                structured_query::SharedQueryArgs {
                    file,
                    class,
                    line,
                    scope,
                    scope_mode,
                    exclude_scope,
                    lang: None,
                    limit,
                    package: None,
                    methods: None,
                },
            ),
            Commands::Inventory {
                by,
                file,
                scope,
                scope_mode,
                exclude_scope,
            } => structured_query::run_inventory(
                &ctx,
                by,
                structured_query::SharedQueryArgs {
                    file,
                    class: None,
                    line: None,
                    scope,
                    scope_mode,
                    exclude_scope,
                    lang: None,
                    limit: None,
                    package: None,
                    methods: None,
                },
            ),
            Commands::Status => session_status::run_status(&ctx),
            Commands::Update {
                path,
                files,
                since,
                force,
                cascade_depth,
                languages,
                exclude,
                no_wait,
                wait_timeout,
            } => update::run(
                &ctx,
                update::UpdateArgs {
                    path,
                    files,
                    since,
                    force,
                    cascade_depth,
                    languages,
                    exclude: join_exclude_patterns(&exclude),
                    no_wait,
                    wait_timeout_secs: wait_timeout,
                },
            ),
            Commands::Rules { action } => match action {
                RulesCommands::Run {
                    rules_dir,
                    target,
                    catalog,
                } => rules::run_rules(&ctx, rules_dir, target, catalog),
            },
            Commands::Query { action } => match action {
                QueryCommands::Find {
                    pattern,
                    type_name,
                    file,
                    lang,
                    scope,
                    scope_mode,
                    exclude_scope,
                    exact,
                    limit,
                    count_only,
                    annotation,
                    show_attributes,
                    package,
                } => structured_query::run_find(
                    &ctx,
                    pattern,
                    type_name,
                    structured_query::SharedQueryArgs {
                        file,
                        class: None,
                        line: None,
                        scope,
                        scope_mode,
                        exclude_scope,
                        lang,
                        limit,
                        package,
                        methods: None,
                    },
                    exact,
                    count_only,
                    annotation,
                    show_attributes,
                ),
                QueryCommands::Callers {
                    symbol,
                    depth,
                    file,
                    class,
                    line,
                    scope,
                    scope_mode,
                    exclude_scope,
                    limit,
                    package,
                    methods,
                } => structured_query::run_call_neighbors(
                    &ctx,
                    symbol,
                    true,
                    depth,
                    structured_query::SharedQueryArgs {
                        file,
                        class,
                        line,
                        scope,
                        scope_mode,
                        exclude_scope,
                        lang: None,
                        limit,
                        package,
                        methods: if methods.is_empty() {
                            None
                        } else {
                            Some(methods)
                        },
                    },
                ),
                QueryCommands::Callees {
                    symbol,
                    depth,
                    file,
                    class,
                    line,
                    scope,
                    scope_mode,
                    exclude_scope,
                    limit,
                } => structured_query::run_call_neighbors(
                    &ctx,
                    symbol,
                    false,
                    depth,
                    structured_query::SharedQueryArgs {
                        file,
                        class,
                        line,
                        scope,
                        scope_mode,
                        exclude_scope,
                        lang: None,
                        limit,
                    package: None,
                    methods: None,
                },
                ),
                QueryCommands::Relations {
                    symbol,
                    edge,
                    direction,
                    from_type,
                    to_type,
                    depth,
                    file,
                    class,
                    line,
                    scope,
                    scope_mode,
                    exclude_scope,
                    limit,
                } => structured_query::run_relations(
                    &ctx,
                    symbol,
                    edge,
                    direction,
                    from_type,
                    to_type,
                    depth,
                    structured_query::SharedQueryArgs {
                        file,
                        class,
                        line,
                        scope,
                        scope_mode,
                        exclude_scope,
                        lang: None,
                        limit,
                    package: None,
                    methods: None,
                },
                ),
                QueryCommands::Inventory {
                    by,
                    file,
                    scope,
                    scope_mode,
                    exclude_scope,
                } => structured_query::run_inventory(
                    &ctx,
                    by,
                    structured_query::SharedQueryArgs {
                        file,
                        class: None,
                        line: None,
                        scope,
                        scope_mode,
                        exclude_scope,
                        lang: None,
                        limit: None,
                    package: None,
                    methods: None,
                },
                ),
            },
            Commands::Inspect {
                symbol,
                target,
                layer,
            } => inspect::run(
                &ctx,
                inspect::InspectArgs {
                    symbol,
                    file: target.file,
                    class: target.class,
                    line: target.line,
                    layer,
                },
            ),
            Commands::Metrics {
                pagerank,
                betweenness,
                communities,
                iterations,
            } => metrics::run(
                &ctx,
                metrics::MetricsArgs {
                    pagerank,
                    betweenness,
                    communities,
                    iterations,
                },
            ),
            Commands::Semantic { action } => match action {
                SemanticCommands::Index {
                    dimensions,
                    incremental,
                    embedder,
                    model,
                    tokenizer,
                    embed_bodies,
                    diffuse,
                    no_diffuse,
                    diffuse_alpha,
                    diffuse_iters,
                    diffuse_bidirectional,
                    scope,
                } => semantic::run_index(
                    &ctx,
                    semantic::SemanticIndexArgs {
                        dimensions,
                        incremental,
                        embedder,
                        model,
                        tokenizer,
                        embed_bodies,
                        diffuse: diffuse && !no_diffuse,
                        diffuse_alpha,
                        diffuse_iters,
                        diffuse_bidirectional,
                        scope,
                    },
                ),
                SemanticCommands::Distill {
                    matrix,
                    tokens,
                    embedder,
                    dimensions,
                    batch_size,
                    model,
                    tokenizer,
                } => semantic::run_distill(
                    &ctx,
                    semantic::SemanticDistillArgs {
                        output: matrix,
                        tokens,
                        embedder,
                        dimensions,
                        batch_size,
                        model,
                        tokenizer,
                    },
                ),
                SemanticCommands::Query {
                    query,
                    limit,
                    expand,
                    expand_depth,
                    model,
                    tokenizer,
                    no_fusion,
                    candidate_pool,
                    keyword_and,
                    scope,
                } => semantic::run_query(
                    &ctx,
                    semantic::SemanticQueryArgs {
                        query,
                        limit,
                        expand,
                        expand_depth,
                        model,
                        tokenizer,
                        fusion: !no_fusion,
                        candidate_pool,
                        keyword_and,
                        scope,
                    },
                ),
            },
            Commands::Communities { action } => match action {
                CommunitiesCommands::List => communities::run_list(&ctx),
                CommunitiesCommands::Label { write } => {
                    communities::run_label(&ctx, communities::CommunitiesLabelArgs { write })
                }
            },
            Commands::Clones {
                symbol,
                mode,
                seed,
                lines,
                min_statements,
                max_statements,
                min_loc,
                exclude,
                language,
                threshold,
                file,
                class,
                line,
                no_write,
                no_cache,
            } => clones::run(
                &ctx,
                clones::ClonesArgs {
                    symbol,
                    mode,
                    seed,
                    lines,
                    min_statements,
                    max_statements,
                    min_loc,
                    exclude,
                    language,
                    threshold,
                    file,
                    class,
                    line,
                    write: !no_write,
                    no_cache,
                },
            ),
            Commands::Cpg { action } => {
                let mapped = match action {
                    CpgCommands::Status => cpg::CpgAction::Status,
                    CpgCommands::Function { symbol, target } => cpg::CpgAction::Function {
                        symbol,
                        file: target.file,
                        class: target.class,
                        line: target.line,
                    },
                    CpgCommands::Calls { symbol, target } => cpg::CpgAction::Calls {
                        symbol,
                        file: target.file,
                        class: target.class,
                        line: target.line,
                    },
                    CpgCommands::Mutations {
                        type_name,
                        exclude_ctors,
                        member,
                        include_unresolved,
                    } => cpg::CpgAction::Mutations {
                        type_name,
                        exclude_ctors,
                        member,
                        include_unresolved,
                    },
                    CpgCommands::Flows {
                        file,
                        line,
                        variable,
                        function,
                        language,
                        direction,
                        with_alias,
                    } => cpg::CpgAction::Flows {
                        file,
                        line,
                        variable,
                        function,
                        language,
                        direction,
                        with_alias,
                    },
                    CpgCommands::Ast { symbol, target } => cpg::CpgAction::Ast {
                        symbol,
                        file: target.file,
                        class: target.class,
                        line: target.line,
                    },
                    CpgCommands::Export {
                        format,
                        output,
                        path_contains,
                        include_l_proc,
                        include_field_writes,
                    } => cpg::CpgAction::Export {
                        format,
                        output,
                        path_contains,
                        include_l_proc,
                        include_field_writes,
                    },
                    CpgCommands::Pdg {
                        symbol,
                        target,
                        edge_layer,
                        def_use,
                    } => cpg::CpgAction::Pdg {
                        symbol,
                        file: target.file,
                        class: target.class,
                        line: target.line,
                        edge_layer,
                        def_use,
                    },
                    CpgCommands::Slice {
                        file,
                        line,
                        variable,
                        function,
                        language,
                        direction,
                        taint,
                        view,
                    } => cpg::CpgAction::Slice {
                        file,
                        line,
                        variable,
                        function,
                        language,
                        direction,
                        taint,
                        view,
                    },
                };
                cpg::run(&ctx, mapped)
            }
            Commands::Check {
                policy_file,
                base_ref,
                head_ref,
                strict,
                temporal,
                strict_calendar,
            } => check::run(
                &ctx,
                check::CheckArgs {
                    policy_file,
                    base_ref,
                    head_ref,
                    strict,
                    temporal,
                    strict_calendar,
                },
            ),
            Commands::Review { action } => match action {
                ReviewCommands::Paths {
                    base_artifact,
                    head_artifact,
                    base_ref,
                    head_ref,
                    cascade_depth,
                    full_snapshots,
                    synthetic_head,
                    upstream_depth,
                    downstream_depth,
                    symbol,
                    max_symbols,
                    fanout,
                } => review::run_paths(
                    &ctx,
                    review::ReviewPathsArgs {
                        base_artifact,
                        head_artifact,
                        base_ref,
                        head_ref,
                        cascade_depth,
                        full_snapshots,
                        synthetic_head,
                        upstream_depth,
                        downstream_depth,
                        symbol,
                        max_symbols,
                        fanout,
                    },
                ),
                ReviewCommands::Check {
                    policy_file,
                    base_artifact,
                    head_artifact,
                    base_ref,
                    head_ref,
                    strict,
                    cascade_depth,
                    full_snapshots,
                    bisect,
                    synthetic_head,
                    strict_calendar,
                } => review::run_check(
                    &ctx,
                    pr_check::PrCheckArgs {
                        policy_file,
                        base_artifact,
                        head_artifact,
                        base_ref,
                        head_ref,
                        strict,
                        cascade_depth,
                        full_snapshots,
                        bisect,
                        synthetic_head,
                        strict_calendar,
                    },
                ),
                ReviewCommands::Export {
                    base_artifact,
                    head_artifact,
                    base_ref,
                    head_ref,
                    cascade_depth,
                    full_snapshots,
                    synthetic_head,
                    upstream_depth,
                    downstream_depth,
                    symbol,
                    max_symbols,
                    fanout,
                    policy_file,
                    stdout,
                } => review_export::run_export(
                    &ctx,
                    review_export::ReviewExportArgs {
                        paths: review::ReviewPathsArgs {
                            base_artifact,
                            head_artifact,
                            base_ref,
                            head_ref,
                            cascade_depth,
                            full_snapshots,
                            synthetic_head,
                            upstream_depth,
                            downstream_depth,
                            symbol,
                            max_symbols,
                            fanout,
                        },
                        policy_file,
                        stdout,
                    },
                ),
            },
            Commands::PrCheck {
                policy_file,
                base_artifact,
                head_artifact,
                base_ref,
                head_ref,
                strict,
                cascade_depth,
                full_snapshots,
                bisect,
                synthetic_head,
                strict_calendar,
            } => review::run_check(
                &ctx,
                pr_check::PrCheckArgs {
                    policy_file,
                    base_artifact,
                    head_artifact,
                    base_ref,
                    head_ref,
                    strict,
                    cascade_depth,
                    full_snapshots,
                    bisect,
                    synthetic_head,
                    strict_calendar,
                },
            ),
            Commands::Export {
                export_format,
                export_output,
                query,
            } => export::run(
                &ctx,
                export::ExportArgs {
                    export_format,
                    export_output,
                    query,
                },
            ),
            Commands::Install {
                skill,
                with_policy,
                list_agents,
                global_install,
                tools,
                host,
                force,
            } => install::run(
                &ctx,
                install::InstallArgs {
                    skill,
                    with_policy,
                    list_agents,
                    global_install,
                    tools,
                    host,
                    force,
                },
            ),
            Commands::Vuln { action } => match action {
                VulnCommands::Triage { osv } => vuln_deps::run_vuln_triage(&ctx, osv),
                VulnCommands::Analyze {
                    osv,
                    include_jars,
                    include_node_modules,
                } => vuln_deps::run_vuln_analyze(&ctx, osv, include_jars, include_node_modules),
            },
            Commands::Deps { action } => match action {
                DepsCommands::Check {
                    osv,
                    include_jars,
                    include_node_modules,
                } => vuln_deps::run_deps_check(&ctx, osv, include_jars, include_node_modules),
            },
            Commands::Security { action } => match action {
                SecurityCommands::Vuln { action } => match action {
                    VulnCommands::Triage { osv } => vuln_deps::run_vuln_triage(&ctx, osv),
                    VulnCommands::Analyze {
                        osv,
                        include_jars,
                        include_node_modules,
                    } => vuln_deps::run_vuln_analyze(&ctx, osv, include_jars, include_node_modules),
                },
                SecurityCommands::Deps { action } => match action {
                    DepsCommands::Check {
                        osv,
                        include_jars,
                        include_node_modules,
                    } => vuln_deps::run_deps_check(&ctx, osv, include_jars, include_node_modules),
                },
                SecurityCommands::Taint {
                    sink,
                    source,
                    depth,
                } => vuln_deps::run_sink_taint(&ctx, sink, source, depth),
            },
            Commands::Diff { base, head } => diff::run(
                &ctx,
                diff::DiffArgs { base, head },
            ),
            Commands::Serve {
                path,
                no_pipeline,
                host,
                port,
                dashboard_dir,
                open,
                query_only,
                dashboard_only,
                watch,
            } => http_serve::serve(
                &ctx,
                http_serve::HttpServeArgs {
                    host,
                    port,
                    dashboard_dir,
                    open,
                    query_only,
                    dashboard_only,
                    no_pipeline,
                    path,
                    watch,
                },
            ),
        };

        if !long_running && !quiet_json {
            log_command_wall_time(command_label, wall_start.elapsed(), result.is_ok());
        }

        match result {
            Ok(()) => Ok(()),
            Err(err) if quiet_json => {
                if !ctx.json_error_emitted() {
                    let _ = ctx.emit_json_error_from_anyhow(&err);
                }
                // Exit here so anyhow's default `Error:` stderr dump does not pollute agents.
                std::process::exit(1);
            }
            Err(err) => Err(err),
        }
    }
}

fn command_label_for(command: &Commands) -> &'static str {
    match command {
        Commands::Discover { .. } => "discover",
        Commands::Gql { .. } => "gql",
        Commands::Find { .. } => "find",
        Commands::Callers { .. } => "callers",
        Commands::Callees { .. } => "callees",
        Commands::Relations { .. } => "relations",
        Commands::Inventory { .. } => "inventory",
        Commands::Status => "status",
        Commands::Update { .. } => "update",
        Commands::Rules { .. } => "rules",
        Commands::Query { action } => match action {
            QueryCommands::Find { .. } => "query find",
            QueryCommands::Callers { .. } => "query callers",
            QueryCommands::Callees { .. } => "query callees",
            QueryCommands::Relations { .. } => "query relations",
            QueryCommands::Inventory { .. } => "query inventory",
        },
        Commands::Slice { .. } => "slice",
        Commands::BlastRadius { .. } => "blast-radius",
        Commands::Taint { .. } => "taint",
        Commands::Inspect { .. } => "inspect",
        Commands::Metrics { .. } => "metrics",
        Commands::Semantic { action } => match action {
            SemanticCommands::Index { .. } => "semantic index",
            SemanticCommands::Query { .. } => "semantic query",
            SemanticCommands::Distill { .. } => "semantic distill",
        },
        Commands::Communities { action } => match action {
            CommunitiesCommands::List => "communities list",
            CommunitiesCommands::Label { .. } => "communities label",
        },
        Commands::Clones { .. } => "clones",
        Commands::Cpg { action } => match action {
            CpgCommands::Status => "cpg status",
            CpgCommands::Function { .. } => "cpg function",
            CpgCommands::Calls { .. } => "cpg calls",
            CpgCommands::Mutations { .. } => "cpg mutations",
            CpgCommands::Flows { .. } => "cpg flows",
            CpgCommands::Ast { .. } => "cpg ast",
            CpgCommands::Export { .. } => "cpg export",
            CpgCommands::Pdg { .. } => "cpg pdg",
            CpgCommands::Slice { .. } => "cpg slice",
        },
        Commands::Check { .. } => "check",
        Commands::Review { action } => match action {
            ReviewCommands::Paths { .. } => "review paths",
            ReviewCommands::Check { .. } => "review check",
            ReviewCommands::Export { .. } => "review export",
        },
        Commands::PrCheck { .. } => "pr-check",
        Commands::Export { .. } => "export",
        Commands::Install { .. } => "install",
        Commands::Vuln { action } => match action {
            VulnCommands::Triage { .. } => "vuln triage",
            VulnCommands::Analyze { .. } => "vuln analyze",
        },
        Commands::Deps { action } => match action {
            DepsCommands::Check { .. } => "deps check",
        },
        Commands::Security { action } => match action {
            SecurityCommands::Vuln { action } => match action {
                VulnCommands::Triage { .. } => "security vuln triage",
                VulnCommands::Analyze { .. } => "security vuln analyze",
            },
            SecurityCommands::Deps { action } => match action {
                DepsCommands::Check { .. } => "security deps check",
            },
            SecurityCommands::Taint { .. } => "security taint",
        },
        Commands::Diff { .. } => "diff",
        Commands::Serve { .. } => "serve",
    }
}

fn log_command_wall_time(command: &str, elapsed: Duration, ok: bool) {
    let mark = if ok { "✓" } else { "✗" };
    let duration = format_elapsed(elapsed);
    eprintln!("[{mark}] rgctl {command} finished in {duration}");
}

fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64();
    if secs < 1.0 {
        format!("{:.0}ms", secs * 1000.0)
    } else if secs < 10.0 {
        format!("{:.2}s", secs)
    } else {
        format!("{:.1}s", secs)
    }
}

fn init_logging(verbose: bool, discover_json: bool) {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt::format::FmtSpan;

    if verbose {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info,rgctl=debug,profile=info")),
            )
            .with_target(true)
            .with_level(true)
            .with_span_events(FmtSpan::CLOSE)
            .init();
    } else if discover_json {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("error")),
            )
            .with_target(false)
            .with_level(false)
            .with_ansi(false)
            .without_time()
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("warn,rgctl=info,rgctl_extraction=warn,rgctl_analysis=warn")
            }))
            .with_target(false)
            .with_level(false)
            .with_ansi(true)
            .without_time()
            .init();
    }
}
