//! Red-Green byte-span helpers for Tier-1 lossless write-back.
//!
//! Zero heap allocations on the hot path: only scalar `u32`/`u16` field writes.

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

/// Record call-site / reference identifier spans onto a relation (no heap alloc).
#[inline]
pub fn record_call_site_spans(relation: &mut Relation, name_node: Node) {
    relation.call_site_start_byte = Some(name_node.start_byte() as u32);
    relation.call_site_byte_len = u16::try_from(name_node.end_byte() - name_node.start_byte()).ok();
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
