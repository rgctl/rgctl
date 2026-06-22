//! Render DOT to PNG/SVG/PDF via Graphviz CLI (Phase 14.3).

use crate::graphviz::Layout;
use rbuilder_error::{Error, Result};
use std::path::Path;
use std::process::Command;

/// Output image format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG raster
    Png,
    /// SVG vector
    Svg,
    /// PDF document
    Pdf,
}

impl ImageFormat {
    /// Graphviz `-T` flag value.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
            Self::Pdf => "pdf",
        }
    }

    /// Detect format from file extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => Some(Self::Png),
            "svg" => Some(Self::Svg),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }
}

/// Return `true` if the `dot` binary is available.
pub fn check_graphviz_installed() -> bool {
    Command::new("dot")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write `dot_content` to `output_path` using Graphviz.
pub fn render_dot_to_file(
    dot_content: &str,
    output_path: &Path,
    format: ImageFormat,
    layout: Layout,
) -> Result<()> {
    if !check_graphviz_installed() {
        return Err(Error::Other(
            "Graphviz not found. Install with: brew install graphviz (macOS) or apt install graphviz (Linux)".into(),
        ));
    }

    let temp_dir = std::env::temp_dir().join(format!("rbuilder-dot-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| Error::Other(e.to_string()))?;
    let dot_path = temp_dir.join("graph.dot");
    std::fs::write(&dot_path, dot_content).map_err(|e| Error::Other(e.to_string()))?;

    let status = Command::new(layout.as_str())
        .arg(format!("-T{}", format.extension()))
        .arg("-o")
        .arg(output_path)
        .arg(&dot_path)
        .status()
        .map_err(|e| Error::Other(format!("Failed to run {}: {e}", layout.as_str())))?;

    if !status.success() {
        return Err(Error::Other(format!(
            "Graphviz {} failed with status {}",
            layout.as_str(),
            status
        )));
    }
    Ok(())
}
