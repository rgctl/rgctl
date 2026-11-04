use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;
pub fn fixture(name: &str) -> TempDir {
    fn copy(source: &Path, target: &Path) {
        fs::create_dir_all(target).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".rgctl" {
                continue;
            }
            let path = target.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &path);
            } else {
                fs::copy(entry.path(), path).unwrap();
            }
        }
    }
    let dir = TempDir::new().unwrap();
    copy(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("rgctl-tests")
            .join(name),
        dir.path(),
    );
    dir
}
pub fn cli(root: &Path, args: &[&str]) -> Value {
    let bin = std::env::var_os("RGCTL_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_rgctl")));
    let output = Command::new(bin)
        .current_dir(root)
        .args(["-f", "json"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{args:?}: {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}
