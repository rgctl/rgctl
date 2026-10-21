//! Session graph status: cheap freshness / presence check (no rediscover).

use super::context::CliContext;
use super::OutputFormat;
use crate::discovery::{DiscoveryConfig, FileDiscoverer};
use crate::languages::registry::LanguageRegistry;
use anyhow::Result;
use rgctl_incremental::FileTracker;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;

const STATUS_SCHEMA_VERSION: u32 = 3;
const DIRTY_SAMPLE_LIMIT: usize = 10;

#[derive(Debug, Serialize)]
struct CapabilityReady {
    ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    unlock_command: &'static str,
}

#[derive(Debug, Serialize)]
struct CfgCpgCapability {
    ready: bool,
    cpg_overlay: bool,
    ast_skeleton: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    unlock_command: &'static str,
}

#[derive(Debug, Serialize)]
struct SemanticCapability {
    ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    unlock_command: &'static str,
}

#[derive(Debug, Serialize)]
struct AgentSkillsCapability {
    installed_hosts: Vec<String>,
    unlock_command: &'static str,
}

#[derive(Debug, Serialize)]
struct SessionCapabilities {
    structural_graph: CapabilityReady,
    cfg_cpg: CfgCpgCapability,
    semantic_index: SemanticCapability,
    agent_skills: AgentSkillsCapability,
}

#[derive(Debug, Serialize)]
struct SessionStatus {
    schema_version: u32,
    command: &'static str,
    /// `ok` | `missing`
    status: &'static str,
    repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nodes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    edges: Option<usize>,
    kantra_findings: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    kantra_findings_path: Option<String>,
    /// Whether discoverable sources match `file_hashes.json` (None when snapshot missing).
    #[serde(skip_serializing_if = "Option::is_none")]
    index_current: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dirty_files: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_added: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_changed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_deleted: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dirty_sample: Option<Vec<String>>,
    /// Whether a live external `serve --watch` holds `.rgctl/watch.lock`.
    #[serde(skip_serializing_if = "Option::is_none")]
    watcher_alive: Option<bool>,
    /// Pid of the live watcher when `watcher_alive` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    watcher_pid: Option<u32>,
    /// Pending lines in `.rgctl/update_queue.jsonl` (0 if absent).
    #[serde(skip_serializing_if = "Option::is_none")]
    update_queue_pending: Option<usize>,
    capabilities: SessionCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn infer_semantic_scope(repo: &Path) -> Option<String> {
    let path = crate::analysis::SemanticIndex::default_path(repo);
    let index = crate::analysis::SemanticIndex::load(&path).ok()?;
    let has_functions = index
        .entries
        .iter()
        .any(|e| e.node_type.as_deref() == Some("Function"));
    let has_docs = index.entries.iter().any(|e| {
        e.node_type.as_deref() == Some("Module")
            && e.kind
                .as_deref()
                .is_some_and(|kind| kind == "heading" || kind == "code_block")
    });
    match (has_functions, has_docs) {
        (true, true) => Some("all".into()),
        (true, false) => Some("function".into()),
        (false, true) => Some("docs".into()),
        (false, false) => Some("unknown".into()),
    }
}

fn detect_agent_skill_hosts(repo: &Path) -> Vec<String> {
    let mut hosts = Vec::new();
    let candidates = [
        ("cursor", repo.join(".cursor/skills/rgctl/SKILL.md")),
        ("claude", repo.join(".claude/skills/rgctl/SKILL.md")),
        ("agents", repo.join(".agents/skills/rgctl/SKILL.md")),
    ];
    for (host, path) in candidates {
        if path.is_file() {
            hosts.push(host.to_string());
        }
    }
    hosts
}

fn build_capabilities(repo: &Path, structural_ready: bool) -> SessionCapabilities {
    let snapshot = rgctl_graph::paths::artifact_path(repo, rgctl_graph::snapshot::SNAPSHOT_FILE);
    let cfg_path = crate::analysis::archive_path(repo);
    let ast_path = repo
        .join(".rgctl")
        .join(crate::analysis::AST_SKELETON_ARCHIVE_FILE);
    let sem_path = crate::analysis::SemanticIndex::default_path(repo);
    let cfg_ready = cfg_path.is_file();
    let ast_ready = ast_path.is_file();
    SessionCapabilities {
        structural_graph: CapabilityReady {
            ready: structural_ready,
            path: structural_ready.then(|| snapshot.display().to_string()),
            unlock_command: "rgctl discover",
        },
        cfg_cpg: CfgCpgCapability {
            ready: cfg_ready,
            cpg_overlay: cfg_ready,
            ast_skeleton: ast_ready,
            path: cfg_ready.then(|| cfg_path.display().to_string()),
            unlock_command: "rgctl discover --with-cfg",
        },
        semantic_index: SemanticCapability {
            ready: sem_path.is_file(),
            scope: if sem_path.is_file() {
                infer_semantic_scope(repo)
            } else {
                None
            },
            path: sem_path
                .is_file()
                .then(|| sem_path.display().to_string()),
            unlock_command: "rgctl semantic index",
        },
        agent_skills: AgentSkillsCapability {
            installed_hosts: detect_agent_skill_hosts(repo),
            unlock_command: "rgctl install --skill",
        },
    }
}

#[derive(Debug, Default)]
struct Staleness {
    index_current: bool,
    dirty_files: usize,
    files_added: usize,
    files_changed: usize,
    files_deleted: usize,
    dirty_sample: Vec<String>,
}

fn kantra_findings_path(repo: &Path) -> std::path::PathBuf {
    rgctl_graph::paths::artifact_path(repo, "kantra_findings.json")
}

/// Compute working-tree vs tracker freshness without extract/compact.
fn compute_staleness(repo: &Path) -> Staleness {
    let registry: Arc<rgctl_registry::LanguageRegistry> = LanguageRegistry::new().into();
    let discoverer = FileDiscoverer::with_config(registry, DiscoveryConfig::default());
    let files = match discoverer.discover(repo) {
        Ok(f) => f,
        Err(_) => {
            return Staleness {
                index_current: true,
                ..Default::default()
            };
        }
    };
    let tracker = FileTracker::load(repo).unwrap_or_else(|_| FileTracker::new(repo));
    let changes = match tracker.detect_changes(&files) {
        Ok(c) => c,
        Err(_) => {
            return Staleness {
                index_current: true,
                ..Default::default()
            };
        }
    };
    let files_added = changes.added.len();
    let files_changed = changes.changed.len();
    let files_deleted = changes.deleted.len();
    let dirty_files = files_added + files_changed + files_deleted;
    let mut dirty_sample: Vec<String> = changes
        .added
        .iter()
        .chain(changes.changed.iter())
        .chain(changes.deleted.iter())
        .take(DIRTY_SAMPLE_LIMIT)
        .cloned()
        .collect();
    dirty_sample.sort();
    Staleness {
        index_current: dirty_files == 0,
        dirty_files,
        files_added,
        files_changed,
        files_deleted,
        dirty_sample,
    }
}

/// `rgctl status` — report whether `.rgctl/` has a usable snapshot.
pub fn run_status(ctx: &CliContext) -> Result<()> {
    let findings = kantra_findings_path(&ctx.repo);
    let kantra_present = findings.is_file();
    let kantra_path = kantra_present.then(|| findings.display().to_string());

    let watcher = super::pipeline_status::detect_live_watcher(&ctx.repo);
    let watcher_alive = Some(watcher.is_some());
    let watcher_pid = watcher.map(|w| w.pid);
    let update_queue_pending = Some(super::pipeline_status::update_queue::queue_pending_count(
        &ctx.repo,
    ));

    let session = ctx.snapshot_session()?;
    let payload = match session {
        Some(s) => {
            let stale = compute_staleness(&ctx.repo);
            let message = if stale.index_current {
                None
            } else {
                Some(format!(
                    "Index not current ({} dirty source file(s)); run `rgctl update` or use `rgctl serve --watch`",
                    stale.dirty_files
                ))
            };
            SessionStatus {
                schema_version: STATUS_SCHEMA_VERSION,
                command: "status",
                status: "ok",
                repo: ctx.repo.display().to_string(),
                snapshot: Some(
                    rgctl_graph::paths::artifact_path(
                        &ctx.repo,
                        rgctl_graph::snapshot::SNAPSHOT_FILE,
                    )
                    .display()
                    .to_string(),
                ),
                digest: Some(s.digest.to_string()),
                nodes: Some(s.store.node_count()),
                edges: Some(s.store.edge_count()),
                kantra_findings: kantra_present,
                kantra_findings_path: kantra_path,
                index_current: Some(stale.index_current),
                dirty_files: Some(stale.dirty_files),
                files_added: Some(stale.files_added),
                files_changed: Some(stale.files_changed),
                files_deleted: Some(stale.files_deleted),
                dirty_sample: if stale.dirty_sample.is_empty() {
                    None
                } else {
                    Some(stale.dirty_sample)
                },
                watcher_alive,
                watcher_pid,
                update_queue_pending,
                capabilities: build_capabilities(&ctx.repo, true),
                message,
            }
        }
        None => SessionStatus {
            schema_version: STATUS_SCHEMA_VERSION,
            command: "status",
            status: "missing",
            repo: ctx.repo.display().to_string(),
            snapshot: None,
            digest: None,
            nodes: None,
            edges: None,
            kantra_findings: kantra_present,
            kantra_findings_path: kantra_path,
            index_current: None,
            dirty_files: None,
            files_added: None,
            files_changed: None,
            files_deleted: None,
            dirty_sample: None,
            watcher_alive,
            watcher_pid,
            update_queue_pending,
            capabilities: build_capabilities(&ctx.repo, false),
            message: Some("Graph snapshot not found; run `rgctl discover` first".into()),
        },
    };

    if ctx.format == OutputFormat::Json {
        let v = serde_json::to_value(&payload)?;
        ctx.emit_json_value(&v)?;
    } else if payload.status == "ok" {
        let current = payload
            .index_current
            .map(|c| if c { "yes" } else { "no" })
            .unwrap_or("?");
        ctx.stdout_line(&format!(
            "status=ok nodes={} edges={} digest={} index_current={}{}{}",
            payload.nodes.unwrap_or(0),
            payload.edges.unwrap_or(0),
            payload.digest.as_deref().unwrap_or("?"),
            current,
            if payload.kantra_findings {
                " kantra_findings=yes"
            } else {
                ""
            },
            payload
                .message
                .as_ref()
                .map(|m| format!(" — {m}"))
                .unwrap_or_default()
        ))?;
        emit_capabilities_text(ctx, &payload.capabilities)?;
    } else {
        ctx.stdout_line(
            payload
                .message
                .as_deref()
                .unwrap_or("Graph snapshot missing"),
        )?;
        emit_capabilities_text(ctx, &payload.capabilities)?;
    }

    if payload.status == "missing" {
        anyhow::bail!("graph snapshot missing");
    }
    Ok(())
}

fn emit_capabilities_text(ctx: &CliContext, caps: &SessionCapabilities) -> Result<()> {
    ctx.stdout_line("capabilities:")?;
    let sg = if caps.structural_graph.ready {
        "ready"
    } else {
        "missing"
    };
    ctx.stdout_line(&format!(
        "  structural_graph: {sg}  ({})",
        caps.structural_graph.unlock_command
    ))?;
    let cfg = if caps.cfg_cpg.ready { "ready" } else { "missing" };
    ctx.stdout_line(&format!(
        "  cfg_cpg: {cfg} overlay={} ast_skeleton={}  ({})",
        caps.cfg_cpg.cpg_overlay,
        caps.cfg_cpg.ast_skeleton,
        caps.cfg_cpg.unlock_command
    ))?;
    let sem = if caps.semantic_index.ready {
        "ready"
    } else {
        "missing"
    };
    let scope = caps
        .semantic_index
        .scope
        .as_deref()
        .unwrap_or("-");
    ctx.stdout_line(&format!(
        "  semantic_index: {sem} scope={scope}  ({})",
        caps.semantic_index.unlock_command
    ))?;
    let hosts = if caps.agent_skills.installed_hosts.is_empty() {
        "none".to_string()
    } else {
        caps.agent_skills.installed_hosts.join(",")
    };
    ctx.stdout_line(&format!(
        "  agent_skills: hosts={hosts}  ({})",
        caps.agent_skills.unlock_command
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{PipelineConfig, ProcessingPipeline};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn staleness_detects_dirty_source() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn a() {}\n").unwrap();
        let pipeline = ProcessingPipeline::with_config(
            LanguageRegistry::new().into(),
            PipelineConfig {
                show_progress: false,
                ..PipelineConfig::default()
            },
        );
        let (graph, _) = pipeline.process_repository(root).unwrap();
        graph.save_to_repo(root).unwrap();
        let mut tracker = FileTracker::new(root);
        tracker
            .index_files(&[root.join("src/lib.rs")], &graph)
            .unwrap();
        tracker.save().unwrap();

        let clean = compute_staleness(root);
        assert!(clean.index_current, "expected clean index");

        fs::write(root.join("src/lib.rs"), "pub fn a() {}\npub fn b() {}\n").unwrap();
        let dirty = compute_staleness(root);
        assert!(!dirty.index_current);
        assert!(dirty.dirty_files >= 1);
    }
}
