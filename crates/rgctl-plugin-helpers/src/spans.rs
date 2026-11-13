//! Red-Green byte-span helpers for Tier-1 lossless write-back.
//!
//! Symbol/call-site recording writes scalar fields without allocating. Field
//! spans travel in worker-owned metadata until graph member materialization.

use rgctl_plugin_api::{Relation, Symbol};
use tree_sitter::Node;

/// Compute declaration + identifier spans from tree-sitter nodes.
///
/// Returns `(start_byte, end_byte, name_rel_offset, name_span_overflow)`.
/// When the identifier starts ≥ 65535 bytes after the declaration start,
/// `name_rel_offset` is `u16::MAX` and `name_span_overflow` holds `(rel_start, token_len)`.
#[inline]
pub fn compute_symbol_byte_spans(
    decl_node: Node,
    name_node: Option<Node>,
) -> (u32, u32, u16, Option<(u32, u32)>) {
    let start_byte = decl_node.start_byte() as u32;
    let end_byte = decl_node.end_byte() as u32;
    let Some(name) = name_node else {
        return (start_byte, end_byte, u16::MAX, None);
    };
    let name_start = name.start_byte() as u32;
    let name_len = (name.end_byte() - name.start_byte()) as u32;
    let rel = name_start.saturating_sub(start_byte);
    match u16::try_from(rel) {
        Ok(rel_u16) => (start_byte, end_byte, rel_u16, None),
        Err(_) => (start_byte, end_byte, u16::MAX, Some((rel, name_len))),
    }
}

/// Record declaration / identifier byte spans onto `symbol` (no heap alloc).
///
/// When `name_node` is present and its byte length differs from `symbol.name.len()`,
/// sets `name_token_len` so Red-tree reconstruction slices only the identifier token.
#[inline]
pub fn record_symbol_spans(symbol: &mut Symbol, decl_node: Node, name_node: Option<Node>) {
    let (start, end, rel, overflow) = compute_symbol_byte_spans(decl_node, name_node);
    symbol.start_byte = Some(start);
    symbol.end_byte = Some(end);
    if let Some(overflow) = overflow {
        symbol.name_rel_offset = None;
        symbol.name_span_overflow = Some(overflow);
        if let Some(name) = name_node {
            let len = name.end_byte() - name.start_byte();
            symbol.name_token_len = u16::try_from(len).ok();
        }
    } else if rel == u16::MAX && name_node.is_none() {
        symbol.name_rel_offset = None;
        symbol.name_span_overflow = None;
    } else {
        symbol.name_rel_offset = Some(rel);
        symbol.name_span_overflow = None;
        if let Some(name) = name_node {
            let token_len = name.end_byte() - name.start_byte();
            if token_len != symbol.name.len() {
                symbol.name_token_len = u16::try_from(token_len).ok();
            }
        }
    }
}

/// Locate a field's name within its already parsed declaration, preserving sigils
/// and quotes outside the identifier span. No source parse or file read occurs.
pub fn field_source_span(
    declaration: Node,
    source: &[u8],
    name: &str,
) -> Option<rgctl_plugin_api::FieldSourceSpan> {
    // A record's type name and body are not component declarations. Restrict
    // the search before considering same-spelled identifiers elsewhere.
    let search_root = if declaration.kind() == "record_declaration" {
        declaration.child_by_field_name("parameters")?
    } else {
        declaration
    };
    let mut stack = vec![search_root];
    let mut candidate = None;
    while let Some(node) = stack.pop() {
        // Attributes/decorators describe a declaration; their identifiers are
        // never its binding, even when both have exactly the same spelling.
        if node.id() != declaration.id()
            && (matches!(
                node.kind(),
                "annotation"
                    | "marker_annotation"
                    | "decorator"
                    | "decorators"
                    | "attribute_list"
                    | "attribute_specifier"
                    | "attribute_specifier_sequence"
                    | "annotations"
            ) || node.kind() == "attribute"
                && node
                    .utf8_text(source)
                    .is_ok_and(|text| text.starts_with('@')))
        {
            continue;
        }
        let mut priority = match node.kind() {
            "field_identifier" | "property_identifier" | "private_property_identifier" => 0,
            "identifier" | "name" | "simple_identifier" | "variable_name" | "variable"
            | "instance_variable" | "class_variable" | "simple_symbol" | "string" | "atom"
            | "quoted_atom" | "keyword" => 1,
            "type_identifier" | "qualified_type" => 2,
            _ => 3,
        };
        if node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "variable_declarator"
                    | "field_declaration"
                    | "property_declaration"
                    | "public_field_definition"
                    | "formal_parameter"
                    | "class_parameter"
                    | "parameter"
                    | "property_promotion_parameter"
                    | "assignment"
                    | "pair"
            ) && ["name", "left", "key"].iter().any(|field| {
                parent
                    .child_by_field_name(field)
                    .is_some_and(|binding| binding.id() == node.id())
            })
        }) {
            priority = 0;
        }
        let mut ancestor = node;
        while ancestor.id() != declaration.id() {
            let Some(parent) = ancestor.parent() else {
                break;
            };
            if parent
                .child_by_field_name("type")
                .is_some_and(|ty| ty.id() == ancestor.id())
            {
                priority = priority.max(2);
                break;
            }
            ancestor = parent;
        }
        if priority < 3
            && let Ok(raw) = node.utf8_text(source)
        {
            let normalized = raw
                .trim()
                .trim_matches(&['$', '@', ':', '\'', '"', '`'][..]);
            if (raw == name || normalized == name)
                && candidate
                    .as_ref()
                    .is_none_or(|(best, _, _)| priority < *best)
            {
                let offset = if raw == name { 0 } else { raw.find(name)? };
                candidate = Some((priority, node, offset));
                if priority == 0 {
                    break;
                }
            }
        }
        if node.id() != declaration.id()
            && matches!(
                node.kind(),
                "block"
                    | "class_body"
                    | "declaration_list"
                    | "function_body"
                    | "argument_list"
                    | "arguments"
                    | "type_arguments"
                    | "type_annotation"
            )
        {
            continue;
        }
        if matches!(node.kind(), "attribute" | "member_expression")
            && let Some(property) = node
                .child_by_field_name("attribute")
                .or_else(|| node.child_by_field_name("property"))
        {
            stack.push(property);
            continue;
        }
        // Initializers may reference a later declarator with the same name.
        // Those uses are not declaration bindings (for example `a = self.b, b`).
        let initializer_nodes = ["value", "right", "default_value", "initializer"]
            .map(|field| node.child_by_field_name(field));
        for i in (0..node.named_child_count()).rev() {
            if let Some(child) = node.named_child(i)
                && !initializer_nodes
                    .iter()
                    .flatten()
                    .any(|initializer| initializer.id() == child.id())
            {
                stack.push(child);
            }
        }
    }
    let (_, identifier, offset) = candidate?;
    let mut owner = declaration;
    // Constructor/record field parameters have their own declaration spans.
    if matches!(
        declaration.kind(),
        "record_declaration" | "primary_constructor" | "class_parameters" | "parameter_list"
    ) {
        let mut parent = identifier.parent();
        while let Some(node) = parent {
            if matches!(
                node.kind(),
                "formal_parameter"
                    | "class_parameter"
                    | "parameter"
                    | "property_promotion_parameter"
            ) {
                owner = node;
                break;
            }
            if node.id() == declaration.id() {
                break;
            }
            parent = node.parent();
        }
    }
    let start = u32::try_from(owner.start_byte()).ok()?;
    let end = u32::try_from(owner.end_byte()).ok()?;
    let relative = u32::try_from(identifier.start_byte().checked_add(offset)?)
        .ok()?
        .checked_sub(start)?;
    let length = u32::try_from(name.len()).ok()?;
    let overflow = relative >= u32::from(u16::MAX) || length >= u32::from(u16::MAX);
    Some(rgctl_plugin_api::FieldSourceSpan {
        start_byte: start,
        end_byte: end,
        name_rel_offset: (!overflow).then_some(relative as u16),
        name_token_len: u16::try_from(length).ok(),
        name_span_overflow: overflow.then_some((relative, length)),
        start_line: u32::try_from(owner.start_position().row + 1).ok()?,
        end_line: u32::try_from(owner.end_position().row + 1).ok()?,
    })
}

/// Attach dedicated byte spans to a directly emitted field/property symbol.
pub fn field_symbol_spans(symbol: &mut Symbol, declaration: Node, source: &[u8]) {
    if let Some(span) = field_source_span(declaration, source, &symbol.name) {
        symbol.start_byte = Some(span.start_byte);
        symbol.end_byte = Some(span.end_byte);
        symbol.name_rel_offset = span.name_rel_offset;
        symbol.name_token_len = span.name_token_len;
        symbol.name_span_overflow = span.name_span_overflow;
    }
}

/// Record call-site / reference identifier spans onto a relation (no heap alloc).
#[inline]
pub fn record_call_site_spans(relation: &mut Relation, name_node: Node) {
    relation.call_site_start_byte = Some(name_node.start_byte() as u32);
    relation.call_site_byte_len = u16::try_from(name_node.end_byte() - name_node.start_byte()).ok();
}

fn is_body_container_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_body"
            | "class_body"
            | "enum_class_body"
            | "protocol_body"
            | "template_body"
            | "block"
            | "do_block"
            | "compound_statement"
    )
}

fn find_matching_name_node<'a>(
    node: Node<'a>,
    source: &[u8],
    expected: &[u8],
    depth: usize,
) -> Option<Node<'a>> {
    if expected.is_empty() || depth > 5 {
        return None;
    }
    if let Some(name) = node.child_by_field_name("name")
        && source.get(name.start_byte()..name.end_byte()) == Some(expected)
    {
        return Some(name);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_body_container_kind(child.kind()) {
            continue;
        }
        if source.get(child.start_byte()..child.end_byte()) == Some(expected)
            && (child.child_count() == 0
                || matches!(
                    child.kind(),
                    "dot_index_expression"
                        | "method_index_expression"
                        | "alias"
                        | "object_reference"
                        | "command_name"
                        | "variable_list"
                ))
        {
            return Some(child);
        }
        if let Some(found) = find_matching_name_node(child, source, expected, depth + 1) {
            return Some(found);
        }
    }
    None
}

/// Record declaration and identifier byte spans onto `symbol`, resolving the
/// identifier child from `decl_node` header tokens matching `symbol.name` (no heap alloc).
#[inline]
pub fn record_symbol_spans_from_source(symbol: &mut Symbol, decl_node: Node, source: &[u8]) {
    let name_node = find_matching_name_node(decl_node, source, symbol.name.as_bytes(), 0);
    record_symbol_spans(symbol, decl_node, name_node);
}

/// Extract a compact declaration header signature without cloning multi-line or `{ ... }` bodies.
pub fn declaration_header_signature(decl_node: Node, source: &[u8]) -> Option<String> {
    let start = decl_node.start_byte();
    let mut end = decl_node.end_byte();
    let mut cursor = decl_node.walk();
    for child in decl_node.children(&mut cursor) {
        if is_body_container_kind(child.kind()) && child.start_byte() >= start {
            end = end.min(child.start_byte());
            break;
        }
    }
    let slice = source.get(start..end)?;
    let text = std::str::from_utf8(slice).ok()?;
    let line = text.lines().next().unwrap_or(text);
    let before_brace = line.split('{').next().unwrap_or(line).trim();
    let trimmed = before_brace
        .strip_suffix('=')
        .map(str::trim_end)
        .or_else(|| before_brace.strip_suffix(" do").map(str::trim_end))
        .unwrap_or(before_brace);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_plugin_api::{SourceLocation, SymbolType};
    use tree_sitter::Parser;

    fn parse_rust(src: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(src, None).unwrap()
    }

    #[test]
    fn record_symbol_spans_sets_rel_offset() {
        let src = "fn hello() {}\n";
        let tree = parse_rust(src);
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        let name = fn_node.child_by_field_name("name").unwrap();
        let mut sym = Symbol {
            name: "hello".into(),
            symbol_type: SymbolType::Function,
            location: SourceLocation {
                file: "t.rs".into(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 12,
            },
            ..Default::default()
        };
        record_symbol_spans(&mut sym, fn_node, Some(name));
        assert_eq!(sym.start_byte, Some(0));
        assert_eq!(sym.end_byte, Some(src.trim_end().len() as u32));
        assert_eq!(sym.name_rel_offset, Some(3)); // "fn "
        assert!(sym.name_span_overflow.is_none());
    }
}
