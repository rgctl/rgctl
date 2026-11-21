//! Language-aware value-call facts, fed by existing bounded plugin traversals.
use crate::{Relation, RelationType, SourceLocation, Symbol, SymbolType, intern_language_cow};
use std::{borrow::Cow, collections::HashMap, path::Path};
use tree_sitter::Node;

type Point = (usize, usize);
fn point(node: Node<'_>) -> Point {
    (node.start_position().row + 1, node.start_position().column)
}
fn symbol_point(symbol: &Symbol) -> Point {
    (symbol.location.start_line, symbol.location.start_column)
}
fn text<'s>(node: Node<'_>, source: &'s [u8]) -> &'s str {
    node.utf8_text(source).unwrap_or("")
}

#[derive(Clone, Copy)]
struct Scope {
    start: usize,
    end: usize,
    start_point: Point,
    end_point: Point,
    captures: bool,
}
#[derive(Clone, Copy)]
struct Binding {
    start: usize,
    end: usize,
    declaration: (Point, Point),
    function: Option<usize>,
    function_declaration: bool,
}
struct Bindings {
    values: Vec<Binding>,
    maxima: Vec<usize>,
    leaves: usize,
}
impl Bindings {
    fn new(mut values: Vec<Binding>) -> Self {
        values.sort_unstable_by_key(|b| (b.start, b.declaration.0));
        let leaves = values.len().next_power_of_two();
        let mut maxima = vec![0; leaves * 2];
        for (i, b) in values.iter().enumerate() {
            maxima[leaves + i] = b.end;
        }
        for i in (1..leaves).rev() {
            maxima[i] = maxima[2 * i].max(maxima[2 * i + 1]);
        }
        Self {
            values,
            maxima,
            leaves,
        }
    }
    fn at(&self, position: usize) -> Option<&Binding> {
        let limit = self.values.partition_point(|b| b.start <= position);
        self.rightmost(1, 0, self.leaves, limit, position)
            .map(|i| &self.values[i])
    }
    // Segment-tree recursion is bounded by usize::BITS, not source depth.
    fn rightmost(
        &self,
        node: usize,
        lo: usize,
        hi: usize,
        limit: usize,
        position: usize,
    ) -> Option<usize> {
        if lo >= limit || self.maxima[node] <= position {
            return None;
        }
        if hi - lo == 1 {
            return Some(lo);
        }
        let middle = lo + (hi - lo) / 2;
        self.rightmost(node * 2 + 1, middle, hi, limit, position)
            .or_else(|| self.rightmost(node * 2, lo, middle, limit, position))
    }
}
struct Call<'s> {
    location: SourceLocation,
    name: &'s str,
    byte: usize,
    force: bool,
    function: Option<Scope>,
}

/// A per-file collector. `observe` consumes the caller's existing preorder
/// visits; `apply` indexes declarations once and never scans symbols per call.
pub struct LexicalCallTargets<'s> {
    language: Cow<'static, str>,
    scopes: Vec<Scope>,
    functions: Vec<Scope>,
    bindings: HashMap<&'s str, Vec<Binding>>,
    calls: Vec<Call<'s>>,
}
impl<'s> LexicalCallTargets<'s> {
    /// Select the native language's value-invocation and scope rules.
    pub fn new(language: &str) -> Self {
        Self {
            language: intern_language_cow(language),
            scopes: Vec::new(),
            functions: Vec::new(),
            bindings: HashMap::new(),
            calls: Vec::new(),
        }
    }
    /// Feed each node once, before visiting its children.
    pub fn observe(&mut self, node: Node<'_>, source: &'s [u8]) {
        let lang = self.language.as_ref();
        if matches!(lang, "go" | "java") {
            return;
        }
        while self
            .scopes
            .last()
            .is_some_and(|s| node.start_byte() < s.start || node.end_byte() > s.end)
        {
            self.scopes.pop();
        }
        while self
            .functions
            .last()
            .is_some_and(|scope| node.start_byte() < scope.start || node.end_byte() > scope.end)
        {
            self.functions.pop();
        }
        let kind = node.kind();
        let function = (lang == "elixir"
            && kind == "call"
            && node.child_by_field_name("target").is_some_and(|n| {
                matches!(text(n, source), "def" | "defp" | "defmacro" | "defmacrop")
            }))
            || matches!(
                kind,
                "function_definition"
                    | "function_declaration"
                    | "function_item"
                    | "method_declaration"
                    | "method_definition"
                    | "method"
                    | "singleton_method"
                    | "arrow_function"
                    | "function_expression"
                    | "generator_function"
                    | "lambda_expression"
                    | "lambda_literal"
                    | "closure_expression"
                    | "anonymous_function"
                    | "local_function_statement"
            );
        let block = matches!(
            kind,
            "program"
                | "source_file"
                | "translation_unit"
                | "compilation_unit"
                | "module"
                | "chunk"
                | "source"
                | "block"
                | "statement_block"
                | "compound_statement"
                | "function_body"
                | "template_body"
                | "class_body"
                | "for_statement"
                | "foreach_statement"
                | "for_in_statement"
                | "if_statement"
                | "catch_clause"
                | "switch_section"
        );
        let push_scope = function || (block && (lang != "python" || self.scopes.is_empty()));
        let captures = !(lang == "rust" && kind == "function_item");
        if function
            && !self.functions.is_empty()
            && !matches!(lang, "ruby" | "bash" | "php" | "puppet" | "elixir")
            && let Some(name) = node.child_by_field_name("name")
            && matches!(name.kind(), "identifier" | "simple_identifier")
        {
            // A nested named function is a declaration, not an unknown value.
            // Keep its lexical binding so a same-named outer declaration
            // cannot hijack it; apply() uses the native callable's identity.
            self.record(name, node, source, true, false);
        }
        if push_scope {
            self.scopes.push(Scope {
                start: node.start_byte(),
                end: node.end_byte(),
                start_point: point(node),
                end_point: (node.end_position().row + 1, node.end_position().column),
                captures,
            });
            if function {
                self.functions.push(Scope {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    start_point: point(node),
                    end_point: (node.end_position().row + 1, node.end_position().column),
                    captures,
                });
            }
        }
        self.parameters(node, source);
        self.declaration(node, source);
        if let Some((target, force)) = self.target(node, source) {
            let p = node.start_position();
            let end = node.end_position();
            self.calls.push(Call {
                location: SourceLocation {
                    file: String::new(),
                    start_line: p.row + 1,
                    end_line: end.row + 1,
                    start_column: p.column,
                    end_column: end.column,
                },
                name: text(target, source).trim_start_matches('$'),
                byte: target.start_byte(),
                force,
                function: self.functions.last().copied(),
            });
        }
    }
    fn record(
        &mut self,
        name: Node<'_>,
        declaration: Node<'_>,
        source: &'s [u8],
        parameter: bool,
        hoist: bool,
    ) {
        let Some(scope) = (if hoist {
            self.functions.last()
        } else {
            self.scopes.last()
        })
        .copied()
        .or_else(|| self.scopes.last().copied()) else {
            return;
        };
        let name = text(name, source).trim_start_matches('$');
        if name.is_empty() || name == "_" {
            return;
        }
        let end = declaration.end_position();
        let start = if parameter
            || hoist
            || matches!(self.language.as_ref(), "javascript" | "typescript")
        {
            scope.start
        } else {
            declaration.end_byte()
        };
        self.bindings.entry(name).or_default().push(Binding {
            start,
            end: scope.end,
            declaration: (point(declaration), (end.row + 1, end.column)),
            function: self.functions.last().map(|scope| scope.start),
            function_declaration: matches!(
                declaration.kind(),
                "function_declaration"
                    | "function_definition"
                    | "function_item"
                    | "local_function_statement"
            ),
        });
    }
    fn identifier(mut node: Node<'_>) -> Option<Node<'_>> {
        for _ in 0..64 {
            if matches!(
                node.kind(),
                "identifier" | "simple_identifier" | "variable_name" | "word" | "name"
            ) {
                return Some(node);
            }
            node = node
                .child_by_field_name("declarator")
                .or_else(|| node.child_by_field_name("pattern"))
                .or_else(|| node.named_child(0))?;
        }
        None
    }
    fn parameters(&mut self, node: Node<'_>, source: &'s [u8]) {
        let kind = node.kind();
        let Some(parent) = node.parent() else {
            return;
        };
        let list = matches!(
            parent.kind(),
            "parameters"
                | "formal_parameters"
                | "parameter_list"
                | "function_value_parameters"
                | "formal_parameter_list"
                | "method_parameters"
                | "lambda_parameters"
        );
        if !list && !(self.language == "swift" && kind == "parameter") {
            return;
        }
        // Names in callback *types* do not enter the enclosing body's scope.
        if parent.parent().is_some_and(|n| {
            matches!(
                n.kind(),
                "function_type" | "function_pointer_type" | "function_type_parameters"
            )
        }) {
            return;
        }
        if matches!(self.language.as_ref(), "c" | "cpp")
            && let Some(declarator) = parent.parent()
            && declarator.kind() == "function_declarator"
        {
            let mut owner = declarator.parent();
            for _ in 0..64 {
                let Some(node) = owner else { break };
                if matches!(
                    node.kind(),
                    "parameter_declaration" | "init_declarator" | "type_definition"
                ) {
                    return;
                }
                if node.kind() == "function_definition" {
                    break;
                }
                owner = node.parent();
            }
        }
        let name = match kind {
            "identifier" | "simple_identifier" | "variable_name" => Some(node),
            "parameter"
            | "parameter_declaration"
            | "formal_parameter"
            | "simple_parameter"
            | "optional_parameter"
            | "required_parameter"
            | "typed_parameter"
            | "default_parameter"
            | "typed_default_parameter"
            | "rest_pattern" => node
                .child_by_field_name("pattern")
                .and_then(Self::identifier)
                .or_else(|| {
                    node.child_by_field_name("declarator")
                        .and_then(Self::identifier)
                })
                .or_else(|| {
                    let mut cursor = node.walk();
                    node.children_by_field_name("name", &mut cursor).find(|n| {
                        matches!(
                            n.kind(),
                            "identifier" | "simple_identifier" | "variable_name" | "name"
                        )
                    })
                })
                .or_else(|| {
                    let mut c = node.walk();
                    node.named_children(&mut c).find(|n| {
                        matches!(n.kind(), "identifier" | "simple_identifier" | "variable")
                    })
                }),
            _ => None,
        };
        if let Some(name) = name {
            self.record(name, node, source, true, true);
        }
    }
    fn declaration(&mut self, node: Node<'_>, source: &'s [u8]) {
        if matches!(
            self.language.as_ref(),
            "java" | "puppet" | "elixir" | "bash" | "ruby"
        ) {
            return;
        }
        let kind = node.kind();
        let candidate = match kind {
            "variable_declarator" | "init_declarator" | "initialized_variable_definition" => node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("declarator")),
            "let_declaration"
            | "assignment"
            | "assignment_statement"
            | "assignment_expression"
            | "val_definition"
            | "var_definition"
            | "local_variable_declaration"
            | "local_declaration" => node
                .child_by_field_name("pattern")
                .or_else(|| node.child_by_field_name("left"))
                .or_else(|| node.child_by_field_name("name")),
            "variable_declaration"
                if matches!(self.language.as_ref(), "swift" | "dart" | "lua" | "kotlin") =>
            {
                node.child_by_field_name("name")
                    .or_else(|| node.child_by_field_name("pattern"))
            }
            "property_declaration" | "variable_declaration_statement" => {
                node.child_by_field_name("name").or_else(|| {
                    let mut c = node.walk();
                    node.named_children(&mut c)
                        .find(|n| matches!(n.kind(), "variable_declaration" | "pattern"))
                })
            }
            _ => None,
        };
        if let Some(name) = candidate.and_then(Self::identifier) {
            let hoist = self.language == "python"
                || (matches!(self.language.as_ref(), "javascript" | "typescript")
                    && node
                        .parent()
                        .is_some_and(|p| p.kind() == "variable_declaration"));
            let declaration = if matches!(
                kind,
                "variable_declarator" | "initialized_variable_definition"
            ) {
                node.parent()
                    .filter(|p| {
                        matches!(
                            p.kind(),
                            "lexical_declaration"
                                | "variable_declaration"
                                | "local_variable_declaration"
                        )
                    })
                    .unwrap_or(node)
            } else {
                node
            };
            self.record(name, declaration, source, false, hoist);
        }
    }
    fn target<'n>(&self, node: Node<'n>, source: &[u8]) -> Option<(Node<'n>, bool)> {
        let lang = self.language.as_ref();
        if lang == "bash" && node.kind() == "command" {
            let name = node.child_by_field_name("name")?;
            let raw = text(name, source);
            return raw.contains('$').then_some((name, true));
        }
        if lang == "puppet" && node.kind() == "function_call" {
            let mut c = node.walk();
            let mut children = node.named_children(&mut c);
            if text(children.next()?, source) == "call" {
                return children.find(|n| n.kind() == "variable").map(|n| (n, true));
            }
            return None;
        }
        if lang == "ruby" && node.kind() == "call" {
            if text(node.child_by_field_name("method")?, source) == "call" {
                return node
                    .child_by_field_name("receiver")
                    .and_then(Self::identifier)
                    .map(|n| (n, true));
            }
            return None;
        }
        if lang == "elixir" && node.kind() == "call" {
            let target = node.child_by_field_name("target")?;
            return (target.kind() == "dot" && target.child_by_field_name("right").is_none())
                .then(|| target.child_by_field_name("left").map(|n| (n, true)))
                .flatten();
        }
        if !matches!(
            node.kind(),
            "call"
                | "call_expression"
                | "invocation_expression"
                | "method_invocation"
                | "function_call"
                | "function_call_expression"
        ) {
            return None;
        }
        let mut target = node
            .child_by_field_name("function")
            .or_else(|| node.child_by_field_name("name"))
            .or_else(|| {
                let mut c = node.walk();
                node.named_children(&mut c)
                    .find(|n| matches!(n.kind(), "identifier" | "simple_identifier"))
            })?;
        if lang == "php" {
            return (target.kind() == "variable_name").then_some((target, true));
        }
        if node.child_by_field_name("object").is_some()
            || node.child_by_field_name("receiver").is_some()
        {
            return None;
        }
        for _ in 0..512 {
            match target.kind() {
                "identifier" | "simple_identifier" => return Some((target, false)),
                "parenthesized_expression" | "pointer_expression" | "generic_function" => {
                    target = target
                        .child_by_field_name("function")
                        .or_else(|| target.named_child(0))?
                }
                _ => return None,
            }
        }
        None
    }
    /// Attach proven value-call provenance after native finalization. Existing
    /// callable symbols inside a binding declaration retain their precise target.
    pub fn apply(self, path: &Path, symbols: &[Symbol], relations: &mut Vec<Relation>) {
        if self.calls.is_empty()
            || (self.bindings.is_empty() && !self.calls.iter().any(|call| call.force))
        {
            return;
        }
        let bindings: HashMap<_, _> = self
            .bindings
            .into_iter()
            .map(|(n, v)| (n, Bindings::new(v)))
            .collect();
        let mut functions: HashMap<&str, Vec<&Symbol>> = HashMap::new();
        let mut owners: Vec<&Symbol> = Vec::new();
        for symbol in symbols.iter().filter(|s| {
            matches!(s.symbol_type, SymbolType::Function | SymbolType::Method)
                && (self.language == "elixir" || s.callable_role_str() != Some("declaration"))
        }) {
            functions.entry(&symbol.name).or_default().push(symbol);
            owners.push(symbol);
        }
        for values in functions.values_mut() {
            values.sort_unstable_by_key(|s| symbol_point(s));
        }
        owners.sort_unstable_by_key(|s| symbol_point(s));
        let mut sites: HashMap<Point, Vec<usize>> = HashMap::new();
        for (i, r) in relations
            .iter()
            .enumerate()
            .filter(|(_, r)| r.relation_type == RelationType::Calls)
        {
            sites
                .entry((r.location.start_line, r.location.start_column))
                .or_default()
                .push(i);
        }
        for call in self.calls {
            let binding = bindings
                .get(call.name)
                .and_then(|b| b.at(call.byte))
                .filter(|binding| {
                    // Rust fn items do not capture enclosing value bindings;
                    // closures do. Function declarations remain visible items.
                    binding.function_declaration
                        || call.function.is_none_or(|scope| {
                            scope.captures || binding.function == Some(scope.start)
                        })
                });
            if !call.force && binding.is_none() {
                continue;
            }
            let known = binding.and_then(|binding| {
                let values = functions.get(call.name)?;
                let first = values.partition_point(|s| symbol_point(s) < binding.declaration.0);
                let last = values.partition_point(|s| symbol_point(s) <= binding.declaration.1);
                (last == first + 1).then(|| values[first])
            });
            let mut indices = sites
                .get(&(call.location.start_line, call.location.start_column))
                .cloned()
                .unwrap_or_default();
            if indices.is_empty() {
                // A native emitter may omit an anonymous invocation entirely.
                // Select its enclosing callable without an O(calls × symbols) scan.
                let count = owners.partition_point(|s| {
                    symbol_point(s) <= (call.location.start_line, call.location.start_column)
                });
                if let Some(owner) = count.checked_sub(1).map(|i| owners[i]).filter(|s| {
                    if self.language == "elixir" {
                        // Elixir's native symbol span may stop at the head of
                        // a one-line do: definition. The existing traversal
                        // supplies its complete definition scope instead.
                        call.function.is_some_and(|scope| {
                            symbol_point(s) >= scope.start_point
                                && scope.end_point
                                    >= (call.location.end_line, call.location.end_column)
                        })
                    } else {
                        (s.location.end_line, s.location.end_column)
                            >= (call.location.end_line, call.location.end_column)
                    }
                }) {
                    let mut location = call.location.clone();
                    location.file = path.to_string_lossy().into_owned();
                    indices.push(relations.len());
                    relations.push(Relation {
                        from: owner
                            .qualified_name
                            .clone()
                            .unwrap_or_else(|| owner.name.clone()),
                        to: call.name.into(),
                        relation_type: RelationType::Calls,
                        location,
                        language: Some(self.language.clone()),
                        exact_callable_resolution: true,
                        from_identity_discriminator: Some(
                            owner.identity_discriminator.as_deref().unwrap_or("").into(),
                        ),
                        call_site_start_byte: u32::try_from(call.byte).ok(),
                        ..Default::default()
                    });
                }
            }
            for index in indices {
                let r = &mut relations[index];
                if self.language == "ruby" && r.to_type_hint.is_some() && !r.inferred_callable_owner
                {
                    // Preserve a receiver type proven by the native plugin;
                    // project objects may define an ordinary `call` method.
                    continue;
                }
                if known.is_some() {
                    // Native callable finalization already carries the owner,
                    // overload and namespace information for this declaration.
                    continue;
                }
                r.to = call.name.into();
                r.to_type_hint = None;
                r.inferred_callable_owner = false;
                r.to_identity_discriminator = None;
                r.to_qualified_hint = None;
                r.to_qualified_hint_origin = crate::QualifiedNameOrigin::Native;
                r.exact_callable_resolution = true;
                r.indirect_call = true;
            }
        }
    }
}
