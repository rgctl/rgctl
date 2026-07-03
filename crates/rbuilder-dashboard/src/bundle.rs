//! Extract embedded Vite build output into `.rbuilder/dashboard/`.

use include_dir::{include_dir, Dir};
use std::fs;
use std::path::{Path, PathBuf};

static DASHBOARD_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../dashboard/dist");

/// Root directory name under `.rbuilder/`.
pub const DASHBOARD_DIR_NAME: &str = "dashboard";

fn workspace_dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dashboard/dist")
}

/// True when a usable dashboard build exists (disk tree in dev, or full embed for release).
pub fn dist_embedded() -> bool {
    let disk = workspace_dist_dir();
    if disk.join("index.html").is_file() && disk.join("assets").is_dir() {
        return true;
    }
    embedded_file_count() > 1
}

fn embedded_file_count() -> usize {
    DASHBOARD_DIST.files().count()
}

/// Write all files from embedded `dashboard/dist` into `out_dir`.
pub fn extract_static_assets(out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

    let disk = workspace_dist_dir();
    if disk.join("index.html").is_file() && disk.join("assets").is_dir() {
        copy_dir_recursive(&disk, out_dir)?;
        return Ok(());
    }

    if embedded_file_count() <= 1 {
        return Err(
            "dashboard/dist incomplete — run: ./scripts/build-dashboard.sh && cargo build --release"
                .into(),
        );
    }

    for file in DASHBOARD_DIST.files() {
        write_embedded_file(out_dir, file.path(), file.contents())?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            fs::create_dir_all(&to).map_err(|e| e.to_string())?;
            copy_dir_recursive(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn write_embedded_file(out_dir: &Path, rel: &Path, contents: &[u8]) -> Result<(), String> {
    let dest = out_dir.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&dest, contents).map_err(|e| e.to_string())
}

/// Inject manifest JSON into index.html for offline `file://` bootstrap.
pub fn inject_manifest_bootstrap(out_dir: &Path, manifest_json: &str) -> Result<(), String> {
    let index_path = out_dir.join("index.html");
    let html = fs::read_to_string(&index_path).map_err(|e| format!("read index.html: {e}"))?;
    const MARKER: &str = "</head>";
    let script = format!(
        r#"<script id="rbuilder-manifest" type="application/json">{manifest_json}</script>"#
    );
    if html.contains("id=\"rbuilder-manifest\"") {
        return Ok(());
    }
    let updated = html.replace(MARKER, &format!("{script}\n  {MARKER}"));
    if updated == html {
        return Err("index.html missing </head> — cannot inject manifest".into());
    }
    fs::write(&index_path, updated).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn default_dashboard_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".rbuilder").join(DASHBOARD_DIR_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dist_has_index_and_assets_when_built() {
        if !dist_embedded() {
            eprintln!("skip: dashboard/dist not built");
            return;
        }
        let disk = workspace_dist_dir();
        assert!(disk.join("index.html").is_file());
        assert!(disk.join("assets").is_dir());
    }

    #[test]
    fn embedded_dist_has_no_double_nested_assets() {
        if embedded_file_count() == 0 {
            return;
        }
        for file in DASHBOARD_DIST.files() {
            let p = file.path().to_string_lossy();
            assert!(
                !p.contains("assets/assets/"),
                "double-nested asset path in embed: {p}"
            );
        }
    }
}
