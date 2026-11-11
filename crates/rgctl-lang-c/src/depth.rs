//! Deep C AST extraction helpers (OpenSpec 3.4).

use crate::plugin::source_location;
use rgctl_plugin_api::{Field, Symbol, SymbolType};
use rgctl_plugin_helpers::record_symbol_spans;
use tree_sitter::Node;

fn text(node: Node, source: &[u8]) -> Option<String> {
    node.utf8_text(source).ok().map(|s| s.trim().to_string())
}

/// Enumerators from `enumerator_list` → fields.
pub fn extract_enumerator_fields(enum_node: Node, source: &[u8]) -> Vec<Field> {
    let Some(body) = enum_node.child_by_field_name("body") else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "enumerator" {
            continue;
        }
        let name = child
            .child_by_field_name("name")
            .and_then(|n| text(n, source));
        if let Some(name) = name {
            fields.push(Field {
                name,
                field_type: None,
                visibility: None,
            });
        }
    }
    fields
}

/// `union U { ... }` or anonymous `typedef union { ... } Alias;`
pub fn extract_union_specifier(
    node: Node,
    source: &[u8],
    file_path: &str,
    fields: Vec<Field>,
) -> Option<Symbol> {
    extract_union_specifier_named(node, source, file_path, fields, None)
}

/// Like [`extract_union_specifier`], with optional typedef alias override.
pub fn extract_union_specifier_named(
    node: Node,
    source: &[u8],
    file_path: &str,
    fields: Vec<Field>,
    typedef_name: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "union_specifier" {
        return None;
    }
    let tag = node
        .child_by_field_name("name")
        .and_then(|n| text(n, source));
    let name = typedef_name.map(str::to_string).or(tag.clone())?;
    let mut metadata = serde_json::json!({
        "language": "c",
        "kind": "union_specifier",
    });
    if let (Some(alias), Some(tag)) = (typedef_name, tag.as_deref()) {
        if alias != tag {
            metadata["underlying_type"] = serde_json::Value::String(tag.to_string());
        }
    }
    let mut symbol = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Struct,
        qualified_name: Some(name),
        location: source_location(node, file_path),
        signature: None,
        return_type: None,
        parameters: vec![],
        fields,
        modifiers: vec![],
        documentation: None,
        metadata,
        ..Default::default()
    };
    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// `#define MAX 10`
/// Opt-in `#define` extractor (not wired into default discover — see linux cold node cap).
#[allow(dead_code)]
pub fn extract_preproc_def(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    if node.kind() != "preproc_def" {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .and_then(|n| text(n, source))?;
    let value = node
        .child_by_field_name("value")
        .and_then(|n| text(n, source));
    let mut symbol = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Macro,
        qualified_name: Some(name),
        location: source_location(node, file_path),
        signature: value.clone(),
        return_type: value,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "c",
            "kind": "preproc_def",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// `#define SQR(x) ((x)*(x))`
/// Opt-in function-like `#define` extractor (not wired into default discover).
#[allow(dead_code)]
pub fn extract_preproc_function_def(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    if node.kind() != "preproc_function_def" {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .and_then(|n| text(n, source))?;
    let value = node
        .child_by_field_name("value")
        .and_then(|n| text(n, source));
    let mut symbol = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Macro,
        qualified_name: Some(name),
        location: source_location(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: value,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "c",
            "kind": "preproc_function_def",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}
