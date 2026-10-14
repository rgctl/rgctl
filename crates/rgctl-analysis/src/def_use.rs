//! Definition and use extraction from tree-sitter AST nodes.

use crate::cfg::DefVar;
use smallvec::SmallVec;
use std::collections::HashSet;
use tree_sitter::Node;

/// Maximum AST walk depth (matches C++ extraction and CFG expression walks).
const AST_WALK_MAX_DEPTH: usize = 2048;

/// Extract variables defined and used in a statement node.
pub fn extract_def_use(node: Node, source: &[u8]) -> (SmallVec<[DefVar; 2]>, SmallVec<[String; 3]>) {
    let mut defined = HashSet::new();
    let mut used = HashSet::new();
    let mut stack = vec![(node, false, 0usize)];
    while let Some((node, is_def_target, depth)) = stack.pop() {
        collect_def_use_node(
            node,
            source,
            &mut defined,
            &mut used,
            is_def_target,
            depth,
            &mut stack,
        );
    }
    (defined.into_iter().collect(), used.into_iter().collect())
}

type DefUseStack<'a> = Vec<(Node<'a>, bool, usize)>;

fn is_field_access_kind(kind: &str) -> bool {
    matches!(
        kind,
        "field_access"
            | "field_expression"
            | "member_expression"
            | "member_access_expression"
            | "selector_expression"
            | "attribute"
            // Kotlin: `order.status` / `this.status` (expression + identifier children).
            | "navigation_expression"
    )
}

/// Build a typed field definition for a field-access style AST node.
fn field_access_def(node: Node, source: &[u8]) -> Option<DefVar> {
    // Kotlin navigation_expression: children are expression + identifier (no field names).
    if node.kind() == "navigation_expression" {
        let mut named: Vec<Node> = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                named.push(child);
            }
        }
        // Last identifier is the member; everything before is the receiver expression.
        if let Some((last, prefix)) = named.split_last() {
            if matches!(last.kind(), "identifier" | "simple_identifier") {
                let member = last.utf8_text(source).ok()?.to_string();
                let receiver = if prefix.is_empty() {
                    None
                } else if prefix.len() == 1 {
                    prefix[0].utf8_text(source).ok().map(str::to_string)
                } else {
                    // Multi-hop `a.b.c` — use full prefix text as receiver (best-effort).
                    let start = prefix[0].start_byte();
                    let end = prefix[prefix.len() - 1].end_byte();
                    std::str::from_utf8(&source[start..end])
                        .ok()
                        .map(str::to_string)
                };
                if let Some(receiver) = receiver {
                    return Some(DefVar::Field { receiver, member });
                }
            }
        }
        return node
            .utf8_text(source)
            .ok()
            .map(|s| DefVar::local(s.to_string()));
    }

    let field = node
        .child_by_field_name("field")
        .or_else(|| node.child_by_field_name("property"))
        .or_else(|| node.child_by_field_name("attribute"))
        .or_else(|| node.child_by_field_name("name"));
    let object = node
        .child_by_field_name("object")
        .or_else(|| node.child_by_field_name("operand"))
        .or_else(|| node.child_by_field_name("argument"))
        .or_else(|| node.child_by_field_name("value"))
        .or_else(|| node.child_by_field_name("expression"));
    match (object, field) {
        (Some(obj), Some(fld)) => {
            let obj_txt = obj.utf8_text(source).ok()?;
            let fld_txt = fld.utf8_text(source).ok()?;
            Some(DefVar::Field {
                receiver: obj_txt.to_string(),
                member: fld_txt.to_string(),
            })
        }
        _ => node
            .utf8_text(source)
            .ok()
            .map(|s| DefVar::local(s.to_string())),
    }
}

fn collect_field_access_base_uses<'a>(
    node: Node<'a>,
    _source: &[u8],
    _used: &mut HashSet<String>,
    stack: &mut DefUseStack<'a>,
    depth: usize,
) {
    if let Some(object) = node
        .child_by_field_name("object")
        .or_else(|| node.child_by_field_name("argument"))
        .or_else(|| node.child_by_field_name("value"))
        .or_else(|| node.child_by_field_name("expression"))
    {
        stack.push((object, false, depth + 1));
    }
}

fn collect_assignment_lhs<'a>(
    left: Node<'a>,
    source: &[u8],
    defined: &mut HashSet<DefVar>,
    used: &mut HashSet<String>,
    depth: usize,
    stack: &mut DefUseStack<'a>,
) {
    if is_field_access_kind(left.kind()) {
        if let Some(def) = field_access_def(left, source) {
            defined.insert(def);
        }
        collect_field_access_base_uses(left, source, used, stack, depth);
    } else if left.kind() == "instance_variable" {
        if let Ok(name) = left.utf8_text(source) {
            defined.insert(DefVar::Field {
                receiver: "self".into(),
                member: name.trim_start_matches('@').into(),
            });
        }
    } else {
        collect_pattern_defs(left, source, defined, depth + 1);
    }
}

/// Go `var_spec`: `name` / `name_list` + optional `value` / `value_list`.
fn collect_go_var_spec<'a>(
    spec: Node<'a>,
    source: &[u8],
    defined: &mut HashSet<DefVar>,
    _used: &mut HashSet<String>,
    depth: usize,
    stack: &mut DefUseStack<'a>,
) {
    if let Some(name) = spec.child_by_field_name("name") {
        collect_pattern_defs(name, source, defined, depth + 1);
    }
    let mut cursor = spec.walk();
    for child in spec.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                collect_pattern_defs(child, source, defined, depth + 1);
            }
            "expression_list" => {
                stack.push((child, false, depth + 1));
            }
            _ => {}
        }
    }
    if let Some(value) = spec
        .child_by_field_name("value")
        .or_else(|| spec.child_by_field_name("right"))
    {
        stack.push((value, false, depth + 1));
    }
}

fn collect_def_use_node<'a>(
    node: Node<'a>,
    source: &[u8],
    defined: &mut HashSet<DefVar>,
    used: &mut HashSet<String>,
    is_def_target: bool,
    depth: usize,
    stack: &mut DefUseStack<'a>,
) {
    if depth > AST_WALK_MAX_DEPTH {
        return;
    }
    let kind = node.kind();

    match kind {
        // Rust
        "let_declaration" | "let_statement" => {
            if let Some(pattern) = node.child_by_field_name("pattern") {
                collect_pattern_defs(pattern, source, defined, depth + 1);
            }
            if let Some(value) = node.child_by_field_name("value") {
                stack.push((value, false, depth + 1));
            }
        }
        "assignment_expression" | "augmented_assignment_expression" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_assignment_lhs(left, source, defined, used, depth, stack);
            }
            if let Some(right) = node.child_by_field_name("right") {
                stack.push((right, false, depth + 1));
            }
        }
        "compound_assignment_expr" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_assignment_lhs(left, source, defined, used, depth, stack);
            }
            if let Some(right) = node.child_by_field_name("right") {
                stack.push((right, false, depth + 1));
            }
        }

        // Python
        "assignment" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_assignment_lhs(left, source, defined, used, depth, stack);
            }
            if let Some(right) = node.child_by_field_name("right") {
                stack.push((right, false, depth + 1));
            }
        }
        "augmented_assignment" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_assignment_lhs(left, source, defined, used, depth, stack);
                stack.push((left, false, depth + 1));
            }
            if let Some(right) = node.child_by_field_name("right") {
                stack.push((right, false, depth + 1));
            }
        }
        "for_statement" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_pattern_defs(left, source, defined, depth + 1);
            }
            if let Some(init) = node.child_by_field_name("initializer") {
                stack.push((init, false, depth + 1));
            }
            if let Some(body) = node.child_by_field_name("body") {
                stack.push((body, false, depth + 1));
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "range_clause" | "for_clause") {
                    stack.push((child, false, depth + 1));
                }
            }
        }

        // Go
        "short_var_declaration" | "assignment_statement" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_assignment_lhs(left, source, defined, used, depth, stack);
            }
            if let Some(right) = node.child_by_field_name("right") {
                stack.push((right, false, depth + 1));
            }
        }
        // `var` / `var ( ... )` — children are var_spec / var_spec_list (no left/right fields).
        "var_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "var_spec" => collect_go_var_spec(child, source, defined, used, depth + 1, stack),
                    "var_spec_list" => {
                        let mut c2 = child.walk();
                        for spec in child.children(&mut c2) {
                            if spec.kind() == "var_spec" {
                                collect_go_var_spec(spec, source, defined, used, depth + 1, stack);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        "range_clause" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_pattern_defs(left, source, defined, depth + 1);
            }
            if let Some(right) = node.child_by_field_name("right") {
                stack.push((right, false, depth + 1));
            }
        }
        "expression_list" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    collect_pattern_defs(child, source, defined, depth + 1);
                    stack.push((child, false, depth + 1));
                }
            }
        }
        "parameter_declaration" => {
            if let Some(name) = node.child_by_field_name("name") {
                collect_pattern_defs(name, source, defined, depth + 1);
            }
        }

        // C# / Java / JS / TS local declarations
        "variable_declaration"
        | "local_declaration_statement"
        | "local_variable_declaration"
        | "lexical_declaration"
        | "variable_declaration_list" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    if let Some(name) = child.child_by_field_name("name") {
                        collect_pattern_defs(name, source, defined, depth + 1);
                    }
                    if let Some(value) = child.child_by_field_name("value") {
                        stack.push((value, false, depth + 1));
                    }
                } else if matches!(
                    child.kind(),
                    "variable_declaration" | "variable_declaration_list"
                ) {
                    stack.push((child, false, depth + 1));
                }
            }
        }
        "variable_declarator" => {
            if let Some(name) = node.child_by_field_name("name") {
                collect_pattern_defs(name, source, defined, depth + 1);
            }
            if let Some(value) = node.child_by_field_name("value") {
                stack.push((value, false, depth + 1));
            }
        }

        // C
        "declaration" => {
            if let Some(decl) = node.child_by_field_name("declarator") {
                collect_declarator_defs(decl, source, defined);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "init_declarator" {
                    if let Some(name) = child.child_by_field_name("declarator") {
                        collect_declarator_defs(name, source, defined);
                    }
                    if let Some(value) = child.child_by_field_name("value") {
                        stack.push((value, false, depth + 1));
                    }
                }
            }
        }
        "init_declarator" => {
            if let Some(name) = node.child_by_field_name("declarator") {
                collect_declarator_defs(name, source, defined);
            }
            if let Some(value) = node.child_by_field_name("value") {
                stack.push((value, false, depth + 1));
            }
        }

        // Field / member access (Java field_access, Rust field_expression, …)
        k if is_field_access_kind(k) => {
            if is_def_target {
                if let Some(def) = field_access_def(node, source) {
                    defined.insert(def);
                }
                collect_field_access_base_uses(node, source, used, stack, depth);
            } else {
                collect_field_access_base_uses(node, source, used, stack, depth);
                if let Some(def) = field_access_def(node, source) {
                    used.insert(def.name());
                }
            }
        }

        // Shared identifiers
        "identifier" | "shorthand_field_identifier" | "field_identifier" | "type_identifier"
        | "variable_name" => {
            if is_def_target {
                if let Ok(name) = node.utf8_text(source) {
                    if kind == "identifier" || kind == "shorthand_field_identifier" || kind == "variable_name" {
                        defined.insert(DefVar::local(name.trim_start_matches('$')));
                    }
                }
            } else if kind == "identifier" || kind == "shorthand_field_identifier" || kind == "variable_name" {
                if let Ok(name) = node.utf8_text(source) {
                    used.insert(name.trim_start_matches('$').to_string());
                }
            }
        }

        "scoped_identifier" => {
            if let Some(name) = node.child_by_field_name("name") {
                if is_def_target {
                    collect_pattern_defs(name, source, defined, depth + 1);
                } else {
                    stack.push((name, false, depth + 1));
                }
            }
        }

        _ if is_def_target && is_binding_pattern(kind) => {
            collect_pattern_defs(node, source, defined, depth + 1);
        }

        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push((child, false, depth + 1));
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
            // JS/TS destructuring. Without these, a pattern reaches the catch-all arm below and
            // its children are walked as *uses*, so nothing is ever defined.
            | "object_pattern"
            | "array_pattern"
            | "pair_pattern"
            | "assignment_pattern"
            | "object_assignment_pattern"
            | "shorthand_property_identifier_pattern"
    )
}

fn collect_declarator_defs(node: Node, source: &[u8], defined: &mut HashSet<DefVar>) {
    match node.kind() {
        "identifier" => {
            if let Ok(name) = node.utf8_text(source) {
                defined.insert(DefVar::local(name));
            }
        }
        "pointer_declarator"
        | "function_declarator"
        | "array_declarator"
        | "parenthesized_declarator" => {
            if let Some(inner) = node.child_by_field_name("declarator") {
                collect_declarator_defs(inner, source, defined);
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    collect_declarator_defs(child, source, defined);
                }
            }
        }
    }
}

fn collect_pattern_defs(node: Node, source: &[u8], defined: &mut HashSet<DefVar>, depth: usize) {
    match node.kind() {
        // `shorthand_property_identifier_pattern` is the binding in `const { a } = src`. It is a
        // leaf whose kind is not `identifier`, so without naming it here the name is never
        // recorded as defined.
        "identifier" | "shorthand_field_identifier" | "shorthand_property_identifier_pattern" => {
            if let Ok(name) = node.utf8_text(source) {
                defined.insert(DefVar::local(name));
            }
        }
        "tuple_pattern"
        | "tuple_struct_pattern"
        | "struct_pattern"
        | "pattern"
        | "list_pattern"
        | "attribute"
        | "rest_pattern"
        // JS/TS destructuring: `const { a, b } = src` and `const [a, b] = src`. Recursing keeps
        // nested patterns binding and reaches a `rest_pattern` inside them.
        | "object_pattern"
        | "array_pattern" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_pattern_defs(child, source, defined, depth + 1);
            }
        }
        // `{ key: binding }` — only the value binds. Recurring into every child would treat the
        // property name as a variable.
        "pair_pattern" => {
            if let Some(value) = node.child_by_field_name("value") {
                collect_pattern_defs(value, source, defined, depth + 1);
            }
        }
        // `{ a = default }` and `{ a: x = default }` — `left` binds; `right` is read, not defined.
        // Descending into `right` would invert the dependence, recording a *use* as a definition.
        "object_assignment_pattern" | "assignment_pattern" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_pattern_defs(left, source, defined, depth + 1);
            }
        }
        _ => {
            let mut inner_defined = HashSet::new();
            let mut inner_used = HashSet::new();
            let mut stack = vec![(node, true, depth + 1)];
            while let Some((n, is_def, d)) = stack.pop() {
                collect_def_use_node(
                    n,
                    source,
                    &mut inner_defined,
                    &mut inner_used,
                    is_def,
                    d,
                    &mut stack,
                );
            }
            defined.extend(inner_defined);
        }
    }
}

/// Collect all identifier uses under a subtree.
pub fn extract_used_variables(node: Node, source: &[u8]) -> HashSet<String> {
    let (_, used) = extract_def_use(node, source);
    used.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::DefVar;
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

    fn parse_go(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn parse_typescript(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
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
    fn deep_call_chain_def_use_does_not_overflow() {
        let depth = 3000;
        let chain = (0..depth).fold(String::from("s()"), |acc, _| format!("{acc}()"));
        let source = format!("int deep() {{ return {chain}; }}");
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(&source, None).unwrap();
        let _ = extract_def_use(tree.root_node(), source.as_bytes());
    }

    fn defs_has(defs: &[DefVar], name: &str) -> bool {
        defs.iter().any(|d| d.defines_name(name))
    }

    #[test]
    fn test_rust_let_def_use() {
        let source = "fn f(a: i32) { let x = a + 1; x }";
        let tree = parse_rust(source);
        let let_node = find_kind(tree.root_node(), "let_declaration").unwrap();
        let (defs, uses) = extract_def_use(let_node, source.as_bytes());
        assert!(defs_has(&defs, "x"));
        assert!(uses.iter().any(|u| u == "a"));
    }

    /// Destructuring must bind names. Regression: `object_pattern`, `array_pattern`,
    /// `pair_pattern` and `shorthand_property_identifier_pattern` were absent from
    /// `collect_pattern_defs`, so a destructured name was never recorded as defined. The
    /// consequence was silent under-approximation — no data dependence linked the destructuring
    /// line to the lines reading those names, so slices dropped it without saying so.
    #[test]
    fn test_typescript_object_destructuring_binds_shorthand_names() {
        let source = "const { a, b } = src;";
        let tree = parse_typescript(source);
        let decl = find_kind(tree.root_node(), "lexical_declaration").unwrap();
        let (defs, uses) = extract_def_use(decl, source.as_bytes());
        assert!(defs_has(&defs, "a"), "shorthand `a` should be defined");
        assert!(defs_has(&defs, "b"), "shorthand `b` should be defined");
        assert!(uses.iter().any(|u| u == "src"), "`src` is read");
    }

    #[test]
    fn test_typescript_array_destructuring_binds_names() {
        let source = "const [a, b] = src;";
        let tree = parse_typescript(source);
        let decl = find_kind(tree.root_node(), "lexical_declaration").unwrap();
        let (defs, uses) = extract_def_use(decl, source.as_bytes());
        assert!(defs_has(&defs, "a"), "array element `a` should be defined");
        assert!(defs_has(&defs, "b"), "array element `b` should be defined");
        assert!(uses.iter().any(|u| u == "src"), "`src` is read");
    }

    /// `{ key: binding }` binds the value. The property name must not become a variable, or the
    /// slice would reference a name that does not exist in the source.
    #[test]
    fn test_typescript_renaming_destructuring_binds_value_not_key() {
        let source = "const { data: members } = src;";
        let tree = parse_typescript(source);
        let decl = find_kind(tree.root_node(), "lexical_declaration").unwrap();
        let (defs, _uses) = extract_def_use(decl, source.as_bytes());
        assert!(
            defs_has(&defs, "members"),
            "the bound value should be defined"
        );
        assert!(
            !defs_has(&defs, "data"),
            "the property key must not be bound as a variable"
        );
    }

    #[test]
    fn test_typescript_nested_destructuring_binds_inner_names() {
        let source = "const { outer: { inner } } = src;";
        let tree = parse_typescript(source);
        let decl = find_kind(tree.root_node(), "lexical_declaration").unwrap();
        let (defs, _) = extract_def_use(decl, source.as_bytes());
        assert!(defs_has(&defs, "inner"), "nested binding should be defined");
        assert!(!defs_has(&defs, "outer"), "`outer` is a key, not a binding");
    }

    #[test]
    fn test_typescript_destructuring_rest_binds_rest() {
        let source = "const { a, ...rest } = src;";
        let tree = parse_typescript(source);
        let decl = find_kind(tree.root_node(), "lexical_declaration").unwrap();
        let (defs, _) = extract_def_use(decl, source.as_bytes());
        assert!(defs_has(&defs, "a"));
        assert!(defs_has(&defs, "rest"), "rest element should be defined");
    }

    /// A default expression is read, not defined. Descending into it would invert the dependence.
    #[test]
    fn test_typescript_destructuring_default_binds_left_not_default() {
        let source = "const { a = fallback } = src;";
        let tree = parse_typescript(source);
        let decl = find_kind(tree.root_node(), "lexical_declaration").unwrap();
        let (defs, _) = extract_def_use(decl, source.as_bytes());
        assert!(defs_has(&defs, "a"), "the binding should be defined");
        assert!(
            !defs_has(&defs, "fallback"),
            "the default expression must not be defined"
        );
    }

    #[test]
    fn test_python_assignment_def_use() {
        let source = "def f(a):\n    x = a + 1\n    return x\n";
        let tree = parse_python(source);
        let assign = find_kind(tree.root_node(), "assignment").unwrap();
        let (defs, uses) = extract_def_use(assign, source.as_bytes());
        assert!(defs_has(&defs, "x"));
        assert!(uses.iter().any(|u| u == "a"));
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

    #[test]
    fn test_go_short_var_and_range_def_use() {
        let source = "package demo\nfunc f(m map[string]int) {\n    x := 1\n    for k, v := range m {\n        use(k, v, x)\n    }\n}\n";
        let tree = parse_go(source);
        let assign = find_kind(tree.root_node(), "short_var_declaration").unwrap();
        let (defs, _uses) = extract_def_use(assign, source.as_bytes());
        assert!(defs_has(&defs, "x"));

        let for_node = find_kind(tree.root_node(), "for_statement").unwrap();
        let (for_defs, for_uses) = extract_def_use(for_node, source.as_bytes());
        assert!(
            defs_has(&for_defs, "k") || defs_has(&for_defs, "v"),
            "for defs: {for_defs:?}"
        );
        assert!(for_uses.iter().any(|u| u == "m"));
    }

    #[test]
    fn test_go_field_assignment_def_use() {
        let source = "package demo\nfunc Process(order *OrderDTO) {\n  order.Status = \"PROCESSED\"\n}\n";
        let tree = parse_go(source);
        let assign = find_kind(tree.root_node(), "assignment_statement").expect("assignment");
        let (defs, uses) = extract_def_use(assign, source.as_bytes());
        assert!(
            defs_has(&defs, "order.Status"),
            "defs should include order.Status, got {defs:?}"
        );
        let _ = uses;
    }

    #[test]
    fn test_kotlin_navigation_assignment_def_use() {
        let source = r#"
class OrderProcessor {
  fun process(order: OrderDTO): OrderDTO {
    order.status = "PROCESSED"
    return order
  }
}
"#;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let assign = find_kind(tree.root_node(), "assignment").expect("assignment");
        let (defs, _uses) = extract_def_use(assign, source.as_bytes());
        assert!(
            defs_has(&defs, "order.status"),
            "kotlin defs should include order.status, got {defs:?}"
        );
    }

    #[test]
    fn test_java_field_assignment_def_use() {
        let source = "class C { void m(OrderDTO order) { order.status = \"X\"; } }";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let assign = find_kind(tree.root_node(), "assignment_expression").expect("assignment");
        let (defs, uses) = extract_def_use(assign, source.as_bytes());
        assert!(
            defs_has(&defs, "order.status"),
            "defs should include order.status, got {defs:?}"
        );
        assert!(
            uses.iter().any(|u| u == "order"),
            "uses should include order, got {uses:?}"
        );
    }

    #[test]
    fn test_java_local_variable_declaration_def_use() {
        let source = "class C { void m(OrderDTO order) { OrderDTO other = order; } }";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let decl = find_kind(tree.root_node(), "local_variable_declaration").expect("local decl");
        let (defs, uses) = extract_def_use(decl, source.as_bytes());
        assert!(
            defs_has(&defs, "other"),
            "defs should include other, got {defs:?}"
        );
        assert!(
            uses.iter().any(|u| u == "order"),
            "uses should include order, got {uses:?}"
        );
        assert!(
            !uses.iter().any(|u| u == "other"),
            "declarator name must not be a use, got {uses:?}"
        );
    }
}
