//! Assert relative `.md` links in key docs resolve to existing files.

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn check_file(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let re = Regex::new(r"\[([^\]]*)\]\(([^)]+)\)").unwrap();
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut broken = Vec::new();
    for cap in re.captures_iter(&text) {
        let href = cap[2].trim();
        if href.starts_with("http://")
            || href.starts_with("https://")
            || href.starts_with("mailto:")
            || href.starts_with('#')
        {
            continue;
        }
        let file_part = href.split('#').next().unwrap_or("");
        if file_part.is_empty() || !file_part.ends_with(".md") {
            continue;
        }
        let target = base.join(file_part);
        if !target.exists() {
            broken.push(format!(
                "{}: [{}]({}) → missing {}",
                path.display(),
                &cap[1],
                href,
                target.display()
            ));
        }
    }
    broken
}

#[test]
fn key_docs_relative_md_links_resolve() {
    let root = repo_root();
    let files = [
        root.join("README.md"),
        root.join("CONTRIBUTING.md"),
        root.join("docs/README.md"),
        root.join("docs/guides/README.md"),
    ];
    let mut all = Vec::new();
    for f in &files {
        assert!(f.is_file(), "expected {}", f.display());
        all.extend(check_file(f));
    }
    assert!(
        all.is_empty(),
        "broken relative .md links:\n{}",
        all.join("\n")
    );
}
