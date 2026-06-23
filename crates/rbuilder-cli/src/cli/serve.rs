//! Web server CLI command

use rbuilder_error::Result;
use rbuilder_mcp::api::server;
use rbuilder_mcp::api::state::AppState;
use std::path::{Path, PathBuf};

/// Run `rbuilder serve`.
pub fn run_serve(repo_root: &Path, port: u16, open: bool) -> Result<()> {
    let state = AppState::from_repo(repo_root)?;
    let web_dir = find_web_dir();

    eprintln!("Serving repository: {}", repo_root.display());

    if let Some(ref dir) = web_dir {
        eprintln!("Serving static files from: {}", dir.display());
    } else {
        eprintln!("Warning: web directory not found - only API endpoints will work");
    }

    if open {
        let url = format!("http://127.0.0.1:{port}/");
        open_browser(&url);
    }

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| rbuilder_error::Error::Other(format!("Failed to start runtime: {e}")))?;
    rt.block_on(server::run_server(state, port, web_dir))
}

fn find_web_dir() -> Option<PathBuf> {
    // Check relative to current working directory
    let candidates = ["web", "../web", "./web"];
    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.join("index.html").exists() {
            eprintln!("Found web directory at: {}", path.display());
            return Some(path.canonicalize().ok()?);
        }
    }

    // Check relative to executable (for installed binary)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join("web");
            if path.join("index.html").exists() {
                eprintln!("Found web directory at: {}", path.display());
                return Some(path);
            }
            // Also check in parent dirs (for target/release/rbuilder -> ../../web)
            if let Some(parent) = dir.parent() {
                if let Some(grandparent) = parent.parent() {
                    let path = grandparent.join("web");
                    if path.join("index.html").exists() {
                        eprintln!("Found web directory at: {}", path.display());
                        return Some(path);
                    }
                }
            }
        }
    }

    eprintln!("Warning: Could not find web directory");
    None
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }
}
