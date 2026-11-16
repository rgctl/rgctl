//! Definition and use extraction from tree-sitter AST nodes.

use crate::cfg::DefVar;
use smallvec::SmallVec;
use std::collections::HashSet;
use tree_sitter::Node;

/// Maximum AST walk depth (matches C++ extraction and CFG expression walks).
const AST_WALK_MAX_DEPTH: usize = 2048;

/// AST node kinds inspected by [`extract_def_use`] for variable definitions, uses, and patterns.
const DEF_USE_HANDLED_KINDS: &[&str] = &[
    "Attribute",
    "accessibility_modifier",
    "alignof_expression",
    "alternative_pattern",
    "annotated_expression",
    "anonymous_function_use_clause",
    "arithmetic_expansion",
    "array_creation_expression",
    "array_declarator",
    "array_pattern",
    "array_size_definition",
    "as_expression",
    "as_pattern",
    "assignable_expression",
    "assignment",
    "assignment_expression",
    "assignment_pattern",
    "assignment_statement",
    "attribute",
    "attribute_declaration",
    "augmented_assignment",
    "augmented_assignment_expression",
    "bitfield_clause",
    "brace_expression",
    "case_pattern",
    "cast_expression",
    "cast_pattern",
    "catch_formal_parameter",
    "class_parameters",
    "class_pattern",
    "clone_expression",
    "column_definition",
    "comment_statement",
    "cte",
    "complex_pattern",
    "compound_assignment_expr",
    "constant_pattern",
    "constructor_param",
    "declaration",
    "declaration_command",
    "default_parameter",
    "destructured_left_assignment",
    "dict_pattern",
    "dictionary_splat_pattern",
    "element_reference",
    "enhanced_for_statement",
    "error_suppression_expression",
    "expansion",
    "expression_list",
    "expression_reference_pattern",
    "extension_expression",
    "field_access",
    "field_expression",
    "field_identifier",
    "filter_expression",
    "find_pattern",
    "frame_definition",
    "for_clause",
    "for_statement",
    "formal_parameter",
    "formal_parameter_list",
    "formal_parameters",
    "function_declarator",
    "function_declaration",
    "function_value_parameters",
    "generic_expression",
    "gnu_asm_expression",
    "hash_pattern",
    "identifier",
    "inferred_parameters",
    "init_declarator",
    "initialized_identifier",
    "initialized_identifier_list",
    "initialized_variable_definition",
    "instance_variable",
    "instanceof_expression",
    "keyword_pattern",
    "lambda_parameters",
    "left_assignment_list",
    "let_declaration",
    "let_statement",
    "lexical_declaration",
    "list_pattern",
    "list_splat_pattern",
    "local_declaration_statement",
    "local_variable_declaration",
    "map_pattern",
    "mapped_type_clause",
    "match_pattern",
    "member_access_expression",
    "member_expression",
    "ms_call_modifier",
    "multi_variable_declaration",
    "navigation_expression",
    "non_null_expression",
    "not_operator",
    "null_assert_pattern",
    "null_check_pattern",
    "object_assignment_pattern",
    "object_pattern",
    "offsetof_expression",
    "optional_formal_parameters",
    "optional_parameter",
    "pair_pattern",
    "parameter",
    "parameter_declaration",
    "parameter_list",
    "parameters",
    "parenthesized_declarator",
    "pattern",
    "pattern_assignment",
    "pattern_variable_declaration",
    "pointer_declarator",
    "pointer_expression",
    "postfix_expression",
    "preproc_call",
    "preproc_directive",
    "property_delegate",
    "property_promotion_parameter",
    "range",
    "range_clause",
    "record_pattern",
    "record_pattern_component",
    "reference_assignment_expression",
    "required_parameter",
    "resource",
    "rest_assignment",
    "rest_pattern",
    "right_assignment_list",
    "satisfies_expression",
    "scoped_identifier",
    "select_expression",
    "selector_expression",
    "sequence_expression",
    "short_var_declaration",
    "shorthand_field_identifier",
    "shorthand_property_identifier_pattern",
    "simple_expansion",
    "simple_parameter",
    "sizeof_expression",
    "splat_pattern",
    "spread_expression",
    "spread_parameter",
    "static_variable_declaration",
    "struct_pattern",
    "subscript_expression",
    "super_formal_parameter",
    "test_pattern",
    "top_level_variable_declaration",
    "tuple_pattern",
    "tuple_struct_pattern",
    "type_arguments",
    "type_bound",
    "type_constraint",
    "type_identifier",
    "type_parameter",
    "type_parameter_declaration",
    "type_parameter_list",
    "type_parameters",
    "type_pattern",
    "typed_default_parameter",
    "typed_parameter",
    "unary",
    "unary_expression",
    "unary_op_expression",
    "unary_operator",
    "union_pattern",
    "unset_command",
    "update_expression",
    "var_declaration",
    "var_spec",
    "var_spec_list",
    "variable_assignment",
    "variable_declaration",
    "variable_declaration_list",
    "variable_declarator",
    "variable_name",
    "variable_pattern",
    "variable_reference_pattern",
    "variadic_parameter",
    "variadic_parameter_declaration",
    "wildcard_pattern",
    "window_clause",
    "window_function",
];

/// Return the slice of Tree-sitter AST node kinds handled by [`extract_def_use`] for `language_id`.
///
/// Used by Tier-1 AST coverage parity gates (`check_plugin_manifest_sync` and
/// `tier1_parity_gate`) to verify that `"AstSkeleton"` entries in `{id}-ast-coverage.json`
/// correspond to real def-use, CFG, or plugin handlers.
pub fn def_use_handled_kinds(language_id: &str) -> &'static [&'static str] {
    if crate::language_profile::profile_for_language(language_id).is_some_and(|p| p.cfg_enabled) {
        DEF_USE_HANDLED_KINDS
    } else {
        &[]
    }
}

/// Extract variables defined and used in a statement node.
pub fn extract_def_use(
    node: Node,
    source: &[u8],
) -> (SmallVec<[DefVar; 2]>, SmallVec<[String; 3]>) {
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
        if let Some((last, prefix)) = named.split_last()
            && matches!(last.kind(), "identifier" | "simple_identifier")
        {
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
    if left.kind() == "directly_assignable_expression"
        && let Some(inner) = left.named_child(0)
    {
        collect_assignment_lhs(inner, source, defined, used, depth + 1, stack);
        return;
    }
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

        // Python / Ruby / Kotlin / Puppet
        "assignment" => {
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            if left.is_some() || right.is_some() {
                if let Some(left) = left {
                    collect_assignment_lhs(left, source, defined, used, depth, stack);
                }
                if let Some(right) = right {
                    stack.push((right, false, depth + 1));
                }
            } else {
                // Grammars without left/right fields (e.g. tree-sitter-kotlin-ng, tree-sitter-puppet):
                // first named child is LHS target, remaining named children are RHS expressions.
                let mut cursor = node.walk();
                let mut saw_lhs = false;
                for child in node.children(&mut cursor) {
                    if !child.is_named() {
                        continue;
                    }
                    if !saw_lhs {
                        saw_lhs = true;
                        collect_assignment_lhs(child, source, defined, used, depth, stack);
                    } else {
                        stack.push((child, false, depth + 1));
                    }
                }
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
        // Bash: `NAME=value`, `declare/local/export …`, `unset NAME`, `read -r NAME`
        "variable_assignment" => {
            let name_node = node.child_by_field_name("name").or_else(|| {
                let mut c = node.walk();
                node.children(&mut c)
                    .find(|ch| ch.kind() == "variable_name")
            });
            if let Some(name) = name_node {
                collect_pattern_defs(name, source, defined, depth + 1);
            }
            if let Some(value) = node.child_by_field_name("value") {
                stack.push((value, false, depth + 1));
            } else {
                let name_id = name_node.map(|n| n.id());
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.is_named() && Some(ch.id()) != name_id {
                        stack.push((ch, false, depth + 1));
                    }
                }
            }
        }
        "declaration_command" | "unset_command" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "variable_assignment" => {
                        stack.push((child, false, depth + 1));
                    }
                    "variable_name" => {
                        collect_pattern_defs(child, source, defined, depth + 1);
                    }
                    "word" => {
                        if let Ok(t) = child.utf8_text(source) {
                            let t = t.trim();
                            if !t.is_empty()
                                && !t.starts_with('-')
                                && !matches!(
                                    t,
                                    "local"
                                        | "declare"
                                        | "export"
                                        | "readonly"
                                        | "typeset"
                                        | "unset"
                                )
                            {
                                defined.insert(DefVar::local(t));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        "command" => {
            let cmd_name = node
                .child_by_field_name("name")
                .or_else(|| {
                    let mut c = node.walk();
                    node.children(&mut c).find(|ch| ch.kind() == "command_name")
                })
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::trim);
            if cmd_name == Some("read") {
                let mut skip_next = false;
                let mut define_next = false;
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if !ch.is_named() || ch.kind() == "command_name" {
                        continue;
                    }
                    if let Ok(raw) = ch.utf8_text(source) {
                        let tok = raw.trim();
                        if skip_next {
                            skip_next = false;
                            continue;
                        }
                        if define_next {
                            define_next = false;
                            if !tok.is_empty() && !tok.starts_with('-') {
                                defined.insert(DefVar::local(tok.trim_start_matches('$')));
                            }
                            continue;
                        }
                        if matches!(tok, "-p" | "-d" | "-n" | "-N" | "-t" | "-u") {
                            skip_next = true;
                            continue;
                        }
                        if tok == "-a" {
                            define_next = true;
                            continue;
                        }
                        if tok.starts_with('-') {
                            continue;
                        }
                        if matches!(ch.kind(), "word" | "variable_name") && !tok.is_empty() {
                            defined.insert(DefVar::local(tok.trim_start_matches('$')));
                            continue;
                        }
                    }
                    stack.push((ch, false, depth + 1));
                }
            } else {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.kind() != "command_name" {
                        stack.push((ch, false, depth + 1));
                    }
                }
            }
        }
        "simple_expansion" => {
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if matches!(ch.kind(), "variable_name" | "special_variable_name")
                    && let Ok(name) = ch.utf8_text(source)
                {
                    let trimmed = name.trim().trim_start_matches('$');
                    if !trimmed.is_empty() {
                        used.insert(trimmed.to_string());
                    }
                }
            }
        }
        "arithmetic_expansion" => {
            let mut arith_stack = vec![node];
            while let Some(cur) = arith_stack.pop() {
                match cur.kind() {
                    "variable_name" | "identifier" => {
                        if let Ok(name) = cur.utf8_text(source) {
                            let trimmed = name.trim().trim_start_matches('$');
                            if !trimmed.is_empty() {
                                used.insert(trimmed.to_string());
                            }
                        }
                    }
                    "word" => {
                        if let Ok(name) = cur.utf8_text(source) {
                            let trimmed = name.trim().trim_start_matches('$');
                            if trimmed
                                .chars()
                                .next()
                                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
                                && trimmed
                                    .chars()
                                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                            {
                                used.insert(trimmed.to_string());
                            }
                        }
                    }
                    "simple_expansion" | "expansion" | "command_substitution" => {
                        stack.push((cur, false, depth + 1));
                    }
                    _ => {
                        let mut c = cur.walk();
                        for ch in cur.children(&mut c) {
                            if ch.is_named() {
                                arith_stack.push(ch);
                            }
                        }
                    }
                }
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

        // Go / Lua
        "short_var_declaration" | "assignment_statement" => {
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            if left.is_some() || right.is_some() {
                if let Some(left) = left {
                    collect_assignment_lhs(left, source, defined, used, depth, stack);
                }
                if let Some(right) = right {
                    stack.push((right, false, depth + 1));
                }
            } else {
                // Lua `assignment_statement`: `variable_list` + `expression_list` children.
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if !child.is_named() {
                        continue;
                    }
                    if child.kind() == "variable_list" {
                        let mut vc = child.walk();
                        for v in child.children(&mut vc) {
                            if v.is_named() {
                                collect_assignment_lhs(v, source, defined, used, depth + 1, stack);
                            }
                        }
                    } else {
                        stack.push((child, false, depth + 1));
                    }
                }
            }
        }
        // `var` / `var ( ... )` — children are var_spec / var_spec_list (no left/right fields).
        "var_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "var_spec" => {
                        collect_go_var_spec(child, source, defined, used, depth + 1, stack)
                    }
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
                    if is_def_target {
                        collect_pattern_defs(child, source, defined, depth + 1);
                    } else {
                        stack.push((child, false, depth + 1));
                    }
                }
            }
        }
        "parameter_declaration" => {
            if let Some(name) = node.child_by_field_name("name") {
                collect_pattern_defs(name, source, defined, depth + 1);
            }
        }

        // Kotlin local/member `val x = expr` / `var (a, b) = expr`
        "property_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                match child.kind() {
                    "variable_declaration" | "multi_variable_declaration" => {
                        collect_pattern_defs(child, source, defined, depth + 1);
                    }
                    "modifiers" | "type_parameters" | "type_constraints" | "user_type"
                    | "nullable_type" | "non_nullable_type" | "function_type" | "getter"
                    | "setter" => {}
                    _ => {
                        stack.push((child, false, depth + 1));
                    }
                }
            }
        }

        // C# / Java / JS / TS / Dart / Lua local declarations
        "variable_declaration"
        | "local_declaration_statement"
        | "local_variable_declaration"
        | "top_level_variable_declaration"
        | "lexical_declaration"
        | "variable_declaration_list"
        | "resource" => {
            let mut saw_declarator = false;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    saw_declarator = true;
                    let name = child.child_by_field_name("name");
                    if let Some(name) = name {
                        collect_pattern_defs(name, source, defined, depth + 1);
                    }
                    push_declarator_initializer(child, name, stack, depth);
                } else if child.kind() == "variable_list" {
                    saw_declarator = true;
                    let mut vc = child.walk();
                    for v in child.children(&mut vc) {
                        if v.is_named() {
                            collect_pattern_defs(v, source, defined, depth + 1);
                        }
                    }
                } else if matches!(
                    child.kind(),
                    "initialized_variable_definition"
                        | "assignment_statement"
                        | "variable_declaration"
                        | "variable_declaration_list"
                ) {
                    saw_declarator = true;
                    stack.push((child, false, depth + 1));
                }
            }
            // Kotlin `variable_declaration` (`for (x in xs)` / `val x: T`) has direct identifier children.
            if !saw_declarator && kind == "variable_declaration" && is_def_target {
                collect_pattern_defs(node, source, defined, depth + 1);
            }
            // try-with-resources: `resource` may expose name/value fields directly.
            if kind == "resource" {
                if let Some(name) = node.child_by_field_name("name") {
                    collect_pattern_defs(name, source, defined, depth + 1);
                }
                if let Some(value) = node.child_by_field_name("value") {
                    stack.push((value, false, depth + 1));
                }
            }
        }
        "variable_declarator" | "initialized_variable_definition" => {
            let name = node.child_by_field_name("name").or_else(|| {
                let mut c = node.walk();
                node.children(&mut c)
                    .find(|ch| ch.kind() == "identifier" || ch.kind() == "initialized_identifier")
            });
            if let Some(name) = name {
                collect_pattern_defs(name, source, defined, depth + 1);
            }
            push_declarator_initializer(node, name, stack, depth);
        }
        // Java pattern bindings (`instanceof` / `switch` `type_pattern`).
        "instanceof_expression" => {
            if let Some(name) = node.child_by_field_name("name") {
                collect_pattern_defs(name, source, defined, depth + 1);
            }
            if let Some(pattern) = node.child_by_field_name("pattern") {
                stack.push((pattern, true, depth + 1));
            }
            if let Some(left) = node.child_by_field_name("left") {
                stack.push((left, false, depth + 1));
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "type_pattern" | "record_pattern") {
                    stack.push((child, true, depth + 1));
                }
            }
        }
        "type_pattern" => {
            if let Some(name) = node.child_by_field_name("name") {
                collect_pattern_defs(name, source, defined, depth + 1);
            }
        }
        "record_pattern" | "record_pattern_component" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(
                    child.kind(),
                    "type_pattern" | "record_pattern" | "record_pattern_component"
                ) {
                    stack.push((child, true, depth + 1));
                }
            }
        }
        "catch_formal_parameter" => {
            if let Some(name) = node.child_by_field_name("name") {
                collect_pattern_defs(name, source, defined, depth + 1);
            }
        }
        "enhanced_for_statement" => {
            if let Some(name) = node.child_by_field_name("name") {
                collect_pattern_defs(name, source, defined, depth + 1);
            }
            if let Some(value) = node.child_by_field_name("value") {
                stack.push((value, false, depth + 1));
            }
            if let Some(body) = node.child_by_field_name("body") {
                stack.push((body, false, depth + 1));
            }
        }
        // `i++` / `--i` — both define and use the operand.
        "update_expression" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    collect_assignment_lhs(child, source, defined, used, depth, stack);
                    stack.push((child, false, depth + 1));
                }
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
        "identifier"
        | "simple_identifier"
        | "shorthand_field_identifier"
        | "field_identifier"
        | "type_identifier"
        | "variable_name"
        | "variable" => {
            if is_def_target {
                if let Ok(name) = node.utf8_text(source)
                    && (kind == "identifier"
                        || kind == "simple_identifier"
                        || kind == "shorthand_field_identifier"
                        || kind == "variable_name"
                        || kind == "variable")
                {
                    defined.insert(DefVar::local(name.trim_start_matches('$')));
                }
            } else if (kind == "identifier"
                || kind == "simple_identifier"
                || kind == "shorthand_field_identifier"
                || kind == "variable_name"
                || kind == "variable")
                && let Ok(name) = node.utf8_text(source)
            {
                used.insert(name.trim_start_matches('$').to_string());
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

fn push_declarator_initializer<'a>(
    decl: Node<'a>,
    name_node: Option<Node<'a>>,
    stack: &mut DefUseStack<'a>,
    depth: usize,
) {
    if let Some(value) = decl
        .child_by_field_name("value")
        .or_else(|| decl.child_by_field_name("initializer"))
    {
        stack.push((value, false, depth + 1));
        return;
    }
    let name_id = name_node.map(|n| n.id());
    let mut cursor = decl.walk();
    for child in decl.children(&mut cursor) {
        if child.is_named() && Some(child.id()) != name_id && child.kind() != "type_annotation" {
            stack.push((child, false, depth + 1));
        }
    }
}

fn is_binding_pattern(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "simple_identifier"
            | "variable"
            | "variable_name"
            | "variable_declaration"
            | "multi_variable_declaration"
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
        "identifier"
        | "simple_identifier"
        | "variable"
        | "variable_name"
        | "shorthand_field_identifier"
        | "shorthand_property_identifier_pattern" => {
            if let Ok(name) = node.utf8_text(source) {
                defined.insert(DefVar::local(name.trim_start_matches('$')));
            }
        }
        "variable_declaration" | "multi_variable_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named()
                    && !matches!(
                        child.kind(),
                        "user_type"
                            | "nullable_type"
                            | "non_nullable_type"
                            | "function_type"
                            | "type_annotation"
                    )
                {
                    collect_pattern_defs(child, source, defined, depth + 1);
                }
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
        let source =
            "package demo\nfunc Process(order *OrderDTO) {\n  order.Status = \"PROCESSED\"\n}\n";
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

    #[test]
    fn test_java_resource_type_pattern_and_update_def_use() {
        let source = r#"
class C {
  void m(Object o) {
    try (java.io.StringReader r = new java.io.StringReader("")) {
      if (o instanceof String s) { System.out.println(s); }
      int i = 0;
      i++;
    } catch (Exception ex) {}
  }
}
"#;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let resource = find_kind(tree.root_node(), "resource").expect("resource");
        let (defs, _) = extract_def_use(resource, source.as_bytes());
        assert!(
            defs_has(&defs, "r"),
            "resource should define r, got {defs:?}"
        );

        let inst =
            find_kind(tree.root_node(), "instanceof_expression").expect("instanceof_expression");
        let (defs, _) = extract_def_use(inst, source.as_bytes());
        assert!(
            defs_has(&defs, "s"),
            "instanceof should define pattern binding s, got {defs:?}"
        );

        let upd = find_kind(tree.root_node(), "update_expression").expect("update");
        let (defs, uses) = extract_def_use(upd, source.as_bytes());
        assert!(defs_has(&defs, "i"), "i++ should define i, got {defs:?}");
        assert!(
            uses.iter().any(|u| u == "i"),
            "i++ should use i, got {uses:?}"
        );
    }

    #[test]
    fn test_csharp_local_declaration_extracts_initializer_uses() {
        let source = r#"
class C {
  int M(int aliasVal) {
    var finalOut = aliasVal * 2;
    return finalOut;
  }
}
"#;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let decl = find_kind(tree.root_node(), "local_declaration_statement")
            .expect("local_declaration_statement");
        let (defs, uses) = extract_def_use(decl, source.as_bytes());
        assert!(
            defs_has(&defs, "finalOut"),
            "var declaration should define finalOut, got {defs:?}"
        );
        assert!(
            uses.iter().any(|u| u == "aliasVal"),
            "var declaration initializer should use aliasVal, got {uses:?}"
        );
        assert!(
            !uses.iter().any(|u| u == "finalOut"),
            "declarator name must not be a use, got {uses:?}"
        );
    }

    #[test]
    fn test_kotlin_property_declaration_def_use() {
        let source = r#"
class C {
  fun m(seed: Int): Int {
    val aliasVal = seed + 1
    return aliasVal
  }
}
"#;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let prop = find_kind(tree.root_node(), "property_declaration").expect("property");
        let (defs, uses) = extract_def_use(prop, source.as_bytes());
        assert!(
            defs_has(&defs, "aliasVal"),
            "val declaration should define aliasVal, got {defs:?}"
        );
        assert!(
            uses.iter().any(|u| u == "seed"),
            "val initializer should use seed, got {uses:?}"
        );
    }

    #[test]
    fn test_puppet_variable_assignment_def_use() {
        let source = "$alias_val = $seed + 1\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_puppet::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let assign = find_kind(tree.root_node(), "assignment").expect("assignment");
        let (defs, uses) = extract_def_use(assign, source.as_bytes());
        assert!(
            defs_has(&defs, "alias_val"),
            "puppet assignment should define alias_val, got {defs:?}"
        );
        assert!(
            uses.iter().any(|u| u == "seed"),
            "puppet assignment RHS should use seed, got {uses:?}"
        );
    }

    #[test]
    fn test_dart_local_variable_declaration_def_use() {
        let source = "int f(int seedInput) { final stepVal = seedInput + 10; return stepVal; }\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_dart::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let decl =
            find_kind(tree.root_node(), "local_variable_declaration").expect("dart local_var");
        let (defs, uses) = extract_def_use(decl, source.as_bytes());
        assert!(
            defs_has(&defs, "stepVal"),
            "dart final decl should define stepVal, got {defs:?}"
        );
        assert!(
            uses.iter().any(|u| u == "seedInput"),
            "dart final decl should use seedInput, got {uses:?}"
        );
    }

    #[test]
    fn test_lua_variable_declaration_def_use() {
        let source =
            "function f(seed_input)\n  local step_val = seed_input + 10\n  return step_val\nend\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_lua::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let decl = find_kind(tree.root_node(), "variable_declaration").expect("lua var_decl");
        let (defs, uses) = extract_def_use(decl, source.as_bytes());
        assert!(
            defs_has(&defs, "step_val"),
            "lua local decl should define step_val, got {defs:?}"
        );
        assert!(
            !defs_has(&defs, "seed_input"),
            "lua RHS seed_input must not be marked defined, got {defs:?}"
        );
        assert!(
            uses.iter().any(|u| u == "seed_input"),
            "lua local decl should use seed_input, got {uses:?}"
        );
    }

    #[test]
    fn test_bash_declaration_and_read_def_use() {
        let source = "f() {\n  local step_val=$((seed_input + 10))\n  read -r raw_cmd\n  eval \"$raw_cmd\"\n}\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bash::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let decl = find_kind(tree.root_node(), "declaration_command").expect("bash decl");
        let (defs, uses) = extract_def_use(decl, source.as_bytes());
        assert!(
            defs_has(&defs, "step_val") && !defs_has(&defs, "local"),
            "bash local decl should define only step_val, got {defs:?}"
        );
        assert!(
            uses.iter().any(|u| u == "seed_input"),
            "bash arithmetic expansion should use seed_input, got {uses:?}"
        );

        let read_cmd = find_kind(tree.root_node(), "command").expect("bash read command");
        let (rdefs, _) = extract_def_use(read_cmd, source.as_bytes());
        assert!(
            defs_has(&rdefs, "raw_cmd"),
            "bash read -r raw_cmd should define raw_cmd, got {rdefs:?}"
        );
    }
}
