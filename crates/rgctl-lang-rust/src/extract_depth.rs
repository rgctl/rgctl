//! Deep extraction: module graph, trait heritage, attributes, instantiates, references.

use rgctl_plugin_api::*;
use std::path::Path;
use tree_sitter::Node;

/// Maximum AST depth for iterative walks (rustc UI tests include ~2k nested `if`s).
pub const AST_MAX_DEPTH: usize = 2048;

/// Push children onto a depth-tagged stack (pre-order via LIFO + reverse iteration).
pub fn push_children<'a>(stack: &mut Vec<(Node<'a>, usize)>, node: Node<'a>, depth: usize) {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    for child in children.into_iter().rev() {
        stack.push((child, depth + 1));
    }
}

/// `src/services/order.rs` → `services::order`
pub fn module_prefix_from_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    let normalized = path_str.replace('\\', "/");
    let after = normalized
        .find("/src/")
        .map(|i| &normalized[i + 5..])
        .or_else(|| normalized.strip_prefix("src/"))?;
    let stem = after.strip_suffix(".rs")?;
    if stem.is_empty() || stem == "mod" || stem == "lib" {
        return None;
    }
    Some(stem.replace('/', "::"))
}

pub fn qualify_with_module(prefix: Option<&str>, local: &str) -> String {
    match prefix {
        Some(p) if !p.is_empty() => format!("{p}::{local}"),
        _ => local.to_string(),
    }
}

pub fn rust_type_name(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node;
    for _ in 0..AST_MAX_DEPTH {
        match current.kind() {
            "type_identifier" | "identifier" | "primitive_type" => {
                return current.utf8_text(source).ok().map(str::to_string);
            }
            "scoped_type_identifier" | "scoped_identifier" => {
                return current
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string)
                    .or_else(|| {
                        current
                            .utf8_text(source)
                            .ok()
                            .map(|s| s.rsplit("::").next().unwrap_or(s).trim().to_string())
                    });
            }
            "generic_type" | "reference_type" | "pointer_type" | "tuple_type" => {
                current = current.child_by_field_name("type")?;
            }
            _ => return None,
        }
    }
    None
}

fn loc(node: Node, file: &str) -> SourceLocation {
    SourceLocation {
        file: file.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

fn push_relation(
    relations: &mut Vec<Relation>,
    from: &str,
    to: &str,
    relation_type: RelationType,
    location: SourceLocation,
    extra: serde_json::Value,
) {
    relations.push(Relation {
        from: from.to_string(),
        to: to.to_string(),
        relation_type,
        location,
        metadata: extra,
        to_qualified_hint: None,
        to_type_hint: None,
        ..Default::default()
    });
}

/// Walk `use_declaration` and emit `Import` symbols.
pub fn extract_use_symbols(node: Node, source: &[u8], file_path: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "use_as_clause" => {
                let path = n
                    .child_by_field_name("path")
                    .or_else(|| n.child_by_field_name("name"))
                    .and_then(|p| use_path_text(p, source));
                let alias = n
                    .child_by_field_name("alias")
                    .and_then(|a| a.utf8_text(source).ok())
                    .map(str::to_string);
                if let Some(module_path) = path {
                    let binding = alias.clone().unwrap_or_else(|| {
                        module_path
                            .rsplit("::")
                            .next()
                            .unwrap_or(&module_path)
                            .to_string()
                    });
                    out.push(import_symbol(&binding, &module_path, alias, n, file_path));
                }
            }
            "use_wildcard" => {
                let mut symbol = Symbol {
                    name: "*".to_string(),
                    symbol_type: SymbolType::Import,
                    qualified_name: Some("*".to_string()),
                    location: loc(n, file_path),
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({
                        "language": "rust",
                        "module_path": "*",
                        "is_glob": true,
                    }),
                    ..Default::default()
                };
                rgctl_plugin_helpers::record_symbol_spans(&mut symbol, n, None);
                out.push(symbol);
            }
            "identifier" | "type_identifier" | "scoped_identifier" | "scoped_type_identifier"
                if n.parent().is_some_and(|p| {
                    matches!(p.kind(), "use_list" | "scoped_use_list" | "use_declaration")
                }) =>
            {
                if let Some(module_path) = use_path_text(n, source) {
                    let binding = module_path
                        .rsplit("::")
                        .next()
                        .unwrap_or(&module_path)
                        .to_string();
                    out.push(import_symbol(&binding, &module_path, None, n, file_path));
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            stack.push(ch);
        }
    }
    out
}

fn use_path_text(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "type_identifier" => node.utf8_text(source).ok().map(str::to_string),
        "scoped_identifier" | "scoped_type_identifier" => node.utf8_text(source).ok().map(|s| {
            s.trim()
                .trim_end_matches("::{")
                .trim_end_matches('{')
                .to_string()
        }),
        _ => node.utf8_text(source).ok().map(str::to_string),
    }
}

fn import_symbol(
    binding: &str,
    module_path: &str,
    alias: Option<String>,
    node: Node,
    file_path: &str,
) -> Symbol {
    let name_node = node.child_by_field_name("alias").or_else(|| {
        if matches!(
            node.kind(),
            "identifier" | "type_identifier" | "scoped_identifier" | "scoped_type_identifier"
        ) {
            Some(node)
        } else {
            None
        }
    });
    let mut symbol = Symbol {
        name: binding.to_string(),
        symbol_type: SymbolType::Import,
        qualified_name: Some(module_path.to_string()),
        location: loc(node, file_path),
        signature: Some(module_path.to_string()),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "rust",
            "module_path": module_path,
            "import_alias": alias,
        }),
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, name_node);
    symbol
}

pub fn extract_mod_symbol(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)?;
    let mut symbol = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Module,
        qualified_name: Some(name),
        location: loc(node, file_path),
        signature: None,
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({ "language": "rust", "mod_declaration": true }),
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

pub fn extract_trait_symbols(
    node: Node,
    source: &[u8],
    file_path: &str,
    module_prefix: Option<&str>,
) -> Vec<Symbol> {
    let mut out = Vec::new();
    let name_node = node
        .child_by_field_name("name")
        .or_else(|| node.child(0).filter(|c| c.kind() == "type_identifier"))
        .or_else(|| {
            let mut c = node.walk();
            node.children(&mut c)
                .find(|ch| ch.kind() == "type_identifier")
        });
    let trait_name = name_node
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string);
    let Some(trait_name) = trait_name else {
        return out;
    };
    let qn = qualify_with_module(module_prefix, &trait_name);
    let mut symbol = Symbol {
        name: trait_name.clone(),
        symbol_type: SymbolType::Interface,
        qualified_name: Some(qn.clone()),
        location: loc(node, file_path),
        signature: None,
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({ "language": "rust", "is_trait": true }),
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, name_node);
    out.push(symbol);

    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "function_signature_item" | "function_item" => {
                if let Some(method) = extract_trait_method(n, source, file_path, &trait_name, &qn) {
                    out.push(method);
                }
                continue;
            }
            "associated_type" => {
                if let Some(at) = extract_associated_type(n, source, file_path, Some(&qn)) {
                    out.push(at);
                }
                continue;
            }
            _ => {}
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            stack.push(ch);
        }
    }
    out
}

/// `type Foo = Bar;` → [`SymbolType::TypeAlias`].
pub fn extract_type_item(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)?;
    let aliased = node
        .child_by_field_name("type")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim().to_string());
    let module_prefix = module_prefix_from_path(Path::new(file_path));
    let qn = qualify_with_module(module_prefix.as_deref(), &name);
    let mut meta = serde_json::json!({ "language": "rust", "kind": "type_item" });
    if let Some(ref ty) = aliased {
        meta["aliased_type"] = serde_json::Value::String(ty.clone());
    }
    let mut symbol = Symbol {
        name,
        symbol_type: SymbolType::TypeAlias,
        qualified_name: Some(qn),
        location: loc(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: aliased,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: meta,
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// `macro_rules! name { ... }` → [`SymbolType::Macro`].
pub fn extract_macro_definition(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)?;
    let module_prefix = module_prefix_from_path(Path::new(file_path));
    let qn = qualify_with_module(module_prefix.as_deref(), &name);
    let mut symbol = Symbol {
        name,
        symbol_type: SymbolType::Macro,
        qualified_name: Some(qn),
        location: loc(node, file_path),
        signature: Some("macro_rules!".to_string()),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({ "language": "rust", "kind": "macro_definition" }),
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// `extern crate foo;` / `extern crate foo as bar;` → Import symbol.
pub fn extract_extern_crate(node: Node, source: &[u8], file_path: &str) -> Option<Symbol> {
    if node.kind() != "extern_crate_declaration" {
        return None;
    }
    let name = node
        .child_by_field_name("name")
        .or_else(|| {
            let mut c = node.walk();
            node.children(&mut c)
                .find(|ch| matches!(ch.kind(), "identifier" | "crate"))
        })
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)?;
    let alias = node
        .child_by_field_name("alias")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string);
    let binding = alias.clone().unwrap_or_else(|| name.clone());
    let mut meta = serde_json::json!({
        "language": "rust",
        "kind": "extern_crate_declaration",
        "module_path": name,
    });
    if let Some(a) = alias {
        meta["alias"] = serde_json::Value::String(a);
    }
    let mut symbol = Symbol {
        name: binding.clone(),
        symbol_type: SymbolType::Import,
        qualified_name: Some(binding),
        location: loc(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: meta,
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// Trait / impl associated type (`type Item;`).
pub fn extract_associated_type(
    node: Node,
    source: &[u8],
    file_path: &str,
    owner_qn: Option<&str>,
) -> Option<Symbol> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)?;
    let qn = match owner_qn {
        Some(owner) => format!("{owner}::{name}"),
        None => {
            let module_prefix = module_prefix_from_path(Path::new(file_path));
            qualify_with_module(module_prefix.as_deref(), &name)
        }
    };
    let mut symbol = Symbol {
        name,
        symbol_type: SymbolType::TypeAlias,
        qualified_name: Some(qn),
        location: loc(node, file_path),
        signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "rust",
            "kind": "associated_type",
            "associated": true,
        }),
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

/// Standalone enum variant symbol (`Status::Active`).
pub fn extract_enum_variant_symbol(
    node: Node,
    source: &[u8],
    file_path: &str,
    enum_name: &str,
    enum_qn: &str,
) -> Option<Symbol> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)?;
    let mut symbol = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Variable,
        qualified_name: Some(format!("{enum_qn}::{name}")),
        location: loc(node, file_path),
        signature: None,
        return_type: Some(enum_name.to_string()),
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "rust",
            "kind": "enum_variant",
            "enum": enum_name,
            "is_enum_variant": true,
        }),
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

fn extract_trait_method(
    node: Node,
    source: &[u8],
    file_path: &str,
    trait_name: &str,
    trait_qn: &str,
) -> Option<Symbol> {
    let name = node
        .child_by_field_name("name")
        .or_else(|| {
            let mut c = node.walk();
            node.children(&mut c).find(|ch| ch.kind() == "identifier")
        })
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)?;
    let mut symbol = Symbol {
        name: name.clone(),
        symbol_type: SymbolType::Function,
        qualified_name: Some(format!("{trait_qn}::{name}")),
        location: loc(node, file_path),
        signature: node
            .utf8_text(source)
            .ok()
            .map(|s| s.lines().next().unwrap_or(s).trim().to_string()),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "rust",
            "trait_method": true,
            "receiver_type": trait_name,
        }),
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
    Some(symbol)
}

pub fn emit_impl_relations(
    node: Node,
    source: &[u8],
    file_path: &str,
    symbols: &[Symbol],
    relations: &mut Vec<Relation>,
) {
    if node.kind() != "impl_item" {
        return;
    }
    let type_name = node
        .child_by_field_name("type")
        .and_then(|n| rust_type_name(n, source));
    let Some(type_name) = type_name else {
        return;
    };
    let trait_name = node
        .child_by_field_name("trait")
        .and_then(|n| rust_type_name(n, source));
    let Some(trait_name) = trait_name else {
        return;
    };
    let from = symbols
        .iter()
        .find(|s| {
            matches!(s.symbol_type, SymbolType::Struct | SymbolType::Enum) && s.name == type_name
        })
        .and_then(|s| s.qualified_name.clone())
        .unwrap_or(type_name);
    push_relation(
        relations,
        &from,
        &trait_name,
        RelationType::Implements,
        loc(node, file_path),
        serde_json::json!({ "language": "rust" }),
    );
}

pub fn emit_item_attributes(
    item: Node,
    owner: &str,
    source: &[u8],
    file_path: &str,
    relations: &mut Vec<Relation>,
) {
    // Outer attributes are often siblings before the item (not children).
    let mut sib = Some(item);
    while let Some(node) = sib {
        if node.kind() == "attribute_item" {
            push_attribute_relation(node, owner, source, file_path, relations);
        } else if node != item
            && !matches!(
                node.kind(),
                "line_comment" | "block_comment" | "inner_attribute_item"
            )
        {
            break;
        }
        sib = node.prev_sibling();
    }

    let mut stack = vec![item];
    while let Some(n) = stack.pop() {
        if n != item && n.kind() == "attribute_item" {
            push_attribute_relation(n, owner, source, file_path, relations);
        }
        if n != item
            && matches!(
                n.kind(),
                "function_item" | "struct_item" | "enum_item" | "impl_item" | "trait_item"
            )
        {
            continue;
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            stack.push(ch);
        }
    }
}

fn push_attribute_relation(
    attr_item: Node,
    owner: &str,
    source: &[u8],
    file_path: &str,
    relations: &mut Vec<Relation>,
) {
    if let Some(attr_name) = attribute_path(attr_item, source) {
        let args = attribute_args_text(attr_item, source);
        let mut meta = serde_json::json!({ "language": "rust" });
        if let Some(a) = args {
            meta["arguments"] = serde_json::Value::String(a);
        }
        push_relation(
            relations,
            owner,
            &attr_name,
            RelationType::AnnotatedWith,
            loc(attr_item, file_path),
            meta,
        );
    }
}

fn attribute_path(attr_item: Node, source: &[u8]) -> Option<String> {
    let mut stack = vec![attr_item];
    while let Some(n) = stack.pop() {
        if n.kind() == "attribute" {
            if let Some(path) = n.child_by_field_name("name") {
                return Some(attribute_path_text(path, source));
            }
            let mut c = n.walk();
            for ch in n.children(&mut c) {
                if matches!(ch.kind(), "identifier" | "scoped_identifier") {
                    return Some(attribute_path_text(ch, source));
                }
            }
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            stack.push(ch);
        }
    }
    None
}

fn attribute_path_text(node: Node, source: &[u8]) -> String {
    match node.kind() {
        "identifier" => node
            .utf8_text(source)
            .map(|s| s.to_string())
            .unwrap_or_default(),
        "scoped_identifier" => node
            .utf8_text(source)
            .ok()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        _ => {
            if let Some(id) = node.child_by_field_name("name") {
                return attribute_path_text(id, source);
            }
            node.utf8_text(source)
                .map(|s| s.to_string())
                .unwrap_or_default()
        }
    }
}

fn attribute_args_text(attr_item: Node, source: &[u8]) -> Option<String> {
    let text = attr_item.utf8_text(source).ok()?;
    let inner = text
        .trim_start_matches('#')
        .trim_start_matches('[')
        .trim_end_matches(']');
    Some(inner.to_string())
}

/// Emit depth relations for one CST node.
///
/// When `from_fn` is `Some`, it is used for Instantiates/References (single-pass
/// `ScopeStack` path). When `None`, falls back to a line-range scan of `symbols`.
pub fn visit_depth_relation_node(
    node: Node,
    source: &[u8],
    file: &str,
    symbols: &[Symbol],
    from_fn: Option<&str>,
    relations: &mut Vec<Relation>,
) {
    if node.kind() == "impl_item" {
        emit_impl_relations(node, source, file, symbols, relations);
    }

    if matches!(
        node.kind(),
        "struct_item" | "enum_item" | "function_item" | "impl_item" | "trait_item"
    ) && let Some(owner) = item_owner_name(node, source, symbols)
    {
        emit_item_attributes(node, &owner, source, file, relations);
    }

    let from = from_fn
        .map(str::to_string)
        .or_else(|| enclosing_function_name(node, symbols));

    match node.kind() {
        "struct_expression" => {
            if let Some(ref from) = from
                && let Some(ty) = node
                    .child_by_field_name("name")
                    .and_then(|n| rust_type_name(n, source))
            {
                push_relation(
                    relations,
                    from,
                    &ty,
                    RelationType::Instantiates,
                    loc(node, file),
                    serde_json::json!({ "language": "rust" }),
                );
            }
        }
        "call_expression" | "macro_invocation" => {
            if let Some(ref from) = from
                && let Some(ty) = instantiates_from_call(node, source)
            {
                push_relation(
                    relations,
                    from,
                    &ty,
                    RelationType::Instantiates,
                    loc(node, file),
                    serde_json::json!({ "language": "rust" }),
                );
            }
        }
        "field_expression" => {
            if !is_assignment_lhs(node)
                && let Some(ref from) = from
                && let Some(field) = node
                    .child_by_field_name("field")
                    .and_then(|n| n.utf8_text(source).ok())
            {
                push_relation(
                    relations,
                    from,
                    field,
                    RelationType::References,
                    loc(node, file),
                    serde_json::json!({ "language": "rust" }),
                );
            }
        }
        _ => {}
    }
}

pub fn walk_depth_relations(
    root: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    relations: &mut Vec<Relation>,
) {
    let file = file_path.to_string_lossy();
    let mut stack = vec![(root, 0usize)];
    let mut depth_warned = false;

    while let Some((node, depth)) = stack.pop() {
        if depth > AST_MAX_DEPTH {
            if !depth_warned {
                tracing::warn!(
                    file = %file,
                    depth = AST_MAX_DEPTH,
                    "AST depth limit exceeded during depth relation walk; skipping deep branches"
                );
                depth_warned = true;
            }
            continue;
        }

        visit_depth_relation_node(node, source, &file, symbols, None, relations);
        push_children(&mut stack, node, depth);
    }
}

fn item_owner_name(node: Node, source: &[u8], symbols: &[Symbol]) -> Option<String> {
    let line = node.start_position().row + 1;
    if let Some(sym) = symbols.iter().find(|s| {
        s.location.start_line == line
            && matches!(
                s.symbol_type,
                SymbolType::Function
                    | SymbolType::Struct
                    | SymbolType::Enum
                    | SymbolType::Interface
            )
    }) {
        return sym
            .qualified_name
            .clone()
            .or_else(|| Some(sym.name.clone()));
    }
    match node.kind() {
        "struct_item" | "enum_item" => node
            .child_by_field_name("name")
            .or_else(|| {
                let mut c = node.walk();
                node.children(&mut c)
                    .find(|ch| ch.kind() == "type_identifier")
            })
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string),
        "function_item" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string),
        "trait_item" => node
            .child_by_field_name("name")
            .or_else(|| {
                let mut c = node.walk();
                node.children(&mut c)
                    .find(|ch| ch.kind() == "type_identifier")
            })
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string),
        "impl_item" => node
            .child_by_field_name("type")
            .and_then(|n| rust_type_name(n, source)),
        _ => None,
    }
}

fn enclosing_function_name(node: Node, symbols: &[Symbol]) -> Option<String> {
    let line = node.start_position().row + 1;
    symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Function)
        .filter(|s| line >= s.location.start_line && line <= s.location.end_line)
        .min_by_key(|s| s.location.end_line - s.location.start_line)
        .map(|s| s.qualified_name.clone().unwrap_or_else(|| s.name.clone()))
}

fn instantiates_from_call(node: Node, source: &[u8]) -> Option<String> {
    let func = node
        .child_by_field_name("function")
        .or_else(|| node.child_by_field_name("macro"))?;
    match func.kind() {
        "scoped_identifier" | "scoped_type_identifier" => {
            let path = func
                .child_by_field_name("path")
                .and_then(|n| rust_type_name(n, source));
            let name = func
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string);
            match (path.as_deref(), name.as_deref()) {
                (Some(p), Some("new") | Some("default")) => Some(p.to_string()),
                (None, Some("default")) => None,
                (Some(p), _) => Some(p.to_string()),
                _ => None,
            }
        }
        "identifier" => {
            let name = func.utf8_text(source).ok()?;
            if name == "default" {
                None
            } else {
                Some(name.to_string())
            }
        }
        _ => rust_type_name(func, source),
    }
}

fn is_assignment_lhs(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() == "assignment_expression" || parent.kind() == "compound_assignment_expr" {
        return parent
            .child_by_field_name("left")
            .is_some_and(|l| l.id() == node.id());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn module_prefix_from_src_path() {
        let p = Path::new("/app/src/services/order.rs");
        assert_eq!(
            module_prefix_from_path(p).as_deref(),
            Some("services::order")
        );
    }

    #[test]
    fn use_declaration_imports() {
        let src = "use crate::services::order;\nuse uuid::Uuid as Id;";
        let tree = parse(src);
        let imports = extract_use_symbols(tree.root_node(), src.as_bytes(), "lib.rs");
        assert!(imports.len() >= 2, "{imports:?}");
        assert!(imports.iter().any(|i| i.name == "order"));
        assert!(imports.iter().any(|i| i.name == "Id"));
    }

    #[test]
    fn impl_trait_emits_implements() {
        let src = "impl Default for Foo {}";
        let tree = parse(src);
        let mut rels = Vec::new();
        walk_depth_relations(
            tree.root_node(),
            src.as_bytes(),
            Path::new("t.rs"),
            &[],
            &mut rels,
        );
        assert!(
            rels.iter().any(|r| {
                r.relation_type == RelationType::Implements && r.from == "Foo" && r.to == "Default"
            }),
            "{rels:?}"
        );
    }

    #[test]
    fn derive_emits_annotated_with() {
        let src = "#[derive(Debug, Serialize)]\nstruct Dto { x: i32 }";
        let tree = parse(src);
        let symbols = vec![Symbol {
            name: "Dto".to_string(),
            symbol_type: SymbolType::Struct,
            qualified_name: Some("Dto".to_string()),
            location: SourceLocation {
                file: "t.rs".into(),
                start_line: 2,
                end_line: 2,
                start_column: 0,
                end_column: 0,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
            ..Default::default()
        }];
        let mut rels = Vec::new();
        walk_depth_relations(
            tree.root_node(),
            src.as_bytes(),
            Path::new("t.rs"),
            &symbols,
            &mut rels,
        );
        assert!(
            rels.iter()
                .any(|r| r.relation_type == RelationType::AnnotatedWith && r.to.contains("derive")),
            "{rels:?}"
        );
    }
}
