//! Phase 14: Graphviz rendering tests.

use rbuilder::export::{check_graphviz_installed, render_dot_to_file, ImageFormat, Layout};
use std::process::Command;
use tempfile::TempDir;

const MINI_DOT: &str = r#"digraph G { a -> b; }"#;

#[test]
fn test_check_graphviz_installed_returns_bool() {
    let installed = check_graphviz_installed();
    let dot_ok = Command::new("dot")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert_eq!(installed, dot_ok);
}

#[test]
fn test_render_without_graphviz_returns_error() {
    if check_graphviz_installed() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let out = temp.path().join("graph.png");
    let err = render_dot_to_file(MINI_DOT, &out, ImageFormat::Png, Layout::Dot).unwrap_err();
    assert!(err.to_string().contains("Graphviz not found"));
}

#[test]
#[ignore = "requires Graphviz installed"]
fn test_render_png_creates_file() {
    let temp = TempDir::new().unwrap();
    let out = temp.path().join("graph.png");
    render_dot_to_file(MINI_DOT, &out, ImageFormat::Png, Layout::Dot).unwrap();
    assert!(out.exists());
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"\x89PNG"));
}

#[test]
#[ignore = "requires Graphviz installed"]
fn test_render_svg_creates_file() {
    let temp = TempDir::new().unwrap();
    let out = temp.path().join("graph.svg");
    render_dot_to_file(MINI_DOT, &out, ImageFormat::Svg, Layout::Dot).unwrap();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("<svg"));
}

#[test]
fn test_image_format_from_extension() {
    use std::path::Path;
    assert_eq!(
        ImageFormat::from_path(Path::new("out.png")),
        Some(ImageFormat::Png)
    );
    assert_eq!(
        ImageFormat::from_path(Path::new("out.svg")),
        Some(ImageFormat::Svg)
    );
}
