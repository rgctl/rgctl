//! Definition and use extraction from tree-sitter AST nodes.

use std::collections::HashSet;
use tree_sitter::Node;

/// Extract variables defined and used in a statement node.
pub fn extract_def_use(node: Node, source: &[u8]) -> (HashSet<String>, HashSet<String>) {
    let mut defined = HashSet::new();
    let mut used = HashSet::new();
    collect_def_use(node, source, &mut defined, &mut used, false);
    (defined, used)
}

fn collect_def_use(
    node: Node,
    source: &[u8],
    defined: &mut HashSet<String>,
    used: &mut HashSet<String>,
    is_def_target: bool,
) {
    let kind = node.kind();

    match kind {
        // Rust
        "let_declaration" | "let_statement" => {
            if let Some(pattern) = node.child_by_field_name("pattern") {
                collect_pattern_defs(pattern, source, defined);
            }
            if let Some(value) = node.child_by_field_name("value") {
                collect_def_use(value, source, defined, used, false);
            }
        }
        "assignment_expression" | "augmented_assignment_expression" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_pattern_defs(left, source, defined);
            }
            if let Some(right) = node.child_by_field_name("right") {
                collect_def_use(right, source, defined, used, false);
            }
        }
        "compound_assignment_expr" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_pattern_defs(left, source, defined);
            }
            if let Some(right) = node.child_by_field_name("right") {
                collect_def_use(right, source, defined, used, false);
            }
        }

        // Python
        "assignment" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_pattern_defs(left, source, defined);
            }
            if let Some(right) = node.child_by_field_name("right") {
                collect_def_use(right, source, defined, used, false);
            }
        }
        "augmented_assignment" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_pattern_defs(left, source, defined);
                collect_def_use(left, source, defined, used, false);
            }
            if let Some(right) = node.child_by_field_name("right") {
                collect_def_use(right, source, defined, used, false);
            }
        }
        "for_statement" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_pattern_defs(left, source, defined);
            }
            if let Some(body) = node.child_by_field_name("body") {
                collect_def_use(body, source, defined, used, false);
            }
        }

        // Shared identifiers
        "identifier" | "shorthand_field_identifier" | "field_identifier" | "type_identifier" => {
            if is_def_target {
                if let Ok(name) = node.utf8_text(source) {
                    if kind == "identifier" || kind == "shorthand_field_identifier" {
                        defined.insert(name.to_string());
                    }
                }
            } else if kind == "identifier" || kind == "shorthand_field_identifier" {
                if let Ok(name) = node.utf8_text(source) {
                    used.insert(name.to_string());
                }
            }
        }

        "scoped_identifier" => {
            if let Some(name) = node.child_by_field_name("name") {
                if is_def_target {
                    collect_pattern_defs(name, source, defined);
                } else {
                    collect_def_use(name, source, defined, used, false);
                }
            }
        }

        _ if is_def_target && is_binding_pattern(kind) => {
            collect_pattern_defs(node, source, defined);
        }

        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_def_use(child, source, defined, used, false);
            }
        }
    }
}

fn is_binding_pattern(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "shorthand_field_identifier"
            | "tuple_pattern"
            | "tuple_struct_pattern"
            | "struct_pattern"
            | "pattern"
            | "list_pattern"
            | "attribute"
            | "rest_pattern"
            | "wildcard_pattern"
    )
}

fn collect_pattern_defs(node: Node, source: &[u8], defined: &mut HashSet<String>) {
    match node.kind() {
        "identifier" | "shorthand_field_identifier" => {
            if let Ok(name) = node.utf8_text(source) {
                defined.insert(name.to_string());
            }
        }
        "tuple_pattern"
        | "tuple_struct_pattern"
        | "struct_pattern"
        | "pattern"
        | "list_pattern"
        | "attribute"
        | "rest_pattern" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_pattern_defs(child, source, defined);
            }
        }
        _ => {
            collect_def_use(node, source, defined, &mut HashSet::new(), true);
        }
    }
}

/// Collect all identifier uses under a subtree.
pub fn extract_used_variables(node: Node, source: &[u8]) -> HashSet<String> {
    let (_, used) = extract_def_use(node, source);
    used
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_rust(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn parse_python(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn find_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
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
    fn test_rust_let_def_use() {
        let source = "fn f(a: i32) { let x = a + 1; x }";
        let tree = parse_rust(source);
        let let_node = find_kind(tree.root_node(), "let_declaration").unwrap();
        let (defs, uses) = extract_def_use(let_node, source.as_bytes());
        assert!(defs.contains("x"));
        assert!(uses.contains("a"));
    }

    #[test]
    fn test_python_assignment_def_use() {
        let source = "def f(a):\n    x = a + 1\n    return x\n";
        let tree = parse_python(source);
        let assign = find_kind(tree.root_node(), "assignment").unwrap();
        let (defs, uses) = extract_def_use(assign, source.as_bytes());
        assert!(defs.contains("x"));
        assert!(uses.contains("a"));
    }

    #[test]
    fn test_identifier_use_only() {
        let source = "fn f() { x + y }";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let uses = extract_used_variables(root, source.as_bytes());
        assert!(uses.contains("x"));
        assert!(uses.contains("y"));
    }
}
