//! `rgctl install` — copy the embedded agent pack into a repo or global agent dirs.

use super::agent_pack::{
    PackArtifactKind, bundle_bytes, collect_artifacts, load_manifest, resolve_tools,
    validate_global_install,
};
use super::args::{OutputFormat, SkillHost};
use super::context::CliContext;
use super::install_output::{
    InstallJsonResponse, InstallWrite, InstallWriteKind, InstallWriteStatus,
    build_install_response, host_compat,
};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

pub struct InstallArgs {
    pub skill: bool,
    pub with_policy: bool,
    pub list_agents: bool,
    pub global_install: bool,
    pub tools: Option<Vec<String>>,
    pub host: Option<SkillHost>,
    pub force: bool,
}

pub fn run(ctx: &CliContext, args: InstallArgs) -> Result<()> {
    let manifest = load_manifest().map_err(anyhow::Error::msg)?;

    if args.list_agents {
        return emit_list_agents(ctx, &manifest);
    }

    if !args.skill && !args.with_policy {
        bail!(
            "pass --skill and/or --with-policy to install agent artifacts (see `rgctl install --help`)"
        );
    }

    if args.host.is_some() {
        eprintln!(
            "warning: --host is deprecated; use --tools cursor,claude,codex,agents (see `rgctl install --help`)"
        );
    }

    let agent_ids =
        resolve_tools(&manifest, args.tools.clone(), args.host).map_err(anyhow::Error::msg)?;
    if args.global_install {
        validate_global_install(&manifest, &agent_ids).map_err(anyhow::Error::msg)?;
    }
    let scope = if args.global_install { "global" } else { "local" };
    let prefix = install_prefix(ctx, args.global_install)?;

    let artifacts = collect_artifacts(&manifest, &agent_ids, args.skill, args.with_policy)
        .map_err(anyhow::Error::msg)?;

    if artifacts.is_empty() {
        bail!("no agent pack artifacts matched the requested flags");
    }

    let mut writes = Vec::new();
    let mut planned: Vec<(PathBuf, PathBuf)> = Vec::new();
    for art in &artifacts {
        let dest = prefix.join(&art.repo_rel);
        let contents = bundle_bytes(&art.bundle_rel)
            .with_context(|| format!("missing bundle file {}", art.bundle_rel.display()))?;
        let status = classify_write(&dest, contents, args.force)?;
        let kind = match art.kind {
            PackArtifactKind::Skill => InstallWriteKind::Skill,
            PackArtifactKind::Policy => InstallWriteKind::Policy,
            PackArtifactKind::Meta => InstallWriteKind::Meta,
            PackArtifactKind::Command => InstallWriteKind::Command,
        };
        let path = dest.display().to_string();
        planned.push((dest, art.bundle_rel.clone()));
        writes.push(InstallWrite {
            agent: art.agent_id.clone(),
            workflow: art.workflow.clone(),
            kind,
            path,
            status,
            host: host_compat(&art.agent_id),
        });
    }

    let blocked = writes
        .iter()
        .any(|w| w.status == InstallWriteStatus::SkippedExists);
    if !blocked {
        for (write, (dest, bundle_rel)) in writes.iter().zip(planned.iter()) {
            if matches!(
                write.status,
                InstallWriteStatus::Created | InstallWriteStatus::Overwritten
            ) {
                let contents = bundle_bytes(bundle_rel).context("bundle bytes")?;
                atomic_write(dest, contents)?;
            }
        }
    }

    let response = build_install_response(
        &prefix.display().to_string(),
        scope,
        agent_ids,
        args.with_policy,
        args.force,
        writes,
    );
    emit_response(ctx, &response)?;

    if response
        .writes
        .iter()
        .any(|w| w.status == InstallWriteStatus::SkippedExists)
    {
        bail!("agent pack file exists and differs (pass --force to overwrite)");
    }
    Ok(())
}

fn install_prefix(ctx: &CliContext, global: bool) -> Result<PathBuf> {
    if global {
        dirs::home_dir().context("home directory for --global install")
    } else {
        Ok(abs_path(&ctx.repo))
    }
}

fn emit_list_agents(ctx: &CliContext, manifest: &super::agent_pack::PackManifest) -> Result<()> {
    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::json!({
            "schema_version": 1,
            "command": "list-agents",
            "list_agents": true,
            "agents": manifest.agents,
            "workflows": manifest.workflows,
            "rgctl_version": manifest.rgctl_version,
        }))?;
        return Ok(());
    }
    let mut lines = vec!["rgctl agent registry:".to_string()];
    for a in &manifest.agents {
        let cmds = if a.commands_path.is_empty() {
            "-".to_string()
        } else {
            a.commands_path.clone()
        };
        lines.push(format!(
            "  {}  skills={}  commands={}  global={}  invoke={}",
            a.id, a.skills_path, cmds, a.supports_global, a.invoke_prefix
        ));
    }
    ctx.emit(&lines.join("\n"))?;
    Ok(())
}

fn classify_write(dest: &Path, contents: &[u8], force: bool) -> Result<InstallWriteStatus> {
    match fs::symlink_metadata(dest) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(InstallWriteStatus::Created);
        }
        Err(err) => {
            return Err(err).with_context(|| format!("stat {}", dest.display()));
        }
        Ok(_) => {}
    }
    let existing = fs::read(dest).with_context(|| format!("read {}", dest.display()))?;
    if existing.as_slice() == contents {
        return Ok(InstallWriteStatus::Unchanged);
    }
    if force {
        Ok(InstallWriteStatus::Overwritten)
    } else {
        Ok(InstallWriteStatus::SkippedExists)
    }
}

fn atomic_write(dest: &Path, contents: &[u8]) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(dest) {
        if meta.file_type().is_symlink() {
            fs::remove_file(dest).with_context(|| format!("remove symlink {}", dest.display()))?;
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let tmp_name = dest.file_name().map_or_else(
        || ".skill.rg-install.tmp".to_string(),
        |name| format!(".{}.rg-install.tmp", name.to_string_lossy()),
    );
    let tmp = dest.with_file_name(tmp_name);
    fs::write(&tmp, contents).with_context(|| format!("write {}", tmp.display()))?;
    if dest.exists() {
        let _ = fs::remove_file(dest);
    }
    fs::rename(&tmp, dest)
        .with_context(|| format!("rename {} -> {}", tmp.display(), dest.display()))?;
    Ok(())
}

fn abs_path(path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    fs::canonicalize(&joined).unwrap_or(joined)
}

fn emit_response(ctx: &CliContext, response: &InstallJsonResponse) -> Result<()> {
    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(response)?)?;
        return Ok(());
    }
    let mut lines = vec!["Installed rgctl agent pack:".to_string()];
    for write in &response.writes {
        lines.push(format!(
            "  {:<16} {}",
            status_label(write.status),
            write.path
        ));
    }
    ctx.emit(&lines.join("\n"))?;
    Ok(())
}

fn status_label(status: InstallWriteStatus) -> &'static str {
    match status {
        InstallWriteStatus::Created => "created",
        InstallWriteStatus::Unchanged => "unchanged",
        InstallWriteStatus::Overwritten => "overwritten",
        InstallWriteStatus::SkippedExists => "skipped_exists",
    }
}
