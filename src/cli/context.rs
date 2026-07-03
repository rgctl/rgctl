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
        let snapshot = self
            .repo
            .join(".rbuilder")
            .join(rbuilder_graph::snapshot::SNAPSHOT_FILE);
        if snapshot.exists() {
            return CodeGraph::open_snapshot(&snapshot).map_err(Into::into);
        }
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

    /// Graph content digest when a binary snapshot is present.
    pub fn graph_digest(&self) -> Result<Option<String>> {
        let snapshot = self
            .repo
            .join(".rbuilder")
            .join(rbuilder_graph::snapshot::SNAPSHOT_FILE);
        if !snapshot.exists() {
            return Ok(None);
        }
        let mmap = rbuilder_graph::snapshot::MmappedGraphSnapshot::open(&snapshot)?;
        Ok(Some(mmap.content_digest().to_string()))
    }

    /// Open a read-only mmap node store without hydrating [`MemoryBackend`].
    pub fn open_snapshot_store(&self) -> Result<Option<rbuilder_graph::SnapshotNodeStore>> {
        let snapshot = self
            .repo
            .join(".rbuilder")
            .join(rbuilder_graph::snapshot::SNAPSHOT_FILE);
        if !snapshot.exists() {
            return Ok(None);
        }
        Ok(Some(rbuilder_graph::SnapshotNodeStore::open(&snapshot)?))
    }

    pub fn emit(&self, text: &str) -> Result<()> {
        self.emit_bytes(text.as_bytes())
    }

    pub fn emit_bytes(&self, bytes: &[u8]) -> Result<()> {
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
