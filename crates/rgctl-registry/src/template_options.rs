//! Repository-local language routing. Protected manifests/workflows precede
//! associations, followed by standard suffixes, role layouts, and source probes.
//! Includes are repository-relative globs. Loader roots are relative directories;
//! absolute/parent-traversing paths are rejected rather than escaping the repo.
use globset::{Glob, GlobSet, GlobSetBuilder};
use rgctl_error::{Error, Result};
use rgctl_plugin_api::JinjaSyntax;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

/// Explicit standalone task/playbook associations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnsibleOptions {
    /// Repository-relative globs, e.g. `automation/tasks/*.yaml`.
    pub include: Vec<String>,
}
/// Standalone templates and their independently known loader/syntax profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JinjaOptions {
    /// Repository-relative globs; compound and extensionless names are allowed.
    pub include: Vec<String>,
    /// Ordered repository-relative loader roots (no implicit global suffix lookup).
    pub template_roots: Vec<String>,
    /// Defaults overridden only by a valid literal `#jinja2:` file header.
    pub syntax: JinjaSyntax,
}
/// Optional `[languages.ansible]` and `[languages.jinja]` in `rgctl.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateOptions {
    /// Ansible source associations.
    pub ansible: AnsibleOptions,
    /// Jinja associations, loader roots and syntax.
    pub jinja: JinjaOptions,
}

pub fn safe_source_relative(path: &str) -> bool {
    !path.contains('\\')
        && !Path::new(path).is_absolute()
        && !Path::new(path)
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
}
fn compile(patterns: &[String]) -> Result<GlobSet> {
    let mut set = GlobSetBuilder::new();
    for pattern in patterns {
        if !safe_source_relative(pattern) || pattern.is_empty() {
            return Err(Error::ConfigError(format!(
                "association must be repository relative: {pattern}"
            )));
        }
        set.add(Glob::new(pattern).map_err(|e| Error::ConfigError(e.to_string()))?);
    }
    set.build().map_err(|e| Error::ConfigError(e.to_string()))
}
#[derive(Clone)]
pub(crate) struct CompiledOptions {
    pub options: TemplateOptions,
    ansible: GlobSet,
    jinja: GlobSet,
}
impl CompiledOptions {
    pub fn new(options: TemplateOptions) -> Result<Self> {
        options
            .jinja
            .syntax
            .validate()
            .map_err(Error::ConfigError)?;
        for root in &options.jinja.template_roots {
            if !safe_source_relative(root) || root.is_empty() || root.contains(['*', '?', '[']) {
                return Err(Error::ConfigError(format!("invalid template root: {root}")));
            }
        }
        Ok(Self {
            ansible: compile(&options.ansible.include)?,
            jinja: compile(&options.jinja.include)?,
            options,
        })
    }
    pub fn association(&self, path: &Path) -> Result<Option<&'static str>> {
        match (self.ansible.is_match(path), self.jinja.is_match(path)) {
            (true, true) => Err(Error::ConfigError(format!(
                "conflicting Ansible/Jinja associations: {}",
                path.display()
            ))),
            (true, false) => Ok(Some("ansible")),
            (false, true) => Ok(Some("jinja")),
            _ => Ok(None),
        }
    }
}
/// Recognized role directories, including `ansible_collections/ns/name/roles/r`.
/// The returned root is lexical; filesystem eligibility is decided by discovery.
pub fn role_root(path: &Path) -> Option<std::path::PathBuf> {
    path.ancestors()
        .skip(1)
        .find(|ancestor| {
            ancestor
                .parent()
                .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "roles"))
        })
        .map(Path::to_path_buf)
}
/// Known alternate template dialects require an explicit Jinja association.
pub fn alternate_template_dialect(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension,
                "erb" | "epp" | "mustache" | "hbs" | "liquid" | "twig" | "mako" | "vm"
            )
        })
}
pub(crate) fn role_template(path: &Path) -> bool {
    !alternate_template_dialect(path)
        && role_root(path).is_some_and(|root| {
            path.strip_prefix(&root).is_ok_and(|relative| {
                relative.starts_with("templates") && relative.components().count() > 1
            })
        })
}
