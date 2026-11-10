//! Shared ECMAScript module and class-heritage extraction for JS/TS plugins.

use rgctl_plugin_api::{Relation, RelationType, SourceLocation, Symbol, SymbolType};
use std::path::Path;
use tree_sitter::Node;

/// Extract `Import` symbols from `import_statement` nodes.
pub fn extract_import_symbols(
    node: Node,
    source: &[u8],
    file_path: &str,
    language: &str,
) -> Vec<Symbol> {
    if node.kind() != "import_statement" {
        return Vec::new();
    }

    let text = node.utf8_text(source).unwrap_or("").trim().to_string();
    let is_type_only = text.starts_with("import type")
        || text.starts_with("import typeof")
        || node
            .children(&mut node.walk())
            .any(|c| c.kind() == "type" && c.utf8_text(source).ok() == Some("type"));

    let mut symbols = Vec::new();
    let mut module = None;

    if let Some(src) = node.child_by_field_name("source") {
        module = src
            .utf8_text(source)
            .ok()
            .map(|s| s.trim_matches(['"', '\'']).to_string());
    } else {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "string" {
                module = child
                    .utf8_text(source)
                    .ok()
                    .map(|s| s.trim_matches(['"', '\'']).to_string());
            }
        }
    }

    let name = module.clone().unwrap_or_else(|| text.clone());
    symbols.push(import_symbol(
        &name,
        node,
        file_path,
        language,
        is_type_only,
        module.as_deref(),
        None,
    ));

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_import_clause_children(
            child,
            source,
            &name,
            file_path,
            language,
            is_type_only,
            module.as_deref(),
            &mut symbols,
        );
    }

    symbols
}

#[allow(clippy::too_many_arguments)]
fn collect_import_clause_children(
    node: Node,
    source: &[u8],
    name: &str,
    file_path: &str,
    language: &str,
    is_type_only: bool,
    module: Option<&str>,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "named_imports" => {
            let mut ic = node.walk();
            for spec in node.children(&mut ic) {
                if spec.kind() == "import_specifier" {
                    let local = import_specifier_local_name(spec, source);
                    symbols.push(import_symbol(
                        &format!("{name}:{local}"),
                        spec,
                        file_path,
                        language,
                        is_type_only,
                        module,
                        Some(&local),
                    ));
                }
            }
        }
        "namespace_import" => {
            if let Some(local) = node
                .child_by_field_name("name")
                .or_else(|| find_child_kind(node, "identifier"))
                .and_then(|n| n.utf8_text(source).ok())
            {
                symbols.push(import_symbol(
                    &format!("{name}:* as {local}"),
                    node,
                    file_path,
                    language,
                    is_type_only,
                    module,
                    Some(local),
                ));
            }
        }
        "import_clause" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_import_clause_children(
                    child,
                    source,
                    name,
                    file_path,
                    language,
                    is_type_only,
                    module,
                    symbols,
                );
            }
        }
        _ => {}
    }
}

/// `Extends` edges from `class_heritage` / `extends_clause` on a class declaration.
/// Extract CommonJS `require('module')` call sites as `Import` symbols.
pub fn extract_cjs_require_symbols(
    root: Node,
    source: &[u8],
    file_path: &str,
    language: &str,
) -> Vec<Symbol> {
    const MAX_DEPTH: usize = 2048;
    let mut symbols = Vec::new();
    let mut stack = vec![(root, 0usize)];

    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }
        if node.kind() == "call_expression" {
            let is_require = node
                .child_by_field_name("function")
                .and_then(|f| f.utf8_text(source).ok())
                == Some("require");
            if is_require && let Some(module) = require_module_argument(node, source) {
                let mut meta = serde_json::json!({
                    "language": language,
                    "module_system": "cjs",
                });
                meta["module"] = serde_json::Value::String(module.clone());
                let mut symbol = Symbol {
                    name: module,
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
                crate::spans::record_symbol_spans(&mut symbol, node, None);
                symbols.push(symbol);
            }
        }

        let mut cursor = node.walk();
        let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1));
        }
    }

    symbols
}

fn require_module_argument(call: Node, source: &[u8]) -> Option<String> {
    let mut cursor = call.walk();
    for child in call.children(&mut cursor) {
        if child.kind() == "arguments" || child.kind() == "argument_list" {
            let mut ac = child.walk();
            for arg in child.children(&mut ac) {
                if arg.kind() == "string" || arg.kind() == "string_fragment" {
                    return arg
                        .utf8_text(source)
                        .ok()
                        .map(|s| s.trim_matches(['"', '\'']).to_string());
                }
                if arg.kind() == "template_string"
                    && let Some(inner) = arg.named_child(0)
                {
                    return inner
                        .utf8_text(source)
                        .ok()
                        .map(|s| s.trim_matches(['"', '\'']).to_string());
                }
            }
        }
        if child.kind() == "string" {
            return child
                .utf8_text(source)
                .ok()
                .map(|s| s.trim_matches(['"', '\'']).to_string());
        }
    }
    None
}

pub fn extract_class_extends_relations(
    class_node: Node,
    source: &[u8],
    file_path: &Path,
    class_name: &str,
    language: &str,
    relations: &mut Vec<Relation>,
) {
    let heritage = find_child_kind(class_node, "class_heritage");
    let Some(heritage) = heritage else {
        return;
    };

    if let Some(extends_clause) = find_child_kind(heritage, "extends_clause") {
        let mut cursor = extends_clause.walk();
        for child in extends_clause.children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "type_identifier" {
                if let Ok(base) = child.utf8_text(source) {
                    push_relation(
                        class_name,
                        base,
                        RelationType::Extends,
                        child,
                        file_path,
                        language,
                        relations,
                    );
                }
            } else if let Some(value) = child.child_by_field_name("value")
                && let Some(base) = type_name_from_node(value, source)
            {
                push_relation(
                    class_name,
                    &base,
                    RelationType::Extends,
                    child,
                    file_path,
                    language,
                    relations,
                );
            }
        }
        return;
    }

    // JavaScript: `class_heritage` is `extends <expression>` (no `extends_clause` node).
    let mut cursor = heritage.walk();
    for child in heritage.children(&mut cursor) {
        if child.kind() == "extends" {
            continue;
        }
        if let Some(base) = type_name_from_node(child, source) {
            push_relation(
                class_name,
                &base,
                RelationType::Extends,
                child,
                file_path,
                language,
                relations,
            );
            break;
        }
    }
}

fn import_symbol(
    name: &str,
    node: Node,
    file_path: &str,
    language: &str,
    is_type_only: bool,
    module: Option<&str>,
    local: Option<&str>,
) -> Symbol {
    let mut meta = serde_json::json!({ "language": language });
    if is_type_only {
        meta["is_type_only"] = serde_json::Value::Bool(true);
    }
    if let Some(module) = module {
        meta["module"] = serde_json::Value::String(module.to_string());
    }
    if let Some(local) = local {
        meta["local"] = serde_json::Value::String(local.to_string());
    }
    let mut symbol = Symbol {
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
    crate::spans::record_symbol_spans(&mut symbol, node, None);
    symbol
}

fn import_specifier_local_name(spec: Node, source: &[u8]) -> String {
    if let Some(alias) = spec.child_by_field_name("alias")
        && let Ok(name) = alias.utf8_text(source)
    {
        return name.to_string();
    }
    if let Some(name) = spec.child_by_field_name("name")
        && let Ok(text) = name.utf8_text(source)
    {
        return text.to_string();
    }
    spec.utf8_text(source)
        .unwrap_or("default")
        .split(" as ")
        .last()
        .unwrap_or("default")
        .trim()
        .to_string()
}

pub fn type_name_from_node(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" | "identifier" | "predefined_type" => {
            node.utf8_text(source).ok().map(str::to_string)
        }
        "generic_type" | "nested_type_identifier" => node
            .child_by_field_name("name")
            .or_else(|| find_child_kind(node, "type_identifier"))
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string)),
        "member_expression" => node
            .child_by_field_name("property")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string)),
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(name) = type_name_from_node(child, source) {
                    return Some(name);
                }
            }
            None
        }
    }
}

pub fn simple_type_name(raw: &str) -> String {
    raw.split('<')
        .next()
        .unwrap_or(raw)
        .trim()
        .rsplit('.')
        .next()
        .unwrap_or(raw)
        .to_string()
}

pub fn find_child_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = find_child_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

/// True for arrow / anonymous function expressions (not declarations or methods).
pub fn is_function_expression_kind(kind: &str) -> bool {
    matches!(
        kind,
        "arrow_function" | "function_expression" | "generator_function"
    )
}

/// True when `node` is a real JS/TS callable CST node (not the anonymous
/// `'function'` keyword token that appears under `function_declaration`).
///
/// Tree-sitter emits that keyword as `kind == "function"` with `is_named() == false`.
/// Matching it as a Function symbol produces a body-less `anonymous@L{N}` duplicate
/// on every declaration.
pub fn is_ecmascript_function_node(node: Node<'_>) -> bool {
    match node.kind() {
        "function_declaration"
        | "function_expression"
        | "method_definition"
        | "arrow_function"
        | "generator_function"
        | "generator_function_declaration" => true,
        "function" => node.is_named(),
        _ => false,
    }
}

/// Graph Function name for a JS/TS callable node — matches language-plugin naming.
///
/// Bound arrows / function expressions use the parent binding; true callbacks use
/// `anonymous@L{line}`. Declarations and methods use the CST name field.
pub fn ecmascript_function_symbol_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    if !is_ecmascript_function_node(node) {
        return None;
    }

    let is_expr = is_function_expression_kind(node.kind())
        || (node.kind() == "function"
            && node.is_named()
            && node.child_by_field_name("name").is_none());

    if is_expr {
        return Some(
            bound_function_expression_name(node, source)
                .unwrap_or_else(|| format!("anonymous@L{}", node.start_position().row + 1)),
        );
    }

    if let Some(name_node) = node.child_by_field_name("name")
        && let Ok(text) = name_node.utf8_text(source)
    {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    // method_definition / fallbacks
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier" | "property_identifier" | "private_property_identifier"
        ) && let Ok(text) = child.utf8_text(source)
        {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Resolve a display name for `arrow_function` / `function_expression` from a
/// parent binding: `variable_declarator`, object `pair`, or class field.
///
/// Returns `None` for true callbacks (e.g. `arr.map(x => …)`).
pub fn bound_function_expression_name(node: Node, source: &[u8]) -> Option<String> {
    if !is_function_expression_kind(node.kind())
        && !(node.kind() == "function"
            && node.is_named()
            && node.child_by_field_name("name").is_none())
    {
        return None;
    }

    let mut current = node;
    for _ in 0..12 {
        let parent = current.parent()?;
        match parent.kind() {
            "variable_declarator" => {
                return identifier_text(parent.child_by_field_name("name")?, source);
            }
            "pair" => {
                let key = parent.child_by_field_name("key").or_else(|| {
                    find_direct_child_kinds(
                        parent,
                        &["property_identifier", "identifier", "string", "number"],
                    )
                })?;
                return property_key_text(key, source);
            }
            "public_field_definition" | "field_definition" | "property_definition" => {
                let name = parent.child_by_field_name("name").or_else(|| {
                    find_direct_child_kinds(
                        parent,
                        &[
                            "property_identifier",
                            "private_property_identifier",
                            "identifier",
                        ],
                    )
                })?;
                return identifier_text(name, source);
            }
            "assignment_expression" => {
                let left = parent.child_by_field_name("left")?;
                return match left.kind() {
                    "identifier" => identifier_text(left, source),
                    "member_expression" | "subscript_expression" => left
                        .child_by_field_name("property")
                        .and_then(|p| property_key_text(p, source)),
                    _ => None,
                };
            }
            // Peel type / grouping wrappers without consuming a binding.
            "parenthesized_expression"
            | "as_expression"
            | "type_assertion"
            | "satisfies_expression"
            | "non_null_expression"
            | "await_expression"
            | "ternary_expression" => {
                current = parent;
            }
            _ => return None,
        }
    }
    None
}

fn find_direct_child_kinds<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|&child| kinds.contains(&child.kind()))
}

fn identifier_text(node: Node, source: &[u8]) -> Option<String> {
    node.utf8_text(source)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn property_key_text(node: Node, source: &[u8]) -> Option<String> {
    let raw = node.utf8_text(source).ok()?.trim();
    let trimmed = raw.trim_matches(['"', '\'', '`']);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn push_relation(
    from: &str,
    to: &str,
    relation_type: RelationType,
    node: Node,
    file_path: &Path,
    language: &str,
    relations: &mut Vec<Relation>,
) {
    relations.push(Relation {
        from: from.to_string(),
        to: to.to_string(),
        relation_type,
        location: source_location(node, &file_path.to_string_lossy()),
        metadata: serde_json::json!({ "language": language }),
        to_qualified_hint: None,
        to_type_hint: None,
        ..Default::default()
    });
}

fn source_location(node: Node, file_path: &str) -> SourceLocation {
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
    use std::path::Path;
    use tree_sitter::Parser;

    fn parse_ts(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn test_named_import_symbol() {
        let source = "import { foo, bar as Baz } from 'lodash';";
        let tree = parse_ts(source);
        let root = tree.root_node();
        let import = root.named_child(0).unwrap();
        let symbols = extract_import_symbols(import, source.as_bytes(), "a.ts", "typescript");
        assert!(symbols.iter().all(|s| s.symbol_type == SymbolType::Import));
        assert!(symbols.len() >= 2);
    }

    #[test]
    fn test_type_only_import_metadata() {
        let source = "import type { Foo } from './foo';";
        let tree = parse_ts(source);
        let import = tree.root_node().named_child(0).unwrap();
        let symbols = extract_import_symbols(import, source.as_bytes(), "a.ts", "typescript");
        assert!(
            symbols
                .first()
                .and_then(|s| s.metadata.get("is_type_only"))
                .and_then(|v| v.as_bool())
                == Some(true)
        );
    }

    #[test]
    fn test_class_extends_relation() {
        let source = "class AppError extends Error {}";
        let tree = parse_ts(source);
        let class = tree.root_node().named_child(0).unwrap();
        let mut relations = Vec::new();
        extract_class_extends_relations(
            class,
            source.as_bytes(),
            Path::new("a.ts"),
            "AppError",
            "typescript",
            &mut relations,
        );
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Extends && r.to == "Error")
        );
    }

    fn parse_js(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn test_js_require_import() {
        let source = "const fs = require('fs');";
        let tree = parse_js(source);
        let symbols =
            extract_cjs_require_symbols(tree.root_node(), source.as_bytes(), "a.js", "javascript");
        assert!(symbols.iter().any(|s| s.name == "fs"));
        assert_eq!(
            symbols[0]
                .metadata
                .get("module_system")
                .and_then(|v| v.as_str()),
            Some("cjs")
        );
    }

    #[test]
    fn test_js_class_extends_relation() {
        let source = "class AppError extends Error {}";
        let tree = parse_js(source);
        let class = tree.root_node().named_child(0).unwrap();
        let mut relations = Vec::new();
        extract_class_extends_relations(
            class,
            source.as_bytes(),
            Path::new("a.js"),
            "AppError",
            "javascript",
            &mut relations,
        );
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Extends && r.to == "Error")
        );
    }

    fn find_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn test_bound_name_const_arrow() {
        let source = "const multiply = (x, y) => x * y;";
        let tree = parse_ts(source);
        let arrow = find_kind(tree.root_node(), "arrow_function").unwrap();
        assert_eq!(
            bound_function_expression_name(arrow, source.as_bytes()).as_deref(),
            Some("multiply")
        );
    }

    #[test]
    fn test_bound_name_object_property_arrow() {
        let source = "const api = { fetchAll: async () => 1 };";
        let tree = parse_ts(source);
        let arrow = find_kind(tree.root_node(), "arrow_function").unwrap();
        assert_eq!(
            bound_function_expression_name(arrow, source.as_bytes()).as_deref(),
            Some("fetchAll")
        );
    }

    #[test]
    fn test_bound_name_class_field_arrow() {
        let source = "class C { foo = () => 2; }";
        let tree = parse_ts(source);
        let arrow = find_kind(tree.root_node(), "arrow_function").unwrap();
        assert_eq!(
            bound_function_expression_name(arrow, source.as_bytes()).as_deref(),
            Some("foo")
        );
    }

    #[test]
    fn test_callback_arrow_has_no_bound_name() {
        let source = "[1].map(x => x + 1);";
        let tree = parse_js(source);
        let arrow = find_kind(tree.root_node(), "arrow_function").unwrap();
        assert_eq!(
            bound_function_expression_name(arrow, source.as_bytes()),
            None
        );
    }

    #[test]
    fn test_keyword_function_token_is_not_a_function_node() {
        let source = "export function gamma(n: number): number { return n + 1; }";
        let tree = parse_ts(source);
        let mut keyword = None;
        let mut decl = None;
        let mut stack = vec![tree.root_node()];
        while let Some(n) = stack.pop() {
            if n.kind() == "function_declaration" {
                decl = Some(n);
            }
            if n.kind() == "function" && !n.is_named() {
                keyword = Some(n);
            }
            let mut c = n.walk();
            for ch in n.children(&mut c) {
                stack.push(ch);
            }
        }
        let keyword = keyword.expect("anonymous 'function' keyword token");
        let decl = decl.expect("function_declaration");
        assert!(!is_ecmascript_function_node(keyword));
        assert!(is_ecmascript_function_node(decl));
        assert_eq!(
            ecmascript_function_symbol_name(decl, source.as_bytes()).as_deref(),
            Some("gamma")
        );
    }

    #[test]
    fn test_cfg_name_for_bound_arrow() {
        let source = "export const uniqueOne = (n: number): number => n + 1;";
        let tree = parse_ts(source);
        let arrow = find_kind(tree.root_node(), "arrow_function").unwrap();
        assert_eq!(
            ecmascript_function_symbol_name(arrow, source.as_bytes()).as_deref(),
            Some("uniqueOne")
        );
    }

    #[test]
    fn ts_method_preceding_sibling_decorators() {
        let source = r#"
@Controller('users')
export class UsersController {
  constructor(private readonly svc: UserService) {}

  @Get(':id')
  findOne(@Param('id') id: string) {
    return id;
  }
}
"#;
        let tree = parse_ts(source);
        let body = find_kind(tree.root_node(), "class_body").expect("class_body");
        let mut methods = Vec::new();
        let mut c = body.walk();
        for child in body.children(&mut c) {
            if child.kind() == "method_definition" {
                methods.push(child);
            }
        }
        assert!(methods.len() >= 2, "need constructor + findOne");
        let find_one = methods[1];
        // Simulate single-pass: hold a TreeCursor while resolving decorators.
        let hold = tree.root_node().walk();
        let _ = hold.node();
        let decs = crate::decorators_for_node(find_one, source.as_bytes());
        assert!(
            decs.iter()
                .any(|(n, a)| n == "Get" && a.as_ref().is_some_and(|s| s.contains(":id"))),
            "expected Get(':id') from preceding siblings, got {decs:?}"
        );
    }
}
