//! Shared Python module, heritage, and decorator extraction helpers.

use rgctl_plugin_api::{Relation, RelationType, SourceLocation, Symbol, SymbolType};
use tree_sitter::Node;

/// Extract `Import` symbols from `import_statement` and `import_from_statement`.
pub fn extract_import_symbols(
    node: Node,
    source: &[u8],
    file_path: &str,
    language: &str,
) -> Vec<Symbol> {
    match node.kind() {
        "import_statement" => extract_import_statement(node, source, file_path, language),
        "import_from_statement" => extract_import_from_statement(node, source, file_path, language),
        _ => Vec::new(),
    }
}

fn extract_import_statement(
    node: Node,
    source: &[u8],
    file_path: &str,
    language: &str,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "dotted_name" | "aliased_import" => {
                if let Some((module, binding)) = import_binding(child, source) {
                    symbols.push(import_symbol(
                        &module,
                        child,
                        file_path,
                        language,
                        false,
                        Some(&module),
                        binding.as_deref(),
                    ));
                }
            }
            "wildcard_import" => {
                symbols.push(import_symbol(
                    "*",
                    child,
                    file_path,
                    language,
                    false,
                    None,
                    Some("*"),
                ));
            }
            _ => {}
        }
    }
    symbols
}

fn extract_import_from_statement(
    node: Node,
    source: &[u8],
    file_path: &str,
    language: &str,
) -> Vec<Symbol> {
    let module = node
        .child_by_field_name("module_name")
        .and_then(|n| module_text(n, source));
    let is_relative = module.as_deref().is_some_and(|m| m.starts_with('.'));
    let module = module.unwrap_or_default();
    let mut symbols = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "dotted_name" | "aliased_import" => {
                if let Some((binding_module, binding)) = import_binding(child, source) {
                    let effective_module = if module.is_empty() {
                        binding_module
                    } else {
                        module.clone()
                    };
                    let name = if let Some(b) = &binding {
                        format!("{effective_module}:{b}")
                    } else {
                        effective_module.clone()
                    };
                    symbols.push(import_symbol(
                        &name,
                        child,
                        file_path,
                        language,
                        is_relative,
                        Some(&effective_module),
                        binding.as_deref(),
                    ));
                }
            }
            "wildcard_import" => {
                let name = if module.is_empty() {
                    "*".to_string()
                } else {
                    format!("{module}:*")
                };
                symbols.push(import_symbol(
                    &name,
                    child,
                    file_path,
                    language,
                    is_relative,
                    Some(&module),
                    Some("*"),
                ));
            }
            _ => {}
        }
    }
    symbols
}

fn import_binding(node: Node, source: &[u8]) -> Option<(String, Option<String>)> {
    match node.kind() {
        "dotted_name" => {
            let text = node.utf8_text(source).ok()?.trim().to_string();
            if text.is_empty() {
                return None;
            }
            let binding = text.rsplit('.').next().unwrap_or(&text).to_string();
            Some((text, Some(binding)))
        }
        "aliased_import" => {
            let mut name = None;
            let mut alias = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "dotted_name" => name = child.utf8_text(source).ok().map(str::to_string),
                    "identifier" => alias = child.utf8_text(source).ok().map(str::to_string),
                    _ => {}
                }
            }
            let module = name?;
            let binding = alias.or_else(|| module.rsplit('.').next().map(str::to_string));
            Some((module, binding))
        }
        _ => None,
    }
}

fn module_text(node: Node, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn import_symbol(
    name: &str,
    node: Node,
    file_path: &str,
    language: &str,
    is_relative: bool,
    module: Option<&str>,
    binding: Option<&str>,
) -> Symbol {
    let mut meta = serde_json::json!({ "language": language });
    if is_relative {
        meta["is_relative"] = serde_json::Value::Bool(true);
    }
    if let Some(module) = module {
        meta["module"] = serde_json::Value::String(module.to_string());
    }
    if let Some(binding) = binding {
        meta["binding"] = serde_json::Value::String(binding.to_string());
    }
    let mut sym = Symbol {
        name: name.to_string(),
        symbol_type: SymbolType::Import,
        qualified_name: None,
        location: source_location(node, file_path),
        signature: None,
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: meta,
        ..Default::default()
    };
    crate::spans::record_symbol_spans(&mut sym, node, None);
    sym
}

/// `Extends` edges from `class_definition` superclasses.
pub fn extract_class_extends_relations(
    class_node: Node,
    source: &[u8],
    file_path: &str,
    language: &str,
) -> Vec<Relation> {
    if class_node.kind() != "class_definition" {
        return Vec::new();
    }
    let Some(class_name) = class_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)
    else {
        return Vec::new();
    };

    let Some(superclasses) = class_node.child_by_field_name("superclasses") else {
        return Vec::new();
    };

    let mut relations = Vec::new();
    collect_base_relations(
        superclasses,
        source,
        file_path,
        language,
        &class_name,
        &mut relations,
    );
    relations
}

fn collect_base_relations(
    node: Node,
    source: &[u8],
    file_path: &str,
    language: &str,
    class_name: &str,
    relations: &mut Vec<Relation>,
) {
    match node.kind() {
        "identifier" | "attribute" | "dotted_name" => {
            if let Some(base) = python_simple_type_name(node, source) {
                push_extends(
                    class_name, &base, node, source, file_path, language, None, relations,
                );
            }
        }
        "subscript" => {
            let generic_text = node.utf8_text(source).ok().map(str::to_string);
            if let Some(value) = node.child_by_field_name("value")
                && let Some(base) = python_simple_type_name(value, source)
            {
                push_extends(
                    class_name,
                    &base,
                    node,
                    source,
                    file_path,
                    language,
                    generic_text.as_deref(),
                    relations,
                );
            }
        }
        "argument_list" | "tuple" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "," {
                    collect_base_relations(
                        child, source, file_path, language, class_name, relations,
                    );
                }
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_base_relations(child, source, file_path, language, class_name, relations);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_extends(
    class_name: &str,
    base: &str,
    node: Node,
    source: &[u8],
    file_path: &str,
    language: &str,
    generic_args: Option<&str>,
    relations: &mut Vec<Relation>,
) {
    let _ = source;
    let mut meta = serde_json::json!({ "language": language });
    if let Some(generic) = generic_args {
        meta["generic_args"] = serde_json::Value::String(generic.to_string());
    }
    relations.push(Relation {
        from: class_name.to_string(),
        to: base.to_string(),
        relation_type: RelationType::Extends,
        location: source_location(node, file_path),
        metadata: meta,
        to_qualified_hint: Some(base.to_string()),
        to_type_hint: None,
        ..Default::default()
    });
}

/// Decorator names (and optional argument text) attached to a class or function node.
pub fn decorators_for_node(node: Node, source: &[u8]) -> Vec<(String, Option<String>)> {
    if let Some(parent) = node.parent() {
        if parent.kind() == "decorated_definition" {
            return collect_decorator_children(parent, source);
        }
        // TypeScript/JavaScript: method/property decorators are often preceding
        // siblings inside `class_body` / `export_statement`, not children of the
        // method node itself.
        let sibling_decs = collect_preceding_sibling_decorators(node, parent, source);
        if !sibling_decs.is_empty() {
            let mut child_decs = collect_decorator_children(node, source);
            if child_decs.is_empty() {
                return sibling_decs;
            }
            let mut out = sibling_decs;
            out.append(&mut child_decs);
            return out;
        }
    }
    collect_decorator_children(node, source)
}

fn collect_decorator_children(node: Node, source: &[u8]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "decorator"
            && let Some(parsed) = decorator_name_and_args(child, source)
        {
            out.push(parsed);
        }
    }
    out
}

fn collect_preceding_sibling_decorators(
    node: Node,
    parent: Node,
    source: &[u8],
) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut pending: Vec<(String, Option<String>)> = Vec::new();
    // Index-based walk — safe when the caller already holds a TreeCursor.
    let target_start = node.start_byte();
    let target_end = node.end_byte();
    for i in 0..parent.child_count() {
        let Some(child) = parent.child(i) else {
            continue;
        };
        // Match by byte range — more stable than `Node::id()` under concurrent cursors.
        if child.start_byte() == target_start
            && child.end_byte() == target_end
            && child.kind() == node.kind()
        {
            out.append(&mut pending);
            break;
        }
        if child.kind() == "decorator" {
            if let Some(parsed) = decorator_name_and_args(child, source) {
                pending.push(parsed);
            }
        } else if !child.is_extra()
            && child.kind() != "comment"
            && child.kind() != ";"
            && child.is_named()
        {
            pending.clear();
        }
    }
    out
}

/// Emit `AnnotatedWith` relations for decorators on a decorated symbol.
pub fn extract_decorator_relations(
    node: Node,
    source: &[u8],
    file_path: &str,
    language: &str,
    relations: &mut Vec<Relation>,
) {
    let from = match node.kind() {
        "class_definition" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string),
        "function_definition" => {
            let method = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string);
            method.map(|m| {
                containing_class_name(node, source)
                    .map(|owner| format!("{owner}.{m}"))
                    .unwrap_or(m)
            })
        }
        _ => None,
    };

    if let Some(from) = from {
        for (decorator_name, args) in decorators_for_node(node, source) {
            let mut meta = serde_json::json!({ "language": language });
            if let Some(args) = args {
                meta["arguments"] = serde_json::Value::String(args);
            }
            relations.push(Relation {
                from: from.clone(),
                to: decorator_name,
                relation_type: RelationType::AnnotatedWith,
                location: source_location(node, file_path),
                metadata: meta,
                to_qualified_hint: None,
                to_type_hint: None,
                ..Default::default()
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_decorator_relations(child, source, file_path, language, relations);
    }
}

pub fn containing_class_name(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "class_definition" {
            return parent
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string);
        }
        current = parent;
    }
    None
}

pub fn decorator_name_and_args(decorator: Node, source: &[u8]) -> Option<(String, Option<String>)> {
    let inner = decorator.named_child(0)?;
    match inner.kind() {
        "identifier" | "attribute" | "dotted_name" => {
            Some((python_simple_type_name(inner, source)?, None))
        }
        // Python uses `call`; TypeScript/JavaScript use `call_expression`.
        "call" | "call_expression" => {
            let name = inner
                .child_by_field_name("function")
                .and_then(|f| decorator_callee_name(f, source))?;
            let args = inner
                .child_by_field_name("arguments")
                .and_then(|a| a.utf8_text(source).ok().map(str::to_string));
            Some((name, args))
        }
        _ => None,
    }
}

fn decorator_callee_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "attribute" | "dotted_name" => python_simple_type_name(node, source),
        _ => node.utf8_text(source).ok().map(|s| {
            s.split('(')
                .next()
                .unwrap_or(s)
                .trim()
                .rsplit('.')
                .next()
                .unwrap_or(s)
                .to_string()
        }),
    }
}

pub fn python_simple_type_name(node: Node, source: &[u8]) -> Option<String> {
    let raw = node.utf8_text(source).ok()?.trim();
    if raw.is_empty() {
        return None;
    }
    let base = raw.split('[').next().unwrap_or(raw).trim();
    let simple = base.rsplit('.').next().unwrap_or(base).trim();
    if simple.is_empty() {
        None
    } else {
        Some(simple.to_string())
    }
}

pub fn source_location(node: Node, file_path: &str) -> SourceLocation {
    SourceLocation {
        file: file_path.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_py(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn import_from_extracts_module_and_binding() {
        let source = "from app.repositories.order import OrderRepository";
        let tree = parse_py(source);
        let root = tree.root_node();
        let mut cursor = root.walk();
        let import = root
            .children(&mut cursor)
            .find(|c| c.kind() == "import_from_statement")
            .unwrap();
        let symbols = extract_import_symbols(import, source.as_bytes(), "svc.py", "python");
        assert!(!symbols.is_empty());
        assert!(symbols.iter().all(|s| s.symbol_type == SymbolType::Import));
        assert!(
            symbols
                .iter()
                .any(|s| s.metadata.get("module").and_then(|v| v.as_str())
                    == Some("app.repositories.order")),
            "{symbols:?}"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.metadata.get("binding").and_then(|v| v.as_str())
                    == Some("OrderRepository")),
            "{symbols:?}"
        );
    }

    #[test]
    fn import_statement_alias() {
        let source = "import sqlalchemy.orm as orm";
        let tree = parse_py(source);
        let root = tree.root_node();
        let mut cursor = root.walk();
        let import = root
            .children(&mut cursor)
            .find(|c| c.kind() == "import_statement")
            .unwrap();
        let symbols = extract_import_symbols(import, source.as_bytes(), "svc.py", "python");
        assert!(
            symbols
                .iter()
                .any(|s| s.metadata.get("binding").and_then(|v| v.as_str()) == Some("orm")),
            "{symbols:?}"
        );
    }

    #[test]
    fn class_extends_generic_base() {
        let source = "class OrderRepository(BaseRepository[Order]): pass";
        let tree = parse_py(source);
        let root = tree.root_node();
        let mut cursor = root.walk();
        let class = root
            .children(&mut cursor)
            .find(|c| c.kind() == "class_definition")
            .unwrap();
        let relations =
            extract_class_extends_relations(class, source.as_bytes(), "repo.py", "python");
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Extends && r.to == "BaseRepository"),
            "{relations:?}"
        );
        assert!(
            relations
                .iter()
                .any(|r| r.metadata.get("generic_args").is_some()),
            "{relations:?}"
        );
    }

    #[test]
    fn decorator_call_with_args() {
        let source = r#"
@router.get("", response_model=list[OrderResponse])
def list_orders():
    pass
"#;
        let tree = parse_py(source);
        let root = tree.root_node();
        let mut cursor = root.walk();
        let func = root
            .children(&mut cursor)
            .find(|c| c.kind() == "decorated_definition")
            .and_then(|d| {
                let mut c = d.walk();
                d.children(&mut c)
                    .find(|ch| ch.kind() == "function_definition")
            })
            .unwrap();
        let decs = decorators_for_node(func, source.as_bytes());
        assert!(decs.iter().any(|(n, _)| n == "get"), "{decs:?}");
        assert!(
            decs[0]
                .1
                .as_ref()
                .is_some_and(|a| a.contains("response_model"))
        );
    }
}
