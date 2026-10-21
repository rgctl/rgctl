//! Shared CLI context: paths, graph I/O, and output routing.

use anyhow::{Context, Result, bail};
use rgctl_error::SymbolCandidate;
use rgctl_graph::CodeGraph;
use rgctl_graph::SnapshotNodeStore;
use std::cell::{Cell, RefCell};
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
    /// Whether a structured JSON error envelope was already written to stdout.
    json_error_emitted: Cell<bool>,
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
        let _ = rgctl_graph::paths::ensure_artifact_dir_migrated(&repo);
        let db = db.unwrap_or_else(|| rgctl_graph::paths::artifact_path(&repo, "graph.db"));
        Self {
            repo,
            db,
            format,
            output,
            verbose,
            snapshot_cache: RefCell::new(None),
            json_error_emitted: Cell::new(false),
        }
    }

    /// Whether a JSON error envelope was already emitted on stdout.
    pub fn json_error_emitted(&self) -> bool {
        self.json_error_emitted.get()
    }

    /// Mark that a JSON error envelope was written (prevents duplicate top-level emit).
    pub fn mark_json_error_emitted(&self) {
        self.json_error_emitted.set(true);
    }

    /// Emit a structured JSON error envelope on stdout and mark it emitted.
    pub fn emit_json_error(
        &self,
        error: &str,
        error_code: &str,
        message: &str,
        candidates: Option<&[SymbolCandidate]>,
        suggestions: Option<&[String]>,
        recovery_command: Option<&str>,
    ) -> Result<()> {
        let mut value = serde_json::json!({
            "schema_version": 1,
            "ok": false,
            "error": error,
            "error_code": error_code,
            "message": message,
        });
        if let Some(c) = candidates {
            if !c.is_empty() {
                value["candidates"] = serde_json::to_value(c)?;
            }
        }
        if let Some(s) = suggestions {
            if !s.is_empty() {
                value["suggestions"] = serde_json::to_value(s)?;
            }
        }
        if let Some(cmd) = recovery_command {
            value["recovery_command"] = serde_json::Value::String(cmd.to_string());
        }
        self.emit_json_value(&value)?;
        self.mark_json_error_emitted();
        Ok(())
    }

    /// Classify an [`anyhow::Error`] into a JSON error envelope for `-f json` top-level trapping.
    pub fn emit_json_error_from_anyhow(&self, err: &anyhow::Error) -> Result<()> {
        let msg = format!("{err:#}");
        let (error, error_code, recovery) = classify_cli_error(&msg);
        self.emit_json_error(error, error_code, &msg, None, None, recovery)
    }

    fn snapshot_path(&self) -> PathBuf {
        rgctl_graph::paths::artifact_path(&self.repo, rgctl_graph::snapshot::SNAPSHOT_FILE)
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
        let legacy = rgctl_graph::paths::artifact_path(&self.repo, "graph.json");
        if legacy.exists() {
            let json = fs::read_to_string(&legacy)?;
            return CodeGraph::import_json(&json).map_err(Into::into);
        }
        bail!(
            "Graph not found at {} (run `rgctl discover` first)",
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
            if let Err(e) = out.write_all(bytes) {
                if e.kind() != io::ErrorKind::BrokenPipe {
                    return Err(e.into());
                }
                return Ok(());
            }
            if !bytes.ends_with(b"\n") {
                if let Err(e) = out.write_all(b"\n") {
                    if e.kind() != io::ErrorKind::BrokenPipe {
                        return Err(e.into());
                    }
                }
            }
        }
        Ok(())
    }

    /// Write one line to stdout; ignore broken pipe when piped to `head`, etc.
    pub fn stdout_line(&self, line: &str) -> Result<()> {
        let mut out = io::stdout().lock();
        match writeln!(out, "{line}") {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(e.into()),
        }
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

fn classify_cli_error(message: &str) -> (&'static str, &'static str, Option<&'static str>) {
    let lower = message.to_ascii_lowercase();
    if lower.contains("ambiguous symbol") {
        (
            "ambiguous_symbol",
            "AMBIGUOUS_SYMBOL",
            Some("rgctl status"),
        )
    } else if lower.contains("graph not found") || lower.contains("run `rgctl discover`") {
        (
            "graph_not_found",
            "GRAPH_NOT_FOUND",
            Some("rgctl discover"),
        )
    } else if lower.contains("cfg") && (lower.contains("not indexed") || lower.contains("archive"))
    {
        (
            "cfg_not_indexed",
            "CFG_NOT_INDEXED",
            Some("rgctl discover --with-cfg"),
        )
    } else if lower.contains("incompatible with index scope")
        || lower.contains("semantic query scope")
    {
        let recovery = if lower.contains("--scope docs") || lower.contains("scope 'docs'") {
            Some("rgctl semantic index --scope docs")
        } else if lower.contains("--scope function") || lower.contains("scope 'function'") {
            Some("rgctl semantic index --scope function")
        } else {
            Some("rgctl semantic index")
        };
        ("index_scope_mismatch", "INDEX_SCOPE_MISMATCH", recovery)
    } else if lower.contains("not found") {
        ("not_found", "NOT_FOUND", None)
    } else {
        ("error", "ERROR", None)
    }
}

pub fn language_from_path(path: &Path) -> String {
    if super::markup::is_markup_context_path(path) {
        return "markdown".to_string();
    }
    crate::analysis::language_id_from_path(path)
        .unwrap_or("rust")
        .to_string()
}
