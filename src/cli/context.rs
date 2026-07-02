//! Shared CLI context: paths, graph I/O, and output routing.

use anyhow::{bail, Context, Result};
use rbuilder_graph::CodeGraph;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::OutputFormat;

/// Resolved global CLI paths and formatting options.
#[derive(Debug, Clone)]
pub struct CliContext {
    pub repo: PathBuf,
    pub db: PathBuf,
    pub format: OutputFormat,
    pub output: Option<PathBuf>,
    pub verbose: bool,
}

impl CliContext {
    pub fn new(
        repo: Option<PathBuf>,
        db: Option<PathBuf>,
        format: OutputFormat,
        output: Option<PathBuf>,
        verbose: bool,
    ) -> Self {
        let repo = repo
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let db = db.unwrap_or_else(|| repo.join(".rbuilder").join("graph.db"));
        Self {
            repo,
            db,
            format,
            output,
            verbose,
        }
    }

    pub fn load_graph(&self) -> Result<CodeGraph> {
        if self.db.exists() {
            let json = fs::read_to_string(&self.db)
                .with_context(|| format!("read graph db {}", self.db.display()))?;
            return CodeGraph::import_json(&json).map_err(Into::into);
        }
        let legacy = self.repo.join(".rbuilder").join("graph.json");
        if legacy.exists() {
            let json = fs::read_to_string(&legacy)?;
            return CodeGraph::import_json(&json).map_err(Into::into);
        }
        bail!(
            "Graph not found at {} (run `rbuilder discover` first)",
            self.db.display()
        );
    }

    pub fn is_html_dashboard(&self) -> bool {
        self.format == OutputFormat::HtmlDashboard
    }

    /// Write the interactive HTML dashboard (`-f html-dashboard`, optional `-o` path).
    pub fn emit_html_dashboard(&self) -> Result<()> {
        use crate::export::export_html_dashboard;

        let graph = self.load_graph()?;
        let path = self
            .output
            .clone()
            .unwrap_or_else(|| self.repo.join(".rbuilder/dashboard.html"));
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let analysis_dir = self.repo.join(".rbuilder/analysis");
        export_html_dashboard(
            graph.backend(),
            if analysis_dir.exists() {
                Some(&analysis_dir)
            } else {
                None
            },
            &path,
        )
        .map_err(|e| anyhow::anyhow!(e))?;

        if self.output.is_none() {
            println!("HTML dashboard: {}", path.display());
        }
        Ok(())
    }

    pub fn emit(&self, text: &str) -> Result<()> {
        if self.is_html_dashboard() {
            return Ok(());
        }
        self.emit_bytes(text.as_bytes())
    }

    pub fn emit_bytes(&self, bytes: &[u8]) -> Result<()> {
        if self.is_html_dashboard() {
            return Ok(());
        }
        if let Some(path) = &self.output {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, bytes)?;
        } else {
            let mut out = io::stdout().lock();
            out.write_all(bytes)?;
            if !bytes.ends_with(b"\n") {
                out.write_all(b"\n")?;
            }
        }
        Ok(())
    }

    pub fn emit_json_value(&self, value: &serde_json::Value) -> Result<()> {
        if self.is_html_dashboard() {
            return Ok(());
        }
        match self.format {
            OutputFormat::Json => {
                let text = serde_json::to_string_pretty(value)?;
                self.emit(&text)
            }
            _ => {
                if let Some(s) = value.as_str() {
                    self.emit(s)
                } else {
                    self.emit(&serde_json::to_string_pretty(value)?)
                }
            }
        }
    }
}

pub fn language_from_path(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("py") => "python".into(),
        Some("java") => "java".into(),
        Some("js") | Some("jsx") => "javascript".into(),
        Some("ts") | Some("tsx") => "typescript".into(),
        Some("go") => "go".into(),
        _ => "rust".into(),
    }
}
