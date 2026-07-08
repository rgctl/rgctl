//! Shared helpers for extracting `Calls` relations from tree-sitter ASTs.

use crate::{Relation, RelationType, SourceLocation, Symbol, SymbolType};
use std::path::Path;
use tree_sitter::Node;

/// Call node kinds per language grammar.
pub const RUST_CALL_KINDS: &[&str] = &["call_expression", "macro_invocation"];
pub const GO_CALL_KINDS: &[&str] = &["call_expression"];
pub const PYTHON_CALL_KINDS: &[&str] = &["call"];
pub const CSHARP_CALL_KINDS: &[&str] = &["invocation_expression"];

/// Find the innermost function symbol containing `node`.
pub fn containing_function<'a>(node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
    let line = node.start_position().row + 1;
    symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Function)
        .filter(|s| line >= s.location.start_line && line <= s.location.end_line)
        .min_by_key(|s| s.location.end_line - s.location.start_line)
}

/// Best-effort callee name from a call expression subtree.
pub fn callee_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "type_identifier" => node.utf8_text(source).ok().map(str::to_string),
        "field_expression" | "selector_expression" | "attribute" => node
            .child_by_field_name("field")
            .or_else(|| node.child_by_field_name("attribute"))
            .or_else(|| node.child_by_field_name("name"))
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string)),
        "scoped_identifier" | "qualified_type" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string)),
        "parenthesized_expression" => node
            .named_child(0)
            .and_then(|inner| callee_name(inner, source)),
        "invocation_expression" => node
            .named_child(0)
            .and_then(|n| callee_name(n, source)),
        _ => {
            if let Some(func) = node.child_by_field_name("function") {
                return callee_name(func, source);
            }
            if let Some(name) = node.child_by_field_name("name") {
                return name.utf8_text(source).ok().map(str::to_string);
            }
            None
        }
    }
}

/// Push a `Calls` relation when `node` is a recognized call site.
pub fn push_call_relation(
    node: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    call_kinds: &[&str],
    language: &str,
    relations: &mut Vec<Relation>,
) {
    if !call_kinds.contains(&node.kind()) {
        return;
    }

    let callee = node
        .child_by_field_name("function")
        .or_else(|| node.child_by_field_name("macro"))
        .or_else(|| node.child_by_field_name("name"))
        .and_then(|n| callee_name(n, source))
        .or_else(|| callee_name(node, source));

    let Some(callee) = callee else {
        return;
    };
    if callee.is_empty() {
        return;
    }

    let Some(from_fn) = containing_function(node, symbols) else {
        return;
    };

    let from = from_fn
        .qualified_name
        .clone()
        .unwrap_or_else(|| from_fn.name.clone());

    let local_target = symbols
        .iter()
        .find(|s| s.name == callee && s.symbol_type == SymbolType::Function)
        .and_then(|s| s.qualified_name.clone())
        .unwrap_or_else(|| callee.clone());

    relations.push(Relation {
        from,
        to: local_target,
        relation_type: RelationType::Calls,
        location: SourceLocation {
            file: file_path.to_string_lossy().to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        },
        metadata: serde_json::json!({ "language": language }),
        to_qualified_hint: None,
        to_type_hint: None,
    });
}

/// Depth-first walk that records call relations.
pub fn walk_calls(
    node: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    call_kinds: &[&str],
    language: &str,
    relations: &mut Vec<Relation>,
) {
    push_call_relation(
        node, source, file_path, symbols, call_kinds, language, relations,
    );
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_calls(
            child, source, file_path, symbols, call_kinds, language, relations,
        );
    }
}
