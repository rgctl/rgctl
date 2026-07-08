//! Shared CLI context: paths, graph I/O, and output routing.

use anyhow::{bail, Context, Result};
use rbuilder_graph::CodeGraph;
use rbuilder_graph::SnapshotNodeStore;
use std::cell::RefCell;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::OutputFormat;

/// Cached mmap graph snapshot opened once per CLI invocation.
#[derive(Clone)]
pub struct SnapshotSession {
    pub store: Arc<SnapshotNodeStore>,
    pub digest: Arc<str>,
}

/// Resolved global CLI paths and formatting options.
pub struct CliContext {
    pub repo: PathBuf,
    pub db: PathBuf,
    pub format: OutputFormat,
    pub output: Option<PathBuf>,
    pub verbose: bool,
    snapshot_cache: RefCell<Option<SnapshotSession>>,
}

impl CliContext {
    pub fn new(
        repo: Option<PathBuf>,
        db: Option<PathBuf>,
        format: OutputFormat,
        output: Option<PathBuf>,
        verbose: bool,
    ) -> Self {
        let repo =
            repo.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let db = db.unwrap_or_else(|| repo.join(".rbuilder").join("graph.db"));
        Self {
            repo,
            db,
            format,
            output,
            verbose,
            snapshot_cache: RefCell::new(None),
        }
    }

    fn snapshot_path(&self) -> PathBuf {
        self.repo
            .join(".rbuilder")
            .join(rbuilder_graph::snapshot::SNAPSHOT_FILE)
    }

    fn ensure_snapshot_loaded(&self) -> Result<()> {
        let path = self.snapshot_path();
        if !path.exists() {
            *self.snapshot_cache.borrow_mut() = None;
            return Ok(());
        }
        if self.snapshot_cache.borrow().is_some() {
            return Ok(());
        }
        let store = SnapshotNodeStore::open(&path)?;
        let digest: Arc<str> = Arc::from(store.content_digest()?);
        *self.snapshot_cache.borrow_mut() = Some(SnapshotSession {
            store: Arc::new(store),
            digest,
        });
        Ok(())
    }

    /// Open the mmap snapshot once; reuse on subsequent calls within this context.
    pub fn snapshot_session(&self) -> Result<Option<SnapshotSession>> {
        self.ensure_snapshot_loaded()?;
        Ok(self.snapshot_cache.borrow().clone())
    }

    pub fn load_graph(&self) -> Result<CodeGraph> {
        let snapshot = self.snapshot_path();
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

    /// Graph content digest when a binary snapshot is present (cached with snapshot session).
    pub fn graph_digest(&self) -> Result<Option<String>> {
        Ok(self
            .snapshot_session()?
            .map(|session| session.digest.to_string()))
    }

    /// Open a read-only mmap node store without hydrating [`MemoryBackend`].
    pub fn open_snapshot_store(&self) -> Result<Option<Arc<SnapshotNodeStore>>> {
        Ok(self.snapshot_session()?.map(|session| session.store))
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
    crate::analysis::language_id_from_path(path)
        .unwrap_or("rust")
        .to_string()
}
