//! HTTP server for the analysis dashboard and GQL query API (`rgctl serve`).

use super::context::CliContext;
use super::discover::resolve_session_root;
use super::pipeline_status::{self, PIPELINE_STATUS_SCHEMA_VERSION};
use super::semantic::SemanticQueryArgs;
use super::semantic_api::{
    execute_semantic_query_with_analysis, load_analysis_results, semantic_index_path,
    semantic_status, semantic_status_for_index,
};
use super::semantic_output::query_response_to_json;
use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, Method, Request, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use rgctl_analysis::{AnalysisResults, CommunityQueryContext, SemanticIndex};
use rgctl_dashboard::default_dashboard_path;
use rgctl_gql::QueryMacroRegistry;
use rgctl_graph::CodeGraph;
use rgctl_graph::backend::GraphBackend;
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_service::command::{ImpactArgs, QueryArgs};
use rgctl_service::impact::run_impact;
use rgctl_service::query::run_query;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tower::ServiceExt;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

/// Options for the unified HTTP `serve` command.
///
/// Serve is **serve-only**: it never starts `discover` / a background pipeline. Run
/// `rgctl discover . --with-universe` first. Always serves the universe UI.
pub struct HttpServeArgs {
    pub host: String,
    pub port: u16,
    pub dashboard_dir: Option<PathBuf>,
    pub open: bool,
    pub query_only: bool,
    pub path: Option<String>,
    /// Watch sources and run incremental `update` on change.
    pub watch: bool,
}

pub(crate) struct AppState {
    repo: PathBuf,
    dashboard_dir: PathBuf,
    data_dir: PathBuf,
    graph: RwLock<Option<CodeGraph>>,
    #[allow(dead_code)]
    registry: QueryMacroRegistry,
    semantic: RwLock<Option<Arc<SemanticIndex>>>,
    analysis: Option<Arc<AnalysisResults>>,
    community: RwLock<Option<CommunityQueryContext>>,
    source_cache: RwLock<Option<Arc<SourceLookup>>>,
    record_cache: RwLock<Option<Arc<RecordPacks>>>,
    engine_cache: RwLock<Option<Arc<rgctl_wasm::EngineContext>>>,
    dashboard_announced: AtomicBool,
}

#[derive(Debug, Deserialize)]
struct QueryRequest {
    query: Option<String>,
    #[serde(default)]
    explain: bool,
    #[serde(default)]
    r#macro: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SemanticQueryRequest {
    query: String,
    #[serde(default = "default_semantic_limit")]
    limit: usize,
    #[serde(default = "default_true")]
    fusion: bool,
    #[serde(default = "default_candidate_pool")]
    candidate_pool: usize,
    #[serde(default)]
    keyword_and: bool,
    #[serde(default)]
    expand: Option<String>,
    #[serde(default = "default_expand_depth")]
    expand_depth: usize,
    /// `function` (default) or `community`
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FunctionQuery {
    /// Exact function UUID; when present, returns at most one row.
    id: Option<String>,
    /// Case-insensitive substring over name and file path.
    q: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct FunctionRow {
    function_id: String,
    name: String,
    file_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct FunctionsResponse {
    functions: Vec<FunctionRow>,
    total: usize,
}

#[derive(Debug, Clone)]
struct SourceLookup {
    files: HashMap<String, String>,
    root: String,
}

#[derive(Debug, Deserialize)]
struct SourceQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecordQuery {
    id: String,
}

#[derive(Debug, Deserialize)]
struct BlastQuery {
    id: String,
    #[serde(default = "default_blast_depth")]
    depth: usize,
}

fn default_blast_depth() -> usize {
    5
}

#[derive(Debug, Clone)]
struct RecordPack {
    data_path: PathBuf,
    entries: HashMap<Uuid, (u64, u32)>,
}

#[derive(Debug, Clone, Default)]
struct RecordPacks {
    cfg: Option<RecordPack>,
    slice: Option<RecordPack>,
}

fn default_semantic_limit() -> usize {
    20
}

fn default_candidate_pool() -> usize {
    256
}

fn default_expand_depth() -> usize {
    1
}

fn default_true() -> bool {
    true
}

/// Start the HTTP server (dashboard static files + `/api/query` and `/graphql`).
pub fn serve(ctx: &CliContext, args: HttpServeArgs) -> Result<()> {
    let session_root = PathBuf::from(resolve_session_root(ctx, args.path.as_deref()));
    let session_ctx = CliContext::new(
        Some(session_root.clone()),
        None,
        ctx.format.clone(),
        None,
        ctx.verbose,
    );

    let dashboard_dir = args
        .dashboard_dir
        .clone()
        .unwrap_or_else(|| default_dashboard_path(&session_root));

    if !args.query_only {
        let index = dashboard_dir.join("index.html");
        if !index.is_file() {
            bail!(
                "dashboard not found at {}\n  \
                 Run `rgctl discover . --with-universe` first (serve does not index).",
                dashboard_dir.display()
            );
        }
        let before_mode = std::fs::read_to_string(dashboard_dir.join("manifest.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("mode").and_then(|m| m.as_str()).map(str::to_owned));
        rgctl_dashboard::ensure_static_shell(&dashboard_dir, rgctl_dashboard::BundleVersion::V2)
            .map_err(|e| anyhow::anyhow!("ensure universe shell: {e}"))?;
        if before_mode.as_deref() != Some("universe") {
            eprintln!(
                "[i] Serving universe UI (swapped static shell under {})",
                dashboard_dir.display()
            );
        }
    }

    let (graph, community) = if let Ok(graph) = session_ctx.load_graph() {
        let community = super::gql::load_community_context(&session_ctx, graph.backend());
        (Some(graph), community)
    } else {
        bail!(
            "graph snapshot not found under {}\n  \
             Run `rgctl discover .` first (serve does not index).",
            session_root.display()
        );
    };

    let semantic = load_semantic_index(&session_root);
    let analysis = match load_analysis_results(&session_root) {
        Ok(analysis) => analysis.map(Arc::new),
        Err(err) => {
            eprintln!("[warn] failed to load analysis results: {err}");
            None
        }
    };
    let state = Arc::new(AppState {
        repo: session_root.clone(),
        data_dir: dashboard_dir.with_file_name("dashboard-data"),
        dashboard_dir: dashboard_dir.clone(),
        graph: RwLock::new(graph),
        registry: QueryMacroRegistry::with_defaults(),
        semantic: RwLock::new(semantic.map(Arc::new)),
        analysis,
        community: RwLock::new(community),
        source_cache: RwLock::new(None),
        record_cache: RwLock::new(None),
        engine_cache: RwLock::new(None),
        dashboard_announced: AtomicBool::new(dashboard_dir.join("index.html").is_file()),
    });

    // Hold for the lifetime of serve so Drop releases `.rgctl/watch.lock` on exit.
    let _watch_lock = if args.watch {
        let lock =
            super::pipeline_status::try_acquire_watch_lock(&session_root).with_context(|| {
                format!("cannot start serve --watch for {}", session_root.display())
            })?;
        super::file_watch::spawn_repo_watcher(session_root.clone())
            .context("start source file watcher")?;
        eprintln!("[>] Source watch enabled (incremental update on save; exclusive lock held)");
        Some(lock)
    } else {
        None
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("create tokio runtime")?;

    let result = rt.block_on(run_server(ctx, args, state));
    drop(_watch_lock);
    result
}

fn load_semantic_index(repo: &Path) -> Option<SemanticIndex> {
    let path = semantic_index_path(repo);
    if !path.is_file() {
        return None;
    }
    match SemanticIndex::load(&path) {
        Ok(index) => Some(index),
        Err(err) => {
            eprintln!(
                "[warn] failed to load semantic index {}: {err}",
                path.display()
            );
            None
        }
    }
}

async fn run_server(ctx: &CliContext, args: HttpServeArgs, state: Arc<AppState>) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("invalid bind address {}:{}", args.host, args.port))?;

    let mut app = Router::new().route("/api/health", get(health));

    let mut rest = Router::new().route("/api/status", get(api_pipeline_status));
    rest = rest
        .route("/api/query", post(api_query))
        .route("/graphql", post(api_query))
        .route("/api/manifest", get(api_manifest))
        .route("/api/graph-payload", get(api_graph_payload))
        .route("/api/expand", get(api_expand))
        .route("/api/architecture", get(api_architecture))
        .route("/api/migration", get(api_migration))
        .route("/api/taint/index", get(api_taint_index))
        .route("/api/taint/detail", get(api_taint_detail))
        .route("/api/blast-index", get(api_blast_index))
        .route("/api/functions", get(api_functions))
        .route("/api/source", get(api_source))
        .route("/api/source/root", get(api_source_root))
        .route("/api/source/index", get(api_source_index))
        .route("/api/cfg", get(api_cfg))
        .route("/api/pdg", get(api_pdg))
        .route("/api/blast", get(api_blast))
        .route("/api/semantic/status", get(api_semantic_status))
        .route("/api/semantic/query", post(api_semantic_query));
    if !args.query_only {
        rest = rest.fallback(dashboard_fallback);
    }
    app = app.merge(rest.with_state(state.clone()));
    // The bundle's cfg/slice indexes are tens of MB of JSON. Localhost hides the transfer cost;
    // compressed responses make the same files usable over a remote/WSL-forwarded serve too.
    app = app.layer(CompressionLayer::new());

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind HTTP server on {addr}"))?;
    let bound = listener
        .local_addr()
        .context("read bound HTTP listen address")?;

    if !ctx.verbose {
        if args.query_only {
            eprintln!("[✓] Query API: http://{bound}/api/query");
            eprintln!("[✓] GraphQL alias: http://{bound}/graphql");
            eprintln!("[✓] Semantic API: http://{bound}/api/semantic/query");
        } else {
            eprintln!("[✓] Dashboard: http://{bound}/");
            eprintln!("[✓] Query API: http://{bound}/api/query");
            eprintln!("[✓] GraphQL alias: http://{bound}/graphql");
            eprintln!("[✓] Semantic search: http://{bound}/ (Search tab)");
        }
        eprintln!("[✓] Pipeline status: http://{bound}/api/status");
        eprintln!("[i] Press Ctrl+C to stop");
    } else {
        eprintln!("rgctl HTTP server listening on http://{bound}");
    }

    let public_url = format!("http://{bound}/");
    if args.open && !args.query_only {
        open_browser(&public_url)?;
    }

    let watch_state = state.clone();
    let watch_url = public_url.clone();
    tokio::spawn(async move {
        pipeline_watch_loop(watch_state, watch_url).await;
    });

    axum::serve(listener, app)
        .await
        .context("HTTP server exited with error")?;
    Ok(())
}

async fn pipeline_watch_loop(state: Arc<AppState>, public_url: String) {
    let mut last_digest: Option<String> = None;
    loop {
        tokio::time::sleep(Duration::from_millis(400)).await;
        reload_graph_if_needed(&state, &mut last_digest);
        let ready = state.dashboard_dir.join("index.html").is_file();
        if ready && !state.dashboard_announced.swap(true, Ordering::SeqCst) {
            eprintln!("[✓] Dashboard ready: {public_url}");
        }
    }
}

fn reload_graph_if_needed(state: &AppState, last_digest: &mut Option<String>) {
    let status = pipeline_status::read_status(&state.repo);
    if status.graph_digest == *last_digest {
        return;
    }
    let snapshot = rgctl_graph::snapshot::MmappedGraphSnapshot::default_path(&state.repo);
    if !snapshot.is_file() {
        return;
    }
    let Ok(graph) = CodeGraph::open_snapshot(&snapshot) else {
        return;
    };
    let ctx = CliContext::new(
        Some(state.repo.clone()),
        None,
        super::args::OutputFormat::Text,
        None,
        false,
    );
    let community = super::gql::load_community_context(&ctx, graph.backend());
    if let Ok(mut guard) = state.graph.write() {
        *guard = Some(graph);
    }
    if let Ok(mut guard) = state.community.write() {
        *guard = community;
    }
    if let Ok(mut guard) = state.semantic.write() {
        *guard = load_semantic_index(&state.repo).map(Arc::new);
    }
    *last_digest = status.graph_digest;
}

async fn api_pipeline_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut status = pipeline_status::read_status(&state.repo);
    pipeline_status::refresh_ready_flags(&mut status, &state.repo);
    Json(status)
}

/// Whether a URL belongs to the **client** router rather than to a file on disk.
///
/// v2 is the first dashboard with real paths: v1 was tabs, so its URL never changed and a reload always asked
/// for `/`. v2's routes are `/$roomId` with `?lens=…&focus=…` as search params, so a reload asks the server
/// for `/understand` — and a static file server has no such file. It answered `404` with an empty body, which
/// is a blank page: the shell never loaded, and the router that would have read the URL never ran.
///
/// The rule is deliberately narrow, because a fallback that answers *everything* is worse than a 404 — it
/// turns a missing asset into a MIME error and a missing endpoint into a 200:
///
///   * only `GET`/`HEAD`: a `POST` is a method error, not a page (`ServeDir` answers `405`, a truer answer
///     than `404` — the path exists, the method does not);
///   * never `/api/…` or `/graphql`, which answer for themselves;
///   * never a last segment containing `.` — `/assets/index-abc.js` and `/favicon.ico` are *file*
///     requests, and handing one the HTML shell is how a page renders blank a second way.
fn is_client_route(method: &Method, path: &str) -> bool {
    if method != Method::GET && method != Method::HEAD {
        return false;
    }
    if path.starts_with("/api/") || path == "/graphql" {
        return false;
    }
    !path.rsplit('/').next().unwrap_or("").contains('.')
}

/// `/understand/` → `/understand`, query preserved.
///
/// Not cosmetic. The shell references its assets relatively (`./assets/index-abc.js`, because the build uses
/// `base: "./"`), so a document served at `/understand/` resolves them against `/understand/assets/…`, where
/// nothing exists — the same blank page arriving by a different door. Canonicalising the URL is what keeps
/// relative asset resolution correct for a route that is one path segment deep.
fn without_trailing_slash(path: &str, query: Option<&str>) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed.len() == path.len() {
        return None;
    }
    Some(match query {
        Some(q) if !q.is_empty() => format!("{trimmed}?{q}"),
        _ => trimmed.to_string(),
    })
}

/// True when `path` names a bundle directory that holds *data*, not a page.
///
/// The export writes detail shards into directories named after the analysis — `taint/` today, `cfg/`,
/// `slice/` and `dataflow/` in bundles that carry them — and a room id can equal one of those names. Then
/// two rules fight: `ServeDir` 307s `/taint` to `/taint/` because a directory is there, and the SPA fallback
/// canonicalises `/taint/` straight back to `/taint`. The browser ends in a redirect loop
/// (`ERR_TOO_MANY_REDIRECTS`), so the room is unreachable from a URL — reload, bookmark, shared link —
/// even though in-app navigation works, because pushState never asks the server.
///
/// Reading the request as a client route first is the fix: a route outranks a data directory that has no
/// page of its own. The shards stay reachable, since `/taint/<uuid>.json` is not a client route (it has an
/// extension) and `ServeDir` serves it unchanged.
fn client_route_shadowed_by_data_dir(dir: &Path, path: &str) -> bool {
    let rel = path.trim_matches('/');
    if rel.is_empty() || rel.split('/').any(|seg| seg == ".." || seg == ".") {
        return false;
    }
    let candidate = dir.join(rel);
    candidate.is_dir() && !candidate.join("index.html").is_file()
}

/// The shell, for a URL the client router owns: `200`, because the route exists and the server simply has no
/// file for it. A 404 here would be a lie about the route, not a report about the filesystem.
async fn spa_index(dir: &Path) -> Response {
    match tokio::fs::read(dir.join("index.html")).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("cannot read dashboard index.html: {err}"),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod client_route_tests {
    use super::*;

    #[test]
    fn a_room_url_gets_the_shell_but_a_file_url_does_not() {
        assert!(is_client_route(&Method::GET, "/architecture"));
        assert!(is_client_route(&Method::GET, "/understand"));
        assert!(is_client_route(&Method::GET, "/"));
        // files, endpoint paths and non-GET methods are not the router's business
        assert!(!is_client_route(&Method::GET, "/assets/index-abc.js"));
        assert!(!is_client_route(&Method::GET, "/favicon.ico"));
        assert!(!is_client_route(&Method::GET, "/api/status"));
        assert!(!is_client_route(&Method::GET, "/graphql"));
        assert!(!is_client_route(&Method::POST, "/architecture"));
        assert!(!is_client_route(&Method::DELETE, "/review"));
    }

    #[test]
    fn a_trailing_slash_is_canonicalised_with_its_query() {
        assert_eq!(
            without_trailing_slash("/understand/", Some("lens=spine")),
            Some("/understand?lens=spine".to_string())
        );
        assert_eq!(
            without_trailing_slash("/understand/", None),
            Some("/understand".to_string())
        );
        // nothing to do for the root or for a path that is already canonical
        assert_eq!(without_trailing_slash("/", None), None);
        assert_eq!(without_trailing_slash("/understand", None), None);
    }

    /// The `/taint` loop: a data directory named after a room must not shadow that room's route.
    #[test]
    fn a_data_directory_does_not_shadow_a_client_route() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // The taint export writes uuid shards into `taint/`; the Taint room's route is `/taint`.
        std::fs::create_dir(root.join("taint")).unwrap();
        std::fs::write(root.join("taint/9b7d64a9.json"), b"{}").unwrap();

        assert!(client_route_shadowed_by_data_dir(root, "/taint"));
        assert!(client_route_shadowed_by_data_dir(root, "/taint/"));
        // A real page is served by `ServeDir`, so it is not shadowed.
        std::fs::create_dir(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/index.html"), b"<html></html>").unwrap();
        assert!(!client_route_shadowed_by_data_dir(root, "/docs"));
        // Absent paths, assets and traversal attempts are not this check's business.
        assert!(!client_route_shadowed_by_data_dir(root, "/architecture"));
        assert!(!client_route_shadowed_by_data_dir(
            root,
            "/taint/9b7d64a9.json"
        ));
        assert!(!client_route_shadowed_by_data_dir(root, "/../taint"));
        assert!(!client_route_shadowed_by_data_dir(root, "/"));
    }
}

async fn dashboard_fallback(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    let index = state.dashboard_dir.join("index.html");
    if index.is_file() {
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let query = req.uri().query().map(str::to_string);

        // Redirect before serving: a trailing slash would otherwise serve the shell at a depth where its own
        // relative asset URLs do not resolve.
        if is_client_route(&method, &path)
            && let Some(target) = without_trailing_slash(&path, query.as_deref())
        {
            return (StatusCode::PERMANENT_REDIRECT, [(header::LOCATION, target)]).into_response();
        }

        // Before `ServeDir` sees it: a bundle data directory must not shadow a client route, or the
        // directory redirect below and the canonicalisation above chase each other forever.
        if is_client_route(&method, &path)
            && client_route_shadowed_by_data_dir(&state.dashboard_dir, &path)
        {
            return spa_index(&state.dashboard_dir).await;
        }

        // API-first: static serving is the shell and its hashed assets only. Bundle data files are
        // API-backed now and must not remain reachable as static JSON/bin endpoints.
        let is_static_shell =
            path == "/index.html" || path == "/favicon.ico" || path.starts_with("/assets/");
        if !is_static_shell {
            if is_client_route(&method, &path) {
                return spa_index(&state.dashboard_dir).await;
            }
            return StatusCode::NOT_FOUND.into_response();
        }

        let svc = ServeDir::new(&state.dashboard_dir).append_index_html_on_directories(true);
        let response = svc.oneshot(req).await.into_response();
        if response.status() == StatusCode::NOT_FOUND && is_client_route(&method, &path) {
            return spa_index(&state.dashboard_dir).await;
        }
        return response;
    }
    let wants_json = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("application/json"));
    let mut status = pipeline_status::read_status(&state.repo);
    pipeline_status::refresh_ready_flags(&mut status, &state.repo);
    if status.message.is_none() {
        status.message = Some("Dashboard is being prepared".into());
    }
    if wants_json {
        (StatusCode::ACCEPTED, Json(status)).into_response()
    } else {
        (
            StatusCode::ACCEPTED,
            Html(preparing_html(
                status
                    .message
                    .as_deref()
                    .unwrap_or("Dashboard is being prepared"),
            )),
        )
            .into_response()
    }
}

fn preparing_html(message: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>rgctl</title></head>\
         <body><p>{message}</p><p>Poll <code>/api/status</code> for pipeline progress.</p></body></html>"
    )
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn api_query(
    State(state): State<Arc<AppState>>,
    Json(body): Json<QueryRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let graph_guard = state.graph.read().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "graph lock poisoned"})),
        )
    })?;
    let Some(graph) = graph_guard.as_ref() else {
        let mut status = pipeline_status::read_status(&state.repo);
        pipeline_status::refresh_ready_flags(&mut status, &state.repo);
        status.message = Some("Graph is being prepared".into());
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::to_value(&status).unwrap_or_else(|_| {
                serde_json::json!({"schema_version": PIPELINE_STATUS_SCHEMA_VERSION, "message": "graph not ready"})
            })),
        ));
    };
    let community = state.community.read().ok();
    let _community_ref = community.as_ref().and_then(|c| c.as_ref());

    let args = QueryArgs {
        query: body.query.clone(),
        macro_name: body.r#macro.clone(),
        explain: body.explain,
        limit: body.limit,
    };
    match run_query(graph, &state.repo, &args) {
        Ok(value) => Ok(Json(value)),
        Err(err) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": err.to_string()})),
        )),
    }
}

fn parse_record_index(index: &[u8], data_path: PathBuf) -> Result<RecordPack, String> {
    if index.len() < 16 || &index[..4] != b"RBCI" {
        return Err("invalid record index".into());
    }
    let version = u32::from_le_bytes(index[4..8].try_into().map_err(|_| "index version")?);
    if version != 1 {
        return Err(format!("unsupported record index version {version}"));
    }
    let count = u64::from_le_bytes(index[8..16].try_into().map_err(|_| "index count")?) as usize;
    let entry_size = 16 + 8 + 4;
    if index.len() < 16 + count * entry_size {
        return Err("record index truncated".into());
    }
    let mut entries = HashMap::with_capacity(count);
    for i in 0..count {
        let pos = 16 + i * entry_size;
        let id = Uuid::from_slice(&index[pos..pos + 16]).map_err(|e| e.to_string())?;
        let offset =
            u64::from_le_bytes(index[pos + 16..pos + 24].try_into().map_err(|_| "offset")?);
        let length =
            u32::from_le_bytes(index[pos + 24..pos + 28].try_into().map_err(|_| "length")?);
        entries.insert(id, (offset, length));
    }
    Ok(RecordPack { data_path, entries })
}

async fn load_record_packs(state: &AppState) -> Result<Arc<RecordPacks>, (StatusCode, String)> {
    if let Ok(guard) = state.record_cache.read()
        && let Some(cache) = guard.as_ref()
    {
        return Ok(cache.clone());
    }
    let dash = state.data_dir.clone();
    let packs = tokio::task::spawn_blocking(move || {
        let load = |index: &str, data: &str| -> Result<Option<RecordPack>, String> {
            let index_path = dash.join(index);
            let data_path = dash.join(data);
            if !index_path.is_file() || !data_path.is_file() {
                return Ok(None);
            }
            let bytes = std::fs::read(&index_path)
                .map_err(|e| format!("read {}: {e}", index_path.display()))?;
            Ok(Some(parse_record_index(&bytes, data_path)?))
        };
        Ok::<_, String>(RecordPacks {
            cfg: load("cfg_pdg.record_index.bin", "cfg_pdg.record_data.bin")?,
            slice: load("slice.record_index.bin", "slice.record_data.bin")?,
        })
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let packs = Arc::new(packs);
    if let Ok(mut guard) = state.record_cache.write() {
        *guard = Some(packs.clone());
    }
    Ok(packs)
}

fn read_record(pack: &RecordPack, id: Uuid) -> Result<Vec<u8>, (StatusCode, String)> {
    let (offset, length) = *pack
        .entries
        .get(&id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "record not found".into()))?;
    let mut file = std::fs::File::open(&pack.data_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut bytes = vec![0u8; length as usize];
    file.read_exact(&mut bytes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(bytes)
}

async fn api_cfg(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RecordQuery>,
) -> Result<Response, (StatusCode, String)> {
    let id = Uuid::parse_str(params.id.trim())
        .map_err(|_| (StatusCode::BAD_REQUEST, "id must be a UUID".into()))?;
    let packs = load_record_packs(&state).await?;
    if let Some(pack) = packs.cfg.clone() {
        let bytes = tokio::task::spawn_blocking(move || read_record(&pack, id))
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
        return Ok(([(header::CONTENT_TYPE, "application/json")], bytes).into_response());
    }
    let path = state.data_dir.join("cfg").join(format!("{id}.json"));
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(([(header::CONTENT_TYPE, "application/json")], bytes).into_response())
}

async fn api_pdg(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RecordQuery>,
) -> Result<Response, (StatusCode, String)> {
    let id = Uuid::parse_str(params.id.trim())
        .map_err(|_| (StatusCode::BAD_REQUEST, "id must be a UUID".into()))?;
    let packs = load_record_packs(&state).await?;
    if let Some(pack) = packs.slice.clone() {
        let bytes = tokio::task::spawn_blocking(move || read_record(&pack, id))
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
        return Ok(([(header::CONTENT_TYPE, "application/json")], bytes).into_response());
    }
    let path = state.data_dir.join("slice").join(format!("{id}.json"));
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(([(header::CONTENT_TYPE, "application/json")], bytes).into_response())
}

async fn api_blast(
    State(state): State<Arc<AppState>>,
    Query(params): Query<BlastQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let id = Uuid::parse_str(params.id.trim())
        .map_err(|_| (StatusCode::BAD_REQUEST, "id must be a UUID".into()))?;
    let graph = {
        let guard = state.graph.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "graph lock poisoned".into(),
            )
        })?;
        guard
            .clone()
            .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "graph not ready".into()))?
    };
    let repo = state.repo.clone();
    let depth = params.depth.clamp(1, 10);
    let payload = tokio::task::spawn_blocking(move || {
        let value = run_impact(
            &graph,
            &repo,
            &ImpactArgs {
                symbol: id.to_string(),
                depth: Some(depth),
                class: None,
                file: None,
            },
        )
        .map_err(|e| e.to_string())?;

        // The service gives us direct and transitive IDs; compute hop depth for the rings here.
        let view = rgctl_analysis::PetGraphView::from_backend(graph.backend())
            .map_err(|e| e.to_string())?;
        let mut hop = std::collections::HashMap::<Uuid, usize>::new();
        if let Some(seed_idx) = view.get_index(id) {
            let mut queue = std::collections::VecDeque::from([(seed_idx, 0usize)]);
            hop.insert(id, 0);
            while let Some((idx, d)) = queue.pop_front() {
                if d >= depth {
                    continue;
                }
                for next in view.incoming_filtered(idx, &[EdgeType::Calls]) {
                    if let Some(uuid) = view.get_uuid(next)
                        && !hop.contains_key(&uuid)
                    {
                        hop.insert(uuid, d + 1);
                        queue.push_back((next, d + 1));
                    }
                }
            }
        }

        let seed_name = value["target"]["symbol"].as_str().unwrap_or("").to_string();
        let score = value["metrics"]["score"].as_f64().unwrap_or(0.0);
        let direct = value["metrics"]["direct_callers_count"]
            .as_u64()
            .unwrap_or(0) as u32;
        let zone = value["metrics"]["impact_zone_size"].as_u64().unwrap_or(0) as u32;
        let mut callers = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (key, depth_one) in [("direct_callers", true), ("impact_zone", false)] {
            if let Some(rows) = value["topology"][key].as_array() {
                for row in rows {
                    let Some(raw) = row["id"].as_str() else {
                        continue;
                    };
                    let Ok(uuid) = Uuid::parse_str(raw) else {
                        continue;
                    };
                    if !seen.insert(uuid) {
                        continue;
                    }
                    let name = row["symbol"]
                        .as_str()
                        .or_else(|| row["fqn"].as_str())
                        .unwrap_or("")
                        .to_string();
                    let d = if depth_one {
                        1
                    } else {
                        *hop.get(&uuid).unwrap_or(&0)
                    };
                    if d == 0 && !depth_one {
                        continue;
                    }
                    callers.push(serde_json::json!({
                        "index": 0,
                        "name": name,
                        "depth": d,
                        "node_type": 0,
                        "node_type_name": "caller",
                    }));
                }
            }
        }
        Ok::<_, String>(serde_json::json!({
            "seed_index": 0,
            "seed_name": seed_name,
            "depth_limit": depth,
            "direct_caller_count": direct,
            "impact_zone_count": zone,
            "score": score,
            "callers": callers,
        }))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(payload))
}

async fn load_source_lookup(state: &AppState) -> Result<Arc<SourceLookup>, (StatusCode, String)> {
    if let Ok(guard) = state.source_cache.read()
        && let Some(cache) = guard.as_ref()
    {
        return Ok(cache.clone());
    }
    let dashboard = state.data_dir.clone();
    let lookup = tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(dashboard.join("sources_index.json"))
            .map_err(|e| format!("read sources_index.json: {e}"))?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("parse sources_index.json: {e}"))?;
        let files = value
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| {
                let path = row.get("file_path")?.as_str()?.to_string();
                let id = row.get("source_id")?.as_str()?.to_string();
                Some((path, id))
            })
            .collect::<HashMap<_, _>>();
        let mut paths = files.keys().map(|p| {
            let mut parts = p.split('/').collect::<Vec<_>>();
            parts.pop();
            parts
        });
        let mut common = paths.next().unwrap_or_default();
        for parts in paths {
            let mut i = 0;
            while i < common.len() && i < parts.len() && common[i] == parts[i] {
                i += 1;
            }
            common.truncate(i);
            if common.is_empty() {
                break;
            }
        }
        let root = common.join("/") + if common.is_empty() { "" } else { "/" };
        Ok::<_, String>(SourceLookup { files, root })
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::NOT_FOUND, e))?;
    let lookup = Arc::new(lookup);
    if let Ok(mut guard) = state.source_cache.write() {
        *guard = Some(lookup.clone());
    }
    Ok(lookup)
}

/// Repo-relative source root, computed once from the source catalog.
async fn api_source_index(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    Ok(Json(dashboard_json(&state, "sources_index.json").await?))
}

async fn api_source_root(
    State(state): State<Arc<AppState>>,
) -> Result<String, (StatusCode, String)> {
    let lookup = load_source_lookup(&state).await?;
    Ok(lookup.root.clone())
}

/// Source text by repo path, read from the dashboard's exported source files.
async fn api_source(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SourceQuery>,
) -> Result<Response, (StatusCode, String)> {
    let path = params
        .path
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "path is required".into()))?;
    let lookup = load_source_lookup(&state).await?;
    let id = lookup
        .files
        .get(&path)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "source path not found".into()))?
        .clone();
    let file = state.data_dir.join("sources").join(format!("{id}.txt"));
    let text = tokio::task::spawn_blocking(move || std::fs::read_to_string(file))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response())
}

/// Compact function lookup/search for the dashboard shell.
///
/// The browser must not fetch/parse `cfg_index.json` + `slice_index.json` (67 MB each on metasfresh)
/// just to resolve a Focus or search the palette. The server already has the graph; this returns only
/// the rows the shell needs. Static bundles still have the JSON index fallback.
#[derive(Debug, Deserialize)]
struct ExpandQuery {
    indices: String,
    #[serde(default)]
    type_mask: u32,
}

async fn load_engine(
    state: &AppState,
) -> Result<Arc<rgctl_wasm::EngineContext>, (StatusCode, String)> {
    if let Ok(guard) = state.engine_cache.read()
        && let Some(engine) = guard.as_ref()
    {
        return Ok(engine.clone());
    }
    let path = state.data_dir.join("graph_payload.bin");
    let engine = tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("read graph payload: {e}"))?;
        rgctl_wasm::EngineContext::from_bytes(&bytes).map_err(|e| format!("{e:?}"))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let engine = Arc::new(engine);
    if let Ok(mut guard) = state.engine_cache.write() {
        *guard = Some(engine.clone());
    }
    Ok(engine)
}

async fn api_expand(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExpandQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let indices = params
        .indices
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "indices must be comma-separated u32".into(),
            )
        })?;
    let engine = load_engine(&state).await?;
    let json = tokio::task::spawn_blocking(move || {
        engine
            .expand_indices(&indices, params.type_mask)
            .map_err(|e| format!("{e:?}"))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let value: Value = serde_json::from_str(&json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(value))
}

async fn api_graph_payload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    let path = state.data_dir.join("graph_payload.bin");
    if !path.is_file() {
        return Err((StatusCode::NOT_FOUND, "graph payload not present".into()));
    }
    let mut request = Request::builder()
        .uri("/")
        .body(Body::empty())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    *request.headers_mut() = headers;
    let response = ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(response.map(Body::new))
}

async fn dashboard_json(
    state: &AppState,
    name: &'static str,
) -> Result<Value, (StatusCode, String)> {
    let path = state.data_dir.join(name);
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::NOT_FOUND, e))
}

async fn api_manifest(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    Ok(Json(dashboard_json(&state, "manifest.json").await?))
}

async fn api_architecture(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let metagraph = dashboard_json(&state, "metagraph.json").await?;
    let communities = dashboard_json(&state, "communities.json").await.ok();
    let migration = dashboard_json(&state, "migration_graph.json").await.ok();
    Ok(Json(serde_json::json!({
        "metagraph": metagraph,
        "communities": communities,
        "migration_graph": migration,
    })))
}

async fn api_migration(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    Ok(Json(dashboard_json(&state, "migration_graph.json").await?))
}

async fn api_taint_index(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    Ok(Json(dashboard_json(&state, "taint_index.json").await?))
}

async fn api_blast_index(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    Ok(Json(dashboard_json(&state, "blast_index.json").await?))
}

async fn api_taint_detail(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RecordQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let id = Uuid::parse_str(params.id.trim())
        .map_err(|_| (StatusCode::BAD_REQUEST, "id must be a UUID".into()))?;
    let path = state.data_dir.join("taint").join(format!("{id}.json"));
    let value = tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("read taint detail: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("parse taint detail: {e}"))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::NOT_FOUND, e))?;
    Ok(Json(value))
}

async fn api_functions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FunctionQuery>,
) -> Result<Json<FunctionsResponse>, (StatusCode, String)> {
    let graph = {
        let guard = state.graph.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "graph lock poisoned".into(),
            )
        })?;
        guard
            .clone()
            .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "graph not ready".into()))?
    };

    if let Some(raw) = params.id.as_deref() {
        let id = Uuid::parse_str(raw.trim())
            .map_err(|_| (StatusCode::BAD_REQUEST, "function id must be a UUID".into()))?;
        let node = graph
            .backend()
            .get_node(id)
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "function not found".into()))?;
        if node.node_type != NodeType::Function {
            return Err((StatusCode::NOT_FOUND, "node is not a function".into()));
        }
        return Ok(Json(FunctionsResponse {
            functions: vec![FunctionRow {
                function_id: node.id.to_string(),
                name: node.name.to_string(),
                file_path: node.file_path.as_ref().map(ToString::to_string),
            }],
            total: 1,
        }));
    }

    let query = params.q.unwrap_or_default().trim().to_ascii_lowercase();
    let limit = params.limit.unwrap_or(200).clamp(1, 500);
    let response = tokio::task::spawn_blocking(move || {
        let mut functions = Vec::with_capacity(limit.min(256));
        let mut total = 0usize;
        let _ = graph.backend().for_each_node(|node| {
            if node.node_type != NodeType::Function {
                return;
            }
            let name = node.name.to_string();
            let file_path = node.file_path.as_ref().map(ToString::to_string);
            if !query.is_empty() {
                let name_match = name.to_ascii_lowercase().contains(&query);
                let path_match = file_path
                    .as_deref()
                    .is_some_and(|p| p.to_ascii_lowercase().contains(&query));
                if !name_match && !path_match {
                    return;
                }
            }
            total += 1;
            if functions.len() < limit {
                functions.push(FunctionRow {
                    function_id: node.id.to_string(),
                    name,
                    file_path,
                });
            }
        });
        functions.sort_by(|a, b| a.name.cmp(&b.name));
        FunctionsResponse { functions, total }
    })
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, format!("{err}")))?;

    Ok(Json(response))
}

async fn api_semantic_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Use the index the server already holds; `semantic_status` reloads the 100+ MB index from disk.
    let cached = state
        .semantic
        .read()
        .ok()
        .and_then(|guard| guard.as_ref().cloned())
        .map(|index| semantic_status_for_index(&index));
    let status = cached.unwrap_or_else(|| semantic_status(&state.repo));
    Ok(Json(serde_json::to_value(status).map_err(internal_error)?))
}

async fn api_semantic_query(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SemanticQueryRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let query = body.query.trim();
    if query.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "`query` must not be empty".into()));
    }

    let index = {
        let guard = state.semantic.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "graph lock poisoned".into(),
            )
        })?;
        guard.clone().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "semantic index not available — wait for the full pipeline or run `rgctl semantic index`".into(),
            )
        })?
    };

    let expand = parse_expand_mode(body.expand.as_deref())?;

    let scope = match body
        .scope
        .as_deref()
        .unwrap_or("function")
        .to_ascii_lowercase()
        .as_str()
    {
        "function" | "functions" => super::semantic::CliSemanticScope::Function,
        "community" | "communities" => super::semantic::CliSemanticScope::Community,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown scope `{other}` (use function or community)"),
            ));
        }
    };

    let graph = {
        let guard = state.graph.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "graph lock poisoned".into(),
            )
        })?;
        guard
            .clone()
            .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "graph not ready".into()))?
    };
    let repo = state.repo.clone();

    let index = Arc::clone(&index);
    let analysis = state.analysis.clone();
    let args = SemanticQueryArgs {
        query: query.to_string(),
        limit: body.limit.clamp(1, 100),
        expand,
        expand_depth: body.expand_depth.max(1),
        model: None,
        tokenizer: None,
        fusion: body.fusion,
        candidate_pool: body.candidate_pool.max(body.limit),
        keyword_and: body.keyword_and,
        scope,
    };

    let response = tokio::task::spawn_blocking(move || {
        execute_semantic_query_with_analysis(&repo, &graph, &index, analysis.as_deref(), &args)
    })
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, format!("{err}")))?
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(query_response_to_json(&response)))
}

fn parse_expand_mode(
    raw: Option<&str>,
) -> Result<Option<super::semantic::CliExpandMode>, (StatusCode, String)> {
    let Some(value) = raw else {
        return Ok(None);
    };
    let mode = match value.to_ascii_lowercase().as_str() {
        "neighbors" => super::semantic::CliExpandMode::Neighbors,
        "blast" => super::semantic::CliExpandMode::Blast,
        "gql" => super::semantic::CliExpandMode::Gql,
        "all" => super::semantic::CliExpandMode::All,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown expand mode `{other}` (use neighbors, blast, gql, or all)"),
            ));
        }
    };
    Ok(Some(mode))
}

fn internal_error(err: serde_json::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .context("open browser (macOS)")?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .context("open browser (Linux)")?;
    }
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .context("open browser (Windows)")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_request_deserializes_macro() {
        let body: QueryRequest = serde_json::from_str(r#"{"macro":"all_functions"}"#).unwrap();
        assert_eq!(body.r#macro.as_deref(), Some("all_functions"));
        assert!(body.query.is_none());
    }

    #[test]
    fn semantic_query_request_defaults() {
        let body: SemanticQueryRequest =
            serde_json::from_str(r#"{"query":"shopping cart"}"#).unwrap();
        assert_eq!(body.query, "shopping cart");
        assert!(body.fusion);
        assert_eq!(body.limit, 20);
        assert_eq!(body.candidate_pool, 256);
    }
}
