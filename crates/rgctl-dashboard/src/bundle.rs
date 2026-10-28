//! Extract embedded Vite build output into `.rgctl/dashboard/`.

use include_dir::{Dir, include_dir};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

static DASHBOARD_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../dashboard/dist");
static DASHBOARD_DIST_V2: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../dashboard/dist-v2");

/// Root directory name under `.rgctl/`.
pub const DASHBOARD_DIR_NAME: &str = "dashboard";

/// Selects the v2 bundle. Kept as a development affordance: the user-facing switch is now
/// `--with-universe`, and the v2 bundle is what it serves.
pub const DASHBOARD_V2_ENV: &str = "RGCTL_DASHBOARD_V2";

/// Emergency hatch: force the pre-universe (v1) embed until `remove-v1-dashboard`.
///
/// The CLI only exposes `--with-universe`. This env is for scripts / one-release rollback.
pub const DASHBOARD_LEGACY_ENV: &str = "RGCTL_DASHBOARD_LEGACY";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BundleVersion {
    V1,
    V2,
}

/// Process-local CLI override: 0 = none, 1 = V1, 2 = V2. Wins over env.
static BUNDLE_OVERRIDE: AtomicU8 = AtomicU8::new(0);

/// Pin the bundle for this process (discover/serve CLI). `None` clears the override.
pub fn set_bundle_override(version: Option<BundleVersion>) {
    let tag = match version {
        None => 0,
        Some(BundleVersion::V1) => 1,
        Some(BundleVersion::V2) => 2,
    };
    BUNDLE_OVERRIDE.store(tag, Ordering::SeqCst);
}

fn truthy(value: Option<&str>) -> bool {
    match value {
        Some(v) => !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false"),
        None => false,
    }
}

/// Which bundle to extract from env alone.
///
/// Pure, so it can be tested without mutating process-global state. CLI overrides go through
/// [`set_bundle_override`] and are applied in [`selected_version`].
fn version_from_env(legacy: Option<&str>, universe: Option<&str>) -> BundleVersion {
    if truthy(legacy) {
        return BundleVersion::V1;
    }
    let _ = universe;
    BundleVersion::V2
}

pub fn selected_version() -> BundleVersion {
    match BUNDLE_OVERRIDE.load(Ordering::SeqCst) {
        1 => return BundleVersion::V1,
        2 => return BundleVersion::V2,
        _ => {}
    }
    version_from_env(
        std::env::var(DASHBOARD_LEGACY_ENV).ok().as_deref(),
        std::env::var(DASHBOARD_V2_ENV).ok().as_deref(),
    )
}

/// Short tag for the selected bundle, recorded in the manifest so a later run can tell which one produced
/// the bundle already on disk.
pub fn selected_version_tag() -> &'static str {
    match selected_version() {
        BundleVersion::V1 => "v1",
        BundleVersion::V2 => "v2",
    }
}

/// What the extracted bundle is, recorded as `manifest.mode`.
pub fn selected_mode() -> &'static str {
    match selected_version() {
        BundleVersion::V1 => "dashboard",
        BundleVersion::V2 => "universe",
    }
}

/// Whether the v2 bundle is available to this build (embedded, or on disk in a source checkout).
pub fn v2_available() -> bool {
    let disk = dist_dir_for(BundleVersion::V2);
    disk_dist_complete(&disk) || embedded_dist_complete(&DASHBOARD_DIST_V2)
}

/// Say so when the legacy hatch is on, so nobody arrives at the old dashboard by accident.
pub fn warn_if_legacy_selected() {
    if selected_version() == BundleVersion::V1 {
        eprintln!(
            "note: {} is set, so the pre-universe dashboard is being served.\n  \
             Unset it (and use `--with-universe`) for the universe UI — removed next release.",
            DASHBOARD_LEGACY_ENV
        );
    }
}

/// Re-extract the static SPA for `version` when on-disk shell does not match.
///
/// Used by `rgctl serve` so `--open` defaults to the universe shell even if an older v1
/// bundle is still under `.rgctl/dashboard/`. JSON sidecars are left untouched.
///
/// Matching `manifest.mode` alone is not enough: a prior partial swap can leave mixed
/// `assets/` (v1 + v2 hashes) or a missing entry script while mode already says `universe`.
pub fn ensure_static_shell(out_dir: &Path, version: BundleVersion) -> Result<(), String> {
    set_bundle_override(Some(version));
    let want_mode = selected_mode();
    let want_tag = selected_version_tag();
    let manifest_path = out_dir.join("manifest.json");
    let current_mode = read_manifest_mode(&manifest_path);
    if current_mode.as_deref() == Some(want_mode) && index_matches_version(out_dir, version) {
        return Ok(());
    }
    // Drop stale hashed SPA chunks so v1 files cannot shadow / confuse a v2 extract.
    let assets = out_dir.join("assets");
    if assets.is_dir() {
        let _ = fs::remove_dir_all(&assets);
    }
    extract_static_assets(out_dir)?;
    if manifest_path.is_file() {
        patch_manifest_bundle_fields(&manifest_path, want_mode, want_tag)?;
    }
    Ok(())
}

/// True when `index.html` references the entry script naming for this bundle version.
fn index_matches_version(out_dir: &Path, version: BundleVersion) -> bool {
    let Ok(html) = fs::read_to_string(out_dir.join("index.html")) else {
        return false;
    };
    match version {
        // v2 vite build prefixes the entry as `index.v2-*.js`.
        BundleVersion::V2 => html.contains("index.v2-") && html.contains("<title>rgctl</title>"),
        BundleVersion::V1 => {
            html.contains("rgctl Analysis Dashboard") && !html.contains("index.v2-")
        }
    }
}

fn read_manifest_mode(path: &Path) -> Option<String> {
    let bytes = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&bytes).ok()?;
    value
        .get("mode")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

fn patch_manifest_bundle_fields(path: &Path, mode: &str, bundle_version: &str) -> Result<(), String> {
    let bytes = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut value: serde_json::Value =
        serde_json::from_str(&bytes).map_err(|e| format!("parse manifest for shell swap: {e}"))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("mode".into(), serde_json::Value::String(mode.into()));
        obj.insert(
            "bundle_version".into(),
            serde_json::Value::String(bundle_version.into()),
        );
    }
    let pretty = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("serialize manifest after shell swap: {e}"))?;
    fs::write(path, pretty).map_err(|e| e.to_string())
}

/// Say so when a bundle was requested that this build cannot serve.
///
/// `RGCTL_DASHBOARD_V2=1` against a build with no v2 bundle used to produce **no output at all** — the
/// variable was read, the embed was empty, and every path downstream either skipped or fell back without a
/// word. Silence is the worst response to a request you cannot honour: it sends the reader looking at their
/// repository, their flags and their branch, which is exactly what happened here. Printed before the work
/// rather than after it, so a long `discover` does not have to finish to report a problem it could have
/// stated at the start.
pub fn warn_if_selection_unavailable() {
    if selected_version() == BundleVersion::V2 && !v2_available() {
        eprintln!(
            "warn: {DASHBOARD_V2_ENV} is set but this build has no v2 dashboard bundle.\n  \
             Build one with: npm --prefix dashboard run build:v2 && cargo build\n  \
             The v2 bundle is selected at extraction time, so this is stated now rather than at the end."
        );
    }
}

fn dist_dir_for(version: BundleVersion) -> PathBuf {
    let dir = match version {
        BundleVersion::V1 => "dist",
        BundleVersion::V2 => "dist-v2",
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dashboard")
        .join(dir)
}

fn embedded_for(version: BundleVersion) -> &'static Dir<'static> {
    match version {
        BundleVersion::V1 => &DASHBOARD_DIST,
        BundleVersion::V2 => &DASHBOARD_DIST_V2,
    }
}

fn workspace_dist_dir() -> PathBuf {
    dist_dir_for(selected_version())
}

/// True when a usable dashboard build exists (disk tree in dev, or full embed for release).
///
/// Reports false when v2 is selected and unavailable, rather than falling back to v1: a silent fallback
/// would serve the old dashboard while the operator believes they are looking at the new one.
pub fn dist_embedded() -> bool {
    let disk = workspace_dist_dir();
    disk_dist_complete(&disk) || embedded_dist_complete(embedded_for(selected_version()))
}

fn disk_dist_complete(dist: &Path) -> bool {
    dist.join("index.html").is_file() && dist.join("assets").is_dir()
}

fn embedded_dist_complete(dist: &Dir<'_>) -> bool {
    dist.get_file("index.html").is_some()
        && dist
            .get_dir("assets")
            .is_some_and(|assets| embedded_file_count(assets) > 0)
}

fn embedded_file_count(dir: &Dir<'_>) -> usize {
    dir.files().count() + dir.dirs().map(embedded_file_count).sum::<usize>()
}

/// Write all files from the selected embedded bundle into `out_dir`.
pub fn extract_static_assets(out_dir: &Path) -> Result<(), String> {
    let version = selected_version();
    extract_static_assets_from(out_dir, &dist_dir_for(version), embedded_for(version)).map_err(|e| {
        match version {
            BundleVersion::V1 => e,
            BundleVersion::V2 => format!(
                "{e}\n  note: {DASHBOARD_V2_ENV} selected the v2 bundle; build it with: npm --prefix dashboard run build:v2"
            ),
        }
    })
}

fn extract_static_assets_from(
    out_dir: &Path,
    disk: &Path,
    embedded: &Dir<'_>,
) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

    if disk_dist_complete(disk) {
        copy_dir_recursive(disk, out_dir)?;
        return Ok(());
    }

    if !embedded_dist_complete(embedded) {
        return Err(
            "dashboard/dist incomplete — run: ./scripts/build-dashboard.sh && cargo build --release"
                .into(),
        );
    }

    write_embedded_dir(out_dir, embedded)?;
    Ok(())
}

fn write_embedded_dir(out_dir: &Path, dir: &Dir<'_>) -> Result<(), String> {
    for file in dir.files() {
        write_embedded_file(out_dir, file.path(), file.contents())?;
    }
    for child in dir.dirs() {
        write_embedded_dir(out_dir, child)?;
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
    let script =
        format!(r#"<script id="rgctl-manifest" type="application/json">{manifest_json}</script>"#);
    if html.contains("id=\"rgctl-manifest\"") {
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
    repo_root.join(".rgctl").join(DASHBOARD_DIR_NAME)
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

    /// Shared by the v1 and v2 embed tests.
    fn assert_paths(dir: &Dir<'_>) {
        for file in dir.files() {
            let p = file.path().to_string_lossy();
            assert!(
                !p.contains("assets/assets/"),
                "double-nested asset path in embed: {p}"
            );
        }
        for child in dir.dirs() {
            assert_paths(child);
        }
    }

    #[test]
    fn v2_is_opt_in_and_explicit() {
        // the universe is the default now: neither flag nor variable needs to be set
        assert_eq!(version_from_env(None, None), BundleVersion::V2);
        assert_eq!(version_from_env(Some(""), Some("")), BundleVersion::V2);
        assert_eq!(version_from_env(Some("0"), None), BundleVersion::V2);
        assert_eq!(
            version_from_env(Some("false"), Some("0")),
            BundleVersion::V2
        );
        assert_eq!(version_from_env(None, Some("1")), BundleVersion::V2);
        // the one-release hatch is the only way back via env
        assert_eq!(version_from_env(Some("1"), None), BundleVersion::V1);
        assert_eq!(version_from_env(Some("true"), Some("1")), BundleVersion::V1);
        assert_eq!(version_from_env(Some("TRUE"), None), BundleVersion::V1);
    }

    #[test]
    fn cli_override_wins_over_env_default() {
        set_bundle_override(Some(BundleVersion::V1));
        assert_eq!(selected_version(), BundleVersion::V1);
        set_bundle_override(Some(BundleVersion::V2));
        assert_eq!(selected_version(), BundleVersion::V2);
        set_bundle_override(None);
        assert_eq!(selected_version(), version_from_env(None, None));
    }

    /// The v2 bundle is committed, so a fresh checkout must carry a complete one. The embed is the check.
    #[test]
    fn v2_embed_is_complete() {
        let v2 = embedded_for(BundleVersion::V2);
        assert!(
            embedded_dist_complete(v2),
            "dashboard/dist-v2 needs index.html and a non-empty assets/ — rebuild: npm --prefix dashboard run build:v2"
        );
        assert_paths(v2);
    }

    /// Rust finds the entry by name (`disk_dist_complete`, `embedded_dist_complete`,
    /// `inject_manifest_bootstrap`). The v2 build renames its entry to `index.html` to match; if that
    /// regresses, this says so rather than the dashboard silently 404ing.
    #[test]
    fn both_bundles_define_the_same_entry() {
        for version in [BundleVersion::V1, BundleVersion::V2] {
            assert!(
                embedded_for(version).get_file("index.html").is_some(),
                "{version:?} has no index.html"
            );
        }
    }

    #[test]
    fn embedded_dist_has_no_double_nested_assets() {
        if embedded_file_count(&DASHBOARD_DIST) == 0 {
            return;
        }
        assert_paths(&DASHBOARD_DIST);
    }

    #[test]
    fn embedded_bundle_extracts_without_workspace_dist() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing_disk = tmp.path().join("missing-dashboard-dist");
        let out = tmp.path().join("extracted");

        extract_static_assets_from(&out, &missing_disk, &DASHBOARD_DIST)
            .expect("extract embedded dashboard");

        assert!(out.join("index.html").is_file());
        let assets = out.join("assets");
        assert!(assets.is_dir());
        assert!(
            fs::read_dir(assets)
                .expect("read extracted assets")
                .next()
                .is_some(),
            "embedded dashboard assets should be extracted recursively"
        );
    }

    /// The v1 worker bundle must reference a `rgctl_wasm_bg-*.wasm` asset that exists in its dist.
    ///
    /// Pinned to **V1 explicitly**. It used to follow the selected version, which was harmless while v1 was
    /// the default and became a false failure the moment the universe became what both flags produce: the
    /// v2 bundle carries no engine worker yet (no v2 surface calls the WASM expander until the L2 drill-down
    /// lands), so the assertion was asking a bundle for an asset it does not have.
    #[test]
    fn dist_worker_wasm_asset_exists() {
        let v1 = dist_dir_for(BundleVersion::V1);
        if !(disk_dist_complete(&v1) || embedded_dist_complete(&DASHBOARD_DIST)) {
            eprintln!("skip: dashboard/dist not built");
            return;
        }
        let assets = v1.join("assets");
        let worker = fs::read_dir(&assets)
            .map_err(|e| e.to_string())
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| {
                e.file_name().to_string_lossy().starts_with("worker-")
                    && e.path().extension().is_some_and(|ext| ext == "js")
            })
            .map(|e| e.path())
            .expect("worker-*.js in dashboard/dist/assets");
        let worker_src = fs::read_to_string(&worker).expect("read worker bundle");
        let wasm_name = worker_src
            .split("rgctl_wasm_bg-")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .map(|hash| format!("rgctl_wasm_bg-{hash}"))
            .expect("worker references rgctl_wasm_bg-*.wasm");
        assert!(
            assets.join(&wasm_name).is_file(),
            "missing WASM asset {wasm_name} referenced by {}",
            worker.file_name().unwrap_or_default().to_string_lossy()
        );
    }
}
