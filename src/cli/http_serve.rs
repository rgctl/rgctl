//! HTTP server for the analysis dashboard and GQL query API (`rbuilder serve`).

use super::context::CliContext;
use super::gql_output::gql_result_to_json;
use anyhow::{bail, Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use rbuilder_dashboard::default_dashboard_path;
use rbuilder_gql::{execute, execute_explain, execute_macro, QueryMacroRegistry};
use rbuilder_graph::CodeGraph;
use serde::Deserialize;
use serde_json::Value;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower_http::services::ServeDir;

/// Options for the unified HTTP `serve` command.
pub struct HttpServeArgs {
    pub host: String,
    pub port: u16,
    pub dashboard_dir: Option<PathBuf>,
    pub open: bool,
    pub query_only: bool,
    pub dashboard_only: bool,
}

struct AppState {
    graph: Mutex<CodeGraph>,
    registry: QueryMacroRegistry,
}

#[derive(Debug, Deserialize)]
struct QueryRequest {
    query: Option<String>,
    #[serde(default)]
    explain: bool,
    #[serde(default)]
    r#macro: Option<String>,
}

/// Start the HTTP server (dashboard static files + `/api/query` and `/graphql`).
pub fn serve(ctx: &CliContext, args: HttpServeArgs) -> Result<()> {
    if args.query_only && args.dashboard_only {
        bail!("--query-only and --dashboard-only cannot be used together");
    }

    let dashboard_dir = args
        .dashboard_dir
        .clone()
        .unwrap_or_else(|| default_dashboard_path(&ctx.repo));

    if !args.query_only {
        let index = dashboard_dir.join("index.html");
        if !index.is_file() {
            bail!(
                "dashboard not found at {} (run `rbuilder discover` first)",
                dashboard_dir.display()
            );
        }
    }

    let state = if args.dashboard_only {
        None
    } else {
        let graph = ctx
            .load_graph()
            .context("load graph for query API (run `rbuilder discover` first)")?;
        Some(Arc::new(AppState {
            graph: Mutex::new(graph),
            registry: QueryMacroRegistry::with_defaults(),
        }))
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("create tokio runtime")?;

    rt.block_on(run_server(ctx, args, dashboard_dir, state))
}

async fn run_server(
    ctx: &CliContext,
    args: HttpServeArgs,
    dashboard_dir: PathBuf,
    state: Option<Arc<AppState>>,
) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("invalid bind address {}:{}", args.host, args.port))?;

    let mut app = Router::new().route("/api/health", get(health));

    if let Some(state) = state {
        let query = Router::new()
            .route("/api/query", post(api_query))
            .route("/graphql", post(api_query))
            .with_state(state);
        app = app.merge(query);
    }

    if !args.query_only {
        let static_files = ServeDir::new(dashboard_dir).append_index_html_on_directories(true);
        app = app.fallback_service(static_files);
    }

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
        } else if args.dashboard_only {
            eprintln!("[✓] Dashboard: http://{bound}/");
        } else {
            eprintln!("[✓] Dashboard: http://{bound}/");
            eprintln!("[✓] Query API: http://{bound}/api/query");
            eprintln!("[✓] GraphQL alias: http://{bound}/graphql");
        }
        eprintln!("[i] Press Ctrl+C to stop");
    } else {
        eprintln!("rbuilder HTTP server listening on http://{bound}");
    }

    if args.open && !args.query_only {
        open_browser(&format!("http://{bound}/"))?;
    }

    axum::serve(listener, app)
        .await
        .context("HTTP server exited with error")?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn api_query(
    State(state): State<Arc<AppState>>,
    Json(body): Json<QueryRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let graph = state
        .graph
        .lock()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "graph lock poisoned".into()))?;
    let backend = graph.backend();

    let result = if let Some(name) = body.r#macro {
        execute_macro(backend, &state.registry, &name)
    } else if let Some(query) = body.query.as_deref() {
        if body.explain {
            execute_explain(backend, query)
        } else {
            execute(backend, query)
        }
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            "request must include `query` or `macro`".into(),
        ));
    }
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    Ok(Json(gql_result_to_json(&result, body.explain)))
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
}
