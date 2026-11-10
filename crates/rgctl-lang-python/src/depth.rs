//! Deep Python AST extraction helpers (OpenSpec 3.2).

use rgctl_plugin_api::{Field, Relation, RelationType, SourceLocation, Symbol, SymbolType};
use rgctl_plugin_helpers::record_symbol_spans;
use std::collections::HashSet;
use tree_sitter::Node;

fn loc(node: Node, file: &str) -> SourceLocation {
    SourceLocation {
        file: file.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

fn text(node: Node, source: &[u8]) -> Option<String> {
    node.utf8_text(source).ok().map(|s| s.trim().to_string())
}

/// `self.x` / `cls.x` attribute LHS → field name.
pub fn self_or_cls_attr_name(left: Node, source: &[u8]) -> Option<String> {
    if left.kind() != "attribute" {
        return None;
    }
    let object = left.child_by_field_name("object")?;
    let attr = left.child_by_field_name("attribute")?;
    let obj = object.utf8_text(source).ok()?;
    if obj != "self" && obj != "cls" {
        return None;
    }
    text(attr, source)
}

/// Push a field onto `fields` if unseen (or fill missing type).
pub fn push_field(
    fields: &mut Vec<Field>,
    seen: &mut HashSet<String>,
    name: String,
    field_type: Option<String>,
) {
    if seen.insert(name.clone()) {
        fields.push(Field {
            name,
            field_type,
            visibility: None,
        });
    } else if let Some(field) = fields.iter_mut().find(|f| f.name == name) {
        if field.field_type.is_none() {
            field.field_type = field_type;
        }
    }
}

/// Annotated assignment `x: T [= v]` name + type.
pub fn annotated_assignment_field(node: Node, source: &[u8]) -> Option<(String, Option<String>)> {
    // tree-sitter-python: `type` field on assignment, or `typed_parameter`-like patterns.
    let name = node
        .child_by_field_name("left")
        .and_then(|n| {
            if n.kind() == "identifier" {
                text(n, source)
            } else {
                None
            }
        })
        .or_else(|| {
            // Some grammars nest identifier under pattern.
            let mut c = node.walk();
            node.children(&mut c)
                .find(|ch| ch.kind() == "identifier")
                .and_then(|n| text(n, source))
        })?;
    let ty = node
        .child_by_field_name("type")
        .and_then(|n| text(n, source));
    Some((name, ty))
}

/// Class-body `name = lambda ...` binding name.
pub fn class_lambda_binding_name(node: Node, source: &[u8]) -> Option<String> {
    if node.kind() != "assignment" {
        return None;
    }
    let left = node.child_by_field_name("left")?;
    if left.kind() != "identifier" {
        return None;
    }
    let right = node.child_by_field_name("right")?;
    if right.kind() != "lambda" {
        return None;
    }
    text(left, source)
}

/// `type Point = tuple[int, int]` → TypeAlias symbol.
pub fn extract_type_alias_statement(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    let left = node.child_by_field_name("left")?;
    // Grammar wraps the name in a `type` node: `(type (identifier))`.
    let name_node = if left.kind() == "identifier" {
        Some(left)
    } else {
        let mut c = left.walk();
        left.children(&mut c)
            .find(|ch| ch.kind() == "identifier")
            .or(Some(left))
    };
    let name = name_node.and_then(|n| text(n, source))?;
    // Strip type params if present: `Point[T]` → `Point`
    let name = name.split('[').next().unwrap_or(&name).trim().to_string();
    let right = node
        .child_by_field_name("right")
        .and_then(|n| text(n, source));
    let mut meta = serde_json::json!({ "language": "python", "kind": "type_alias_statement" });
    if let Some(ref ty) = right {
        meta["aliased_type"] = serde_json::Value::String(ty.clone());
    }
    let mut sym = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::TypeAlias,
        qualified_name: Some(name),
        location: loc(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: right,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: meta,
        ..Default::default()
    };
    record_symbol_spans(&mut sym, node, name_node);
    Some(sym)
}

/// Module / local binding from identifier assignment.
pub fn extract_simple_binding(
    node: Node,
    source: &[u8],
    file_path: &str,
    kind_tag: &str,
) -> Option<Symbol> {
    let left = node.child_by_field_name("left")?;
    if left.kind() != "identifier" {
        return None;
    }
    let name = text(left, source)?;
    let mut sym = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Variable,
        qualified_name: Some(name),
        location: loc(node, file_path),
        signature: None,
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "python",
            "kind": kind_tag,
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut sym, node, Some(left));
    Some(sym)
}

/// `with open(...) as f` / `as_pattern_target` binding.
pub fn extract_as_pattern_target(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    let name_node = if node.kind() == "as_pattern_target" || node.kind() == "identifier" {
        Some(node)
    } else if node.kind() == "as_pattern" {
        node.child_by_field_name("alias")
    } else {
        None
    };
    let name = name_node.and_then(|n| text(n, source))?;
    let mut sym = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Variable,
        qualified_name: Some(name),
        location: loc(node, file_path),
        signature: None,
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "python",
            "kind": "with_as_target",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut sym, node, name_node);
    Some(sym)
}

/// True when an `if_statement` looks like `if TYPE_CHECKING:` / version guard.
pub fn is_typing_guard_if(node: Node, source: &[u8]) -> bool {
    if node.kind() != "if_statement" {
        return false;
    }
    let cond = node.child_by_field_name("condition").or_else(|| {
        let mut c = node.walk();
        node.children(&mut c).find(|ch| {
            !matches!(
                ch.kind(),
                "if" | ":" | "block" | "elif_clause" | "else_clause"
            )
        })
    });
    let Some(cond) = cond else {
        return false;
    };
    let t = cond.utf8_text(source).unwrap_or("");
    t.contains("TYPE_CHECKING") || t.contains("version_info")
}

/// True when `try` has an `except ImportError` (or ModuleNotFoundError).
pub fn is_import_error_try(node: Node, source: &[u8]) -> bool {
    if node.kind() != "try_statement" {
        return false;
    }
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "except_clause" {
            let t = n.utf8_text(source).unwrap_or("");
            if t.contains("ImportError") || t.contains("ModuleNotFoundError") {
                return true;
            }
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            stack.push(ch);
        }
    }
    false
}

/// Emit `__getitem__` / `__setitem__` Calls when subscript receiver has a type hint.
///
/// Resolves:
/// - bare `param[k]` against typed parameters on `from_fn`
/// - `self.<field>[k]` / `cls.<field>[k]` against typed [`Field`] entries on `enclosing_class`
pub fn maybe_subscript_call_relation(
    node: Node,
    source: &[u8],
    file_path: &str,
    from_fn: &Symbol,
    is_store: bool,
    enclosing_class: Option<&Symbol>,
) -> Option<Relation> {
    if node.kind() != "subscript" {
        return None;
    }
    let value = node.child_by_field_name("value")?;
    let ty = match value.kind() {
        "identifier" => {
            let recv_name = text(value, source)?;
            from_fn
                .parameters
                .iter()
                .find(|p| p.name == recv_name)
                .and_then(|p| p.param_type.as_deref())
                .map(str::to_string)?
        }
        "attribute" => {
            let obj = value.child_by_field_name("object")?;
            let attr = value.child_by_field_name("attribute")?;
            let obj_name = text(obj, source)?;
            if obj_name != "self" && obj_name != "cls" {
                return None;
            }
            let field_name = text(attr, source)?;
            enclosing_class?
                .fields
                .iter()
                .find(|f| f.name == field_name)?
                .field_type
                .clone()?
        }
        _ => return None,
    };
    let simple = ty
        .split('[')
        .next()
        .unwrap_or(ty.as_str())
        .rsplit('.')
        .next()
        .unwrap_or(ty.as_str())
        .trim();
    let method = if is_store {
        "__setitem__"
    } else {
        "__getitem__"
    };
    let from = from_fn
        .qualified_name
        .clone()
        .unwrap_or_else(|| from_fn.name.clone());
    Some(Relation {
        from,
        to: method.to_string(),
        relation_type: RelationType::Calls,
        location: loc(node, file_path),
        metadata: serde_json::json!({
            "language": "python",
            "kind": "subscript",
            "dunder": method,
        }),
        to_qualified_hint: Some(format!("{simple}.{method}")),
        to_type_hint: Some(simple.to_string()),
        ..Default::default()
    })
}

/// Whether `subscript` appears on the left of an assignment / augmented assignment (store).
pub fn subscript_is_store(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "assignment" | "augmented_assignment" => parent
            .child_by_field_name("left")
            .is_some_and(|left| left.id() == node.id()),
        _ => false,
    }
}

/// Walrus `name := expr` → Variable symbol.
pub fn extract_named_expression(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    if node.kind() != "named_expression" {
        return None;
    }
    let name_node = node.child_by_field_name("name")?;
    let name = text(name_node, source)?;
    let mut sym = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Variable,
        qualified_name: Some(name),
        location: loc(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "python",
            "kind": "named_expression",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut sym, node, Some(name_node));
    Some(sym)
}

/// Standalone / class-bound `lambda` → Function symbol.
///
/// `class_qn` prefixes the qualified name (`ClassName.lambda_name`) when the
/// lambda appears in a class body.
pub fn extract_lambda_symbol(
    node: Node,
    source: &[u8],
    file_path: &str,
    class_qn: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "lambda" {
        return None;
    }
    // Prefer binding name from `name = lambda ...` parent assignment.
    let name_node = node.parent().and_then(|p| {
        if p.kind() == "assignment" {
            p.child_by_field_name("left")
                .filter(|l| l.kind() == "identifier")
        } else {
            None
        }
    });
    let name = name_node
        .and_then(|l| text(l, source))
        .unwrap_or_else(|| format!("<lambda@{}>", node.start_position().row + 1));
    let qualified_name = match class_qn {
        Some(c) if !c.is_empty() => Some(format!("{c}.{name}")),
        _ => Some(name.clone()),
    };
    let mut sym = Symbol {
        name,
        symbol_type: SymbolType::Function,
        qualified_name,
        location: loc(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "python",
            "kind": "lambda",
        }),
        ..Default::default()
    };
    record_symbol_spans(&mut sym, node, name_node);
    sym = rgctl_plugin_helpers::callable_identity::symbol(
        sym,
        node,
        source,
        rgctl_plugin_helpers::callable_identity::Policy::Python,
    );
    Some(sym)
}
