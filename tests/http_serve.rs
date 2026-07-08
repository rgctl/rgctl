//! Integration tests for `rbuilder serve` HTTP mode.

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

fn rbuilder_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_rbuilder")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rbuilder")
        })
}

fn repo_with_dashboard() -> Option<std::path::PathBuf> {
    let repo = std::path::PathBuf::from("/Users/sshaaf/git/java/gbuilder");
    if repo.join(".rbuilder/dashboard/index.html").is_file() {
        return Some(repo);
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
        eprintln!("skip: gbuilder dashboard bundle not present");
        return;
    };
    let bin = rbuilder_bin();
    assert!(bin.is_file(), "missing rbuilder binary at {}", bin.display());

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
        .expect("spawn rbuilder serve");

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
    assert!(dashboard.contains("rBuilder") || dashboard.contains("rb-app") || dashboard.contains("<!doctype html"));

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

    if let Ok(entries) = std::fs::read_dir(repo.join(".rbuilder/dashboard/assets")) {
        if let Some(wasm) = entries.flatten().find(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext == "wasm")
        }) {
            let resp = client
                .get(format!("{base}/assets/{}", wasm.file_name().to_string_lossy()))
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
