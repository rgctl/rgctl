//! Lexical Go bindings collected by the caller's existing preorder traversal.

use std::collections::HashMap;
use tree_sitter::Node;

/// Parentheses preserve an identifier's lexical binding. Bound the descent to
/// match the shared callee extractor rather than walking arbitrary expressions.
pub(crate) fn bare_call_target(call: Node<'_>) -> Option<Node<'_>> {
    let mut target = call.child_by_field_name("function")?;
    for _ in 0..512 {
        match target.kind() {
            "identifier" => return Some(target),
            "parenthesized_expression" => target = target.named_child(0)?,
            _ => return None,
        }
    }
    None
}

#[derive(Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

/// Per-file binding intervals. Building this index does not walk the tree;
/// extraction feeds each visited node, then finalizes once before flushing calls.
#[derive(Default)]
pub struct GoCallBindings<'s> {
    scopes: Vec<Span>,
    names: HashMap<&'s str, Vec<Span>>,
}

impl<'s> GoCallBindings<'s> {
    /// Observe a node in source preorder, before descending into its children.
    pub fn observe(&mut self, node: Node<'_>, source: &'s [u8]) {
        if !node.is_named() {
            return;
        }
        while self
            .scopes
            .last()
            .is_some_and(|scope| node.start_byte() < scope.start || node.end_byte() > scope.end)
        {
            self.scopes.pop();
        }
        match node.kind() {
            "source_file"
            | "function_declaration"
            | "method_declaration"
            | "func_literal"
            | "block"
            | "if_statement"
            | "for_statement"
            | "expression_switch_statement"
            | "type_switch_statement"
            | "select_statement"
            | "expression_case"
            | "type_case"
            | "communication_case"
            | "default_case" => self.scopes.push(Span {
                start: node.start_byte(),
                end: node.end_byte(),
            }),
            _ => {}
        }
        let Some(scope) = self.scopes.last().copied() else {
            return;
        };
        match node.kind() {
            "parameter_declaration" | "variadic_parameter_declaration" => {
                // Parameter names inside a function *type* do not bind names
                // in the enclosing function (e.g. callback func(Helper int)).
                if let Some(owner) = node.parent().and_then(|list| list.parent())
                    && matches!(
                        owner.kind(),
                        "function_declaration" | "method_declaration" | "func_literal"
                    )
                    && let Some(body) = owner.child_by_field_name("body")
                {
                    self.named_fields(node, source, body.start_byte(), owner.end_byte());
                }
            }
            "var_spec" | "const_spec" | "type_spec" | "type_alias" => {
                // Package declarations cover the whole package. Local value
                // declarations start after their initializer, so Helper() on
                // the RHS of `Helper := Helper()` still denotes the outer name.
                let start = if self.scopes.len() == 1 {
                    scope.start
                } else if matches!(node.kind(), "type_spec" | "type_alias") {
                    node.start_byte()
                } else {
                    node.end_byte()
                };
                self.named_fields(node, source, start, scope.end);
            }
            "type_parameter_declaration" => {
                // A generic type's parameters end with its type declaration;
                // they must not shadow names in subsequent package functions.
                if let Some(owner) = node.parent().and_then(|list| list.parent()) {
                    self.named_fields(node, source, owner.start_byte(), owner.end_byte());
                }
            }
            "short_var_declaration" | "range_clause" | "receive_statement" => {
                let mut cursor = node.walk();
                if node.children(&mut cursor).any(|child| child.kind() == ":=")
                    && let Some(left) = node.child_by_field_name("left")
                {
                    self.identifiers(left, source, node.end_byte(), scope.end);
                }
            }
            "type_case" | "default_case" => {
                if let Some(parent) = node.parent()
                    && parent.kind() == "type_switch_statement"
                    && let Some(alias) = parent.child_by_field_name("alias")
                {
                    self.identifiers(alias, source, node.start_byte(), node.end_byte());
                }
            }
            _ => {}
        }
    }

    fn named_fields(&mut self, node: Node<'_>, source: &'s [u8], start: usize, end: usize) {
        let mut cursor = node.walk();
        for name in node.children_by_field_name("name", &mut cursor) {
            self.name(name, source, start, end);
        }
    }

    fn identifiers(&mut self, list: Node<'_>, source: &'s [u8], start: usize, end: usize) {
        let mut cursor = list.walk();
        for name in list.named_children(&mut cursor) {
            if name.kind() == "identifier" {
                self.name(name, source, start, end);
            }
        }
    }

    fn name(&mut self, node: Node<'_>, source: &'s [u8], start: usize, end: usize) {
        if let Ok(name) = node.utf8_text(source)
            && name != "_"
        {
            self.names
                .entry(name)
                .or_default()
                .push(Span { start, end });
        }
    }

    /// Sort once and store prefix maxima, giving O(log B) lookups even when a
    /// name is shadowed repeatedly in nested or disjoint blocks.
    pub fn finish(mut self) -> Self {
        self.scopes.clear();
        for spans in self.names.values_mut() {
            spans.sort_unstable_by_key(|span| span.start);
            let mut maximum = 0;
            for span in spans {
                maximum = maximum.max(span.end);
                span.end = maximum;
            }
        }
        self
    }

    /// Whether a bare call target is a lexical value/type rather than a
    /// package-level function. Selectors are handled by receiver resolution.
    pub fn shadows_call(&self, call: Node<'_>, source: &[u8]) -> bool {
        let Some(function) = bare_call_target(call) else {
            return false;
        };
        let Ok(name) = function.utf8_text(source) else {
            return false;
        };
        let Some(spans) = self.names.get(name) else {
            return false;
        };
        let position = function.start_byte();
        let count = spans.partition_point(|span| span.start <= position);
        count > 0 && position < spans[count - 1].end
    }
}
