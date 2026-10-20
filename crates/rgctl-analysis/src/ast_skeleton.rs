//! Coarse AST skeleton for hybrid CPG P4a (sidecar, opt-in).
//!
//! Nodes: function / block / if / loop / call / assign / decl — plus parent links.
//! Not written into `graph.snapshot.bin`.

use crate::language_profile::{function_kinds_for, parse_source};
use rgctl_error::{Error, Result};
use rgctl_plugin_helpers::{ecmascript_function_symbol_name, extract_name_from_node};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::Node;
use uuid::Uuid;

/// Archive filename under `.rgctl/analysis/`.
pub const AST_SKELETON_ARCHIVE_FILE: &str = "ast_skeleton.archive.bin";
/// Format version.
pub const AST_SKELETON_VERSION: u32 = 1;

/// Coarse syntax role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstSkeletonKind {
    /// Function / method / constructor.
    Function,
    /// Block / compound statement.
    Block,
    /// If / conditional.
    If,
    /// Loop (while/for/loop).
    Loop,
    /// Call / invocation.
    Call,
    /// Assignment.
    Assign,
    /// Declaration / let / local.
    Decl,
    /// Catch-all named construct.
    Other,
}

/// One skeleton node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstSkeletonNode {
    /// Stable id within the function skeleton.
    pub id: u32,
    /// Parent id (`None` = function root).
    pub parent: Option<u32>,
    /// Kind.
    pub kind: AstSkeletonKind,
    /// 1-based start line.
    pub start_line: usize,
    /// 1-based end line.
    pub end_line: usize,
    /// Short label (truncated source).
    pub label: String,
}

/// Per-function skeleton.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstSkeletonRecord {
    /// L_repo function id when known.
    pub function_id: Option<Uuid>,
    /// Function name.
    pub function_name: String,
    /// Source path.
    pub file_path: String,
    /// Skeleton nodes.
    pub nodes: Vec<AstSkeletonNode>,
}

/// On-disk archive of skeletons.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AstSkeletonArchive {
    /// Format version.
    pub version: u32,
    /// Optional graph digest.
    pub graph_digest: Option<String>,
    /// Records keyed by `file::function`.
    pub records: Vec<AstSkeletonRecord>,
}

impl AstSkeletonArchive {
    /// Default path under repo root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root
            .join(".rgctl")
            .join("analysis")
            .join(AST_SKELETON_ARCHIVE_FILE)
    }

    /// Serialize archive to `path` (`RBAS` magic + version + bincode).
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = bincode::serialize(self)
            .map_err(|e| Error::SerdeError(format!("ast skeleton: {e}")))?;
        let mut bytes = Vec::with_capacity(8 + payload.len());
        bytes.extend_from_slice(b"RBAS");
        bytes.extend_from_slice(&AST_SKELETON_VERSION.to_le_bytes());
        bytes.extend_from_slice(&payload);
        fs::write(path, bytes)?;
        Ok(())
    }

    /// Load archive from `path`.
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)?;
        if bytes.len() < 8 || &bytes[0..4] != b"RBAS" {
            return Err(Error::SerdeError("invalid ast skeleton magic".into()));
        }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        if version != AST_SKELETON_VERSION {
            return Err(Error::SerdeError(format!(
                "unsupported ast skeleton version {version}"
            )));
        }
        bincode::deserialize(&bytes[8..])
            .map_err(|e| Error::SerdeError(format!("ast skeleton: {e}")))
    }

    /// Open default archive under `repo_root` if present.
    pub fn open_if_exists(repo_root: &Path) -> Result<Option<Self>> {
        let path = Self::default_path(repo_root);
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(Self::load_from_path(&path)?))
    }
}

/// Build a skeleton for one named function in `source`.
pub fn build_function_skeleton(
    language: &str,
    source: &str,
    function_name: &str,
    file_path: &str,
    function_id: Option<Uuid>,
) -> Result<AstSkeletonRecord> {
    let bytes = source.as_bytes();
    let tree = parse_source(language, bytes)?;
    let kinds = function_kinds_for(language)?;
    let func = find_function(tree.root_node(), bytes, function_name, kinds, language).ok_or_else(
        || {
            Error::NotFound(format!(
                "function '{function_name}' not found for AST skeleton"
            ))
        },
    )?;
    build_function_skeleton_from_tree(func, bytes, function_name, file_path, function_id)
}

/// Build a skeleton from an already-resolved function [`Node`] (no re-parse).
pub fn build_function_skeleton_from_tree(
    func: Node<'_>,
    source: &[u8],
    function_name: &str,
    file_path: &str,
    function_id: Option<Uuid>,
) -> Result<AstSkeletonRecord> {
    let mut nodes = Vec::new();
    let root_id = 0u32;
    nodes.push(AstSkeletonNode {
        id: root_id,
        parent: None,
        kind: AstSkeletonKind::Function,
        start_line: func.start_position().row + 1,
        end_line: func.end_position().row + 1,
        label: truncate_label(func.utf8_text(source).unwrap_or(function_name)),
    });
    walk_skeleton(func, source, root_id, &mut nodes, &mut 1);
    Ok(AstSkeletonRecord {
        function_id,
        function_name: function_name.to_string(),
        file_path: file_path.to_string(),
        nodes,
    })
}

fn find_function<'a>(
    node: Node<'a>,
    source: &[u8],
    name: &str,
    kinds: &[&str],
    language: &str,
) -> Option<Node<'a>> {
    if kinds.contains(&node.kind()) {
        let resolved = match language {
            "javascript" | "js" | "typescript" | "ts" => {
                ecmascript_function_symbol_name(node, source)
            }
            "puppet" => puppet_callable_name(node, source),
            _ => extract_name_from_node(node, source).ok().flatten(),
        };
        if resolved.as_deref() == Some(name) {
            return Some(node);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_function(child, source, name, kinds, language) {
            return Some(found);
        }
    }
    None
}

/// Match CFG / Function symbol naming for Puppet class/define/node/function hosts.
fn puppet_callable_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "class_identifier" | "identifier" | "node_name" | "string"
        ) {
            if let Ok(t) = child.utf8_text(source) {
                let name = t.trim_matches('\'').trim_matches('"');
                if node.kind() == "node_definition" {
                    return Some(format!("node:{name}"));
                }
                return Some(name.to_string());
            }
        }
    }
    extract_name_from_node(node, source).ok().flatten()
}

fn walk_skeleton(
    node: Node,
    source: &[u8],
    parent: u32,
    out: &mut Vec<AstSkeletonNode>,
    next_id: &mut u32,
) {
    let kind = classify(node.kind());
    let emit = kind.is_some()
        && node.kind() != "method_declaration"
        && node.kind() != "function_item"
        && node.kind() != "function_declaration"
        && node.kind() != "constructor_declaration"
        && node.kind() != "function_definition"
        && node.kind() != "method"
        && node.kind() != "singleton_method";

    let mut child_parent = parent;
    if emit {
        if let Some(k) = kind {
            let id = *next_id;
            *next_id += 1;
            out.push(AstSkeletonNode {
                id,
                parent: Some(parent),
                kind: k,
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                label: truncate_label(node.utf8_text(source).unwrap_or("")),
            });
            child_parent = id;
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_skeleton(child, source, child_parent, out, next_id);
    }
}

fn classify(kind: &str) -> Option<AstSkeletonKind> {
    Some(match kind {
        "block" | "compound_statement" | "statement_block" | "body" => AstSkeletonKind::Block,
        "if_statement" | "if_expression" | "if" | "unless" | "unless_statement"
        | "when_expression" => AstSkeletonKind::If,
        "while_statement" | "while_expression" | "for_statement" | "for_expression"
        | "loop_expression" | "do_statement" | "do_while_statement" | "foreach_statement"
        | "while" | "until" | "for" | "iterator_statement" | "case_statement" => {
            AstSkeletonKind::Loop
        }
        "call_expression" | "method_invocation" | "invocation_expression" | "function_call"
        | "call" => {
            AstSkeletonKind::Call
        }
        "assignment_expression"
        | "assignment"
        | "assignment_statement"
        | "augmented_assignment"
        | "compound_assignment_expr" => AstSkeletonKind::Assign,
        "let_declaration"
        | "let_statement"
        | "local_variable_declaration"
        | "variable_declaration"
        | "short_var_declaration"
        | "property_declaration"
        | "declaration" => AstSkeletonKind::Decl,
        _ => return None,
    })
}

fn truncate_label(s: &str) -> String {
    let one = s.lines().next().unwrap_or(s).trim();
    if one.chars().count() > 80 {
        format!("{}…", one.chars().take(79).collect::<String>())
    } else {
        one.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_skeleton_has_assign() {
        let src = r#"
public class C {
  void m(int x) {
    x = x + 1;
    foo(x);
  }
}
"#;
        let rec = build_function_skeleton("java", src, "m", "C.java", None).unwrap();
        assert!(rec.nodes.iter().any(|n| n.kind == AstSkeletonKind::Assign));
        assert!(rec.nodes.iter().any(|n| n.kind == AstSkeletonKind::Call));
    }
}
