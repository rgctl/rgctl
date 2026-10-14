//! Embedded agent pack manifest and path helpers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Parsed embedded `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackManifest {
    pub profile: String,
    pub version: u32,
    pub rgctl_version: String,
    pub workflows: Vec<WorkflowEntry>,
    #[serde(default)]
    pub commands: Vec<CommandEntry>,
    pub agents: Vec<AgentManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEntry {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEntry {
    pub id: String,
    pub title: String,
    pub skill: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifestEntry {
    pub id: String,
    pub agent_dir: String,
    pub skills_subdir: String,
    pub skills_path: String,
    #[serde(default)]
    pub commands_path: String,
    pub invoke_prefix: String,
    pub supports_global: bool,
}

static PACK_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/agent_pack.zip"));

static PACK_FILES: OnceLock<HashMap<PathBuf, Vec<u8>>> = OnceLock::new();

fn pack_files() -> &'static HashMap<PathBuf, Vec<u8>> {
    PACK_FILES.get_or_init(|| {
        let mut map = HashMap::new();
        let cursor = Cursor::new(PACK_ZIP);
        let mut archive = zip::ZipArchive::new(cursor).expect("agent_pack.zip");
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).expect("zip entry");
            let name = PathBuf::from(file.name());
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut buf).expect("read zip entry");
            map.insert(name, buf);
        }
        map
    })
}

pub fn load_manifest() -> Result<PackManifest, String> {
    let bytes = pack_files()
        .get(Path::new("manifest.json"))
        .ok_or("embedded agent pack missing manifest.json")?;
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

/// One installable artifact from the embedded pack.
#[derive(Debug, Clone)]
pub struct PackArtifact {
    pub agent_id: String,
    pub workflow: Option<String>,
    pub kind: PackArtifactKind,
    pub repo_rel: PathBuf,
    pub bundle_rel: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackArtifactKind {
    Skill,
    Policy,
    Meta,
    Command,
}

fn agent_entry<'a>(
    manifest: &'a PackManifest,
    agent_id: &str,
) -> Result<&'a AgentManifestEntry, String> {
    manifest
        .agents
        .iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| format!("agent {agent_id} missing from manifest"))
}

/// Collect artifacts to install for the given agents and flags.
pub fn collect_artifacts(
    manifest: &PackManifest,
    agent_ids: &[String],
    skills: bool,
    with_policy: bool,
) -> Result<Vec<PackArtifact>, String> {
    let mut out = Vec::new();
    let mut seen_dest: HashMap<PathBuf, ()> = HashMap::new();

    for agent_id in agent_ids {
        let entry = agent_entry(manifest, agent_id)?;
        let embed_prefix = format!("agents/host-{}/", agent_id);

        if skills {
            let skills_prefix = format!("{}{}/", embed_prefix, entry.skills_subdir);
            collect_skills(
                &skills_prefix,
                embed_prefix.as_str(),
                agent_id,
                &entry.agent_dir,
                &entry.skills_subdir,
                &mut out,
                &mut seen_dest,
            )?;
            // OpenSpec-style: --skill also installs commands when present in the pack.
            if !entry.commands_path.is_empty() {
                let cmd_subdir = entry
                    .commands_path
                    .strip_prefix(&format!("{}/", entry.agent_dir))
                    .unwrap_or(entry.commands_path.as_str())
                    .trim_matches('/');
                if !cmd_subdir.is_empty() {
                    let commands_embed = format!("{}{}/", embed_prefix, cmd_subdir);
                    collect_commands(
                        &commands_embed,
                        embed_prefix.as_str(),
                        agent_id,
                        &entry.agent_dir,
                        &mut out,
                        &mut seen_dest,
                    )?;
                }
            }
        }
    }

    if with_policy {
        let policy_bundle = PathBuf::from("policy/rgctl-structural.mdc");
        let policy_dest = PathBuf::from(".cursor/rules/rgctl-structural.mdc");
        if pack_files().contains_key(&policy_bundle) && !seen_dest.contains_key(&policy_dest) {
            seen_dest.insert(policy_dest.clone(), ());
            out.push(PackArtifact {
                agent_id: "cursor".to_string(),
                workflow: None,
                kind: PackArtifactKind::Policy,
                repo_rel: policy_dest,
                bundle_rel: policy_bundle,
            });
        }
    }

    Ok(out)
}

fn collect_skills(
    skills_prefix: &str,
    embed_prefix: &str,
    agent_id: &str,
    agent_dir: &str,
    skills_subdir: &str,
    out: &mut Vec<PackArtifact>,
    seen_dest: &mut HashMap<PathBuf, ()>,
) -> Result<(), String> {
    for (path, _) in pack_files() {
        let path = path.to_string_lossy();
        if !path.starts_with(skills_prefix) {
            continue;
        }
        let after_embed = path.strip_prefix(embed_prefix).unwrap_or(&path);
        let repo_rel = PathBuf::from(agent_dir).join(after_embed);
        if seen_dest.contains_key(&repo_rel) {
            continue;
        }
        seen_dest.insert(repo_rel.clone(), ());
        let workflow = workflow_from_skill_path(after_embed, skills_subdir);
        let kind = skill_kind(after_embed, skills_subdir);
        out.push(PackArtifact {
            agent_id: agent_id.to_string(),
            workflow,
            kind,
            repo_rel,
            bundle_rel: PathBuf::from(path.as_ref()),
        });
    }
    Ok(())
}

fn collect_commands(
    commands_prefix: &str,
    embed_prefix: &str,
    agent_id: &str,
    agent_dir: &str,
    out: &mut Vec<PackArtifact>,
    seen_dest: &mut HashMap<PathBuf, ()>,
) -> Result<(), String> {
    for (path, _) in pack_files() {
        let path = path.to_string_lossy();
        if !path.starts_with(commands_prefix) {
            continue;
        }
        let after_embed = path.strip_prefix(embed_prefix).unwrap_or(&path);
        let repo_rel = PathBuf::from(agent_dir).join(after_embed);
        if seen_dest.contains_key(&repo_rel) {
            continue;
        }
        seen_dest.insert(repo_rel.clone(), ());
        let workflow = command_workflow_id(after_embed);
        out.push(PackArtifact {
            agent_id: agent_id.to_string(),
            workflow,
            kind: PackArtifactKind::Command,
            repo_rel,
            bundle_rel: PathBuf::from(path.as_ref()),
        });
    }
    Ok(())
}

fn skill_kind(after_embed: &str, skills_subdir: &str) -> PackArtifactKind {
    let prefix = format!("{skills_subdir}/");
    let rest = after_embed.strip_prefix(&prefix).unwrap_or(after_embed);
    let name = rest.split('/').next().unwrap_or("");
    if name == "rgctl" {
        PackArtifactKind::Meta
    } else {
        PackArtifactKind::Skill
    }
}

fn workflow_from_skill_path(after_embed: &str, skills_subdir: &str) -> Option<String> {
    let prefix = format!("{skills_subdir}/");
    let rest = after_embed.strip_prefix(&prefix)?;
    let name = rest.split('/').next()?;
    if name == "rgctl" {
        return None;
    }
    name.strip_prefix("rgctl-")
        .map(str::to_string)
        .or_else(|| Some(name.to_string()))
}

fn command_workflow_id(after_embed: &str) -> Option<String> {
    let file = Path::new(after_embed)
        .file_stem()
        .and_then(|s| s.to_str())?;
    let id = file
        .strip_prefix("rgctl-")
        .or_else(|| file.strip_prefix("rgctl:"))
        .unwrap_or(file);
    Some(id.to_string())
}

pub fn bundle_bytes(bundle_rel: &Path) -> Option<&[u8]> {
    pack_files()
        .get(bundle_rel)
        .map(|v| v.as_slice())
}

/// Default install targets when `--tools` is omitted (phased v1 per #84).
pub const DEFAULT_INSTALL_AGENTS: &[&str] =
    &["cursor", "claude", "codex", "agents", "antigravity"];

pub fn all_agent_ids(manifest: &PackManifest) -> Vec<String> {
    manifest.agents.iter().map(|a| a.id.clone()).collect()
}

pub fn default_agent_ids(manifest: &PackManifest) -> Vec<String> {
    DEFAULT_INSTALL_AGENTS
        .iter()
        .filter(|id| manifest.agents.iter().any(|a| a.id == **id))
        .map(|s| s.to_string())
        .collect()
}

/// Resolve agent ids from `--tools`, deprecated `--host`, or v1 default.
pub fn resolve_tools(
    manifest: &PackManifest,
    tools: Option<Vec<String>>,
    host: Option<super::args::SkillHost>,
) -> Result<Vec<String>, String> {
    if let Some(list) = tools {
        let (ids, unknown) = normalize_tool_ids(manifest, list)?;
        if !unknown.is_empty() {
            eprintln!(
                "warning: unknown agent id(s) in --tools: {} (see `rgctl install --list-agents`)",
                unknown.join(", ")
            );
        }
        if ids.is_empty() {
            return Err(
                "no valid agent ids in --tools (use `rgctl install --list-agents`)".into(),
            );
        }
        return Ok(ids);
    }
    if let Some(h) = host {
        return Ok(match h {
            super::args::SkillHost::All => default_agent_ids(manifest),
            super::args::SkillHost::Claude => vec!["claude".to_string()],
            super::args::SkillHost::Codex => vec!["codex".to_string()],
            super::args::SkillHost::Cursor => vec!["cursor".to_string()],
        });
    }
    Ok(default_agent_ids(manifest))
}

/// Expand `all`; return `(known ids, unknown ids)`.
pub fn normalize_tool_ids(
    manifest: &PackManifest,
    tools: Vec<String>,
) -> Result<(Vec<String>, Vec<String>), String> {
    let known: HashMap<&str, ()> = manifest.agents.iter().map(|a| (a.id.as_str(), ())).collect();
    if tools.iter().any(|t| t == "all") {
        return Ok((all_agent_ids(manifest), Vec::new()));
    }
    let mut ids = Vec::new();
    let mut unknown = Vec::new();
    for t in tools {
        if known.contains_key(t.as_str()) {
            ids.push(t);
        } else {
            unknown.push(t);
        }
    }
    Ok((ids, unknown))
}

/// Reject `-g` when any selected agent is project-local only.
pub fn validate_global_install(manifest: &PackManifest, agent_ids: &[String]) -> Result<(), String> {
    for id in agent_ids {
        let entry = manifest
            .agents
            .iter()
            .find(|a| a.id == *id)
            .ok_or_else(|| format!("agent {id} missing from manifest"))?;
        if !entry.supports_global {
            return Err(format!(
                "agent {id} does not support --global (project-local only; omit -g)"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_pack_lists_cursor_skills() {
        let n = pack_files()
            .keys()
            .filter(|p| p.to_string_lossy().starts_with("agents/host-cursor/skills/"))
            .count();
        assert!(n > 5, "expected cursor skills in zip, got {n}");
    }

    #[test]
    fn embedded_pack_includes_review_skill_and_cursor_command() {
        let m = load_manifest().expect("manifest");
        assert!(
            m.commands.iter().any(|c| c.id == "review" && c.skill == "rgctl-review"),
            "manifest commands should list review"
        );
        assert!(
            pack_files()
                .keys()
                .any(|p| p.to_string_lossy().ends_with("skills/rgctl-review/SKILL.md")),
            "missing rgctl-review skill in zip"
        );
        assert!(
            pack_files().keys().any(|p| {
                let s = p.to_string_lossy();
                s.starts_with("agents/host-cursor/commands/") && s.contains("rgctl-review")
            }),
            "missing cursor rgctl-review command in zip"
        );
        let arts = collect_artifacts(&m, &["cursor".to_string()], true, false).expect("arts");
        assert!(
            arts.iter()
                .any(|a| a.kind == PackArtifactKind::Command && a.workflow.as_deref() == Some("review")),
            "collect_artifacts should include review command"
        );
        assert!(
            arts.iter().any(|a| {
                a.kind == PackArtifactKind::Skill
                    && a.repo_rel.to_string_lossy().contains("rgctl-review")
            }),
            "collect_artifacts should include rgctl-review skill"
        );
    }

    #[test]
    fn default_install_agents_are_v1_quad() {
        let m = load_manifest().expect("manifest");
        let ids = default_agent_ids(&m);
        assert_eq!(
            ids,
            ["cursor", "claude", "codex", "agents", "antigravity"]
        );
    }

    #[test]
    fn normalize_all_expands_full_registry() {
        let m = load_manifest().expect("manifest");
        let (ids, unknown) = normalize_tool_ids(&m, vec!["all".to_string()]).expect("normalize");
        assert!(unknown.is_empty());
        assert!(ids.len() >= 30);
        assert!(ids.iter().any(|id| id == "opencode"));
    }

    #[test]
    fn normalize_unknown_ids_reported() {
        let m = load_manifest().expect("manifest");
        let (ids, unknown) =
            normalize_tool_ids(&m, vec!["cursor".to_string(), "not-an-agent".to_string()])
                .expect("normalize");
        assert_eq!(ids, vec!["cursor".to_string()]);
        assert_eq!(unknown, vec!["not-an-agent".to_string()]);
    }

    #[test]
    fn resolve_tools_rejects_all_unknown() {
        let m = load_manifest().expect("manifest");
        let err = resolve_tools(&m, Some(vec!["typo".to_string()]), None).unwrap_err();
        assert!(err.contains("no valid agent ids"));
    }

    #[test]
    fn validate_global_rejects_project_only_agent() {
        let m = load_manifest().expect("manifest");
        let err = validate_global_install(&m, &["amazon-q".to_string()]).unwrap_err();
        assert!(err.contains("does not support --global"));
    }

    #[test]
    fn embedded_pack_includes_opencode_and_pi() {
        let m = load_manifest().expect("manifest");
        assert!(m.agents.iter().any(|a| a.id == "opencode"));
        assert!(m.agents.iter().any(|a| a.id == "pi"));
        assert!(
            pack_files()
                .keys()
                .any(|p| p.to_string_lossy().contains("host-opencode/"))
        );
        assert!(
            pack_files()
                .keys()
                .any(|p| p.to_string_lossy().contains("host-pi/"))
        );
    }
}
