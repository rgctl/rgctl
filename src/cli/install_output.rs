//! Structured `rgctl install` JSON response.

use serde::{Deserialize, Serialize};

/// Current install JSON schema version.
pub const INSTALL_SCHEMA_VERSION: u32 = 3;

/// Legacy host ids (schema v1); prefer `agent` string in v2+.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InstallWriteHost {
    Claude,
    Codex,
    Cursor,
}

/// Kind of installed artifact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InstallWriteKind {
    Skill,
    Policy,
    Meta,
    Command,
}

/// Outcome of one destination file.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallWriteStatus {
    Created,
    Unchanged,
    Overwritten,
    SkippedExists,
}

/// One planned or completed file write.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallWrite {
    /// Agent registry id (`cursor`, `claude`, …).
    pub agent: String,
    /// Workflow id when applicable (`search`, `migrate`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    /// Artifact kind.
    pub kind: InstallWriteKind,
    /// Absolute destination path.
    pub path: String,
    /// Write outcome.
    pub status: InstallWriteStatus,
    /// Schema v1 compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<InstallWriteHost>,
}

/// Top-level install JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallJsonResponse {
    pub schema_version: u32,
    pub command: String,
    pub skill: String,
    pub repo: String,
    pub scope: String,
    pub agents: Vec<String>,
    pub with_policy: bool,
    pub force: bool,
    pub writes: Vec<InstallWrite>,
}

/// Build the install response object.
pub fn build_install_response(
    repo: &str,
    scope: &str,
    agents: Vec<String>,
    with_policy: bool,
    force: bool,
    writes: Vec<InstallWrite>,
) -> InstallJsonResponse {
    InstallJsonResponse {
        schema_version: INSTALL_SCHEMA_VERSION,
        command: "install".into(),
        skill: "rgctl".into(),
        repo: repo.to_string(),
        scope: scope.to_string(),
        agents,
        with_policy,
        force,
        writes,
    }
}

pub fn host_compat(agent: &str) -> Option<InstallWriteHost> {
    match agent {
        "claude" => Some(InstallWriteHost::Claude),
        "codex" | "agents" => Some(InstallWriteHost::Codex),
        "cursor" => Some(InstallWriteHost::Cursor),
        _ => None,
    }
}
