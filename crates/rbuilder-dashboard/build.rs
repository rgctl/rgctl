//! Rebuild when dashboard static assets change (include_dir is compile-time only).

use std::path::Path;

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dashboard/dist");
    println!("cargo:rerun-if-changed={}", dist.join("index.html").display());
    let assets = dist.join("assets");
    if let Ok(read) = std::fs::read_dir(&assets) {
        for entry in read.flatten() {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }
}
