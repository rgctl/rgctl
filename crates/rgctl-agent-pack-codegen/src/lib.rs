//! Generate the embedded agent pack from `skills/` and agent adapters.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Agent adapter definition (`agent-pack/agents/registry.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct AgentDef {
    pub id: String,
    pub agent_dir: String,
    pub skills_subdir: String,
    #[serde(default)]
    pub commands_subdir: String,
    #[serde(default = "default_command_style")]
    pub command_style: String,
    #[serde(default = "default_command_extension")]
    pub command_extension: String,
    #[serde(default)]
    pub commands_enabled: bool,
    pub invoke_prefix: String,
    pub supports_global: bool,
}

fn default_command_style() -> String {
    "hyphen".into()
}

fn default_command_extension() -> String {
    "md".into()
}

/// Installed pack manifest written to `out/manifest.json`.
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

#[derive(Debug, Deserialize)]
struct RootManifest {
    profile: String,
    version: u32,
    workflows: Vec<WorkflowEntry>,
    meta_skills: Vec<WorkflowEntry>,
    #[serde(default)]
    commands: Vec<CommandEntry>,
}

/// Generate the full agent pack under `out_dir`.
pub fn generate(pack_root: &Path, out_dir: &Path, rgctl_version: &str) -> Result<(), String> {
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

    let manifest_yaml =
        fs::read_to_string(pack_root.join("manifest.yaml")).map_err(|e| e.to_string())?;
    let root: RootManifest = serde_yaml::from_str(&manifest_yaml).map_err(|e| e.to_string())?;
    if !root.meta_skills.iter().any(|m| m.id == "rgctl") {
        return Err("manifest meta_skills must include id rgctl".into());
    }

    let repo_root = pack_root.parent().unwrap_or(pack_root);
    let agents = load_agents(&pack_root.join("agents"))?;

    // Skills: meta_skills + skills referenced by commands
    let mut skill_ids: Vec<String> = root.meta_skills.iter().map(|m| m.id.clone()).collect();
    for cmd in &root.commands {
        if !skill_ids.iter().any(|s| s == &cmd.skill) {
            skill_ids.push(cmd.skill.clone());
        }
    }

    for agent in &agents {
        for skill_id in &skill_ids {
            let skill_src = repo_root.join("skills").join(skill_id);
            if !skill_src.is_dir() {
                return Err(format!("skills/{skill_id} missing"));
            }
            if skill_id == "rgctl" && !skill_src.join("references/workflows.md").is_file() {
                return Err("skills/rgctl/references/workflows.md missing".into());
            }
            let skill_dest = agent_out_root(out_dir, &agent.id)
                .join(&agent.skills_subdir)
                .join(skill_id);
            copy_skill_tree(&skill_src, &skill_dest)?;
            ensure_rgctl_managed_frontmatter(
                &skill_dest.join("SKILL.md"),
                skill_id,
                rgctl_version,
            )?;
        }

        if agent.commands_enabled && !agent.commands_subdir.is_empty() {
            for cmd in &root.commands {
                emit_command(pack_root, out_dir, agent, cmd, rgctl_version)?;
            }
        }
    }

    let pack_manifest = PackManifest {
        profile: root.profile.clone(),
        version: root.version,
        rgctl_version: rgctl_version.to_string(),
        workflows: root.workflows.clone(),
        commands: root.commands.clone(),
        agents: agents
            .iter()
            .map(|a| AgentManifestEntry {
                id: a.id.clone(),
                agent_dir: a.agent_dir.clone(),
                skills_subdir: a.skills_subdir.clone(),
                skills_path: format!("{}/{}/", a.agent_dir, a.skills_subdir),
                commands_path: if a.commands_enabled && !a.commands_subdir.is_empty() {
                    format!("{}/{}/", a.agent_dir, a.commands_subdir)
                } else {
                    String::new()
                },
                invoke_prefix: a.invoke_prefix.clone(),
                supports_global: a.supports_global,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&pack_manifest).map_err(|e| e.to_string())?;
    fs::write(out_dir.join("manifest.json"), json).map_err(|e| e.to_string())?;

    let policy = render_policy(rgctl_version);
    let policy_path = out_dir.join("policy/rgctl-structural.mdc");
    if let Some(parent) = policy_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(policy_path, policy).map_err(|e| e.to_string())?;

    Ok(())
}

fn emit_command(
    pack_root: &Path,
    out_dir: &Path,
    agent: &AgentDef,
    cmd: &CommandEntry,
    rgctl_version: &str,
) -> Result<(), String> {
    let stub_src = pack_root
        .join("commands")
        .join(format!("rgctl-{}.md", cmd.id));
    let body = if stub_src.is_file() {
        fs::read_to_string(&stub_src).map_err(|e| e.to_string())?
    } else {
        default_command_stub(cmd)
    };
    let filename = command_filename(cmd, &agent.command_style, &agent.command_extension);
    let dest = agent_out_root(out_dir, &agent.id)
        .join(&agent.commands_subdir)
        .join(&filename);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = ensure_command_frontmatter(&body, cmd, &agent.command_style, rgctl_version)?;
    fs::write(&dest, content).map_err(|e| e.to_string())?;
    Ok(())
}

fn command_filename(cmd: &CommandEntry, style: &str, ext: &str) -> String {
    let base = match style {
        "colon" => format!("rgctl:{}", cmd.id),
        "dollar" => format!("rgctl-{}", cmd.id),
        "slash" | "hyphen" | _ => format!("rgctl-{}", cmd.id),
    };
    if ext.is_empty() {
        base
    } else {
        format!("{base}.{ext}")
    }
}

fn default_command_stub(cmd: &CommandEntry) -> String {
    format!(
        r#"---
name: /rgctl:{}
id: rgctl-{}
category: Workflow
description: "{}"
---

Follow the **{}** skill protocol installed under this agent's skills root.
"#,
        cmd.id, cmd.id, cmd.title, cmd.skill
    )
}

fn ensure_command_frontmatter(
    body: &str,
    cmd: &CommandEntry,
    style: &str,
    version: &str,
) -> Result<String, String> {
    let invoke = match style {
        "hyphen" | "slash" | "dollar" => format!("/rgctl-{}", cmd.id),
        _ => format!("/rgctl:{}", cmd.id),
    };
    // Prefer documenting colon form as canonical in description; keep name host-friendly.
    let mut text = body.to_string();
    if !text.contains("rgctl-managed: true") {
        if text.starts_with("---\n") {
            let rest = &text[4..];
            if let Some(end) = rest.find("\n---\n") {
                let front = &rest[..end];
                let after = &rest[end + 5..];
                text = format!(
                    "---\n{front}\nrgctl-managed: true\nmetadata:\n  generatedBy: \"rgctl {version}\"\n  canonicalInvoke: \"/rgctl:{}\"\n---\n{after}",
                    cmd.id
                );
            }
        } else {
            text = format!(
                "---\nname: {invoke}\nid: rgctl-{}\nrgctl-managed: true\nmetadata:\n  generatedBy: \"rgctl {version}\"\n  canonicalInvoke: \"/rgctl:{}\"\n---\n\n{text}",
                cmd.id, cmd.id
            );
        }
    }
    // Ensure name documents slash invoke when missing
    if !text.contains("name:") {
        text = format!("---\nname: {invoke}\n---\n\n{text}");
    }
    let _ = invoke;
    Ok(text)
}

#[derive(Debug, Deserialize)]
struct AgentRegistryFile {
    agent: Vec<AgentDef>,
}

fn load_agents(agents_dir: &Path) -> Result<Vec<AgentDef>, String> {
    let registry = agents_dir.join("registry.toml");
    let s = fs::read_to_string(&registry)
        .map_err(|e| format!("read {}: {}", registry.display(), e))?;
    let file: AgentRegistryFile = toml::from_str(&s).map_err(|e| e.to_string())?;
    let mut out = file.agent;
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn embed_agent_dir(agent_id: &str) -> String {
    format!("host-{agent_id}")
}

fn agent_out_root(out_dir: &Path, agent_id: &str) -> PathBuf {
    out_dir.join("agents").join(embed_agent_dir(agent_id))
}

/// Copy a skill tree. Skip a leftover `workflows/` dir if present.
fn copy_skill_tree(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for ent in walkdir::WalkDir::new(src) {
        let ent = ent.map_err(|e| e.to_string())?;
        let rel = ent.path().strip_prefix(src).map_err(|e| e.to_string())?;
        if rel
            .components()
            .next()
            .is_some_and(|c| c.as_os_str() == "workflows")
        {
            continue;
        }
        let target = dest.join(rel);
        if ent.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = target.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            fs::copy(ent.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn ensure_rgctl_managed_frontmatter(
    skill_path: &Path,
    skill_id: &str,
    version: &str,
) -> Result<(), String> {
    let existing = fs::read_to_string(skill_path).map_err(|e| e.to_string())?;
    if existing.contains("rgctl-managed: true") {
        return Ok(());
    }
    if existing.starts_with("---\n") {
        let rest = &existing[4..];
        if let Some(end) = rest.find("\n---\n") {
            let front = &rest[..end];
            let body = &rest[end + 5..];
            let updated = format!(
                "---\n{front}\nrgctl-managed: true\nmetadata:\n  generatedBy: \"rgctl {version}\"\n---\n{body}"
            );
            fs::write(skill_path, updated).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    let updated = format!(
        "---\nname: {skill_id}\nrgctl-managed: true\nmetadata:\n  generatedBy: \"rgctl {version}\"\n---\n\n{existing}"
    );
    fs::write(skill_path, updated).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod codegen_tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn generate_copies_workflows_reference() {
        let repo = repo_root();
        let pack = repo.join("agent-pack");
        let out = repo.join("target/agent-pack-test-workflows-ref");
        generate(&pack, &out, "test").expect("gen");
        let src = fs::read(repo.join("skills/rgctl/references/workflows.md")).expect("src");
        let dest = fs::read(
            out.join("agents/host-cursor/skills/rgctl/references/workflows.md"),
        )
        .expect("dest");
        assert_eq!(src, dest);
        assert!(!out
            .join("agents/host-cursor/skills/rgctl/workflows")
            .exists());
    }

    #[test]
    fn generate_emits_rgctl_and_review_for_cursor() {
        let repo = repo_root();
        let pack = repo.join("agent-pack");
        let out = repo.join("target/agent-pack-test-review");
        generate(&pack, &out, "test").expect("gen");
        let skills = out.join("agents/host-cursor/skills");
        assert!(skills.join("rgctl/SKILL.md").is_file());
        assert!(skills.join("rgctl-review/SKILL.md").is_file());
        assert!(skills.join("rgctl/references/workflows.md").is_file());
        assert!(!skills.join("rgctl-search").exists());
        assert!(!skills.join("rgctl-discover").exists());
        assert!(!skills.join("rgctl-migrate").exists());
        assert!(!skills.join("rgctl-gql").exists());
        let cmd = out.join("agents/host-cursor/commands/rgctl-review.md");
        assert!(cmd.is_file(), "missing {}", cmd.display());
        let body = fs::read_to_string(&cmd).expect("cmd");
        assert!(body.contains("/rgctl:review") || body.contains("rgctl:review"));
        assert!(body.contains("rgctl-managed: true"));
    }

    #[test]
    fn generate_emits_colon_command_for_claude() {
        let repo = repo_root();
        let pack = repo.join("agent-pack");
        let out = repo.join("target/agent-pack-test-claude-cmd");
        generate(&pack, &out, "test").expect("gen");
        let cmd = out.join("agents/host-claude/commands/rgctl/rgctl:review.md");
        assert!(cmd.is_file(), "missing {}", cmd.display());
    }

    #[test]
    fn generate_deterministic() {
        let repo = repo_root();
        let pack = repo.join("agent-pack");
        let out = repo.join("target/agent-pack-test-det-a");
        let out2 = repo.join("target/agent-pack-test-det-b");
        generate(&pack, &out, "test").expect("gen a");
        generate(&pack, &out2, "test").expect("gen b");
        assert_eq!(
            fs::read(out.join("agents/host-cursor/skills/rgctl-review/SKILL.md")).unwrap(),
            fs::read(out2.join("agents/host-cursor/skills/rgctl-review/SKILL.md")).unwrap()
        );
        assert_eq!(
            fs::read(out.join("agents/host-cursor/commands/rgctl-review.md")).unwrap(),
            fs::read(out2.join("agents/host-cursor/commands/rgctl-review.md")).unwrap()
        );
    }

    #[test]
    fn agents_without_commands_skip_emit() {
        let repo = repo_root();
        let pack = repo.join("agent-pack");
        let out = repo.join("target/agent-pack-test-no-cmd");
        generate(&pack, &out, "test").expect("gen");
        // forgecode has commands_enabled = false
        let forge = out.join("agents/host-forgecode");
        assert!(forge.join("skills/rgctl/SKILL.md").is_file());
        assert!(forge.join("skills/rgctl-review/SKILL.md").is_file());
        // no commands dir expected (empty commands_subdir)
        assert!(
            !forge.join("commands").exists()
                || walkdir::WalkDir::new(forge.join("commands"))
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .count()
                    == 0
        );
    }

    #[test]
    fn dogfood_agents_skills_match_skills_tree() {
        let repo = repo_root();
        for name in ["rgctl", "rgctl-review"] {
            let src = repo.join("skills").join(name);
            let dst = repo.join(".agents/skills").join(name);
            assert!(src.is_dir(), "missing skills/{name}");
            assert!(dst.is_dir(), "missing .agents/skills/{name} — sync from skills/");
            for entry in walkdir::WalkDir::new(&src)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let rel = entry.path().strip_prefix(&src).unwrap();
                let other = dst.join(rel);
                assert!(
                    other.is_file(),
                    ".agents/skills/{name}/{} missing",
                    rel.display()
                );
                assert_eq!(
                    fs::read(entry.path()).unwrap(),
                    fs::read(&other).unwrap(),
                    "drift: skills/{name}/{} vs .agents/skills/{name}/{}",
                    rel.display(),
                    rel.display()
                );
            }
        }
    }

    #[test]
    fn skills_markdown_has_no_broken_relative_docs_links() {
        let repo = repo_root();
        let skills = repo.join("skills");
        let mut offenders = Vec::new();
        for entry in walkdir::WalkDir::new(&skills)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        {
            let text = fs::read_to_string(entry.path()).unwrap();
            if text.contains("../../docs/") {
                offenders.push(entry.path().strip_prefix(&repo).unwrap().display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "skills/ must not contain broken ../../docs/ links (breaks consumer installs): {offenders:?}"
        );
    }
}

fn render_policy(version: &str) -> String {
    format!(
        r#"---
description: Prefer rgctl for structural codebase questions when .rgctl/ exists
globs:
alwaysApply: true
rgctl-managed: true
metadata:
  generatedBy: "rgctl {version}"
---

# rgctl structural policy (best-effort)

Agent hosts do **not** guarantee blocking grep or read tools. This rule biases behavior only.

When the repository contains `.rgctl/` (after `discover`):

- **MUST** use `rgctl -f json` first for **structural** questions: callers/callees, blast radius, communities, data flow, migration **roadmap** (`discover --export-migration-hints` → `migration_plan.json`), semantic/graph search, PR/branch review (`/rgctl:review`).
- **MAY** use ripgrep/grep for **lexical** search (fixed strings, comments, logs) and to open files **after** rgctl cites paths.
- **MUST NOT** use bulk grep-as-call-graph (e.g. searching for `foo(` to find callers) when rgctl can answer.

Parse `schema_version` from stdout; never redirect stderr to `/dev/null`.
"#
    )
}
