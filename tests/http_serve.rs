//! Integration tests for `rgctl serve` HTTP mode.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn pick_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn rgctl_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_rgctl")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl")
        })
}

/// A repo that already has a dashboard bundle, or `None`.
///
/// This used to be the absolute path of one developer's checkout, so on every other machine the only
/// dashboard test here skipped while reporting success — the same defect `0211f97` fixed in
/// `test-dashboard-golden.sh`. Order: an explicit override, then this workspace (kept out of git), then any
/// indexed corpus under `example/`.
fn repo_with_dashboard() -> Option<std::path::PathBuf> {
    let has_bundle = |dir: &std::path::Path| dir.join(".rgctl/dashboard/index.html").is_file();

    if let Some(explicit) = std::env::var_os("RGCTL_HTTP_SERVE_REPO") {
        let dir = std::path::PathBuf::from(explicit);
        if has_bundle(&dir) {
            return Some(dir);
        }
    }
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if has_bundle(&workspace) {
        return Some(workspace);
    }
    if let Ok(entries) = std::fs::read_dir(workspace.join("example")) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if has_bundle(&dir) {
                return Some(dir);
            }
        }
    }
    None
}

struct ServerGuard {
    child: Child,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for_health(base: &str, timeout: Duration) -> bool {
    let client = reqwest::blocking::Client::new();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(resp) = client.get(format!("{base}/api/health")).send() {
            if resp.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
fn http_serve_serves_dashboard_and_query_api() {
    let Some(repo) = repo_with_dashboard() else {
        eprintln!("skip: no repo with a dashboard bundle (set RGCTL_HTTP_SERVE_REPO)");
        return;
    };
    let bin = rgctl_bin();
    assert!(bin.is_file(), "missing rgctl binary at {}", bin.display());

    let port = pick_port();
    let base = format!("http://127.0.0.1:{port}");

    let child = Command::new(&bin)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn rgctl serve");

    let _guard = ServerGuard { child };
    assert!(
        wait_for_health(&base, Duration::from_secs(15)),
        "server did not become healthy"
    );

    let client = reqwest::blocking::Client::new();

    let dashboard = client
        .get(format!("{base}/"))
        .send()
        .expect("GET /")
        .text()
        .expect("dashboard body");
    assert!(
        dashboard.contains("rgctl")
            || dashboard.contains("rb-app")
            || dashboard.contains("<!doctype html")
    );

    let query = client
        .post(format!("{base}/api/query"))
        .json(&serde_json::json!({ "macro": "all_functions" }))
        .send()
        .expect("POST /api/query")
        .json::<serde_json::Value>()
        .expect("query json");
    assert!(query.get("rows").is_some());
    assert!(query.get("count").is_some());

    let gql_alias = client
        .post(format!("{base}/graphql"))
        .json(&serde_json::json!({
            "query": "MATCH (n:Function) RETURN n LIMIT 1"
        }))
        .send()
        .expect("POST /graphql")
        .status();
    assert!(gql_alias.is_success());

    if let Ok(entries) = std::fs::read_dir(repo.join(".rgctl/dashboard/assets")) {
        if let Some(wasm) = entries.flatten().find(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext == "wasm")
        }) {
            let resp = client
                .get(format!(
                    "{base}/assets/{}",
                    wasm.file_name().to_string_lossy()
                ))
                .send()
                .expect("GET wasm asset");
            assert!(resp.status().is_success());
            let mime = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            assert!(
                mime.contains("application/wasm"),
                "expected application/wasm, got {mime}"
            );
        }
    }
}

/// A v2 URL is a *client* route: the server has no file for `/architecture`, and answering `404` produced a
/// blank page because the shell that would have read the URL never loaded. The fallback is deliberately
/// narrow, and each of the narrowings is asserted here — a fallback that answers everything hides missing
/// assets and turns an unknown endpoint into a 200.
#[test]
fn spa_fallback_serves_the_shell_for_client_routes_only() {
    let Some(repo) = repo_with_dashboard() else {
        eprintln!("skip: no repo with a dashboard bundle (set RGCTL_HTTP_SERVE_REPO)");
        return;
    };
    let bin = rgctl_bin();
    assert!(bin.is_file(), "missing rgctl binary at {}", bin.display());

    let port = pick_port();
    let base = format!("http://127.0.0.1:{port}");
    let child = Command::new(&bin)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--no-pipeline",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn rgctl serve");
    let _guard = ServerGuard { child };
    assert!(
        wait_for_health(&base, Duration::from_secs(20)),
        "server did not become healthy"
    );

    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");

    // the room URL, which no file answers
    let room = client
        .get(format!("{base}/architecture"))
        .send()
        .expect("GET /architecture");
    assert_eq!(
        room.status(),
        200,
        "a client route must serve the shell, not a 404"
    );
    let content_type = room
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/html"),
        "got content-type {content_type:?}"
    );
    // Vite emits `<!DOCTYPE html>` in caps, and the shell mounts into a `#root` div — accept either marker
    let shell_body = room.text().unwrap_or_default().to_lowercase();
    assert!(
        shell_body.contains("<!doctype html") || shell_body.contains("id=\"root\""),
        "the shell, not an empty body"
    );

    // the same route with a trailing slash resolves its assets against the wrong directory, so it redirects
    let slash = client
        .get(format!("{base}/architecture/"))
        .send()
        .expect("GET /architecture/");
    assert_eq!(
        slash.status(),
        308,
        "a trailing slash must be canonicalised"
    );
    assert_eq!(
        slash
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/architecture")
    );

    // a file request is not a client route: this is how a page renders blank a second way
    let missing = client
        .get(format!("{base}/assets/does-not-exist.js"))
        .send()
        .expect("GET missing asset");
    assert_eq!(missing.status(), 404, "a missing asset must stay a 404");

    // and the API answers for itself
    let api = client
        .get(format!("{base}/api/status"))
        .send()
        .expect("GET /api/status");
    assert!(
        api.status().is_success(),
        "api/status should answer for itself"
    );
}

/// A bundle data directory named after a room must not hijack that room's URL.
///
/// The taint export writes `.rgctl/dashboard/taint/` (uuid shards), and the Taint room's client route is
/// `/taint`. `ServeDir` answers a directory with `307 /taint/`, and the SPA fallback canonicalises `/taint/`
/// back to `/taint` — a loop the browser reports as `ERR_TOO_MANY_REDIRECTS`. Clicking the room in the nav
/// worked (pushState never asks the server), so the defect hid exactly where the design makes a promise:
/// reload, bookmark, shared link. The shards themselves must keep being served, which is the other half of
/// this test — a fix that answered the shell for everything under `/taint` would trade one bug for another.
#[test]
fn a_bundle_data_directory_does_not_hijack_a_room_url() {
    let Some(repo) = repo_with_dashboard() else {
        eprintln!("skip: no repo with a dashboard bundle (set RGCTL_HTTP_SERVE_REPO)");
        return;
    };
    let bin = rgctl_bin();
    assert!(bin.is_file(), "missing rgctl binary at {}", bin.display());

    // A bundle whose taint export found nothing still writes the directory, so create it for symmetry.
    let detail_dir = repo.join(".rgctl/dashboard/taint");
    std::fs::create_dir_all(&detail_dir).expect("create taint detail dir");
    let shard = detail_dir.join("http-serve-probe.json");
    std::fs::write(&shard, b"{\"probe\":true}").expect("write probe shard");

    let port = pick_port();
    let base = format!("http://127.0.0.1:{port}");
    let child = Command::new(&bin)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn rgctl serve");
    let _guard = ServerGuard { child };
    assert!(
        wait_for_health(&base, Duration::from_secs(20)),
        "server did not become healthy"
    );

    // No redirects followed: the bug was a redirect *chain*, so a client that followed it would pass.
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");

    let room = client
        .get(format!("{base}/taint"))
        .send()
        .expect("GET /taint");
    assert_eq!(
        room.status(),
        200,
        "/taint is a client route even though a `taint/` directory exists"
    );
    let body = room.text().unwrap_or_default().to_lowercase();
    assert!(
        body.contains("<!doctype html") || body.contains("id=\"root\""),
        "the shell, not a directory redirect or an empty body"
    );

    // The shards behind the room are still files, and still served.
    let served = client
        .get(format!("{base}/taint/http-serve-probe.json"))
        .send()
        .expect("GET taint shard");
    assert_eq!(served.status(), 200, "a shard is a file, not a route");
    assert!(
        served.text().unwrap_or_default().contains("probe"),
        "the shard body, not the shell"
    );

    std::fs::remove_file(&shard).ok();
}
