//! HTTP server for the analysis dashboard and GQL query API (`rgctl serve`).

use super::context::CliContext;
use super::discover::resolve_session_root;
use super::pipeline_status::{self, PIPELINE_STATUS_SCHEMA_VERSION};
use super::semantic::SemanticQueryArgs;
use super::semantic_api::{execute_semantic_query, semantic_index_path, semantic_status};
use super::semantic_output::query_response_to_json;
use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Method, Request, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use rgctl_analysis::{CommunityQueryContext, SemanticIndex};
use rgctl_dashboard::default_dashboard_path;
use rgctl_gql::QueryMacroRegistry;
use rgctl_graph::CodeGraph;
use rgctl_service::command::{QueryArgs, SearchArgs, SearchScope};
use rgctl_service::query::run_query;
use rgctl_service::search::run_search;
use serde::Deserialize;
use serde_json::Value;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tower::ServiceExt;
use tower_http::services::ServeDir;

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
    graph: RwLock<Option<CodeGraph>>,
    #[allow(dead_code)]
    registry: QueryMacroRegistry,
    semantic: RwLock<Option<Arc<SemanticIndex>>>,
    community: RwLock<Option<CommunityQueryContext>>,
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
            .and_then(|v| {
                v.get("mode")
                    .and_then(|m| m.as_str())
                    .map(str::to_owned)
            });
        rgctl_dashboard::ensure_static_shell(
            &dashboard_dir,
            rgctl_dashboard::BundleVersion::V2,
        )
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
    let state = Arc::new(AppState {
        repo: session_root.clone(),
        dashboard_dir: dashboard_dir.clone(),
        graph: RwLock::new(graph),
        registry: QueryMacroRegistry::with_defaults(),
        semantic: RwLock::new(semantic.map(Arc::new)),
        community: RwLock::new(community),
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
        .route("/api/semantic/status", get(api_semantic_status))
        .route("/api/semantic/query", post(api_semantic_query));
    if !args.query_only {
        rest = rest.fallback(dashboard_fallback);
    }
    app = app.merge(rest.with_state(state.clone()));

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

async fn api_semantic_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let status = semantic_status(&state.repo);
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

    if expand.is_none() {
        let _ = index;
        let search_scope = match scope {
            super::semantic::CliSemanticScope::Function => SearchScope::Function,
            super::semantic::CliSemanticScope::Community => SearchScope::Community,
            super::semantic::CliSemanticScope::Docs => SearchScope::Docs,
            super::semantic::CliSemanticScope::All => SearchScope::All,
        };
        let text = query.to_string();
        let limit = body.limit.clamp(1, 100);
        let value = tokio::task::spawn_blocking(move || {
            run_search(
                &graph,
                &repo,
                &SearchArgs {
                    text,
                    scope: search_scope,
                    limit: Some(limit),
                },
            )
        })
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, format!("{err}")))?
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        return Ok(Json(value));
    }

    let index = Arc::clone(&index);
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

    let response =
        tokio::task::spawn_blocking(move || execute_semantic_query(&repo, &graph, &index, &args))
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
