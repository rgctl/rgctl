//! Deep C++ AST extraction helpers (OpenSpec 3.4).

use crate::plugin::source_location;
use rgctl_plugin_api::{Field, Relation, RelationType, ScopeStack, Symbol, SymbolType};
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
                source_span: rgctl_plugin_helpers::field_source_span(child, source, &name),
                name,
                field_type: None,
                visibility: None,
            });
        }
    }
    fields
}

/// `using IntVec = std::vector<int>;`
pub fn extract_alias_declaration(
    node: Node,
    source: &[u8],
    file_path: &str,
    scopes: &ScopeStack,
) -> Option<Symbol> {
    if node.kind() != "alias_declaration" {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .and_then(|n| text(n, source))?;
    let aliased = node
        .child_by_field_name("type")
        .and_then(|n| text(n, source));
    let qn = scopes.qualified_name_sep(&name, "::");
    let mut symbol = Symbol {
        name,
        symbol_type: SymbolType::TypeAlias,
        qualified_name: Some(qn),
        location: source_location(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: aliased,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "cpp",
            "kind": "alias_declaration",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// `concept Addable = ...`
pub fn extract_concept_definition(
    node: Node,
    source: &[u8],
    file_path: &str,
    scopes: &ScopeStack,
) -> Option<Symbol> {
    if node.kind() != "concept_definition" {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .and_then(|n| text(n, source))?;
    let qn = scopes.qualified_name_sep(&name, "::");
    let mut symbol = Symbol {
        name,
        symbol_type: SymbolType::TypeAlias,
        qualified_name: Some(qn),
        location: source_location(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "cpp",
            "kind": "concept_definition",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// `union U { ... }` or anonymous `typedef union { ... } Alias;`
pub fn extract_union_specifier(
    node: Node,
    source: &[u8],
    file_path: &str,
    scopes: &ScopeStack,
) -> Option<Symbol> {
    extract_union_specifier_named(node, source, file_path, scopes, None)
}

/// Like [`extract_union_specifier`], with optional typedef alias override.
pub fn extract_union_specifier_named(
    node: Node,
    source: &[u8],
    file_path: &str,
    scopes: &ScopeStack,
    typedef_name: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "union_specifier" {
        return None;
    }
    let tag = node
        .child_by_field_name("name")
        .and_then(|n| text(n, source));
    let name = typedef_name.map(str::to_string).or(tag.clone())?;
    let fields = crate::plugin::extract_type_fields(node, source).unwrap_or_default();
    let qn = scopes.qualified_name_sep(&name, "::");
    let mut metadata = serde_json::json!({
        "language": "cpp",
        "kind": "union_specifier",
    });
    if let (Some(alias), Some(tag)) = (typedef_name, tag.as_deref()) {
        if alias != tag {
            metadata["underlying_type"] = serde_json::Value::String(tag.to_string());
        }
    }
    let mut symbol = Symbol {
        name,
        symbol_type: SymbolType::Struct,
        qualified_name: Some(qn),
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
            "language": "cpp",
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
            "language": "cpp",
            "kind": "preproc_function_def",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// `namespace N` → Module symbol.
pub fn extract_namespace_definition(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    if node.kind() != "namespace_definition" {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .and_then(|n| text(n, source))
        .unwrap_or_else(|| "anonymous".to_string());
    let mut symbol = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Module,
        qualified_name: Some(name),
        location: source_location(node, file_path),
        signature: None,
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "cpp",
            "kind": "namespace_definition",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// `namespace Alias = std;` → Module alias symbol.
pub fn extract_namespace_alias(
    node: Node,
    source: &[u8],
    file_path: &str,
    scopes: &ScopeStack,
) -> Option<Symbol> {
    if node.kind() != "namespace_alias_definition" {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .and_then(|n| text(n, source))?;
    let mut cursor = node.walk();
    let target = node
        .children(&mut cursor)
        .find(|c| {
            matches!(
                c.kind(),
                "namespace_identifier" | "nested_namespace_specifier" | "qualified_identifier"
            )
        })
        .and_then(|n| text(n, source));
    let qn = scopes.qualified_name_sep(&name, "::");
    let mut symbol = Symbol {
        name,
        symbol_type: SymbolType::Module,
        qualified_name: Some(qn),
        location: source_location(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: target,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "cpp",
            "kind": "namespace_alias_definition",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// `[](int x){ return x; }` → synthetic Function symbol.
pub fn extract_lambda_expression(
    node: Node,
    source: &[u8],
    file_path: &str,
    scopes: &ScopeStack,
) -> Option<Symbol> {
    if node.kind() != "lambda_expression" {
        return None;
    }
    let line = node.start_position().row + 1;
    let name = format!("$lambda${line}");
    let qn = scopes.qualified_name_sep(&name, "::");
    let mut symbol = Symbol {
        name,
        symbol_type: SymbolType::Function,
        qualified_name: Some(qn),
        location: source_location(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| {
            let t = s.trim();
            if t.len() > 120 {
                format!("{}…", &t[..117])
            } else {
                t.to_string()
            }
        }),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "cpp",
            "kind": "lambda_expression",
            "is_lambda": true,
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    symbol = rgctl_plugin_helpers::callable_identity::symbol(
        symbol,
        node,
        source,
        rgctl_plugin_helpers::callable_identity::Policy::Cpp,
    );
    Some(symbol)
}

/// `friend class Foo;` / `friend void bar();` → Uses relation.
pub fn friend_declaration_relation(node: Node, source: &[u8], file_path: &str) -> Option<Relation> {
    if node.kind() != "friend_declaration" {
        return None;
    }
    let target = node
        .utf8_text(source)
        .ok()?
        .trim()
        .trim_start_matches("friend")
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string();
    if target.is_empty() {
        return None;
    }
    let simple = target
        .rsplit([':', ' '])
        .find(|p| !p.is_empty())
        .unwrap_or(&target)
        .to_string();
    Some(Relation {
        from: file_path.to_string(),
        to: simple,
        relation_type: RelationType::Uses,
        location: source_location(node, file_path),
        metadata: serde_json::json!({
            "language": "cpp",
            "kind": "friend_declaration",
            "qualified": target,
        }),
        to_qualified_hint: Some(target),
        to_type_hint: None,
        ..Default::default()
    })
}

/// `delete p;` → Deletes-style Uses relation from enclosing context.
pub fn delete_expression_relation(
    node: Node,
    source: &[u8],
    file_path: &str,
    from: &str,
) -> Option<Relation> {
    if node.kind() != "delete_expression" {
        return None;
    }
    let arg = node
        .named_child(0)
        .and_then(|n| text(n, source))
        .unwrap_or_else(|| "object".to_string());
    Some(Relation {
        from: from.to_string(),
        to: arg,
        relation_type: RelationType::Uses,
        location: source_location(node, file_path),
        metadata: serde_json::json!({
            "language": "cpp",
            "kind": "delete_expression",
        }),
        to_qualified_hint: None,
        to_type_hint: None,
        ..Default::default()
    })
}

/// `using std::vector;` → Uses relation.
pub fn using_declaration_relation(node: Node, source: &[u8], file_path: &str) -> Option<Relation> {
    if node.kind() != "using_declaration" {
        return None;
    }
    let target = node
        .utf8_text(source)
        .ok()?
        .trim()
        .trim_start_matches("using")
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string();
    if target.is_empty() {
        return None;
    }
    let simple = target.rsplit("::").next().unwrap_or(&target).to_string();
    Some(Relation {
        from: file_path.to_string(),
        to: simple,
        relation_type: RelationType::Uses,
        location: source_location(node, file_path),
        metadata: serde_json::json!({
            "language": "cpp",
            "kind": "using_declaration",
            "qualified": target,
        }),
        to_qualified_hint: Some(target),
        to_type_hint: None,
        ..Default::default()
    })
}
