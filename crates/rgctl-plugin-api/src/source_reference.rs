//! Typed static references whose resolution depends on other source files.

use crate::SourceLocation;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Semantic reference kind; graph edges are selected from existing typed edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceReferenceKind {
    /// Jinja template inheritance.
    JinjaExtends,
    /// Jinja template inclusion, possibly with ordered fallback candidates.
    JinjaInclude,
    /// Jinja template or macro import.
    JinjaImport,
    /// A potentially resolvable project macro invocation.
    MacroCall,
    /// Ansible play/task includes a role.
    AnsibleRole,
    /// A role metadata dependency.
    RoleDependency,
    /// Ansible task-file reuse.
    TaskInclude,
    /// Ansible playbook reuse.
    PlaybookInclude,
    /// Ansible builtin template action source.
    TemplateRender,
    /// Ansible notification by handler name or listen topic.
    HandlerNotify,
}

/// Known source context, without inferred runtime inventory or variable values.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceReferenceContext {
    /// Canonical source role root, when statically known.
    pub role_root: Option<String>,
    /// Statically known playbook source.
    pub playbook: Option<String>,
    /// Ordered, repository-local loader/search roots.
    pub template_roots: Vec<String>,
}

/// A deferred reference emitted by a file worker and resolved with exact indexes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceReference {
    /// Static reference semantics.
    pub kind: SourceReferenceKind,
    /// Qualified identity of the source symbol.
    pub source: String,
    /// Original target name or expression; never evaluated.
    pub target: String,
    /// Ordered literal candidates, when the syntax supplies a fallback list.
    #[serde(default)]
    pub candidates: Vec<String>,
    /// True when the target requires runtime expression evaluation.
    #[serde(default)]
    pub dynamic: bool,
    /// Original source location.
    pub location: SourceLocation,
    /// Known source context for exact resolution.
    #[serde(default)]
    pub context: SourceReferenceContext,
    /// Literal binding/alias for an import or macro invocation.
    #[serde(default)]
    pub binding: Option<String>,
    /// Named imported macro or member, when distinct from its template.
    #[serde(default)]
    pub member: Option<String>,
    /// Compact semantic modifiers, settings, or unresolved evidence.
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::ExtractAllResult;

    #[test]
    fn old_json_facts_default_to_no_deferred_references() {
        let old = r#"{"symbols":[],"relations":[],"content_blobs":{}}"#;
        let result: ExtractAllResult = serde_json::from_str(old).expect("old facts");
        assert!(result.source_references.is_empty());
        assert_eq!(serde_json::to_string(&result).expect("facts"), old);
    }
}
